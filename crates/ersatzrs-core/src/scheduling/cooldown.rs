use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Duration, Utc};
use ersatzrs_domain::EntityId;
use serde::{Deserialize, Serialize};

const POLICY_VERSION: u32 = 1;
const STATE_VERSION: u32 = 1;

#[must_use]
pub fn program_schedule_cooldown_config_key(schedule_id: EntityId) -> String {
    format!("scheduling.cooldown.program_schedule.{schedule_id}")
}

#[must_use]
pub fn playout_cooldown_config_key(playout_id: EntityId) -> String {
    format!("scheduling.cooldown.playout.{playout_id}")
}

/// Scope used to isolate active cooldown tags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SchedulingCooldownScope {
    #[default]
    Playout,
    ScheduleItem,
    ContentSource,
}

/// Constraint behavior used when every deterministic candidate is blocked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SchedulingCooldownMode {
    #[default]
    Soft,
}

/// Admin-facing presets. `Custom` leaves all explicit policy fields intact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SchedulingCooldownPreset {
    #[default]
    Off,
    Light,
    Balanced,
    Custom,
}

/// Similarity facet emitted for one scheduling candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SchedulingCooldownRuleKind {
    Artist,
    AlbumArtist,
    Show,
    Season,
    ContentParent,
    Collection,
    Tag,
    Genre,
    Studio,
    Filter,
}

/// One ordered similarity rule from ADR 0019.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchedulingCooldownRule {
    pub id: String,
    pub kind: SchedulingCooldownRuleKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search: Option<String>,
}

/// Versioned, disabled-by-default cooldown policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchedulingCooldownPolicy {
    #[serde(default = "policy_version")]
    pub version: u32,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub preset: SchedulingCooldownPreset,
    #[serde(default)]
    pub scope: SchedulingCooldownScope,
    #[serde(default)]
    pub mode: SchedulingCooldownMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_items: Option<u32>,
    /// Cooldown after the whole scheduling unit finishes, formatted as HH:MM:SS.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_duration: Option<String>,
    #[serde(default)]
    pub include_filler: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<SchedulingCooldownRule>,
}

impl Default for SchedulingCooldownPolicy {
    fn default() -> Self {
        Self {
            version: POLICY_VERSION,
            enabled: false,
            preset: SchedulingCooldownPreset::Off,
            scope: SchedulingCooldownScope::Playout,
            mode: SchedulingCooldownMode::Soft,
            after_items: None,
            after_duration: None,
            include_filler: false,
            rules: Vec::new(),
        }
    }
}

impl SchedulingCooldownPolicy {
    #[must_use]
    pub fn is_disabled(&self) -> bool {
        *self == Self::default()
    }

    /// Return the effective policy for an admin preset.
    #[must_use]
    pub fn for_preset(preset: SchedulingCooldownPreset) -> Self {
        let common_rules = || {
            vec![
                rule("artist", SchedulingCooldownRuleKind::Artist),
                rule("show", SchedulingCooldownRuleKind::Show),
                rule("content_parent", SchedulingCooldownRuleKind::ContentParent),
            ]
        };
        match preset {
            SchedulingCooldownPreset::Off => Self::default(),
            SchedulingCooldownPreset::Light => Self {
                enabled: true,
                preset,
                after_items: Some(2),
                rules: common_rules(),
                ..Self::default()
            },
            SchedulingCooldownPreset::Balanced => Self {
                enabled: true,
                preset,
                after_items: Some(4),
                after_duration: Some("00:30:00".to_owned()),
                rules: common_rules()
                    .into_iter()
                    .chain([
                        rule("season", SchedulingCooldownRuleKind::Season),
                        rule("collection", SchedulingCooldownRuleKind::Collection),
                        rule("tag", SchedulingCooldownRuleKind::Tag),
                    ])
                    .collect(),
                ..Self::default()
            },
            SchedulingCooldownPreset::Custom => Self {
                enabled: true,
                preset,
                ..Self::default()
            },
        }
    }

    #[must_use]
    pub fn is_active(&self) -> bool {
        self.enabled
            && (!self.rules.is_empty())
            && (self.after_items.is_some() || self.after_duration_value().is_some())
    }

    #[must_use]
    pub fn after_duration_value(&self) -> Option<Duration> {
        self.after_duration
            .as_deref()
            .and_then(parse_policy_duration)
            .filter(|duration| *duration > Duration::zero())
    }

