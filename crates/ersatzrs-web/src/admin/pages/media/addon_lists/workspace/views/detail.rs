use super::{
    artwork_value, button, checkbox, choice, content_kind_value, detail_state_text_key, filtered,
    import_can_retry, import_discovery_summary, input, kind_select, liveness_select,
    metadata_fields, revision_can_restore, save_bar, save_button, select, textarea, tr, value,
    AddonMediaListArtworkRole, AddonMediaListContentKind, AddonMediaListImportState,
    AddonMediaListItemRole, AddonMediaListSourceProbeOutcome, AddonMediaListSourceProbeState,
    PublicationActivity, TextKey, Workspace, FORM,
};
use leptos::prelude::*;

#[expect(
    clippy::too_many_lines,
    reason = "One SSR composition keeps item editing and saved-item tools in document order"
)]
pub(super) fn item(w: &Workspace) -> AnyView {
    let id = w.item_id.unwrap_or_default();
    let Ok(Some(detail)) = super::super::super::load_addon_media_list_item_initial(w.form.id, id)
    else {
        return view! {<p role="alert">{tr(TextKey::AddonMediaListNotFound)}</p>}.into_any();
    };
    let d = &w.submission.draft;
    let group = &w.task;
    let row = &detail.item;
    let details_active = matches!(
        row.detail_state,
        ersatzrs_domain::AddonMediaListDetailState::Queued
            | ersatzrs_domain::AddonMediaListDetailState::Running
            | ersatzrs_domain::AddonMediaListDetailState::WaitingForSlot
    );
    let rows = filtered(w);
    let position = rows.iter().position(|r| r.id == id);
    let previous = position
        .and_then(|p| p.checked_sub(1))
        .and_then(|p| rows.get(p))
        .map(|r| r.id);
    let next = position.and_then(|p| rows.get(p + 1)).map(|r| r.id);
    let options = super::settings::option_views(w, vec![id], true);
    let season = d.field(
        group,
        "season",
        row.season.map(|n| n.to_string()).unwrap_or_default(),
    );
    let episode = d.field(
        group,
        "episode",
        row.episode.map(|n| n.to_string()).unwrap_or_default(),
    );
    let numbering = d.field(
        group,
        "numbering_mode",
        if season.is_empty() && episode.is_empty() {
            "none"
        } else {
            "numbered"
        },
    );
    let first_aired = d.field(
        group,
        "metadata_release_date",
        detail.nfo.metadata.release_date.clone().unwrap_or_default(),
    );
    // Provider metadata can retain timestamps or partial dates. A date input
    // would silently submit an empty value for those existing strings.
    let first_aired_kind = if first_aired.is_empty()
        || super::parse_first_aired(Some(&first_aired))
            .is_some_and(|date| date.to_string() == first_aired)
    {
        "date"
    } else {
        "text"
    };
    let pending_replacement = d
        .groups
        .get(group)
        .filter(|f| f.contains_key("replacement"));
    let replacement_title = pending_replacement.map_or_else(
        || {
            row.replacement_title
                .clone()
                .unwrap_or_else(|| tr(TextKey::NoReplacement).into())
        },
        |f| {
            f.get("replacement_title")
                .filter(|v| !v.is_empty())
                .cloned()
                .unwrap_or_else(|| tr(TextKey::NoReplacement).into())
        },
    );
    let replacement_open = w.submission.replacement_searched
        || !d.field(group, "replacement_query", "").is_empty()
        || pending_replacement.is_some()
        || !w.submission.replacement_results.is_empty()
        || w.submission.error.is_some()
        || (row.replacement_media_item_id.is_some() && !row.replacement_valid);
    view! {
        <div class="item-review settings-layout">
        <input type="hidden" form=FORM name="review_snapshot" value=super::super::review_snapshot(row)/>
        <nav class="item-navigation" aria-label=tr(TextKey::ReviewItem)>
            {[(previous,TextKey::ItemPrevious),(next,TextKey::ItemNext)].into_iter().map(|(target,label)|view!{
                <button class="button secondary item-adjacent" form=FORM name="intent" value="navigate" formaction=format!("/media/addon-lists/{}/items/{}",w.form.id,target.unwrap_or(id)) disabled=target.is_none() formnovalidate>{tr(label)}</button>
            }).collect_view()}
            {options.trigger}
            <button class="button secondary" form=FORM name="intent" value="navigate" formaction=format!("/media/addon-lists/{}#item-{id}",w.form.id) formnovalidate>{tr(TextKey::ItemBack)}</button>
        </nav>
        <section class="item-edit" aria-labelledby="item-review-heading">
            <header class="section-heading"><h2 id="item-review-heading">{tr(TextKey::ReviewItem)}</h2>
                <p class="item-status">{tr(TextKey::ItemPosition)}" "{row.rank.saturating_add(1)}" · "{tr(match row.availability {ersatzrs_domain::AddonMediaListItemAvailability::Available=>TextKey::Available,ersatzrs_domain::AddonMediaListItemAvailability::Unavailable=>TextKey::Unavailable,ersatzrs_domain::AddonMediaListItemAvailability::Unknown=>TextKey::UnknownAvailability})}" · "{tr(item_detail_state_key(row.detail_state))}" · "{tr(TextKey::ItemProviderPlayback)}": "{tr(super::super::super::liveness_text_key(row.provider_liveness))}</p>
            </header>
            <super::MetadataConflictSummary conflicts=detail.nfo.conflicts.clone()/>
            {row.availability_reason.clone().filter(|s|!s.is_empty()).map(|reason|view!{<p class="notice">{reason}</p>})}
            {row.detail_diagnostic.clone().filter(|s|!s.is_empty()).map(|reason|view!{<p class="notice">{reason}</p>})}
            <div class="grid-2 item-basics">
                {input("metadata_title",TextKey::Title,d.field(group,"metadata_title",detail.nfo.metadata.title.clone()),"text")}
                {input("metadata_release_date",TextKey::FirstAired,first_aired,first_aired_kind)}
                {kind_select("content_kind",d.field(group,"content_kind",content_kind_value(row.content_kind)))}
                {(row.kind==ersatzrs_domain::AddonMediaListItemKind::RemoteStream).then(||liveness_select("liveness",d.field(group,"liveness",row.liveness_override.map_or_else(||"inherit".into(),value))))}
            </div>
            <fieldset class="item-numbering"><legend>{tr(TextKey::ItemEpisodePosition)}</legend>
                {super::baseline("numbering_mode",&numbering)}
                <div class="numbering-kind">{[("numbered",TextKey::ItemNumbered),("none",TextKey::Unnumbered)].into_iter().map(|(mode,label)|view!{<label class="check"><input form=FORM type="radio" name="numbering_mode" value=mode checked={numbering==mode}/><span>{tr(label)}</span></label>}).collect_view()}</div>
                <div class="episode-pair">{input("season",TextKey::Season,season,"number")}{input("episode",TextKey::Episode,episode,"number")}</div>
                <small class="numbered-help">{tr(TextKey::ItemNumberingHelp)}</small><small class="unnumbered-help">{tr(TextKey::ItemUnnumberedHelp)}</small>
            </fieldset>
            {checkbox("included",TextKey::IncludeInPublication,d.field(group,"included",row.included.to_string())=="true")}
            {(row.content_kind==AddonMediaListContentKind::Movie || row.kind==ersatzrs_domain::AddonMediaListItemKind::Movie || (row.content_kind==AddonMediaListContentKind::Auto && w.form.requested_presentation_kind==ersatzrs_domain::AddonMediaListPresentationKind::Movies)).then(||select("role",TextKey::MovieFilesAndExtras,d.field(group,"role",value(row.collection_role)),vec![choice(AddonMediaListItemRole::Auto,TextKey::AddonAutoDetect),choice(AddonMediaListItemRole::Primary,TextKey::PrimaryFeature),choice(AddonMediaListItemRole::Extra,TextKey::MovieExtra)]))}
            {options.fields}
            <details class="disclosure" id="item-metadata" data-disclosure-kind="section" open=w.submission.error.is_some()>
                <summary><span class="disclosure-label"><strong>{tr(TextKey::ItemMetadata)}</strong><small>{tr(TextKey::ItemMetadataHelp)}</small></span><span class="disclosure-toggle" aria-hidden="true"></span></summary>
                <div class="disclosure-body">{metadata_fields(d,group,&detail.nfo,true)}</div>
            </details>
            <details class="disclosure" id="item-chapters" data-disclosure-kind="section" open=w.submission.error.is_some()>
                <summary><span class="disclosure-label"><strong>{tr(TextKey::ChapterInput)}</strong></span><span class="disclosure-toggle" aria-hidden="true"></span></summary>
                <div class="disclosure-body">
                {if row.parent_provider_id.is_none() {view!{
                    {textarea("chapter_input",TextKey::ChapterInput,d.field(group,"chapter_input",if row.chapter_input.is_empty(){row.provider_chapter_input.clone()}else{row.chapter_input.clone()}))}
                    <p>{tr(TextKey::ChapterInputHelp)}</p>{button("restore-chapters",TextKey::RestoreProviderChapters,false)}
                }.into_any()} else {view!{<p>{row.provider_chapter_input.clone()}</p>}.into_any()}}
                </div>
            </details>
            <div id="item-save" tabindex="-1" class="apply-bar">
                {save_button(w,TextKey::AddonWorkspaceSaveItem)}{button("discard",TextKey::AddonWorkspaceDiscard,false)}<small>{tr(TextKey::ItemSaveHelp)}</small>
            </div>
        </section>
        <section class="item-tools" aria-labelledby="item-tools-heading">
            <h2 id="item-tools-heading">{tr(TextKey::ItemTools)}</h2>
            {options.status}
            <div class="action-row">
                <button class="button secondary" form=FORM name="intent" value="navigate" formaction=format!("/media/addon-lists/{}#item-{id}",w.form.id) formnovalidate>{tr(TextKey::ItemEditNumbering)}</button>
                <button class="button secondary" form=FORM name="intent" value="navigate" formaction=format!("/media/addon-lists/{}/artwork?item_id={id}",w.form.id) formnovalidate>{tr(TextKey::Artwork)}</button>
            </div>
            <details class="disclosure" id="item-replacement" data-disclosure-kind="section" open=replacement_open>
                <summary><span class="disclosure-label"><strong>{tr(TextKey::LocalReplacement)}</strong><small>{replacement_title}</small></span><span class="disclosure-toggle" aria-hidden="true"></span></summary>
                <div class="disclosure-body">
                    <p>{tr(TextKey::ItemReplacementHelp)}</p>
                    <p class="notice" hidden={row.replacement_media_item_id.is_none() || row.replacement_valid}>{tr(TextKey::InvalidReplacement)}</p>
                    <p role="status" hidden=pending_replacement.is_none()>{tr(TextKey::ItemReplacementPending)}</p>
                    {input("replacement_query",TextKey::ItemSearchReplacement,d.field(group,"replacement_query",""),"search")}
                    <div class="action-row">{button("replacement-search",TextKey::Search,false)}{button("replacement-clear",TextKey::ItemRemoveReplacement,false)}</div>
                    <p role="status" hidden={!w.submission.replacement_searched || !w.submission.replacement_results.is_empty()}>{tr(TextKey::NoMatchingRecordsFound)}</p>
                    {w.submission.replacement_results.iter().map(|r|view!{<div class="replacement-result"><span>{r.title.clone()}</span><small>{r.path.clone()}</small><button class="button secondary" form=FORM name="intent" value=format!("replacement:{}:{}",r.media_item_id,r.media_version_id) formnovalidate>{tr(TextKey::Select)}</button></div>}).collect_view()}
                    <a class="button secondary" href="#item-save">{tr(TextKey::AddonWorkspaceSaveItem)}</a>
                </div>
            </details>
            <details class="disclosure" id="item-information" data-disclosure-kind="section">
                <summary><span class="disclosure-label"><strong>{tr(TextKey::ItemInformation)}</strong><small>{tr(TextKey::ItemInformationHelp)}</small></span><span class="disclosure-toggle" aria-hidden="true"></span></summary>
                <div class="disclosure-body">
                    <p>{tr(TextKey::ItemSavedToolsHelp)}</p>
                    <div class="action-row"><button class="button secondary" form=FORM name="intent" value="fetch-details" disabled=details_active aria-busy=details_active.to_string() formnovalidate>{tr(TextKey::FetchDetails)}</button>{button("navigate",TextKey::AddonAccessRefresh,false)}
                    {row.remote_stream_id.map(|media_id|view!{<a class="button secondary" href=format!("/media/remote/streams/{media_id}")>{tr(TextKey::ItemViewMedia)}</a>})}</div>
                    {options.help}
                </div>
            </details>
        </section>
        </div>
    }.into_any()
}

