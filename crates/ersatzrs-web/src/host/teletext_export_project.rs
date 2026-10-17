//! Bounded editor projection of an immutable saved original, without authority.
//!
//! Exact resource/magazine assets stay outside editable display TTI files.
//! Versioned `QTeletextMaker` carriers preserve its effective editor charset;
//! legacy receipts retain their exact original PS mapping for authenticated replay.
//! Original raw header flags and navigation subcodes stay in trusted mappings.

use std::{collections::BTreeSet, fmt::Write as _, io, time::Instant};

use ersatzrs_addon_contract::{
    TeletextNavigationLinkWire, TeletextNavigationRoleWire, TeletextNavigationTargetWire,
    TeletextPageAddressWire, TeletextPageKindWire, TeletextRawDataPageNumberWire,
    TeletextResourceIdentityWire, TeletextResourceKindWire,
};
use ersatzrs_app::{
    teletext_design_documents::TeletextSavedV1Region,
    teletext_export_project::{
        TeletextExportAssetMapping, TeletextExportAssetScope, TeletextExportFile,
        TeletextExportInstallation, TeletextExportOriginal, TeletextExportPageMapping,
        TeletextExportProducer, TeletextExportProject, TeletextExportProjectBudget,
        TeletextExportProjectError, TeletextExportReceiptBody,
    },
    teletext_tti::{
        write_teletext_tti, TeletextTtiBudget, TeletextTtiCycleUnit, TeletextTtiDocument,
        TeletextTtiError, TeletextTtiRecord,
    },
};
use ersatzrs_domain::{
    TeletextNavigationRole, TeletextPageAddress, TeletextPageDraft, TeletextPageKind,
    TeletextProducerPagePresentation, TeletextResourceIdentity, TeletextResourceKind,
};
use ersatzrs_ffmpeg::teletext::{
    project_encoded_teletext_display, project_encoded_teletext_drcs,
    project_encoded_teletext_magazines, EncodedTeletextAuthoredPage,
    EncodedTeletextAuthoredService, EncodedTeletextPage, TeletextEncodedDisplayProjection,
    TeletextEncodedDisplayProjectionError, TeletextEncodedDisplayRecord,
    TeletextEncodedMagazineRecord, TeletextEncodingRegion,
};
use sha2::{Digest, Sha256};

use super::teletext_stored_designs::OriginalDesignView;

mod drcs_sets;
pub(super) mod editor_presentation;
mod object_fragments;
pub(super) mod resource_objects;

/// Located failures without original text, settings or file bodies.
#[derive(Debug, thiserror::Error)]
pub enum NativeTeletextExportProjectError {
    #[error("teletext export exceeded its original deadline")]
    TimedOut,
    #[error("teletext export exceeds its trusted storage or count bounds")]
    Budget,
    #[error("teletext export selection or original ownership is invalid")]
    Association,
    #[error("the effective teletext original has no enabled display pages to export")]
    NoDisplay,
    #[error(transparent)]
    Projection(#[from] TeletextEncodedDisplayProjectionError),
    #[error(transparent)]
    Tti(#[from] TeletextTtiError),
    #[error(transparent)]
    Project(#[from] TeletextExportProjectError),
}
type Result<T> = std::result::Result<T, NativeTeletextExportProjectError>;

fn check_deadline(deadline: Instant) -> Result<()> {
    if Instant::now() >= deadline {
        return Err(NativeTeletextExportProjectError::TimedOut);
    }
    Ok(())
}
fn sha256(bytes: &[u8]) -> String {
    let mut value = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        let _ = write!(value, "{byte:02x}");
    }
    value
}
fn wire_address(address: TeletextPageAddress) -> TeletextPageAddressWire {
    TeletextPageAddressWire {
        page: address.page.get(),
        subcode: address.subcode.get(),
    }
}
fn editor_page(page: u16) -> u16 {
    ((page / 100) << 8) | (((page / 10) % 10) << 4) | (page % 10)
}
/// Editor carrier convention, distinct from native DVB header controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TeletextEditorCarrier {
    Legacy,
    V2,
    V3,
    V4,
    V5,
    V6,
    V7,
    V8,
}
impl TeletextEditorCarrier {
    pub(super) fn from_profile(profile: &str) -> Option<Self> {
        match profile {
            "qteletextmaker-1.0" => Some(Self::Legacy),
            "qteletextmaker-1.0-carrier-v2" => Some(Self::V2),
            "qteletextmaker-1.0-carrier-v3" => Some(Self::V3),
            "qteletextmaker-1.0-carrier-v4" => Some(Self::V4),
            "qteletextmaker-1.0-carrier-v5" => Some(Self::V5),
            "qteletextmaker-1.0-carrier-v6" => Some(Self::V6),
            "qteletextmaker-1.0-carrier-v7" => Some(Self::V7),
            "qteletextmaker-1.0-carrier-v8" => Some(Self::V8),
            _ => None,
        }
    }
    pub(super) const fn profile(self) -> &'static str {
        match self {
            Self::Legacy => "qteletextmaker-1.0",
            Self::V2 => "qteletextmaker-1.0-carrier-v2",
            Self::V3 => "qteletextmaker-1.0-carrier-v3",
            Self::V4 => "qteletextmaker-1.0-carrier-v4",
            Self::V5 => "qteletextmaker-1.0-carrier-v5",
            Self::V6 => "qteletextmaker-1.0-carrier-v6",
            Self::V7 => "qteletextmaker-1.0-carrier-v7",
            Self::V8 => "qteletextmaker-1.0-carrier-v8",
        }
    }
    /// Keep native flags separate from QTM's single effective primary NOS.
    pub(super) fn status(self, controls: u16, effective_primary: u8) -> u16 {
        let common = 0x8000 | ((controls & 1) << 14) | ((controls >> 1) & 0x7f);
        let national = match self {
            // Immutable legacy receipts copied compact native control positions.
            Self::Legacy => {
                ((controls & 0x100) << 1) | ((controls & 0x200) >> 1) | ((controls & 0x400) >> 3)
            }
            // Pinned QTM LevelOnePage loads X28 before applying PS C12–C14.
            // Its C12 means numeric NOS bit 0, unlike compact native bit 8.
            // Use the resolved X28/M29/header primary; retain native flags in
            // the mapping, including an independently authored fallback NOS.
            Self::V2 | Self::V3 | Self::V4 | Self::V5 | Self::V6 | Self::V7 | Self::V8 => {
                let primary = u16::from(effective_primary);
                ((primary & 1) << 9) | ((primary & 2) << 7) | ((primary & 4) << 5)
            }
        };
        common | national
    }
}
fn navigation(page: &TeletextPageDraft) -> Vec<TeletextNavigationLinkWire> {
    page.navigation
        .iter()
        .map(|link| TeletextNavigationLinkWire {
            role: match link.role {
                TeletextNavigationRole::Index => TeletextNavigationRoleWire::Index,
                TeletextNavigationRole::Red => TeletextNavigationRoleWire::Red,
                TeletextNavigationRole::Green => TeletextNavigationRoleWire::Green,
                TeletextNavigationRole::Yellow => TeletextNavigationRoleWire::Yellow,
                TeletextNavigationRole::Cyan => TeletextNavigationRoleWire::Cyan,
            },
            target: TeletextNavigationTargetWire {
                page: link.target.page.get(),
                subcode: link
                    .target
                    .subcode
                    .map(ersatzrs_domain::TeletextSubcode::get),
            },
        })
        .collect()
}
fn fast_links(page: &TeletextPageDraft) -> [u16; 6] {
    [
        Some(TeletextNavigationRole::Red),
        Some(TeletextNavigationRole::Green),
        Some(TeletextNavigationRole::Yellow),
        Some(TeletextNavigationRole::Cyan),
        None,
        Some(TeletextNavigationRole::Index),
    ]
    .map(|role| {
        role.and_then(|role| page.navigation.iter().find(|link| link.role == role))
            .map_or_else(
                || (u16::from(page.address.page.magazine()) << 8) | 0xff,
                |link| editor_page(link.target.page.get()),
            )
    })
}
fn resource_identity(identity: TeletextResourceIdentity) -> TeletextResourceIdentityWire {
    TeletextResourceIdentityWire {
        kind: match identity.kind() {
            TeletextResourceKind::Pop => TeletextResourceKindWire::Pop,
            TeletextResourceKind::Gpop => TeletextResourceKindWire::Gpop,
            TeletextResourceKind::Drcs => TeletextResourceKindWire::Drcs,
            TeletextResourceKind::Gdrcs => TeletextResourceKindWire::Gdrcs,
            TeletextResourceKind::Mot => TeletextResourceKindWire::Mot,
            TeletextResourceKind::Mip => TeletextResourceKindWire::Mip,
        },
        page: TeletextRawDataPageNumberWire {
            magazine: identity.page().magazine(),
            page: identity.page().page(),
        },
        subpage: identity.logical_subpage().get(),
    }
}

