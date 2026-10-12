//! Exact historical request bytes, without source or invocation authority.

use super::{
    failure, limit, map_only, Required, TeletextDesignDocumentBudget, TeletextDesignDocumentError,
    TeletextDesignDocumentFailure,
};
use crate::teletext_channel_documents::TeletextChannelRequestTemplates;
use ersatzrs_addon_contract::{TeletextPagesRequest, MAX_TELETEXT_PAGES_INPUT_BYTES};
use ersatzrs_core::repositories::TeletextGuideSourceFence;
use ersatzrs_domain::{TeletextChannelSelectionSnapshot, TeletextPageContract};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

const INPUT_VERSION: u16 = 1;

/// Historical guide inputs, not authenticated invocation or source receipts.
/// The optional parent distinguishes legacy absence from a captured Off state.
pub struct TeletextSavedGuideInputs {
    v1: Option<Vec<u8>>,
    v2: Option<Vec<u8>>,
}

impl TeletextSavedGuideInputs {
    /// Borrow original UTF-8 request bytes for an actually selected contract.
    #[must_use]
    pub fn request(&self, contract: TeletextPageContract) -> Option<&[u8]> {
        match contract {
            TeletextPageContract::V1 => self.v1.as_deref(),
            TeletextPageContract::V2 => self.v2.as_deref(),
        }
    }
}

impl fmt::Debug for TeletextSavedGuideInputs {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TeletextSavedGuideInputs")
            .field("v1_bytes", &self.v1.as_ref().map(Vec::len))
            .field("v2_bytes", &self.v2.as_ref().map(Vec::len))
            .finish()
    }
}

#[derive(Default)]
pub(super) struct OptionalGuideInputs(pub Option<StoredGuideInputs>);

impl<'de> Deserialize<'de> for OptionalGuideInputs {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        StoredGuideInputs::deserialize(deserializer).map(|value| Self(Some(value)))
    }
}

impl Serialize for OptionalGuideInputs {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

#[derive(Serialize)]
pub(super) struct StoredGuideInputs {
    input_version: u16,
    v1: Required<String>,
    v2: Required<String>,
}

impl<'de> Deserialize<'de> for StoredGuideInputs {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            input_version: u16,
            v1: Required<String>,
            v2: Required<String>,
        }
        let fields: Fields = map_only(deserializer)?;
        Ok(Self {
            input_version: fields.input_version,
            v1: fields.v1,
            v2: fields.v2,
        })
    }
}

impl StoredGuideInputs {
    pub(super) fn restore(
        self,
        selection: &TeletextChannelSelectionSnapshot,
        generation: &str,
        budget: TeletextDesignDocumentBudget,
        source: Option<&TeletextGuideSourceFence>,
    ) -> Result<TeletextSavedGuideInputs, TeletextDesignDocumentError> {
        if self.input_version != INPUT_VERSION {
            return Err(failure(TeletextDesignDocumentFailure::GuideInputs));
        }
        // The parent preflight already admitted all context, including requests,
        // before allocating these strings; validate their wire integrity here.
        let request = validate(
            selection,
            generation,
            self.v1.0.as_deref(),
            self.v2.0.as_deref(),
            0,
            budget,
        )?;
        if let Some(source) = source {
            super::source_fence::associate(source, request.as_ref())?;
        }
        Ok(TeletextSavedGuideInputs {
            v1: self.v1.0.map(String::into_bytes),
            v2: self.v2.0.map(String::into_bytes),
        })
    }
}

#[derive(Serialize)]
pub(super) struct BorrowedGuideInputs<'a> {
    input_version: u16,
    v1: Option<&'a str>,
    v2: Option<&'a str>,
}

pub(super) fn borrow<'a>(
    requests: &'a TeletextChannelRequestTemplates,
    selection: &TeletextChannelSelectionSnapshot,
    generation: &str,
    context_bytes: usize,
    budget: TeletextDesignDocumentBudget,
    source: Option<&TeletextGuideSourceFence>,
) -> Result<BorrowedGuideInputs<'a>, TeletextDesignDocumentError> {
    let text = |contract| {
        requests
            .request(contract)
            .map(std::str::from_utf8)
            .transpose()
            .map_err(|_| failure(TeletextDesignDocumentFailure::GuideInputs))
    };
    let v1 = text(TeletextPageContract::V1)?;
    let v2 = text(TeletextPageContract::V2)?;
    let request = validate(selection, generation, v1, v2, context_bytes, budget)?;
    if let Some(source) = source {
        super::source_fence::associate(source, request.as_ref())?;
    }
    Ok(BorrowedGuideInputs {
        input_version: INPUT_VERSION,
        v1,
        v2,
    })
}

fn validate(
    selection: &TeletextChannelSelectionSnapshot,
    generation: &str,
    v1: Option<&str>,
    v2: Option<&str>,
    mut context_bytes: usize,
    budget: TeletextDesignDocumentBudget,
) -> Result<Option<TeletextPagesRequest>, TeletextDesignDocumentError> {
    let inputs = [
        (TeletextPageContract::V1, v1),
        (TeletextPageContract::V2, v2),
    ];
    for (contract, input) in inputs {
        let selected = selection
            .bindings
            .iter()
            .any(|binding| binding.contract == contract);
        if selected != input.is_some() {
            return Err(failure(TeletextDesignDocumentFailure::GuideInputs));
        }
        if let Some(input) = input {
            limit(input.len(), MAX_TELETEXT_PAGES_INPUT_BYTES, "guide request")?;
            context_bytes = context_bytes
                .checked_add(input.len())
                .ok_or_else(|| failure(TeletextDesignDocumentFailure::Storage))?;
            limit(context_bytes, budget.max_context_bytes, "context")?;
        }
    }
    let mut canonical: Option<TeletextPagesRequest> = None;
    for (contract, input) in inputs {
        let Some(input) = input else { continue };
        let mut parsed =
            TeletextPagesRequest::from_json_for_schema(input.as_bytes(), contract.schema_version())
                .map_err(|_| failure(TeletextDesignDocumentFailure::GuideInputs))?;
        if parsed.generation != generation {
            return Err(failure(TeletextDesignDocumentFailure::GuideInputs));
        }
        if let Some(previous) = &canonical {
            // Compare all typed request fields except the deliberately selected
            // schema. Do not clone or reserialize either original byte buffer.
            parsed.schema_version = previous.schema_version;
            if parsed != *previous {
                return Err(failure(TeletextDesignDocumentFailure::GuideInputs));
            }
        } else {
            canonical = Some(parsed);
        }
    }
    Ok(canonical)
}