fn item_detail_state_key(state: ersatzrs_domain::AddonMediaListDetailState) -> TextKey {
    use ersatzrs_domain::AddonMediaListDetailState;
    match state {
        AddonMediaListDetailState::NotRequested => TextKey::ItemDetailsNotRequested,
        AddonMediaListDetailState::Failed => TextKey::ItemDetailsFailed,
        AddonMediaListDetailState::WaitingToRetry => TextKey::ItemDetailsRetry,
        _ => detail_state_text_key(state),
    }
}

pub(super) fn artwork(w: &Workspace) -> AnyView {
    let metadata = if let Some(id) = w.item_id {
        match super::super::super::load_addon_media_list_item_initial(w.form.id, id) {
            Ok(Some(detail)) => detail.nfo,
            _ => {
                return view! {<p role="alert">{tr(TextKey::AddonMediaListNotFound)}</p>}.into_any()
            }
        }
    } else {
        w.metadata.clone()
    };
    let group = &w.task;
    view!{<section class="settings-layout"><h2>{tr(TextKey::Artwork)}</h2>
        <button class="button secondary" form=FORM name="intent" value="navigate" formaction=w.item_id.map_or_else(||format!("/media/addon-lists/{}/settings",w.form.id),|id|format!("/media/addon-lists/{}/items/{id}",w.form.id)) formnovalidate>{tr(TextKey::Back)}</button>
        {[("poster",AddonMediaListArtworkRole::Poster,TextKey::Poster),("fanart",AddonMediaListArtworkRole::Fanart,TextKey::FanArt),("thumbnail",AddonMediaListArtworkRole::Thumb,TextKey::Thumbnail)].into_iter().map(|(name,role,label)| {
            let selected=w.submission.draft.field(group,&format!("metadata_{name}"),artwork_value(&metadata.metadata.artwork,role));
            let suggestions=format!("artwork-{name}-candidates");
            view!{<section class="artwork-row"><h3>{tr(label)}</h3>
                <img class="artwork-preview" src=artwork_preview_url(&selected) alt=tr(label) hidden=selected.is_empty()/>
                <label class="field"><span>{tr(label)}</span><input form=FORM name=format!("metadata_{name}") value=selected type="text" inputmode="url" list=suggestions.clone()/></label>
                <datalist id=suggestions>{metadata.metadata.artwork.iter().filter(|a|a.role==role).map(|a|view!{<option value=a.url.clone()>{format!("{} × {} · {}",a.width.unwrap_or_default(),a.height.unwrap_or_default(),a.language.clone().unwrap_or_default())}</option>}).collect_view()}</datalist>
                <label class="field"><span>{tr(TextKey::UploadImage)}</span><input form=FORM type="file" name=format!("upload_{name}") accept="image/jpeg,image/png,image/webp"/></label>
                <button class="button secondary" form=FORM name="intent" value=format!("upload:{name}") formenctype="multipart/form-data" formnovalidate>{tr(TextKey::UploadImage)}</button>
            </section>}
        }).collect_view()}
        {metadata.metadata.artwork.iter().filter(|a|a.role==AddonMediaListArtworkRole::Other).map(|a|view!{<figure><img class="artwork-preview" src=artwork_preview_url(&a.url) alt=tr(TextKey::Artwork)/><figcaption>{a.language.clone()}</figcaption></figure>}).collect_view()}
        <input type="hidden" form=FORM name="metadata_etag" value=w.submission.draft.field(group,"metadata_etag",metadata.etag.clone())/>
        {save_bar(w,TextKey::AddonWorkspaceSaveSettings)}
    </section>}.into_any()
}

