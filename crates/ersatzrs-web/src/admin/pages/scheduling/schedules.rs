#![allow(clippy::too_many_lines)]

use std::fmt::Write as _;

use leptos::prelude::*;
use leptos_router::hooks::{use_params_map, use_query_map};

use crate::admin::i18n::{
    media_item_fallback_label, range_of, schedule_error_message, schedule_item_fallback_label, tr,
    TextKey,
};
use crate::admin::server_fns::schedules::{
    copy_schedule, delete_schedule, get_schedule_form, get_schedule_items_page,
    get_schedules_page_size, list_schedules, replace_schedule_items, save_schedule,
    save_schedules_detail_page_size, FillerOption, PlaylistOption, ScheduleForm, ScheduleItemForm,
    ScheduleItemListRow, ScheduleItemOption, ScheduleItemsEditorOptions, ScheduleItemsPageData,
    ScheduleLanguageOption, ScheduleListRow, COLLECTION_TYPE_ARTIST, COLLECTION_TYPE_COLLECTION,
    COLLECTION_TYPE_MULTI_COLLECTION, COLLECTION_TYPE_PLAYLIST, COLLECTION_TYPE_RERUN_FIRST_RUN,
    COLLECTION_TYPE_RERUN_RERUN, COLLECTION_TYPE_SEARCH_QUERY, COLLECTION_TYPE_SMART_COLLECTION,
    COLLECTION_TYPE_TELEVISION_SEASON, COLLECTION_TYPE_TELEVISION_SHOW, FILL_WITH_GROUP_MODE_NONE,
    MARATHON_GROUP_BY_NONE, MULTIPLE_MODE_COLLECTION_SIZE, MULTIPLE_MODE_COUNT,
    MULTIPLE_MODE_MULTI_EPISODE_GROUP_SIZE, MULTIPLE_MODE_PLAYLIST_ITEM_SIZE,
    PLAYBACK_ORDER_CHRONOLOGICAL, PLAYBACK_ORDER_MARATHON, PLAYBACK_ORDER_NONE,
    PLAYBACK_ORDER_RANDOM, PLAYBACK_ORDER_SEASON_EPISODE, PLAYBACK_ORDER_SHUFFLE,
    PLAYBACK_ORDER_SHUFFLE_IN_ORDER, PLAYOUT_MODE_DURATION, PLAYOUT_MODE_FLOOD,
    PLAYOUT_MODE_MULTIPLE, PLAYOUT_MODE_ONE, START_TYPE_DYNAMIC, START_TYPE_FIXED,
};
use crate::admin::ui::primitives::dialog::{DeleteDialog, Modal};
use crate::admin::ui::primitives::icon::{icon_style, IconName};

stylance::import_crate_style!(style, "src/admin/pages/scheduling/schedules.module.css");

#[component]
pub fn SchedulesPage() -> impl IntoView {
    let query = use_query_map();
    let error = query.with_untracked(|params| params.get("error").unwrap_or_default());
    view! { <SchedulesIsland initial_error=error/> }
}

