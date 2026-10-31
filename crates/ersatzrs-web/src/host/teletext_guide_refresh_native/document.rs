//! Versioned runtime-only framing; no manual revision or export receipt is minted.

use std::io::{self, Write};

use ersatzrs_app::teletext_design_documents::TeletextDesignDocumentBudget;
use ersatzrs_core::repositories::{TeletextGuideRefreshSeed, TeletextGuideRuntimeSnapshot};
use serde::{Deserialize, Serialize};

use super::{compilation, FreshGuideBaseline, GuideRefreshNativeError as Error};

pub(super) const VERSION: u16 = 1;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    version: u16,
    seed_id: i64,
    origin_generation_id: i64,
    origin_request_id: i64,
    origin_design_id: i64,
    origin_revision: i64,
    origin_digest: String,
    role: i64,
    baseline_version: u16,
    // A JSON string preserves exact saved bytes, including original spelling.
    baseline: String,
}
impl Envelope {
    fn matches(&self, seed: &TeletextGuideRefreshSeed) -> bool {
        let origin = &seed.generation;
        self.version == VERSION
            && self.seed_id == seed.id
            && seed.id > 0
            && self.origin_generation_id == origin.id
            && origin.id > 0
            && self.origin_request_id == origin.request.id
            && origin.request.id > 0
            && self.origin_design_id == origin.request.design_id
            && self.origin_revision == origin.request.revision
            && self.origin_digest == origin.request.digest
            && self.role == seed.role.code()
    }
}

struct BoundedEnvelope {
    bytes: Vec<u8>,
    maximum: usize,
}
impl Write for BoundedEnvelope {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let size = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .filter(|size| *size <= self.maximum)
            .ok_or_else(|| io::Error::other("guide runtime envelope exceeds its byte bound"))?;
        self.bytes
            .try_reserve(size - self.bytes.len())
            .map_err(|_| io::Error::other("guide runtime envelope allocation failed"))?;
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(super) fn encode(
    seed: &TeletextGuideRefreshSeed,
    baseline: &FreshGuideBaseline,
    maximum: usize,
) -> Result<Vec<u8>, Error> {
    if maximum == 0 || baseline.document.len() > maximum {
        return Err(Error::NeedsReview);
    }
    let origin = &seed.generation;
    let envelope = Envelope {
        version: VERSION,
        seed_id: seed.id,
        origin_generation_id: origin.id,
        origin_request_id: origin.request.id,
        origin_design_id: origin.request.design_id,
        origin_revision: origin.request.revision,
        origin_digest: origin.request.digest.clone(),
        role: seed.role.code(),
        baseline_version: baseline.document_version,
        baseline: String::from_utf8(baseline.document.clone()).map_err(|_| Error::NeedsReview)?,
    };
    if !envelope.matches(seed) {
        return Err(Error::NeedsReview);
    }
    let mut output = BoundedEnvelope {
        bytes: Vec::new(),
        maximum,
    };
    serde_json::to_writer(&mut output, &envelope).map_err(|_| Error::NeedsReview)?;
    Ok(output.bytes)
}

pub(super) fn decode(
    seed: &TeletextGuideRefreshSeed,
    snapshot: &TeletextGuideRuntimeSnapshot,
    budget: TeletextDesignDocumentBudget,
) -> Result<FreshGuideBaseline, Error> {
    if snapshot.saved.document.is_empty()
        || snapshot.saved.document.len() > budget.max_serialized_bytes
    {
        return Err(Error::NeedsReview);
    }
    let envelope: Envelope =
        serde_json::from_slice(&snapshot.saved.document).map_err(|_| Error::NeedsReview)?;
    if !envelope.matches(seed)
        || snapshot.saved.document_version != VERSION
        || envelope.baseline.len() > budget.max_serialized_bytes
    {
        return Err(Error::NeedsReview);
    }
    let document = envelope.baseline.into_bytes();
    let selection = compilation::selection(&document, envelope.baseline_version, budget)?;
    Ok(FreshGuideBaseline {
        document_version: envelope.baseline_version,
        document,
        qualification: snapshot.saved.qualification.clone(),
        selection,
    })
}
