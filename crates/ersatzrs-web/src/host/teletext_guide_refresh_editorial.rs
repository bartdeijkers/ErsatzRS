//! Historical editorial intent rebased onto fresh guide originals.
//! This creates neither manual revisions/export receipts nor current authority.

use super::{
    teletext_channel_documents::TeletextChannelDocumentEncodingPolicy,
    teletext_editorial_designs::{
        qualify_refresh_editorial_seed, read_editorial_baseline, NativeTeletextEditorialImportError,
    },
    teletext_editorial_effects::{
        validate_teletext_editorial_transition_with_operands, TeletextEditorialEdits,
    },
    teletext_stored_designs::StoredDesignParts,
};
use ersatzrs_addon_contract::{
    TeletextAuthoringPageWire, TeletextAuthoringRegionAreaWire, TeletextAuthoringRegionKindWire,
};
use ersatzrs_app::{
    teletext_design_documents::{
        TeletextDesignDocumentBudget, TeletextSavedDocumentSource, TeletextSavedEditorialOverlay,
    },
    teletext_export_project::TeletextExportProject,
    ApplicationServices,
};
use ersatzrs_core::{
    locking::TeletextWorkHold,
    repositories::{
        TeletextGuideRefreshRole, TeletextGuideRefreshSeed,
        TELETEXT_EFFECTIVE_GUIDE_GENERATION_ARTIFACT_VERSION,
        TELETEXT_GUIDE_GENERATION_ARTIFACT_VERSION,
    },
};
use ersatzrs_domain::{
    TeletextBaseTextEdit, TeletextColumnOperation, TeletextEnhancementCommand,
    TeletextEnhancementProgram, TeletextG3Compatibility, TeletextLanguage,
    TeletextLocalProgramOperandOverlay, TeletextMainProgramPresenceOverlay, TeletextNationalOption,
    TeletextPageAddress, TeletextPageCharsetDesignation, TeletextPageKind,
    TeletextPhysicalBaseOverlay, TeletextPhysicalBaseRow,
};
use ersatzrs_ffmpeg::teletext::{
    project_encoded_teletext_display, EncodedTeletextAuthoredPage, EncodedTeletextAuthoredService,
    TeletextAuthoredServiceEncodingBudget, TeletextEncodedDisplayRecord, TeletextEncodingRegion,
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GuideRefreshEditorialReview {
    Association,
    Mapping,
    ExpectedValue,
    ProtectedRegion,
    UnsupportedEnhancedIntent,
    UnsupportedBaseContext,
}
#[derive(Debug, thiserror::Error)]
pub(crate) enum GuideRefreshEditorialError {
    #[error("guide refresh editorial intent needs review: {0:?}")]
    NeedsReview(GuideRefreshEditorialReview),
    #[error("guide refresh editorial operation exceeded its deadline")]
    TimedOut,
    #[error("guide refresh editorial worker failed")]
    Worker,
    #[error("guide refresh editorial storage failed")]
    Storage,
    #[error("guide refresh historical editorial authentication failed: {0}")]
    Authentication(Box<NativeTeletextEditorialImportError>),
}
type Result<T> = std::result::Result<T, GuideRefreshEditorialError>;
fn review(reason: GuideRefreshEditorialReview) -> GuideRefreshEditorialError {
    GuideRefreshEditorialError::NeedsReview(reason)
}
fn check(deadline: Instant) -> Result<()> {
    if Instant::now() >= deadline {
        Err(GuideRefreshEditorialError::TimedOut)
    } else {
        Ok(())
    }
}

#[derive(Clone, PartialEq, Eq)]
struct PageContext {
    kind: TeletextPageKind,
    language: TeletextLanguage,
    national_option: TeletextNationalOption,
    primary: TeletextPageCharsetDesignation,
    secondary: Option<TeletextPageCharsetDesignation>,
    extension_present: bool,
}
struct CellIntent {
    owner: TeletextPageAddress,
    row: u8,
    column: u8,
    expected: u8,
    replacement: u8,
}
/// Only the repository-bound normal editorial replay can construct this value.
pub(crate) struct AuthenticatedGuideRefreshIntent {
    project: TeletextExportProject,
    contexts: BTreeMap<TeletextPageAddress, PageContext>,
    presence: Vec<TeletextMainProgramPresenceOverlay>,
    region: Option<TeletextEncodingRegion>,
    cells: Vec<CellIntent>,
    base_text: Vec<TeletextBaseTextEdit>,
    operands: Vec<TeletextLocalProgramOperandOverlay>,
}
/// Fresh compiler inputs, not a receipt or a synthetic manual design revision.
pub(crate) struct GuideRefreshEditorialBaseline<'a> {
    pub encoded: &'a EncodedTeletextAuthoredService,
    pub sources: &'a [TeletextSavedDocumentSource],
    pub region: Option<TeletextEncodingRegion>,
}
pub(crate) struct RebasedGuideEditorial {
    rows: Vec<TeletextPhysicalBaseOverlay>,
    base_text: Vec<TeletextBaseTextEdit>,
    operands: Vec<TeletextLocalProgramOperandOverlay>,
    presence: Vec<TeletextMainProgramPresenceOverlay>,
    project: TeletextExportProject,
}
/// Bound to the exact successfully validated derived encoded service.
pub(crate) struct ValidatedGuideEditorial {
    encoded_sha256: [u8; 32],
}
impl ValidatedGuideEditorial {
    pub(crate) fn matches(&self, encoded: &EncodedTeletextAuthoredService) -> bool {
        self.encoded_sha256 == encoded_digest(encoded)
    }
}
fn encoded_digest(encoded: &EncodedTeletextAuthoredService) -> [u8; 32] {
    let mut digest = Sha256::new();
    for page in encoded.pages() {
        for unit in page.data_units() {
            digest.update(unit);
        }
    }
    digest.finalize().into()
}

