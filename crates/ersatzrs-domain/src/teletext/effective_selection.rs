//! Observed base/published Scripted origins, never interchangeable producer tokens.

use std::fmt;

use chrono::{DateTime, Utc};

use super::{TeletextOverrideInterval, TeletextOverrideState, TeletextSelectionSnapshot};
use crate::EntityId;

/// Exact published interval observation with its owning playout identity.
/// Recheck its bounds, captured target and owning channel before consuming it;
/// an interval alone grants no native invocation or generation authority.
#[derive(Clone, PartialEq, Eq)]
pub struct TeletextPublishedOverride {
    pub playout_id: EntityId,
    pub interval: TeletextOverrideInterval,
}

impl fmt::Debug for TeletextPublishedOverride {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TeletextPublishedOverride")
            .field("playout_id", &self.playout_id)
            .field("channel_id", &self.interval.state.channel_id())
            .field("start", &self.interval.start)
            .field("finish", &self.interval.finish)
            .finish_non_exhaustive()
    }
}

/// Distinct authority origins for one channel/add-on. Active Scripted Off is
/// retained as a member of the complete fence even though it has no producer.
/// A Scripted candidate never acquires a fabricated base binding revision.
#[derive(Clone, PartialEq, Eq)]
pub enum TeletextEffectiveBindingSnapshot {
    Base(TeletextSelectionSnapshot),
    Scripted(TeletextPublishedOverride),
}

impl TeletextEffectiveBindingSnapshot {
    #[must_use]
    pub const fn channel_id(&self) -> EntityId {
        match self {
            Self::Base(base) => base.channel_id,
            Self::Scripted(published) => published.interval.state.channel_id(),
        }
    }

    #[must_use]
    pub fn addon_id(&self) -> &str {
        match self {
            Self::Base(base) => &base.addon_id,
            Self::Scripted(published) => published.interval.state.addon_id(),
        }
    }

    #[must_use]
    pub const fn enabled(&self) -> bool {
        match self {
            Self::Base(_) => true,
            Self::Scripted(published) => {
                matches!(published.interval.state, TeletextOverrideState::Enabled(_))
            }
        }
    }
}

impl fmt::Debug for TeletextEffectiveBindingSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Base(base) => f
                .debug_struct("Base")
                .field("channel_id", &base.channel_id)
                .field("binding_revision", &base.binding_revision)
                .field("profile_id", &base.profile_id)
                .field("profile_revision", &base.profile_revision)
                .finish_non_exhaustive(),
            Self::Scripted(published) => f.debug_tuple("Scripted").field(published).finish(),
        }
    }
}

/// Whole effective channel membership from one repository read transaction.
/// Members are ordered by add-on identity and include active Scripted Off.
/// Overridden base profiles are not validated or substituted into the result.
/// This observation grants no publication or native producer authority.
#[derive(Clone, PartialEq, Eq)]
pub struct TeletextEffectiveChannelSelectionSnapshot {
    pub channel_id: EntityId,
    pub bindings: Vec<TeletextEffectiveBindingSnapshot>,
    /// Earliest future published interval start/finish for this owning channel.
    /// Preparation/promotion must finish strictly before this boundary. `None`
    /// means no scheduled transition was observed, not an unlimited validity lease.
    /// The observation clock is deliberately absent from equality so repeated
    /// reads at a later clock compare equal until membership or this fence changes.
    pub valid_until: Option<DateTime<Utc>>,
}

impl fmt::Debug for TeletextEffectiveChannelSelectionSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TeletextEffectiveChannelSelectionSnapshot")
            .field("channel_id", &self.channel_id)
            .field("binding_count", &self.bindings.len())
            .field("valid_until", &self.valid_until)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        TeletextInstallationSnapshot, TeletextPageContract, TeletextProfileCandidateSnapshot,
        TeletextProfileSetting, TeletextProfileValue,
    };
    use chrono::{Duration, TimeZone as _};

    #[test]
    fn effective_selection_debug_redacts_every_base_and_scripted_payload() {
        let private = "private fixture payload";
        let base = TeletextSelectionSnapshot {
            contract: TeletextPageContract::V3,
            channel_id: 7,
            addon_id: private.to_owned(),
            binding_revision: 4,
            profile_id: 11,
            profile_revision: 2,
            installation: TeletextInstallationSnapshot {
                addon_id: private.to_owned(),
                state_revision: private.to_owned(),
                version: private.to_owned(),
                package_sha256: private.to_owned(),
                content_sha256: private.to_owned(),
                manifest_sha256: private.to_owned(),
            },
            effective_settings: vec![TeletextProfileSetting {
                key: private.to_owned(),
                value: TeletextProfileValue::Literal(private.to_owned()),
            }],
        };
        let candidate = TeletextProfileCandidateSnapshot {
            contract: base.contract,
            channel_id: base.channel_id,
            addon_id: base.addon_id.clone(),
            profile_id: base.profile_id,
            profile_revision: base.profile_revision,
            installation: base.installation.clone(),
            effective_settings: base.effective_settings.clone(),
        };
        let start = Utc.with_ymd_and_hms(2026, 10, 14, 12, 0, 0).unwrap();
        let finish = start + Duration::minutes(3);
        for state in [
            TeletextOverrideState::Enabled(candidate),
            TeletextOverrideState::Off {
                channel_id: 7,
                addon_id: private.to_owned(),
            },
        ] {
            let published = TeletextPublishedOverride {
                playout_id: 42,
                interval: TeletextOverrideInterval {
                    start,
                    finish,
                    state,
                },
            };
            let channel = TeletextEffectiveChannelSelectionSnapshot {
                channel_id: 7,
                bindings: vec![
                    TeletextEffectiveBindingSnapshot::Base(base.clone()),
                    TeletextEffectiveBindingSnapshot::Scripted(published.clone()),
                ],
                valid_until: Some(finish),
            };
            assert!(!format!("{published:?}").contains(private));
            assert!(format!("{published:?}").contains("playout_id: 42"));
            for binding in &channel.bindings {
                assert!(!format!("{binding:?}").contains(private));
            }
            assert!(!format!("{channel:?}").contains(private));
            assert!(format!("{channel:?}").contains("binding_count: 2"));
        }
    }
}
