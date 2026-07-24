//! Playouts list + detail page — ports `Pages/Playouts.razor`.
//!
//! Action bar (Add Playout menu + Reset All Playouts), a searchable paged
//! playouts table with per-kind row actions (Edit / Refresh|Reset /
//! Schedule Reset / Delete / Edit External Json), a per-row build spinner and
//! build-failure highlighting, and a paged playout-detail table with a
//! Show Filler toggle and per-item Troubleshoot dialog.
#![allow(clippy::too_many_lines)]
use leptos::prelude::*;

use crate::admin::i18n::{playout_reset_prompt, playout_schedule_kind_label, tr, TextKey};
use crate::admin::server_fns::playouts::{
    count_playout_detail_items, count_playouts, delete_playout, get_show_filler_pref,
    list_playout_detail_items, list_playouts, process_scheduling_context, reset_all_playouts,
    reset_playout, save_playout_daily_reset, set_show_filler_pref, update_external_json_playout,
    PlayoutDetailItem, PlayoutListRow, KIND_BLOCK, KIND_CLASSIC, KIND_EXTERNAL_JSON, KIND_SCRIPTED,
    KIND_SEQUENTIAL,
};
use crate::admin::ui::primitives::dialog::{DeleteDialog, Modal};
use crate::admin::ui::primitives::form::TextField;
use crate::admin::ui::primitives::icon::{icon_style, IconName};
use crate::admin::ui::primitives::resource_state::{
    admin_loading_resource_view, admin_result_view,
};
use crate::admin::ui::primitives::toast::{load_or_notify, use_toast, ToastSeverity};

stylance::import_crate_style!(style, "src/admin/pages/scheduling/playouts.module.css");

const LIST_PAGE_SIZE: u32 = 10;
const DETAIL_PAGE_SIZE: u32 = 10;

#[component]
pub fn PlayoutsPage() -> impl IntoView {
    let initial = Resource::new_blocking(
        || (),
        |()| async move {
            tokio::try_join!(
                list_playouts(String::new(), 0, LIST_PAGE_SIZE),
                count_playouts(),
                get_show_filler_pref(),
            )
        },
    );

    view! {
        <Suspense fallback=admin_loading_resource_view>
            {move || {
                initial.get().map(|result| admin_result_view(
                    result,
                    |(initial_rows, initial_count, initial_show_filler)| view! {
                        <PlayoutsIsland
                            initial_rows=initial_rows
                            initial_count=initial_count
                            initial_show_filler=initial_show_filler
                        />
                    }.into_any(),
                ))
            }}
        </Suspense>
    }
}

