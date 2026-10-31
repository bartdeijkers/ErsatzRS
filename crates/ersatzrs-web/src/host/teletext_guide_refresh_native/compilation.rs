//! Shared native encoding over fresh baseline bytes and authenticated seed intent.

use std::time::Instant;

use chrono::{DateTime, Utc};
use ersatzrs_app::teletext_design_documents::{
    decode_teletext_design_documents, decode_teletext_effective_design_documents,
    TeletextDesignDocumentBudget, TeletextSavedDocumentSource, TeletextSavedGuideInputs,
    TeletextSavedResourceTransmission, TeletextSavedV1Region,
};
use ersatzrs_app::teletext_profile_documents::TeletextProfileDocument;
use ersatzrs_core::repositories::{
    TeletextGuideRefreshRole, TeletextGuideRefreshSeed, TeletextGuideRefreshSelection,
};
use ersatzrs_domain::{
    TeletextPageAddress, TELETEXT_DESIGN_DOCUMENT_VERSION,
    TELETEXT_EFFECTIVE_DESIGN_DOCUMENT_VERSION,
};
use ersatzrs_ffmpeg::teletext::{EncodedTeletextAuthoredService, TeletextEncodingRegion};

use super::{FreshGuideBaseline, GuideRefreshNativeError as Error};
use crate::host::{
    teletext_channel_documents::{
        encode_original_producer_documents_with_editorial_resources, EditorialResourceDocuments,
        TeletextChannelDocumentEncodingPolicy,
    },
    teletext_guide_refresh_editorial::{
        AuthenticatedGuideRefreshIntent, GuideRefreshEditorialBaseline, RebasedGuideEditorial,
    },
};

pub(super) struct DecodedBaseline {
    selection: TeletextGuideRefreshSelection,
    entry_producer: Option<usize>,
    entry: Option<TeletextPageAddress>,
    documents: Option<Vec<TeletextProfileDocument>>,
    stored: Vec<u8>,
    document_version: u16,
    budget: TeletextDesignDocumentBudget,
    sources: Vec<TeletextSavedDocumentSource>,
    region: Option<TeletextEncodingRegion>,
    transmissions: Vec<TeletextSavedResourceTransmission>,
    guide_inputs: Option<TeletextSavedGuideInputs>,
}
impl DecodedBaseline {
    pub(super) fn guide_inputs(&self) -> Option<&TeletextSavedGuideInputs> {
        self.guide_inputs.as_ref()
    }
    pub(super) const fn scope_expiry(&self) -> Option<DateTime<Utc>> {
        match &self.selection {
            TeletextGuideRefreshSelection::Base(_) => None,
            TeletextGuideRefreshSelection::Effective(scope) => scope.valid_until,
        }
    }
}

pub(super) fn selection(
    bytes: &[u8],
    version: u16,
    budget: TeletextDesignDocumentBudget,
) -> Result<TeletextGuideRefreshSelection, Error> {
    match version {
        TELETEXT_DESIGN_DOCUMENT_VERSION => Ok(TeletextGuideRefreshSelection::Base(
            decode_teletext_design_documents(bytes, budget)
                .map_err(|_| Error::NeedsReview)?
                .into_parts()
                .selection,
        )),
        TELETEXT_EFFECTIVE_DESIGN_DOCUMENT_VERSION => Ok(TeletextGuideRefreshSelection::Effective(
            decode_teletext_effective_design_documents(bytes, budget)
                .map_err(|_| Error::NeedsReview)?
                .into_parts()
                .selection,
        )),
        _ => Err(Error::NeedsReview),
    }
}

