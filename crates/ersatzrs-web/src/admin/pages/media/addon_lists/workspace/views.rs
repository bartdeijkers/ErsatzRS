use super::super::{
    artwork_value, content_kind_value, detail_state_text_key, format_progress_timestamp,
    icon_style, import_can_retry, import_discovery_summary, parse_first_aired, people_lines,
    presentation_icon, presentation_text_key, revision_can_restore, tr, AddonMediaListArtworkRole,
    AddonMediaListContentKind, AddonMediaListDeliveryMode, AddonMediaListDetailPolicy,
    AddonMediaListDraftDiffRow, AddonMediaListForm, AddonMediaListImportState,
    AddonMediaListItemRole, AddonMediaListItemRow, AddonMediaListNfoForm,
    AddonMediaListPresentationKind, AddonMediaListProviderRow, AddonMediaListPublicationHistoryRow,
    AddonMediaListPublicationReadiness, AddonMediaListRefreshCadence, AddonMediaListResetImpactRow,
    AddonMediaListRevisionDiffRow, AddonMediaListRevisionSummaryRow,
    AddonMediaListSourceProbeOutcome, AddonMediaListSourceProbeRow, AddonMediaListSourceProbeState,
    IconName, MetadataConflictSummary, PublicationActivity, TextKey,
};
use leptos::prelude::*;
mod bulk;
mod detail;
mod settings;
use super::{Draft, Submission, FORM};
use detail::{activity, artwork, item};
use settings::{metadata_fields, option_controls, playback, settings};

pub(super) struct Workspace {
    pub load_status: super::super::EditorLoadStatus,
    pub form: AddonMediaListForm,
    pub items: Vec<AddonMediaListItemRow>,
    pub readiness: AddonMediaListPublicationReadiness,
    pub history: AddonMediaListPublicationHistoryRow,
    pub metadata: AddonMediaListNfoForm,
    pub providers: Vec<AddonMediaListProviderRow>,
    pub source: Option<AddonMediaListSourceProbeRow>,
    pub diff: AddonMediaListDraftDiffRow,
    pub impact: AddonMediaListResetImpactRow,
    pub revisions: Vec<AddonMediaListRevisionSummaryRow>,
    pub comparison: Option<AddonMediaListRevisionDiffRow>,
    pub submission: Submission,
    pub task: String,
    pub item_id: Option<i64>,
}

