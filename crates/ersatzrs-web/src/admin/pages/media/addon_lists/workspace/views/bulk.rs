use leptos::prelude::*;
use std::collections::BTreeMap;

use super::{
    button, checkbox, content_kind_value, input, select, tr, value, AddonMediaListItemRow, TextKey,
    Workspace, FORM,
};

/// The numbering tools as three panels: which items change, manual numbering
/// and season detection. Each numbering action reads only its own panel plus
/// the shared scope, range and length filter.
pub(super) fn numbering_panels(w: &Workspace, rows: &[&AddonMediaListItemRow]) -> AnyView {
    let d = &w.submission.draft;
    let endpoint = |name: &str, label: TextKey, open: TextKey| {
        let choices = std::iter::once((String::new(), tr(open).to_owned()))
            .chain(
                rows.iter()
                    .map(|r| (r.id.to_string(), r.display_title.clone())),
            )
            .collect();
        select(name, label, d.field("items", name, ""), choices)
    };
    let scopes = [
        ("selected", TextKey::AddonWorkspaceSelected),
        ("page", TextKey::AddonWorkspacePage),
        ("matching", TextKey::AddonWorkspaceMatching),
        ("all", TextKey::All),
    ]
    .into_iter()
    .map(|(scope, key)| (scope.to_owned(), tr(key).to_owned()))
    .collect();
    let checked = |name: &str| d.field("items", name, "false") == "true";
    view! {
        <div class="bulk-task-grid bulk-numbering-panels">
            <section class="bulk-panel" aria-labelledby="bulk-panel-scope"><h3 id="bulk-panel-scope">{tr(TextKey::BulkItemsToChange)}</h3>
                {select("bulk_scope",TextKey::Scope,d.field("items","bulk_scope","selected"),scopes)}
                <div class="grid-2">{endpoint("start_item",TextKey::RangeStart,TextKey::FirstIncludedItem)}{endpoint("end_item",TextKey::RangeEnd,TextKey::AllRemaining)}</div>
                <div class="action-row">{button("select-range",TextKey::Select,false)}{button("clear-selection",TextKey::ClearSelection,false)}</div>
                <details class="length-filter"><summary>{tr(TextKey::AddonWorkspaceLengthFilter)}</summary><div class="length-fields">
                    {input("minimum_length",TextKey::AddonWorkspaceMinimumLength,d.field("items","minimum_length",""),"text")}{input("maximum_length",TextKey::AddonWorkspaceMaximumLength,d.field("items","maximum_length",""),"text")}
                </div>{checkbox("include_unknown_length",TextKey::AddonWorkspaceUnknownLength,checked("include_unknown_length"))}<p>{tr(TextKey::AddonWorkspaceLengthHelp)}</p></details>
                <p class="bulk-help">{tr(TextKey::BulkScopeHelp)}</p>
            </section>
            <section class="bulk-panel" aria-labelledby="bulk-panel-number"><h3 id="bulk-panel-number">{tr(TextKey::NumberEpisodes)}</h3>
                <div class="grid-3">{input("bulk_season",TextKey::Season,d.field("items","bulk_season","1"),"number")}{input("bulk_episode",TextKey::Episode,d.field("items","bulk_episode","1"),"number")}{input("bulk_count",TextKey::Count,d.field("items","bulk_count",""),"number")}</div>
                {checkbox("replace_existing",TextKey::ReplaceExistingEpisodeNumbers,checked("replace_existing"))}
                {checkbox("season_only",TextKey::AddonWorkspaceSeasonOnly,checked("season_only"))}
                <div class="action-row">{button("number",TextKey::Preview,false)}{button("clear-numbering",TextKey::Unnumbered,false)}</div>
            </section>
            <section class="bulk-panel" aria-labelledby="bulk-panel-detect"><h3 id="bulk-panel-detect">{tr(TextKey::BulkDetectSeasons)}</h3>
                <div class="grid-2">{input("detect_season",TextKey::BulkFirstSeason,d.field("items","detect_season","1"),"number")}{input("gap_days",TextKey::MinimumSeasonGapDays,d.field("items","gap_days","90"),"number")}</div>
                {checkbox("season_per_year",TextKey::AddonWorkspaceSeasonPerYear,checked("season_per_year"))}
                <p class="bulk-help">{tr(TextKey::AutoDetectSeasonsHelp)}</p>
                <div class="action-row">{button("auto-number",TextKey::AutoDetectSeasons,false)}</div>
            </section>
        </div>
    }
    .into_any()
}