pub(super) fn recovery(w: &Workspace) -> AnyView {
    view! { <super::super::publication::PublicationRecoveryIsland list_id=w.form.id initial=w.readiness.clone()/> }.into_any()
}

/// Latest failed preparation attempts. Earlier saved revisions stay visible as
/// history so an older failure is explained, but never reads as current.
fn preparation_failures(w: &Workspace) -> AnyView {
    let r = &w.readiness;
    (!r.prepared_media_failures.is_empty()).then(|| view!{
        <section class="settings-section" data-testid="addon-preparation-failures"><h2>{tr(TextKey::AddonWorkspacePreparationActivity)}</h2>
            {r.prepared_media_failures.iter().map(|f| {
                let current = f.saved_revision == r.saved_revision;
                view!{<div class=if current {"notice error"} else {"notice"} data-preparation-revision=f.saved_revision.to_string()>
                    <p>{tr(super::super::publication::preparation_failure_text_key(f.reason))}</p>
                    <dl class="facts"><div><dt>{tr(if current {TextKey::SavedRevision} else {TextKey::PreviousSavedRevision})}</dt><dd>{f.saved_revision}</dd></div><div><dt>{tr(TextKey::PreparationFailedItems)}</dt><dd>{f.count}</dd></div></dl>
                </div>}
            }).collect_view()}
        </section>
    }).into_any()
}

