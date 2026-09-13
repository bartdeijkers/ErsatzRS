use super::super::{
    artwork_value, content_kind_value, detail_state_text_key, format_progress_timestamp,
    icon_style, import_can_retry, import_discovery_summary, parse_first_aired, people_lines,
    presentation_icon, presentation_text_key, tr, AddonMediaListArtworkRole,
    AddonMediaListContentKind, AddonMediaListDeliveryMode, AddonMediaListDetailPolicy,
    AddonMediaListDraftDiffRow, AddonMediaListForm, AddonMediaListImportState,
    AddonMediaListItemRole, AddonMediaListItemRow, AddonMediaListNfoForm,
    AddonMediaListPresentationKind, AddonMediaListProviderRow, AddonMediaListPublicationHistoryRow,
    AddonMediaListPublicationReadiness, AddonMediaListRefreshCadence,
    AddonMediaListRefreshPublicationPolicy, AddonMediaListResetImpactRow,
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
    pub form: AddonMediaListForm,
    pub items: Vec<AddonMediaListItemRow>,
    pub readiness: AddonMediaListPublicationReadiness,
    pub history: AddonMediaListPublicationHistoryRow,
    pub metadata: AddonMediaListNfoForm,
    pub providers: Vec<AddonMediaListProviderRow>,
    pub source: Option<AddonMediaListSourceProbeRow>,
    pub diff: AddonMediaListDraftDiffRow,
    pub impact: AddonMediaListResetImpactRow,
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
    let heading = match w.task.as_str() {
        "settings" => tr(TextKey::AddonWorkspaceSettings).to_owned(),
        "playback" => tr(TextKey::AddonWorkspacePlayback).to_owned(),
        "activity" => tr(TextKey::AddonWorkspaceActivity).to_owned(),
        _ if w.task.starts_with("artwork") => tr(TextKey::Artwork).to_owned(),
        _ if w.item_id.is_some() => w
            .items
            .iter()
            .find(|r| Some(r.id) == w.item_id)
            .map_or(title.clone(), |r| r.display_title.clone()),
        _ => title.clone(),
    };
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
                    if value == "season"
                        || value == "bulk_season"
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
    let task_view = match w.task.as_str() {
        "settings" => settings(w),
        "playback" => playback(w),
        "activity" => activity(w),
        _ if w.task.starts_with("artwork") => artwork(w),
        _ if w.item_id.is_some() => item(w),
        _ if w.form.published_revision == 0
            && w.submission.draft.groups.is_empty()
            && w.form
                .import_progress
                .as_ref()
                .is_some_and(|p| super::import::active(p.state)) =>
        {
            view! { <super::import::ImportLandingIsland list_id=w.form.id/> }.into_any()
        }
        _ => items(w),
    };
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
            <div class=if w.task=="items" {"page-heading media-masthead"} else {"page-heading"}>{(w.task=="items").then(||view!{<div class="cover-art"><span aria-hidden="true" style=icon_style(presentation_icon(w.form.presentation_kind),"2rem")></span></div>})}<div><h1>{heading}</h1></div></div>
            {w.submission.arrival_error.map(|key| view! { <div class="notice error" role="alert" data-testid="addon-list-arrival-error"><p>{tr(key)}</p><button class="button secondary" form=FORM name="intent" value="navigate" formaction=format!("{base}/settings") formnovalidate>{tr(TextKey::AddonWorkspaceSettings)}</button></div> })}
            {w.submission.error.clone().map(|error| view! { <div class="notice error" role="alert" tabindex="-1" autofocus id="workspace-errors"><strong>{tr(TextKey::AddonWorkspaceRetained)}</strong><p>{error}</p><ul>{error_links}</ul>{button("review-current",TextKey::AddonWorkspaceReviewLatest,false)}</div> })}
            {w.submission.notice.map(|key| view! { <p class="notice success" role="status">{tr(key)}</p> })}
            {detail::confirmation(w)}
            {w.submission.draft.option_undo.as_ref().map(|_|button("undo-options",TextKey::UndoLastChange,false))}
            {task_view}
        </div>
    }.into_any()
}

fn baseline(name: &str, displayed: &str) -> AnyView {
    let persistent = name.starts_with("metadata_")
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
    view! { <div class="field">{baseline(name,current)}<label for=id.clone()>{tr(label)}</label><select id=id.clone() name=name.to_owned() form=FORM>{choices.into_iter().map(|(value,label)| view! { <option selected={value == current} disabled=disabled.is_some_and(|v|v==value) value=value>{label}</option> }).collect_view()}</select></div> }.into_any()
}

