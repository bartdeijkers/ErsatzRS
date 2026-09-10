use super::{
    button, checkbox, choice, format_progress_timestamp, icon_style, input, people_lines,
    presentation_text_key, save_bar, select, select_disabled, textarea, tr, value,
    AddonMediaListDeliveryMode, AddonMediaListDetailPolicy, AddonMediaListNfoForm,
    AddonMediaListPresentationKind, AddonMediaListRefreshCadence,
    AddonMediaListRefreshPublicationPolicy, Draft, IconName, MetadataConflictSummary, TextKey,
    Workspace, FORM,
};
use leptos::prelude::*;

pub(super) fn settings(w: &Workspace) -> AnyView {
    let d = &w.submission.draft;
    let f = &w.form;
    let adaptive = w
        .providers
        .iter()
        .any(|p| p.addon_id == f.addon_id && p.supports_adaptive_refresh);
    view! { <div class="settings-form settings-layout">
        <section class="form-section" id="source">
        <h2 class="section-title">{tr(TextKey::Source)}</h2><dl class="facts"><div><dt>{tr(TextKey::Source)}</dt><dd>{f.source_url.clone()}</dd></div><div><dt>{tr(TextKey::Addon)}</dt><dd>{f.addon_id.clone()}</dd></div></dl>
        <div class="grid-2">
            {select_disabled("refresh_cadence",TextKey::AutomaticRefresh,&d.field("settings","refresh_cadence",value(f.refresh_cadence)),vec![choice(AddonMediaListRefreshCadence::Adaptive,TextKey::RefreshCadenceAdaptive),choice(AddonMediaListRefreshCadence::Daily,TextKey::RefreshCadenceDaily),choice(AddonMediaListRefreshCadence::Weekly,TextKey::RefreshCadenceWeekly),choice(AddonMediaListRefreshCadence::Monthly,TextKey::RefreshCadenceMonthly),choice(AddonMediaListRefreshCadence::Yearly,TextKey::RefreshCadenceYearly),choice(AddonMediaListRefreshCadence::Never,TextKey::RefreshCadenceNever)],(!adaptive && f.refresh_cadence!=AddonMediaListRefreshCadence::Adaptive).then_some("adaptive"))}
            {select("refresh_publication_policy",TextKey::RefreshPublicationPolicy,d.field("settings","refresh_publication_policy",value(f.refresh_publication_policy)),vec![choice(AddonMediaListRefreshPublicationPolicy::Review,TextKey::RefreshPublicationReview),choice(AddonMediaListRefreshPublicationPolicy::AutoPublishAdditions,TextKey::RefreshPublicationAutomatic)])}
        </div>
        <p hidden=adaptive>{tr(TextKey::AdaptiveRefreshUnavailable)}</p><p>{tr(TextKey::RefreshPublicationPolicyHelp)}</p>
        {checkbox("load_item_details",TextKey::LoadItemDetailsAfterPublishing,d.field("settings","load_item_details",(f.detail_import_policy==AddonMediaListDetailPolicy::Automatic).to_string())=="true")}
        {checkbox("generate_playlist",TextKey::GeneratePlaylist,d.field("settings","generate_playlist",f.generate_playlist.to_string())=="true")}
        <dl class="facts"><div><dt>{tr(TextKey::LastSuccessfulRefresh)}</dt><dd>{f.last_refresh_succeeded_at.clone().unwrap_or_default()}</dd></div><div><dt>{tr(TextKey::NextScheduledRefresh)}</dt><dd>{f.next_refresh_at.clone().unwrap_or_default()}</dd></div><div><dt>{tr(TextKey::Retry)}</dt><dd>{f.refresh_retry_at.clone().unwrap_or_default()}</dd></div></dl>
        <div class="source-actions">{button("test-source",TextKey::TestSource,false)}{button("reconcile",TextKey::ReconcileEntireSource,false)}

        </div>
        <div class="grid-2">{select("addon_id",TextKey::ChangeAddon,d.field("settings","addon_id",f.addon_id.clone()),w.providers.iter().map(|p|(p.addon_id.clone(),p.name.clone())).collect())}{button("change-addon-preview",TextKey::Preview,false)}</div>
        {super::detail::source_status(w)}</section>
        <section class="form-section" id="metadata"><h2 class="section-title">{tr(TextKey::EditMetadata)}</h2>
        <div>
            {input("metadata_title",TextKey::Title,d.field("settings","metadata_title",w.metadata.metadata.title.clone()),"text")}
            {select("presentation_kind",TextKey::CollectionType,d.field("settings","presentation_kind",value(f.requested_presentation_kind)),[AddonMediaListPresentationKind::Auto,AddonMediaListPresentationKind::TelevisionShow,AddonMediaListPresentationKind::Movies,AddonMediaListPresentationKind::MusicVideos,AddonMediaListPresentationKind::OtherVideos,AddonMediaListPresentationKind::Songs,AddonMediaListPresentationKind::Images,AddonMediaListPresentationKind::RemoteStreams].into_iter().map(|p|choice(p,presentation_text_key(p))).collect())}
        </div>
        <p data-testid="addon-media-type-change-help"><strong>{tr(TextKey::CurrentLibraryType)}": "</strong>{tr(presentation_text_key(f.presentation_kind))}<span data-testid="addon-media-type-change-route" hidden={f.requested_presentation_kind==AddonMediaListPresentationKind::Auto || f.requested_presentation_kind==f.presentation_kind}>" → "{tr(presentation_text_key(f.requested_presentation_kind))}</span><br/>{tr(TextKey::CollectionTypeChangeHelp)}</p>
        {metadata_fields(d,"settings",&w.metadata,false)}
<button class="button secondary" form=FORM formaction=format!("/media/addon-lists/{}/artwork",f.id) name="intent" value="navigate" formnovalidate>{tr(TextKey::Artwork)}</button>
        </section>
        {save_bar(TextKey::AddonWorkspaceSaveSettings)}
        <section class="work-section"><h3>{tr(TextKey::AddonMediaListFolder)}</h3><p>{f.folder_name.clone()}</p>{button("reset-preview",TextKey::ResetAndReload,false)}</section>
    </div> }.into_any()
}

