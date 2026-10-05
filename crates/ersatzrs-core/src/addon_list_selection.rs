//! Bounded, provider-neutral selection of an already discovered overview.

use chrono::{DateTime, Duration, Utc};
pub use ersatzrs_domain::addon_list_selection::*;

/// Initial import choices captured before a durable worker can publish.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AddonListImportOptions {
    pub detail_policy: ersatzrs_domain::AddonMediaListDetailPolicy,
    pub delivery_mode: ersatzrs_domain::AddonMediaListDeliveryMode,
    pub initial_selection_review: bool,
}

/// Evaluate without mutating the overview or consulting a provider.
/// Pins bypass filters and do not consume the newest-N allowance.
///
/// # Errors
/// Returns invalid-policy or excessive-overview errors.
pub fn preview(
    policy: Option<&AddonListSelectionPolicy>,
    items: &[AddonListSelectionItem],
    now: DateTime<Utc>,
) -> Result<AddonListSelectionPreview, &'static str> {
    if items.len() > 10_000 {
        return Err("selection preview exceeds the 10000-item bound");
    }
    if let Some(policy) = policy {
        policy.validate()?;
    }
    let cutoff_at = policy
        .and_then(|p| p.age_days)
        .map(|days| {
            now.checked_sub_signed(Duration::days(i64::from(days)))
                .ok_or("selection cutoff is outside the supported clock range")
        })
        .transpose()?;
    let mut ordered = items.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|item| (std::cmp::Reverse(item.published_at), item.rank, item.id));
    let mut selected = 0_u32;
    let mut decisions = std::collections::BTreeMap::new();
    for item in ordered {
        let mut reason = classify(policy, item, cutoff_at, now);
        let at_limit = policy
            .and_then(|p| p.newest_count)
            .is_some_and(|n| selected >= n);
        if at_limit
            && matches!(
                reason,
                AddonListSelectionReason::Accepted | AddonListSelectionReason::MissingDuration
            )
        {
            // Older dated items cannot enter newest N, even if duration is enriched.
            reason = AddonListSelectionReason::NewestLimit;
        } else if reason == AddonListSelectionReason::Accepted {
            selected += 1;
        }
        decisions.insert(
            item.id,
            AddonListSelectionDecision {
                item_id: item.id,
                title: item.title.clone(),
                accepted: matches!(
                    reason,
                    AddonListSelectionReason::Accepted | AddonListSelectionReason::Pinned
                ),
                reason,
            },
        );
    }
    let decisions = items
        .iter()
        .filter_map(|item| decisions.remove(&item.id))
        .collect::<Vec<_>>();
    Ok(AddonListSelectionPreview {
        evaluated_at: now,
        cutoff_at,
        accepted_count: decisions.iter().filter(|d| d.accepted).count(),
        decisions,
    })
}

