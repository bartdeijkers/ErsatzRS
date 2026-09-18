use super::{
    enum_value, field, liveness, number, parse, row_has_edits, AddonMediaListContentKind,
    AddonMediaListPresentationKind, AppState, Draft, Fields, RemoteStreamLiveness, Result, TextKey,
};
use crate::admin::server_fns::addon_media_lists::{
    AddonMediaListDraftDiffRow, AddonMediaListItemRow,
};
use chrono::Datelike as _;
use std::collections::BTreeSet;

pub(super) async fn preview_bulk(
    state: &AppState,
    d: &mut Draft,
    fields: &Fields,
    intent: &str,
) -> Result<()> {
    let all = state
        .application
        .list_addon_media_list_items(d.list_id)
        .await
        .map_err(|_| TextKey::NoItems)?;
    let form = state
        .application
        .get_addon_media_list(d.list_id)
        .await
        .map_err(|_| TextKey::NoItems)?
        .ok_or(TextKey::AddonMediaListNotFound)?;
    if form.draft_revision != d.revision {
        return Err(TextKey::AddonWorkspaceRetained);
    }
    let diff: AddonMediaListDraftDiffRow = state
        .application
        .addon_media_list_saved_draft_diff(d.list_id)
        .await
        .map_err(|_| TextKey::DraftChanges)?
        .into();
    let all: Vec<AddonMediaListItemRow> = all.into_iter().map(Into::into).collect();
    let changed = preview(d, fields, intent, &all, form.presentation_kind, &diff)?;
    d.undo = Some(d.groups.clone());
    d.groups.insert("items".into(), changed);
    Ok(())
}