pub(super) fn activity(w: &Workspace) -> AnyView {
    view!{<section class="settings-layout">{recovery(w)}{preparation_failures(w)}
        <div class="action-row">{button("test-source",TextKey::TestSource,false)}{button("reconcile",TextKey::ReconcileEntireSource,false)}<a class="button secondary" href=format!("/api/addon-lists/{}/publication-diagnostics",w.form.id) download>{tr(TextKey::DownloadRedactedDiagnostics)}</a></div>
        {source_status(w)}{changes(w)}
        <PublicationActivity list_id=w.form.id history=RwSignal::new(w.history.clone()) hold=RwSignal::new(w.form.auto_publication_hold.clone())/>
        <h2>{tr(TextKey::RevisionHistory)}</h2><p>{tr(TextKey::RevisionHistoryHelp)}</p><p hidden=!w.revisions.is_empty()>{tr(TextKey::NoRevisionHistory)}</p>
        {w.revisions.iter().map(|r|view!{<section class="settings-section"><strong>{r.revision}</strong><p>{r.completed_at.clone()}</p><button class="button secondary" form=FORM name="intent" value=format!("restore-preview:{}",r.revision) disabled=!revision_can_restore(r.restorable,r.revision,w.form.published_revision) formnovalidate>{tr(TextKey::RestoreRevision)}</button></section>}).collect_view()}
    </section>}.into_any()
}

