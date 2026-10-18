//! Readonly owner-scoped declarations from the exact restored producer source.

use super::{
    check_deadline, failure, RestoredOriginalTeletextDesign, TeletextAuthoringError,
    TeletextAuthoringStatus,
};
use crate::host::teletext_effective_designs::RestoredEffectiveOriginalDesign;
use ersatzrs_addon_contract::{
    TeletextAuthoringDynamicFieldWire, TeletextAuthoringMetadataWire, TeletextAuthoringPageWire,
    TeletextAuthoringRegionWire, TeletextAuthoringTemplateWire, TeletextAuthoringThemeWire,
};
use ersatzrs_app::teletext_design_documents::TeletextSavedDocumentSource;
use ersatzrs_domain::{TeletextPageAddress, TeletextProducerPresentation};
use std::{fmt, time::Instant};

/// Descriptive original boundaries; these values never authorize an edit.
#[derive(Clone, PartialEq, Eq)]
pub struct TeletextAuthoringPageMetadata {
    pub producer_index: usize,
    pub binding_index: usize,
    pub addon_id: String,
    pub template: TeletextAuthoringTemplateWire,
    pub theme: TeletextAuthoringThemeWire,
    pub regions: Vec<TeletextAuthoringRegionWire>,
    pub dynamic_fields: Vec<TeletextAuthoringDynamicFieldWire>,
}

impl fmt::Debug for TeletextAuthoringPageMetadata {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TeletextAuthoringPageMetadata")
            .field("producer_index", &self.producer_index)
            .field("binding_index", &self.binding_index)
            .field("regions", &self.regions.len())
            .field("dynamic_fields", &self.dynamic_fields.len())
            .finish_non_exhaustive()
    }
}

type Selected<'a> = (
    usize,
    &'a TeletextAuthoringMetadataWire,
    &'a TeletextAuthoringPageWire,
);

fn invalid() -> TeletextAuthoringError {
    failure(TeletextAuthoringStatus::PreviewFailed)
}

fn selected<'a>(
    sources: &'a [TeletextSavedDocumentSource],
    producers: &[TeletextProducerPresentation],
    owner: Option<TeletextPageAddress>,
    deadline: Instant,
) -> Result<Option<Selected<'a>>, TeletextAuthoringError> {
    check_deadline(deadline)?;
    if sources.len() != producers.len() {
        return Err(invalid());
    }
    let Some(owner) = owner else {
        return Ok(None);
    };
    let mut found = None;
    for (index, producer) in producers.iter().enumerate() {
        check_deadline(deadline)?;
        if producer.page(owner).is_some() && found.replace(index).is_some() {
            return Err(invalid());
        }
    }
    let index = found.ok_or_else(invalid)?;
    let Some(metadata) = sources[index].authoring() else {
        return Ok(None);
    };
    let mut pages = metadata.pages.iter().filter(|page| {
        page.page.page == owner.page.get() && page.page.subcode == owner.subcode.get()
    });
    let (Some(page), None) = (pages.next(), pages.next()) else {
        return Err(invalid());
    };
    check_deadline(deadline)?;
    Ok(Some((index, metadata, page)))
}

fn observation(
    selected: Selected<'_>,
    binding_index: usize,
    addon_id: &str,
) -> TeletextAuthoringPageMetadata {
    let (producer_index, metadata, page) = selected;
    TeletextAuthoringPageMetadata {
        producer_index,
        binding_index,
        addon_id: addon_id.to_owned(),
        template: metadata.template.clone(),
        theme: metadata.theme.clone(),
        regions: page.regions.clone(),
        dynamic_fields: page.dynamic_fields.clone(),
    }
}

pub(super) fn base(
    restored: &RestoredOriginalTeletextDesign,
    owner: TeletextPageAddress,
    deadline: Instant,
) -> Result<Vec<TeletextAuthoringPageMetadata>, TeletextAuthoringError> {
    if restored.sources().len() != restored.selection().bindings.len() {
        return Err(invalid());
    }
    let Some(selected) = selected(
        restored.sources(),
        restored.producers(),
        Some(owner),
        deadline,
    )?
    else {
        return Ok(Vec::new());
    };
    let binding = restored
        .selection()
        .bindings
        .get(selected.0)
        .ok_or_else(invalid)?;
    Ok(vec![observation(selected, selected.0, &binding.addon_id)])
}

pub(super) fn effective(
    restored: &RestoredEffectiveOriginalDesign,
    owner: Option<TeletextPageAddress>,
    deadline: Instant,
) -> Result<Vec<TeletextAuthoringPageMetadata>, TeletextAuthoringError> {
    if restored.sources().len() != restored.producer_bindings().len() {
        return Err(invalid());
    }
    let Some(selected) = selected(restored.sources(), restored.producers(), owner, deadline)?
    else {
        return Ok(Vec::new());
    };
    let binding_index = restored.producer_bindings()[selected.0];
    let binding = restored
        .selection()
        .bindings
        .get(binding_index)
        .ok_or_else(invalid)?;
    if !binding.enabled() {
        return Err(invalid());
    }
    Ok(vec![observation(
        selected,
        binding_index,
        binding.addon_id(),
    )])
}