pub(super) fn render(w: &Workspace) -> AnyView {
    let base = format!("/media/addon-lists/{}", w.form.id);
    let draft = &w.submission.draft;
    let title = &w.metadata.metadata.title;
    let title = if title.trim().is_empty() {
        w.form.name.clone()
    } else {
        title.clone()
    };
    let heading = heading_text(w, &title);
    provide_context(w.submission.clone());
    let error_links = w
        .submission
        .error_fields
        .iter()
        .map(|name| {
            let label = match name.as_str() {
                "metadata_title" => TextKey::Title,
                "metadata_year" => TextKey::Year,
                "chapter_input" => TextKey::UseChaptersAsMediaItems,
                value
                    if matches!(value, "season" | "bulk_season" | "detect_season")
                        || value.starts_with("season_") =>
                {
                    TextKey::Season
                }
                _ => TextKey::Episode,
            };
            let item = name
                .rsplit_once('_')
                .and_then(|(_, id)| id.parse::<i64>().ok())
                .and_then(|id| w.items.iter().find(|row| row.id == id));
            let label = item.map_or_else(
                || tr(label).to_owned(),
                |row| format!("{}: {}", tr(label), row.display_title),
            );
            view! {<li><a href=format!("#workspace-{name}")>{label}</a></li>}
        })
        .collect_view();
    let import_landing = w.form.published_revision == 0
        && w.submission.draft.groups.is_empty()
        && w.form
            .import_progress
            .as_ref()
            .is_some_and(|p| super::import::active(p.state));
    let task_view = match w.task.as_str() {
        "settings" => settings(w),
        "playback" => playback(w),
        "activity" => activity(w),
        _ if w.task.starts_with("artwork") => artwork(w),
        _ if w.item_id.is_some() => item(w),
        _ if import_landing => {
            view! { <super::import::ImportLandingIsland list_id=w.form.id/> }.into_any()
        }
        _ => items(w),
    };
    // Items keeps its publication state beside the heading so the item list
    // starts above the fold; it is not a separate block between them.
    let items_heading = w.task == "items" && w.item_id.is_none() && !import_landing;
    let publication = items_heading.then(|| publication_summary(w));
    let heading = if items_heading {
        list_title_editor(w)
    } else {
        view! { <h1>{heading}</h1> }.into_any()
    };
    // One draft status per page, beside the heading it describes. Activity has
    // nothing to save; the import landing replaces the Items task.
    let status =
        (w.task != "activity" && !(w.task == "items" && import_landing)).then(|| save_status(w));
    let view_class = match w.task.split(':').next().unwrap_or("items") {
        "playback" => "options",
        "item" => "detail",
        task => task,
    };
    view! {
        <div class=format!("{} addon-workspace design-main view-{view_class}", super::style::workspace)>
            <div class="content-breadcrumb"><a href="/media/addon-lists">{tr(TextKey::AddonMediaLists)}</a><span aria-hidden="true">"/"</span><span>{title.clone()}</span></div>
            <form id=FORM method="post" enctype=w.task.starts_with("artwork").then_some("multipart/form-data")>
                <input type="hidden" name="draft" value=serde_json::to_string(draft).expect("draft serializes")/>
                <input type="hidden" name="group" value=w.task.clone()/>
            </form>
            <nav class="content-navigation" aria-label=tr(TextKey::AddonWorkspaceSections)>
                {[("items", TextKey::AddonWorkspaceItems), ("settings", TextKey::AddonWorkspaceSettings), ("playback",TextKey::AddonWorkspacePlayback), ("activity", TextKey::AddonWorkspaceActivity)].into_iter().map(|(task,key)| {
                    let url = if task == "items" { base.clone() } else { format!("{base}/{task}") };
                    view! { <button class="tab-link" form=FORM name="intent" value="navigate" formaction=url formnovalidate aria-current=(w.task == task || (task == "items" && w.task.starts_with("item:"))).then_some("page")>{tr(key)}</button> }
                }).collect_view()}
            </nav>
            <div class=if w.task=="items" {"page-heading media-masthead"} else {"page-heading"}>{(w.task=="items").then(||view!{<div class="cover-art"><span aria-hidden="true" style=icon_style(presentation_icon(w.form.presentation_kind),"2rem")></span></div>})}<div class="masthead-body"><div class="title-row">{heading}{status}</div>{publication}</div></div>
            {w.submission.arrival_error.map(|key| view! { <div class="notice error" role="alert" data-testid="addon-list-arrival-error"><p>{tr(key)}</p><button class="button secondary" form=FORM name="intent" value="navigate" formaction=format!("{base}/settings") formnovalidate>{tr(TextKey::AddonWorkspaceSettings)}</button></div> })}
            {w.submission.error.clone().map(|error| view! { <div class="notice error" role="alert" tabindex="-1" autofocus id="workspace-errors"><strong>{tr(TextKey::AddonWorkspaceRetained)}</strong><p>{error}</p><ul>{error_links}</ul>{button("review-current",TextKey::AddonWorkspaceReviewLatest,false)}</div> })}
            // A successful save is shown by the Save button returning to its
            // resting state; other notices still need their banner.
            {w.submission.notice.filter(|key| *key != TextKey::Saved).map(|key| view! { <p class="notice success" role="status">{tr(key)}</p> })}
            <script src="/public/workspace-draft-status.js" defer></script>
            <script src="/public/workspace-auto-apply.js" defer></script>

            {detail::confirmation(w)}
            {w.submission.draft.option_undo.as_ref().map(|_|button("undo-options",TextKey::UndoLastChange,false))}
            <fieldset class="workspace-task-fields" disabled=w.load_status.disabled_tasks.contains(w.task.split(':').next().unwrap_or("items"))>{task_view}</fieldset>
        </div>
    }.into_any()
}

/// Heading for tasks whose title is not edited in place.
fn heading_text(w: &Workspace, title: &str) -> String {
    match w.task.as_str() {
        "settings" => tr(TextKey::AddonWorkspaceSettings).to_owned(),
        "playback" => tr(TextKey::AddonWorkspacePlayback).to_owned(),
        "activity" => tr(TextKey::AddonWorkspaceActivity).to_owned(),
        _ if w.task.starts_with("artwork") => tr(TextKey::Artwork).to_owned(),
        _ if w.item_id.is_some() => w
            .items
            .iter()
            .find(|r| Some(r.id) == w.item_id)
            .map_or_else(|| title.to_owned(), |r| r.display_title.clone()),
        _ => title.to_owned(),
    }
}

