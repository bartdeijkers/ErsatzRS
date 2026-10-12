//! Historical guide association, never current source or publication authority.

use super::{
    failure, limit, TeletextDesignDocumentBudget, TeletextDesignDocumentError,
    TeletextDesignDocumentFailure,
};
use chrono::{DateTime, Utc};
use ersatzrs_addon_contract::TeletextPagesRequest;
use ersatzrs_core::repositories::{TeletextGuideSourceFence, MAX_TELETEXT_GUIDE_SOURCE_CHANNELS};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::io;

#[derive(Default)]
pub(super) struct OptionalGuideSource(pub Option<TeletextGuideSourceFence>);

impl<'de> Deserialize<'de> for OptionalGuideSource {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // The core DTO accepts only a closed, bounded, validated map. Present
        // null is invalid; Default is exclusively for an omitted legacy field.
        TeletextGuideSourceFence::deserialize(deserializer).map(|value| Self(Some(value)))
    }
}

impl Serialize for OptionalGuideSource {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

pub(super) fn admit(
    source: &TeletextGuideSourceFence,
    mut context_bytes: usize,
    budget: TeletextDesignDocumentBudget,
) -> Result<usize, TeletextDesignDocumentError> {
    limit(
        source.channel_ids.len(),
        MAX_TELETEXT_GUIDE_SOURCE_CHANNELS,
        "guide channels",
    )?;
    for bytes in [
        source.locale.len(),
        source.timezone.len(),
        date_bytes(&source.captured_at)?,
        date_bytes(&source.start)?,
        date_bytes(&source.finish)?,
    ] {
        limit(bytes, budget.max_context_field_bytes, "context field")?;
        context_bytes = context_bytes
            .checked_add(bytes)
            .ok_or_else(|| failure(TeletextDesignDocumentFailure::Storage))?;
        limit(context_bytes, budget.max_context_bytes, "context")?;
    }
    source
        .validate()
        .map_err(|_| failure(TeletextDesignDocumentFailure::GuideSource))?;
    Ok(context_bytes)
}

pub(super) fn associate(
    source: &TeletextGuideSourceFence,
    request: Option<&TeletextPagesRequest>,
) -> Result<(), TeletextDesignDocumentError> {
    let request = request.ok_or_else(|| failure(TeletextDesignDocumentFailure::GuideSource))?;
    if source.locale != request.locale
        || source.timezone != request.timezone
        || source.captured_at != request.captured_at
        || source.start != request.window.start
        || source.finish != request.window.finish
        || !source
            .channel_ids
            .iter()
            .copied()
            .eq(request.channels.iter().map(|channel| channel.id))
    {
        return Err(failure(TeletextDesignDocumentFailure::GuideSource));
    }
    Ok(())
}

// Serialize the exact chrono scalar into a counting sink, not a temporary JSON
// buffer. RFC 3339 consists of unescaped ASCII; remove only its two JSON quotes.
fn date_bytes(date: &DateTime<Utc>) -> Result<usize, TeletextDesignDocumentError> {
    let mut counter = Counter(0);
    serde_json::to_writer(&mut counter, date)
        .map_err(|_| failure(TeletextDesignDocumentFailure::Storage))?;
    counter
        .0
        .checked_sub(2)
        .ok_or_else(|| failure(TeletextDesignDocumentFailure::Storage))
}

struct Counter(usize);
impl io::Write for Counter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 = self
            .0
            .checked_add(bytes.len())
            .ok_or_else(|| io::Error::other("saved source size"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