#[island]
fn PlayoutsIsland(
    initial_rows: Vec<PlayoutListRow>,
    initial_count: u64,
    initial_show_filler: bool,
) -> impl IntoView {
    // -- list state --
    let search = RwSignal::new(String::new());
    let page = RwSignal::new(0u32);
    let add_menu_open = RwSignal::new(false);

    let current_list_rows = RwSignal::new(initial_rows);
    let current_list_count = RwSignal::new(initial_count);
    let load_toast = use_toast();
    let list_rows: LocalResource<Vec<PlayoutListRow>> = LocalResource::new(move || {
        let query = search.get();
        let page = page.get();
        async move { load_or_notify(list_playouts(query, page, LIST_PAGE_SIZE).await, load_toast) }
    });
    let list_count: LocalResource<u64> =
        LocalResource::new(
            move || async move { load_or_notify(count_playouts().await, load_toast) },
        );
    Effect::new(move |_| {
        if let Some(rows) = list_rows.get() {
            current_list_rows.set(rows);
        }
    });
    Effect::new(move |_| {
        if let Some(count) = list_count.get() {
            current_list_count.set(count);
        }
    });
    let list_total = Signal::derive(move || Some(current_list_count.get()));

    // -- selection / detail state --
    let selected_id: RwSignal<Option<i64>> = RwSignal::new(None);
    let build_message = RwSignal::new(String::new());
    let show_filler = RwSignal::new(initial_show_filler);
    let detail_page = RwSignal::new(0u32);

    // Refresh the persisted Show Filler preference after hydration.
    let filler_pref: LocalResource<bool> = LocalResource::new(move || async move {
        load_or_notify(get_show_filler_pref().await, load_toast)
    });
    Effect::new(move |_| {
        if let Some(value) = filler_pref.get() {
            show_filler.set(value);
        }
    });

    let current_detail_rows = RwSignal::new(Vec::<PlayoutDetailItem>::new());
    let current_detail_count = RwSignal::new(0u64);
    let detail_rows: LocalResource<Vec<PlayoutDetailItem>> = LocalResource::new(move || {
        let id = selected_id.get();
        let filler = show_filler.get();
        let page = detail_page.get();
        async move {
            match id {
                Some(id) => load_or_notify(
                    list_playout_detail_items(id, filler, page, DETAIL_PAGE_SIZE).await,
                    load_toast,
                ),
                None => Vec::new(),
            }
        }
    });
    let detail_count: LocalResource<u64> = LocalResource::new(move || {
        let id = selected_id.get();
        let filler = show_filler.get();
        async move {
            match id {
                Some(id) => {
                    load_or_notify(count_playout_detail_items(id, filler).await, load_toast)
                }
                None => 0,
            }
        }
    });
    Effect::new(move |_| {
        if let Some(rows) = detail_rows.get() {
            current_detail_rows.set(rows);
        }
    });
    Effect::new(move |_| {
        if let Some(count) = detail_count.get() {
            current_detail_count.set(count);
        }
    });
    let detail_total = Signal::derive(move || Some(current_detail_count.get()));

    // -- build activity polling --
    let locked: RwSignal<Vec<i64>> = RwSignal::new(Vec::new());
    Effect::new(move |_| {
        #[cfg(target_arch = "wasm32")]
        {
            use crate::admin::server_fns::playouts::playout_build_activity;
            let activity_toast = use_toast();
            leptos::task::spawn_local(async move {
                let mut previous: Vec<i64> = Vec::new();
                let mut reported_failure = false;
                loop {
                    match playout_build_activity().await {
                        Ok(activity) => {
                            reported_failure = false;
                            let mut ids = activity.queued;
                            ids.extend(activity.building);
                            ids.sort_unstable();
                            ids.dedup();
                            // When accepted work reaches a terminal state, refetch
                            // the persisted result and failure status.
                            let just_unlocked = previous.iter().any(|id| !ids.contains(id));
                            if just_unlocked {
                                list_rows.refetch();
                                list_count.refetch();
                                detail_rows.refetch();
                                detail_count.refetch();
                            }
                            previous = ids.clone();
                            locked.set(ids);
                        }
                        Err(error) if !reported_failure => {
                            activity_toast.push(
                                format!("{}: {error}", tr(TextKey::ErrorOccurred)),
                                ToastSeverity::Error,
                            );
                            reported_failure = true;
                        }
                        Err(_) => {}
                    }
                    gloo_timers::future::TimeoutFuture::new(1_500).await;
                }
            });
        }
    });

    // -- actions --
    let reset_action = Action::new(|(id, kind): &(i64, i32)| {
        let (id, kind) = (*id, *kind);
        async move { reset_playout(id, kind).await }
    });
    let reset_all_action = Action::new(|(): &()| async move { reset_all_playouts().await });
    let delete_action = Action::new(|id: &i64| delete_playout(*id));
    let schedule_reset_action = Action::new(|(id, minutes): &(i64, Option<i32>)| {
        let (id, minutes) = (*id, *minutes);
        async move { save_playout_daily_reset(id, minutes).await }
    });
    let json_action = Action::new(|(id, file): &(i64, String)| {
        let (id, file) = (*id, file.clone());
        async move { update_external_json_playout(id, file).await }
    });
    let context_action = Action::new(|ctx: &String| {
        let ctx = ctx.clone();
        async move { process_scheduling_context(ctx).await }
    });
    let save_filler_action = Action::new(|value: &bool| set_show_filler_pref(*value));
    let reset_toast = use_toast();
    Effect::new(move |_| {
        if let Some(Ok(_)) = reset_action.value().get() {
            reset_toast.push(
                tr(TextKey::PlayoutBuildQueued).to_string(),
                ToastSeverity::Success,
            );
        }
    });
    let reset_all_toast = use_toast();
    Effect::new(move |_| {
        if let Some(Ok(result)) = reset_all_action.value().get() {
            if result.accepted > 0 {
                reset_all_toast.push(
                    format!("{} {}", result.accepted, tr(TextKey::PlayoutBuildsQueued)),
                    ToastSeverity::Success,
                );
            }
        }
    });

    // -- delete dialog --
    let delete_target: RwSignal<Option<PlayoutListRow>> = RwSignal::new(None);
    let delete_open = RwSignal::new(false);
    let delete_name = Signal::derive(move || {
        delete_target
            .get()
            .map(playout_entity_name)
            .unwrap_or_default()
    });
    Effect::new(move |_| {
        if matches!(delete_action.value().get(), Some(Ok(()))) {
            list_rows.refetch();
            list_count.refetch();
            delete_open.set(false);
        }
    });

    // -- schedule reset dialog --
    let schedule_target: RwSignal<Option<PlayoutListRow>> = RwSignal::new(None);
    let schedule_open = RwSignal::new(false);
    let schedule_minutes: RwSignal<Option<i32>> = RwSignal::new(None);
    let schedule_prompt = Signal::derive(move || {
        schedule_target
            .get()
            .map(|r| playout_reset_prompt(&r.channel_name, &r.schedule_name))
            .unwrap_or_default()
    });
    Effect::new(move |_| {
        if matches!(schedule_reset_action.value().get(), Some(Ok(()))) {
            list_rows.refetch();
            schedule_open.set(false);
        }
    });

    // -- edit external json dialog --
    let json_target: RwSignal<Option<PlayoutListRow>> = RwSignal::new(None);
    let json_open = RwSignal::new(false);
    let json_value = RwSignal::new(String::new());
    Effect::new(move |_| {
        if matches!(json_action.value().get(), Some(Ok(()))) {
            list_rows.refetch();
            json_open.set(false);
            selected_id.set(None);
        }
    });

    // -- scheduling context dialog --
    let context_open = RwSignal::new(false);
    let context_title = RwSignal::new(String::new());
    let context_body = RwSignal::new(String::new());
    Effect::new(move |_| {
        if let Some(Ok(body)) = context_action.value().get() {
            context_body.set(body);
            context_open.set(true);
        }
    });

    // Clicking a row selects it; only Classic/Block/Sequential/Scripted show
    // detail (ErsatzTV `PlayoutSelected`).
    let select_row = move |row: &PlayoutListRow| {
        detail_page.set(0);
        if row.schedule_kind == KIND_EXTERNAL_JSON {
            selected_id.set(None);
            build_message.set(String::new());
        } else {
            selected_id.set(Some(row.id));
            let msg = if row.build_success == Some(false) {
                row.build_message.clone().unwrap_or_default()
            } else {
                String::new()
            };
            build_message.set(msg);
        }
    };

    view! {
        <div class=style::page>
            // ---- action bar ----
            <div class=style::action_bar>
                <div class=style::add_menu>
                    <button
                        class=style::btn_add
                        on:click=move |_| add_menu_open.update(|o| *o = !*o)
                    >
                        <span style=icon_style(IconName::Add, "1.05rem") aria-hidden="true"/>
                        {tr(TextKey::AddPlayout)}
                        <span style=icon_style(IconName::ChevronDown, "1.05rem") aria-hidden="true"/>
                    </button>
                    <div
                        class=style::add_menu_items
                        style:display=move || if add_menu_open.get() { "" } else { "none" }
                    >
                        <a class=style::menu_item href="/playouts/add">
                            {tr(TextKey::ClassicScheduleMenu)}
                        </a>
                        <a class=style::menu_item href="/playouts/add/Block">
                            {tr(TextKey::BlockScheduleMenu)}
                        </a>
                        <a class=style::menu_item href="/playouts/add/Sequential">
                            {tr(TextKey::SequentialScheduleMenu)}
                        </a>
                        <a class=style::menu_item href="/playouts/add/Scripted">
                            {tr(TextKey::ScriptedScheduleMenu)}
                        </a>
                        <a class=style::menu_item href="/playouts/add/ExternalJson">
                            <span
                                class=style::menu_warn
                                style=icon_style(IconName::Warning, "1rem")
                                aria-hidden="true"
                            />
                            {tr(TextKey::JsonDizqueTvScheduleMenu)}
                        </a>
                    </div>
                </div>
                <button
                    class=style::btn_reset_all
                    on:click=move |_| {
                        selected_id.set(None);
                        reset_all_action.dispatch(());
                    }
                >
                    <span style=icon_style(IconName::Refresh, "1.05rem") aria-hidden="true"/>
                    {tr(TextKey::ResetAllPlayouts)}
                </button>
            </div>

            <section class=style::content>
                <h1 class=style::page_title>{tr(TextKey::Playouts)}</h1>
                <div class=style::divider/>

                <div class=style::table_toolbar>
                    <span
                        class=style::search_icon
                        style=icon_style(IconName::FilterList, "1.1rem")
                        aria-hidden="true"
                    />
                    <input
                        class=style::search_input
                        type="search"
                        placeholder=tr(TextKey::SearchForPlayouts)
                        on:input=move |e| {
                            selected_id.set(None);
                            page.set(0);
                            search.set(event_target_value(&e));
                        }
                    />
                </div>

                <div class=style::table_wrap>
                    <table>
                        <thead>
                            <tr>
                                <th>{tr(TextKey::Channel)}</th>
                                <th class=style::col_md>{tr(TextKey::DefaultSchedule)}</th>
                                <th class=style::col_md>{tr(TextKey::ScheduleKind)}</th>
                                <th class=style::col_actions/>
                            </tr>
                        </thead>
                        <tbody>
                            {move || {
                                let rows = current_list_rows.get();
                                if rows.is_empty() {
                                    return view! {
                                        <tr><td colspan="4" class=style::empty>
                                            {tr(TextKey::NoPlayoutsFound)}
                                        </td></tr>
                                    }.into_any();
                                }
                                rows.into_iter().map(|row| {
                                    playout_row(
                                        row,
                                        selected_id,
                                        locked,
                                        select_row,
                                        reset_action,
                                        delete_target,
                                        delete_open,
                                        schedule_target,
                                        schedule_minutes,
                                        schedule_open,
                                        json_target,
                                        json_value,
                                        json_open,
                                    )
                                }).collect_view().into_any()
                            }}
                        </tbody>
                    </table>
                </div>

                <PlayoutPager page=page total=list_total page_size=LIST_PAGE_SIZE/>

                // ---- detail section ----
                <section
                    class=style::detail_section
                    style:display=move || if selected_id.get().is_some() { "" } else { "none" }
                >
                    <h2 class=style::detail_title>{tr(TextKey::PlayoutDetail)}</h2>
                    <div class=style::divider/>

                    // Build-failure alert (shown instead of the table).
                    <div
                        class=style::build_alert
                        style:display=move || {
                            if build_message.with(|m| !m.trim().is_empty()) { "" } else { "none" }
                        }
                    >
                        {move || build_message.get()}
                    </div>

                    <div
                        style:display=move || {
                            if build_message.with(|m| m.trim().is_empty()) { "" } else { "none" }
                        }
                    >
                        <label class=style::switch_label>
                            <input
                                type="checkbox"
                                class=style::switch_input
                                prop:checked=move || show_filler.get()
                                on:change=move |e| {
                                    let value = event_target_checked(&e);
                                    show_filler.set(value);
                                    detail_page.set(0);
                                    save_filler_action.dispatch(value);
                                }
                            />
                            <span class=style::switch_track><span class=style::switch_thumb/></span>
                            <span>{tr(TextKey::ShowFiller)}</span>
                        </label>

                        <div class=style::table_wrap>
                            <table>
                                <thead>
                                    <tr>
                                        <th>{tr(TextKey::Start)}</th>
                                        <th>{tr(TextKey::Finish)}</th>
                                        <th>{tr(TextKey::MediaItem)}</th>
                                        <th>{tr(TextKey::Duration)}</th>
                                        <th class=style::col_detail_actions/>
                                    </tr>
                                </thead>
                                <tbody>
                                    {move || {
                                        let items = current_detail_rows.get();
                                        if items.is_empty() {
                                            return view! {
                                                <tr><td colspan="5" class=style::empty>
                                                    {tr(TextKey::NoItems)}
                                                </td></tr>
                                            }.into_any();
                                        }
                                        items.into_iter().map(|item| {
                                            detail_row(item, context_title, context_action)
                                        }).collect_view().into_any()
                                    }}
                                </tbody>
                            </table>
                        </div>

                        <PlayoutPager page=detail_page total=detail_total page_size=DETAIL_PAGE_SIZE/>
                    </div>
                </section>
            </section>

            // ---- dialogs ----
            <DeleteDialog
                open=delete_open
                entity_name=delete_name
                on_confirm=Callback::new(move |confirmed: bool| {
                    if confirmed {
                        if let Some(r) = delete_target.get_untracked() {
                            if selected_id.get_untracked() == Some(r.id) {
                                selected_id.set(None);
                            }
                            delete_action.dispatch(r.id);
                        }
                    } else {
                        delete_open.set(false);
                    }
                })
            />

            <Modal
                open=schedule_open
                title=tr(TextKey::SchedulePlayoutReset).to_string()
                confirm_label=Some(tr(TextKey::SaveChanges))
                on_close=Callback::new(move |confirmed: bool| {
                    if confirmed {
                        if let Some(r) = schedule_target.get_untracked() {
                            schedule_reset_action.dispatch((r.id, schedule_minutes.get_untracked()));
                        }
                    } else {
                        schedule_open.set(false);
                    }
                })
            >
                <p class=style::dialog_prompt>{move || schedule_prompt.get()}</p>
                <label class=style::field_label>{tr(TextKey::DailyResetTime)}</label>
                <select
                    class=style::select_input
                    prop:value=move || {
                        schedule_minutes.get().map_or_else(|| "none".to_owned(), |m| m.to_string())
                    }
                    on:change=move |e| {
                        let v = event_target_value(&e);
                        schedule_minutes.set(v.parse::<i32>().ok());
                    }
                >
                    <option value="none">{tr(TextKey::DoNotAutomaticallyReset)}</option>
                    {(1..48).map(|i| {
                        let minutes = i * 30;
                        let label = format!("{:02}:{:02}", minutes / 60, minutes % 60);
                        view! { <option value=minutes.to_string()>{label}</option> }
                    }).collect_view()}
                </select>
            </Modal>

            <Modal
                open=json_open
                title=tr(TextKey::EditExternalJsonFile).to_string()
                confirm_label=Some(tr(TextKey::SaveChanges))
                on_close=Callback::new(move |confirmed: bool| {
                    if confirmed {
                        if let Some(r) = json_target.get_untracked() {
                            let file = json_value.get_untracked();
                            if file.trim().is_empty() {
                                json_open.set(false);
                            } else {
                                json_action.dispatch((r.id, file));
                            }
                        }
                    } else {
                        json_open.set(false);
                    }
                })
            >
                <p class=style::dialog_prompt>{tr(TextKey::EditExternalJsonFilePrompt)}</p>
                <TextField label=tr(TextKey::ExternalJsonFileLabel) value=json_value/>
            </Modal>

            // Scheduling context dialog — custom so Copy does not close it.
            <div
                class=style::ctx_backdrop
                style:display=move || if context_open.get() { "" } else { "none" }
            />
            <div
                class=style::ctx_overlay
                style:display=move || if context_open.get() { "" } else { "none" }
                on:click=move |_| context_open.set(false)
            >
                <div class=style::ctx_panel on:click=|e| e.stop_propagation()>
                    <h2 class=style::ctx_title>{move || context_title.get()}</h2>
                    <pre class=style::ctx_pre>{move || context_body.get()}</pre>
                    <div class=style::ctx_actions>
                        <button
                            class=style::ctx_copy
                            on:click=move |_| copy_to_clipboard(&context_body.get_untracked())
                        >
                            {tr(TextKey::Copy)}
                        </button>
                        <button
                            class=style::ctx_close
                            on:click=move |_| context_open.set(false)
                        >
                            {tr(TextKey::Close)}
                        </button>
                    </div>
                </div>
            </div>
        </div>
    }
}