pub(super) fn metadata_fields(
    d: &Draft,
    group: &str,
    nfo: &AddonMediaListNfoForm,
    item: bool,
) -> AnyView {
    let m = &nfo.metadata;
    let text_fields = [
        (
            "original_title",
            TextKey::OriginalTitle,
            m.original_title.clone().unwrap_or_default(),
        ),
        (
            "sort_title",
            TextKey::SortTitle,
            m.sort_title.clone().unwrap_or_default(),
        ),
        (
            "year",
            TextKey::Year,
            m.year.map(|n| n.to_string()).unwrap_or_default(),
        ),
        ("languages", TextKey::Languages, m.languages.join(", ")),
    ];
    view! {
        <input type="hidden" form=FORM name="metadata_etag" value=d.field(group,"metadata_etag",nfo.etag.clone())/>
        <div class="grid-2">{text_fields.into_iter().map(|(name,key,saved)|input(&format!("metadata_{name}"),key,d.field(group,&format!("metadata_{name}"),saved),if name=="year" {"number"} else {"text"})).collect_view()}</div>
        {textarea("metadata_plot",TextKey::Summary,d.field(group,"metadata_plot",m.plot.clone().unwrap_or_default()))}
        <div class="grid-2">{input("metadata_genres",TextKey::Genres,d.field(group,"metadata_genres",m.genres.join(", ")),"text")}{textarea("metadata_tags",TextKey::Tags,d.field(group,"metadata_tags",m.tags.join("\n")))}</div>
        {if item {view!{<section class="metadata-credits"><h3>{tr(TextKey::CastAndCredits)}</h3><div class="grid-2">            {[ ("studios",TextKey::Studios,m.studios.join(", ")), ("directors",TextKey::Directors,m.directors.join(", ")), ("writers",TextKey::Writers,m.writers.join(", ")), ("artists",TextKey::Artists,m.artists.join(", ")) ].into_iter().map(|(name,key,saved)|input(&format!("metadata_{name}"),key,d.field(group,&format!("metadata_{name}"),saved),"text")).collect_view()}
            {textarea("metadata_people",TextKey::CastAndCredits,d.field(group,"metadata_people",people_lines(&m.people)))}
</div></section>}.into_any()} else {view!{<details class="disclosure" data-disclosure-kind="section"><summary><span class="disclosure-label">{tr(TextKey::CastAndCredits)}</span><span class="disclosure-toggle" aria-hidden="true"></span></summary><div class="disclosure-body grid-2">            {[ ("studios",TextKey::Studios,m.studios.join(", ")), ("directors",TextKey::Directors,m.directors.join(", ")), ("writers",TextKey::Writers,m.writers.join(", ")), ("artists",TextKey::Artists,m.artists.join(", ")) ].into_iter().map(|(name,key,saved)|input(&format!("metadata_{name}"),key,d.field(group,&format!("metadata_{name}"),saved),"text")).collect_view()}
            {textarea("metadata_people",TextKey::CastAndCredits,d.field(group,"metadata_people",people_lines(&m.people)))}
</div></details>}.into_any()}}
        {(!item).then(||view!{<MetadataConflictSummary conflicts=nfo.conflicts.clone()/>})}
    }.into_any()
}