#[expect(
    clippy::too_many_lines,
    reason = "Numbering algorithms share one ordered scope, eligibility and occupied-number set"
)]
fn preview(
    d: &Draft,
    fields: &Fields,
    intent: &str,
    all: &[AddonMediaListItemRow],
    presentation: AddonMediaListPresentationKind,
    diff: &AddonMediaListDraftDiffRow,
) -> Result<Fields> {
    if matches!(intent, "include-selected" | "exclude-selected") {
        let mut changed = fields.clone();
        let targets: Vec<_> = all
            .iter()
            .filter(|row| d.selected.contains(&row.id))
            .collect();
        if targets.is_empty() {
            return Err(TextKey::AddonWorkspaceSelectFirst);
        }
        for row in targets {
            if !row_has_edits(fields, row.id) {
                changed.insert(
                    format!("review_snapshot_{}", row.id),
                    super::super::review_snapshot(row),
                );
            }
            changed.insert(
                format!("included_{}", row.id),
                (intent == "include-selected").to_string(),
            );
        }
        return Ok(changed);
    }
    let filtered: Vec<_> = all
        .iter()
        .filter(|r| super::super::matches_row(d, r, presentation, diff))
        .collect();
    let size = if d.page_size == 0 {
        filtered.len().max(1)
    } else {
        d.page_size
    };
    let page = d.page.min(filtered.len().div_ceil(size).saturating_sub(1));
    let selected = match field(fields, "bulk_scope") {
        "selected" => all
            .iter()
            .filter(|r| d.selected.contains(&r.id))
            .collect::<Vec<_>>(),
        "all" => all.iter().collect(),
        "page" => filtered.into_iter().skip(page * size).take(size).collect(),
        "matching" => filtered,
        _ => return Err(TextKey::Scope),
    };
    let eligible: Vec<_> = selected
        .into_iter()
        .filter(|r| {
            intent == "classify"
                || (r.included
                    && r.parent_provider_id.is_none()
                    && r.effective_liveness != RemoteStreamLiveness::Live)
        })
        .collect();
    let endpoint = |name: &str, default: usize| -> Result<usize> {
        if field(fields, name).is_empty() {
            Ok(default)
        } else {
            let id = parse::<i64>(fields, name)?;
            eligible
                .iter()
                .position(|r| r.id == id)
                .ok_or(TextKey::Range)
        }
    };
    let start = endpoint("start_item", 0)?;
    let end = endpoint("end_item", eligible.len().saturating_sub(1))?;
    if end < start {
        return Err(TextKey::Range);
    }
    let count = if field(fields, "bulk_count").is_empty() {
        eligible.len()
    } else {
        parse::<usize>(fields, "bulk_count")?
    };
    if count == 0 {
        return Err(TextKey::NumberToAssign);
    }
    let effective = |row: &AddonMediaListItemRow| -> Result<(Option<i32>, Option<i32>)> {
        let season = format!("season_{}", row.id);
        let episode = format!("episode_{}", row.id);
        Ok((
            if fields.contains_key(&season) {
                number(fields, &season)?
            } else {
                row.season
            },
            if fields.contains_key(&episode) {
                number(fields, &episode)?
            } else {
                row.episode
            },
        ))
    };
    let parse_length = |name: &str| -> Result<Option<u64>> {
        let value = field(fields, name).trim();
        if value.is_empty() {
            return Ok(None);
        }
        let (minutes, seconds) = value
            .split_once(':')
            .ok_or(TextKey::AddonWorkspaceLengthHelp)?;
        let minutes = minutes
            .parse::<u64>()
            .map_err(|_| TextKey::AddonWorkspaceLengthHelp)?;
        let seconds = seconds
            .parse::<u64>()
            .map_err(|_| TextKey::AddonWorkspaceLengthHelp)?;
        if seconds >= 60 {
            return Err(TextKey::AddonWorkspaceLengthHelp);
        }
        Ok(Some(
            minutes
                .checked_mul(60)
                .and_then(|v| v.checked_add(seconds))
                .ok_or(TextKey::AddonWorkspaceLengthHelp)?,
        ))
    };
    let minimum = parse_length("minimum_length")?;
    let maximum = parse_length("maximum_length")?;
    if minimum.zip(maximum).is_some_and(|(a, b)| a > b) {
        return Err(TextKey::AddonWorkspaceLengthHelp);
    }
    let mut targets = Vec::new();
    for row in eligible.into_iter().skip(start).take(end - start + 1) {
        if intent != "classify" && (minimum.is_some() || maximum.is_some()) {
            if let Some(runtime) = row.duration_seconds {
                if minimum.is_some_and(|n| runtime < n) || maximum.is_some_and(|n| runtime > n) {
                    continue;
                }
            } else if field(fields, "include_unknown_length") != "true" {
                continue;
            }
        }
        let pair = effective(row)?;
        if intent != "number"
            || field(fields, "replace_existing") == "true"
            || field(fields, "season_only") == "true"
            || pair.0.is_none()
            || pair.1.is_none_or(|n| n < 1)
        {
            targets.push(row);
        }
        if targets.len() == count {
            break;
        }
    }
    if targets.is_empty() {
        return Err(TextKey::NoItems);
    }
    let ids: BTreeSet<_> = targets.iter().map(|r| r.id).collect();
    let mut occupied = BTreeSet::new();
    for row in all
        .iter()
        .filter(|r| r.included && r.parent_provider_id.is_none() && !ids.contains(&r.id))
    {
        if let (Some(s), Some(e)) = effective(row)? {
            occupied.insert((s, e));
        }
    }
    let mut season = if matches!(intent, "number" | "auto-number") {
        parse::<i32>(fields, "bulk_season")?
    } else {
        0
    };
    let mut episode = if intent == "number" {
        parse::<i32>(fields, "bulk_episode")?
    } else {
        1
    };
    let gap = if intent == "auto-number" {
        parse::<i64>(fields, "gap_days")?
    } else {
        1
    };
    if season < 0 || episode < 0 || gap < 1 {
        return Err(TextKey::NumberEpisodes);
    }
    let season_per_year = intent == "auto-number" && field(fields, "season_per_year") == "true";
    if intent == "auto-number" {
        targets = super::super::super::order_by_first_aired(targets);
    }
    let mut previous_date: Option<chrono::NaiveDate> = None;
    let mut changed = fields.clone();
    for row in targets {
        if !row_has_edits(fields, row.id) {
            changed.insert(
                format!("review_snapshot_{}", row.id),
                super::super::review_snapshot(row),
            );
        }
        if intent == "classify" {
            if field(fields, "bulk_kind") != "__unchanged" {
                enum_value::<AddonMediaListContentKind>(fields, "bulk_kind")?;
                changed.insert(
                    format!("kind_{}", row.id),
                    field(fields, "bulk_kind").into(),
                );
            }
            if field(fields, "bulk_liveness") != "__unchanged" {
                liveness(fields, "bulk_liveness")?;
                changed.insert(
                    format!("liveness_{}", row.id),
                    field(fields, "bulk_liveness").into(),
                );
            }
        } else if intent == "clear-numbering" {
            changed.insert(format!("season_{}", row.id), String::new());
            changed.insert(format!("episode_{}", row.id), String::new());
        } else {
            if intent == "auto-number" {
                let date = super::super::super::parse_first_aired(row.first_aired.as_deref());
                if let (Some(previous), Some(current)) = (previous_date, date) {
                    let new_season = if season_per_year {
                        current.year() != previous.year()
                    } else {
                        current.signed_duration_since(previous).num_days() >= gap
                    };
                    if new_season {
                        season = season.checked_add(1).ok_or(TextKey::NumberEpisodes)?;
                        episode = 1;
                    }
                }
                if date.is_some() {
                    previous_date = date;
                }
            }
            let assigned = if field(fields, "season_only") == "true" && intent == "number" {
                effective(row)?
                    .1
                    .ok_or(TextKey::BlockerSeasonEpisodeMissing)?
            } else {
                while occupied.contains(&(season, episode)) {
                    episode = episode.checked_add(1).ok_or(TextKey::NumberEpisodes)?;
                }
                episode
            };
            changed.insert(format!("season_{}", row.id), season.to_string());
            changed.insert(format!("episode_{}", row.id), assigned.to_string());
            occupied.insert((season, assigned));
            episode = assigned.checked_add(1).ok_or(TextKey::NumberEpisodes)?;
        }
    }
    Ok(changed)
}