struct ByteCounter {
    remaining: usize,
}
impl io::Write for ByteCounter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.remaining = self
            .remaining
            .checked_sub(bytes.len())
            .ok_or_else(|| io::Error::other("teletext export byte budget"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
struct Files {
    carrier: TeletextEditorCarrier,
    values: Vec<TeletextExportFile>,
    remaining: usize,
    budget: TeletextExportProjectBudget,
}
impl Files {
    fn append(&mut self, name: String, bytes: Vec<u8>) -> Result<()> {
        if self.values.len() >= self.budget.max_files || bytes.len() > self.budget.max_file_bytes {
            return Err(NativeTeletextExportProjectError::Budget);
        }
        self.remaining = self
            .remaining
            .checked_sub(bytes.len())
            .ok_or(NativeTeletextExportProjectError::Budget)?;
        self.values.push(TeletextExportFile { name, bytes });
        Ok(())
    }
    fn asset(
        &mut self,
        name: String,
        scope: TeletextExportAssetScope,
        units: &[[u8; 46]],
    ) -> Result<TeletextExportAssetMapping> {
        let bytes = units
            .len()
            .checked_mul(46)
            .filter(|&n| n <= self.budget.max_file_bytes && n <= self.remaining)
            .ok_or(NativeTeletextExportProjectError::Budget)?;
        if self.values.len() >= self.budget.max_files {
            return Err(NativeTeletextExportProjectError::Budget);
        }
        let mut value = Vec::new();
        value
            .try_reserve_exact(bytes)
            .map_err(|_| NativeTeletextExportProjectError::Budget)?;
        for unit in units {
            value.extend_from_slice(unit);
        }
        let mapping = TeletextExportAssetMapping {
            name: name.clone(),
            scope,
            unit_count: units.len(),
            bytes,
            sha256: sha256(&value),
        };
        self.append(name, value)?;
        Ok(mapping)
    }
}
fn display_page(page: &EncodedTeletextAuthoredPage) -> Option<&EncodedTeletextPage> {
    match page {
        EncodedTeletextAuthoredPage::NativeDisplay(value) => Some(value.encoded().page()),
        EncodedTeletextAuthoredPage::CompiledDisplay(value) => Some(value.page()),
        EncodedTeletextAuthoredPage::Object(_) | EncodedTeletextAuthoredPage::Resource(_) => None,
    }
}
fn charge_units(used: &mut usize, count: usize, maximum: usize) -> Result<()> {
    *used = used
        .checked_add(count)
        .filter(|&n| n <= maximum)
        .ok_or(NativeTeletextExportProjectError::Budget)?;
    Ok(())
}

struct ExportInventory<'a> {
    encoded: &'a EncodedTeletextAuthoredService,
    wanted: BTreeSet<TeletextPageAddress>,
    selected: bool,
    selected_count: usize,
    asset_count: usize,
}
fn selected_inventory<'a>(
    original: OriginalDesignView<'a>,
    expected_head: i64,
    selected: Option<&[TeletextPageAddress]>,
    project_budget: TeletextExportProjectBudget,
    tti_budget: TeletextTtiBudget,
    deadline: Instant,
    carrier: TeletextEditorCarrier,
) -> Result<ExportInventory<'a>> {
    check_deadline(deadline)?;
    for limit in [
        project_budget.max_bytes,
        project_budget.max_files,
        project_budget.max_producers,
        project_budget.max_file_bytes,
        project_budget.max_units,
        tti_budget.max_bytes,
        tti_budget.max_records,
    ] {
        if limit == 0 || isize::try_from(limit).is_err() || u32::try_from(limit).is_err() {
            return Err(NativeTeletextExportProjectError::Budget);
        }
    }
    let producer_count = original.producer_count();
    if matches!(original, OriginalDesignView::Effective(_)) && producer_count == 0 {
        return Err(NativeTeletextExportProjectError::NoDisplay);
    }
    if expected_head < original.revision()
        || producer_count == 0
        || producer_count > project_budget.max_producers
        || original.producers().len() != producer_count
        || original.sources().len() != producer_count
    {
        return Err(NativeTeletextExportProjectError::Association);
    }
    let encoded = original
        .encoded()
        .ok_or(NativeTeletextExportProjectError::Association)?;
    if selected.is_some_and(|pages| pages.is_empty() || pages.len() > project_budget.max_files) {
        return Err(NativeTeletextExportProjectError::Association);
    }
    let mut wanted = BTreeSet::new();
    if let Some(selected) = selected {
        for &address in selected {
            if !wanted.insert(address) {
                return Err(NativeTeletextExportProjectError::Association);
            }
        }
    }
    let selected_count = encoded
        .pages()
        .iter()
        .filter_map(display_page)
        .filter(|page| selected.is_none() || wanted.contains(&page.address()))
        .count();
    let asset_count = asset_count(encoded, carrier, project_budget.max_units, deadline)?;
    if selected_count == 0
        || selected.is_some_and(|pages| selected_count != pages.len())
        || selected_count
            .checked_add(asset_count)
            .is_none_or(|n| n > project_budget.max_files)
    {
        return Err(NativeTeletextExportProjectError::Association);
    }
    Ok(ExportInventory {
        encoded,
        wanted,
        selected: selected.is_some(),
        selected_count,
        asset_count,
    })
}