pub(super) fn playback(w: &Workspace) -> AnyView {
    let state = expect_context::<crate::host::state::AppState>();
    let warning = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(
            state
                .application
                .addon_media_list_disk_warning_gb(Some(w.form.id)),
        )
    });
    let supports_download = w
        .providers
        .iter()
        .any(|p| p.addon_id == w.form.addon_id && p.supports_download_ahead);
    let d = &w.submission.draft;
    view! { <div class="settings-layout"><section class="work-section"><h2>{tr(TextKey::AddonDeliveryMode)}</h2>
        <fieldset class="delivery-choices"><legend>{tr(TextKey::AddonDeliveryMode)}</legend><div class="mode-grid">
            {[(AddonMediaListDeliveryMode::Stream,TextKey::AddonDeliveryStream,IconName::Play),(AddonMediaListDeliveryMode::DownloadAhead,TextKey::AddonDeliveryDownloadAhead,IconName::Download),(AddonMediaListDeliveryMode::DownloadAheadNewVideos,TextKey::AddonDeliveryNewVideos,IconName::Download)].into_iter().map(|(mode,key,icon)|view!{<label class="mode-choice"><input form=FORM type="radio" disabled={!supports_download && mode!=AddonMediaListDeliveryMode::Stream && mode!=w.form.delivery_mode} name="delivery_mode" value=value(mode) checked={d.field("playback","delivery_mode",value(w.form.delivery_mode))==value(mode)}/><span class="mode-art" aria-hidden="true"><span style=icon_style(icon,"1.5rem")></span></span><strong>{tr(key)}</strong><small>{tr(match mode {AddonMediaListDeliveryMode::Stream=>TextKey::AddonWorkspaceStreamHelp,AddonMediaListDeliveryMode::DownloadAhead=>TextKey::AddonWorkspaceDownloadHelp,AddonMediaListDeliveryMode::DownloadAheadNewVideos=>TextKey::AddonWorkspaceNewHelp})}</small></label>}).collect_view()}
        </div></fieldset>
        <p id="addon-delivery-help">{tr(TextKey::AddonDeliveryHelp)}</p><p hidden=supports_download>{tr(TextKey::AddonDeliveryUnavailable)}</p>
        {w.form.delivery_cutoff_at.clone().filter(|_|w.form.delivery_mode==AddonMediaListDeliveryMode::DownloadAheadNewVideos).map(|cutoff| {
            let label=chrono::DateTime::parse_from_rfc3339(&cutoff).ok().and_then(|t|format_progress_timestamp(&t.with_timezone(&chrono::Utc).to_rfc3339())).unwrap_or_else(||cutoff.clone());
            view! {<p data-testid="addon-delivery-cutoff">{tr(TextKey::AddonDeliveryCutoff)}": "<time datetime=cutoff>{label}</time></p>}
        })}
        <p hidden={w.form.delivery_mode==AddonMediaListDeliveryMode::DownloadAheadNewVideos}>{tr(TextKey::AddonDeliveryFreshCutoff)}</p>
        </section><section class="work-section"><h2>{tr(TextKey::AddonWorkspaceAccess)}</h2><div class="option-grid access-stack">{option_controls(w,Vec::new())}</div>
        </section><section class="work-section storage-settings"><h3>{tr(TextKey::AddonListDiskWarnings)}</h3>
        {match warning {Ok(n)=>input("disk_warning_gb",TextKey::AddonListDiskWarningGb,d.field("playback","disk_warning_gb",n.to_string()),"number"),Err(_)=>view! {<p role="alert">{tr(TextKey::AddonListDiskWarningFailed)}</p>}.into_any()}}
        <p>{tr(TextKey::AddonListDiskWarningHelp)}</p>
        </section>{save_bar(TextKey::AddonWorkspaceSaveSettings)}
    </div> }.into_any()
}