    #[must_use]
    pub fn scope_key(
        &self,
        playout_id: EntityId,
        schedule_item: Option<EntityId>,
        content_source: Option<&str>,
    ) -> String {
        match self.scope {
            SchedulingCooldownScope::Playout => format!("playout:{playout_id}"),
            SchedulingCooldownScope::ScheduleItem => schedule_item.map_or_else(
                || format!("playout:{playout_id}"),
                |id| format!("schedule_item:{id}"),
            ),
            SchedulingCooldownScope::ContentSource => content_source.map_or_else(
                || format!("playout:{playout_id}"),
                |key| format!("content_source:{key}"),
            ),
        }
    }
}

fn policy_version() -> u32 {
    POLICY_VERSION
}

fn rule(id: &str, kind: SchedulingCooldownRuleKind) -> SchedulingCooldownRule {
    SchedulingCooldownRule {
        id: id.to_owned(),
        kind,
        search: None,
    }
}

fn parse_policy_duration(value: &str) -> Option<Duration> {
    let parts = value
        .trim()
        .split(':')
        .map(str::parse::<i64>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    match parts.as_slice() {
        [seconds] => Some(Duration::seconds(*seconds)),
        [minutes, seconds] if *seconds < 60 => {
            Some(Duration::minutes(*minutes) + Duration::seconds(*seconds))
        }
        [hours, minutes, seconds] if *minutes < 60 && *seconds < 60 => Some(
            Duration::hours(*hours) + Duration::minutes(*minutes) + Duration::seconds(*seconds),
        ),
        _ => None,
    }
}

/// One candidate in the scheduler's existing deterministic base order.
#[derive(Debug, Clone, Copy)]
pub struct SchedulingCooldownCandidate<'a> {
    pub media_item_id: EntityId,
    pub tags: &'a BTreeSet<String>,
}

/// An indivisible scheduling unit. Tags are the union of all child tags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchedulingCooldownUnit {
    pub media_item_ids: Vec<EntityId>,
    pub tags: BTreeSet<String>,
    pub duration: Duration,
}

impl SchedulingCooldownUnit {
    #[must_use]
    pub fn single(media_item_id: EntityId, tags: BTreeSet<String>, duration: Duration) -> Self {
        Self {
            media_item_ids: vec![media_item_id],
            tags,
            duration,
        }
    }

