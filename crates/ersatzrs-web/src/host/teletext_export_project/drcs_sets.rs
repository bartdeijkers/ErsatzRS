//! V4 editor-only indexed DRCS banks; native resource ownership is unchanged.

use ersatzrs_domain::TeletextRawDataPageNumber;

use super::{
    check_deadline, project_encoded_teletext_drcs, resource_identity, sha256, write_teletext_tti,
    EncodedTeletextAuthoredPage, EncodedTeletextAuthoredService, Files, Instant,
    NativeTeletextExportProjectError, Result, TeletextEditorCarrier, TeletextEncodedDisplayRecord,
    TeletextExportAssetMapping, TeletextExportAssetScope, TeletextResourceKind, TeletextTtiBudget,
    TeletextTtiDocument, TeletextTtiRecord,
};

type Bank = (TeletextResourceKind, TeletextRawDataPageNumber);
type Slot<'a> = Option<(&'a EncodedTeletextAuthoredPage, &'a str)>;

fn bank(page: &EncodedTeletextAuthoredPage) -> Option<Bank> {
    let identity = page.resource_header()?.identity();
    matches!(
        identity.kind(),
        TeletextResourceKind::Drcs | TeletextResourceKind::Gdrcs
    )
    .then_some((identity.kind(), identity.page()))
}

fn first_bank(encoded: &EncodedTeletextAuthoredService, index: usize, wanted: Bank) -> bool {
    !encoded.pages()[..index]
        .iter()
        .any(|page| bank(page) == Some(wanted))
}

/// Count companions before the parent allocates its bounded file inventory.
pub(super) fn count_banks(
    encoded: &EncodedTeletextAuthoredService,
    deadline: Instant,
) -> Result<usize> {
    let mut count = 0usize;
    for (index, page) in encoded.pages().iter().enumerate() {
        check_deadline(deadline)?;
        if bank(page).is_some_and(|wanted| first_bank(encoded, index, wanted)) {
            count = count
                .checked_add(1)
                .ok_or(NativeTeletextExportProjectError::Budget)?;
        }
    }
    Ok(count)
}