fn retained_edits(w: &Workspace) -> bool {
    w.submission
        .draft
        .groups
        .get(&w.task)
        .is_some_and(|fields| {
            fields.iter().any(|(name, value)| {
                if name == "delivery_mode" {
                    return *value != self::value(w.form.delivery_mode);
                }
                if name == "disk_warning_gb" {
                    return w
                        .submission
                        .draft
                        .playback_baseline
                        .as_ref()
                        .and_then(|baseline| baseline.disk_warning_gb)
                        .is_none_or(|saved| *value != saved.to_string());
                }
                name.starts_with("metadata_") && name != "metadata_etag"
                    || name.starts_with("option_") && name != "option_targets"
                    || name.starts_with("season_")
                    || name.starts_with("episode_")
                    || name.starts_with("included_")
                    || name.starts_with("content_kind_")
                    || matches!(
                        name.as_str(),
                        "season"
                            | "episode"
                            | "included"
                            | "content_kind"
                            | "liveness"
                            | "role"
                            | "chapter_input"
                            | "refresh_cadence"
                            | "refresh_publication_policy"
                            | "generate_playlist"
                            | "presentation_kind"
                            | "load_item_details"
                    )
            })
        })
}

fn baseline(name: &str, displayed: &str) -> AnyView {
    let persistent = name.starts_with("option_")
        || name.starts_with("metadata_")
        || name.starts_with("season_")
        || name.starts_with("episode_")
        || matches!(
            name,
            "season"
                | "numbering_mode"
                | "episode"
                | "content_kind"
                | "liveness"
                | "included"
                | "role"
                | "chapter_input"
                | "refresh_cadence"
                | "refresh_publication_policy"
                | "load_item_details"
                | "generate_playlist"
                | "presentation_kind"
                | "disk_warning_gb"
        );
    if !persistent {
        return ().into_any();
    }
    view! { <input type="hidden" form=FORM name=format!("baseline_{name}") value=super::input_token(displayed)/> }.into_any()
}

fn input(name: &str, label: TextKey, value: String, kind: &str) -> AnyView {
    let id = format!("workspace-{name}");
    let invalid = use_context::<Submission>()
        .is_some_and(|s| s.error_fields.iter().any(|field| field == name));
    view! { <div class="field">{baseline(name,&value)}<label for=id.clone()>{tr(label)}</label><input id=id.clone() name=name.to_owned() type=kind.to_owned() min=(kind=="number").then_some("0") max=(kind=="number").then_some(if name=="disk_warning_gb" {"4294967295"}else{"2147483647"}) step=(kind=="number").then_some("1") value=value form=FORM aria-invalid=invalid.then_some("true") aria-describedby=invalid.then_some("workspace-errors")/></div> }.into_any()
}

fn textarea(name: &str, label: TextKey, value: String) -> AnyView {
    let id = format!("workspace-{name}");
    let invalid = use_context::<Submission>()
        .is_some_and(|s| s.error_fields.iter().any(|field| field == name));
    view! { <div class="field full">{baseline(name,&value)}<label for=id.clone()>{tr(label)}</label><textarea id=id.clone() name=name.to_owned() rows="4" form=FORM aria-invalid=invalid.then_some("true") aria-describedby=invalid.then_some("workspace-errors")>{value}</textarea></div> }.into_any()
}

fn checkbox(name: &str, label: TextKey, checked: bool) -> AnyView {
    view! { <label class="check">{baseline(name,&checked.to_string())}<input type="hidden" name=name.to_owned() value="false" form=FORM/><input type="checkbox" name=name.to_owned() value="true" checked=checked form=FORM/><span>{tr(label)}</span></label> }.into_any()
}

fn select(
    name: &str,
    label: TextKey,
    current: impl AsRef<str>,
    choices: Vec<(String, String)>,
) -> AnyView {
    select_disabled(name, label, current.as_ref(), choices, None)
}

fn select_disabled(
    name: &str,
    label: TextKey,
    current: &str,
    choices: Vec<(String, String)>,
    disabled: Option<&str>,
) -> AnyView {
    let id = format!("workspace-{name}");
    let selected_label = (name == "category").then(|| {
        choices
            .iter()
            .find(|(value, _)| value == current)
            .map(|(_, label)| label.clone())
            .unwrap_or_default()
    });
    view! { <div class="field">{baseline(name,current)}<label for=id.clone()>{tr(label)}</label><select id=id.clone() name=name.to_owned() form=FORM>{choices.into_iter().map(|(value,label)| view! { <option selected={value == current} disabled=disabled.is_some_and(|v|v==value) value=value>{label}</option> }).collect_view()}</select>{selected_label.map(|label| view! { <output class="selected-filter-label" for=id>{label}</output> })}</div> }.into_any()
}

fn button(intent: &str, label: TextKey, primary: bool) -> AnyView {
    view! { <button type="submit" class=if primary { "button primary" } else { "button secondary" } form=FORM name="intent" value=intent.to_owned() formnovalidate>{tr(label)}</button> }.into_any()
}

fn save_bar(w: &Workspace, key: TextKey) -> AnyView {
    view! { <div class="apply-bar">{save_button(w,key)}{button("discard",TextKey::AddonWorkspaceDiscard,false)}<small>{tr(TextKey::AddonWorkspaceNavigation)}</small></div> }.into_any()
}