#[island]
fn SchedulesIsland(initial_error: String) -> impl IntoView {
    let page = RwSignal::new(0u32);
    let page_size = RwSignal::new(10u32);
    let detail_page = RwSignal::new(0u32);
    let detail_page_size = RwSignal::new(10u32);
    let query = RwSignal::new(String::new());
    let selected = RwSignal::new(None::<ScheduleListRow>);
    let delete_open = RwSignal::new(false);
    let delete_target = RwSignal::new(None::<ScheduleListRow>);
    let copy_open = RwSignal::new(false);
    let copy_target = RwSignal::new(None::<ScheduleListRow>);
    let copy_error = RwSignal::new(String::new());

    let saved_page_sizes = Resource::new_blocking(
        || (),
        |()| async { get_schedules_page_size().await.unwrap_or((10, 10)) },
    );
    Effect::new(move |_| {
        if let Some((saved, detail_saved)) = saved_page_sizes.get() {
            page_size.set(saved);
            detail_page_size.set(detail_saved);
        }
    });

    let data: Resource<(Vec<ScheduleListRow>, u64)> = Resource::new_blocking(
        move || (query.get(), page.get(), page_size.get()),
        move |(query, page, page_size)| async move {
            list_schedules(query, page, page_size)
                .await
                .unwrap_or_default()
        },
    );
    let detail_page_size_action =
        Action::new(|page_size: &u32| save_schedules_detail_page_size(*page_size));

    let detail_data: Resource<Vec<ScheduleItemForm>> = Resource::new_blocking(
        move || selected.get().map(|row| row.id),
        move |id| async move {
            if let Some(id) = id {
                get_schedule_items_page(id)
                    .await
                    .ok()
                    .flatten()
                    .map(|data| data.items)
                    .unwrap_or_default()
            } else {
                Vec::new()
            }
        },
    );

    let delete_action = Action::new(|id: &i64| delete_schedule(*id));
    Effect::new(move |_| {
        if delete_action.value().get().is_some() {
            selected.set(None);
            data.refetch();
        }
    });

    let copy_name = RwSignal::new(String::new());
    let copy_action = Action::new(|(id, name): &(i64, String)| copy_schedule(*id, name.clone()));
    Effect::new(move |_| {
        if let Some(result) = copy_action.value().get() {
            match result {
                Ok(id) => {
                    #[cfg(target_arch = "wasm32")]
                    {
                        let _ = window()
                            .location()
                            .set_href(&format!("/schedules/{id}/items"));
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    let _ = id;
                }
                Err(error) => {
                    copy_error.set(schedule_error_message(error));
                    copy_open.set(true);
                }
            }
        }
    });

    view! {
        <div class=style::page>
            <div class=style::action_bar>
                <a href="/schedules/add" class=style::btn_primary>
                    <span style=icon_style(IconName::Add, "1.125rem") aria-hidden="true"></span>
                    <span>{tr(TextKey::AddSchedule)}</span>
                </a>
            </div>
            <section class=style::content>
                <h1 class=style::page_title>{tr(TextKey::Schedules)}</h1>
                <div class=style::divider/>
                <p class=style::error style:display=move || if initial_error.is_empty() { "none" } else { "" }>
                    {tr(TextKey::ScheduleActionFailed)}
                </p>
                <label class=style::search>
                    <span style=icon_style(IconName::FilterList, "1.125rem") aria-hidden="true"></span>
                    <input
                        type="search"
                        placeholder=tr(TextKey::SearchForSchedules)
                        prop:value=move || query.get()
                        on:input=move |event| {
                            query.set(event_target_value(&event));
                            page.set(0);
                            selected.set(None);
                        }
                    />
                </label>

                <Suspense fallback=move || view! { <p class=style::empty>{tr(TextKey::Loading)}</p> }>
                    {move || {
                        data.get().map(|(rows, total)| {
                            let size = page_size.get().max(1);
                            let total_pages = u32::try_from(total.div_ceil(u64::from(size)))
                                .unwrap_or(u32::MAX)
                                .max(1);
                            let current_page = page.get().min(total_pages.saturating_sub(1));
                            let range_start = if total == 0 {
                                0
                            } else {
                                u64::from(current_page) * u64::from(size) + 1
                            };
                            let range_end = ((u64::from(current_page) + 1) * u64::from(size)).min(total);

                            view! {
                                <div class=style::table_wrap>
                                    <table>
                                        <colgroup>
                                            <col/>
                                            <col style="width: 300px"/>
                                        </colgroup>
                                        <tbody>
                                            {if rows.is_empty() {
                                                view! { <tr><td colspan="2" class=style::empty>{tr(TextKey::NoSchedules)}</td></tr> }.into_any()
                                            } else {
                                                rows.into_iter().map(|row| {
                                                    let id = row.id;
                                                    let name = row.name.clone();
                                                    let hidden_name = name.clone();
                                                    let row_for_select = row.clone();
                                                    let row_for_copy = row.clone();
                                                    let row_for_delete = row.clone();
                                                    view! {
                                                        <tr on:click=move |_| {
                                                            selected.set(Some(row_for_select.clone()));
                                                            detail_page.set(0);
                                                        }>
                                                            <td>{name.clone()}</td>
                                                            <td>
                                                                <div class=style::actions>
                                                                    <a class=style::icon_button href=format!("/schedules/{id}") title=tr(TextKey::EditProperties) aria-label=tr(TextKey::EditProperties)>
                                                                        <span style=icon_style(IconName::Edit, "1.25rem") aria-hidden="true"></span>
                                                                    </a>
                                                                    <a class=style::icon_button href=format!("/schedules/{id}/items") title=tr(TextKey::EditScheduleItems) aria-label=tr(TextKey::EditScheduleItems)>
                                                                        <span style=icon_style(IconName::Menu, "1.25rem") aria-hidden="true"></span>
                                                                    </a>
                                                                    <form method="post" action="/api/schedules/copy">
                                                                        <input type="hidden" name="id" value=id.to_string()/>
                                                                        <input type="hidden" name="name" value=format!("{} {}", hidden_name, tr(TextKey::CopyNameSuffix))/>
                                                                        <button
                                                                            type="submit"
                                                                            class=style::icon_button
                                                                            title=tr(TextKey::CopySchedule)
                                                                            aria-label=tr(TextKey::CopySchedule)
                                                                            on:click=move |event| {
                                                                                event.prevent_default();
                                                                                copy_target.set(Some(row_for_copy.clone()));
                                                                                copy_name.set(String::new());
                                                                                copy_error.set(String::new());
                                                                                copy_open.set(true);
                                                                            }
                                                                        >
                                                                            <span style=icon_style(IconName::Copy, "1.25rem") aria-hidden="true"></span>
                                                                        </button>
                                                                    </form>
                                                                    <a class=style::icon_button href=format!("/system/troubleshooting?schedule={id}") title=tr(TextKey::Troubleshooting) aria-label=tr(TextKey::Troubleshooting)>
                                                                        <span style=icon_style(IconName::Info, "1.25rem") aria-hidden="true"></span>
                                                                    </a>
                                                                    <form method="post" action="/api/schedules/delete">
                                                                        <input type="hidden" name="id" value=id.to_string()/>
                                                                        <button
                                                                            type="submit"
                                                                            class=style::icon_button
                                                                            title=tr(TextKey::DeleteSchedule)
                                                                            aria-label=tr(TextKey::DeleteSchedule)
                                                                            on:click=move |event| {
                                                                                event.prevent_default();
                                                                                delete_target.set(Some(row_for_delete.clone()));
                                                                                delete_open.set(true);
                                                                            }
                                                                        >
                                                                            <span style=icon_style(IconName::Delete, "1.25rem") aria-hidden="true"></span>
                                                                        </button>
                                                                    </form>
                                                                </div>
                                                            </td>
                                                        </tr>
                                                    }
                                                }).collect_view().into_any()
                                            }}
                                        </tbody>
                                    </table>
                                </div>
                                <div class=style::pager>
                                    <span>{tr(TextKey::RowsPerPage)}</span>
                                    <select
                                        class=style::page_size
                                        aria-label=tr(TextKey::RowsPerPage)
                                        prop:value=move || page_size.get().to_string()
                                        on:change=move |event| {
                                            if let Ok(value) = event_target_value(&event).parse::<u32>() {
                                                page_size.set(value.max(1));
                                                page.set(0);
                                            }
                                        }
                                    >
                                        <option value="10">"10"</option>
                                        <option value="25">"25"</option>
                                        <option value="50">"50"</option>
                                        <option value="100">"100"</option>
                                    </select>
                                    <span>{range_of(u32::try_from(range_start).unwrap_or(u32::MAX), u32::try_from(range_end).unwrap_or(u32::MAX), total)}</span>
                                    <button
                                        class=style::icon_button
                                        disabled=move || { page.get() == 0 }
                                        on:click=move |_| {
                                            page.update(|value| *value = value.saturating_sub(1));
                                        }
                                        title=tr(TextKey::PreviousPage)
                                    >
                                        <span style=icon_style(IconName::ChevronLeft, "1.25rem") aria-hidden="true"></span>
                                    </button>
                                    <button
                                        class=style::icon_button
                                        disabled=move || { page.get() + 1 >= total_pages }
                                        on:click=move |_| {
                                            page.update(|value| *value = (*value + 1).min(total_pages.saturating_sub(1)));
                                        }
                                        title=tr(TextKey::NextPage)
                                    >
                                        <span style=icon_style(IconName::ChevronRight, "1.25rem") aria-hidden="true"></span>
                                    </button>
                                </div>
                            }
                        })
                    }}
                </Suspense>

                <div class=style::detail style:display=move || if selected.get().is_some() { "" } else { "none" }>
                    <h2 class=style::page_title>
                        {move || selected.get().map(|row| format!("{} {}", row.name, tr(TextKey::Items))).unwrap_or_default()}
                    </h2>
                    <Suspense fallback=move || view! { <p class=style::empty>{tr(TextKey::Loading)}</p> }>
                        {move || {
                            detail_data.get().map(|items| {
                                let total = u64::try_from(items.len()).unwrap_or(u64::MAX);
                                let size = detail_page_size.get().max(1);
                                let total_pages = u32::try_from(total.div_ceil(u64::from(size)))
                                    .unwrap_or(u32::MAX)
                                    .max(1);
                                let current_page = detail_page.get().min(total_pages.saturating_sub(1));
                                let range_start = if total == 0 {
                                    0
                                } else {
                                    u64::from(current_page) * u64::from(size) + 1
                                };
                                let range_end = ((u64::from(current_page) + 1) * u64::from(size)).min(total);
                                let skip_count = usize::try_from(u64::from(current_page) * u64::from(size))
                                    .unwrap_or(usize::MAX);
                                let take_count = usize::try_from(size).unwrap_or(usize::MAX);
                                let page_items = items
                                    .into_iter()
                                    .skip(skip_count)
                                    .take(take_count)
                                    .collect::<Vec<_>>();

                                view! {
                                    <div class=style::table_wrap>
                                        <table>
                                            <thead>
                                                <tr>
                                                    <th>{tr(TextKey::StartTime)}</th>
                                                    <th>{tr(TextKey::Collection)}</th>
                                                    <th>{tr(TextKey::PlayoutMode)}</th>
                                                </tr>
                                            </thead>
                                            <tbody>
                                                {if page_items.is_empty() {
                                                    view! { <tr><td colspan="3" class=style::empty>{tr(TextKey::NoScheduleItems)}</td></tr> }.into_any()
                                                } else {
                                                    page_items.into_iter().map(|item| {
                                                        view! {
                                                            <tr>
                                                                <td>{start_label(&list_row_from_item(&item))}</td>
                                                                <td>{collection_name_display(&item)}</td>
                                                                <td>{playout_label(&list_row_from_item(&item))}</td>
                                                            </tr>
                                                        }
                                                    }).collect_view().into_any()
                                                }}
                                            </tbody>
                                        </table>
                                    </div>
                                    <div class=style::pager>
                                        <span>{tr(TextKey::RowsPerPage)}</span>
                                        <select
                                            class=style::page_size
                                            aria-label=tr(TextKey::DetailRowsPerPage)
                                            prop:value=move || detail_page_size.get().to_string()
                                            on:change=move |event| {
                                                if let Ok(value) = event_target_value(&event).parse::<u32>() {
                                                    let value = value.max(1);
                                                    detail_page_size.set(value);
                                                    detail_page.set(0);
                                                    detail_page_size_action.dispatch(value);
                                                }
                                            }
                                        >
                                            <option value="10">"10"</option>
                                            <option value="25">"25"</option>
                                            <option value="50">"50"</option>
                                            <option value="100">"100"</option>
                                        </select>
                                        <span>{range_of(u32::try_from(range_start).unwrap_or(u32::MAX), u32::try_from(range_end).unwrap_or(u32::MAX), total)}</span>
                                        <button
                                            class=style::icon_button
                                            disabled=move || { detail_page.get() == 0 }
                                            on:click=move |_| {
                                                detail_page.update(|value| *value = value.saturating_sub(1));
                                            }
                                            title=tr(TextKey::PreviousPage)
                                        >
                                            <span style=icon_style(IconName::ChevronLeft, "1.25rem") aria-hidden="true"></span>
                                        </button>
                                        <button
                                            class=style::icon_button
                                            disabled=move || { detail_page.get() + 1 >= total_pages }
                                            on:click=move |_| {
                                                detail_page.update(|value| *value = (*value + 1).min(total_pages.saturating_sub(1)));
                                            }
                                            title=tr(TextKey::NextPage)
                                        >
                                            <span style=icon_style(IconName::ChevronRight, "1.25rem") aria-hidden="true"></span>
                                        </button>
                                    </div>
                                }
                            })
                        }}
                    </Suspense>
                </div>
            </section>
            <Modal
                open=copy_open
                title=tr(TextKey::CopySchedule).to_string()
                on_close=Callback::new(move |confirmed| {
                    if confirmed {
                        let name = copy_name.get_untracked();
                        if name.trim().is_empty() {
                            copy_error.set(tr(TextKey::NameRequired).to_string());
                            copy_open.set(true);
                        } else if let Some(target) = copy_target.get_untracked() {
                            copy_error.set(String::new());
                            copy_action.dispatch((target.id, name));
                        }
                    } else {
                        copy_error.set(String::new());
                    }
                })
                confirm_label=Some(tr(TextKey::CopySchedule))
            >
                <div class=style::dialog_body>
                    <p>{tr(TextKey::EnterNameForNewSchedule)}</p>
                    <label class=style::dialog_field>
                        <span>{tr(TextKey::NewScheduleName)}</span>
                        <input
                            class=style::input
                            prop:value=move || copy_name.get()
                            on:input=move |event| {
                                copy_name.set(event_target_value(&event));
                                copy_error.set(String::new());
                            }
                        />
                    </label>
                    <p class=style::error style:display=move || if copy_error.get().is_empty() { "none" } else { "" }>
                        {move || copy_error.get()}
                    </p>
                </div>
            </Modal>
            <DeleteDialog
                open=delete_open
                entity_name=Signal::derive(move || {
                    delete_target.get().map_or_else(
                        || tr(TextKey::Schedule).to_string(),
                        |row| format!("{} {}", tr(TextKey::Schedule), row.name),
                    )
                })
                on_confirm=Callback::new(move |confirmed| {
                    if confirmed {
                        if let Some(target) = delete_target.get_untracked() {
                            delete_action.dispatch(target.id);
                        }
                    }
                })
            />
        </div>
    }
}

#[component]
pub fn ScheduleEditorPage() -> impl IntoView {
    let params = use_params_map();
    let query = use_query_map();
    let id = params.with(|p| p.get("id").and_then(|s| s.parse::<i64>().ok()).unwrap_or(0));
    let error = query.with_untracked(|params| params.get("error").unwrap_or_default());
    view! { <ScheduleEditorIsland id initial_error=error/> }
}

#[island]
fn ScheduleEditorIsland(id: i64, initial_error: String) -> impl IntoView {
    let form = RwSignal::new(ScheduleForm::default());
    let save_error = RwSignal::new(if initial_error.is_empty() {
        String::new()
    } else {
        tr(TextKey::ScheduleSaveFailed).to_owned()
    });
    let data = Resource::new_blocking(
        move || id,
        |id| async move { get_schedule_form(id).await.ok().flatten() },
    );
    Effect::new(move |_| {
        if let Some(Some(loaded)) = data.get() {
            form.set(loaded);
        }
    });

    let save_action = Action::new(|form: &ScheduleForm| save_schedule(form.clone()));
    Effect::new(move |_| {
        if let Some(result) = save_action.value().get() {
            match result {
                Ok(saved_id) => {
                    save_error.set(String::new());
                    #[cfg(target_arch = "wasm32")]
                    {
                        let target = if id == 0 {
                            format!("/schedules/{saved_id}/items")
                        } else {
                            "/schedules".to_string()
                        };
                        let _ = window().location().set_href(&target);
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    let _ = saved_id;
                }
                Err(error) => save_error.set(schedule_error_message(error)),
            }
        }
    });

    view! {
        <div class=style::page>
            <div class=style::action_bar>
                <button type="submit" form="schedule-editor-form" class=style::btn_primary>
                    <span style=icon_style(if id == 0 { IconName::Add } else { IconName::Save }, "1.125rem") aria-hidden="true"></span>
                    <span>{if id == 0 { tr(TextKey::AddSchedule) } else { tr(TextKey::SaveSchedule) }}</span>
                </button>
            </div>
            <section class=style::editor>
                <h1 class=style::page_title>{tr(TextKey::Schedule)}</h1>
                <div class=style::divider/>
                <p class=style::error style:display=move || if save_error.get().is_empty() { "none" } else { "" }>
                    {move || save_error.get()}
                </p>
                <form
                    id="schedule-editor-form"
                    class=style::editor_form
                    method="post"
                    action="/api/schedules/save"
                    on:submit=move |event| {
                        event.prevent_default();
                        save_action.dispatch(form.get_untracked());
                    }
                >
                    <input type="hidden" name="id" value=move || form.get().id.to_string()/>
                    <label class=style::field>
                        <span>{tr(TextKey::Name)}</span>
                        <input class=style::input name="name" required value=move || form.get().name on:input=move |event| form.update(|form| form.name = event_target_value(&event))/>
                    </label>
                    {checkbox_field(tr(TextKey::KeepMultiPartEpisodesTogether), tr(TextKey::GroupMultiPartEpisodesHelp), "keep_multi_part_episodes_together", form, |form| form.keep_multi_part_episodes_together, |form, value| form.keep_multi_part_episodes_together = value, false)}
                    {checkbox_field(tr(TextKey::TreatCollectionsAsShows), tr(TextKey::TreatCollectionsAsShowsHelp), "treat_collections_as_shows", form, |form| form.treat_collections_as_shows, |form, value| form.treat_collections_as_shows = value, false)}
                    {checkbox_field(tr(TextKey::ShuffleScheduleItems), tr(TextKey::ShuffleScheduleItemsHelp), "shuffle_schedule_items", form, |form| form.shuffle_schedule_items, |form, value| form.shuffle_schedule_items = value, false)}
                    {checkbox_field(tr(TextKey::RandomStartPoint), "", "random_start_point", form, |form| form.random_start_point, |form, value| form.random_start_point = value, false)}
                    <label class=style::field>
                        <span>{tr(TextKey::FixedStartTimeBehavior)}</span>
                        <select
                            class=style::select
                            name="fixed_start_time_behavior"
                            prop:value=move || form.get().fixed_start_time_behavior.to_string()
                            on:change=move |event| {
                                if let Ok(value) = event_target_value(&event).parse::<i32>() {
                                    form.update(|form| form.fixed_start_time_behavior = value);
                                }
                            }
                        >
                            <option value="0">{tr(TextKey::StrictWaitExactStartTime)}</option>
                            <option value="1">{tr(TextKey::FlexibleStartSoonAsPossible)}</option>
                        </select>
                    </label>
                </form>
            </section>
        </div>
    }
}

#[component]
pub fn ScheduleItemsPage() -> impl IntoView {
    let params = use_params_map();
    let query = use_query_map();
    let id = params.with(|p| p.get("id").and_then(|s| s.parse::<i64>().ok()).unwrap_or(0));
    let initial_error = query.with_untracked(|params| params.get("error").unwrap_or_default());
    let fallback_data = Resource::new_blocking(
        move || id,
        |id| async move { get_schedule_items_page(id).await.ok().flatten() },
    );
    view! {
        <Suspense fallback=move || view! { <noscript><p class=style::empty>{tr(TextKey::Loading)}</p></noscript> }>
            {move || fallback_data.get().map(|data| view! {
                <noscript inner_html=schedule_items_fallback_html(id, data, &initial_error)></noscript>
            })}
        </Suspense>
        <ScheduleItemsIsland id/>
    }
}

fn schedule_items_fallback_html(
    id: i64,
    data: Option<ScheduleItemsPageData>,
    initial_error: &str,
) -> String {
    let Some(data) = data else {
        return format!(
            r#"<p class="{}">{}</p>"#,
            style::empty,
            tr(TextKey::ScheduleNotFound)
        );
    };
    let items = normalized_items(data.items);
    let mut rows = String::new();
    if items.is_empty() {
        let _ = write!(
            rows,
            r#"<tr><td colspan="5" class="{}">{}</td></tr>"#,
            style::empty,
            tr(TextKey::NoScheduleItems)
        );
    } else {
        for (index, item) in items.iter().enumerate() {
            rows.push_str(&schedule_item_fallback_row_html(index, item, &data.options));
        }
    }

    format!(
        r#"<div class="{page}"><div class="{action_bar}"><div class="{action_row}"><div class="{grow}">{name}</div><button type="submit" form="schedule-items-fallback-form" class="{btn_primary}" name="action" value="save">{save_schedule_items}</button><button type="submit" form="schedule-items-fallback-form" class="{btn_default}" name="action" value="add">{add_schedule_item}</button><a href="/schedules" class="{btn_default}">{back}</a></div></div><section class="{content}"><h1 class="{page_title}">{schedule_items}</h1><div class="{divider}"></div><p class="{error}" style="display:{error_display}">{action_failed}</p><form id="schedule-items-fallback-form" method="post" action="/api/schedules/items/save" class="{fallback_form}"><input type="hidden" name="schedule_id" value="{id}"><div class="{table_wrap}"><table><thead><tr><th>{start}</th><th>{collection_label}</th><th>{playback}</th><th>{playout}</th><th></th></tr></thead><tbody>{rows}</tbody></table></div><h2 class="{section_title}">{add_schedule_item}</h2><div class="{editor_form}">{add_fields}</div></form></section></div>"#,
        page = style::page,
        action_bar = style::action_bar,
        action_row = style::action_row,
        grow = style::grow,
        name = escape_html(&data.schedule.name),
        btn_primary = style::btn_primary,
        btn_default = style::btn_default,
        content = style::content,
        page_title = style::page_title,
        divider = style::divider,
        error = style::error,
        error_display = if initial_error.is_empty() { "none" } else { "" },
        save_schedule_items = tr(TextKey::SaveScheduleItems),
        add_schedule_item = tr(TextKey::AddScheduleItem),
        back = tr(TextKey::Back),
        schedule_items = tr(TextKey::ScheduleItems),
        action_failed = tr(TextKey::ScheduleItemActionFailed),
        start = tr(TextKey::Start),
        collection_label = tr(TextKey::Collection),
        playback = tr(TextKey::Playback),
        playout = tr(TextKey::Playout),
        fallback_form = style::fallback_form,
        table_wrap = style::table_wrap,
        section_title = style::section_title,
        editor_form = style::editor_form,
        add_fields = schedule_item_add_fields_html(&data.options),
    )
}

fn schedule_item_fallback_row_html(
    index: usize,
    item: &ScheduleItemForm,
    options: &ScheduleItemsEditorOptions,
) -> String {
    format!(
        r#"<tr><td>{hidden}<input type="hidden" name="item_index" value="{index_value}"><input type="hidden" name="item_id" value="{id}">{start_type}<input class="{input}" name="item_start_time" value="{start_time}"></td><td>{collection_type}{collection}</td><td>{playback}<input class="{input}" name="item_custom_title" placeholder="{custom_title_label}" value="{custom_title}"></td><td>{playout}{multiple_mode}<input class="{input}" name="item_multiple_count" value="{multiple_count}"><input class="{input}" type="number" name="item_duration_hours" value="{duration_hours}"><input class="{input}" type="number" name="item_duration_minutes" value="{duration_minutes}"></td><td><div class="{actions}"><button class="{icon_button}" type="submit" name="action" value="copy:{index}" title="{copy_item}" aria-label="{copy_item}">{copy}</button><button class="{icon_button}" type="submit" name="action" value="move-up:{index}" {move_up_disabled} title="{move_up}" aria-label="{move_up}">{up}</button><button class="{icon_button}" type="submit" name="action" value="move-down:{index}" title="{move_down}" aria-label="{move_down}">{down}</button><button class="{icon_button}" type="submit" name="action" value="delete:{index}" title="{delete_item}" aria-label="{delete_item}">{delete}</button></div></td></tr>"#,
        hidden = schedule_item_hidden_fields_html(item),
        index_value = item.index,
        id = item.id,
        start_type = select_html("item_start_type", item.start_type, &start_type_options()),
        input = style::input,
        start_time = escape_html(&item.start_time.clone().unwrap_or_default()),
        collection_type = select_html(
            "item_collection_type",
            item.collection_type,
            &collection_type_options()
        ),
        collection = option_select_html(
            "item_collection_id",
            item.collection_id,
            &options.collections
        ),
        playback = select_html(
            "item_playback_order",
            item.playback_order,
            &playback_options(item.collection_type)
        ),
        custom_title = escape_html(&item.custom_title),
        custom_title_label = tr(TextKey::CustomTitle),
        playout = select_html("item_playout_mode", item.playout_mode, &playout_options()),
        multiple_mode = select_html(
            "item_multiple_mode",
            item.multiple_mode,
            &multiple_mode_options(item.collection_type, item.playback_order)
        ),
        multiple_count = escape_html(&item.multiple_count),
        duration_hours = item.duration_hours,
        duration_minutes = item.duration_minutes,
        actions = style::actions,
        icon_button = style::icon_button,
        copy_item = tr(TextKey::CopyScheduleItem),
        copy = tr(TextKey::Copy),
        move_up = tr(TextKey::MoveUp),
        up = tr(TextKey::MoveUp),
        move_down = tr(TextKey::MoveDown),
        down = tr(TextKey::MoveDown),
        delete_item = tr(TextKey::DeleteScheduleItem),
        delete = tr(TextKey::Delete),
        move_up_disabled = if index == 0 { "disabled" } else { "" },
    )
}

fn schedule_item_hidden_fields_html(item: &ScheduleItemForm) -> String {
    let mut fields = String::new();
    for (name, value) in [
        (
            "item_fixed_start_time_behavior",
            item.fixed_start_time_behavior
                .map_or_else(String::new, |value| value.to_string()),
        ),
        (
            "item_media_item_id",
            item.media_item_id
                .map_or_else(String::new, |id| id.to_string()),
        ),
        (
            "item_multi_collection_id",
            item.multi_collection_id
                .map_or_else(String::new, |id| id.to_string()),
        ),
        (
            "item_smart_collection_id",
            item.smart_collection_id
                .map_or_else(String::new, |id| id.to_string()),
        ),
        (
            "item_rerun_collection_id",
            item.rerun_collection_id
                .map_or_else(String::new, |id| id.to_string()),
        ),
        (
            "item_playlist_id",
            item.playlist_id
                .map_or_else(String::new, |id| id.to_string()),
        ),
        (
            "item_search_title",
            item.search_title.clone().unwrap_or_default(),
        ),
        (
            "item_search_query",
            item.search_query.clone().unwrap_or_default(),
        ),
        ("item_collection_name", item.collection_name.clone()),
        ("item_marathon_group_by", item.marathon_group_by.to_string()),
        (
            "item_marathon_batch_size",
            item.marathon_batch_size
                .map_or_else(String::new, |value| value.to_string()),
        ),
        (
            "item_fill_with_group_mode",
            item.fill_with_group_mode.to_string(),
        ),
        ("item_tail_mode", item.tail_mode.to_string()),
        (
            "item_discard_to_fill_attempts",
            item.discard_to_fill_attempts
                .map_or_else(String::new, |value| value.to_string()),
        ),
        ("item_guide_mode", item.guide_mode.to_string()),
        (
            "item_pre_roll_filler_id",
            item.pre_roll_filler_id
                .map_or_else(String::new, |id| id.to_string()),
        ),
        (
            "item_mid_roll_filler_id",
            item.mid_roll_filler_id
                .map_or_else(String::new, |id| id.to_string()),
        ),
        (
            "item_post_roll_filler_id",
            item.post_roll_filler_id
                .map_or_else(String::new, |id| id.to_string()),
        ),
        (
            "item_tail_filler_id",
            item.tail_filler_id
                .map_or_else(String::new, |id| id.to_string()),
        ),
        (
            "item_fallback_filler_id",
            item.fallback_filler_id
                .map_or_else(String::new, |id| id.to_string()),
        ),
        ("item_watermark_ids", id_csv(&item.watermark_ids)),
        (
            "item_graphics_element_ids",
            id_csv(&item.graphics_element_ids),
        ),
        (
            "item_preferred_audio_language_code",
            item.preferred_audio_language_code
                .clone()
                .unwrap_or_default(),
        ),
        (
            "item_preferred_audio_title",
            item.preferred_audio_title.clone(),
        ),
        (
            "item_preferred_subtitle_language_code",
            item.preferred_subtitle_language_code
                .clone()
                .unwrap_or_default(),
        ),
        (
            "item_subtitle_mode",
            item.subtitle_mode
                .map_or_else(String::new, |value| value.to_string()),
        ),
    ] {
        fields.push_str(&hidden_input_html(name, &value));
    }
    if item.marathon_shuffle_groups {
        fields.push_str(&hidden_input_html(
            "item_marathon_shuffle_groups_index",
            &item.index.to_string(),
        ));
    }
    if item.marathon_shuffle_items {
        fields.push_str(&hidden_input_html(
            "item_marathon_shuffle_items_index",
            &item.index.to_string(),
        ));
    }
    fields
}

fn schedule_item_add_fields_html(options: &ScheduleItemsEditorOptions) -> String {
    format!(
        r#"<input type="hidden" name="new_collection_type" value="{collection_type}"><label class="{field}"><span>{collection_label}</span>{collection}</label><label class="{field}"><span>{playback_order}</span>{playback}</label><label class="{field}"><span>{playout_mode}</span>{playout}</label>"#,
        field = style::field,
        collection_type = COLLECTION_TYPE_COLLECTION,
        collection_label = tr(TextKey::Collection),
        collection = option_select_html("new_collection_id", None, &options.collections),
        playback_order = tr(TextKey::PlaybackOrder),
        playback = select_html(
            "new_playback_order",
            PLAYBACK_ORDER_SHUFFLE,
            &playback_options(COLLECTION_TYPE_COLLECTION)
        ),
        playout_mode = tr(TextKey::PlayoutMode),
        playout = select_html("new_playout_mode", PLAYOUT_MODE_ONE, &playout_options()),
    )
}

fn hidden_input_html(name: &str, value: &str) -> String {
    format!(
        r#"<input type="hidden" name="{}" value="{}">"#,
        escape_html(name),
        escape_html(value)
    )
}

fn select_html(name: &str, selected: i32, options: &[(i32, &'static str)]) -> String {
    let options = options
        .iter()
        .fold(String::new(), |mut html, (value, label)| {
            let selected_attr = if *value == selected { " selected" } else { "" };
            let _ = write!(
                html,
                r#"<option value="{}"{}>{}</option>"#,
                value,
                selected_attr,
                escape_html(label)
            );
            html
        });
    format!(
        r#"<select class="{}" name="{}">{}</select>"#,
        style::select,
        escape_html(name),
        options
    )
}

fn option_select_html(name: &str, selected: Option<i64>, options: &[ScheduleItemOption]) -> String {
    let mut option_html = r#"<option value=""></option>"#.to_string();
    for option in options {
        let selected_attr = if Some(option.id) == selected {
            " selected"
        } else {
            ""
        };
        let _ = write!(
            option_html,
            r#"<option value="{}"{}>{}</option>"#,
            option.id,
            selected_attr,
            escape_html(&option.name)
        );
    }
    format!(
        r#"<select class="{}" name="{}">{}</select>"#,
        style::select,
        escape_html(name),
        option_html
    )
}

fn id_csv(ids: &[i64]) -> String {
    ids.iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[island]
fn ScheduleItemsIsland(id: i64) -> impl IntoView {
    let schedule_name = RwSignal::new(String::new());
    let shuffle_schedule_items = RwSignal::new(false);
    let items = RwSignal::new(Vec::<ScheduleItemForm>::new());
    let options = RwSignal::new(ScheduleItemsEditorOptions::default());
    let selected = RwSignal::new(None::<usize>);
    let error = RwSignal::new(String::new());

    let data = Resource::new_blocking(
        move || id,
        |id| async move { get_schedule_items_page(id).await.ok().flatten() },
    );
    Effect::new(move |_| {
        if let Some(Some(data)) = data.get() {
            schedule_name.set(data.schedule.name);
            shuffle_schedule_items.set(data.schedule.shuffle_schedule_items);
            items.set(data.items);
            options.set(data.options);
            selected.set(None);
        }
    });

    let save_action = Action::new(|(schedule_id, items): &(i64, Vec<ScheduleItemForm>)| {
        replace_schedule_items(*schedule_id, items.clone())
    });
    Effect::new(move |_| {
        if let Some(result) = save_action.value().get() {
            match result {
                Ok(()) => {
                    error.set(String::new());
                    data.refetch();
                }
                Err(err) => error.set(schedule_error_message(err)),
            }
        }
    });

    let add_item = move |_| {
        let item = ScheduleItemForm {
            index: i32::try_from(items.get_untracked().len()).unwrap_or(i32::MAX),
            ..ScheduleItemForm::default()
        };
        items.update(|items| {
            items.push(item);
            selected.set(Some(items.len().saturating_sub(1)));
        });
    };
    let copy_item = move |index: usize| {
        items.update(|items| {
            if let Some(source) = items.get(index).cloned() {
                let mut copy = source;
                copy.id = 0;
                copy.index = i32::try_from(index + 1).unwrap_or(i32::MAX);
                items.insert(index + 1, copy);
                normalize_indexes(items);
                selected.set(Some(index + 1));
            }
        });
    };
    let move_item = move |from: usize, to: usize| {
        items.update(|items| {
            if from < items.len() && to < items.len() {
                items.swap(from, to);
                normalize_indexes(items);
                selected.set(Some(to));
            }
        });
    };
    let delete_item = move |index: usize| {
        items.update(|items| {
            if index < items.len() {
                items.remove(index);
                normalize_indexes(items);
                selected.set(None);
            }
        });
    };

    view! {
        <div class=style::page>
            <div class=style::action_bar>
                <div class=style::action_row>
                    <div class=style::grow>
                        {move || selected.get().and_then(|index| {
                            items.get().get(index).map(|item| {
                                format!("{} ({})", schedule_name.get(), item_summary(item))
                            })
                        }).unwrap_or_else(|| schedule_name.get())}
                    </div>
                    <button class=style::btn_primary on:click=move |_| { save_action.dispatch((id, items.get_untracked())); }>
                        <span style=icon_style(IconName::Save, "1.125rem") aria-hidden="true"></span>
                        <span>{tr(TextKey::SaveScheduleItems)}</span>
                    </button>
                    <button class=style::btn_default on:click=add_item>
                        <span style=icon_style(IconName::Add, "1.125rem") aria-hidden="true"></span>
                        <span>{tr(TextKey::AddScheduleItem)}</span>
                    </button>
                    <a href="/schedules" class=style::btn_default>{tr(TextKey::Back)}</a>
                </div>
            </div>
            <section class=style::content>
                <h1 class=style::page_title>{tr(TextKey::ScheduleItems)}</h1>
                <div class=style::divider/>
                <p class=style::error style:display=move || if error.get().is_empty() { "none" } else { "" }>
                    {move || error.get()}
                </p>
                <Suspense fallback=move || view! { <p class=style::empty>{tr(TextKey::Loading)}</p> }>
                    {move || data.get().map(|data| {
                        if data.is_some() {
                            view! {
                                <div class=style::table_wrap>
                                    <table>
                                        <colgroup>
                                            <col/>
                                            <col/>
                                            <col/>
                                            <col style="width: 240px"/>
                                        </colgroup>
                                        <thead>
                                            <tr>
                                                <th>{tr(TextKey::StartTime)}</th>
                                                <th>{tr(TextKey::Collection)}</th>
                                                <th>{tr(TextKey::PlayoutMode)}</th>
                                                <th/>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            {let rows = items.get();
                                            if rows.is_empty() {
                                                view! { <tr><td colspan="4" class=style::empty>{tr(TextKey::NoScheduleItems)}</td></tr> }.into_any()
                                            } else {
                                                rows.into_iter().enumerate().map(|(index, item)| {
                                                    let row = list_row_from_item(&item);
                                                    view! {
                                                        <tr class=move || if selected.get() == Some(index) { style::selected_row } else { "" } on:click=move |_| selected.set(Some(index))>
                                                            <td>{start_label(&row)}</td>
                                                            <td>{collection_name_display(&item)}</td>
                                                            <td>{playout_label(&row)}</td>
                                                            <td>
                                                                <div class=style::actions>
                                                                    <button class=style::icon_button title=tr(TextKey::CopyScheduleItem) aria-label=tr(TextKey::CopyScheduleItem) on:click=move |_| copy_item(index)>
                                                                        <span style=icon_style(IconName::Copy, "1.25rem") aria-hidden="true"></span>
                                                                    </button>
                                                                    <button class=style::icon_button disabled=move || { shuffle_schedule_items.get() || index == 0 } title=tr(TextKey::MoveUp) aria-label=tr(TextKey::MoveUp) on:click=move |_| move_item(index, index.saturating_sub(1))>
                                                                        <span style=icon_style(IconName::ChevronLeft, "1.25rem") aria-hidden="true"></span>
                                                                    </button>
                                                                    <button class=style::icon_button disabled=move || { shuffle_schedule_items.get() || (index + 1) >= items.get().len() } title=tr(TextKey::MoveDown) aria-label=tr(TextKey::MoveDown) on:click=move |_| move_item(index, index + 1)>
                                                                        <span style=icon_style(IconName::ChevronRight, "1.25rem") aria-hidden="true"></span>
                                                                    </button>
                                                                    <button class=style::icon_button title=tr(TextKey::DeleteScheduleItem) aria-label=tr(TextKey::DeleteScheduleItem) on:click=move |_| delete_item(index)>
                                                                        <span style=icon_style(IconName::Delete, "1.25rem") aria-hidden="true"></span>
                                                                    </button>
                                                                </div>
                                                            </td>
                                                        </tr>
                                                    }
                                                }).collect_view().into_any()
                                            }}
                                        </tbody>
                                    </table>
                                </div>
                                {move || selected.get().and_then(|index| items.get().get(index).cloned().map(|item| {
                                    let editor_options = options.get();
                                    schedule_item_editor(index, &item, items, &editor_options, shuffle_schedule_items.get())
                                })).unwrap_or_else(|| view! { <p class=style::empty>{tr(TextKey::SelectScheduleItemPrompt)}</p> }.into_any())}
                            }.into_any()
                        } else {
                            view! { <p class=style::empty>{tr(TextKey::ScheduleNotFound)}</p> }.into_any()
                        }
                    })}
                </Suspense>
            </section>
        </div>
    }
}

fn start_label(item: &ScheduleItemListRow) -> String {
    if item.start_type == 2 {
        item.start_time.clone().unwrap_or_default()
    } else {
        tr(TextKey::Dynamic).to_string()
    }
}

fn playout_label(item: &ScheduleItemListRow) -> String {
    match item.playout_mode {
        1 => tr(TextKey::Flood).to_string(),
        2 => tr(TextKey::One).to_string(),
        3 => match item.multiple_mode {
            Some(1) => tr(TextKey::CollectionSize).to_string(),
            Some(2) => tr(TextKey::PlaylistItemSize).to_string(),
            Some(3) => tr(TextKey::MultiEpisodeGroupSize).to_string(),
            _ => format!(
                "{} ({})",
                tr(TextKey::Multiple),
                item.multiple_count.clone().unwrap_or_default()
            ),
        },
        4 => tr(TextKey::Duration).to_string(),
        _ => String::new(),
    }
}

fn schedule_item_editor(
    index: usize,
    item: &ScheduleItemForm,
    items: RwSignal<Vec<ScheduleItemForm>>,
    options: &ScheduleItemsEditorOptions,
    shuffle_schedule_items: bool,
) -> AnyView {
    let selected_collection_type = item.collection_type;
    let selected_playout_mode = item.playout_mode;
    let selected_playback_order = item.playback_order;
    let start_type_options = if shuffle_schedule_items {
        vec![(START_TYPE_DYNAMIC, tr(TextKey::Dynamic))]
    } else {
        vec![
            (START_TYPE_DYNAMIC, tr(TextKey::Dynamic)),
            (START_TYPE_FIXED, tr(TextKey::Fixed)),
        ]
    };
    let playout_mode_options = if shuffle_schedule_items {
        vec![
            (PLAYOUT_MODE_ONE, tr(TextKey::One)),
            (PLAYOUT_MODE_MULTIPLE, tr(TextKey::Multiple)),
            (PLAYOUT_MODE_DURATION, tr(TextKey::Duration)),
        ]
    } else {
        playout_options()
    };
    let can_fill_with_groups = can_fill_with_groups(item);

    view! {
        <form class=style::item_editor>
            <h2 class=style::section_title>{tr(TextKey::ScheduleItem)}</h2>
            {select_i32(tr(TextKey::StartType), item.start_type, start_type_options, move |value| {
                update_item(items, index, |item| {
                    item.start_type = value;
                    if value == START_TYPE_DYNAMIC {
                        item.start_time = None;
                        item.fixed_start_time_behavior = None;
                    }
                });
            })}
            {text_field(tr(TextKey::StartTime), item.start_time.clone().unwrap_or_else(|| "00:00:00".to_string()), selected_collection_type == -1, move |value| {
                update_item(items, index, |item| item.start_time = Some(value));
            })}
            {select_i32(tr(TextKey::FixedStartTimeBehavior), item.fixed_start_time_behavior.unwrap_or(0), vec![(0, tr(TextKey::Strict)), (1, tr(TextKey::Flexible))], move |value| {
                update_item(items, index, |item| item.fixed_start_time_behavior = Some(value));
            })}
            {select_i32(tr(TextKey::CollectionType), item.collection_type, collection_type_options(), move |value| {
                update_item(items, index, |item| {
                    item.collection_type = value;
                    clear_collection_refs(item);
                    apply_collection_type_rules(item);
                });
            })}
            {collection_picker(index, item, items, options)}
            {select_i32(tr(TextKey::PlaybackOrder), item.playback_order, playback_options(item.collection_type), move |value| {
                update_item(items, index, |item| {
                    item.playback_order = value;
                    apply_playback_order_rules(item);
                });
            })}
            <div style:display=move || if selected_playback_order == PLAYBACK_ORDER_MARATHON { "" } else { "none" }>
                {select_i32(tr(TextKey::MarathonGroupBy), item.marathon_group_by, vec![(4, tr(TextKey::Album)), (3, tr(TextKey::Artist)), (2, tr(TextKey::Season)), (1, tr(TextKey::Show)), (5, tr(TextKey::Director))], move |value| {
                    update_item(items, index, |item| item.marathon_group_by = value);
                })}
                {bool_field(tr(TextKey::MarathonShuffleGroups), item.marathon_shuffle_groups, move |value| {
                    update_item(items, index, |item| item.marathon_shuffle_groups = value);
                })}
                {bool_field(tr(TextKey::MarathonShuffleItems), item.marathon_shuffle_items, move |value| {
                    update_item(items, index, |item| item.marathon_shuffle_items = value);
                })}
                {number_field(tr(TextKey::MarathonBatchSize), item.marathon_batch_size.unwrap_or(0), move |value| {
                    update_item(items, index, |item| item.marathon_batch_size = positive_i32(value));
                })}
            </div>
            {select_i32(tr(TextKey::PlayoutMode), item.playout_mode, playout_mode_options, move |value| {
                update_item(items, index, |item| {
                    item.playout_mode = value;
                    if matches!(value, PLAYOUT_MODE_FLOOD | PLAYOUT_MODE_ONE) {
                        item.fill_with_group_mode = FILL_WITH_GROUP_MODE_NONE;
                    }
                });
            })}
            {select_i32(tr(TextKey::MultipleMode), item.multiple_mode, multiple_mode_options(item.collection_type, item.playback_order), move |value| {
                update_item(items, index, |item| item.multiple_mode = value);
            })}
            {text_field(tr(TextKey::MultipleCount), item.multiple_count.clone(), selected_playout_mode != PLAYOUT_MODE_MULTIPLE || item.multiple_mode != MULTIPLE_MODE_COUNT, move |value| {
                update_item(items, index, |item| item.multiple_count = value);
            })}
            {number_field(tr(TextKey::PlayoutDurationHours), item.duration_hours, move |value| {
                update_item(items, index, |item| item.duration_hours = value.max(0));
            })}
            {number_field(tr(TextKey::PlayoutDurationMinutes), item.duration_minutes, move |value| {
                update_item(items, index, |item| item.duration_minutes = value.clamp(0, 59));
            })}
            {select_i32_disabled(tr(TextKey::FillWithGroupMode), item.fill_with_group_mode, vec![(0, tr(TextKey::NoneOption)), (1, tr(TextKey::OrderedGroups)), (2, tr(TextKey::ShuffledGroups))], !can_fill_with_groups, move |value| {
                update_item(items, index, |item| item.fill_with_group_mode = value);
            })}
            {select_i32(tr(TextKey::TailMode), item.tail_mode, vec![(0, tr(TextKey::NoneOption)), (1, tr(TextKey::Offline)), (3, tr(TextKey::FillerPresets))], move |value| {
                update_item(items, index, |item| item.tail_mode = value);
            })}
            {number_field(tr(TextKey::DiscardToFillAttempts), item.discard_to_fill_attempts.unwrap_or(0), move |value| {
                update_item(items, index, |item| item.discard_to_fill_attempts = Some(value.max(0)));
            })}
            {text_field(tr(TextKey::CustomTitle), item.custom_title.clone(), false, move |value| {
                update_item(items, index, |item| item.custom_title = value);
            })}
            {select_i32(tr(TextKey::GuideMode), item.guide_mode, vec![(0, tr(TextKey::Normal)), (1, tr(TextKey::FillerPresets))], move |value| {
                update_item(items, index, |item| item.guide_mode = value);
            })}

            <h2 class=style::section_title>{tr(TextKey::ScheduleItemFiller)}</h2>
            {filler_picker(tr(TextKey::PreRollFiller), item.pre_roll_filler_id, &options.fillers, move |value| {
                update_item(items, index, |item| item.pre_roll_filler_id = value);
            })}
            {filler_picker(tr(TextKey::MidRollFiller), item.mid_roll_filler_id, &options.fillers, move |value| {
                update_item(items, index, |item| item.mid_roll_filler_id = value);
            })}
            {filler_picker(tr(TextKey::PostRollFiller), item.post_roll_filler_id, &options.fillers, move |value| {
                update_item(items, index, |item| item.post_roll_filler_id = value);
            })}
            {filler_picker(tr(TextKey::TailFiller), item.tail_filler_id, &options.fillers, move |value| {
                update_item(items, index, |item| item.tail_filler_id = value);
            })}
            {filler_picker(tr(TextKey::FallbackFiller), item.fallback_filler_id, &options.fillers, move |value| {
                update_item(items, index, |item| item.fallback_filler_id = value);
            })}

            <h2 class=style::section_title>{tr(TextKey::ScheduleItemOverrides)}</h2>
            {multi_select_i64(tr(TextKey::Watermarks), &item.watermark_ids, options.watermarks.clone(), move |values| {
                update_item(items, index, |item| item.watermark_ids = values);
            })}
            {multi_select_i64(tr(TextKey::GraphicsElements), &item.graphics_element_ids, options.graphics_elements.clone(), move |values| {
                update_item(items, index, |item| item.graphics_element_ids = values);
            })}
            {language_picker(tr(TextKey::PreferredAudioLanguage), item.preferred_audio_language_code.clone(), options.languages.clone(), move |value| {
                update_item(items, index, |item| item.preferred_audio_language_code = blank_to_none(&value));
            })}
            {text_field(tr(TextKey::PreferredAudioTitle), item.preferred_audio_title.clone(), false, move |value| {
                update_item(items, index, |item| item.preferred_audio_title = value);
            })}
            {language_picker(tr(TextKey::PreferredSubtitleLanguage), item.preferred_subtitle_language_code.clone(), options.languages.clone(), move |value| {
                update_item(items, index, |item| item.preferred_subtitle_language_code = blank_to_none(&value));
            })}
            {select_i32(tr(TextKey::SubtitleMode), item.subtitle_mode.unwrap_or(0), vec![(0, tr(TextKey::NoneOption)), (1, tr(TextKey::Forced)), (2, tr(TextKey::DefaultOption)), (3, tr(TextKey::Any))], move |value| {
                update_item(items, index, |item| item.subtitle_mode = Some(value));
            })}
        </form>
    }
    .into_any()
}

fn collection_picker(
    index: usize,
    item: &ScheduleItemForm,
    items: RwSignal<Vec<ScheduleItemForm>>,
    options: &ScheduleItemsEditorOptions,
) -> AnyView {
    match item.collection_type {
        COLLECTION_TYPE_COLLECTION => {
            let opts = options.collections.clone();
            option_picker(tr(TextKey::Collection), item.collection_id, opts.clone(), move |value| update_item(items, index, |item| {
                item.collection_id = value;
                item.collection_name = option_name(&opts, value);
            }))
        }
        COLLECTION_TYPE_TELEVISION_SHOW => {
            let opts = options.shows.clone();
            option_picker(tr(TextKey::TelevisionShow), item.media_item_id, opts.clone(), move |value| update_item(items, index, |item| {
                item.media_item_id = value;
                item.collection_name = option_name(&opts, value);
            }))
        }
        COLLECTION_TYPE_TELEVISION_SEASON => {
            let opts = options.seasons.clone();
            option_picker(tr(TextKey::TelevisionSeason), item.media_item_id, opts.clone(), move |value| update_item(items, index, |item| {
                item.media_item_id = value;
                item.collection_name = option_name(&opts, value);
            }))
        }
        COLLECTION_TYPE_ARTIST => {
            let opts = options.artists.clone();
            option_picker(tr(TextKey::Artist), item.media_item_id, opts.clone(), move |value| update_item(items, index, |item| {
                item.media_item_id = value;
                item.collection_name = option_name(&opts, value);
            }))
        }
        COLLECTION_TYPE_MULTI_COLLECTION => {
            let opts = options.multi_collections.clone();
            option_picker(tr(TextKey::MultiCollection), item.multi_collection_id, opts.clone(), move |value| update_item(items, index, |item| {
                item.multi_collection_id = value;
                item.collection_name = option_name(&opts, value);
            }))
        }
        COLLECTION_TYPE_SMART_COLLECTION => {
            let opts = options.smart_collections.clone();
            option_picker(tr(TextKey::SmartCollection), item.smart_collection_id, opts.clone(), move |value| update_item(items, index, |item| {
                item.smart_collection_id = value;
                item.collection_name = option_name(&opts, value);
            }))
        }
        COLLECTION_TYPE_PLAYLIST => playlist_picker(index, item.playlist_id, items, options.playlists.clone()),
        COLLECTION_TYPE_RERUN_FIRST_RUN | COLLECTION_TYPE_RERUN_RERUN => {
            let opts = options.rerun_collections.clone();
            option_picker(tr(TextKey::RerunCollections), item.rerun_collection_id, opts.clone(), move |value| update_item(items, index, |item| {
                item.rerun_collection_id = value;
                item.collection_name = option_name(&opts, value);
            }))
        }
        COLLECTION_TYPE_SEARCH_QUERY => view! {
            <div>
                {text_field(tr(TextKey::SearchTitle), item.search_title.clone().unwrap_or_default(), false, move |value| update_item(items, index, |item| item.search_title = blank_to_none(&value)))}
                {text_field(tr(TextKey::SearchQuery), item.search_query.clone().unwrap_or_default(), false, move |value| update_item(items, index, |item| item.search_query = blank_to_none(&value)))}
            </div>
        }.into_any(),
        _ => view! { <div/> }.into_any(),
    }
}

fn playlist_picker(
    index: usize,
    selected: Option<i64>,
    items: RwSignal<Vec<ScheduleItemForm>>,
    options: Vec<PlaylistOption>,
) -> AnyView {
    let options_for_change = options.clone();
    view! {
        <label class=style::field>
            <span>{tr(TextKey::Playlist)}</span>
            <select class=style::select prop:value=selected.map(|id| id.to_string()).unwrap_or_default() on:change=move |event| {
                let value = parse_optional_i64(&event_target_value(&event));
                update_item(items, index, |item| {
                    item.playlist_id = value;
                    item.collection_name = playlist_name(&options_for_change, value);
                });
            }>
                <option value="">""</option>
                {options.into_iter().map(|option| view! {
                    <option value=option.id.to_string()>{option.name}</option>
                }).collect_view()}
            </select>
        </label>
    }.into_any()
}

fn option_picker<F>(
    label: &'static str,
    selected: Option<i64>,
    options: Vec<ScheduleItemOption>,
    on_change: F,
) -> AnyView
where
    F: Fn(Option<i64>) + 'static,
{
    view! {
        <label class=style::field>
            <span>{label}</span>
            <select class=style::select prop:value=selected.map(|id| id.to_string()).unwrap_or_default() on:change=move |event| on_change(parse_optional_i64(&event_target_value(&event)))>
                <option value="">""</option>
                {options.into_iter().map(|option| view! {
                    <option value=option.id.to_string()>{option.name}</option>
                }).collect_view()}
            </select>
        </label>
    }.into_any()
}

fn filler_picker<F>(
    label: &'static str,
    selected: Option<i64>,
    options: &[FillerOption],
    on_change: F,
) -> AnyView
where
    F: Fn(Option<i64>) + 'static,
{
    let options = options.to_vec();
    view! {
        <label class=style::field>
            <span>{label}</span>
            <select class=style::select prop:value=selected.map(|id| id.to_string()).unwrap_or_default() on:change=move |event| on_change(parse_optional_i64(&event_target_value(&event)))>
                <option value="">""</option>
                {options.into_iter().map(|option| view! {
                    <option value=option.id.to_string()>{option.name}</option>
                }).collect_view()}
            </select>
        </label>
    }.into_any()
}

fn language_picker<F>(
    label: &'static str,
    selected: Option<String>,
    options: Vec<ScheduleLanguageOption>,
    on_change: F,
) -> AnyView
where
    F: Fn(String) + 'static,
{
    view! {
        <label class=style::field>
            <span>{label}</span>
            <select class=style::select prop:value=selected.unwrap_or_default() on:change=move |event| on_change(event_target_value(&event))>
                <option value="">{tr(TextKey::NoneOption)}</option>
                {options.into_iter().map(|option| view! {
                    <option value=option.code>{option.name}</option>
                }).collect_view()}
            </select>
        </label>
    }.into_any()
}

fn select_i32<F>(
    label: &'static str,
    selected: i32,
    options: Vec<(i32, &'static str)>,
    on_change: F,
) -> AnyView
where
    F: Fn(i32) + 'static,
{
    select_i32_disabled(label, selected, options, false, on_change)
}

fn select_i32_disabled<F>(
    label: &'static str,
    selected: i32,
    options: Vec<(i32, &'static str)>,
    disabled: bool,
    on_change: F,
) -> AnyView
where
    F: Fn(i32) + 'static,
{
    view! {
        <label class=style::field>
            <span>{label}</span>
            <select class=style::select prop:value=selected.to_string() disabled=disabled on:change=move |event| {
                if let Ok(value) = event_target_value(&event).parse::<i32>() {
                    on_change(value);
                }
            }>
                {options.into_iter().map(|(value, label)| view! {
                    <option value=value.to_string()>{label}</option>
                }).collect_view()}
            </select>
        </label>
    }
    .into_any()
}

fn text_field<F>(label: &'static str, value: String, disabled: bool, on_input: F) -> AnyView
where
    F: Fn(String) + 'static,
{
    view! {
        <label class=style::field>
            <span>{label}</span>
            <input class=style::input value=value disabled=disabled on:input=move |event| on_input(event_target_value(&event))/>
        </label>
    }.into_any()
}

fn number_field<F>(label: &'static str, value: i32, on_input: F) -> AnyView
where
    F: Fn(i32) + 'static,
{
    view! {
        <label class=style::field>
            <span>{label}</span>
            <input class=style::input type="number" value=value.to_string() on:input=move |event| {
                if let Ok(value) = event_target_value(&event).parse::<i32>() {
                    on_input(value);
                }
            }/>
        </label>
    }
    .into_any()
}

fn bool_field<F>(label: &'static str, value: bool, on_change: F) -> AnyView
where
    F: Fn(bool) + 'static,
{
    view! {
        <label class=style::field>
            <span>{label}</span>
            <span>
                <label class=style::check>
                    <input type="checkbox" value="true" checked=value prop:checked=value on:change=move |event| on_change(event_target_checked(&event))/>
                    <span class=style::caption>{label}</span>
                </label>
            </span>
        </label>
    }.into_any()
}

fn multi_select_i64<F>(
    label: &'static str,
    selected: &[i64],
    options: Vec<ScheduleItemOption>,
    on_change: F,
) -> AnyView
where
    F: Fn(Vec<i64>) + Copy + 'static,
{
    let selected = selected.to_vec();
    view! {
        <label class=style::field>
            <span>{label}</span>
            <span class=style::check_list>
                {options.into_iter().map(|option| {
                    let is_selected = selected.contains(&option.id);
                    let selected_values = selected.clone();
                    let option_id = option.id;
                    view! {
                        <label class=style::check>
                            <input
                                type="checkbox"
                                value=option.id.to_string()
                                checked=is_selected
                                prop:checked=is_selected
                                on:change=move |event| {
                                    let mut values = selected_values.clone();
                                    if event_target_checked(&event) {
                                        if !values.contains(&option_id) {
                                            values.push(option_id);
                                        }
                                    } else {
                                        values.retain(|value| *value != option_id);
                                    }
                                    on_change(values);
                                }
                            />
                            <span>{option.name}</span>
                        </label>
                    }
                }).collect_view()}
            </span>
        </label>
    }
    .into_any()
}

fn update_item<F>(items: RwSignal<Vec<ScheduleItemForm>>, index: usize, update: F)
where
    F: FnOnce(&mut ScheduleItemForm),
{
    items.update(|items| {
        if let Some(item) = items.get_mut(index) {
            update(item);
        }
    });
}

fn normalize_indexes(items: &mut [ScheduleItemForm]) {
    for (index, item) in items.iter_mut().enumerate() {
        item.index = i32::try_from(index).unwrap_or(i32::MAX);
    }
}

fn apply_collection_type_rules(item: &mut ScheduleItemForm) {
    if item.collection_type != COLLECTION_TYPE_PLAYLIST
        && item.multiple_mode == MULTIPLE_MODE_PLAYLIST_ITEM_SIZE
    {
        item.multiple_mode = MULTIPLE_MODE_COUNT;
    }

    if matches!(
        item.collection_type,
        COLLECTION_TYPE_PLAYLIST | COLLECTION_TYPE_RERUN_FIRST_RUN | COLLECTION_TYPE_RERUN_RERUN
    ) {
        item.playback_order = PLAYBACK_ORDER_NONE;
    }

    if item.collection_type == COLLECTION_TYPE_MULTI_COLLECTION {
        item.playback_order = PLAYBACK_ORDER_SHUFFLE;
    }

    apply_playback_order_rules(item);
}

fn apply_playback_order_rules(item: &mut ScheduleItemForm) {
    if item.playback_order != PLAYBACK_ORDER_CHRONOLOGICAL
        && item.multiple_mode == MULTIPLE_MODE_MULTI_EPISODE_GROUP_SIZE
    {
        item.multiple_mode = MULTIPLE_MODE_COUNT;
    }

    if item.playback_order != PLAYBACK_ORDER_MARATHON {
        item.marathon_group_by = MARATHON_GROUP_BY_NONE;
        item.marathon_shuffle_groups = false;
        item.marathon_shuffle_items = false;
        item.marathon_batch_size = None;
    }

    if !can_fill_with_groups(item) {
        item.fill_with_group_mode = FILL_WITH_GROUP_MODE_NONE;
    }
}

fn clear_collection_refs(item: &mut ScheduleItemForm) {
    item.collection_id = None;
    item.media_item_id = None;
    item.multi_collection_id = None;
    item.smart_collection_id = None;
    item.rerun_collection_id = None;
    item.playlist_id = None;
    item.search_title = None;
    item.search_query = None;
    item.collection_name.clear();
}

fn normalized_items(mut items: Vec<ScheduleItemForm>) -> Vec<ScheduleItemForm> {
    normalize_indexes(&mut items);
    items
}

fn option_name(options: &[ScheduleItemOption], selected: Option<i64>) -> String {
    selected
        .and_then(|id| options.iter().find(|option| option.id == id))
        .map(|option| option.name.clone())
        .unwrap_or_default()
}

fn playlist_name(options: &[PlaylistOption], selected: Option<i64>) -> String {
    selected
        .and_then(|id| options.iter().find(|option| option.id == id))
        .map(|option| option.name.clone())
        .unwrap_or_default()
}

fn start_type_options() -> Vec<(i32, &'static str)> {
    vec![
        (START_TYPE_DYNAMIC, tr(TextKey::Dynamic)),
        (START_TYPE_FIXED, tr(TextKey::Fixed)),
    ]
}

fn collection_type_options() -> Vec<(i32, &'static str)> {
    vec![
        (COLLECTION_TYPE_COLLECTION, tr(TextKey::Collection)),
        (COLLECTION_TYPE_TELEVISION_SHOW, tr(TextKey::TelevisionShow)),
        (
            COLLECTION_TYPE_TELEVISION_SEASON,
            tr(TextKey::TelevisionSeason),
        ),
        (COLLECTION_TYPE_ARTIST, tr(TextKey::Artist)),
        (
            COLLECTION_TYPE_MULTI_COLLECTION,
            tr(TextKey::MultiCollection),
        ),
        (
            COLLECTION_TYPE_SMART_COLLECTION,
            tr(TextKey::SmartCollection),
        ),
        (COLLECTION_TYPE_SEARCH_QUERY, tr(TextKey::SearchQuery)),
        (COLLECTION_TYPE_PLAYLIST, tr(TextKey::Playlist)),
        (COLLECTION_TYPE_RERUN_FIRST_RUN, tr(TextKey::RerunFirstRun)),
        (COLLECTION_TYPE_RERUN_RERUN, tr(TextKey::RerunRerun)),
    ]
}

fn playout_options() -> Vec<(i32, &'static str)> {
    vec![
        (PLAYOUT_MODE_FLOOD, tr(TextKey::Flood)),
        (PLAYOUT_MODE_ONE, tr(TextKey::One)),
        (PLAYOUT_MODE_MULTIPLE, tr(TextKey::Multiple)),
        (PLAYOUT_MODE_DURATION, tr(TextKey::Duration)),
    ]
}

fn multiple_mode_options(collection_type: i32, playback_order: i32) -> Vec<(i32, &'static str)> {
    let mut options = vec![(MULTIPLE_MODE_COUNT, tr(TextKey::Count))];
    if collection_type == COLLECTION_TYPE_PLAYLIST {
        options.push((
            MULTIPLE_MODE_PLAYLIST_ITEM_SIZE,
            tr(TextKey::PlaylistItemSize),
        ));
    } else {
        options.push((MULTIPLE_MODE_COLLECTION_SIZE, tr(TextKey::CollectionSize)));
    }
    if playback_order == PLAYBACK_ORDER_CHRONOLOGICAL {
        options.push((
            MULTIPLE_MODE_MULTI_EPISODE_GROUP_SIZE,
            tr(TextKey::MultiEpisodeGroupSize),
        ));
    }
    options
}

fn playback_options(collection_type: i32) -> Vec<(i32, &'static str)> {
    match collection_type {
        COLLECTION_TYPE_MULTI_COLLECTION => vec![
            (PLAYBACK_ORDER_SHUFFLE, tr(TextKey::Shuffle)),
            (PLAYBACK_ORDER_SHUFFLE_IN_ORDER, tr(TextKey::ShuffleInOrder)),
        ],
        COLLECTION_TYPE_TELEVISION_SHOW => vec![
            (PLAYBACK_ORDER_CHRONOLOGICAL, tr(TextKey::Chronological)),
            (PLAYBACK_ORDER_SEASON_EPISODE, tr(TextKey::SeasonEpisode)),
            (PLAYBACK_ORDER_RANDOM, tr(TextKey::Random)),
            (PLAYBACK_ORDER_SHUFFLE, tr(TextKey::Shuffle)),
            (5, tr(TextKey::MultiEpisodeShuffle)),
        ],
        COLLECTION_TYPE_PLAYLIST
        | COLLECTION_TYPE_RERUN_FIRST_RUN
        | COLLECTION_TYPE_RERUN_RERUN => {
            vec![(PLAYBACK_ORDER_NONE, tr(TextKey::NoneOption))]
        }
        _ => vec![
            (PLAYBACK_ORDER_CHRONOLOGICAL, tr(TextKey::Chronological)),
            (PLAYBACK_ORDER_RANDOM, tr(TextKey::Random)),
            (PLAYBACK_ORDER_SHUFFLE, tr(TextKey::Shuffle)),
            (PLAYBACK_ORDER_SHUFFLE_IN_ORDER, tr(TextKey::ShuffleInOrder)),
            (PLAYBACK_ORDER_MARATHON, tr(TextKey::Marathon)),
        ],
    }
}

fn can_fill_with_groups(item: &ScheduleItemForm) -> bool {
    matches!(
        item.playout_mode,
        PLAYOUT_MODE_MULTIPLE | PLAYOUT_MODE_DURATION
    ) && item.playback_order != PLAYBACK_ORDER_SHUFFLE_IN_ORDER
        && matches!(
            item.collection_type,
            COLLECTION_TYPE_COLLECTION
                | COLLECTION_TYPE_MULTI_COLLECTION
                | COLLECTION_TYPE_SMART_COLLECTION
        )
}

fn parse_optional_i64(value: &str) -> Option<i64> {
    value.parse::<i64>().ok().filter(|value| *value > 0)
}

fn blank_to_none(value: &str) -> Option<String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn positive_i32(value: i32) -> Option<i32> {
    if value > 0 {
        Some(value)
    } else {
        None
    }
}

fn item_summary(item: &ScheduleItemForm) -> String {
    format!(
        "{} - {} - {}",
        start_label(&list_row_from_item(item)),
        collection_name_display(item),
        playout_label(&list_row_from_item(item))
    )
}

fn collection_name_display(item: &ScheduleItemForm) -> String {
    if let Some(media_item_id) = item.media_item_id {
        let fallback = format!("Media Item #{media_item_id}");
        if item.collection_name == fallback {
            return media_item_fallback_label(media_item_id);
        }
    }

    let fallback = format!("Schedule Item #{}", item.id);
    if item.collection_name == fallback {
        return schedule_item_fallback_label(item.id);
    }

    item.collection_name.clone()
}

fn list_row_from_item(item: &ScheduleItemForm) -> ScheduleItemListRow {
    ScheduleItemListRow {
        id: item.id,
        index: item.index,
        start_type: item.start_type,
        start_time: item.start_time.clone(),
        collection_name: item.collection_name.clone(),
        playout_mode: item.playout_mode,
        multiple_mode: Some(item.multiple_mode),
        multiple_count: Some(item.multiple_count.clone()),
        duration: Some(format!(
            "{:02}:{:02}:00",
            item.duration_hours, item.duration_minutes
        )),
    }
}

fn checkbox_field(
    label: &'static str,
    caption: &'static str,
    name: &'static str,
    form: RwSignal<ScheduleForm>,
    get: fn(&ScheduleForm) -> bool,
    set: fn(&mut ScheduleForm, bool),
    disabled: bool,
) -> AnyView {
    view! {
        <label class=style::field>
            <span>{label}</span>
            <span>
                <label class=style::check>
                    <input
                        type="checkbox"
                        name=name
                        value="true"
                        disabled=disabled
                        checked=move || get(&form.get())
                        prop:checked=move || get(&form.get())
                        on:change=move |event| {
                            let value = event_target_checked(&event);
                            form.update(|form| set(form, value));
                        }
                    />
                    <span class=style::caption>{caption}</span>
                </label>
            </span>
        </label>
    }
    .into_any()
}