/// Renders a single playout list row with the ErsatzTV per-kind action set.
#[allow(clippy::too_many_arguments, clippy::needless_pass_by_value)]
fn playout_row(
    row: PlayoutListRow,
    selected_id: RwSignal<Option<i64>>,
    locked: RwSignal<Vec<i64>>,
    select_row: impl Fn(&PlayoutListRow) + Copy + 'static,
    reset_action: Action<
        (i64, i32),
        Result<crate::admin::server_fns::playouts::PlayoutBuildEnqueueResult, ServerFnError>,
    >,
    delete_target: RwSignal<Option<PlayoutListRow>>,
    delete_open: RwSignal<bool>,
    schedule_target: RwSignal<Option<PlayoutListRow>>,
    schedule_minutes: RwSignal<Option<i32>>,
    schedule_open: RwSignal<bool>,
    json_target: RwSignal<Option<PlayoutListRow>>,
    json_value: RwSignal<String>,
    json_open: RwSignal<bool>,
) -> impl IntoView {
    let id = row.id;
    let kind = row.schedule_kind;
    let channel = if row.channel_number.is_empty() {
        row.channel_name.clone()
    } else {
        format!("{} \u{2014} {}", row.channel_number, row.channel_name)
    };
    let kind_label = playout_schedule_kind_label(kind);
    let (schedule_display, schedule_title) =
        schedule_display_name(&row.schedule_name, &row.schedule_file);
    let build_failed = row.build_success == Some(false);

    let is_locked = move || locked.with(|ids| ids.contains(&id));
    let row_for_select = row.clone();
    let row_for_delete = row.clone();
    let row_for_schedule = row.clone();
    let row_for_json = row.clone();

    let row_class = move || {
        let selected = selected_id.get() == Some(id);
        match (selected, build_failed) {
            (true, true) => style::row_failure_selected.to_string(),
            (true, false) => style::row_selected.to_string(),
            (false, true) => style::row_failure.to_string(),
            (false, false) => String::new(),
        }
    };

    // Per-kind action buttons (Edit / Refresh|Reset / Schedule Reset).
    let actions = match kind {
        KIND_CLASSIC => view! {
            <>
                <a
                    class=move || disabled_icon_class(is_locked())
                    href=format!("/playouts/classic/{id}")
                    title=tr(TextKey::EditPlayout)
                    aria-label=tr(TextKey::EditPlayout)
                    aria-disabled=move || is_locked().to_string()
                >
                    <span style=icon_style(IconName::Edit, "1.1rem") aria-hidden="true"/>
                </a>
                <button
                    class=style::icon_btn
                    disabled=is_locked
                    title=tr(TextKey::RefreshPlayout)
                    aria-label=tr(TextKey::RefreshPlayout)
                    on:click=move |_| { reset_action.dispatch((id, kind)); }
                >
                    <span style=icon_style(IconName::Refresh, "1.1rem") aria-hidden="true"/>
                </button>
                <button
                    class=style::icon_btn
                    disabled=is_locked
                    title=tr(TextKey::ScheduleReset)
                    aria-label=tr(TextKey::ScheduleReset)
                    on:click=move |_| {
                        schedule_minutes.set(row_for_schedule.daily_rebuild_minutes);
                        schedule_target.set(Some(row_for_schedule.clone()));
                        schedule_open.set(true);
                    }
                >
                    <span style=icon_style(IconName::Tune, "1.1rem") aria-hidden="true"/>
                </button>
            </>
        }
        .into_any(),
        KIND_EXTERNAL_JSON => view! {
            <>
                <button
                    class=style::icon_btn
                    disabled=is_locked
                    title=tr(TextKey::EditExternalJsonFile)
                    aria-label=tr(TextKey::EditExternalJsonFile)
                    on:click=move |_| {
                        json_value.set(row_for_json.schedule_file.clone());
                        json_target.set(Some(row_for_json.clone()));
                        json_open.set(true);
                    }
                >
                    <span style=icon_style(IconName::Edit, "1.1rem") aria-hidden="true"/>
                </button>
                <span class=style::icon_slot/>
                <span class=style::icon_slot/>
            </>
        }
        .into_any(),
        KIND_SEQUENTIAL | KIND_SCRIPTED => {
            let edit_href = if kind == KIND_SEQUENTIAL {
                format!("/playouts/sequential/{id}")
            } else {
                format!("/playouts/scripted/{id}")
            };
            view! {
                <>
                    <a
                        class=move || disabled_icon_class(is_locked())
                        href=edit_href
                        title=tr(TextKey::EditPlayout)
                        aria-label=tr(TextKey::EditPlayout)
                        aria-disabled=move || is_locked().to_string()
                    >
                        <span style=icon_style(IconName::Edit, "1.1rem") aria-hidden="true"/>
                    </a>
                    <button
                        class=style::icon_btn
                        disabled=is_locked
                        title=tr(TextKey::ResetPlayout)
                        aria-label=tr(TextKey::ResetPlayout)
                        on:click=move |_| { reset_action.dispatch((id, kind)); }
                    >
                        <span style=icon_style(IconName::Refresh, "1.1rem") aria-hidden="true"/>
                    </button>
                    <span class=style::icon_slot/>
                </>
            }
            .into_any()
        }
        KIND_BLOCK => view! {
            <>
                <a
                    class=move || disabled_icon_class(is_locked())
                    href=format!("/playouts/block/{id}")
                    title=tr(TextKey::EditPlayout)
                    aria-label=tr(TextKey::EditPlayout)
                    aria-disabled=move || is_locked().to_string()
                >
                    <span style=icon_style(IconName::Edit, "1.1rem") aria-hidden="true"/>
                </a>
                <button
                    class=style::icon_btn
                    disabled=is_locked
                    title=tr(TextKey::RefreshPlayout)
                    aria-label=tr(TextKey::RefreshPlayout)
                    on:click=move |_| { reset_action.dispatch((id, kind)); }
                >
                    <span style=icon_style(IconName::Refresh, "1.1rem") aria-hidden="true"/>
                </button>
                <span class=style::icon_slot/>
            </>
        }
        .into_any(),
        _ => view! { <><span class=style::icon_slot/><span class=style::icon_slot/><span class=style::icon_slot/></> }.into_any(),
    };

    view! {
        <tr
            class=row_class
            on:click=move |_| select_row(&row_for_select)
        >
            <td>{channel}</td>
            <td class=style::col_md title=schedule_title>{schedule_display}</td>
            <td class=move || format!("{} {}", style::col_md, style::cell_muted)>{kind_label}</td>
            <td on:click=|e| e.stop_propagation()>
                <div class=style::row_actions>
                    <span
                        class=style::spinner
                        style:display=move || if is_locked() { "" } else { "none" }
                        aria-hidden="true"
                    />
                    {actions}
                    <button
                        class=style::icon_btn
                        disabled=is_locked
                        title=tr(TextKey::DeletePlayout)
                        aria-label=tr(TextKey::DeletePlayout)
                        on:click=move |_| {
                            delete_target.set(Some(row_for_delete.clone()));
                            delete_open.set(true);
                        }
                    >
                        <span style=icon_style(IconName::Delete, "1.1rem") aria-hidden="true"/>
                    </button>
                </div>
            </td>
        </tr>
    }
}

