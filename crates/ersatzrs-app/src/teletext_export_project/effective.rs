//! Descriptive effective selection syntax; immutable originals remain authority.
//!
//! Indices refer to the complete original effective binding list, including Off
//! members. Only real producers have exported origin metadata. The native host
//! must regenerate the exact baseline from its authenticated original. Historical
//! import retains its saved scope under a fresh operation deadline; publication
//! additionally requires the current effective scope to remain eligible.

use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;

use super::{
    failure, indexed, map_only, nullable, Result, TeletextExportProject,
    TeletextExportProjectFailure as K, MAX_BINDINGS,
};

/// Complete original binding count and exact original operation expiry.
/// Neither value authenticates uploaded project claims.
#[derive(Clone, PartialEq, Eq, Serialize)]
pub struct TeletextExportEffectiveScope {
    pub binding_count: usize,
    #[serde(deserialize_with = "nullable")]
    pub valid_until: Option<String>,
}
impl fmt::Debug for TeletextExportEffectiveScope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TeletextExportEffectiveScope(<redacted>)")
    }
}
impl<'de> Deserialize<'de> for TeletextExportEffectiveScope {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            binding_count: usize,
            #[serde(deserialize_with = "nullable")]
            valid_until: Option<String>,
        }
        let fields: Fields = map_only(deserializer)?;
        Ok(Self {
            binding_count: fields.binding_count,
            valid_until: fields.valid_until,
        })
    }
}

/// Original selection role, retaining a real base revision or exact Scripted
/// interval. Off members are retained only in the immutable original document.
#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "origin", rename_all = "snake_case")]
pub enum TeletextExportEffectiveOrigin {
    Base {
        binding_index: usize,
    },
    Scripted {
        binding_index: usize,
        playout_id: i64,
        start: String,
        finish: String,
    },
}
impl fmt::Debug for TeletextExportEffectiveOrigin {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TeletextExportEffectiveOrigin(<redacted>)")
    }
}
impl<'de> Deserialize<'de> for TeletextExportEffectiveOrigin {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "origin", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Base {
                binding_index: usize,
            },
            Scripted {
                binding_index: usize,
                playout_id: i64,
                start: String,
                finish: String,
            },
        }
        Ok(match map_only(deserializer)? {
            Fields::Base { binding_index } => Self::Base { binding_index },
            Fields::Scripted {
                binding_index,
                playout_id,
                start,
                finish,
            } => Self::Scripted {
                binding_index,
                playout_id,
                start,
                finish,
            },
        })
    }
}

// Absence uses the field's default. Explicit null is rejected rather than being
// reinterpreted as an absent origin/scope in a legacy serialized project.
pub(super) fn optional_object<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> std::result::Result<Option<T>, D::Error> {
    T::deserialize(deserializer).map(Some)
}

pub(super) fn validate_schema(project: &TeletextExportProject) -> Result<()> {
    match (project.schema_version, project.original.document_version) {
        (1, 1)
            if matches!(
                project.editor_profile.as_str(),
                "qteletextmaker-1.0"
                    | "qteletextmaker-1.0-carrier-v2"
                    | "qteletextmaker-1.0-carrier-v3"
                    | "qteletextmaker-1.0-carrier-v4"
            ) && project.effective_scope.is_none() =>
        {
            Ok(())
        }
        (2, 2)
            if project.editor_profile == "qteletextmaker-1.0-carrier-v4"
                && project.effective_scope.is_some() =>
        {
            Ok(())
        }
        _ => Err(failure(K::Schema)),
    }
}

/// Parse a UTC RFC3339 instant without normalizing the retained string or
/// accepting subnanosecond precision that Chrono would silently truncate.
fn utc(value: &str) -> Option<DateTime<FixedOffset>> {
    let suffix = if value.ends_with('Z') {
        1
    } else if value.ends_with("+00:00") {
        6
    } else {
        return None;
    };
    let body = value.get(..value.len().checked_sub(suffix)?)?;
    if body.len() < 19 || body.as_bytes().get(10) != Some(&b'T') {
        return None;
    }
    if let Some(fraction) = body.get(19..) {
        if !fraction.is_empty()
            && (fraction.len() < 2
                || fraction.len() > 10
                || !fraction.starts_with('.')
                || !fraction.as_bytes()[1..].iter().all(u8::is_ascii_digit))
        {
            return None;
        }
    }
    let instant = DateTime::parse_from_rfc3339(value).ok()?;
    (instant.offset().local_minus_utc() == 0).then_some(instant)
}

pub(super) fn validate_origins(project: &TeletextExportProject) -> Result<()> {
    if project.schema_version == 1 {
        for (index, producer) in project.producers.iter().enumerate() {
            if producer.effective_origin.is_some() || producer.binding_revision.is_none() {
                return Err(indexed(K::Identity, index));
            }
        }
        return Ok(());
    }
    let scope = project
        .effective_scope
        .as_ref()
        .ok_or_else(|| failure(K::Schema))?;
    if scope.binding_count == 0 || scope.binding_count > MAX_BINDINGS {
        return Err(failure(K::Limit));
    }
    let valid_until = scope
        .valid_until
        .as_deref()
        .map(|value| utc(value).ok_or_else(|| failure(K::Identity)))
        .transpose()?;
    let mut previous = None;
    for (index, producer) in project.producers.iter().enumerate() {
        let origin = producer
            .effective_origin
            .as_ref()
            .ok_or_else(|| indexed(K::Identity, index))?;
        let binding_index = match origin {
            TeletextExportEffectiveOrigin::Base { binding_index } => {
                if producer.binding_revision.is_none() {
                    return Err(indexed(K::Identity, index));
                }
                *binding_index
            }
            TeletextExportEffectiveOrigin::Scripted {
                binding_index,
                playout_id,
                start,
                finish,
            } => {
                if producer.binding_revision.is_some() || *playout_id <= 0 {
                    return Err(indexed(K::Identity, index));
                }
                let start = utc(start).ok_or_else(|| indexed(K::Identity, index))?;
                let finish = utc(finish).ok_or_else(|| indexed(K::Identity, index))?;
                let valid_until = valid_until.ok_or_else(|| indexed(K::Identity, index))?;
                if start >= valid_until || valid_until > finish {
                    return Err(indexed(K::Identity, index));
                }
                *binding_index
            }
        };
        if binding_index >= scope.binding_count
            || previous.is_some_and(|previous| previous >= binding_index)
        {
            return Err(indexed(K::Mapping, index));
        }
        previous = Some(binding_index);
    }
    Ok(())
}