/// Historical authentication only; current source/seed/job CAS stays with the caller.
/// An unedited seed does not consult any editor configuration or receipt.
pub(crate) async fn load_guide_refresh_editorial_intent(
    application: &ApplicationServices,
    seed: &TeletextGuideRefreshSeed,
    budget: TeletextDesignDocumentBudget,
    policy: TeletextChannelDocumentEncodingPolicy,
    deadline: Instant,
    hold: TeletextWorkHold,
) -> Result<Option<AuthenticatedGuideRefreshIntent>> {
    check(deadline)?;
    let request = &seed.generation.request;
    let stored = application
        .teletext_design_revision(
            request.design_id,
            request.revision,
            budget.max_serialized_bytes,
        )
        .await
        .map_err(|_| GuideRefreshEditorialError::Storage)?
        .ok_or_else(|| review(GuideRefreshEditorialReview::Association))?;
    let expected_version = match seed.role {
        TeletextGuideRefreshRole::Base => ersatzrs_domain::TELETEXT_DESIGN_DOCUMENT_VERSION,
        TeletextGuideRefreshRole::Effective => {
            ersatzrs_domain::TELETEXT_EFFECTIVE_DESIGN_DOCUMENT_VERSION
        }
    };
    let expected_artifact = match seed.role {
        TeletextGuideRefreshRole::Base => TELETEXT_GUIDE_GENERATION_ARTIFACT_VERSION,
        TeletextGuideRefreshRole::Effective => TELETEXT_EFFECTIVE_GUIDE_GENERATION_ARTIFACT_VERSION,
    };
    if stored.design_id != request.design_id
        || stored.revision != request.revision
        || stored.digest != request.digest
        || stored.document_version != expected_version
        || seed.generation.artifact_version != expected_artifact
    {
        return Err(review(GuideRefreshEditorialReview::Association));
    }
    let channel = request.channel_id;
    let (stored, parts) = super::teletext_work::spawn_blocking(hold.clone(), move || {
        let parts = StoredDesignParts::decode(&stored, budget)
            .map_err(|_| review(GuideRefreshEditorialReview::Association))?;
        let owner = match &parts {
            StoredDesignParts::Base(parts) => parts.selection.channel_id,
            StoredDesignParts::Effective(parts) => parts.selection.channel_id,
        };
        if owner != channel {
            return Err(review(GuideRefreshEditorialReview::Association));
        }
        Ok::<_, GuideRefreshEditorialError>((stored, parts))
    })
    .await
    .map_err(|_| GuideRefreshEditorialError::Worker)??;
    // Explicit receiver agreements can affect base interpretation even without
    // an editorial overlay. Until qualified rebasing exists, preserve the old
    // accepted output and require review rather than silently erase that intent.
    if !parts.secondary_default_agreements().is_empty() {
        return Err(review(GuideRefreshEditorialReview::UnsupportedBaseContext));
    }
    let Some(overlay) = parts.overlay() else {
        return Ok(None);
    };
    let overlay = overlay.clone();
    let baseline = read_editorial_baseline(
        application,
        overlay.provenance(),
        stored.design_id,
        Some(stored.revision),
        budget,
        &policy,
        deadline,
    )
    .await
    .map_err(|error| GuideRefreshEditorialError::Authentication(Box::new(error)))?;
    super::teletext_work::spawn_blocking(hold, move || {
        check(deadline)?;
        let maximum = policy.encoding.max_units;
        let (original, edited, project) =
            qualify_refresh_editorial_seed(stored, parts, baseline, budget, policy, deadline)
                .map_err(|error| GuideRefreshEditorialError::Authentication(Box::new(error)))?;
        let root = original.view();
        let changed = edited.view();
        let original = root
            .encoded()
            .ok_or_else(|| review(GuideRefreshEditorialReview::Association))?;
        let edited = changed
            .encoded()
            .ok_or_else(|| review(GuideRefreshEditorialReview::Association))?;
        extract(
            &overlay,
            original,
            edited,
            project,
            root.v1_region(),
            maximum,
            deadline,
        )
        .map(Some)
    })
    .await
    .map_err(|_| GuideRefreshEditorialError::Worker)?
}