pub(super) struct OptionViews {
    pub fields: AnyView,
    pub trigger: AnyView,
    pub status: AnyView,
    pub help: AnyView,
}

impl OptionViews {
    fn empty(fields: AnyView) -> Self {
        Self {
            fields,
            trigger: ().into_any(),
            status: ().into_any(),
            help: ().into_any(),
        }
    }
}

pub(super) fn option_controls(w: &Workspace, ids: Vec<i64>) -> AnyView {
    let parts = option_views(w, ids, false);
    view! { {parts.fields}{parts.status} }.into_any()
}

pub(super) fn option_views(w: &Workspace, ids: Vec<i64>, compact: bool) -> OptionViews {
    use crate::admin::pages::media::addon_list_options::{localized, value_label};
    use crate::admin::server_fns::addon_list_options::load_option_editor;
    use ersatzrs_addon_contract::AddonMediaListOptionKind;
    let state = expect_context::<crate::host::state::AppState>();
    let result = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(load_option_editor(&state, w.form.id, ids))
    });
    let editor = match result {
        Ok(Some(editor)) => editor,
        Ok(None) => {
            return OptionViews::empty(if compact {
                ().into_any()
            } else {
                view! {<p>{tr(TextKey::AddonWorkspaceNoOptions)}</p>}.into_any()
            })
        }
        Err(()) => {
            return OptionViews::empty(
                view! {<p role="alert">{tr(TextKey::AddonOptionsConflict)}</p>}.into_any(),
            )
        }
    };
    let trigger = if compact {
        editor
            .access
            .as_ref()
            .map(|a| access_trigger(a, editor.provisional))
            .into_any()
    } else {
        ().into_any()
    };
    let access = editor
        .access
        .as_ref()
        .map(|a| access_controls(a, editor.provisional, compact));
    let help = if compact {
        view!{<section class="option-help"><h3>{tr(TextKey::AddonOptions)}</h3>{editor.fields.iter().map(|f|view!{<div><h4>{localized(&f.definition.label)}</h4><p>{localized(&f.definition.description)}</p></div>}).collect_view()}</section>}.into_any()
    } else {
        ().into_any()
    };
    let list_access = (w.task == "playback" && editor.supports_access_check)
        .then(|| access_target(w, editor.provisional));
    let preview = w.submission.option_previews.iter().filter_map(|preview| {
        let definition=&editor.fields.iter().find(|f|f.definition.key==preview.edit.key)?.definition;
        Some(view!{<section class="settings-section" data-testid="option-preview"><h3>{localized(&definition.label)}</h3><p>{tr(TextKey::AddonOptionsPreviewHelp)}</p><div class="table-scroll"><table><thead><tr><th>{tr(TextKey::Name)}</th><th>{tr(TextKey::AddonOptionsBefore)}</th><th>{tr(TextKey::AddonOptionsAfter)}</th></tr></thead><tbody>{preview.rows.iter().map(|row|view!{<tr><th>{row.item_id.and_then(|id|editor.titles.get(&id).cloned()).unwrap_or_else(||tr(TextKey::AddonOptionsListDefault).into())}</th><td>{value_label(&row.effective_before,definition)}<small>{tr(if row.before.is_none(){TextKey::AddonOptionsInherited}else{TextKey::AddonOptionsOverride})}</small></td><td>{value_label(&row.effective_after,definition)}<small>{tr(if !row.applicable{TextKey::AddonOptionsFiniteOnly}else if row.after.is_none(){TextKey::AddonOptionsInherited}else{TextKey::AddonOptionsOverride})}</small></td></tr>}).collect_view()}</tbody></table></div></section>})
    }).collect_view();
    let fields = view! { <div data-testid="addon-list-options" class=if compact {"item-options access-stack"} else {""}>
        <p hidden=!editor.provisional>{tr(TextKey::AddonOptionsProvisional)}</p>
        {editor.fields.into_iter().map(|field| {
            let name=format!("option_{}",field.definition.key);
            let before=field.rows.first().and_then(|r|r.before.as_ref()).map(|v|serde_json::to_string(v).expect("option serializes")).unwrap_or_default();
            let mixed=field.rows.iter().any(|r|r.before!=field.rows.first().and_then(|r|r.before.clone()));
            let selected=w.submission.draft.field(&w.task,&name,if mixed {"__unchanged".into()} else {before});
            let mut choices=vec![(String::new(),tr(if compact {TextKey::ItemUseListSetting} else {TextKey::AddonOptionsInherited}).to_owned())];
            if mixed {choices.insert(0,("__unchanged".into(),tr(TextKey::AddonOptionsMixed).into()));}
            match field.definition.kind {
                AddonMediaListOptionKind::Boolean=>{choices.push(("true".into(),tr(TextKey::Enabled).to_owned()));choices.push(("false".into(),tr(TextKey::Disabled).to_owned()));},
                AddonMediaListOptionKind::Enum=>choices.extend(field.definition.choices.iter().map(|c|(serde_json::to_string(&c.value).expect("option serializes"),localized(&c.label)))),
            }
            let id=format!("workspace-{name}");
            let applicable=field.rows.iter().any(|r|r.applicable);
            let source = if editor.provisional {TextKey::ItemPackageDefault} else if field.rows.first().is_some_and(|r|r.before.is_some()) {TextKey::ItemOverride} else {TextKey::ItemFromList};
            let state_id = format!("{id}-state");
            let help_id = format!("{id}-help");
            view! { <div class="access-setting option-setting" data-option-key=field.definition.key.clone()><label for=id.clone()>{localized(&field.definition.label)}</label>
                <select form=FORM id=id.clone() name=name disabled=!applicable aria-describedby=format!("{state_id} {help_id}")>{choices.into_iter().map(|(v,label)|view!{<option selected={selected==v} value=v>{label}</option>}).collect_view()}</select>
                <p id=state_id class="option-effective">{tr(if compact && !editor.provisional {TextKey::ItemSavedValue} else {TextKey::EffectiveOptions})}": "{if mixed {tr(TextKey::AddonOptionsMixed).to_owned()}else{field.rows.first().map(|r|value_label(&r.effective_before,&field.definition)).unwrap_or_default()}}" · "{tr(if compact {source} else if field.rows.first().is_some_and(|r|r.before.is_some()){TextKey::AddonOptionsOverride}else{TextKey::AddonOptionsInherited})}</p>
                <div id=help_id><p>{localized(&field.definition.description)}</p><p hidden=applicable>{tr(TextKey::AddonOptionsFiniteOnly)}</p></div>
            </div> }
        }).collect_view()}
        {button("preview-options",if compact {TextKey::ItemPreviewOptions} else {TextKey::Preview},false)}<p hidden=compact>{tr(TextKey::AddonOptionsDraftHelp)}</p>
        {preview}
    </div> }.into_any();
    OptionViews {
        fields,
        trigger,
        status: view! {{access}{list_access}}.into_any(),
        help,
    }
}

