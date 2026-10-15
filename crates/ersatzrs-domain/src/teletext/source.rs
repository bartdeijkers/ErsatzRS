//! Reported source metadata, distinct from authored or actually observed pages.
//!
//! Binding records an adapter-supplied association, not immutable source bytes
//! or certified provenance. Constructors check structure only; repository
//! adapters must validate records again before writing or using deserialized
//! metadata. Raw dump interpretation and freshness checks belong outside domain.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::{SourceTeletextHeaderWindow, TeletextLanguage, TeletextPageNumber};
use crate::{EntityId, MediaSourceKind};

/// Resource policy for opaque adapter revision text, not a provider protocol limit.
pub const MAX_SOURCE_TELETEXT_REVISION_BYTES: usize = 1_024;

/// Static structural failures; input paths and revision text are never copied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SourceTeletextError {
    #[error("source teletext binding requires positive durable parent identities")]
    ParentIdentity,
    #[error("source teletext revision must be nonempty and within its resource limit")]
    Revision,
    #[error("source teletext stream index must be nonnegative")]
    StreamIndex,
    #[error("reported teletext PID must fit thirteen bits")]
    Pid,
    #[error("reported teletext descriptor type must fit five bits")]
    DescriptorType,
    #[error("reported teletext magazine must fit three bits")]
    Magazine,
    #[error("reported teletext descriptor ordinals must be unique")]
    DuplicateOrdinal,
    #[error("reported trailing unpaired descriptor warning must have a positive count")]
    EmptyShrinkWarning,
    #[error("source teletext header window exceeds its structural bounds")]
    HeaderWindow,
    #[error("source teletext header program membership is invalid or duplicated")]
    HeaderProgram,
    #[error("source teletext observed header contains invalid wire fields")]
    ObservedHeader,
    #[error("source teletext observed headers must retain strict transport order")]
    HeaderOrder,
    #[error("source teletext header observations require a bound source and actual PID")]
    HeaderBinding,
}

/// Opaque adapter-supplied revision; syntax validation establishes no assurance.
///
/// Never manufacture this from a path hash or scan timestamp. The adapter owns
/// how a genuine provider/source revision was acquired and compared.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct SourceTeletextRevision(String);

impl std::fmt::Debug for SourceTeletextRevision {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SourceTeletextRevision(<opaque>)")
    }
}

impl SourceTeletextRevision {
    /// Retain supplied revision text without normalization or identity inference.
    /// # Errors
    /// Returns `Revision` for empty text or text exceeding the resource bound.
    pub fn new(revision: String) -> Result<Self, SourceTeletextError> {
        if revision.is_empty() || revision.len() > MAX_SOURCE_TELETEXT_REVISION_BYTES {
            return Err(SourceTeletextError::Revision);
        }
        Ok(Self(revision))
    }

    /// Exact opaque text, for adapter comparisons rather than display/logging.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for SourceTeletextRevision {
    type Error = SourceTeletextError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<SourceTeletextRevision> for String {
    fn from(value: SourceTeletextRevision) -> Self {
        value.0
    }
}

/// Durable parents supplied by the adapter; no byte or source-service guarantee.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceTeletextBindingIdentity {
    pub media_source_id: EntityId,
    pub provider: MediaSourceKind,
    pub media_version_id: EntityId,
    pub media_file_id: EntityId,
    pub media_stream_id: EntityId,
    pub source_revision: SourceTeletextRevision,
}

impl SourceTeletextBindingIdentity {
    /// Associate existing parents with a supplied revision, without proving it.
    /// # Errors
    /// Returns `ParentIdentity` unless all four durable IDs are positive.
    pub fn new(
        media_source_id: EntityId,
        provider: MediaSourceKind,
        media_version_id: EntityId,
        media_file_id: EntityId,
        media_stream_id: EntityId,
        source_revision: SourceTeletextRevision,
    ) -> Result<Self, SourceTeletextError> {
        let identity = Self {
            media_source_id,
            provider,
            media_version_id,
            media_file_id,
            media_stream_id,
            source_revision,
        };
        identity.validate()?;
        Ok(identity)
    }