fn base_operand(operation: TeletextColumnOperation) -> bool {
    matches!(
        operation,
        TeletextColumnOperation::G0 { .. }
            | TeletextColumnOperation::G1 { .. }
            | TeletextColumnOperation::G2 { .. }
            | TeletextColumnOperation::G0Diacritic { .. }
            | TeletextColumnOperation::DesignateCharset { .. }
            | TeletextColumnOperation::G3 {
                compatibility: TeletextG3Compatibility::Level15,
                ..
            }
    )
}
fn base_program(program: &TeletextEnhancementProgram) -> bool {
    program.commands().iter().all(|command| match command {
        TeletextEnhancementCommand::SetPosition { .. } => true,
        TeletextEnhancementCommand::AtColumn { operation, .. } => base_operand(*operation),
        _ => false,
    })
}
fn extract(
    overlay: &TeletextSavedEditorialOverlay,
    original: &EncodedTeletextAuthoredService,
    edited: &EncodedTeletextAuthoredService,
    project: TeletextExportProject,
    region: Option<TeletextEncodingRegion>,
    maximum: usize,
    deadline: Instant,
) -> Result<AuthenticatedGuideRefreshIntent> {
    if !overlay.local_object_operands().is_empty()
        || !overlay.local_object_glyph_program_overlays().is_empty()
        || !overlay.main_local_invocation_program_overlays().is_empty()
        || overlay
            .main_program_presence_overlays()
            .iter()
            .any(|change| {
                change
                    .expected()
                    .is_some_and(|program| !base_program(program))
                    || change
                        .replacement()
                        .is_some_and(|program| !base_program(program))
            })
        || !overlay.main_resource_invocations().is_empty()
        || !overlay.resource_object_operands().is_empty()
        || !overlay.resource_object_program_overlays().is_empty()
        || !overlay.resource_object_definition_additions().is_empty()
        || !overlay.object_resource_additions().is_empty()
        || !overlay.object_subpage_announcements().is_empty()
        || !overlay.object_page_selections().is_empty()
        || !overlay.drcs_bitmap_overlays().is_empty()
        || overlay.local_program_operands().iter().any(|operand| {
            !base_operand(operand.expected()) || !base_operand(operand.replacement())
        })
    {
        return Err(review(
            GuideRefreshEditorialReview::UnsupportedEnhancedIntent,
        ));
    }
    let contexts = original
        .declared()
        .semantic()
        .base()
        .pages
        .iter()
        .map(|page| page_context(original, page.address).map(|context| (page.address, context)))
        .collect::<Result<BTreeMap<_, _>>>()?;
    let mut cells = Vec::new();
    for page in overlay.pages() {
        check(deadline)?;
        let before = rows(original, page.owner(), maximum)?;
        let after = rows(edited, page.owner(), maximum)?;
        for row in page.rows() {
            let old = before.get(&row.row()).copied().unwrap_or([b' '; 40]);
            let new = after.get(&row.row()).copied().unwrap_or([b' '; 40]);
            for column in 0_u8..40 {
                if old[usize::from(column)] != new[usize::from(column)] {
                    if overlay.base_text_edits().iter().any(|edit| {
                        edit.owner() == page.owner()
                            && edit.position().row() == row.row()
                            && edit.position().column() == column
                    }) {
                        continue;
                    }
                    permitted(&project, page.owner(), row.row(), column)?;
                    cells.push(CellIntent {
                        owner: page.owner(),
                        row: row.row(),
                        column,
                        expected: old[usize::from(column)],
                        replacement: new[usize::from(column)],
                    });
                }
            }
        }
    }
    for edit in overlay.base_text_edits() {
        permitted(
            &project,
            edit.owner(),
            edit.position().row(),
            edit.position().column(),
        )?;
    }
    Ok(AuthenticatedGuideRefreshIntent {
        project,
        contexts,
        presence: overlay.main_program_presence_overlays().to_vec(),
        region,
        cells,
        base_text: overlay.base_text_edits().to_vec(),
        operands: overlay.local_program_operands().to_vec(),
    })
}
fn rows(
    service: &EncodedTeletextAuthoredService,
    owner: TeletextPageAddress,
    maximum: usize,
) -> Result<BTreeMap<u8, [u8; 40]>> {
    let page = service
        .pages()
        .iter()
        .find_map(|page| {
            let page = match page {
                EncodedTeletextAuthoredPage::NativeDisplay(page) => page.encoded().page(),
                EncodedTeletextAuthoredPage::CompiledDisplay(page) => page.page(),
                _ => return None,
            };
            (page.address() == owner).then_some(page)
        })
        .ok_or_else(|| review(GuideRefreshEditorialReview::Mapping))?;
    let projection = project_encoded_teletext_display(page, maximum)
        .map_err(|_| review(GuideRefreshEditorialReview::Association))?;
    let mut rows = BTreeMap::new();
    for record in projection.records {
        match record {
            TeletextEncodedDisplayRecord::HeaderText(header) => {
                let mut cells = [b' '; 40];
                cells[8..].copy_from_slice(&header);
                rows.insert(0, cells);
            }
            TeletextEncodedDisplayRecord::BaseRow { row, cells } => {
                rows.insert(row, cells);
            }
            _ => {}
        }
    }
    Ok(rows)
}
fn page_context(
    service: &EncodedTeletextAuthoredService,
    owner: TeletextPageAddress,
) -> Result<PageContext> {
    let page = service
        .declared()
        .semantic()
        .base()
        .pages
        .iter()
        .find(|page| page.address == owner)
        .ok_or_else(|| review(GuideRefreshEditorialReview::Mapping))?;
    let composition = service
        .pages()
        .iter()
        .find_map(|encoded| match encoded {
            EncodedTeletextAuthoredPage::NativeDisplay(encoded)
                if encoded.encoded().page().address() == owner =>
            {
                Some(encoded.composition())
            }
            EncodedTeletextAuthoredPage::CompiledDisplay(encoded)
                if encoded.page().address() == owner =>
            {
                Some(encoded.normalized().original())
            }
            _ => None,
        })
        .ok_or_else(|| review(GuideRefreshEditorialReview::Mapping))?;
    let charset = composition.charset_execution();
    Ok(PageContext {
        kind: page.kind,
        language: page.language.clone(),
        national_option: page.national_option,
        primary: charset.primary(),
        secondary: charset.secondary(),
        extension_present: charset.extension_present(),
    })
}
fn metadata(
    project: &TeletextExportProject,
    owner: TeletextPageAddress,
) -> Result<&TeletextAuthoringPageWire> {
    let page = project
        .pages
        .iter()
        .find(|page| page.page.page == owner.page.get() && page.page.subcode == owner.subcode.get())
        .ok_or_else(|| review(GuideRefreshEditorialReview::Mapping))?;
    project
        .producers
        .iter()
        .find(|producer| producer.ordinal == page.producer_ordinal)
        .and_then(|producer| producer.authoring.as_ref())
        .and_then(|authoring| {
            authoring.pages.iter().find(|page| {
                page.page.page == owner.page.get() && page.page.subcode == owner.subcode.get()
            })
        })
        .ok_or_else(|| review(GuideRefreshEditorialReview::Mapping))
}
fn permitted(
    project: &TeletextExportProject,
    owner: TeletextPageAddress,
    row: u8,
    column: u8,
) -> Result<()> {
    if metadata(project, owner)?.regions.iter().any(|region| {
        region.kind != TeletextAuthoringRegionKindWire::Generated
            && region.area == TeletextAuthoringRegionAreaWire::Main
            && row >= region.row
            && u16::from(row) < u16::from(region.row) + u16::from(region.height)
            && column >= region.column
            && u16::from(column) < u16::from(region.column) + u16::from(region.width)
    }) {
        Ok(())
    } else {
        Err(review(GuideRefreshEditorialReview::ProtectedRegion))
    }
}
impl AuthenticatedGuideRefreshIntent {
    fn fresh_project(
        &self,
        fresh: &GuideRefreshEditorialBaseline<'_>,
    ) -> Result<TeletextExportProject> {
        let mut project = self.project.clone();
        for (producer, source) in project.producers.iter_mut().zip(fresh.sources) {
            if producer.contract != source.contract().schema_version() {
                return Err(review(GuideRefreshEditorialReview::Mapping));
            }
            match (producer.authoring.as_ref(), source.authoring()) {
                (Some(previous), Some(current))
                    if previous.template == current.template && previous.theme == current.theme =>
                {
                    producer.authoring = Some(current.clone());
                }
                (None, None) => {}
                _ => return Err(review(GuideRefreshEditorialReview::Mapping)),
            }
        }
        Ok(project)
    }