/// Review of a pending bulk proposal. Apply is Save changes, so the table lists
/// everything that save writes for the Items task (saved value to new value),
/// including earlier unsaved row edits and a title change. Cancel restores the
/// draft from before the proposal.
pub(super) fn preview(w: &Workspace) -> AnyView {
    let d = &w.submission.draft;
    if d.undo.is_none() {
        return ().into_any();
    }
    let empty = BTreeMap::new();
    let after = d.groups.get("items").unwrap_or(&empty);
    let mut numbering_only = true;
    let mut rows = Vec::new();
    for row in &w.items {
        let fields = [
            (
                "included",
                TextKey::IncludeInPublication,
                row.included.to_string(),
            ),
            (
                "season",
                TextKey::Season,
                row.season.map(|n| n.to_string()).unwrap_or_default(),
            ),
            (
                "episode",
                TextKey::Episode,
                row.episode.map(|n| n.to_string()).unwrap_or_default(),
            ),
            (
                "kind",
                TextKey::MediaKind,
                content_kind_value(row.content_kind).into(),
            ),
            (
                "liveness",
                TextKey::PlaybackBehavior,
                row.liveness_override
                    .map_or_else(|| "inherit".into(), value),
            ),
        ];
        let mut old = Vec::new();
        let mut new = Vec::new();
        for (field, label, saved) in fields {
            let proposed = after.get(&format!("{field}_{}", row.id)).unwrap_or(&saved);
            if *proposed != saved {
                numbering_only &= matches!(field, "season" | "episode");
                old.push(format!("{}: {}", tr(label), display_value(field, &saved)));
                new.push(format!("{}: {}", tr(label), display_value(field, proposed)));
            }
        }
        if !new.is_empty() {
            rows.push(view!{<tr><th scope="row">{row.display_title.clone()}</th><td>{old.join(" · ")}</td><td>{new.join(" · ")}</td></tr>});
        }
    }
    let changed_items = rows.len();
    let title = after
        .get("metadata_title")
        .filter(|title| **title != w.metadata.metadata.title)
        .map(|title| view!{<tr><th scope="row">{tr(TextKey::Title)}</th><td>{w.metadata.metadata.title.clone()}</td><td>{title.clone()}</td></tr>});
    let options = after
        .get("option_targets")
        .and_then(|v| crate::admin::server_fns::addon_list_options::parse_option_items(v).ok())
        .filter(|ids| !ids.is_empty())
        .map(|ids| view!{<p>{tr(TextKey::AddonOptions)}": "{crate::admin::i18n::selected_count(ids.len())}</p>});
    let listed = changed_items > 0 || title.is_some();
    numbering_only &= title.is_none() && options.is_none();
    let nothing = !listed && options.is_none();
    let apply = if numbering_only {
        TextKey::BulkApplyNumbering
    } else {
        TextKey::BulkApplyChanges
    };
    view!{
        <section class="work-section bulk-preview" data-testid="bulk-preview" aria-labelledby="bulk-preview-heading" tabindex="-1" autofocus=w.submission.focus_bulk_preview>
            <h3 id="bulk-preview-heading">{tr(TextKey::BulkReviewChanges)}</h3>
            <p>{if nothing { tr(TextKey::BulkPreviewNothing).to_owned() } else { format!("{} · {}",crate::admin::i18n::item_count(u64::try_from(changed_items).unwrap_or(u64::MAX)),tr(TextKey::BulkPreviewSaves)) }}</p>
            <div class="table-scroll" hidden=!listed><table><thead><tr><th>{tr(TextKey::Title)}</th><th>{tr(TextKey::AddonOptionsBefore)}</th><th>{tr(TextKey::AddonOptionsAfter)}</th></tr></thead><tbody>{title}{rows}</tbody></table></div>
            {options}
            <div class="action-row">
                <button type="submit" class="button primary" form=FORM name="intent" value="save" formnovalidate disabled=nothing>{tr(apply)}</button>
                {button("undo",TextKey::BulkCancelPreview,false)}
            </div>
        </section>
    }.into_any()
}

fn display_value(field: &str, value: &str) -> String {
    if matches!(field, "season" | "episode") {
        return if value.is_empty() {
            tr(TextKey::Unnumbered).into()
        } else {
            value.into()
        };
    }
    tr(match value {
        "true" => TextKey::Yes,
        "false" => TextKey::No,
        "auto" => TextKey::AddonAutoDetect,
        "television_episode" => TextKey::Episode,
        "movie" => TextKey::Movie,
        "music_video" => TextKey::MusicVideo,
        "other_video" => TextKey::OtherVideos,
        "song" => TextKey::Songs,
        "image" => TextKey::Images,
        "inherit" => TextKey::AddonOptionsInherited,
        "finite" => TextKey::FiniteMedia,
        "live" => TextKey::LiveSource,
        _ => TextKey::UnknownAvailability,
    })
    .into()
}