    /// Check parent-ID structure, not database membership or revision assurance.
    /// # Errors
    /// Returns `ParentIdentity` if a parent has no positive durable identity.
    pub fn validate(&self) -> Result<(), SourceTeletextError> {
        if [
            self.media_source_id,
            self.media_version_id,
            self.media_file_id,
            self.media_stream_id,
        ]
        .iter()
        .any(|id| *id <= 0)
        {
            return Err(SourceTeletextError::ParentIdentity);
        }
        Ok(())
    }
}

/// Unbound metadata remains report-only even when its descriptor page is decimal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceTeletextBinding {
    Unavailable,
    Unusable,
    /// Adapter association only; not immutable-byte or certified-provenance proof.
    Bound(SourceTeletextBindingIdentity),
}

/// Acquisition mechanism of the reported metadata, not provider/source authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceTeletextProvenance {
    /// Auxiliary acquisition was not performed or its mechanism is unknown.
    Unavailable,
    /// Language-associated packed codec projection supplied by `FFprobe` 9.
    Ffprobe9PackedDescriptorProjection,
}

/// Independent transport association; malformed or conflicting PIDs stay unusable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceTeletextPid {
    Unavailable,
    Unusable,
    Reported(u16),
}

/// Raw program identity/membership supplied by the acquisition adapter.
/// Neither the first membership nor a PMT PID is a source-service selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceTeletextProgram {
    pub program_id: u32,
    pub program_number: u32,
    pub pmt_pid: u16,
    pub pcr_pid: u16,
}

/// All supplied memberships; availability is independent from PID/descriptors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceTeletextPrograms {
    Unavailable,
    Unusable,
    Reported(Vec<SourceTeletextProgram>),
}

/// One report-only descriptor; repeated addresses and reserved values survive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceTeletextDescriptor {
    /// Position in the supplied projection, never a wire descriptor identity.
    pub projection_ordinal: u32,
    pub language: TeletextLanguage,
    /// Raw five-bit type, independent from forced/default subtitle selection.
    pub descriptor_type: u8,
    /// Raw three-bit magazine; zero represents magazine eight.
    pub wire_magazine: u8,
    /// Raw page byte; hexadecimal/non-display addresses are retained.
    pub wire_page: u8,
}

impl SourceTeletextDescriptor {
    /// Preserve one descriptor without deduplicating its language or address.
    /// # Errors
    /// Returns `DescriptorType` or `Magazine` for values outside their bit width.
    pub fn new(
        projection_ordinal: u32,
        language: TeletextLanguage,
        descriptor_type: u8,
        wire_magazine: u8,
        wire_page: u8,
    ) -> Result<Self, SourceTeletextError> {
        let descriptor = Self {
            projection_ordinal,
            language,
            descriptor_type,
            wire_magazine,
            wire_page,
        };
        descriptor.validate()?;
        Ok(descriptor)
    }

    /// Structural bit-width checks retain reserved types and nondecimal pages.
    /// # Errors
    /// Returns `DescriptorType` or `Magazine` for malformed packed fields.
    pub fn validate(&self) -> Result<(), SourceTeletextError> {
        if self.descriptor_type > 31 {
            return Err(SourceTeletextError::DescriptorType);
        }
        if self.wire_magazine > 7 {
            return Err(SourceTeletextError::Magazine);
        }
        Ok(())
    }

    /// Derive a decimal descriptor address, not an actually observed/selectable page.
    #[must_use]
    pub fn display_page(&self) -> Option<TeletextPageNumber> {
        let tens = self.wire_page >> 4;
        let units = self.wire_page & 15;
        if tens > 9 || units > 9 || self.wire_magazine > 7 {
            return None;
        }
        let magazine = if self.wire_magazine == 0 {
            8
        } else {
            u16::from(self.wire_magazine)
        };
        TeletextPageNumber::new(magazine * 100 + u16::from(tens) * 10 + u16::from(units)).ok()
    }
}