fn access_active(access: &crate::admin::server_fns::addon_list_options::ItemAccessEditor) -> bool {
    use ersatzrs_app::ports::AddonItemAccessState;
    access
        .status
        .as_ref()
        .ok()
        .and_then(|s| s.as_ref())
        .is_some_and(|s| {
            matches!(
                s.state,
                AddonItemAccessState::Queued
                    | AddonItemAccessState::Running
                    | AddonItemAccessState::Waiting
            )
        })
}

fn access_trigger(
    access: &crate::admin::server_fns::addon_list_options::ItemAccessEditor,
    provisional: bool,
) -> AnyView {
    let active = access_active(access);
    view!{<span class="item-access-trigger"><button class="button secondary" form=FORM name="intent" value="access-test" disabled={provisional || active} aria-describedby="item-access-reason" formnovalidate>{tr(TextKey::AddonAccessTest)}</button><small id="item-access-reason" hidden={!provisional && !active}>{tr(if provisional {TextKey::AddonAccessSaveFirst} else {TextKey::ItemAccessActive})}</small></span>}.into_any()
}

fn access_controls(
    access: &crate::admin::server_fns::addon_list_options::ItemAccessEditor,
    provisional: bool,
    compact: bool,
) -> AnyView {
    use crate::admin::pages::media::addon_list_options::{access_outcome_key, access_state_key};
    use ersatzrs_app::ports::AddonItemAccessState;
    let failed = access.status.is_err();
    let status = access.status.as_ref().ok().and_then(|s| s.as_ref());
    let active = status.as_ref().is_some_and(|s| {
        matches!(
            s.state,
            AddonItemAccessState::Queued
                | AddonItemAccessState::Running
                | AddonItemAccessState::Waiting
        )
    });
    view!{<section id="addon-item-access" class="settings-section" data-testid="addon-item-access"><h3>{tr(TextKey::AddonAccessTest)}</h3>
        <p>{tr(TextKey::AddonAccessHelp)}</p><p hidden={compact || !provisional}>{tr(TextKey::AddonAccessSaveFirst)}</p><p role="alert" hidden=!failed>{tr(TextKey::AddonAccessError)}</p>
        <p role="status">{tr(status.as_ref().map_or(TextKey::AddonAccessNotChecked,|s|access_state_key(s.state)))}{status.as_ref().and_then(|s|s.outcome).map(|outcome|format!(" · {}",tr(access_outcome_key(outcome))))}</p>
        {status.as_ref().and_then(|s|s.next_eligible_at).map(|date|view!{<p>{tr(TextKey::AddonAccessNextAttempt)}" "{date.to_rfc3339()}</p>})}
        {status.as_ref().and_then(|s|s.expires_at).map(|date|view!{<p>{tr(TextKey::AddonAccessExpires)}" "{date.to_rfc3339()}</p>})}
        <input form=FORM type="hidden" name="access_item_id" value=access.item_id.to_string()/>
        <input form=FORM type="hidden" name="access_job_id" value=status.as_ref().map_or(0,|s|s.job_id).to_string()/>
        <div class="action-row">{(!compact).then(||view!{<button class="button secondary" form=FORM name="intent" value="access-test" disabled={provisional || active} formnovalidate>{tr(TextKey::AddonAccessTest)}</button>})}
            {button("navigate",TextKey::AddonAccessRefresh,false)}
            <button class="button secondary" form=FORM name="intent" value="access-cancel" hidden={compact && !active} disabled=!active formnovalidate>{tr(TextKey::Cancel)}</button>
        </div>
    </section>}.into_any()
}

fn access_target(w: &Workspace, provisional: bool) -> AnyView {
    let Some(first) = w.items.first() else {
        return ().into_any();
    };
    let target = w
        .submission
        .draft
        .field("playback", "access_target_id", first.id.to_string())
        .parse::<i64>()
        .ok()
        .filter(|id| w.items.iter().any(|r| r.id == *id))
        .unwrap_or(first.id);
    let state = expect_context::<crate::host::state::AppState>();
    let status = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current()
            .block_on(state.application.media_list_access_check(w.form.id, target))
    })
    .map_err(|_| ());
    view!{<section class="access-setting">
        {select("access_target_id",TextKey::ReviewItem,target.to_string(),w.items.iter().map(|r|(r.id.to_string(),r.display_title.clone())).collect())}
        {button("navigate",TextKey::Select,false)}
        {access_controls(&crate::admin::server_fns::addon_list_options::ItemAccessEditor {item_id:target,saved_revision:w.form.draft_revision,status},provisional,false)}
    </section>}.into_any()
}