fn asset_count(
    encoded: &EncodedTeletextAuthoredService,
    carrier: TeletextEditorCarrier,
    max_units: usize,
    deadline: Instant,
) -> Result<usize> {
    let asset_count = encoded
        .pages()
        .iter()
        .filter(|page| page.resource_header().is_some())
        .count()
        .checked_add(encoded.magazine_units().len())
        .and_then(|count| {
            count.checked_add(
                encoded
                    .pages()
                    .iter()
                    .filter(|page| drcs_companion_eligible(page, carrier))
                    .count(),
            )
        })
        .ok_or(NativeTeletextExportProjectError::Budget)?;
    let asset_count = if matches!(
        carrier,
        TeletextEditorCarrier::V4
            | TeletextEditorCarrier::V5
            | TeletextEditorCarrier::V6
            | TeletextEditorCarrier::V7
            | TeletextEditorCarrier::V8
    ) {
        asset_count
            .checked_add(drcs_sets::count_banks(encoded, deadline)?)
            .ok_or(NativeTeletextExportProjectError::Budget)?
    } else {
        asset_count
    };
    let asset_count = if matches!(
        carrier,
        TeletextEditorCarrier::V6 | TeletextEditorCarrier::V8
    ) {
        asset_count
            .checked_add(object_companion_count(encoded, deadline)?)
            .ok_or(NativeTeletextExportProjectError::Budget)?
    } else {
        asset_count
    };
    let asset_count = if carrier == TeletextEditorCarrier::V7 {
        asset_count
            .checked_add(object_fragments::count(encoded, max_units, deadline)?)
            .ok_or(NativeTeletextExportProjectError::Budget)?
    } else {
        asset_count
    };
    Ok(asset_count)
}

fn object_companion_count(
    encoded: &EncodedTeletextAuthoredService,
    deadline: Instant,
) -> Result<usize> {
    let mut count = 0usize;
    for page in encoded.pages() {
        check_deadline(deadline)?;
        if let EncodedTeletextAuthoredPage::Object(page) = page {
            count = count
                .checked_add(page.definition_positions().len())
                .ok_or(NativeTeletextExportProjectError::Budget)?;
        }
    }
    Ok(count)
}

fn append_object_companions(
    page: &EncodedTeletextAuthoredPage,
    resource_name: &str,
    files: &mut Files,
    assets: &mut Vec<TeletextExportAssetMapping>,
    tti: TeletextTtiBudget,
    deadline: Instant,
) -> Result<()> {
    let EncodedTeletextAuthoredPage::Object(object_page) = page else {
        return Ok(());
    };
    for position in object_page.definition_positions() {
        check_deadline(deadline)?;
        if files.values.len() >= files.budget.max_files {
            return Err(NativeTeletextExportProjectError::Budget);
        }
        let budget = TeletextTtiBudget {
            max_bytes: tti
                .max_bytes
                .min(files.budget.max_file_bytes)
                .min(files.remaining),
            max_records: tti.max_records.min(files.budget.max_units),
        };
        let frame = resource_objects::build_frame(
            page,
            position.target(),
            files.budget.max_units,
            budget,
            deadline,
        )?;
        let bytes = write_teletext_tti(&frame.document, budget)?;
        let object_id = position.target().object_id().get();
        let name = format!(
            "{}-object-{object_id:05}.tti",
            resource_name.trim_end_matches(".dvb")
        );
        let mapping = TeletextExportAssetMapping {
            name: name.clone(),
            scope: TeletextExportAssetScope::ObjectTtiV1 {
                resource_name: resource_name.to_owned(),
                object_id,
                editor_page: 0x100,
                editor_ordinal: 0,
            },
            unit_count: 0,
            bytes: bytes.len(),
            sha256: sha256(&bytes),
        };
        files.append(name, bytes)?;
        assets.push(mapping);
    }
    Ok(())
}

/// New context carriers regenerate protected aliases from the complete original.
/// Historical V6/V7 wrappers and native resource files keep their original bytes.
fn append_object_contexts(
    encoded: &EncodedTeletextAuthoredService,
    page: &EncodedTeletextAuthoredPage,
    resource_name: &str,
    files: &mut Files,
    assets: &mut Vec<TeletextExportAssetMapping>,
    tti: TeletextTtiBudget,
    deadline: Instant,
) -> Result<()> {
    let EncodedTeletextAuthoredPage::Object(object_page) = page else {
        return Ok(());
    };
    for position in object_page.definition_positions() {
        check_deadline(deadline)?;
        if files.values.len() >= files.budget.max_files {
            return Err(NativeTeletextExportProjectError::Budget);
        }
        let budget = TeletextTtiBudget {
            max_bytes: tti
                .max_bytes
                .min(files.budget.max_file_bytes)
                .min(files.remaining),
            max_records: tti.max_records.min(files.budget.max_units),
        };
        let frame = resource_objects::build_context(
            encoded,
            position.target(),
            files.budget.max_units,
            budget,
            deadline,
        )?;
        let bytes = write_teletext_tti(&frame.document, budget)?;
        let object_id = position.target().object_id().get();
        let name = format!(
            "{}-object-{object_id:05}-context.tti",
            resource_name.trim_end_matches(".dvb")
        );
        assets.push(TeletextExportAssetMapping {
            name: name.clone(),
            scope: TeletextExportAssetScope::ObjectContextTtiV1 {
                resource_name: resource_name.to_owned(),
                object_id,
                editor_page: 0x100,
                editor_ordinal: 0,
            },
            unit_count: 0,
            bytes: bytes.len(),
            sha256: sha256(&bytes),
        });
        files.append(name, bytes)?;
    }
    Ok(())
}