fn button(intent: &str, label: TextKey, primary: bool) -> AnyView {
    view! { <button type="submit" class=if primary { "button primary" } else { "button secondary" } form=FORM name="intent" value=intent.to_owned() formnovalidate>{tr(label)}</button> }.into_any()
}

fn save_bar(key: TextKey) -> AnyView {
    view! { <div class="apply-bar">{button("save",key,true)}{button("discard",TextKey::AddonWorkspaceDiscard,false)}<small>{tr(TextKey::AddonWorkspaceNavigation)}</small></div> }.into_any()
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
    w.items
        .iter()
        .filter(|row| {
            super::matches_row(&w.submission.draft, row, w.form.presentation_kind, &w.diff)
        })
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
    let selected_here = visible
        .iter()
        .filter(|row| d.selected.contains(&row.id))
        .count();
    let r = &w.readiness;
    view! {
        <section class="publication-summary" aria-label=tr(TextKey::PublicationReadiness)>
            <super::publication::PublicationOverviewIsland list_id=w.form.id initial=r.clone()/>
            <details class="disclosure quiet-details" data-disclosure-kind="section"><summary><span class="disclosure-label">{tr(TextKey::PublicationReadiness)}</span><span class="disclosure-toggle" aria-hidden="true"></span></summary><div class="disclosure-body"><dl class="facts"><div><dt>{tr(TextKey::SavedRevision)}</dt><dd>{w.form.draft_revision}</dd></div><div><dt>{tr(TextKey::Published)}</dt><dd>{w.form.published_revision}</dd></div></dl>
                {detail::import_progress(w)}
            </div></details>
            <p hidden={r.restricted_excluded_count==0}>{tr(TextKey::RestrictedItemsExcluded)}": "{r.restricted_excluded_count}</p>
        </section>
        <section class="panel episode-editor" aria-label=tr(TextKey::AddonWorkspaceItems)>
            <div class="filter-row">
                {select("category",TextKey::Filters,d.category.clone(),category_choices(w))}
                {input("query",TextKey::Search,d.query.clone(),"search")}
                {button("filter",TextKey::Search,false)}
            </div>
            <details class="disclosure group-actions-disclosure bulk-tools" data-disclosure-kind="section" open=d.expanded_bulk>
                <summary><span class="disclosure-label">{tr(TextKey::AddonWorkspaceBulk)}</span><span class="disclosure-toggle" aria-hidden="true"></span></summary><div class="disclosure-body">
                    <div class="action-row">{button("include-selected",TextKey::AddonWorkspaceIncludeSelected,false)}{button("exclude-selected",TextKey::AddonWorkspaceExcludeSelected,false)}</div>
                    <p>{tr(TextKey::AddonWorkspaceInclusionHelp)}</p>
                    {button("clear-selection",TextKey::ClearSelection,false)}<div class="bulk-task-grid"><section><h3>{tr(TextKey::NumberEpisodes)}</h3>
                        {select("bulk_scope",TextKey::Scope,d.field("items","bulk_scope","selected"),vec![("selected".into(),tr(TextKey::AddonWorkspaceSelected).to_owned()),("page".into(),tr(TextKey::AddonWorkspacePage).to_owned()),("matching".into(),tr(TextKey::AddonWorkspaceMatching).to_owned()),("all".into(),tr(TextKey::All).to_owned())])}
                        <div class="grid-3">{input("bulk_season",TextKey::Season,d.field("items","bulk_season","1"),"number")}{input("bulk_episode",TextKey::Episode,d.field("items","bulk_episode","1"),"number")}{input("bulk_count",TextKey::Count,d.field("items","bulk_count",""),"number")}</div>
                        <div class="grid-2">{select("start_item",TextKey::RangeStart,d.field("items","start_item",""),std::iter::once((String::new(),tr(TextKey::FirstIncludedItem).into())).chain(rows.iter().map(|r|(r.id.to_string(),r.display_title.clone()))).collect())}{select("end_item",TextKey::RangeEnd,d.field("items","end_item",""),std::iter::once((String::new(),tr(TextKey::AllRemaining).into())).chain(rows.iter().map(|r|(r.id.to_string(),r.display_title.clone()))).collect())}</div>
                        {button("select-range",TextKey::Select,false)}
                        {checkbox("replace_existing",TextKey::ReplaceExistingEpisodeNumbers,d.field("items","replace_existing","false")=="true")}
                        {checkbox("season_only",TextKey::AddonWorkspaceSeasonOnly,d.field("items","season_only","false") == "true")}
                        <details class="length-filter"><summary>{tr(TextKey::AddonWorkspaceLengthFilter)}</summary><div class="length-fields">
                            {input("minimum_length",TextKey::AddonWorkspaceMinimumLength,d.field("items","minimum_length",""),"text")}{input("maximum_length",TextKey::AddonWorkspaceMaximumLength,d.field("items","maximum_length",""),"text")}
                        </div>{checkbox("include_unknown_length",TextKey::AddonWorkspaceUnknownLength,d.field("items","include_unknown_length","false")=="true")}<p>{tr(TextKey::AddonWorkspaceLengthHelp)}</p></details>
                        {button("number",TextKey::Preview,false)}
                        {input("gap_days",TextKey::MinimumSeasonGapDays,d.field("items","gap_days","90"),"number")}
                        {button("auto-number",TextKey::AutoDetectSeasons,false)}{button("clear-numbering",TextKey::Unnumbered,false)}
                    </section><section><h3>{tr(TextKey::MediaKind)}</h3>{kind_select("bulk_kind",d.field("items","bulk_kind","__unchanged"))}{liveness_select("bulk_liveness",d.field("items","bulk_liveness","__unchanged"))}{button("classify",TextKey::Preview,false)}</section></div>
                    {button("bulk-options",TextKey::AddonOptions,false)}
                    {d.groups.get("items").and_then(|f|f.get("option_targets")).and_then(|v|crate::admin::server_fns::addon_list_options::parse_option_items(v).ok()).filter(|ids|!ids.is_empty()).map(|ids|view!{<section class="settings-section"><h3>{tr(TextKey::AddonOptions)}</h3><p>{crate::admin::i18n::selected_count(ids.len())}</p>{option_controls(w,ids)}</section>})}
                </div>
            </details>
            {bulk::preview(w)}
            <div class="numbering-actions editor-savebar numbering-save-bar">{d.undo.as_ref().map(|_|button("undo",TextKey::UndoLastChange,false))}{d.saved_review_undo.as_ref().filter(|u|d.undo.is_none() && u.expected_revision==d.revision).map(|_|button("undo-saved-review",TextKey::UndoLastChange,false))}{button("save",TextKey::SaveChanges,true)}{button("discard",TextKey::AddonWorkspaceDiscard,false)}<span>{format!("{}: {} · {}: {}",tr(TextKey::AddonWorkspaceSelection),d.selected.len(),tr(TextKey::AddonWorkspaceOffPage),d.selected.len().saturating_sub(selected_here))}</span></div>
            <div class="episode-list"><div class="episode-column-head" aria-hidden="true"><span></span><span>{tr(TextKey::Name)}</span><span>{tr(TextKey::FirstAired)}</span><span>{tr(TextKey::Duration)}</span><span>{tr(TextKey::Season)}" / "{tr(TextKey::Episode)}</span><span></span></div>
                <ol class="episode-rows">{visible.iter().map(|row| item_row(w,row)).collect_view()}</ol>
                <p hidden=!rows.is_empty()>{tr(TextKey::NoItems)}</p>
            </div>
            <footer class="pager"><span class="page-summary">{format!("{}–{} / {}",if rows.is_empty() {0} else {page*size+1},((page+1)*size).min(rows.len()),rows.len())}</span>
                {select("page_size",TextKey::AddonWorkspacePageSize,d.page_size.to_string(),[10,25,50,100,0].into_iter().map(|n|(n.to_string(),if n==0 {tr(TextKey::All).to_owned()} else {n.to_string()})).collect())}
                {button("page-size",TextKey::Apply,false)}
                <button class="button secondary" form=FORM name="intent" value=format!("page:{}",page.saturating_sub(1)) disabled={page==0} formnovalidate>{tr(TextKey::PreviousPage)}</button>
                <button class="button secondary" form=FORM name="intent" value=format!("page:{}",page+1) disabled={page+1>=pages} formnovalidate>{tr(TextKey::NextPage)}</button>
            </footer>
        </section>
    }.into_any()
}

