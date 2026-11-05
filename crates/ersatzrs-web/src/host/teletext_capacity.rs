//! Named teletext capacity profiles that an operator applies explicitly.
//!
//! The authoring (`teletext.authoring.policy`) and output
//! (`teletext.output.policy`) settings stay explicit operator configuration:
//! nothing here is read as a compiler default and absence still means ordinary
//! playback. A profile is a reviewed, versioned pair of complete policies that
//! the admin can write in one confirmed action instead of hand-written JSON.
//!
//! `tvguide-16` is the exact policy pair under which the production TV-guide
//! add-on bundle is qualified by the repository's integrated tests (authoring
//! version 2 with guide-source limits; output version 4 with live updates and
//! guide refresh). Its bounds and their rationale are documented in
//! `docs/reference/teletext-capacity-profiles.md`. Each bound is a ceiling,
//! not a reservation: unused capacity costs nothing, and exceeding one rejects
//! that preparation with a diagnostic while ordinary playback continues.

use ersatzrs_app::ApplicationServices;
use serde_json::Value;

use super::teletext_runtime_policy::{
    parse_native_teletext_authoring_policy, parse_native_teletext_runtime_policy,
    NATIVE_TELETEXT_AUTHORING_POLICY_SETTING, NATIVE_TELETEXT_RUNTIME_POLICY_SETTING,
};

/// One complete reviewed policy pair.
pub(crate) struct TeletextCapacityProfile {
    pub(crate) id: &'static str,
    authoring: &'static str,
    output: &'static str,
}

/// TV guide (Level 1/1.5) with at most sixteen encoded pages per generation.
pub(crate) const TVGUIDE_16: TeletextCapacityProfile = TeletextCapacityProfile {
    id: "tvguide-16",
    authoring: include_str!("teletext_capacity/tvguide-16-authoring.json"),
    output: include_str!("teletext_capacity/tvguide-16-output.json"),
};

pub(crate) const PROFILES: &[TeletextCapacityProfile] = &[TVGUIDE_16];

pub(crate) fn profile(id: &str) -> Option<&'static TeletextCapacityProfile> {
    PROFILES.iter().find(|profile| profile.id == id)
}

impl TeletextCapacityProfile {
    /// Largest number of encoded pages (including subpages) per generation.
    pub(crate) fn max_pages(&self) -> Option<u64> {
        serde_json::from_str::<Value>(self.output)
            .ok()?
            .pointer("/encoding/max_pages")?
            .as_u64()
    }
}

/// Observed state of one policy setting; never includes the raw value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TeletextPolicyState {
    Absent,
    /// Valid and semantically identical to the named profile.
    Profile(&'static str),
    /// Valid but not identical to any named profile (hand-tuned).
    Custom,
    Invalid,
    Unreadable,
}

impl TeletextPolicyState {
    pub(crate) const fn code(&self) -> &'static str {
        match self {
            Self::Absent => "absent",
            Self::Profile(_) => "profile",
            Self::Custom => "custom",
            Self::Invalid => "invalid",
            Self::Unreadable => "unreadable",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Authoring,
    Output,
}

impl Kind {
    const fn key(self) -> &'static str {
        match self {
            Self::Authoring => NATIVE_TELETEXT_AUTHORING_POLICY_SETTING,
            Self::Output => NATIVE_TELETEXT_RUNTIME_POLICY_SETTING,
        }
    }

    fn valid(self, raw: &str) -> bool {
        match self {
            Self::Authoring => parse_native_teletext_authoring_policy(raw).is_ok(),
            Self::Output => parse_native_teletext_runtime_policy(raw).is_ok(),
        }
    }

    const fn source(self, profile: &TeletextCapacityProfile) -> &'static str {
        match self {
            Self::Authoring => profile.authoring,
            Self::Output => profile.output,
        }
    }
}

fn classify(kind: Kind, raw: Option<&str>) -> TeletextPolicyState {
    let Some(raw) = raw else {
        return TeletextPolicyState::Absent;
    };
    if !kind.valid(raw) {
        return TeletextPolicyState::Invalid;
    }
    let Ok(value) = serde_json::from_str::<Value>(raw) else {
        return TeletextPolicyState::Invalid;
    };
    PROFILES
        .iter()
        .find(|profile| {
            serde_json::from_str::<Value>(kind.source(profile)).is_ok_and(|known| known == value)
        })
        .map_or(TeletextPolicyState::Custom, |profile| {
            TeletextPolicyState::Profile(profile.id)
        })
}

async fn observe(application: &ApplicationServices, kind: Kind) -> TeletextPolicyState {
    match application.get_optional_setting(kind.key()).await {
        Ok(raw) => classify(kind, raw.as_deref()),
        Err(_) => TeletextPolicyState::Unreadable,
    }
}

/// Fresh read of both settings for the channel page and Health.
pub(crate) async fn policy_status(
    application: &ApplicationServices,
) -> (TeletextPolicyState, TeletextPolicyState) {
    (
        observe(application, Kind::Authoring).await,
        observe(application, Kind::Output).await,
    )
}

/// Write the named profile into the two global settings.
///
/// Missing or invalid settings are written. A valid setting that differs from
/// the profile is only replaced with `replace`, so a hand-tuned policy is never
/// overwritten by an unconfirmed click. Both profile documents are parsed with
/// the same validators the runtime uses before anything is written.
/// # Errors
/// Returns a stable redirect code; nothing is written on validation failures.
pub(crate) async fn apply_profile(
    application: &ApplicationServices,
    id: &str,
    replace: bool,
) -> Result<(), &'static str> {
    let profile = profile(id).ok_or("invalid")?;
    for kind in [Kind::Authoring, Kind::Output] {
        if !kind.valid(kind.source(profile)) {
            return Err("invalid");
        }
    }
    let mut writes = Vec::new();
    for kind in [Kind::Authoring, Kind::Output] {
        match observe(application, kind).await {
            TeletextPolicyState::Unreadable => return Err("storage"),
            TeletextPolicyState::Profile(current) if current == profile.id => {}
            TeletextPolicyState::Custom | TeletextPolicyState::Profile(_) if !replace => {
                return Err("policy-exists");
            }
            _ => writes.push(kind),
        }
    }
    for kind in writes {
        application
            .set_admin_setting(ersatzrs_app::commands::SetAdminSetting {
                key: kind.key(),
                value: kind.source(profile).trim_end().to_owned(),
            })
            .await
            .map_err(|_| "storage")?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "teletext_capacity_tests.rs"]
mod tests;
