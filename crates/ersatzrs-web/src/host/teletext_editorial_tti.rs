//! Native reconciliation of one trusted export baseline and an edited TTI file.
//!
//! This preserves protected declarations and exact authored identities. It grants
//! no receipt authentication, edit-region permission, persistence or publication
//! authority. The host must validate resulting effects against the full original.

use ersatzrs_app::{
    teletext_export_project::TeletextExportPageMapping,
    teletext_tti::{
        parse_teletext_tti, TeletextTtiBudget, TeletextTtiCycleUnit, TeletextTtiDocument,
        TeletextTtiError, TeletextTtiRecord,
    },
};
use ersatzrs_domain::{
    TeletextPageAddress, TeletextPageNumber, TeletextPhysicalBaseOverlay, TeletextPhysicalBaseRow,
    TeletextSubcode,
};
use std::time::Instant;

/// Static rejection without any authored text, file name or receipt digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeTeletextEditorialTtiFailure {
    TimedOut,
    Mapping,
    Identity,
    Syntax,
    HeaderIdentity,
    Status,
    Region,
    Navigation,
    Description,
    Cycle,
    Packet,
    ProtectedRow,
    Structure,
}
/// Exact display location. Physical grid columns are zero-based; a nested syntax
/// cause retains the TTI parser's one-based file/payload location independently.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("teletext editor {failure:?} at page {page}, subcode {subcode}, row {row:?}, column {column:?}, designation {designation:?}")]
pub struct NativeTeletextEditorialTtiError {
    pub failure: NativeTeletextEditorialTtiFailure,
    pub page: u16,
    pub subcode: u16,
    pub row: Option<u8>,
    pub column: Option<usize>,
    pub designation: Option<u8>,
    pub cause: Option<TeletextTtiError>,
}
type Result<T> = std::result::Result<T, NativeTeletextEditorialTtiError>;

#[derive(Clone, Copy)]
struct Location {
    page: u16,
    subcode: u16,
}
impl Location {
    fn error(self, failure: NativeTeletextEditorialTtiFailure) -> NativeTeletextEditorialTtiError {
        NativeTeletextEditorialTtiError {
            failure,
            page: self.page,
            subcode: self.subcode,
            row: None,
            column: None,
            designation: None,
            cause: None,
        }
    }
    fn check_deadline(self, deadline: Instant) -> Result<()> {
        if Instant::now() >= deadline {
            return Err(self.error(NativeTeletextEditorialTtiFailure::TimedOut));
        }
        Ok(())
    }
    fn parse(
        self,
        bytes: &[u8],
        budget: TeletextTtiBudget,
        deadline: Instant,
    ) -> Result<TeletextTtiDocument> {
        self.check_deadline(deadline)?;
        let document =
            parse_teletext_tti(bytes, budget).map_err(|cause| NativeTeletextEditorialTtiError {
                row: cause.row,
                cause: Some(cause),
                ..self.error(NativeTeletextEditorialTtiFailure::Syntax)
            })?;
        self.check_deadline(deadline)?;
        Ok(document)
    }
}