fn item_row(w: &Workspace, row: &AddonMediaListItemRow) -> AnyView {
    let d = &w.submission.draft;
    let id = row.id;
    let rank = w.items.iter().position(|r| r.id == id).unwrap_or(0);
    let title = row.display_title.clone();
    let select_name = format!("selected_{id}");
    let gap = rank
        .checked_sub(1)
        .and_then(|i| w.items.get(i))
        .and_then(|previous| parse_first_aired(previous.first_aired.as_deref()))
        .zip(parse_first_aired(row.first_aired.as_deref()))
        .map(|(a, b)| b.signed_duration_since(a).num_days());
    let menu = format!("row-commands-{id}");
    view!{<li class="episode-row" id=format!("item-{id}")>
        <input type="hidden" form=FORM name=format!("review_snapshot_{id}") value=super::review_snapshot(row)/>
        <label class="row-pick"><input type="hidden" form=FORM name=select_name.clone() value="false"/><input type="checkbox" form=FORM name=select_name value="true" checked=d.selected.contains(&id) aria-label=format!("{}: {title}",tr(TextKey::AddonWorkspaceSelection))/><span aria-hidden="true">{format!("{:02}",rank+1)}</span></label>
        <div class="episode-identity">{row.thumbnail_url.clone().map_or_else(||view!{<span class="row-art-placeholder" style=icon_style(presentation_icon(w.form.presentation_kind),"2rem") aria-hidden="true"></span>}.into_any(),|url|view!{<img src=url alt="" loading="lazy"/>}.into_any())}<div>
            <button class="button row-title" form=FORM name="intent" value="navigate" formaction=format!("/media/addon-lists/{}/items/{id}",w.form.id) formnovalidate>{title}</button>
            <small>{tr(if d.field("items", &format!("included_{id}"), row.included.to_string()) == "true" {TextKey::IncludeInPublication} else {TextKey::AddonWorkspaceExcluded})}</small>
            {(row.metadata_conflict_count>0 || row.liveness_conflict).then(||view!{<small>{tr(TextKey::Conflicts)}</small>})}
        </div></div>
        <div class="episode-evidence"><div class="airdate"><span class="mobile-column" aria-hidden="true">{tr(TextKey::FirstAired)}</span><span class="sr-only">{tr(TextKey::FirstAired)}": "</span><time datetime=row.first_aired.clone()>{row.first_aired.clone().unwrap_or_else(||tr(TextKey::UnknownAvailability).into())}</time>{gap.map(|days|view!{<small class="gap-note">{format!("{days} d")}</small>})}</div>
        <div class="runtime"><span class="mobile-column" aria-hidden="true">{tr(TextKey::Duration)}</span><span class="sr-only">{tr(TextKey::Duration)}": "</span><span>{row.duration_seconds.map_or_else(||"—".into(),|n|format!("{}:{:02}",n/60,n%60))}</span>{row.duration_seconds.is_none().then(||view!{<small>{tr(TextKey::UnknownAvailability)}</small>})}</div></div>
        <div class="row-numbering"><div class="inline-pair">
            {[("season",TextKey::Season,row.season),("episode",TextKey::Episode,row.episode)].into_iter().map(|(name,key,saved)|view!{<label>{baseline(&format!("{name}_{id}"),&d.field("items",&format!("{name}_{id}"),saved.map(|n|n.to_string()).unwrap_or_default()))}<span aria-hidden="true">{tr(key).chars().next().map(|c|c.to_string())}</span><input id=format!("workspace-{name}_{id}") type="number" min="0" max="2147483647" form=FORM name=format!("{name}_{id}") value=d.field("items",&format!("{name}_{id}"),saved.map(|n|n.to_string()).unwrap_or_default()) placeholder="—" aria-invalid=w.submission.error_fields.contains(&format!("{name}_{id}")).then_some("true") aria-describedby=w.submission.error_fields.contains(&format!("{name}_{id}")).then_some("workspace-errors") aria-label=format!("{}: {}",tr(key),row.display_title)/></label>}).collect_view()}
        </div><small>{tr(TextKey::ItemSavedValue)}": "{match (row.season,row.episode){(Some(s),Some(e))=>format!("S{s:02}E{e:02}"),_=>tr(TextKey::Unnumbered).into()}}</small></div>
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

fn category_choices(w: &Workspace) -> Vec<(String, String)> {
    [
        ("", TextKey::All),
        ("review", TextKey::NeedReview),
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