pub(super) fn append_sets(
    encoded: &EncodedTeletextAuthoredService,
    assets: &mut Vec<TeletextExportAssetMapping>,
    files: &mut Files,
    tti_budget: TeletextTtiBudget,
    deadline: Instant,
) -> Result<()> {
    for (index, page) in encoded.pages().iter().enumerate() {
        check_deadline(deadline)?;
        let Some(wanted) = bank(page).filter(|&wanted| first_bank(encoded, index, wanted)) else {
            continue;
        };
        // S1 is four bits. Fixed slots avoid allocation or any renumbering of
        // sparse native subtables before the companion's bounds are checked.
        let mut slots: [Slot<'_>; 16] = [None; 16];
        for candidate in encoded.pages() {
            check_deadline(deadline)?;
            if bank(candidate) != Some(wanted) {
                continue;
            }
            let header = candidate
                .resource_header()
                .ok_or(NativeTeletextExportProjectError::Association)?;
            let identity = header.identity();
            let slot = usize::from(identity.logical_subpage().get());
            if slots.get(slot).is_none_or(Option::is_some) {
                return Err(NativeTeletextExportProjectError::Association);
            }
            let wire = resource_identity(identity);
            let name = assets
                .iter()
                .find_map(|asset| match &asset.scope {
                    TeletextExportAssetScope::Resource {
                        identity,
                        physical_subcode,
                    } if *identity == wire && *physical_subcode == header.physical_subcode() => {
                        Some(asset.name.as_str())
                    }
                    _ => None,
                })
                .ok_or(NativeTeletextExportProjectError::Association)?;
            slots[slot] = Some((candidate, name));
        }
        let mapping = append_bank(wanted, &slots, files, tti_budget, deadline)?;
        assets.push(mapping);
    }
    Ok(())
}

fn append_bank(
    wanted: Bank,
    slots: &[Slot<'_>; 16],
    files: &mut Files,
    tti_budget: TeletextTtiBudget,
    deadline: Instant,
) -> Result<TeletextExportAssetMapping> {
    check_deadline(deadline)?;
    let highest = slots
        .iter()
        .rposition(Option::is_some)
        .ok_or(NativeTeletextExportProjectError::Association)?;
    let record_count = slots[..=highest].iter().try_fold(0usize, |count, slot| {
        // Real groups: PN, SC, PS plus all projected bodies except header text.
        // Null editor groups: PN, SC, X28/3 only; SPACE rows would define glyphs
        // in QTM and must never be emitted for missing native subtables.
        let group = match slot {
            Some((page, _)) => page.data_units().len().checked_add(2),
            None => Some(3),
        }
        .ok_or(NativeTeletextExportProjectError::Budget)?;
        count
            .checked_add(group)
            .ok_or(NativeTeletextExportProjectError::Budget)
    })?;
    let budget = TeletextTtiBudget {
        max_bytes: tti_budget
            .max_bytes
            .min(files.budget.max_file_bytes)
            .min(files.remaining),
        max_records: tti_budget.max_records.min(files.budget.max_units),
    };
    if record_count > budget.max_records || files.values.len() >= files.budget.max_files {
        return Err(NativeTeletextExportProjectError::Budget);
    }
    let prototype = slots
        .iter()
        .flatten()
        .next()
        .ok_or(NativeTeletextExportProjectError::Association)?
        .0;
    let checked = project_encoded_teletext_drcs(prototype, files.budget.max_units)?;
    let mut undefined = checked
        .records
        .iter()
        .find_map(|record| match record {
            TeletextEncodedDisplayRecord::Triplets {
                row: 28,
                designation: 3,
                words,
            } => Some(*words),
            _ => None,
        })
        .ok_or(NativeTeletextExportProjectError::Association)?;
    // Preserve the strictly checked function/coding convention of the real
    // bank. Native drcs::encode_teletext_drcs_bodies puts function in triplet 0,
    // coding/reserved bits zero, and 48 four-bit modes at bit offsets 18..209.
    // All F therefore fills words 1..10, twelve bits of word 11; word 12 is
    // reserved zero. These are editor logical records, never DVB/header bytes.
    undefined[0] = u32::from(wanted.0.function_code());
    undefined[1..11].fill(0x3ffff);
    undefined[11] = 0xfff;
    undefined[12] = 0;
    let raw_page = (u16::from(wanted.1.magazine()) << 8) | u16::from(wanted.1.page());
    let (records, resource_names) = bank_records(
        &slots[..=highest],
        raw_page,
        undefined,
        record_count,
        files.budget.max_units,
        deadline,
    )?;
    let document = TeletextTtiDocument::checked_from_records(records, budget)?;
    let bytes = write_teletext_tti(&document, budget)?;
    let kind = if wanted.0 == TeletextResourceKind::Gdrcs {
        "global"
    } else {
        "normal"
    };
    let name = format!(
        "drcs-set-{kind}-{}{:02x}.tti",
        wanted.1.magazine(),
        wanted.1.page()
    );
    let mapping = TeletextExportAssetMapping {
        name: name.clone(),
        scope: TeletextExportAssetScope::DrcsTtiSet { resource_names },
        unit_count: 0,
        bytes: bytes.len(),
        sha256: sha256(&bytes),
    };
    files.append(name, bytes)?;
    check_deadline(deadline)?;
    Ok(mapping)
}

fn bank_records(
    slots: &[Slot<'_>],
    raw_page: u16,
    undefined: [u32; 13],
    record_count: usize,
    max_units: usize,
    deadline: Instant,
) -> Result<(Vec<TeletextTtiRecord>, Vec<Option<String>>)> {
    let mut records = Vec::new();
    records
        .try_reserve_exact(record_count)
        .map_err(|_| NativeTeletextExportProjectError::Budget)?;
    let mut resource_names = Vec::new();
    resource_names
        .try_reserve_exact(slots.len())
        .map_err(|_| NativeTeletextExportProjectError::Budget)?;
    for (index, slot) in slots.iter().enumerate() {
        check_deadline(deadline)?;
        let ordinal = u64::try_from(index).map_err(|_| NativeTeletextExportProjectError::Budget)?;
        records.push(TeletextTtiRecord::PageNumber {
            page: raw_page,
            editor_ordinal: ordinal,
        });
        records.push(TeletextTtiRecord::SubpageOrdinal(ordinal));
        if let Some((page, name)) = slot {
            let projection = project_encoded_teletext_drcs(page, max_units)?;
            records.push(TeletextTtiRecord::PageStatus(
                TeletextEditorCarrier::Legacy.status(projection.header.controls.raw(), 0),
            ));
            for record in projection.records {
                match record {
                    TeletextEncodedDisplayRecord::HeaderText(_) => {}
                    TeletextEncodedDisplayRecord::BaseRow { row, cells } => {
                        records.push(TeletextTtiRecord::BaseRow { row, cells });
                    }
                    TeletextEncodedDisplayRecord::Triplets {
                        row,
                        designation,
                        words,
                    } => records.push(TeletextTtiRecord::Packet {
                        row,
                        designation,
                        words,
                    }),
                    TeletextEncodedDisplayRecord::EditorialLinks { .. } => {
                        return Err(NativeTeletextExportProjectError::Association)
                    }
                }
            }
            resource_names.push(Some((*name).to_owned()));
        } else {
            records.push(TeletextTtiRecord::Packet {
                row: 28,
                designation: 3,
                words: undefined,
            });
            resource_names.push(None);
        }
    }
    Ok((records, resource_names))
}
