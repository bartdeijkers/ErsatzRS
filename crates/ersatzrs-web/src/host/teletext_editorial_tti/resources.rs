//! Receipt-qualified DRCS bank reconciliation; editor ordinals never grant S1 authority.

use super::{Location, NativeTeletextEditorialTtiFailure as Failure, Page, Result};
use ersatzrs_app::teletext_tti::{TeletextTtiBudget, TeletextTtiCycleUnit};
use ersatzrs_domain::{
    TeletextDrcsBitmap, TeletextDrcsBitmapOverlay, TeletextDrcsSubtable, TeletextResourceIdentity,
    TeletextResourceKind,
};
use ersatzrs_ffmpeg::teletext::{
    project_encoded_teletext_drcs, EncodedTeletextAuthoredPage, EncodedTeletextAuthoredService,
    TeletextEncodedDisplayRecord,
};
use std::time::Instant;

mod groups;
mod mapping;
mod padding;
pub(in crate::host) use mapping::{mapping, supported};

/// Ordered receipt claims, with a fixed four-bit native subtable namespace.
#[derive(Clone, Copy)]
pub(in crate::host) struct DrcsTtiBank {
    groups: [Option<DrcsTtiContext>; 16],
    len: usize,
}
impl DrcsTtiBank {
    fn location(self) -> Location {
        self.groups.iter().flatten().next().map_or(
            Location {
                page: 0,
                subcode: 0,
                resource: None,
            },
            |context| context.location(),
        )
    }
    fn parse(
        self,
        bytes: &[u8],
        budget: TeletextTtiBudget,
        deadline: Instant,
    ) -> Result<ersatzrs_app::teletext_tti::TeletextTtiDocument> {
        self.location()
            .parse(bytes, budget, deadline)
            .map_err(|mut error| {
                // Only error coordinates use the failing record's group position.
                // Uploaded PN/SC values never choose or authenticate native S1.
                if let Some(cause) = &error.cause {
                    if bytes.len() <= budget.max_bytes
                        && cause.line <= budget.max_records.saturating_add(1)
                    {
                        let count = bytes
                            .split(|byte| *byte == b'\n')
                            .take(cause.line)
                            .filter(|line| line.starts_with(b"PN,"))
                            .take(17)
                            .count();
                        if count > 0 && count <= self.len {
                            let location = self.group_location(count - 1);
                            error.resource = location.resource;
                            error.page = location.page;
                            error.subcode = location.subcode;
                        }
                    }
                }
                error
            })
    }
    fn group_location(self, index: usize) -> Location {
        self.groups.get(index).copied().flatten().map_or_else(
            || Location {
                resource: None,
                subcode: u16::try_from(index).unwrap_or(u16::MAX),
                ..self.location()
            },
            DrcsTtiContext::location,
        )
    }
}

/// Borrowed trusted native identity, independently checked against the linked asset.
#[derive(Clone, Copy)]
pub(in crate::host) struct DrcsTtiContext {
    identity: TeletextResourceIdentity,
    physical_subcode: u16,
}

impl DrcsTtiContext {
    fn location(self) -> Location {
        // Resources retain their raw hexadecimal address, not display-page BCD.
        Location {
            page: (u16::from(self.identity.page().magazine()) << 8)
                | u16::from(self.identity.page().page()),
            subcode: u16::from(self.identity.logical_subpage().get()),
            resource: Some(self.identity),
        }
    }
}

fn metadata(page: &Page<'_>, location: Location, status: u16) -> Result<()> {
    if page.status != Some(status) {
        return Err(location.error(Failure::Status));
    }
    if page.region.is_some_and(|region| region != 0) {
        return Err(location.error(Failure::Region));
    }
    if page
        .cycle
        .is_some_and(|cycle| cycle != (20, TeletextTtiCycleUnit::Time))
    {
        return Err(location.error(Failure::Cycle));
    }
    if page.description.is_some()
        || page.links.is_some()
        || page.rows[0].is_some()
        || page.rows[25].is_some()
    {
        return Err(location.error(Failure::Structure));
    }
    Ok(())
}