// The current native DRCS model authors monochrome mode 0, with F for sparse
// undefined slots. Pinned QTM addresses attached tables by list order, ignoring
// SC and PN suffixes. A one-page companion can faithfully address logical S1=0;
// other subtables retain their protected DVB assets until set mapping is proven.
fn drcs_companion_eligible(
    page: &EncodedTeletextAuthoredPage,
    carrier: TeletextEditorCarrier,
) -> bool {
    carrier == TeletextEditorCarrier::V3
        && page.resource_header().is_some_and(|header| {
            matches!(
                header.identity().kind(),
                TeletextResourceKind::Drcs | TeletextResourceKind::Gdrcs
            ) && header.identity().logical_subpage().get() == 0
        })
}

fn append_drcs_companion(
    page: &EncodedTeletextAuthoredPage,
    resource_name: &str,
    files: &mut Files,
    tti_budget: TeletextTtiBudget,
) -> Result<TeletextExportAssetMapping> {
    let projection = project_encoded_teletext_drcs(page, files.budget.max_units)?;
    let identity = projection.resource_header.identity();
    let raw_page = (u16::from(identity.page().magazine()) << 8) | u16::from(identity.page().page());
    let mut records = vec![
        TeletextTtiRecord::PageNumber {
            page: raw_page,
            editor_ordinal: 0,
        },
        TeletextTtiRecord::SubpageOrdinal(0),
        // Resource C12/C13 are lifecycle flags, not a display NOS. Keep the raw
        // legacy bit permutation; the attached QTM DRCS loader ignores PS.
        TeletextTtiRecord::PageStatus(
            TeletextEditorCarrier::Legacy.status(projection.header.controls.raw(), 0),
        ),
    ];
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
    let budget = TeletextTtiBudget {
        max_bytes: tti_budget
            .max_bytes
            .min(files.budget.max_file_bytes)
            .min(files.remaining),
        max_records: tti_budget.max_records.min(files.budget.max_units),
    };
    let document = TeletextTtiDocument::checked_from_records(records, budget)?;
    let bytes = write_teletext_tti(&document, budget)?;
    let name = format!("{}.tti", resource_name.trim_end_matches(".dvb"));
    let mapping = TeletextExportAssetMapping {
        name: name.clone(),
        scope: TeletextExportAssetScope::DrcsTti {
            resource_name: resource_name.to_owned(),
            editor_ordinal: 0,
        },
        unit_count: 0,
        bytes: bytes.len(),
        sha256: sha256(&bytes),
    };
    files.append(name, bytes)?;
    Ok(mapping)
}

struct ProducerIdentity<'a> {
    contract: ersatzrs_domain::TeletextPageContract,
    addon_id: &'a str,
    binding_revision: Option<i64>,
    profile_id: i64,
    profile_revision: i64,
    pin: &'a ersatzrs_domain::TeletextInstallationSnapshot,
    effective_origin: Option<ersatzrs_app::teletext_export_project::TeletextExportEffectiveOrigin>,
}
fn producer_identity(
    original: OriginalDesignView<'_>,
    ordinal: usize,
) -> Result<ProducerIdentity<'_>> {
    let (contract, addon_id, binding_revision, profile_id, profile_revision, pin, effective_origin) =
        match original {
            OriginalDesignView::Base(v) => {
                let binding = v
                    .selection()
                    .bindings
                    .get(ordinal)
                    .ok_or(NativeTeletextExportProjectError::Association)?;
                (
                    binding.contract,
                    binding.addon_id.as_str(),
                    Some(binding.binding_revision),
                    binding.profile_id,
                    binding.profile_revision,
                    &binding.installation,
                    None,
                )
            }
            OriginalDesignView::Effective(v) => {
                use ersatzrs_app::teletext_export_project::TeletextExportEffectiveOrigin as Origin;
                use ersatzrs_domain::{
                    TeletextEffectiveBindingSnapshot as Binding, TeletextOverrideState,
                };
                let index = *v
                    .producer_bindings()
                    .get(ordinal)
                    .ok_or(NativeTeletextExportProjectError::Association)?;
                match v
                    .selection()
                    .bindings
                    .get(index)
                    .ok_or(NativeTeletextExportProjectError::Association)?
                {
                    Binding::Base(binding) => (
                        binding.contract,
                        binding.addon_id.as_str(),
                        Some(binding.binding_revision),
                        binding.profile_id,
                        binding.profile_revision,
                        &binding.installation,
                        Some(Origin::Base {
                            binding_index: index,
                        }),
                    ),
                    Binding::Scripted(published) => {
                        let TeletextOverrideState::Enabled(candidate) = &published.interval.state
                        else {
                            return Err(NativeTeletextExportProjectError::Association);
                        };
                        (
                            candidate.contract,
                            candidate.addon_id.as_str(),
                            None,
                            candidate.profile_id,
                            candidate.profile_revision,
                            &candidate.installation,
                            Some(Origin::Scripted {
                                binding_index: index,
                                playout_id: published.playout_id,
                                start: published.interval.start.to_rfc3339(),
                                finish: published.interval.finish.to_rfc3339(),
                            }),
                        )
                    }
                }
            }
        };
    Ok(ProducerIdentity {
        contract,
        addon_id,
        binding_revision,
        profile_id,
        profile_revision,
        pin,
        effective_origin,
    })
}