/// Renders a single playout-detail row (item or gap).
#[allow(clippy::needless_pass_by_value)]
fn detail_row(
    item: PlayoutDetailItem,
    context_title: RwSignal<String>,
    context_action: Action<String, Result<String, ServerFnError>>,
) -> impl IntoView {
    let is_gap = item.filler_kind < 0;
    let title = if is_gap {
        tr(TextKey::UnscheduledPlayoutItem).to_owned()
    } else if item.title.trim().is_empty() {
        tr(TextKey::UntitledItem).to_owned()
    } else {
        item.title.clone()
    };
    let row_class = filler_row_class(item.filler_kind);
    let ctx = item.scheduling_context.clone();
    let title_for_dialog = title.clone();

    view! {
        <tr class=row_class>
            <td>{item.start.clone()}</td>
            <td>{item.finish.clone()}</td>
            <td>{title}</td>
            <td>{item.duration.clone()}</td>
            <td>
                {ctx.filter(|c| !c.trim().is_empty()).map(|c| {
                    let title_for_dialog = title_for_dialog.clone();
                    view! {
                        <button
                            class=style::icon_btn
                            title=tr(TextKey::Troubleshoot)
                            aria-label=tr(TextKey::Troubleshoot)
                            on:click=move |_| {
                                context_title.set(title_for_dialog.clone());
                                context_action.dispatch(c.clone());
                            }
                        >
                            <span style=icon_style(IconName::Info, "1.1rem") aria-hidden="true"/>
                        </button>
                    }
                })}
            </td>
        </tr>
    }
}