/// The task's Save action doubles as its draft indicator: outlined while the
/// page matches the saved list, primary once anything on the page (including
/// item rows) differs. The server renders the retained state;
/// `workspace-draft-status.js` updates it while the operator edits.
fn save_button(w: &Workspace, label: TextKey) -> AnyView {
    let dirty = retained_edits(w);
    view! { <button type="submit" class=if dirty { "button primary" } else { "button secondary" } form=FORM name="intent" value="save" data-save-indicator data-dirty=dirty.to_string() formnovalidate>{tr(label)}</button> }.into_any()
}

/// Screen-reader announcement of the same draft state; visually the Save
/// button carries it.
fn save_status(w: &Workspace) -> AnyView {
    let dirty = retained_edits(w);
    view! {
        <span class="save-status sr-only" data-workspace-save-status data-retained=dirty.to_string() data-dirty=dirty.to_string() data-saved=tr(TextKey::Saved) data-unsaved=tr(TextKey::UnsavedChanges) role="status">{tr(if dirty {TextKey::UnsavedChanges} else {TextKey::Saved})}</span>
    }.into_any()
}

/// The Items heading edits the list title in place. It is part of the Items
/// task, so Save changes stores it with the other Items edits through the same
/// list metadata command and concurrency fence as List settings.
fn list_title_editor(w: &Workspace) -> AnyView {
    let d = &w.submission.draft;
    let value = d.field("items", "metadata_title", w.metadata.metadata.title.clone());
    let etag = d.field("items", "metadata_etag", w.metadata.etag.clone());
    let invalid = w
        .submission
        .error_fields
        .iter()
        .any(|name| name == "metadata_title");
    // Width follows the title so the draft status sits right after it;
    // `field-sizing: content` keeps it following while the operator types.
    let size = value
        .chars()
        .count()
        .max(w.form.name.chars().count())
        .clamp(8, 80)
        + 1;
    view! {
        <h1 class="list-title-heading">
            {baseline("metadata_title", &value)}
            <input id="workspace-metadata_title" class="list-title-input" type="text" form=FORM name="metadata_title" size=size value=value placeholder=w.form.name.clone() required aria-label=tr(TextKey::Title) aria-invalid=invalid.then_some("true") aria-describedby=invalid.then_some("workspace-errors")/>
        </h1>
        {baseline("metadata_etag", &etag)}
        <input type="hidden" form=FORM name="metadata_etag" value=etag/>
    }.into_any()
}

/// Publication state, counts, the Publish action and readiness details for
/// the Items heading. Disabled with the Items task while it cannot load.
fn publication_summary(w: &Workspace) -> AnyView {
    let r = &w.readiness;
    view! {
        <fieldset class="publication-fieldset" disabled=w.load_status.disabled_tasks.contains("items")>
            <section class="publication-summary" aria-label=tr(TextKey::PublicationReadiness)>
                <super::publication::PublicationOverviewIsland list_id=w.form.id initial=r.clone()/>
                <details class="disclosure quiet-details" data-disclosure-kind="section"><summary><span class="disclosure-label">{tr(TextKey::PublicationReadiness)}</span><span class="disclosure-toggle" aria-hidden="true"></span></summary><div class="disclosure-body"><dl class="facts"><div><dt>{tr(TextKey::SavedRevision)}</dt><dd>{w.form.draft_revision}</dd></div><div><dt>{tr(TextKey::Published)}</dt><dd>{w.form.published_revision}</dd></div></dl>
                    {detail::import_progress(w)}
                </div></details>
                <p hidden={r.restricted_excluded_count==0}>{tr(TextKey::RestrictedItemsExcluded)}": "{r.restricted_excluded_count}</p>
            </section>
        </fieldset>
    }.into_any()
}