fn capture_producers(
    original: OriginalDesignView<'_>,
    project_budget: TeletextExportProjectBudget,
    deadline: Instant,
) -> Result<(Vec<TeletextExportProducer>, usize)> {
    let producer_count = original.producer_count();
    let mut metadata_bytes = ByteCounter {
        remaining: project_budget.max_bytes,
    };
    let mut producers = Vec::with_capacity(producer_count);
    for (ordinal, source) in original.sources().iter().enumerate() {
        check_deadline(deadline)?;
        let ProducerIdentity {
            contract,
            addon_id,
            binding_revision,
            profile_id,
            profile_revision,
            pin,
            effective_origin,
        } = producer_identity(original, ordinal)?;
        if source.contract() != contract {
            return Err(NativeTeletextExportProjectError::Association);
        }
        // Bound before cloning; source metadata was validated against the actual
        // producer identities, editable zones and effective panel geometry.
        serde_json::to_writer(&mut metadata_bytes, &source.authoring())
            .map_err(|_| NativeTeletextExportProjectError::Budget)?;
        for value in [
            addon_id,
            &pin.state_revision,
            &pin.version,
            &pin.package_sha256,
            &pin.content_sha256,
            &pin.manifest_sha256,
        ] {
            io::Write::write_all(&mut metadata_bytes, value.as_bytes())
                .map_err(|_| NativeTeletextExportProjectError::Budget)?;
        }
        producers.push(TeletextExportProducer {
            ordinal,
            addon_id: addon_id.to_owned(),
            contract: contract.schema_version(),
            binding_revision,
            effective_origin,
            profile_id,
            profile_revision,
            installation: TeletextExportInstallation {
                state_revision: pin.state_revision.clone(),
                version: pin.version.clone(),
                package_sha256: pin.package_sha256.clone(),
                content_sha256: pin.content_sha256.clone(),
                manifest_sha256: pin.manifest_sha256.clone(),
            },
            authoring: source.authoring().cloned(),
        });
    }
    Ok((producers, metadata_bytes.remaining))
}

struct DisplayOwner<'a> {
    ordinal: usize,
    source_page: &'a TeletextPageDraft,
    presentation: &'a TeletextProducerPagePresentation,
}
fn display_owner(
    original: OriginalDesignView<'_>,
    address: TeletextPageAddress,
) -> Result<DisplayOwner<'_>> {
    let mut owner = None;
    for (ordinal, producer) in original.producers().iter().enumerate() {
        if let Some(source_page) = producer
            .original()
            .semantic()
            .base()
            .pages
            .iter()
            .find(|p| p.address == address)
        {
            if owner.replace((ordinal, source_page, producer)).is_some() {
                return Err(NativeTeletextExportProjectError::Association);
            }
        }
    }
    let (ordinal, source_page, producer) =
        owner.ok_or(NativeTeletextExportProjectError::Association)?;
    let presentation = producer
        .page(address)
        .ok_or(NativeTeletextExportProjectError::Association)?;
    Ok(DisplayOwner {
        ordinal,
        source_page,
        presentation,
    })
}

/// Recover the editor charset from the trusted exact-owner presentation.
pub(super) fn editor_primary_for_page(
    original: OriginalDesignView<'_>,
    address: TeletextPageAddress,
) -> Result<u8> {
    Ok(display_owner(original, address)?
        .presentation
        .effective_defaults()
        .primary
        .get())
}

fn display_records(
    projection: &TeletextEncodedDisplayProjection,
    source_page: &TeletextPageDraft,
    effective_primary: u8,
    materialized: Option<&TeletextEncodedMagazineRecord>,
    carrier: TeletextEditorCarrier,
) -> Vec<TeletextTtiRecord> {
    let mut records = vec![
        TeletextTtiRecord::PageNumber {
            page: editor_page(projection.address.page.get()),
            editor_ordinal: 0,
        },
        TeletextTtiRecord::SubpageOrdinal(0),
        TeletextTtiRecord::PageStatus(
            carrier.status(projection.header.controls.raw(), effective_primary),
        ),
        TeletextTtiRecord::Region(u64::from(effective_primary >> 3)),
        // Editor convention only; host carousel timing is unchanged.
        TeletextTtiRecord::Cycle {
            value: 20,
            unit: TeletextTtiCycleUnit::Time,
        },
    ];
    for record in &projection.records {
        match record {
            TeletextEncodedDisplayRecord::HeaderText(text) => {
                let mut cells = [b' '; 40];
                cells[8..].copy_from_slice(text);
                records.push(TeletextTtiRecord::BaseRow { row: 0, cells });
            }
            TeletextEncodedDisplayRecord::BaseRow { row, cells } => {
                records.push(TeletextTtiRecord::BaseRow {
                    row: *row,
                    cells: *cells,
                });
            }
            TeletextEncodedDisplayRecord::EditorialLinks { .. } => {
                records.push(TeletextTtiRecord::FastLinks(fast_links(source_page)));
            }
            TeletextEncodedDisplayRecord::Triplets {
                row,
                designation,
                words,
            } => records.push(TeletextTtiRecord::Packet {
                row: *row,
                designation: *designation,
                words: *words,
            }),
        }
    }
    if let Some(magazine) = materialized {
        records.push(TeletextTtiRecord::Packet {
            row: 28,
            designation: magazine.designation,
            words: magazine.words,
        });
    }
    records
}

fn append_display(
    original: OriginalDesignView<'_>,
    display: &EncodedTeletextPage,
    magazines: &[TeletextEncodedMagazineRecord],
    files: &mut Files,
    tti_budget: TeletextTtiBudget,
) -> Result<TeletextExportPageMapping> {
    let project_budget = files.budget;
    let projection = project_encoded_teletext_display(display, project_budget.max_units)?;
    let DisplayOwner {
        ordinal,
        source_page,
        presentation,
    } = display_owner(original, projection.address)?;
    let name = format!(
        "page-{}-{:04x}.tti",
        projection.address.page.get(),
        projection.address.subcode.get()
    );
    let has_x28 = projection.records.iter().any(|r| {
        matches!(
            r,
            TeletextEncodedDisplayRecord::Triplets {
                row: 28,
                designation: 0,
                ..
            }
        )
    });
    let materialized = (!has_x28)
        .then(|| {
            magazines
                .iter()
                .find(|m| m.magazine == projection.address.page.magazine())
        })
        .flatten();
    let editor_projection = editor_presentation::projection(
        presentation,
        files.carrier,
        has_x28 || materialized.is_some(),
    )?;
    let mut records = display_records(
        &projection,
        source_page,
        presentation.effective_defaults().primary.get(),
        materialized,
        files.carrier,
    );
    let editor_presentation = if let Some((mapping, words)) = editor_projection {
        records.push(TeletextTtiRecord::Packet {
            row: 28,
            designation: 0,
            words,
        });
        Some(mapping)
    } else {
        None
    };
    let file_budget = TeletextTtiBudget {
        max_bytes: tti_budget
            .max_bytes
            .min(project_budget.max_file_bytes)
            .min(files.remaining),
        max_records: tti_budget.max_records.min(project_budget.max_units),
    };
    let tti = TeletextTtiDocument::checked_from_records(records, file_budget)?;
    let bytes = write_teletext_tti(&tti, file_budget)?;
    let mapping = TeletextExportPageMapping {
        name: name.clone(),
        page: wire_address(projection.address),
        producer_ordinal: ordinal,
        editor_page: editor_page(projection.address.page.get()),
        editor_ordinal: 0,
        language: projection.language.as_str().to_owned(),
        kind: match projection.kind {
            TeletextPageKind::FullPage => TeletextPageKindWire::FullPage,
            TeletextPageKind::Newsflash => TeletextPageKindWire::Newsflash,
            TeletextPageKind::Subtitle => TeletextPageKindWire::Subtitle,
        },
        header_controls: projection.header.controls.raw(),
        navigation: navigation(source_page),
        materialized_magazine: materialized.map(|m| m.magazine),
        editor_presentation,
        tti_bytes: bytes.len(),
        tti_sha256: sha256(&bytes),
    };
    files.append(name, bytes)?;
    Ok(mapping)
}