/// Prev/next pager footer with a range label. `total` is the raw
/// resource-backed count; it is read inside `<Suspense>` so the resource read
/// stays inside a tracked boundary (mirrors `PagedTable`).
#[component]
fn PlayoutPager(page: RwSignal<u32>, total: Signal<Option<u64>>, page_size: u32) -> impl IntoView {
    use crate::admin::i18n::{item_count, range_of, rows_per_page};

    let go_prev = move |_| page.update(|p| *p = p.saturating_sub(1));
    let go_next = move |_| {
        let total_items = total.get().unwrap_or(0);
        let max = total_items.saturating_sub(1) / u64::from(page_size.max(1));
        page.update(|p| {
            if u64::from(*p) < max {
                *p += 1;
            }
        });
    };

    view! {
        <Suspense fallback=move || {
            view! {
                <div class=style::pagination>
                    <span>{move || rows_per_page(page_size)}</span>
                    <div class=style::page_btns>
                        <button class=style::page_btn disabled=true>"‹"</button>
                        <button class=style::page_btn disabled=true>"›"</button>
                    </div>
                </div>
            }
        }>
            {move || {
                total
                    .get()
                    .map(|total_items| {
                        view! {
                            <div class=style::pagination>
                                <span>{move || rows_per_page(page_size)}</span>
                                <span>
                                    {move || {
                                        let p = page.get();
                                        let start = p * page_size + 1;
                                        let end = ((p + 1) * page_size)
                                            .min(u32::try_from(total_items).unwrap_or(u32::MAX));
                                        if total_items == 0 {
                                            item_count(0)
                                        } else {
                                            range_of(start, end, total_items)
                                        }
                                    }}
                                </span>
                                <div class=style::page_btns>
                                    <button
                                        class=style::page_btn
                                        disabled=move || page.get() == 0
                                        on:click=go_prev
                                        title=tr(TextKey::PreviousPage)
                                    >"‹"</button>
                                    <button
                                        class=style::page_btn
                                        disabled=move || {
                                            total_items == 0
                                                || u64::from((page.get() + 1) * page_size) >= total_items
                                        }
                                        on:click=go_next
                                        title=tr(TextKey::NextPage)
                                    >"›"</button>
                                </div>
                            </div>
                        }
                    })
            }}
        </Suspense>
    }
}