    #[must_use]
    pub fn grouped(children: impl IntoIterator<Item = Self>) -> Self {
        let mut media_item_ids = Vec::new();
        let mut tags = BTreeSet::new();
        let mut duration = Duration::zero();
        for child in children {
            media_item_ids.extend(child.media_item_ids);
            tags.extend(child.tags);
            duration += child.duration;
        }
        Self {
            media_item_ids,
            tags,
            duration,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SchedulingCooldownEntry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    release_after_items: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    release_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
struct SchedulingCooldownBucket {
    completed_items: u64,
    active: BTreeMap<String, SchedulingCooldownEntry>,
}

/// Versioned runtime state persisted in `PlayoutAnchor.Context`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchedulingCooldownState {
    #[serde(default = "state_version")]
    pub version: u32,
    #[serde(default)]
    buckets: BTreeMap<String, SchedulingCooldownBucket>,
}

impl Default for SchedulingCooldownState {
    fn default() -> Self {
        Self {
            version: STATE_VERSION,
            buckets: BTreeMap::new(),
        }
    }
}

impl SchedulingCooldownState {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.buckets.is_empty()
    }

    /// Select from the supplied base order without random rejection.
    #[must_use]
    pub fn select_candidate(
        &self,
        policy: &SchedulingCooldownPolicy,
        scope_key: &str,
        scheduled_at: DateTime<Utc>,
        candidates: &[SchedulingCooldownCandidate<'_>],
    ) -> Option<usize> {
        if candidates.is_empty() {
            return None;
        }
        if !policy.is_active() {
            return Some(0);
        }

        let empty_bucket = SchedulingCooldownBucket::default();
        let bucket = self.buckets.get(scope_key).unwrap_or(&empty_bucket);
        let selected =
            candidates
                .iter()
                .enumerate()
                .min_by(|(left_rank, left), (right_rank, right)| {
                    candidate_score(bucket, scheduled_at, *left_rank, left).cmp(&candidate_score(
                        bucket,
                        scheduled_at,
                        *right_rank,
                        right,
                    ))
                });
        if let Some((base_rank, candidate)) = selected {
            let score = candidate_score(bucket, scheduled_at, base_rank, candidate);
            if score.blocked_tags > 0 {
                tracing::warn!(
                    scope = scope_key,
                    media_item_id = candidate.media_item_id,
                    blocked_tags = score.blocked_tags,
                    "all cooldown candidates were blocked; selected deterministic soft fallback"
                );
            }
            Some(base_rank)
        } else {
            None
        }
    }

    /// Advance cooldowns once and activate all tags emitted by the chosen unit.
    pub fn record_unit(
        &mut self,
        policy: &SchedulingCooldownPolicy,
        scope_key: &str,
        scheduled_at: DateTime<Utc>,
        unit: &SchedulingCooldownUnit,
        is_filler: bool,
    ) {
        if !policy.is_active() || (is_filler && !policy.include_filler) {
            return;
        }

        let bucket = self.buckets.entry(scope_key.to_owned()).or_default();
        bucket.completed_items = bucket.completed_items.saturating_add(1);
        bucket.active.retain(|_, entry| {
            entry
                .release_after_items
                .is_some_and(|release| bucket.completed_items < release)
                || entry
                    .release_at
                    .is_some_and(|release| scheduled_at < release)
        });

        let finish = scheduled_at + unit.duration;
        for tag in &unit.tags {
            bucket.active.insert(
                tag.clone(),
                SchedulingCooldownEntry {
                    release_after_items: policy
                        .after_items
                        .map(|count| bucket.completed_items.saturating_add(u64::from(count))),
                    release_at: policy
                        .after_duration_value()
                        .map(|duration| finish + duration),
                },
            );
        }
    }
}

fn state_version() -> u32 {
    STATE_VERSION
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CandidateScore {
    blocked_tags: usize,
    latest_release_at: Option<DateTime<Utc>>,
    largest_item_distance: u64,
    base_rank: usize,
    media_item_id: EntityId,
}

impl Ord for CandidateScore {
    fn cmp(&self, other: &Self) -> Ordering {
        self.blocked_tags
            .cmp(&other.blocked_tags)
            .then(self.latest_release_at.cmp(&other.latest_release_at))
            .then(self.largest_item_distance.cmp(&other.largest_item_distance))
            .then(self.base_rank.cmp(&other.base_rank))
            .then(self.media_item_id.cmp(&other.media_item_id))
    }
}

impl PartialOrd for CandidateScore {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn candidate_score(
    bucket: &SchedulingCooldownBucket,
    scheduled_at: DateTime<Utc>,
    base_rank: usize,
    candidate: &SchedulingCooldownCandidate<'_>,
) -> CandidateScore {
    let blocked = candidate
        .tags
        .iter()
        .filter_map(|tag| bucket.active.get(tag))
        .filter(|entry| {
            entry
                .release_after_items
                .is_some_and(|release| bucket.completed_items < release)
                || entry
                    .release_at
                    .is_some_and(|release| scheduled_at < release)
        })
        .collect::<Vec<_>>();

    CandidateScore {
        blocked_tags: blocked.len(),
        latest_release_at: blocked.iter().filter_map(|entry| entry.release_at).max(),
        largest_item_distance: blocked
            .iter()
            .filter_map(|entry| entry.release_after_items)
            .map(|release| release.saturating_sub(bucket.completed_items))
            .max()
            .unwrap_or_default(),
        base_rank,
        media_item_id: candidate.media_item_id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(minute: i64) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 17, 12, minute as u32, 0)
            .single()
            .unwrap()
    }

    fn tags(values: &[&str]) -> BTreeSet<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    fn custom_policy(
        after_items: Option<u32>,
        after_duration: Option<&str>,
    ) -> SchedulingCooldownPolicy {
        SchedulingCooldownPolicy {
            enabled: true,
            preset: SchedulingCooldownPreset::Custom,
            after_items,
            after_duration: after_duration.map(str::to_owned),
            rules: vec![rule("artist", SchedulingCooldownRuleKind::Artist)],
            ..SchedulingCooldownPolicy::default()
        }
    }

    #[test]
    fn disabled_policy_preserves_base_order_and_empty_state() {
        let state = SchedulingCooldownState::default();
        let first = tags(&["artist:1"]);
        let second = tags(&["artist:2"]);
        let candidates = [
            SchedulingCooldownCandidate {
                media_item_id: 9,
                tags: &first,
            },
            SchedulingCooldownCandidate {
                media_item_id: 3,
                tags: &second,
            },
        ];

        assert_eq!(
            state.select_candidate(
                &SchedulingCooldownPolicy::default(),
                "playout:1",
                at(0),
                &candidates
            ),
            Some(0)
        );
        assert_eq!(state, SchedulingCooldownState::default());
    }

    #[test]
    fn item_count_cooldown_avoids_then_releases_a_group() {
        let policy = custom_policy(Some(2), None);
        let mut state = SchedulingCooldownState::default();
        let first_tags = tags(&["artist:1"]);
        let other_tags = tags(&["artist:2"]);
        let first_unit =
            SchedulingCooldownUnit::single(1, first_tags.clone(), Duration::minutes(3));
        state.record_unit(&policy, "playout:1", at(0), &first_unit, false);

        let candidates = [
            SchedulingCooldownCandidate {
                media_item_id: 2,
                tags: &first_tags,
            },
            SchedulingCooldownCandidate {
                media_item_id: 3,
                tags: &other_tags,
            },
        ];
        assert_eq!(
            state.select_candidate(&policy, "playout:1", at(3), &candidates),
            Some(1)
        );

        for minute in [3, 6] {
            let unit = SchedulingCooldownUnit::single(3, other_tags.clone(), Duration::minutes(3));
            state.record_unit(&policy, "playout:1", at(minute), &unit, false);
        }
        assert_eq!(
            state.select_candidate(&policy, "playout:1", at(9), &candidates),
            Some(0)
        );
    }

    #[test]
    fn duration_cooldown_uses_unit_finish_and_longer_threshold_wins() {
        let policy = custom_policy(Some(1), Some("00:10:00"));
        let mut state = SchedulingCooldownState::default();
        let first_tags = tags(&["show:1"]);
        let other_tags = tags(&["show:2"]);
        state.record_unit(
            &policy,
            "playout:1",
            at(0),
            &SchedulingCooldownUnit::single(1, first_tags.clone(), Duration::minutes(5)),
            false,
        );
        let candidates = [
            SchedulingCooldownCandidate {
                media_item_id: 2,
                tags: &first_tags,
            },
            SchedulingCooldownCandidate {
                media_item_id: 3,
                tags: &other_tags,
            },
        ];
        assert_eq!(
            state.select_candidate(&policy, "playout:1", at(14), &candidates),
            Some(1)
        );
        state.record_unit(
            &policy,
            "playout:1",
            at(5),
            &SchedulingCooldownUnit::single(3, other_tags.clone(), Duration::minutes(1)),
            false,
        );
        assert_eq!(
            state.select_candidate(&policy, "playout:1", at(15), &candidates),
            Some(0)
        );
    }

    #[test]
    fn impossible_constraints_relax_by_penalty_then_base_rank() {
        let policy = custom_policy(Some(5), None);
        let mut state = SchedulingCooldownState::default();
        let many_tags = tags(&["artist:1", "tag:live"]);
        let one_tag = tags(&["artist:2"]);
        for (id, item_tags) in [(1, many_tags.clone()), (2, one_tag.clone())] {
            state.record_unit(
                &policy,
                "playout:1",
                at(0),
                &SchedulingCooldownUnit::single(id, item_tags, Duration::minutes(1)),
                false,
            );
        }
        let candidates = [
            SchedulingCooldownCandidate {
                media_item_id: 9,
                tags: &many_tags,
            },
            SchedulingCooldownCandidate {
                media_item_id: 8,
                tags: &one_tag,
            },
        ];
        assert_eq!(
            state.select_candidate(&policy, "playout:1", at(2), &candidates),
            Some(1)
        );
    }

    #[test]
    fn grouped_unit_unions_tags_and_duration() {
        let unit = SchedulingCooldownUnit::grouped([
            SchedulingCooldownUnit::single(1, tags(&["show:1"]), Duration::minutes(20)),
            SchedulingCooldownUnit::single(2, tags(&["show:1", "season:2"]), Duration::minutes(22)),
        ]);
        assert_eq!(unit.media_item_ids, vec![1, 2]);
        assert_eq!(unit.tags, tags(&["show:1", "season:2"]));
        assert_eq!(unit.duration, Duration::minutes(42));
    }

    #[test]
    fn state_round_trip_is_versioned_and_deterministic() {
        let policy = custom_policy(Some(3), Some("00:05:00"));
        let mut state = SchedulingCooldownState::default();
        state.record_unit(
            &policy,
            "content_source:music",
            at(0),
            &SchedulingCooldownUnit::single(1, tags(&["artist:1"]), Duration::minutes(3)),
            false,
        );
        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("\"version\":1"));
        assert_eq!(
            serde_json::from_str::<SchedulingCooldownState>(&json).unwrap(),
            state
        );
    }
}