struct ExportFiles {
    pages: Vec<TeletextExportPageMapping>,
    assets: Vec<TeletextExportAssetMapping>,
    files: Vec<TeletextExportFile>,
}

fn append_drcs_sets(
    encoded: &EncodedTeletextAuthoredService,
    assets: &mut Vec<TeletextExportAssetMapping>,
    files: &mut Files,
    tti_budget: TeletextTtiBudget,
    deadline: Instant,
) -> Result<()> {
    if matches!(
        files.carrier,
        TeletextEditorCarrier::V4
            | TeletextEditorCarrier::V5
            | TeletextEditorCarrier::V6
            | TeletextEditorCarrier::V7
            | TeletextEditorCarrier::V8
    ) {
        drcs_sets::append_sets(encoded, assets, files, tti_budget, deadline)?;
    }
    Ok(())
}

fn append_object_views(
    encoded: &EncodedTeletextAuthoredService,
    page: &EncodedTeletextAuthoredPage,
    resource_name: &str,
    files: &mut Files,
    assets: &mut Vec<TeletextExportAssetMapping>,
    tti: TeletextTtiBudget,
    deadline: Instant,
) -> Result<()> {
    match files.carrier {
        TeletextEditorCarrier::V6 => {
            append_object_companions(page, resource_name, files, assets, tti, deadline)
        }
        TeletextEditorCarrier::V7 => {
            object_fragments::append(encoded, page, resource_name, files, assets, tti, deadline)
        }
        TeletextEditorCarrier::V8 => {
            append_object_contexts(encoded, page, resource_name, files, assets, tti, deadline)
        }
        _ => Ok(()),
    }
}

fn assemble_files(
    original: OriginalDesignView<'_>,
    inventory: ExportInventory<'_>,
    remaining: usize,
    project_budget: TeletextExportProjectBudget,
    tti_budget: TeletextTtiBudget,
    deadline: Instant,
    carrier: TeletextEditorCarrier,
) -> Result<ExportFiles> {
    let ExportInventory {
        encoded,
        wanted,
        selected,
        selected_count,
        asset_count,
    } = inventory;
    let magazines = project_encoded_teletext_magazines(encoded, project_budget.max_units)?;
    let mut files = Files {
        carrier,
        values: Vec::with_capacity(selected_count + asset_count),
        remaining,
        budget: project_budget,
    };
    let mut mappings = Vec::with_capacity(selected_count);
    let mut assets = Vec::with_capacity(asset_count);
    let mut units = 0usize;
    let mut resource_ordinal = 0usize;
    for page in encoded.pages() {
        check_deadline(deadline)?;
        if let Some(display) = display_page(page) {
            if selected && !wanted.contains(&display.address()) {
                continue;
            }
            charge_units(
                &mut units,
                display.data_units().len(),
                project_budget.max_units,
            )?;
            mappings.push(append_display(
                original, display, &magazines, &mut files, tti_budget,
            )?);
        } else if let Some(header) = page.resource_header() {
            charge_units(
                &mut units,
                page.data_units().len(),
                project_budget.max_units,
            )?;
            // Companion files must not renumber the unchanged original DVB
            // assets; retained V2 receipts rebuild their original names exactly.
            let resource_name = format!("resource-{resource_ordinal:04}.dvb");
            resource_ordinal += 1;
            assets.push(files.asset(
                resource_name.clone(),
                TeletextExportAssetScope::Resource {
                    identity: resource_identity(header.identity()),
                    physical_subcode: header.physical_subcode(),
                },
                page.data_units(),
            )?);
            append_object_views(
                encoded,
                page,
                &resource_name,
                &mut files,
                &mut assets,
                tti_budget,
                deadline,
            )?;
            if drcs_companion_eligible(page, carrier) {
                assets.push(append_drcs_companion(
                    page,
                    &resource_name,
                    &mut files,
                    tti_budget,
                )?);
            }
        }
    }
    append_drcs_sets(encoded, &mut assets, &mut files, tti_budget, deadline)?;
    for (magazine, unit) in magazines.iter().zip(encoded.magazine_units()) {
        check_deadline(deadline)?;
        charge_units(&mut units, 1, project_budget.max_units)?;
        assets.push(files.asset(
            format!("magazine-{}.dvb", magazine.magazine),
            TeletextExportAssetScope::Magazine {
                magazine: magazine.magazine,
            },
            std::slice::from_ref(unit),
        )?);
    }
    Ok(ExportFiles {
        pages: mappings,
        assets,
        files: files.values,
    })
}