fn classify(
    policy: Option<&AddonListSelectionPolicy>,
    item: &AddonListSelectionItem,
    cutoff: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> AddonListSelectionReason {
    use AddonListSelectionReason as R;
    if let Some(pinned) = item.manual {
        return if pinned { R::Pinned } else { R::Excluded };
    }
    let Some(policy) = policy else {
        return if item.legacy_included {
            R::Accepted
        } else {
            R::Excluded
        };
    };
    let title = item.title.to_lowercase();
    if policy
        .exclude_terms
        .iter()
        .any(|term| title.contains(&term.trim().to_lowercase()))
    {
        return R::TitleExcluded;
    }
    if !policy.include_terms.is_empty()
        && !policy
            .include_terms
            .iter()
            .any(|term| title.contains(&term.trim().to_lowercase()))
    {
        return R::TitleNotIncluded;
    }
    if policy.newest_count.is_some() && item.published_at.is_none() {
        return R::MissingDate;
    }
    if item.published_at.is_some_and(|date| date > now) {
        return R::FutureDate;
    }
    if let Some(cutoff) = cutoff {
        let Some(date) = item.published_at else {
            return R::MissingDate;
        };
        if date > now {
            return R::FutureDate;
        }
        if date < cutoff {
            return R::TooOld;
        }
    }
    if policy.minimum_seconds.is_some() || policy.maximum_seconds.is_some() {
        let Some(duration) = item.duration_seconds else {
            return R::MissingDuration;
        };
        if policy.minimum_seconds.is_some_and(|n| duration < n) {
            return R::TooShort;
        }
        if policy.maximum_seconds.is_some_and(|n| duration > n) {
            return R::TooLong;
        }
    }
    R::Accepted
}

#[cfg(test)]
mod tests {
    use super::*;
    use AddonListSelectionReason as R;

    fn item(id: i64, now: DateTime<Utc>) -> AddonListSelectionItem {
        AddonListSelectionItem {
            id,
            rank: i32::try_from(id).unwrap(),
            title: "Épisode Wild Ride".into(),
            published_at: Some(now),
            duration_seconds: Some(60),
            manual: None,
            legacy_included: true,
        }
    }

    #[test]
    fn selection_boundaries_pins_exclusions_and_missing_facts() {
        let now = DateTime::parse_from_rfc3339("2026-10-05T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let policy = AddonListSelectionPolicy {
            age_days: Some(365),
            newest_count: Some(1),
            minimum_seconds: Some(60),
            maximum_seconds: Some(120),
            include_terms: vec!["ÉPISODE".into()],
            exclude_terms: vec!["promo".into()],
        };
        let mut items = (1..=12).map(|id| item(id, now)).collect::<Vec<_>>();
        items[0].published_at = Some(now - Duration::days(365));
        items[1].published_at = Some(now - Duration::days(365) - Duration::seconds(1));
        items[2].published_at = Some(now + Duration::seconds(1));
        items[3].published_at = None;
        items[4].duration_seconds = None;
        items[5].duration_seconds = Some(59);
        items[6].duration_seconds = Some(121);
        items[7].title = "Unrelated".into();
        items[8].title = "Épisode PROMO".into();
        items[9].manual = Some(false);
        items[10].manual = Some(true);
        items[10].published_at = None;
        items[10].duration_seconds = None;
        let result = preview(Some(&policy), &items, now).unwrap();
        assert_eq!(
            result
                .decisions
                .iter()
                .map(|d| d.reason)
                .collect::<Vec<_>>(),
            [
                R::NewestLimit,
                R::TooOld,
                R::FutureDate,
                R::MissingDate,
                R::MissingDuration,
                R::TooShort,
                R::TooLong,
                R::TitleNotIncluded,
                R::TitleExcluded,
                R::Excluded,
                R::Pinned,
                R::Accepted
            ]
        );
        assert_eq!(result.accepted_count, 2);
        let boundary = preview(Some(&policy), &items[..1], now).unwrap();
        assert!(boundary.decisions[0].accepted);
    }

    #[test]
    fn selection_newest_limit_avoids_irrelevant_duration_enrichment() {
        let now = Utc::now();
        let mut older = item(2, now - Duration::days(1));
        older.duration_seconds = None;
        let policy = AddonListSelectionPolicy {
            newest_count: Some(1),
            minimum_seconds: Some(60),
            ..Default::default()
        };
        let result = preview(Some(&policy), &[item(1, now), older], now).unwrap();
        assert_eq!(result.decisions[0].reason, R::Accepted);
        assert_eq!(result.decisions[1].reason, R::NewestLimit);
    }

    #[test]
    fn selection_stable_ties_validation_and_legacy_defaults() {
        let now = Utc::now();
        let items = [item(2, now), item(1, now)];
        let policy = AddonListSelectionPolicy {
            newest_count: Some(1),
            ..Default::default()
        };
        let result = preview(Some(&policy), &items, now).unwrap();
        assert!(!result.decisions[0].accepted);
        assert!(result.decisions[1].accepted);
        let mut undated = item(3, now);
        undated.published_at = None;
        assert_eq!(
            preview(Some(&policy), &[undated], now).unwrap().decisions[0].reason,
            AddonListSelectionReason::MissingDate
        );
        let mut legacy = item(3, now);
        legacy.legacy_included = false;
        assert!(!preview(None, &[legacy], now).unwrap().decisions[0].accepted);
        assert!(AddonListSelectionPolicy {
            minimum_seconds: Some(2),
            maximum_seconds: Some(1),
            ..Default::default()
        }
        .validate()
        .is_err());
        assert!(AddonListSelectionPolicy {
            include_terms: vec!["x".repeat(257)],
            ..Default::default()
        }
        .validate()
        .is_err());
        assert!(preview(
            Some(&AddonListSelectionPolicy {
                age_days: Some(1),
                ..Default::default()
            }),
            &[],
            DateTime::<Utc>::MIN_UTC
        )
        .is_err());
    }
}