pub(super) fn decode(
    seed: &TeletextGuideRefreshSeed,
    baseline: &FreshGuideBaseline,
    budget: TeletextDesignDocumentBudget,
) -> Result<DecodedBaseline, Error> {
    if baseline.qualification.source.channel_ids != seed.qualification.source.channel_ids {
        return Err(Error::NeedsReview);
    }
    let decoded = match seed.role {
        TeletextGuideRefreshRole::Base => {
            if baseline.document_version != TELETEXT_DESIGN_DOCUMENT_VERSION {
                return Err(Error::NeedsReview);
            }
            let parts = decode_teletext_design_documents(&baseline.document, budget)
                .map_err(|_| Error::NeedsReview)?
                .into_parts();
            if parts.guide_source.as_ref() != Some(&baseline.qualification.source)
                || parts.editorial_overlay.is_some()
                || !parts.secondary_default_agreements.is_empty()
            {
                return Err(Error::NeedsReview);
            }
            let entry_producer = parts.entry_addon_id.as_deref().and_then(|addon| {
                parts
                    .selection
                    .bindings
                    .iter()
                    .position(|binding| binding.addon_id == addon)
            });
            DecodedBaseline {
                selection: TeletextGuideRefreshSelection::Base(parts.selection),
                entry_producer,
                entry: parts.entry,
                documents: Some(parts.documents),
                stored: baseline.document.clone(),
                document_version: baseline.document_version,
                budget,
                sources: parts.sources,
                region: parts.v1_region.map(region),
                transmissions: parts.resource_transmissions,
                guide_inputs: parts.guide_inputs,
            }
        }
        TeletextGuideRefreshRole::Effective => {
            if baseline.document_version != TELETEXT_EFFECTIVE_DESIGN_DOCUMENT_VERSION {
                return Err(Error::NeedsReview);
            }
            let parts = decode_teletext_effective_design_documents(&baseline.document, budget)
                .map_err(|_| Error::NeedsReview)?
                .into_parts();
            if parts.guide_source.as_ref() != Some(&baseline.qualification.source)
                || parts.editorial_overlay.is_some()
                || !parts.secondary_default_agreements.is_empty()
            {
                return Err(Error::NeedsReview);
            }
            let entry_producer = parts.entry_addon_id.as_deref().and_then(|addon| {
                parts
                    .producer_bindings
                    .iter()
                    .position(|index| parts.selection.bindings[*index].addon_id() == addon)
            });
            DecodedBaseline {
                selection: TeletextGuideRefreshSelection::Effective(parts.selection),
                entry_producer,
                entry: parts.entry,
                documents: Some(parts.documents),
                stored: baseline.document.clone(),
                document_version: baseline.document_version,
                budget,
                sources: parts.sources,
                region: parts.v1_region.map(region),
                transmissions: parts.resource_transmissions,
                guide_inputs: parts.guide_inputs,
            }
        }
    };
    let channel = match &decoded.selection {
        TeletextGuideRefreshSelection::Base(value) => value.channel_id,
        TeletextGuideRefreshSelection::Effective(value) => value.channel_id,
    };
    if decoded.selection != baseline.selection
        || channel != seed.generation.request.channel_id
        || decoded.entry.is_none()
        || decoded.entry_producer.is_none()
        || decoded.guide_inputs.is_none()
    {
        return Err(Error::NeedsReview);
    }
    Ok(decoded)
}

fn region(value: TeletextSavedV1Region) -> TeletextEncodingRegion {
    match value {
        TeletextSavedV1Region::WesternLatin => TeletextEncodingRegion::WesternLatin,
    }
}

pub(super) fn compile(
    mut decoded: DecodedBaseline,
    intent: Option<&AuthenticatedGuideRefreshIntent>,
    policy: &TeletextChannelDocumentEncodingPolicy,
    deadline: Instant,
) -> Result<EncodedTeletextAuthoredService, Error> {
    Error::check_deadline(deadline)?;
    let fresh = encode(&mut decoded, None, policy.clone(), deadline)?;
    let service = if let Some(intent) = intent {
        let rebased = intent.rebase(
            &GuideRefreshEditorialBaseline {
                encoded: &fresh,
                sources: &decoded.sources,
                region: decoded.region,
            },
            policy.encoding.max_units,
            deadline,
        )?;
        let edited = encode(&mut decoded, Some(&rebased), policy.clone(), deadline)?;
        let evidence = rebased.validate(&fresh, &edited, &policy.encoding, deadline)?;
        if !evidence.matches(&edited) {
            return Err(Error::NeedsReview);
        }
        edited
    } else {
        fresh
    };
    service
        .with_resource_lifecycle_variants(deadline)
        .map_err(|_| Error::NeedsReview)
}

fn encode(
    decoded: &mut DecodedBaseline,
    intent: Option<&RebasedGuideEditorial>,
    mut policy: TeletextChannelDocumentEncodingPolicy,
    deadline: Instant,
) -> Result<EncodedTeletextAuthoredService, Error> {
    policy.entry = decoded.entry;
    policy.transmissions =
        crate::host::teletext_stored_designs::resource_transmissions(&decoded.transmissions, &[]);
    let resources = EditorialResourceDocuments {
        secondary_default_agreements: &[],
        base_text_edits: intent.map_or(&[], RebasedGuideEditorial::base_text_edits),
        main_presence: intent.map_or(&[], RebasedGuideEditorial::main_program_presence_overlays),
        main_programs: &[],
        programs: (
            intent.map_or_else(Vec::new, |intent| intent.rows().to_vec()),
            intent.map_or(&[], RebasedGuideEditorial::local_program_operands),
            &[],
            &[],
            &[],
        ),
        resource_operands: &[],
        resource_programs: &[],
        resource_additions: &[],
        object_resource_additions: &[],
        object_subpage_announcements: &[],
        object_page_selections: &[],
        main_resource_invocations: &[],
    };
    let original = encode_original_producer_documents_with_editorial_resources(
        decoded.entry_producer,
        match decoded.documents.take() {
            Some(documents) => documents,
            None => match decoded.document_version {
                TELETEXT_DESIGN_DOCUMENT_VERSION => {
                    decode_teletext_design_documents(&decoded.stored, decoded.budget)
                        .map_err(|_| Error::NeedsReview)?
                        .into_parts()
                        .documents
                }
                TELETEXT_EFFECTIVE_DESIGN_DOCUMENT_VERSION => {
                    decode_teletext_effective_design_documents(&decoded.stored, decoded.budget)
                        .map_err(|_| Error::NeedsReview)?
                        .into_parts()
                        .documents
                }
                _ => return Err(Error::NeedsReview),
            },
        },
        resources,
        deadline,
        decoded.region,
        policy,
    )
    .map_err(|_| Error::NeedsReview)?;
    Error::check_deadline(deadline)?;
    original.encoded.ok_or(Error::NeedsReview)
}