pub(super) fn confirmation(w: &Workspace) -> AnyView {
    let Some(operation) = w.submission.draft.confirmation.as_deref() else {
        return ().into_any();
    };
    let reset = operation == "reset-preview";
    let restore = operation.starts_with("restore-preview:");
    let label = if reset {
        TextKey::ResetAndReload
    } else if restore {
        TextKey::RestoreRevision
    } else {
        TextKey::ChangeAddon
    };
    view!{<section class="notice" role="region" tabindex="-1" autofocus=w.submission.error.is_none() aria-labelledby="workspace-confirm-heading">
        <h2 id="workspace-confirm-heading">{tr(label)}</h2><p><strong>{w.form.name.clone()}</strong></p>
        <dl class="facts">{[(TextKey::PublishedItems,w.impact.published_items),(TextKey::GeneratedPlaylists,w.impact.generated_playlists),(TextKey::Channels,w.impact.channels),(TextKey::Schedules,w.impact.schedules),(TextKey::ActiveOrFuturePlayouts,w.impact.active_or_future_playouts)].into_iter().map(|(key,n)|view!{<div><dt>{tr(key)}</dt><dd>{n}</dd></div>}).collect_view()}</dl>
        <p>{tr(if restore {TextKey::RevisionHistoryHelp} else if reset {TextKey::ResetReloadFailureWarning} else {TextKey::ChangeAddon})}</p>
        {reset.then(||view!{<p>{tr(TextKey::ResetRemovesItems)}" · "{tr(TextKey::ResetRemovesArtifacts)}" · "{tr(TextKey::ResetRetainsSettings)}</p>{input("confirmed_name",TextKey::Name,w.submission.draft.field(&w.task,"confirmed_name",""),"text")} })}
        <input form=FORM type="hidden" name="expected_published_revision" value=w.impact.published_revision.to_string()/>
        {checkbox("acknowledged",label,false)}
        <div class="action-row">{button("confirm-operation",TextKey::Confirm,true)}{button("cancel-confirmation",TextKey::Cancel,false)}</div>
    </section>}.into_any()
}