pub(super) async fn select_options(state: &AppState, d: &mut Draft, fields: &Fields) -> Result<()> {
    let form = state
        .application
        .get_addon_media_list(d.list_id)
        .await
        .map_err(|_| TextKey::Scope)?
        .ok_or(TextKey::AddonMediaListNotFound)?;
    let diff: AddonMediaListDraftDiffRow = state
        .application
        .addon_media_list_saved_draft_diff(d.list_id)
        .await
        .map_err(|_| TextKey::Scope)?
        .into();
    let rows: Vec<AddonMediaListItemRow> = state
        .application
        .list_addon_media_list_items(d.list_id)
        .await
        .map_err(|_| TextKey::Scope)?
        .into_iter()
        .map(Into::into)
        .collect();
    let matching: Vec<_> = rows
        .iter()
        .filter(|r| super::super::matches_row(d, r, form.presentation_kind, &diff))
        .collect();
    let size = if d.page_size == 0 {
        matching.len().max(1)
    } else {
        d.page_size
    };
    let page = d.page.min(matching.len().div_ceil(size).saturating_sub(1));
    let ids: Vec<_> = match field(fields, "bulk_scope") {
        "selected" => rows
            .iter()
            .filter(|r| d.selected.contains(&r.id))
            .map(|r| r.id)
            .collect(),
        "all" => rows.iter().map(|r| r.id).collect(),
        "page" => matching
            .into_iter()
            .skip(page * size)
            .take(size)
            .map(|r| r.id)
            .collect(),
        "matching" => matching.into_iter().map(|r| r.id).collect(),
        _ => return Err(TextKey::Scope),
    };
    if ids.is_empty() {
        return Err(TextKey::NoItems);
    }
    let group = d.groups.entry("items".into()).or_default();
    group.retain(|key, _| !key.starts_with("option_"));
    group.insert(
        "option_targets".into(),
        ids.iter().map(i64::to_string).collect::<Vec<_>>().join(","),
    );
    Ok(())
}

