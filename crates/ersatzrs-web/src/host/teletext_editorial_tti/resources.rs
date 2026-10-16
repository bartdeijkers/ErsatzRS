//! Receipt-qualified singleton DRCS TTI reconciliation for legacy and V4 carriers.

use super::{Location, NativeTeletextEditorialTtiFailure as Failure, Page, Result};
use ersatzrs_addon_contract::TeletextResourceKindWire;
use ersatzrs_app::{
    teletext_export_project::{
        TeletextExportAssetMapping, TeletextExportAssetScope, TeletextExportProject,
    },
    teletext_tti::{TeletextTtiBudget, TeletextTtiCycleUnit},
};
use ersatzrs_domain::{
    TeletextDrcsBitmap, TeletextDrcsBitmapOverlay, TeletextDrcsSubtable, TeletextResourceIdentity,
    TeletextResourceKind,
};
use ersatzrs_ffmpeg::teletext::{
    project_encoded_teletext_drcs, EncodedTeletextAuthoredPage, EncodedTeletextAuthoredService,
    TeletextEncodedDisplayRecord,
};
use std::time::Instant;

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
        }
    }
}

/// Singleton V4 banks carry the same checked records as a legacy companion.
/// Multi-group and padded banks require separate ordinal reconciliation.
pub(in crate::host) fn supported(companion: &TeletextExportAssetMapping) -> bool {
    match &companion.scope {
        TeletextExportAssetScope::DrcsTti {
            editor_ordinal: 0, ..
        } => true,
        TeletextExportAssetScope::DrcsTtiSet { resource_names } => {
            matches!(resource_names.as_slice(), [Some(_)])
        }
        _ => false,
    }
}

pub(in crate::host) fn mapping(
    companion: &TeletextExportAssetMapping,
    project: &TeletextExportProject,
    service: &EncodedTeletextAuthoredService,
) -> Result<DrcsTtiContext> {
    let invalid = || {
        Location {
            page: 0,
            subcode: 0,
        }
        .error(Failure::Mapping)
    };
    let (resource_name, singleton_set) = match &companion.scope {
        TeletextExportAssetScope::DrcsTti {
            resource_name,
            editor_ordinal: 0,
        } => (resource_name, false),
        TeletextExportAssetScope::DrcsTtiSet { resource_names } => {
            let [Some(name)] = resource_names.as_slice() else {
                return Err(invalid());
            };
            (name, true)
        }
        _ => return Err(invalid()),
    };
    let resource = project
        .assets
        .iter()
        .find(|asset| asset.name == *resource_name)
        .ok_or_else(invalid)?;
    let TeletextExportAssetScope::Resource {
        identity,
        physical_subcode,
    } = &resource.scope
    else {
        return Err(invalid());
    };
    if singleton_set && identity.subpage != 0 {
        return Err(invalid());
    }
    let page = service
        .pages()
        .iter()
        .filter_map(EncodedTeletextAuthoredPage::resource_header)
        .find(|header| {
            let native = header.identity();
            matches!(
                (native.kind(), identity.kind),
                (TeletextResourceKind::Drcs, TeletextResourceKindWire::Drcs)
                    | (TeletextResourceKind::Gdrcs, TeletextResourceKindWire::Gdrcs)
            ) && native.page().magazine() == identity.page.magazine
                && native.page().page() == identity.page.page
                && native.logical_subpage().get() == identity.subpage
                && header.physical_subcode() == *physical_subcode
        })
        .ok_or_else(invalid)?;
    Ok(DrcsTtiContext {
        identity: page.identity(),
        physical_subcode: *physical_subcode,
    })
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

/// Exact full candidate/file match; edited metadata never derives native headers.
pub(in crate::host) fn validate(
    context: DrcsTtiContext,
    bytes: &[u8],
    service: &EncodedTeletextAuthoredService,
    limits: (TeletextTtiBudget, usize),
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
    let projection = project_encoded_teletext_drcs(resource, limits.1)
        .map_err(|_| location.error(Failure::Packet))?;
    let document = location.parse(bytes, limits.0, deadline)?;
    let page = Page::index_identity(&document, location.page, 0, location, deadline)?;
    metadata(
        &page,
        location,
        super::super::teletext_export_project::TeletextEditorCarrier::Legacy
            .status(projection.header.controls.raw(), 0),
    )?;
    compare_native(&page, &projection.records, location, deadline)
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
    context: DrcsTtiContext,
    original: &[u8],
    edited: &[u8],
    service: &EncodedTeletextAuthoredService,
    limits: (TeletextTtiBudget, usize),
    deadline: Instant,
) -> Result<Vec<TeletextDrcsBitmapOverlay>> {
    let location = context.location();
    validate(context, original, service, limits, deadline)?;
    let original = location.parse(original, limits.0, deadline)?;
    let edited = location.parse(edited, limits.0, deadline)?;
    let before = Page::index_identity(&original, location.page, 0, location, deadline)?;
    let after = Page::index_identity(&edited, location.page, 0, location, deadline)?;
    metadata(
        &after,
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