fn value<T: serde::Serialize>(value: T) -> String {
    serde_json::to_value(value)
        .expect("enum serializes")
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

fn choice<T: serde::Serialize>(val: T, key: TextKey) -> (String, String) {
    (value(val), tr(key).to_owned())
}

fn filtered(w: &Workspace) -> Vec<&AddonMediaListItemRow> {
    super::sorted_filtered(
        &w.submission.draft,
        &w.items,
        w.form.presentation_kind,
        &w.diff,
    )
}

fn sort_heading(label: TextKey, field: &str, current: &str, compact: bool) -> AnyView {
    let ascending = format!("{field}_asc");
    let descending = format!("{field}_desc");
    let active_ascending = current == ascending || (field == "playlist" && current.is_empty());
    let active_descending = current == descending;
    let next = if active_ascending {
        descending
    } else {
        ascending
    };
    let direction = if active_ascending {
        TextKey::AddonWorkspaceDescending
    } else {
        TextKey::AddonWorkspaceAscending
    };
    let description = format!(
        "{}: {} · {}",
        tr(TextKey::AddonWorkspaceSort),
        tr(label),
        tr(direction)
    );
    view! {
        <button type="submit" class=if active_ascending {"sort-heading is-ascending"} else if active_descending {"sort-heading is-descending"} else {"sort-heading"}
            form=FORM name="intent" value=format!("sort:{next}") formnovalidate
            aria-label=description.clone() title=description>
            <span>{if compact {"#"} else {tr(label)}}</span>
            <span class="sort-arrows" aria-hidden="true"><span>"▲"</span><span>"▼"</span></span>
        </button>
    }.into_any()
}

fn selectable_ids(d: &Draft, visible: &[&AddonMediaListItemRow]) -> Vec<i64> {
    visible
        .iter()
        .filter(|row| {
            row.included
                && d.field(
                    "items",
                    &format!("included_{}", row.id),
                    row.included.to_string(),
                ) == "true"
        })
        .map(|row| row.id)
        .collect()
}

fn items(w: &Workspace) -> AnyView {
    let d = &w.submission.draft;
    let rows = filtered(w);
    let size = if d.page_size == 0 {
        rows.len().max(1)
    } else {
        d.page_size
    };
    let pages = rows.len().div_ceil(size).max(1);
    let page = d.page.min(pages - 1);
    let visible = rows
        .iter()
        .skip(page * size)
        .take(size)
        .copied()
        .collect::<Vec<_>>();
    let selectable_ids = selectable_ids(d, &visible);
    let selectable_here = selectable_ids.len();
    let selected_here = selectable_ids
        .iter()
        .filter(|id| d.selected.contains(id))
        .count();
    let all_visible_selected = selectable_here > 0 && selected_here == selectable_here;
    let selection_intent = if all_visible_selected {
        "deselect-visible"
    } else {
        "select-visible"
    };
    let selection_label = if all_visible_selected {
        TextKey::AddonWorkspaceDeselectVisible
    } else {
        TextKey::AddonWorkspaceSelectVisible
    };
    view! {
        <section class="panel episode-editor" aria-label=tr(TextKey::AddonWorkspaceItems)>
            <div class="filter-row">
                {select("category",TextKey::Filters,d.category.clone(),category_choices(w))}
                {input("query",TextKey::Search,d.query.clone(),"search")}
                {button("filter",TextKey::Search,false)}
            </div>
            {active_filter(w)}
            // A pending bulk proposal keeps the tools open so its review, with
            // Apply and Cancel, stays beside the panel that produced it.
            <details class="disclosure group-actions-disclosure bulk-tools" data-disclosure-kind="section" open=d.expanded_bulk || d.undo.is_some()>
                <summary><span class="disclosure-label">{tr(TextKey::AddonWorkspaceBulk)}</span><span class="disclosure-toggle" aria-hidden="true"></span></summary><div class="disclosure-body">
                    {bulk::numbering_panels(w,&rows)}
                    {bulk::preview(w)}
                    <div class="bulk-task-grid">
                        <section class="bulk-panel" aria-labelledby="bulk-panel-inclusion"><h3 id="bulk-panel-inclusion">{tr(TextKey::IncludeInPublication)}</h3>
                            <div class="action-row">{button("include-selected",TextKey::AddonWorkspaceIncludeSelected,false)}{button("exclude-selected",TextKey::AddonWorkspaceExcludeSelected,false)}</div>
                            <p class="bulk-help">{tr(TextKey::AddonWorkspaceInclusionHelp)}</p>
                        </section>
                        <section class="bulk-panel" aria-labelledby="bulk-panel-kind"><h3 id="bulk-panel-kind">{tr(TextKey::MediaKind)}</h3>{kind_select("bulk_kind",d.field("items","bulk_kind","__unchanged"))}{liveness_select("bulk_liveness",d.field("items","bulk_liveness","__unchanged"))}<div class="action-row">{button("classify",TextKey::Preview,false)}</div></section>
                    </div>
                    <div class="action-row">{button("bulk-options",TextKey::AddonOptions,false)}</div>
                    {d.groups.get("items").and_then(|f|f.get("option_targets")).and_then(|v|crate::admin::server_fns::addon_list_options::parse_option_items(v).ok()).filter(|ids|!ids.is_empty()).map(|ids|view!{<section class="settings-section"><h3>{tr(TextKey::AddonOptions)}</h3><p>{crate::admin::i18n::selected_count(ids.len())}</p>{option_controls(w,ids)}</section>})}
                </div>
            </details>
            // Cancel preview inside the review replaces the bar's Undo for a
            // pending bulk proposal.
            <div class="numbering-actions editor-savebar numbering-save-bar">{d.saved_review_undo.as_ref().filter(|u|d.undo.is_none() && u.expected_revision==d.revision).map(|_|button("undo-saved-review",TextKey::UndoLastChange,false))}{save_button(w,TextKey::SaveChanges)}{button("discard",TextKey::AddonWorkspaceDiscard,false)}<span>{format!("{}: {} · {}: {}",tr(TextKey::AddonWorkspaceSelection),d.selected.len(),tr(TextKey::AddonWorkspaceOffPage),d.selected.len().saturating_sub(selected_here))}</span></div>
            <div class="pager"><span class="page-summary">{format!("{}–{} / {}",if rows.is_empty() {0} else {page*size+1},((page+1)*size).min(rows.len()),rows.len())}</span>
                {select("page_size",TextKey::AddonWorkspacePageSize,d.page_size.to_string(),[10,25,50,100,0].into_iter().map(|n|(n.to_string(),if n==0 {tr(TextKey::All).to_owned()} else {n.to_string()})).collect())}
                {button("page-size",TextKey::Apply,false)}
                <div class="page-jump">
                    <button class="button secondary" form=FORM name="intent" value="page:0" disabled={page==0} formnovalidate>{tr(TextKey::FirstPage)}</button>
                    <button class="button secondary" form=FORM name="intent" value=format!("page:{}",page.saturating_sub(1)) disabled={page==0} formnovalidate>{tr(TextKey::PreviousPage)}</button>
                    <span class="page-position">{crate::admin::i18n::page_of(u32::try_from(page+1).unwrap_or(u32::MAX),u32::try_from(pages).unwrap_or(u32::MAX))}</span>
                    <button class="button secondary" form=FORM name="intent" value=format!("page:{}",page+1) disabled={page+1>=pages} formnovalidate>{tr(TextKey::NextPage)}</button>
                    <button class="button secondary" form=FORM name="intent" value=format!("page:{}",pages-1) disabled={page+1>=pages} formnovalidate>{tr(TextKey::LastPage)}</button>
                </div>
            </div>
            <div class="episode-list">
                <div class="mobile-review-controls">
                    <button type="submit" class="header-select" form=FORM name="intent" value=selection_intent disabled=selectable_here==0 formnovalidate aria-label=tr(selection_label) title=tr(selection_label) aria-pressed=all_visible_selected.to_string()><span aria-hidden="true">{if all_visible_selected {"✓"} else {""}}</span></button>
                    {sort_heading(TextKey::Playlist,"playlist",&d.sort,false)}
                    {sort_heading(TextKey::Name,"name",&d.sort,false)}
                    {sort_heading(TextKey::AddonWorkspaceItemDate,"date",&d.sort,false)}
                </div>
                <div class="episode-column-head">
                    <div class="review-first-column">{sort_heading(TextKey::Playlist,"playlist",&d.sort,true)}<button type="submit" class="header-select" form=FORM name="intent" value=selection_intent disabled=selectable_here==0 formnovalidate aria-label=tr(selection_label) title=tr(selection_label) aria-pressed=all_visible_selected.to_string()><span aria-hidden="true">{if all_visible_selected {"✓"} else {""}}</span></button></div>
                    {sort_heading(TextKey::Name,"name",&d.sort,false)}
                    {sort_heading(TextKey::AddonWorkspaceItemDate,"date",&d.sort,false)}
                    <span>{tr(TextKey::Duration)}</span><span>{tr(TextKey::Season)}" / "{tr(TextKey::Episode)}</span><span></span>
                </div>
                <ol class="episode-rows">{visible.iter().map(|row| item_row(w,row)).collect_view()}</ol>
                <p hidden=!rows.is_empty()>{tr(TextKey::NoItems)}</p>
            </div>
        </section>
    }.into_any()
}

fn item_row(w: &Workspace, row: &AddonMediaListItemRow) -> AnyView {
    let d = &w.submission.draft;
    let id = row.id;
    let rank = w.items.iter().position(|r| r.id == id).unwrap_or(0);
    let title = row.display_title.clone();
    let select_name = format!("selected_{id}");
    let gap = if matches!(d.sort.as_str(), "" | "playlist_asc") {
        rank.checked_sub(1)
            .and_then(|i| w.items.get(i))
            .and_then(|previous| parse_first_aired(previous.first_aired.as_deref()))
            .zip(parse_first_aired(row.first_aired.as_deref()))
            .map(|(a, b)| b.signed_duration_since(a).num_days())
    } else {
        None
    };
    let menu = format!("row-commands-{id}");
    let item_url = format!("/media/addon-lists/{}/items/{id}", w.form.id);
    let saved_number = |value: Option<i32>| value.map(|n| n.to_string()).unwrap_or_default();
    // Retained draft values that differ from the saved row. The draft-status
    // script refines this while the operator edits; without JavaScript the
    // server-rendered state is shown after every retained round trip.
    let inclusion_changed = d.field("items", &format!("included_{id}"), row.included.to_string())
        != row.included.to_string();
    let dirty = inclusion_changed
        || [("season", row.season), ("episode", row.episode)]
            .into_iter()
            .any(|(name, saved)| {
                d.field("items", &format!("{name}_{id}"), saved_number(saved))
                    != saved_number(saved)
            });
    let excluded = d.field("items", &format!("included_{id}"), row.included.to_string()) != "true";
    let unavailable =
        row.availability == ersatzrs_domain::AddonMediaListItemAvailability::Unavailable;
    view!{<li class=if dirty {"episode-row row-dirty"} else {"episode-row"} id=format!("item-{id}") data-inclusion-changed=inclusion_changed.to_string() data-excluded=excluded.to_string() data-unavailable=unavailable.to_string()>
        <input type="hidden" form=FORM name=format!("review_snapshot_{id}") value=super::review_snapshot(row)/>
        <label class="row-pick"><input type="hidden" form=FORM name=select_name.clone() value="false"/><input type="checkbox" form=FORM name=select_name value="true" checked=d.selected.contains(&id) disabled=excluded aria-label=format!("{}: {title}",tr(TextKey::AddonWorkspaceSelection))/><span aria-hidden="true">{format!("{:02}",rank+1)}</span></label>
        <div class="episode-identity"><span class="row-art-placeholder" aria-hidden="true"><span style=icon_style(presentation_icon(w.form.presentation_kind),"2rem")></span><span class="row-thumbnail" style:background-image=row.thumbnail_url.as_ref().map(|url| format!("url({})",serde_json::to_string(url).expect("thumbnail URL serializes"))).unwrap_or_default()></span></span><div>
            <span class="row-title-line"><button class="button row-title" form=FORM name="intent" value="navigate" formaction=item_url.clone() formnovalidate>{title.clone()}</button><a class="row-open-tab" href=item_url target="_blank" rel="noopener" aria-label=format!("{}: {title}",tr(TextKey::OpenItemInNewTab)) title=tr(TextKey::OpenItemInNewTab)><span aria-hidden="true" style=icon_style(IconName::OpenInNew,"0.95rem")></span></a></span>
            <span class=if excluded {"item-state-badge is-excluded"} else {"item-state-badge is-included"}><span aria-hidden="true">{if excluded {"⊘"} else {"✓"}}</span>{tr(if excluded {TextKey::AddonWorkspaceExcluded} else {TextKey::IncludeInPublication})}</span>
            {unavailable.then(||view!{<span class="item-state-badge is-unavailable"><span aria-hidden="true">"!"</span>{tr(TextKey::Unavailable)}</span>})}
            <small hidden=!row.local_media_available>{tr(TextKey::AddonLocalPlaybackAvailable)}</small>
            {(row.metadata_conflict_count>0 || row.liveness_conflict).then(||view!{<small>{tr(TextKey::Conflicts)}</small>})}
        </div></div>
        <div class="episode-evidence"><div class="airdate"><span class="mobile-column" aria-hidden="true">{tr(TextKey::AddonWorkspaceItemDate)}</span><span class="sr-only">{tr(TextKey::AddonWorkspaceItemDate)}": "</span><time datetime=row.first_aired.clone()>{row.first_aired.clone().unwrap_or_else(||tr(TextKey::UnknownAvailability).into())}</time>{gap.map(|days|view!{<small class="gap-note">{format!("{days} d")}</small>})}</div>
        <div class="runtime"><span class="mobile-column" aria-hidden="true">{tr(TextKey::Duration)}</span><span class="sr-only">{tr(TextKey::Duration)}": "</span><span>{row.duration_seconds.map_or_else(||tr(TextKey::UnknownAvailability).into(),|n|format!("{}:{:02}",n/60,n%60))}</span></div></div>
        <div class="row-numbering"><div class="inline-pair">
            {[("season",TextKey::Season,row.season,"S"),("episode",TextKey::Episode,row.episode,"E")].into_iter().map(|(name,key,saved,symbol)|view!{<label>{baseline(&format!("{name}_{id}"),&d.field("items",&format!("{name}_{id}"),saved_number(saved)))}<span aria-hidden="true">{symbol}</span><input id=format!("workspace-{name}_{id}") type="number" min="0" max="2147483647" form=FORM name=format!("{name}_{id}") value=d.field("items",&format!("{name}_{id}"),saved_number(saved)) data-saved=saved_number(saved) placeholder="—" aria-invalid=w.submission.error_fields.contains(&format!("{name}_{id}")).then_some("true") aria-describedby=w.submission.error_fields.contains(&format!("{name}_{id}")).then_some("workspace-errors") aria-label=format!("{}: {}",tr(key),row.display_title)/></label>}).collect_view()}
        </div><small>{tr(TextKey::ItemSavedValue)}": "{match (row.season,row.episode){(Some(s),Some(e))=>format!("S{s:02}E{e:02}"),_=>tr(TextKey::Unnumbered).into()}}</small><small class="row-dirty-note" hidden=!dirty>{tr(TextKey::UnsavedChanges)}</small></div>
        <button class="row-command-trigger" type="button" popovertarget=menu.clone() aria-label=format!("{}: {}",tr(TextKey::Actions),row.display_title)>{tr(TextKey::Actions)}</button>
        <div id=menu class="row-commands" popover="auto" role="group" aria-label=tr(TextKey::Actions)><p class="command-context">{row.display_title.clone()}</p><div class="command-list">
            {button(&format!("start:{id}"),TextKey::StartHere,false)}
            {button(&format!("range-start:{id}"),TextKey::RangeStart,false)}
            {button(&format!("clear-row:{id}"),TextKey::Unnumbered,false)}
        </div></div>
    </li>}.into_any()
}

fn kind_select(name: &str, current: String) -> AnyView {
    select(
        name,
        TextKey::MediaKind,
        current,
        vec![
            ("__unchanged", TextKey::KeepCurrentValue),
            ("auto", TextKey::AddonAutoDetect),
            ("television_episode", TextKey::Episode),
            ("movie", TextKey::Movie),
            ("music_video", TextKey::MusicVideo),
            ("other_video", TextKey::OtherVideos),
            ("song", TextKey::Songs),
            ("image", TextKey::Images),
        ]
        .into_iter()
        .filter(|(v, _)| *v != "__unchanged" || name.starts_with("bulk_"))
        .map(|(v, k)| (v.to_owned(), tr(k).to_owned()))
        .collect(),
    )
}

fn liveness_select(name: &str, current: String) -> AnyView {
    select(
        name,
        TextKey::PlaybackBehavior,
        current,
        vec![
            ("__unchanged", TextKey::KeepCurrentValue),
            ("inherit", TextKey::AddonOptionsInherited),
            ("unknown", TextKey::UnknownAvailability),
            ("finite", TextKey::FiniteMedia),
            ("live", TextKey::LiveSource),
        ]
        .into_iter()
        .filter(|(v, _)| *v != "__unchanged" || name.starts_with("bulk_"))
        .map(|(v, k)| (v.to_owned(), tr(k).to_owned()))
        .collect(),
    )
}

/// The applied filter, shown beside the filter controls with one action that
/// clears it. Unapplied edits of the filter fields are not a filter yet.
fn active_filter(w: &Workspace) -> Option<AnyView> {
    let d = &w.submission.draft;
    if d.category.is_empty() && d.query.is_empty() {
        return None;
    }
    let category = (!d.category.is_empty())
        .then(|| {
            category_choices(w)
                .into_iter()
                .find(|(value, _)| *value == d.category)
                .map(|(_, label)| view! { <span class="filter-chip">{label}</span> })
        })
        .flatten();
    // Matching uses the query exactly as applied, so it is shown unchanged.
    let query = (!d.query.is_empty()).then(
        || view! { <span class="filter-chip">{format!("\u{201c}{}\u{201d}", d.query)}</span> },
    );
    Some(view! {
        <div class="active-filter" data-testid="active-filter"><span>{tr(TextKey::ActiveFilter)}": "</span>{category}{query}{button("clear-filter",TextKey::ClearFilters,false)}</div>
    }.into_any())
}

fn category_choices(w: &Workspace) -> Vec<(String, String)> {
    [
        ("", TextKey::All),
        ("review", TextKey::NeedReview),
        ("blockers", TextKey::PublicationBlockers),
        ("unknown", TextKey::UnknownAvailability),
        ("unnumbered", TextKey::Unnumbered),
        ("added", TextKey::Added),
        ("metadata_changed", TextKey::MetadataChanges),
        ("excluded", TextKey::ExcludedItems),
        ("unavailable", TextKey::Unavailable),
        ("conflicted", TextKey::Conflicts),
        ("pending_removal", TextKey::PendingRemovals),
        ("reappeared", TextKey::Reappearances),
        ("removal_candidate", TextKey::RemovalCandidates),
    ]
    .into_iter()
    .map(|(category, key)| {
        let mut filter = w.submission.draft.clone();
        filter.category = category.into();
        let count = w
            .items
            .iter()
            .filter(|r| super::matches_row(&filter, r, w.form.presentation_kind, &w.diff))
            .count();
        (category.into(), format!("{} ({count})", tr(key)))
    })
    .collect()
}