fn compare_native(
    page: &Page<'_>,
    native: &[TeletextEncodedDisplayRecord],
    location: Location,
    deadline: Instant,
) -> Result<()> {
    let mut rows = [None; 26];
    let mut packets = [[None; 16]; 4];
    for record in native {
        location.check_deadline(deadline)?;
        match record {
            TeletextEncodedDisplayRecord::HeaderText(_) => {}
            TeletextEncodedDisplayRecord::BaseRow { row, cells } => {
                rows[usize::from(*row)] = Some(cells);
            }
            TeletextEncodedDisplayRecord::Triplets {
                row,
                designation,
                words,
            } => packets[usize::from(*row - 26)][usize::from(*designation)] = Some(words),
            TeletextEncodedDisplayRecord::EditorialLinks { .. } => {
                return Err(location.error(Failure::Structure))
            }
        }
    }
    for row in 0..26_u8 {
        if page.rows[usize::from(row)] != rows[usize::from(row)] {
            return Err(super::NativeTeletextEditorialTtiError {
                row: Some(row),
                column: page.rows[usize::from(row)]
                    .zip(rows[usize::from(row)])
                    .and_then(|(actual, expected)| {
                        actual.iter().zip(expected).position(|(a, b)| a != b)
                    }),
                ..location.error(Failure::Packet)
            });
        }
    }
    compare_packets(&page.packets, &packets, location)
}

fn compare_packets(
    before: &[[Option<&[u32; 13]>; 16]; 4],
    after: &[[Option<&[u32; 13]>; 16]; 4],
    location: Location,
) -> Result<()> {
    for row in 26_u8..=29 {
        for designation in 0_u8..16 {
            let old = before[usize::from(row - 26)][usize::from(designation)];
            let new = after[usize::from(row - 26)][usize::from(designation)];
            if old != new {
                return Err(super::NativeTeletextEditorialTtiError {
                    row: Some(row),
                    designation: Some(designation),
                    column: old
                        .zip(new)
                        .and_then(|(old, new)| old.iter().zip(new).position(|(a, b)| a != b)),
                    ..location.error(Failure::Packet)
                });
            }
        }
    }
    Ok(())
}

fn validate_page(
    context: DrcsTtiContext,
    page: &Page<'_>,
    service: &EncodedTeletextAuthoredService,
    max_units: usize,
    deadline: Instant,
) -> Result<()> {
    let location = context.location();
    location.check_deadline(deadline)?;
    let resource = service
        .pages()
        .iter()
        .find(|page| {
            page.resource_header().is_some_and(|header| {
                header.identity() == context.identity
                    && header.physical_subcode() == context.physical_subcode
            })
        })
        .ok_or_else(|| location.error(Failure::Mapping))?;
    let projection = project_encoded_teletext_drcs(resource, max_units)
        .map_err(|_| location.error(Failure::Packet))?;
    metadata(
        page,
        location,
        super::super::teletext_export_project::TeletextEditorCarrier::Legacy
            .status(projection.header.controls.raw(), 0),
    )?;
    compare_native(page, &projection.records, location, deadline)
}

fn validate_original<'a>(
    bank: DrcsTtiBank,
    document: &'a ersatzrs_app::teletext_tti::TeletextTtiDocument,
    service: &EncodedTeletextAuthoredService,
    max_units: usize,
    deadline: Instant,
) -> Result<groups::Groups<'a>> {
    let groups = groups::Groups::index(document, bank, false, deadline)?;
    for index in 0..bank.len {
        let page = groups.page(index, bank, deadline)?;
        if let Some(context) = bank.groups[index] {
            validate_page(context, &page, service, max_units, deadline)?;
        } else {
            padding::validate(&page, bank, index, false)?;
        }
    }
    Ok(groups)
}

/// Exact final candidate/file match, allowing only the pinned editor's global
/// multi-group ordinal offset. Native S1 always comes from the trusted bank.
pub(in crate::host) fn validate(
    bank: DrcsTtiBank,
    bytes: &[u8],
    service: &EncodedTeletextAuthoredService,
    limits: (TeletextTtiBudget, usize),
    deadline: Instant,
) -> Result<()> {
    let document = bank.parse(bytes, limits.0, deadline)?;
    let groups = groups::Groups::index(&document, bank, true, deadline)?;
    for index in 0..bank.len {
        let page = groups.page(index, bank, deadline)?;
        if let Some(context) = bank.groups[index] {
            validate_page(context, &page, service, limits.1, deadline)?;
        } else {
            padding::validate(&page, bank, index, true)?;
        }
    }
    bank.location().check_deadline(deadline)
}

fn table(
    service: &EncodedTeletextAuthoredService,
    context: DrcsTtiContext,
) -> Result<&TeletextDrcsSubtable> {
    service
        .declared()
        .semantic()
        .drcs()
        .subtables()
        .iter()
        .find(|table| table.identity() == context.identity)
        .ok_or_else(|| context.location().error(Failure::Mapping))
}