/// Build a private exact-file receipt from a fully restored original.
/// A subset may omit the original entry; export never changes that entry.
/// Sources, selected settings and the live output remain unchanged.
pub(super) fn build_native_teletext_export_project<'a>(
    original: impl Into<OriginalDesignView<'a>>,
    expected_head: i64,
    selected: Option<&[TeletextPageAddress]>,
    project_budget: TeletextExportProjectBudget,
    tti_budget: TeletextTtiBudget,
    deadline: Instant,
    carrier: TeletextEditorCarrier,
) -> Result<TeletextExportReceiptBody> {
    let original = original.into();
    let inventory = selected_inventory(
        original,
        expected_head,
        selected,
        project_budget,
        tti_budget,
        deadline,
        carrier,
    )?;
    let (producers, remaining) = capture_producers(original, project_budget, deadline)?;
    let exported = assemble_files(
        original,
        inventory,
        remaining,
        project_budget,
        tti_budget,
        deadline,
        carrier,
    )?;
    let metadata = original
        .metadata()
        .ok_or(NativeTeletextExportProjectError::Association)?;
    let body = TeletextExportReceiptBody {
        schema_version: 1,
        project: TeletextExportProject {
            schema_version: u32::from(original.document_version()),
            editor_profile: carrier.profile().to_owned(),
            original: TeletextExportOriginal {
                design_id: original.design_id(),
                revision: original.revision(),
                digest: original.digest().to_owned(),
                expected_head,
                document_version: original.document_version(),
            },
            channel_id: original.channel_id(),
            entry: wire_address(metadata.entry),
            language: metadata.language.as_str().to_owned(),
            v1_region: original.v1_region().map(|region| match region {
                TeletextEncodingRegion::WesternLatin => TeletextSavedV1Region::WesternLatin,
            }),
            effective_scope: match original {
                OriginalDesignView::Base(_) => None,
                OriginalDesignView::Effective(v) => Some(
                    ersatzrs_app::teletext_export_project::TeletextExportEffectiveScope {
                        binding_count: v.selection().bindings.len(),
                        valid_until: v
                            .selection()
                            .valid_until
                            .map(|boundary| boundary.to_rfc3339()),
                    },
                ),
            },
            producers,
            pages: exported.pages,
            assets: exported.assets,
        },
        files: exported.files,
    };
    body.validate(project_budget)?;
    // JSON arrays encode raw bytes as integers. Count the entire serialized
    // representation without allocating a second private receipt body.
    serde_json::to_writer(
        &mut ByteCounter {
            remaining: project_budget.max_bytes,
        },
        &body,
    )
    .map_err(|_| NativeTeletextExportProjectError::Budget)?;
    check_deadline(deadline)?;
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn editor_page_uses_raw_bcd_instead_of_decimal_address() {
        assert_eq!(editor_page(200), 0x200);
        assert_eq!(editor_page(899), 0x899);
        assert_eq!(editor_page(123), 0x123);
    }
    #[test]
    fn editor_status_retains_every_raw_header_control_independently() {
        let destinations = [14, 0, 1, 2, 3, 4, 5, 6, 9, 8, 7];
        for (source, destination) in destinations.into_iter().enumerate() {
            assert_eq!(
                TeletextEditorCarrier::Legacy.status(1 << source, 0),
                0x8000 | (1 << destination)
            );
        }
        assert_eq!(TeletextEditorCarrier::Legacy.status(0x7ff, 0), 0xc3ff);
    }
    #[test]
    fn carrier_profiles_are_explicit_and_closed() {
        for carrier in [
            TeletextEditorCarrier::Legacy,
            TeletextEditorCarrier::V2,
            TeletextEditorCarrier::V3,
            TeletextEditorCarrier::V4,
            TeletextEditorCarrier::V5,
            TeletextEditorCarrier::V6,
            TeletextEditorCarrier::V7,
            TeletextEditorCarrier::V8,
        ] {
            assert_eq!(
                TeletextEditorCarrier::from_profile(carrier.profile()),
                Some(carrier)
            );
        }
        for profile in ["", "qteletextmaker-1.1", "qteletextmaker-1.0-carrier-v9"] {
            assert_eq!(TeletextEditorCarrier::from_profile(profile), None);
        }
    }
    #[test]
    fn v2_uses_literal_numeric_nos_vectors_without_changing_native_controls() {
        for (primary, controls, legacy, v2) in [
            (0, 0x0081, 0xc040, 0xc040),
            (1, 0x0481, 0xc0c0, 0xc240),
            (2, 0x0281, 0xc140, 0xc140),
            (3, 0x0681, 0xc1c0, 0xc340),
            (4, 0x0181, 0xc240, 0xc0c0),
            (5, 0x0581, 0xc2c0, 0xc2c0),
            (6, 0x0381, 0xc340, 0xc1c0),
            (7, 0x0781, 0xc3c0, 0xc3c0),
        ] {
            assert_eq!(
                TeletextEditorCarrier::Legacy.status(controls, primary),
                legacy
            );
            assert_eq!(TeletextEditorCarrier::V2.status(controls, primary), v2);
            assert_eq!(TeletextEditorCarrier::V6.status(controls, primary), v2);
            assert_eq!(TeletextEditorCarrier::V7.status(controls, primary), v2);
            assert_eq!(TeletextEditorCarrier::V8.status(controls, primary), v2);
        }
        // Independent native fallback NOS 1 and enhanced primary NOS 4.
        assert_eq!(TeletextEditorCarrier::Legacy.status(0x0481, 4), 0xc0c0);
        assert_eq!(TeletextEditorCarrier::V2.status(0x0481, 4), 0xc0c0);
        assert_eq!(TeletextEditorCarrier::V2.status(0x0481, 0), 0xc040);
    }
    #[test]
    fn v2_preserves_each_nonnational_control_independently() {
        for (source, destination) in [14, 0, 1, 2, 3, 4, 5, 6].into_iter().enumerate() {
            assert_eq!(
                TeletextEditorCarrier::V2.status(1 << source, 0),
                0x8000 | (1 << destination)
            );
        }
        for source in 8..11 {
            assert_eq!(TeletextEditorCarrier::V2.status(1 << source, 0), 0x8000);
        }
        assert_eq!(TeletextEditorCarrier::V2.status(0x7ff, 7), 0xc3ff);
    }
    #[test]
    fn display_carrier_keeps_re_upper_region_separate_from_primary_low_nos() {
        use ersatzrs_domain::{
            TeletextLanguage, TeletextNationalOption, TeletextPageNumber, TeletextSubcode,
        };
        use ersatzrs_ffmpeg::teletext::source_header::{
            observe_dvb_teletext_header, TeletextHeaderObservation,
        };
        // Independently fixed Hamming/header fixture: page 123, fallback NOS 1,
        // erase and serial flags. Native observation must remain raw 0x0481.
        let mut unit = [0; 46];
        unit[..14].copy_from_slice(&[
            2, 44, 0xe0, 0xe4, 0x40, 0xa8, 0x7a, 0x92, 0xa8, 0x0b, 0xa8, 0xa8, 0xa8, 0xe3,
        ]);
        let TeletextHeaderObservation::Header(header) = observe_dvb_teletext_header(&unit).unwrap()
        else {
            panic!("expected literal native header");
        };
        assert_eq!(header.controls.raw(), 0x0481);
        let address = TeletextPageAddress {
            page: TeletextPageNumber::new(123).unwrap(),
            subcode: TeletextSubcode::new(0).unwrap(),
        };
        let language = TeletextLanguage::new("eng".into()).unwrap();
        let page = TeletextPageDraft {
            address,
            kind: TeletextPageKind::FullPage,
            language: language.clone(),
            national_option: TeletextNationalOption::new(1).unwrap(),
            rows: vec![],
            navigation: vec![],
        };
        let projection = TeletextEncodedDisplayProjection {
            address,
            kind: page.kind,
            language,
            header,
            records: vec![TeletextEncodedDisplayRecord::HeaderText([b' '; 32])],
        };
        for (primary, status, region) in [
            (1, 0xc240, 0),
            (9, 0xc240, 1),
            (17, 0xc240, 2),
            (12, 0xc0c0, 1),
        ] {
            let records =
                display_records(&projection, &page, primary, None, TeletextEditorCarrier::V2);
            assert!(records.contains(&TeletextTtiRecord::PageStatus(status)));
            assert!(records.contains(&TeletextTtiRecord::Region(region)));
            assert_eq!(projection.header.controls.raw(), 0x0481);
        }
        let records = display_records(&projection, &page, 9, None, TeletextEditorCarrier::Legacy);
        assert!(records.contains(&TeletextTtiRecord::PageStatus(0xc0c0)));
        assert!(records.contains(&TeletextTtiRecord::Region(1)));
    }
    #[test]
    fn streaming_counter_rejects_growth_without_allocating_a_body() {
        let mut counter = ByteCounter { remaining: 3 };
        assert!(io::Write::write_all(&mut counter, b"abc").is_ok());
        assert!(io::Write::write_all(&mut counter, b"d").is_err());
        assert_eq!(counter.remaining, 0);
    }

    fn object_page(kind: TeletextResourceKind) -> EncodedTeletextAuthoredPage {
        use ersatzrs_domain::{
            TeletextColourReference, TeletextColumnOperation, TeletextEnhancementCommand,
            TeletextEnhancementObjectCatalog, TeletextEnhancementObjectDefinition,
            TeletextEnhancementProgram, TeletextObjectCatalogBudget, TeletextObjectId,
            TeletextObjectIdentity, TeletextObjectType, TeletextRawDataPageNumber,
            TeletextResourceSubpageIndex, TeletextResourceUpdateCounter,
        };
        use ersatzrs_ffmpeg::teletext::{
            allocate_teletext_resource_objects, encode_teletext_object_page,
            TeletextObjectPageTransmission, TeletextResourceFullUpdate,
            TeletextResourceTransmissionMode,
        };
        let resource = TeletextResourceIdentity::new(
            kind,
            TeletextRawDataPageNumber::new(2, 0xa0).unwrap(),
            TeletextResourceSubpageIndex::new(15).unwrap(),
        )
        .unwrap();
        let identity = if kind == TeletextResourceKind::Pop {
            TeletextObjectIdentity::public(resource, TeletextObjectId::new(19))
        } else {
            TeletextObjectIdentity::global(resource, TeletextObjectId::new(19))
        }
        .unwrap();
        let catalog = TeletextEnhancementObjectCatalog::new(
            vec![TeletextEnhancementObjectDefinition {
                identity,
                object_type: TeletextObjectType::Passive,
                program: TeletextEnhancementProgram::new(vec![
                    TeletextEnhancementCommand::AtColumn {
                        column: 0,
                        operation: TeletextColumnOperation::Foreground {
                            colour: TeletextColourReference::new(2).unwrap(),
                        },
                    },
                ])
                .unwrap(),
            }],
            TeletextObjectCatalogBudget {
                max_objects: 4,
                max_commands: 16,
            },
        )
        .unwrap();
        let allocations = allocate_teletext_resource_objects(&catalog, &[], 4).unwrap();
        EncodedTeletextAuthoredPage::Object(
            encode_teletext_object_page(
                resource,
                &catalog,
                &allocations,
                TeletextObjectPageTransmission {
                    counter: TeletextResourceUpdateCounter::new(1).unwrap(),
                    update: TeletextResourceFullUpdate::Changed,
                    mode: TeletextResourceTransmissionMode::Serial,
                    last_subpage: true,
                },
                16,
            )
            .unwrap(),
        )
    }

    fn object_files() -> Files {
        Files {
            carrier: TeletextEditorCarrier::V6,
            values: vec![],
            remaining: 65_536,
            budget: TeletextExportProjectBudget {
                max_bytes: 65_536,
                max_files: 8,
                max_producers: 2,
                max_file_bytes: 32_768,
                max_units: 128,
            },
        }
    }

    #[test]
    fn v6_object_companions_preserve_raw_resources_and_seal_exact_semantic_identity() {
        for kind in [TeletextResourceKind::Pop, TeletextResourceKind::Gpop] {
            let page = object_page(kind);
            let raw = page
                .data_units()
                .iter()
                .flat_map(|unit| unit.iter().copied())
                .collect::<Vec<_>>();
            let mut files = object_files();
            let name = "resource-0000.dvb";
            let header = page.resource_header().unwrap();
            let mut assets = vec![files
                .asset(
                    name.to_owned(),
                    TeletextExportAssetScope::Resource {
                        identity: resource_identity(header.identity()),
                        physical_subcode: header.physical_subcode(),
                    },
                    page.data_units(),
                )
                .unwrap()];
            append_object_companions(
                &page,
                name,
                &mut files,
                &mut assets,
                TeletextTtiBudget {
                    max_bytes: 32_768,
                    max_records: 32,
                },
                Instant::now() + std::time::Duration::from_secs(30),
            )
            .unwrap();
            assert_eq!(files.values.len(), 2);
            assert_eq!(files.values[0].bytes, raw);
            let companion = &assets[1];
            assert_eq!(companion.name, "resource-0000-object-00019.tti");
            assert!(
                matches!(&companion.scope,TeletextExportAssetScope::ObjectTtiV1 {
                resource_name,object_id:19,editor_page:0x100,editor_ordinal:0} if resource_name==name)
            );
            assert_eq!(companion.unit_count, 0);
            assert_eq!(companion.bytes, files.values[1].bytes.len());
            assert_eq!(companion.sha256, sha256(&files.values[1].bytes));
        }
    }

    #[test]
    fn v6_object_companions_respect_file_admission_before_building_or_appending() {
        let page = object_page(TeletextResourceKind::Pop);
        let mut files = object_files();
        files.budget.max_files = 0;
        let mut assets = vec![];
        assert!(matches!(
            append_object_companions(
                &page,
                "resource-0000.dvb",
                &mut files,
                &mut assets,
                TeletextTtiBudget {
                    max_bytes: 32_768,
                    max_records: 32
                },
                Instant::now() + std::time::Duration::from_secs(30)
            ),
            Err(NativeTeletextExportProjectError::Budget)
        ));
        assert!(assets.is_empty());
        assert!(files.values.is_empty());
        assert_eq!(files.remaining, 65_536);
    }
}