pub(super) fn source_status(w: &Workspace) -> AnyView {
    w.source.as_ref().map(|probe| {
        let key=match probe.state {
            AddonMediaListSourceProbeState::Queued=>TextKey::SourceTestQueued,
            AddonMediaListSourceProbeState::Running=>TextKey::SourceTestRunning,
            AddonMediaListSourceProbeState::Stale=>TextKey::SourceTestStale,
            AddonMediaListSourceProbeState::Complete=>match probe.outcome {Some(AddonMediaListSourceProbeOutcome::Available)=>TextKey::SourceAvailable,Some(AddonMediaListSourceProbeOutcome::Unsupported)=>TextKey::SourceUnsupported,_=>TextKey::SourceTestFailed},
        };
        view!{<section class="settings-section" role="status"><h3>{tr(key)}</h3><dl class="facts">
            <div><dt>{tr(TextKey::SavedRevision)}</dt><dd>{probe.saved_draft_revision}</dd></div>
            <div><dt>{tr(TextKey::ProviderIdentity)}</dt><dd>{probe.provider_id.clone()}</dd></div>
            <div><dt>{tr(TextKey::Title)}</dt><dd>{probe.list_title.clone()}</dd></div>
            <div><dt>{tr(TextKey::FirstPageComplete)}</dt><dd>{probe.page_complete.map(|yes|tr(if yes {TextKey::Yes}else{TextKey::No}))}</dd></div>
            <div><dt>{tr(TextKey::SourceRevision)}</dt><dd>{probe.source_revision.clone()}</dd></div>
            <div><dt>{tr(TextKey::ReturnedFirstPageItems)}</dt><dd>{probe.returned_item_count}</dd></div>
            <div><dt>{tr(TextKey::ProviderTotalItemCount)}</dt><dd>{probe.count_hint}</dd></div>
            <div><dt>{tr(TextKey::Capabilities)}</dt><dd>{probe.capabilities.join(", ")}</dd></div>
            <div><dt>{tr(TextKey::Duration)}</dt><dd>{probe.duration_milliseconds.map(|n|format!("{n} ms"))}</dd></div>
            <div><dt>{tr(TextKey::PublicationAttempts)}</dt><dd>{probe.attempt_count}</dd></div>
            <div><dt>{tr(TextKey::LastProgress)}</dt><dd><time datetime=probe.updated_at.clone()>{probe.updated_at.clone()}</time></dd></div>
            {probe.completed_at.as_ref().map(|date|view!{<div><dt>{tr(TextKey::Completed)}</dt><dd><time datetime=date.clone()>{date.clone()}</time></dd></div>})}
        </dl><p>{probe.diagnostic.clone().or(probe.diagnostic_code.clone())}</p>{button("navigate",TextKey::Refresh,false)}</section>}
    }).into_any()
}