// Fixed indexes retain references to bounded parsed records; no content copy or
// unordered JSON tree is built to compare declarations by their logical key.
#[derive(Default)]
struct Page<'a> {
    description: Option<&'a str>,
    status: Option<u16>,
    region: Option<u64>,
    cycle: Option<(u64, TeletextTtiCycleUnit)>,
    links: Option<&'a [u16; 6]>,
    rows: [Option<&'a [u8; 40]>; 26],
    packets: [[Option<&'a [u32; 13]>; 16]; 4],
}
impl<'a> Page<'a> {
    fn index(
        document: &'a TeletextTtiDocument,
        mapping: &TeletextExportPageMapping,
        location: Location,
        deadline: Instant,
    ) -> Result<Self> {
        let mut page = Self::default();
        let mut page_numbers = 0_usize;
        let mut subcodes = 0_usize;
        for record in document.records() {
            location.check_deadline(deadline)?;
            match record {
                TeletextTtiRecord::PageNumber {
                    page: number,
                    editor_ordinal,
                } => {
                    page_numbers += 1;
                    if *number != mapping.editor_page || *editor_ordinal != 0 {
                        return Err(location.error(NativeTeletextEditorialTtiFailure::Identity));
                    }
                }
                TeletextTtiRecord::SubpageOrdinal(ordinal) => {
                    subcodes += 1;
                    if *ordinal != 0 {
                        return Err(location.error(NativeTeletextEditorialTtiFailure::Identity));
                    }
                }
                TeletextTtiRecord::Description(value) => page.description = Some(value),
                TeletextTtiRecord::PageStatus(value) => page.status = Some(*value),
                TeletextTtiRecord::Region(value) => page.region = Some(*value),
                TeletextTtiRecord::Cycle { value, unit } => page.cycle = Some((*value, *unit)),
                TeletextTtiRecord::FastLinks(value) => page.links = Some(value),
                TeletextTtiRecord::BaseRow { row, cells } => {
                    page.rows[usize::from(*row)] = Some(cells);
                }
                TeletextTtiRecord::Packet {
                    row,
                    designation,
                    words,
                } => page.packets[usize::from(*row - 26)][usize::from(*designation)] = Some(words),
            }
        }
        if page_numbers != 1 || subcodes != 1 {
            return Err(location.error(NativeTeletextEditorialTtiFailure::Identity));
        }
        Ok(page)
    }
    fn validate_header(&self, location: Location) -> Result<()> {
        if let Some(cells) = self.rows[0] {
            if let Some(column) = cells[..8].iter().position(|cell| *cell != b' ') {
                return Err(NativeTeletextEditorialTtiError {
                    row: Some(0),
                    column: Some(column),
                    ..location.error(NativeTeletextEditorialTtiFailure::HeaderIdentity)
                });
            }
        }
        Ok(())
    }
}

fn status(controls: u16) -> u16 {
    0x8000
        | ((controls & 1) << 14)
        | ((controls >> 1) & 0x7f)
        | ((controls & 0x100) << 1)
        | ((controls & 0x200) >> 1)
        | ((controls & 0x400) >> 3)
}
fn owner(mapping: &TeletextExportPageMapping, location: Location) -> Result<TeletextPageAddress> {
    let page = TeletextPageNumber::new(mapping.page.page)
        .map_err(|_| location.error(NativeTeletextEditorialTtiFailure::Mapping))?;
    let subcode = TeletextSubcode::new(mapping.page.subcode)
        .map_err(|_| location.error(NativeTeletextEditorialTtiFailure::Mapping))?;
    let expected = (u16::from(page.magazine()) << 8) | u16::from(page.wire_page());
    if mapping.editor_page != expected
        || mapping.editor_ordinal != 0
        || mapping.header_controls > 0x7ff
    {
        return Err(location.error(NativeTeletextEditorialTtiFailure::Mapping));
    }
    Ok(TeletextPageAddress { page, subcode })
}
fn protected(
    baseline: &Page<'_>,
    edited: &Page<'_>,
    location: Location,
    deadline: Instant,
) -> Result<()> {
    for (same, failure) in [
        (
            baseline.status.is_some() && baseline.status == edited.status,
            NativeTeletextEditorialTtiFailure::Status,
        ),
        (
            baseline.region.is_some() && baseline.region == edited.region,
            NativeTeletextEditorialTtiFailure::Region,
        ),
        (
            baseline.links == edited.links,
            NativeTeletextEditorialTtiFailure::Navigation,
        ),
        (
            baseline.description == edited.description,
            NativeTeletextEditorialTtiFailure::Description,
        ),
        (
            baseline.cycle.is_some() && baseline.cycle == edited.cycle,
            NativeTeletextEditorialTtiFailure::Cycle,
        ),
    ] {
        if !same {
            return Err(location.error(failure));
        }
    }
    // PS stays exact, including NOS bits. The pinned source changes an internal
    // default NOS on X28 load, but that alone does not prove normalization of its
    // separately stored C12..C14 control bits. No permissive status mask is used.
    for row in 26_u8..=29 {
        for designation in 0_u8..16 {
            location.check_deadline(deadline)?;
            if baseline.packets[usize::from(row - 26)][usize::from(designation)]
                != edited.packets[usize::from(row - 26)][usize::from(designation)]
            {
                return Err(NativeTeletextEditorialTtiError {
                    row: Some(row),
                    designation: Some(designation),
                    ..location.error(NativeTeletextEditorialTtiFailure::Packet)
                });
            }
        }
    }
    if baseline.rows[25] != edited.rows[25] {
        return Err(NativeTeletextEditorialTtiError {
            row: Some(25),
            ..location.error(NativeTeletextEditorialTtiFailure::ProtectedRow)
        });
    }
    Ok(())
}

/// Reconcile edited base rows against one authenticated server export mapping.
/// Caller authentication and edit-region/effect validation remain mandatory.
/// Only rows 0..24 may change; protected metadata/declarations must remain exact
/// after numeric/line-ending parsing and logical packet reordering.
/// # Errors
/// Rejects bounds/deadline, identity/header writes, changed/dropped protected
/// records and unsupported syntax. Description/CT changes are explicitly refused
/// because this base-row overlay cannot retain their edited values losslessly.
pub fn reconcile_teletext_editorial_tti(
    mapping: &TeletextExportPageMapping,
    baseline_bytes: &[u8],
    edited_bytes: &[u8],
    budget: TeletextTtiBudget,
    deadline: Instant,
) -> Result<Option<TeletextPhysicalBaseOverlay>> {
    let location = Location {
        page: mapping.page.page,
        subcode: mapping.page.subcode,
    };
    location.check_deadline(deadline)?;
    let owner = owner(mapping, location)?;
    let baseline_document = location.parse(baseline_bytes, budget, deadline)?;
    let edited_document = location.parse(edited_bytes, budget, deadline)?;
    let baseline = Page::index(&baseline_document, mapping, location, deadline)?;
    let edited = Page::index(&edited_document, mapping, location, deadline)?;
    if baseline.status != Some(status(mapping.header_controls)) {
        return Err(location.error(NativeTeletextEditorialTtiFailure::Mapping));
    }
    baseline.validate_header(location)?;
    edited.validate_header(location)?;
    protected(&baseline, &edited, location, deadline)?;
    let mut rows = Vec::new();
    // QTeletextMaker 1.0 LevelOnePage::setCharacter clears all-space packets;
    // SaveFormat::writeX1to25Packets and header output write only packetExists.
    // An absent ordinary base row therefore denotes forty spaces, not unknown
    // payload. This does not apply to protected resource/enhancement packets.
    for row in 0_u8..=24 {
        location.check_deadline(deadline)?;
        let before = baseline.rows[usize::from(row)]
            .copied()
            .unwrap_or([b' '; 40]);
        let after = edited.rows[usize::from(row)].copied().unwrap_or([b' '; 40]);
        if before != after {
            rows.push(TeletextPhysicalBaseRow::new(row, after).map_err(|_| {
                NativeTeletextEditorialTtiError {
                    row: Some(row),
                    ..location.error(NativeTeletextEditorialTtiFailure::Structure)
                }
            })?);
        }
    }
    location.check_deadline(deadline)?;
    if rows.is_empty() {
        return Ok(None);
    }
    TeletextPhysicalBaseOverlay::new(owner, rows)
        .map(Some)
        .map_err(|_| location.error(NativeTeletextEditorialTtiFailure::Structure))
}

#[cfg(test)]
#[path = "teletext_editorial_tti/tests.rs"]
mod tests;