pub(super) async fn select_range(state: &AppState, d: &mut Draft, fields: &Fields) -> Result<()> {
    let form = state
        .application
        .get_addon_media_list(d.list_id)
        .await
        .map_err(|_| TextKey::Range)?
        .ok_or(TextKey::AddonMediaListNotFound)?;
    let diff: AddonMediaListDraftDiffRow = state
        .application
        .addon_media_list_saved_draft_diff(d.list_id)
        .await
        .map_err(|_| TextKey::Range)?
        .into();
    let rows: Vec<AddonMediaListItemRow> = state
        .application
        .list_addon_media_list_items(d.list_id)
        .await
        .map_err(|_| TextKey::Range)?
        .into_iter()
        .map(Into::into)
        .collect();
    let rows: Vec<_> = rows
        .iter()
        .filter(|r| super::super::matches_row(d, r, form.presentation_kind, &diff))
        .collect();
    let endpoint = |name: &str, default: usize| -> Result<usize> {
        if field(fields, name).is_empty() {
            Ok(default)
        } else {
            let id = parse::<i64>(fields, name)?;
            rows.iter().position(|r| r.id == id).ok_or(TextKey::Range)
        }
    };
    let start = endpoint("start_item", 0)?;
    let end = endpoint("end_item", rows.len().saturating_sub(1))?;
    if end < start || rows.is_empty() {
        return Err(TextKey::Range);
    }
    d.selected.extend(
        rows.into_iter()
            .skip(start)
            .take(end - start + 1)
            .map(|r| r.id),
    );
    d.groups
        .entry("items".into())
        .or_default()
        .insert("bulk_scope".into(), "selected".into());
    d.expanded_bulk = true;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        preview, AddonMediaListDraftDiffRow, AddonMediaListItemRow, AddonMediaListPresentationKind,
        Draft, Fields, Result,
    };
    use std::collections::BTreeSet;
    fn row(id: i64, season: Option<i32>, episode: Option<i32>) -> AddonMediaListItemRow {
        AddonMediaListItemRow {
            id,
            display_title: format!("Item {id}"),
            season,
            episode,
            included: true,
            ..Default::default()
        }
    }
    fn fields() -> Fields {
        [
            ("bulk_scope", "selected"),
            ("bulk_season", "1"),
            ("bulk_episode", "1"),
            ("replace_existing", "true"),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), v.into()))
        .collect()
    }
    fn run(d: &Draft, f: &Fields, rows: &[AddonMediaListItemRow]) -> Result<Fields> {
        preview(
            d,
            f,
            "number",
            rows,
            AddonMediaListPresentationKind::TelevisionShow,
            &AddonMediaListDraftDiffRow::default(),
        )
    }
    #[test]
    fn inclusion_uses_selection_not_numbering_filters_and_preserves_review_fences() {
        let rows = vec![
            row(1, None, None),
            row(2, Some(3), Some(4)),
            row(3, None, None),
        ];
        let d = Draft {
            selected: BTreeSet::from([2, 3]),
            page_size: 1,
            ..Default::default()
        };
        let mut fields = fields();
        fields.insert("minimum_length".into(), "not a duration".into());
        fields.insert("season_2".into(), "5".into());
        fields.insert("review_snapshot_2".into(), "retained fence".into());
        let excluded = preview(
            &d,
            &fields,
            "exclude-selected",
            &rows,
            AddonMediaListPresentationKind::TelevisionShow,
            &AddonMediaListDraftDiffRow::default(),
        )
        .unwrap();
        assert!(!excluded.contains_key("included_1"));
        assert_eq!(excluded["included_2"], "false");
        assert_eq!(excluded["included_3"], "false");
        assert_eq!(excluded["season_2"], "5");
        assert_eq!(excluded["review_snapshot_2"], "retained fence");
        assert!(excluded.contains_key("review_snapshot_3"));
        let included = preview(
            &d,
            &excluded,
            "include-selected",
            &rows,
            AddonMediaListPresentationKind::TelevisionShow,
            &AddonMediaListDraftDiffRow::default(),
        )
        .unwrap();
        assert_eq!(included["included_2"], "true");
        assert_eq!(included["included_3"], "true");
        assert_eq!(d.selected, BTreeSet::from([2, 3]));
    }

    #[test]
    fn selected_numbering_skips_occupied_pairs_and_preserves_other_rows() {
        let d = Draft {
            selected: BTreeSet::from([2, 4]),
            page_size: 10,
            ..Default::default()
        };
        let rows = vec![
            row(1, Some(1), Some(1)),
            row(2, None, None),
            row(3, Some(1), Some(3)),
            row(4, None, None),
        ];
        let result = run(&d, &fields(), &rows).unwrap();
        assert_eq!(result.get("episode_2").map(String::as_str), Some("2"));
        assert_eq!(result.get("episode_4").map(String::as_str), Some("4"));
        assert!(!result.contains_key("season_1"));
        assert!(!result.contains_key("episode_3"));
    }
    #[test]
    fn auto_numbering_a_newest_first_list_starts_with_the_oldest_item() {
        let d = Draft {
            selected: BTreeSet::from([1, 2, 3]),
            page_size: 10,
            ..Default::default()
        };
        let rows = [(1, "2026-08-22"), (2, "2026-07-29"), (3, "2013-01-23")]
            .into_iter()
            .map(|(id, first_aired)| AddonMediaListItemRow {
                first_aired: Some(first_aired.into()),
                ..row(id, Some(1), Some(1))
            })
            .collect::<Vec<_>>();
        let mut f = fields();
        f.insert("gap_days".into(), "90".into());
        let result = preview(
            &d,
            &f,
            "auto-number",
            &rows,
            AddonMediaListPresentationKind::TelevisionShow,
            &AddonMediaListDraftDiffRow::default(),
        )
        .unwrap();
        let pair = |id: i64| {
            (
                result[&format!("season_{id}")].clone(),
                result[&format!("episode_{id}")].clone(),
            )
        };
        assert_eq!(pair(3), ("1".into(), "1".into()));
        assert_eq!(pair(2), ("2".into(), "1".into()));
        assert_eq!(pair(1), ("2".into(), "2".into()));
    }
    #[test]
    fn auto_numbering_can_start_a_season_each_calendar_year() {
        let d = Draft {
            selected: BTreeSet::from([1, 2, 3, 4, 5]),
            page_size: 10,
            ..Default::default()
        };
        // Newest first; item 3 is undated and follows its provider neighbour.
        let rows = [
            (1, Some("2015-01-02")),
            (2, Some("2014-12-30")),
            (3, None),
            (4, Some("2014-01-05")),
            (5, Some("2013-12-31")),
        ]
        .into_iter()
        .map(|(id, first_aired)| AddonMediaListItemRow {
            first_aired: first_aired.map(Into::into),
            ..row(id, None, None)
        })
        .collect::<Vec<_>>();
        let mut f = fields();
        f.insert("gap_days".into(), "90".into());
        f.insert("season_per_year".into(), "true".into());
        let result = preview(
            &d,
            &f,
            "auto-number",
            &rows,
            AddonMediaListPresentationKind::TelevisionShow,
            &AddonMediaListDraftDiffRow::default(),
        )
        .unwrap();
        let pair = |id: i64| {
            (
                result[&format!("season_{id}")].as_str(),
                result[&format!("episode_{id}")].as_str(),
            )
        };
        assert_eq!(pair(5), ("1", "1"));
        assert_eq!(pair(4), ("2", "1"));
        assert_eq!(pair(2), ("2", "2"));
        assert_eq!(pair(3), ("2", "3"));
        assert_eq!(pair(1), ("3", "1"));
    }
    #[test]
    fn invalid_range_or_season_only_pair_returns_no_partial_preview() {
        let d = Draft {
            selected: BTreeSet::from([2, 4]),
            page_size: 10,
            ..Default::default()
        };
        let rows = vec![row(2, None, None), row(4, Some(1), Some(3))];
        let mut f = fields();
        f.insert("start_item".into(), "999".into());
        assert!(run(&d, &f, &rows).is_err());
        f.remove("start_item");
        f.insert("season_only".into(), "true".into());
        assert!(run(&d, &f, &rows).is_err());
        assert!(d.groups.is_empty());
    }
    #[test]
    fn missing_only_count_is_applied_after_filtering_numbered_rows() {
        let d = Draft {
            selected: BTreeSet::from([1, 2, 3, 4]),
            page_size: 10,
            ..Default::default()
        };
        let rows = vec![
            row(1, Some(1), Some(1)),
            row(2, None, None),
            row(3, Some(1), Some(3)),
            row(4, None, None),
        ];
        let mut f = fields();
        f.insert("replace_existing".into(), "false".into());
        f.insert("bulk_count".into(), "2".into());
        let result = run(&d, &f, &rows).unwrap();
        assert!(result.contains_key("episode_2"));
        assert!(result.contains_key("episode_4"));
        assert!(!result.contains_key("episode_1"));
    }
    #[test]
    fn all_page_size_and_clamped_page_keep_every_matching_item() {
        let d = Draft {
            page_size: 0,
            page: usize::MAX,
            query: "Item".into(),
            ..Default::default()
        };
        let rows = (1..=165).map(|id| row(id, None, None)).collect::<Vec<_>>();
        let mut f = fields();
        f.insert("bulk_scope".into(), "page".into());
        let result = run(&d, &f, &rows).unwrap();
        assert_eq!(
            result.keys().filter(|k| k.starts_with("episode_")).count(),
            165
        );
    }
}