fn filler_row_class(filler_kind: i32) -> &'static str {
    match filler_kind {
        -1 => style::filler_unscheduled,
        0 => "",
        _ => style::filler_filler,
    }
}

#[allow(clippy::needless_pass_by_value)]
fn playout_entity_name(row: PlayoutListRow) -> String {
    let channel = if row.channel_number.is_empty() {
        row.channel_name.clone()
    } else {
        format!("{} - {}", row.channel_number, row.channel_name)
    };
    if row.schedule_name.trim().is_empty() {
        channel
    } else {
        format!("{} on {}", row.schedule_name, channel)
    }
}

fn disabled_icon_class(disabled: bool) -> String {
    if disabled {
        format!("{} {}", style::icon_btn, style::icon_btn_disabled)
    } else {
        style::icon_btn.to_string()
    }
}

fn schedule_display_name(schedule_name: &str, schedule_file: &str) -> (String, String) {
    if !schedule_name.trim().is_empty() {
        return (schedule_name.to_owned(), String::new());
    }

    if schedule_file.trim().is_empty() {
        return (String::new(), String::new());
    }

    let display = schedule_file
        .rsplit(['/', '\\'])
        .find(|part| !part.is_empty())
        .unwrap_or(schedule_file)
        .to_owned();
    (display, schedule_file.to_owned())
}

/// Best-effort one-click clipboard copy (no-op on the server / unsupported
/// browsers). Mirrors `SchedulingContextDialog`'s Copy button.
#[allow(unused_variables)]
fn copy_to_clipboard(text: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(window) = web_sys::window() {
            let _ = window.navigator().clipboard().write_text(text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::schedule_display_name;

    #[test]
    fn schedule_display_uses_filename_when_schedule_name_is_empty() {
        assert_eq!(
            schedule_display_name("", "/var/lib/ersatztv/schedules/lineup.yaml"),
            (
                "lineup.yaml".to_owned(),
                "/var/lib/ersatztv/schedules/lineup.yaml".to_owned()
            )
        );
        assert_eq!(
            schedule_display_name("", r"C:\Schedules\external.json"),
            (
                "external.json".to_owned(),
                r"C:\Schedules\external.json".to_owned()
            )
        );
        assert_eq!(
            schedule_display_name("Prime", "/ignored.yaml"),
            ("Prime".to_owned(), String::new())
        );
    }
}
