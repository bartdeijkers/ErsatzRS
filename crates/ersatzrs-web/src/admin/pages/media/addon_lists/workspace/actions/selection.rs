use super::{Fields, Result, TextKey};
use ersatzrs_core::addon_list_selection::{AddonListSelectionPolicy, AddonListSelectionSettings};

pub(super) fn parse_selection(
    changes: &Fields,
    saved: &AddonListSelectionSettings,
) -> Result<AddonListSelectionSettings> {
    let policy = saved.policy.clone().unwrap_or_default();
    let mut fields = changes.clone();
    let defaults = [
        ("selection_enabled", saved.policy.is_some().to_string()),
        ("selection_paused", saved.refresh_paused.to_string()),
        (
            "selection_age_days",
            policy.age_days.map(|n| n.to_string()).unwrap_or_default(),
        ),
        (
            "selection_newest_count",
            policy
                .newest_count
                .map(|n| n.to_string())
                .unwrap_or_default(),
        ),
        (
            "selection_minimum_seconds",
            policy
                .minimum_seconds
                .map(|n| n.to_string())
                .unwrap_or_default(),
        ),
        (
            "selection_maximum_seconds",
            policy
                .maximum_seconds
                .map(|n| n.to_string())
                .unwrap_or_default(),
        ),
        ("selection_include_terms", policy.include_terms.join("\n")),
        ("selection_exclude_terms", policy.exclude_terms.join("\n")),
    ];
    for (key, value) in defaults {
        fields.entry(key.to_owned()).or_insert(value);
    }
    let number = |name: &str| -> Result<Option<u64>> {
        fields
            .get(name)
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.parse::<u64>().map_err(|_| TextKey::AddonSelectionInvalid))
            .transpose()
    };
    let small = |name| -> Result<Option<u32>> {
        number(name)?
            .map(|n| u32::try_from(n).map_err(|_| TextKey::AddonSelectionInvalid))
            .transpose()
    };
    let terms = |name: &str| {
        fields.get(name).map_or_else(Vec::new, |s| {
            s.lines()
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(str::to_owned)
                .collect()
        })
    };
    let policy = if fields.get("selection_enabled").is_some_and(|v| v == "true") {
        let policy = AddonListSelectionPolicy {
            age_days: small("selection_age_days")?,
            newest_count: small("selection_newest_count")?,
            minimum_seconds: number("selection_minimum_seconds")?,
            maximum_seconds: number("selection_maximum_seconds")?,
            include_terms: terms("selection_include_terms"),
            exclude_terms: terms("selection_exclude_terms"),
        };
        policy
            .validate()
            .map_err(|_| TextKey::AddonSelectionInvalid)?;
        Some(policy)
    } else {
        None
    };
    Ok(AddonListSelectionSettings {
        policy,
        refresh_paused: fields.get("selection_paused").is_some_and(|v| v == "true"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_partial_edits_preserve_saved_filters_pause_and_explicit_clears() {
        let saved = AddonListSelectionSettings {
            policy: Some(AddonListSelectionPolicy {
                age_days: Some(365),
                newest_count: Some(3),
                include_terms: vec!["Wild Ride".into()],
                ..Default::default()
            }),
            refresh_paused: true,
        };
        let mut fields = Fields::from([("selection_newest_count".into(), "2".into())]);
        let changed = parse_selection(&fields, &saved).unwrap();
        assert!(changed.refresh_paused);
        assert_eq!(changed.policy.as_ref().unwrap().age_days, Some(365));
        assert_eq!(
            changed.policy.as_ref().unwrap().include_terms,
            ["Wild Ride"]
        );
        assert_eq!(changed.policy.as_ref().unwrap().newest_count, Some(2));
        fields.insert("selection_age_days".into(), String::new());
        assert_eq!(
            parse_selection(&fields, &saved)
                .unwrap()
                .policy
                .unwrap()
                .age_days,
            None
        );
        fields.insert("selection_paused".into(), "false".into());
        assert!(!parse_selection(&fields, &saved).unwrap().refresh_paused);
        fields.insert("selection_enabled".into(), "false".into());
        assert!(parse_selection(&fields, &saved).unwrap().policy.is_none());
    }
}