fn changes(w: &Workspace) -> AnyView {
    let d = &w.diff;
    let comparison = w.submission.comparison.as_ref().or(w.comparison.as_ref());
    view!{<section class="settings-section"><h2>{tr(TextKey::DraftChanges)}</h2><p>{tr(TextKey::ExactDraftDiffHelp)}</p>
        <p hidden=d.exact>{tr(TextKey::DraftDiffUnavailable)}</p>
        <dl class="facts">{[(TextKey::Added,d.added),(TextKey::MetadataChanges,d.metadata_changed),(TextKey::ExcludedItems,d.excluded),(TextKey::Unavailable,d.unavailable),(TextKey::UnknownAvailability,d.unknown),(TextKey::Conflicts,d.conflicted),(TextKey::PendingRemovals,d.pending_removals),(TextKey::Reappearances,d.reappeared),(TextKey::RemovalCandidates,d.removal_candidates)].into_iter().map(|(key,n)|view!{<div><dt>{tr(key)}</dt><dd>{n}</dd></div>}).collect_view()}</dl>
        {w.items.iter().filter(|r|d.pending_removal_item_ids.contains(&r.id)).map(|r|view!{<div class="action-row"><span>{r.display_title.clone()}</span><button class="button secondary" form=FORM name="intent" value=format!("remove:{}:{}",r.id,d.pending_removal_generations.get(&r.id).copied().unwrap_or(0)) formnovalidate>{tr(TextKey::ConfirmRemoval)}</button></div>}).collect_view()}
        {(w.revisions.len()>1).then(||view!{
            <div class="grid-2">{select("compare_left",TextKey::SourceRevision,w.submission.draft.field("activity","compare_left",w.revisions[0].revision.to_string()),w.revisions.iter().map(|r|(r.revision.to_string(),r.revision.to_string())).collect())}{select("compare_right",TextKey::TargetPublishedRevision,w.submission.draft.field("activity","compare_right",w.revisions[1].revision.to_string()),w.revisions.iter().map(|r|(r.revision.to_string(),r.revision.to_string())).collect())}</div>
            {button("compare",TextKey::CompareRevisions,false)}
        })}
        {comparison.map(|d|view!{<dl class="facts" data-testid="revision-diff">{[(TextKey::Added,d.items_added),(TextKey::Removed,d.items_removed),(TextKey::Changed,d.items_changed),(TextKey::MetadataChanges,u64::from(d.list_metadata_changed)),(TextKey::EffectiveOptions,u64::from(d.effective_options_changed)),(TextKey::Files,d.files_changed),(TextKey::Artifacts,d.artifacts_added)].into_iter().map(|(key,n)|view!{<div><dt>{tr(key)}</dt><dd>{n}</dd></div>}).collect_view()}</dl>})}
    </section>}.into_any()
}

pub(super) fn import_progress(w: &Workspace) -> AnyView {
    w.form.import_progress.as_ref().map(|p| {
        let actions=if import_can_retry(p) {vec![("retry",TextKey::RetryImportNow)]} else {match p.state {
            AddonMediaListImportState::Queued=>vec![("cancel",TextKey::CancelImport)],
            AddonMediaListImportState::Discovering|AddonMediaListImportState::Verifying=>vec![("pause",TextKey::PauseImport),("cancel",TextKey::CancelImport)],
            AddonMediaListImportState::Paused=>vec![("resume",TextKey::ResumeImport),("cancel",TextKey::CancelImport)],
            _=>Vec::new(),
        }};
        let (_,current,total)=super::super::super::progress_sample(p);
        let phase=super::super::super::import_progress_phase(p,w.form.published_revision>0,w.form.detail_import_policy);
        view!{<section class="settings-section" id="addon-list-import-progress" data-import-state=format!("{:?}",p.state)><h3>{tr(phase)}</h3><p aria-live="polite">{tr(phase)}" · "{import_discovery_summary(p)}</p>
            {total.map(|total|view!{<progress role="progressbar" max=total.max(1).to_string() value=current.min(total).to_string() aria-valuemin="0" aria-valuemax=total.to_string() aria-valuenow=current.min(total).to_string()></progress>})}
            <p>{format!("{} / {} {} · {} {} · {} {}",p.detail_complete,p.detail_total,tr(TextKey::DetailsComplete),p.detail_skipped,tr(TextKey::DetailsSkipped),p.detail_failed,tr(TextKey::ItemDetailsFailed))}</p>
            <p>{p.last_progress_at.clone()}</p>
            <div class="action-row">{actions.into_iter().map(|(intent,key)|button(&format!("import:{intent}"),key,false)).collect_view()}{button("navigate",TextKey::Refresh,false)}</div>
        </section>}
    }).into_any()
}

fn artwork_preview_url(url: &str) -> String {
    ersatzrs_infra::addon_artwork::upload_id(url).map_or_else(
        || url.to_owned(),
        |id| format!("/api/addon-media-lists/artwork/{id}"),
    )
}