    /// Only actual edits are copied. Untouched generated bytes always come from
    /// the fresh compiler, even when a prior edit used a complete row overlay.
    pub(crate) fn rebase(
        &self,
        fresh: &GuideRefreshEditorialBaseline<'_>,
        maximum_units: usize,
        deadline: Instant,
    ) -> Result<RebasedGuideEditorial> {
        check(deadline)?;
        if fresh.region != self.region || fresh.sources.len() != self.project.producers.len() {
            return Err(review(GuideRefreshEditorialReview::Mapping));
        }
        let project = self.fresh_project(fresh)?;
        let owners: BTreeSet<_> = self
            .cells
            .iter()
            .map(|cell| cell.owner)
            .chain(self.base_text.iter().map(|edit| edit.owner()))
            .chain(self.operands.iter().map(|operand| operand.owner()))
            .chain(
                self.presence
                    .iter()
                    .map(TeletextMainProgramPresenceOverlay::owner),
            )
            .collect();
        for owner in owners {
            check(deadline)?;
            if metadata(&self.project, owner)? != metadata(&project, owner)? {
                return Err(review(GuideRefreshEditorialReview::Mapping));
            }
            let old = self
                .project
                .pages
                .iter()
                .find(|page| {
                    page.page.page == owner.page.get() && page.page.subcode == owner.subcode.get()
                })
                .ok_or_else(|| review(GuideRefreshEditorialReview::Mapping))?;
            let page = fresh
                .encoded
                .declared()
                .semantic()
                .base()
                .pages
                .iter()
                .find(|page| page.address == owner)
                .ok_or_else(|| review(GuideRefreshEditorialReview::Mapping))?;
            let context = page_context(fresh.encoded, owner)?;
            if self.contexts.get(&owner) != Some(&context) || old.language != page.language.as_str()
            {
                return Err(review(GuideRefreshEditorialReview::Mapping));
            }
        }
        let mut changed: BTreeMap<TeletextPageAddress, BTreeMap<u8, [u8; 40]>> = BTreeMap::new();
        for cell in &self.cells {
            check(deadline)?;
            permitted(&project, cell.owner, cell.row, cell.column)?;
            let page_rows = match changed.entry(cell.owner) {
                std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(rows(fresh.encoded, cell.owner, maximum_units)?)
                }
            };
            let row = page_rows.entry(cell.row).or_insert([b' '; 40]);
            if row[usize::from(cell.column)] != cell.expected {
                return Err(review(GuideRefreshEditorialReview::ExpectedValue));
            }
            row[usize::from(cell.column)] = cell.replacement;
        }
        let mut output = Vec::new();
        for (owner, page_rows) in changed {
            let touched: BTreeSet<_> = self
                .cells
                .iter()
                .filter(|cell| cell.owner == owner)
                .map(|cell| cell.row)
                .collect();
            let rows = page_rows
                .into_iter()
                .filter(|(row, _)| touched.contains(row))
                .map(|(row, cells)| {
                    TeletextPhysicalBaseRow::new(row, cells)
                        .map_err(|_| review(GuideRefreshEditorialReview::Association))
                })
                .collect::<Result<Vec<_>>>()?;
            output.push(
                TeletextPhysicalBaseOverlay::new(owner, rows)
                    .map_err(|_| review(GuideRefreshEditorialReview::Association))?,
            );
        }
        Ok(RebasedGuideEditorial {
            rows: output,
            base_text: self.base_text.clone(),
            operands: self.operands.clone(),
            presence: self.presence.clone(),
            project,
        })
    }
}
impl RebasedGuideEditorial {
    pub(crate) fn rows(&self) -> &[TeletextPhysicalBaseOverlay] {
        &self.rows
    }
    pub(crate) fn base_text_edits(&self) -> &[TeletextBaseTextEdit] {
        &self.base_text
    }
    pub(crate) fn local_program_operands(&self) -> &[TeletextLocalProgramOperandOverlay] {
        &self.operands
    }
    pub(crate) fn main_program_presence_overlays(&self) -> &[TeletextMainProgramPresenceOverlay] {
        &self.presence
    }
    pub(crate) fn validate(
        &self,
        fresh: &EncodedTeletextAuthoredService,
        edited: &EncodedTeletextAuthoredService,
        budget: &TeletextAuthoredServiceEncodingBudget,
        deadline: Instant,
    ) -> Result<ValidatedGuideEditorial> {
        check(deadline)?;
        let changes = TeletextEditorialEdits {
            rows: &self.rows,
            base_text_edits: &self.base_text,
            operands: &self.operands,
            main_presence: &self.presence,
            ..Default::default()
        };
        validate_teletext_editorial_transition_with_operands(
            fresh,
            edited,
            &self.project,
            changes,
            changes,
            budget,
            deadline,
        )
        .map_err(|_| review(GuideRefreshEditorialReview::ProtectedRegion))?;
        Ok(ValidatedGuideEditorial {
            encoded_sha256: encoded_digest(edited),
        })
    }
}

#[cfg(test)]
#[path = "teletext_guide_refresh_editorial/tests.rs"]
mod tests;