/// Useful paired prefix only; it does not establish descriptor/page completeness.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceTeletextDescriptors {
    Unavailable,
    Unusable,
    Reported {
        entries: Vec<SourceTeletextDescriptor>,
        /// Supplied extra pairs excluded from the fresh language-associated prefix.
        trailing_unpaired_pairs: Option<u32>,
    },
}

/// Actual bounded observations remain distinct from selectable/complete pages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceTeletextObservedPages {
    NotAcquired,
    /// Packet-0 observations only; body, charset and readiness remain unproven.
    HeadersObserved(Box<SourceTeletextHeaderWindow>),
}

/// Independent report-only fields from one supplied stream snapshot.
/// No selection, extraction or source-byte certification is implied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceTeletextObservation {
    pub binding: SourceTeletextBinding,
    /// Fresh global stream index; list ordinals must never substitute for it.
    pub stream_index: i32,
    pub provenance: SourceTeletextProvenance,
    pub pid: SourceTeletextPid,
    pub programs: SourceTeletextPrograms,
    pub descriptors: SourceTeletextDescriptors,
    pub observed_pages: SourceTeletextObservedPages,
}

impl SourceTeletextObservation {
    /// Construct descriptor metadata with actually observed pages explicitly absent.
    /// # Errors
    /// Returns a structural failure from `validate`; no input metadata is logged.
    pub fn new(
        binding: SourceTeletextBinding,
        stream_index: i32,
        provenance: SourceTeletextProvenance,
        pid: SourceTeletextPid,
        programs: SourceTeletextPrograms,
        descriptors: SourceTeletextDescriptors,
    ) -> Result<Self, SourceTeletextError> {
        let observation = Self {
            binding,
            stream_index,
            provenance,
            pid,
            programs,
            descriptors,
            observed_pages: SourceTeletextObservedPages::NotAcquired,
        };
        observation.validate()?;
        Ok(observation)
    }

    /// Check structure before repository use; freshness/parent existence are external.
    /// # Errors
    /// Returns invalid parent/index/bit-width/ordinal or shrink-warning structure.
    pub fn validate(&self) -> Result<(), SourceTeletextError> {
        if self.stream_index < 0 {
            return Err(SourceTeletextError::StreamIndex);
        }
        if let SourceTeletextBinding::Bound(identity) = &self.binding {
            identity.validate()?;
        }
        if matches!(self.pid, SourceTeletextPid::Reported(pid) if pid > 0x1fff) {
            return Err(SourceTeletextError::Pid);
        }
        if let SourceTeletextPrograms::Reported(programs) = &self.programs {
            if programs
                .iter()
                .any(|program| program.pmt_pid > 0x1fff || program.pcr_pid > 0x1fff)
            {
                return Err(SourceTeletextError::Pid);
            }
        }
        if let SourceTeletextDescriptors::Reported {
            entries,
            trailing_unpaired_pairs,
        } = &self.descriptors
        {
            if *trailing_unpaired_pairs == Some(0) {
                return Err(SourceTeletextError::EmptyShrinkWarning);
            }
            let mut ordinals = BTreeSet::new();
            for entry in entries {
                entry.validate()?;
                if !ordinals.insert(entry.projection_ordinal) {
                    return Err(SourceTeletextError::DuplicateOrdinal);
                }
            }
        }
        if let SourceTeletextObservedPages::HeadersObserved(window) = &self.observed_pages {
            if !matches!(self.binding, SourceTeletextBinding::Bound(_))
                || !matches!(self.pid, SourceTeletextPid::Reported(0x20..=0x1ffe))
            {
                return Err(SourceTeletextError::HeaderBinding);
            }
            window.validate()?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "source_tests.rs"]
mod tests;