pub(in crate::host) fn reconcile(
    bank: DrcsTtiBank,
    original: &[u8],
    edited: &[u8],
    service: &EncodedTeletextAuthoredService,
    limits: (TeletextTtiBudget, usize),
    deadline: Instant,
) -> Result<Vec<TeletextDrcsBitmapOverlay>> {
    let location = bank.location();
    let original = bank.parse(original, limits.0, deadline)?;
    let edited = bank.parse(edited, limits.0, deadline)?;
    let before = validate_original(bank, &original, service, limits.1, deadline)?;
    let after = groups::Groups::index(&edited, bank, true, deadline)?;
    let mut changes = Vec::new();
    for index in 0..bank.len {
        location.check_deadline(deadline)?;
        let before = before.page(index, bank, deadline)?;
        let after = after.page(index, bank, deadline)?;
        if let Some(context) = bank.groups[index] {
            changes.extend(reconcile_page(context, &before, &after, service, deadline)?);
        } else {
            padding::validate(&after, bank, index, true)?;
        }
    }
    location.check_deadline(deadline)?;
    Ok(changes)
}

fn reconcile_page(
    context: DrcsTtiContext,
    before: &Page<'_>,
    after: &Page<'_>,
    service: &EncodedTeletextAuthoredService,
    deadline: Instant,
) -> Result<Vec<TeletextDrcsBitmapOverlay>> {
    let location = context.location();
    metadata(
        after,
        location,
        before
            .status
            .ok_or_else(|| location.error(Failure::Status))?,
    )?;
    compare_packets(&before.packets, &after.packets, location)?;
    let table = table(service, context)?;
    let mut changes = Vec::new();
    for row in 1_u8..=24 {
        location.check_deadline(deadline)?;
        let (old, new) = match (before.rows[usize::from(row)], after.rows[usize::from(row)]) {
            (None, None) => continue,
            (Some(old), Some(new)) => (old, new),
            _ => {
                return Err(super::NativeTeletextEditorialTtiError {
                    row: Some(row),
                    ..location.error(Failure::Packet)
                })
            }
        };
        for half in 0_u8..2 {
            let slot = (row - 1) * 2 + half;
            let start = usize::from(half) * 20;
            let old = &old[start..start + 20];
            let new = &new[start..start + 20];
            if old == new {
                continue;
            }
            let glyph = table
                .glyphs()
                .iter()
                .find(|glyph| glyph.slot.get() == slot)
                .ok_or_else(|| super::NativeTeletextEditorialTtiError {
                    row: Some(row),
                    column: Some(start),
                    ..location.error(Failure::Structure)
                })?;
            if let Some(column) = new.iter().position(|byte| byte & 0x40 == 0) {
                return Err(super::NativeTeletextEditorialTtiError {
                    row: Some(row),
                    column: Some(start + column),
                    ..location.error(Failure::Structure)
                });
            }
            let pixels = std::array::from_fn(|line| {
                (u16::from(new[line * 2] & 0x3f) << 6) | u16::from(new[line * 2 + 1] & 0x3f)
            });
            let bitmap =
                TeletextDrcsBitmap::new(pixels).map_err(|_| location.error(Failure::Structure))?;
            changes.push(
                TeletextDrcsBitmapOverlay::new(context.identity, glyph.slot, glyph.bitmap, bitmap)
                    .map_err(|_| location.error(Failure::Structure))?,
            );
        }
    }
    location.check_deadline(deadline)?;
    Ok(changes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ersatzrs_domain::{TeletextRawDataPageNumber, TeletextResourceSubpageIndex};
    use std::time::Duration;

    #[test]
    fn resource_packet_and_syntax_errors_retain_exact_raw_identity() {
        let identity = TeletextResourceIdentity::new(
            TeletextResourceKind::Drcs,
            TeletextRawDataPageNumber::new(1, 0xc0).unwrap(),
            TeletextResourceSubpageIndex::new(1).unwrap(),
        )
        .unwrap();
        let context = DrcsTtiContext {
            identity,
            physical_subcode: 0x0111,
        };
        let location = context.location();
        let words = [0; 13];
        let mut edited_words = words;
        edited_words[5] = 1;
        let mut before = [[None; 16]; 4];
        let mut after = before;
        before[2][3] = Some(&words);
        after[2][3] = Some(&edited_words);
        let error = compare_packets(&before, &after, location).unwrap_err();
        assert_eq!(error.resource, Some(identity));
        assert_eq!((error.page, error.subcode), (0x1c0, 1));
        assert_eq!(
            (error.row, error.column, error.designation),
            (Some(28), Some(5), Some(3))
        );
        let syntax = location
            .parse(
                b"PN,1C000\nSC,0000\nOL,1,\x1b\n",
                TeletextTtiBudget {
                    max_bytes: 1024,
                    max_records: 32,
                },
                Instant::now() + Duration::from_secs(5),
            )
            .unwrap_err();
        assert_eq!(syntax.failure, Failure::Syntax);
        assert_eq!(syntax.resource, Some(identity));
        assert_eq!((syntax.page, syntax.subcode), (0x1c0, 1));
        assert_eq!(syntax.cause.unwrap().line, 3);
    }
}
