#![allow(clippy::too_many_lines, dead_code)]

use leptos::prelude::*;
use leptos_router::hooks::{use_params_map, use_query_map};

use crate::admin::server_fns::filler::{
    delete_filler_preset, get_filler_preset_editor_options, get_filler_preset_form,
    get_filler_presets_page_size, list_filler_presets, save_filler_preset,
    FillerPresetEditorOptions, FillerPresetForm, FillerPresetListRow, FillerPresetPlaylistOption,
    FillerPresetSelectOption,
};
use crate::admin::ui::primitives::{
    dialog::DeleteDialog,
    icon::{icon_style, IconName},
    toast::{use_toast, ToastSeverity},
};

stylance::import_crate_style!(style, "src/admin/pages/media/filler_presets.module.css");

const FILLER_KIND_PRE_ROLL: i32 = 1;
const FILLER_KIND_MID_ROLL: i32 = 2;
const FILLER_KIND_POST_ROLL: i32 = 3;
const FILLER_KIND_TAIL: i32 = 4;
const FILLER_KIND_FALLBACK: i32 = 5;
const FILLER_MODE_NONE: i32 = 0;
const FILLER_MODE_DURATION: i32 = 1;
const FILLER_MODE_COUNT: i32 = 2;
const FILLER_MODE_PAD: i32 = 3;
const FILLER_MODE_RANDOM_COUNT: i32 = 4;
const COLLECTION_TYPE_COLLECTION: i32 = 0;
const COLLECTION_TYPE_TELEVISION_SHOW: i32 = 1;
const COLLECTION_TYPE_TELEVISION_SEASON: i32 = 2;
const COLLECTION_TYPE_ARTIST: i32 = 3;
const COLLECTION_TYPE_MULTI_COLLECTION: i32 = 4;
const COLLECTION_TYPE_SMART_COLLECTION: i32 = 5;
const COLLECTION_TYPE_PLAYLIST: i32 = 6;

#[component]
pub fn FillerPresetsPage() -> impl IntoView {
    let query = use_query_map();
    let error = query.with_untracked(|params| params.get("error").unwrap_or_default());
    view! { <FillerPresetsIsland initial_error=error/> }
}

#[island]
fn FillerPresetsIsland(initial_error: String) -> impl IntoView {
    let page = RwSignal::new(0u32);
    let page_size = RwSignal::new(10u32);

    let saved_page_size = Resource::new_blocking(
        || (),
        |()| async { get_filler_presets_page_size().await.unwrap_or(10) },
    );
    Effect::new(move |_| {
        if let Some(saved) = saved_page_size.get() {
            page_size.set(saved);
        }
    });

    let data: Resource<(Vec<FillerPresetListRow>, u64)> = Resource::new_blocking(
        move || (page.get(), page_size.get()),
        move |(page, page_size)| async move {
            list_filler_presets(page, page_size)
                .await
                .unwrap_or_default()
        },
    );
    let delete_action = Action::new(|id: &i64| delete_filler_preset(*id));
    let delete_target: RwSignal<Option<FillerPresetListRow>> = RwSignal::new(None);
    let delete_open = RwSignal::new(false);
    let toast = use_toast();
    Effect::new(move |_| {
        if let Some(result) = delete_action.value().get() {
            match result {
                Ok(()) => {
                    toast.push("Filler preset deleted.", ToastSeverity::Success);
                    data.refetch();
                }
                Err(error) => toast.push(format!("Delete failed: {error}"), ToastSeverity::Error),
            }
            delete_open.set(false);
        }
    });

    view! {
        <div class=style::page>
            <div class=style::action_bar>
                <a href="/media/filler/presets/add" class=style::btn_add>
                    <span style=icon_style(IconName::Add, "1.125rem") aria-hidden="true"></span>
                    <span>"Add Filler Preset"</span>
                </a>
            </div>
            <section class=style::content>
                <h1 class=style::page_title>"Filler Presets"</h1>
                <div class=style::divider/>
                <p class=style::empty style:display=move || if initial_error.is_empty() { "none" } else { "" }>
                    "Filler preset action failed."
                </p>

                <Suspense fallback=move || view! { <p class=style::empty>"Loading..."</p> }>
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
                                        <thead>
                                            <tr>
                                                <th>"Name"</th>
                                                <th>"Filler Kind"</th>
                                                <th/>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            {if rows.is_empty() {
                                                view! {
                                                    <tr>
                                                        <td class=style::empty_cell colspan="3">"No filler presets yet."</td>
                                                    </tr>
                                                }.into_any()
                                            } else {
                                                rows.into_iter().map(|row| {
                                                    let id = row.id;
                                                    let name = row.name.clone();
                                                    let row_for_delete = row.clone();
                                                    view! {
                                                        <tr>
                                                            <td>{name}</td>
                                                            <td>{filler_kind_label(row.filler_kind)}</td>
                                                            <td>
                                                                <div class=style::row_actions>
                                                                    <a
                                                                        class=style::icon_button
                                                                        href=format!("/media/filler/presets/{id}/edit")
                                                                        title="Edit Filler Preset"
                                                                        aria-label="Edit Filler Preset"
                                                                    >
                                                                        <span style=icon_style(IconName::Edit, "1.25rem") aria-hidden="true"></span>
                                                                    </a>
                                                                    <form method="post" action="/api/filler-presets/delete">
                                                                        <input type="hidden" name="id" value=id.to_string()/>
                                                                        <button
                                                                            type="submit"
                                                                            class=style::icon_button
                                                                            title="Delete Filler Preset"
                                                                            aria-label="Delete Filler Preset"
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
                                    <span>"Rows per page:"</span>
                                    <select
                                        class=style::page_size
                                        aria-label="Rows per page"
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
                                    <span class=style::pager_status>
                                        {format!("{range_start}-{range_end} of {total}")}
                                    </span>
                                    <div class=style::pager_buttons>
                                        <button
                                            type="button"
                                            class=style::icon_button
                                            disabled=move || page.get() == 0
                                            title="Previous page"
                                            aria-label="Previous page"
                                            on:click=move |_| page.update(|page| *page = page.saturating_sub(1))
                                        >
                                            <span style=icon_style(IconName::ChevronLeft, "1.25rem") aria-hidden="true"></span>
                                        </button>
                                        <button
                                            type="button"
                                            class=style::icon_button
                                            disabled=move || { page.get() + 1 >= total_pages }
                                            title="Next page"
                                            aria-label="Next page"
                                            on:click=move |_| {
                                                page.update(|page| {
                                                    *page = (*page + 1).min(total_pages.saturating_sub(1));
                                                });
                                            }
                                        >
                                            <span style=icon_style(IconName::ChevronRight, "1.25rem") aria-hidden="true"></span>
                                        </button>
                                    </div>
                                </div>
                            }
                            .into_any()
                        })
                    }}
                </Suspense>
            </section>
            <DeleteDialog
                open=delete_open
                entity_name=Signal::derive(move || {
                    delete_target.get().map(|row| row.name).unwrap_or_default()
                })
                on_confirm=Callback::new(move |confirmed: bool| {
                    if confirmed {
                        if let Some(row) = delete_target.get_untracked() {
                            delete_action.dispatch(row.id);
                        } else {
                            delete_open.set(false);
                        }
                    } else {
                        delete_open.set(false);
                    }
                })
            />
        </div>
    }
}

#[component]
pub fn FillerPresetEditorPage() -> impl IntoView {
    let params = use_params_map();
    let query = use_query_map();
    let id = params.with(|p| p.get("id").and_then(|s| s.parse::<i64>().ok()).unwrap_or(0));
    let error = query.with_untracked(|params| params.get("error").unwrap_or_default());
    view! { <FillerPresetEditorIsland id initial_error=error/> }
}

#[island]
fn FillerPresetEditorIsland(id: i64, initial_error: String) -> impl IntoView {
    let initial: Resource<FillerPresetForm> = Resource::new_blocking(
        move || id,
        |id| async move {
            if id == 0 {
                FillerPresetForm::default()
            } else {
                get_filler_preset_form(id)
                    .await
                    .unwrap_or_default()
                    .unwrap_or_default()
            }
        },
    );
    let options: Resource<FillerPresetEditorOptions> = Resource::new_blocking(
        || (),
        |()| async { get_filler_preset_editor_options().await.unwrap_or_default() },
    );

    let form_id = RwSignal::new(id);
    let name = RwSignal::new(String::new());
    let filler_kind = RwSignal::new(FILLER_KIND_PRE_ROLL);
    let filler_mode = RwSignal::new(FILLER_MODE_DURATION);
    let duration_hours = RwSignal::new(0_i32);
    let duration_minutes = RwSignal::new(0_i32);
    let duration_seconds = RwSignal::new(1_i32);
    let count = RwSignal::new(String::new());
    let pad_to_nearest_minute = RwSignal::new(String::new());
    let allow_watermarks = RwSignal::new(false);
    let collection_type = RwSignal::new(COLLECTION_TYPE_COLLECTION);
    let collection_id = RwSignal::new(String::new());
    let media_item_id = RwSignal::new(String::new());
    let multi_collection_id = RwSignal::new(String::new());
    let smart_collection_id = RwSignal::new(String::new());
    let playlist_group_id = RwSignal::new(String::new());
    let playlist_id = RwSignal::new(String::new());
    let expression = RwSignal::new(String::new());
    let use_chapters_as_media_items = RwSignal::new(false);

    Effect::new(move |_| {
        if let Some(form) = initial.get() {
            form_id.set(form.id);
            name.set(form.name);
            filler_kind.set(form.filler_kind);
            filler_mode.set(form.filler_mode);
            duration_hours.set(form.duration_hours);
            duration_minutes.set(form.duration_minutes);
            duration_seconds.set(form.duration_seconds);
            count.set(
                form.count
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
            );
            pad_to_nearest_minute.set(
                form.pad_to_nearest_minute
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
            );
            allow_watermarks.set(form.allow_watermarks);
            collection_type.set(form.collection_type);
            collection_id.set(
                form.collection_id
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
            );
            media_item_id.set(
                form.media_item_id
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
            );
            multi_collection_id.set(
                form.multi_collection_id
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
            );
            smart_collection_id.set(
                form.smart_collection_id
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
            );
            playlist_id.set(
                form.playlist_id
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
            );
            expression.set(form.expression);
            use_chapters_as_media_items.set(form.use_chapters_as_media_items);
        }
    });
    Effect::new(move |_| {
        let kind = filler_kind.get();
        if matches!(kind, FILLER_KIND_FALLBACK | FILLER_KIND_TAIL)
            && filler_mode.get_untracked() != FILLER_MODE_NONE
        {
            filler_mode.set(FILLER_MODE_NONE);
        }
        if kind != FILLER_KIND_MID_ROLL && !expression.get_untracked().is_empty() {
            expression.set(String::new());
        }
        if kind == FILLER_KIND_FALLBACK && use_chapters_as_media_items.get_untracked() {
            use_chapters_as_media_items.set(false);
        }
        if matches!(kind, FILLER_KIND_FALLBACK | FILLER_KIND_TAIL)
            && collection_type.get_untracked() == COLLECTION_TYPE_PLAYLIST
        {
            collection_type.set(COLLECTION_TYPE_COLLECTION);
        }
    });
    Effect::new(move |_| {
        if collection_type.get() == COLLECTION_TYPE_PLAYLIST
            && filler_mode.get_untracked() != FILLER_MODE_COUNT
        {
            filler_mode.set(FILLER_MODE_COUNT);
        }
    });
    Effect::new(move |_| {
        if !playlist_group_id.get().is_empty() {
            return;
        }
        let Some(options) = options.get() else {
            return;
        };
        let Some(playlist_id) = parse_optional_i64(&playlist_id.get()) else {
            return;
        };
        if let Some(playlist) = options
            .playlists
            .iter()
            .find(|playlist| playlist.id == playlist_id)
        {
            playlist_group_id.set(playlist.playlist_group_id.to_string());
        }
    });
    Effect::new(move |_| {
        let group_id = parse_optional_i64(&playlist_group_id.get()).unwrap_or(0);
        let Some(selected_playlist_id) = parse_optional_i64(&playlist_id.get()) else {
            return;
        };
        let Some(options) = options.get() else {
            return;
        };
        let playlist_matches_group = options.playlists.iter().any(|playlist| {
            playlist.id == selected_playlist_id && playlist.playlist_group_id == group_id
        });
        if !playlist_matches_group {
            playlist_id.set(String::new());
        }
    });

    let save_action = Action::new(move |form: &FillerPresetForm| {
        let form = form.clone();
        async move { save_filler_preset(form).await }
    });
    let toast = use_toast();
    Effect::new(move |_| {
        if let Some(result) = save_action.value().get() {
            match result {
                Ok(_) => {
                    #[cfg(target_arch = "wasm32")]
                    if let Some(window) = web_sys::window() {
                        let _ = window.location().set_href("/media/filler/presets");
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    toast.push("Filler preset saved.", ToastSeverity::Success);
                }
                Err(error) => toast.push(format!("Save failed: {error}"), ToastSeverity::Error),
            }
        }
    });

    let on_save = Callback::new(move |event: leptos::ev::SubmitEvent| {
        event.prevent_default();
        save_action.dispatch(FillerPresetForm {
            id: form_id.get_untracked(),
            name: name.get_untracked(),
            filler_kind: filler_kind.get_untracked(),
            filler_mode: effective_filler_mode(
                filler_kind.get_untracked(),
                collection_type.get_untracked(),
                filler_mode.get_untracked(),
            ),
            duration_hours: duration_hours.get_untracked(),
            duration_minutes: duration_minutes.get_untracked(),
            duration_seconds: duration_seconds.get_untracked(),
            count: parse_optional_i32(&count.get_untracked()),
            pad_to_nearest_minute: parse_optional_i32(&pad_to_nearest_minute.get_untracked()),
            allow_watermarks: allow_watermarks.get_untracked(),
            collection_type: collection_type.get_untracked(),
            collection_id: parse_optional_i64(&collection_id.get_untracked()),
            media_item_id: parse_optional_i64(&media_item_id.get_untracked()),
            multi_collection_id: parse_optional_i64(&multi_collection_id.get_untracked()),
            smart_collection_id: parse_optional_i64(&smart_collection_id.get_untracked()),
            playlist_id: parse_optional_i64(&playlist_id.get_untracked()),
            expression: expression.get_untracked(),
            use_chapters_as_media_items: use_chapters_as_media_items.get_untracked(),
        });
    });

    let is_edit = id != 0;
    let page_title = "Filler Preset";
    let save_label = if is_edit {
        "Save Filler Preset"
    } else {
        "Add Filler Preset"
    };
    let has_initial_error = !initial_error.is_empty();
    let mode_disabled = Signal::derive(move || {
        matches!(filler_kind.get(), FILLER_KIND_FALLBACK | FILLER_KIND_TAIL)
            || collection_type.get() == COLLECTION_TYPE_PLAYLIST
    });
    let duration_disabled = Signal::derive(move || filler_mode.get() != FILLER_MODE_DURATION);
    let count_disabled = Signal::derive(move || {
        !matches!(
            filler_mode.get(),
            FILLER_MODE_COUNT | FILLER_MODE_RANDOM_COUNT
        )
    });
    let pad_disabled = Signal::derive(move || filler_mode.get() != FILLER_MODE_PAD);
    let chapters_disabled = Signal::derive(move || filler_kind.get() == FILLER_KIND_FALLBACK);
    let expression_disabled = Signal::derive(move || filler_kind.get() != FILLER_KIND_MID_ROLL);

    view! {
        <Suspense fallback=move || view! { <p class=style::empty>"Loading..."</p> }>
            {move || {
                let opts = options.get().unwrap_or_default();
                let on_save = on_save;
                view! {
                    <div class=style::page>
                        <p class=style::empty style:display=move || if has_initial_error { "" } else { "none" }>
                            "Filler preset could not be saved. Check the form values and try again."
                        </p>
                        <form
                            method="post"
                            action="/api/filler-presets/save"
                            on:submit=move |event| on_save.run(event)
                        >
                            <input type="hidden" name="id" value=move || form_id.get().to_string()/>
                            <div class=style::editor_action_bar>
                                <button type="submit" class=style::btn_add>
                                    <span aria-hidden="true">"+"</span>
                                    {save_label.to_uppercase()}
                                </button>
                            </div>
                            <div class=style::editor>
                                <h1 class=style::page_title>{page_title}</h1>
                                {text_input("Name", "name", name, Signal::derive(|| false))}
                                {select_i32("Kind", "filler_kind", filler_kind, vec![
                                    (FILLER_KIND_PRE_ROLL, "Pre-Roll"),
                                    (FILLER_KIND_MID_ROLL, "Mid-Roll"),
                                    (FILLER_KIND_POST_ROLL, "Post-Roll"),
                                    (FILLER_KIND_TAIL, "Tail"),
                                    (FILLER_KIND_FALLBACK, "Fallback"),
                                ], Signal::derive(move || is_edit))}
                                {select_i32("Mode", "filler_mode", filler_mode, vec![
                                    (FILLER_MODE_DURATION, "Duration"),
                                    (FILLER_MODE_COUNT, "Count"),
                                    (FILLER_MODE_PAD, "Pad"),
                                    (FILLER_MODE_RANDOM_COUNT, "Random Count"),
                                ], mode_disabled)}
                                {number_input("Duration", "duration_hours", duration_hours, "hours", duration_disabled)}
                                {number_input("", "duration_minutes", duration_minutes, "minutes", duration_disabled)}
                                {number_input("", "duration_seconds", duration_seconds, "seconds", duration_disabled)}
                                {text_input("Count", "count", count, count_disabled)}
                                {select_string("Pad To Nearest Minute", "pad_to_nearest_minute", pad_to_nearest_minute, vec![
                                    ("5", "5 (:00, :05, :10, :15, :20, etc)"),
                                    ("10", "10 (:00, :10, :20, :30, :40, :50)"),
                                    ("15", "15 (:00, :15, :30, :45)"),
                                    ("30", "30 (:00, :30)"),
                                ], pad_disabled)}
                                {checkbox_input("Allow Watermarks", "allow_watermarks", allow_watermarks, Signal::derive(|| false), None)}
                                {collection_type_select("Collection Type", "collection_type", collection_type, filler_kind)}
                                <div style:display=move || display_for(collection_type.get() == COLLECTION_TYPE_COLLECTION)>
                                    {option_select("Collection", "collection_id", collection_id, opts.collections.clone())}
                                </div>
                                <div style:display=move || display_for(collection_type.get() == COLLECTION_TYPE_MULTI_COLLECTION)>
                                    {option_select("Multi Collection", "multi_collection_id", multi_collection_id, opts.multi_collections.clone())}
                                </div>
                                <div style:display=move || display_for(collection_type.get() == COLLECTION_TYPE_SMART_COLLECTION)>
                                    {option_select("Smart Collection", "smart_collection_id", smart_collection_id, opts.smart_collections.clone())}
                                </div>
                                <div style:display=move || display_for(collection_type.get() == COLLECTION_TYPE_TELEVISION_SHOW)>
                                    {option_select("Television Show", "media_item_id", media_item_id, opts.television_shows.clone())}
                                </div>
                                <div style:display=move || display_for(collection_type.get() == COLLECTION_TYPE_TELEVISION_SEASON)>
                                    {option_select("Television Season", "media_item_id", media_item_id, opts.television_seasons.clone())}
                                </div>
                                <div style:display=move || display_for(collection_type.get() == COLLECTION_TYPE_ARTIST)>
                                    {option_select("Artist", "media_item_id", media_item_id, opts.artists.clone())}
                                </div>
                                <div style:display=move || display_for(collection_type.get() == COLLECTION_TYPE_PLAYLIST)>
                                    {option_select("Playlist Group", "playlist_group_id", playlist_group_id, opts.playlist_groups.clone())}
                                    {playlist_select("Playlist", "playlist_id", playlist_id, playlist_group_id, opts.playlists.clone())}
                                </div>
                                {checkbox_input(
                                    "Use Chapters As Media Items",
                                    "use_chapters_as_media_items",
                                    use_chapters_as_media_items,
                                    chapters_disabled,
                                    Some("Schedule individual chapters instead of entire files"),
                                )}
                                {text_input("Expression", "expression", expression, expression_disabled)}
                                <p class=style::expression_help>
                                    "For mid-roll filler, only add filler when this expression evaluates to true for a given mid-roll point."
                                    <br/><br/>
                                    <strong>"total_points"</strong>": total number of potential mid-roll points"
                                    <br/>
                                    <strong>"matched_points"</strong>": number of mid-roll points that have already matched the expression"
                                    <br/>
                                    <strong>"total_duration"</strong>": total duration of the content, in seconds"
                                    <br/>
                                    <strong>"total_progress"</strong>": normalized position from 0 to 1"
                                    <br/>
                                    <strong>"last_mid_filler"</strong>": seconds since last mid-roll filler"
                                    <br/>
                                    <strong>"remaining_duration"</strong>": duration of the content after this mid-roll point, in seconds"
                                    <br/>
                                    <strong>"point"</strong>": the position of the mid-roll point, in seconds"
                                    <br/>
                                    <strong>"num"</strong>": the mid-roll point number, starting with 1"
                                </p>
                            </div>
                        </form>
                    </div>
                }
            }}
        </Suspense>
    }
}

fn text_input(
    label: &'static str,
    name_attr: &'static str,
    value: RwSignal<String>,
    disabled: Signal<bool>,
) -> impl IntoView {
    view! {
        <label>
            <span>{label}</span>
            <input
                name=name_attr
                disabled=move || disabled.get()
                prop:value=move || value.get()
                on:input=move |event| value.set(event_target_value(&event))
            />
        </label>
    }
}

fn number_input(
    label: &'static str,
    name_attr: &'static str,
    value: RwSignal<i32>,
    suffix: &'static str,
    disabled: Signal<bool>,
) -> impl IntoView {
    view! {
        <label>
            <span>{label}</span>
            <div class=style::input_with_suffix>
                <input
                    type="number"
                    name=name_attr
                    disabled=move || disabled.get()
                    prop:value=move || value.get().to_string()
                    on:input=move |event| {
                        if let Ok(parsed) = event_target_value(&event).parse::<i32>() {
                            value.set(parsed);
                        }
                    }
                />
                <small>{suffix}</small>
            </div>
        </label>
    }
}

fn checkbox_input(
    label: &'static str,
    name_attr: &'static str,
    value: RwSignal<bool>,
    disabled: Signal<bool>,
    description: Option<&'static str>,
) -> impl IntoView {
    view! {
        <label>
            <span>{label}</span>
            <div class=style::checkbox_row>
                <input
                    type="checkbox"
                    name=name_attr
                    value="true"
                    prop:checked=move || value.get()
                    disabled=move || disabled.get()
                    on:change=move |event| value.set(event_target_checked(&event))
                />
                {description.map(|description| view! { <small>{description}</small> })}
            </div>
        </label>
    }
}

fn select_i32(
    label: &'static str,
    name_attr: &'static str,
    value: RwSignal<i32>,
    options: Vec<(i32, &'static str)>,
    disabled: Signal<bool>,
) -> impl IntoView {
    view! {
        <label>
            <span>{label}</span>
            <select
                name=name_attr
                disabled=move || disabled.get()
                prop:value=move || value.get().to_string()
                on:change=move |event| {
                    if let Ok(parsed) = event_target_value(&event).parse::<i32>() {
                        value.set(parsed);
                    }
                }
            >
                {options.into_iter().map(|(id, label)| view! {
                    <option value=id.to_string()>{label}</option>
                }).collect_view()}
            </select>
        </label>
    }
}

fn collection_type_select(
    label: &'static str,
    name_attr: &'static str,
    value: RwSignal<i32>,
    filler_kind: RwSignal<i32>,
) -> impl IntoView {
    view! {
        <label>
            <span>{label}</span>
            <select
                name=name_attr
                prop:value=move || value.get().to_string()
                on:change=move |event| {
                    if let Ok(parsed) = event_target_value(&event).parse::<i32>() {
                        value.set(parsed);
                    }
                }
            >
                <option value=COLLECTION_TYPE_COLLECTION.to_string()>"Collection"</option>
                <option value=COLLECTION_TYPE_TELEVISION_SHOW.to_string()>"Television Show"</option>
                <option value=COLLECTION_TYPE_TELEVISION_SEASON.to_string()>"Television Season"</option>
                <option value=COLLECTION_TYPE_ARTIST.to_string()>"Artist"</option>
                <option value=COLLECTION_TYPE_MULTI_COLLECTION.to_string()>"Multi Collection"</option>
                <option value=COLLECTION_TYPE_SMART_COLLECTION.to_string()>"Smart Collection"</option>
                {move || {
                    (!matches!(
                        filler_kind.get(),
                        FILLER_KIND_FALLBACK | FILLER_KIND_TAIL
                    ))
                    .then(|| {
                        view! {
                            <option value=COLLECTION_TYPE_PLAYLIST.to_string()>"Playlist"</option>
                        }
                    })
                }}
            </select>
        </label>
    }
}

fn select_string(
    label: &'static str,
    name_attr: &'static str,
    value: RwSignal<String>,
    options: Vec<(&'static str, &'static str)>,
    disabled: Signal<bool>,
) -> impl IntoView {
    view! {
        <label>
            <span>{label}</span>
            <select
                name=name_attr
                disabled=move || disabled.get()
                prop:value=move || value.get()
                on:change=move |event| value.set(event_target_value(&event))
            >
                <option value="">""</option>
                {options.into_iter().map(|(id, label)| view! {
                    <option value=id>{label}</option>
                }).collect_view()}
            </select>
        </label>
    }
}

fn option_select(
    label: &'static str,
    name_attr: &'static str,
    value: RwSignal<String>,
    options: Vec<FillerPresetSelectOption>,
) -> impl IntoView {
    view! {
        <label>
            <span>{label}</span>
            <select
                name=name_attr
                prop:value=move || value.get()
                on:change=move |event| value.set(event_target_value(&event))
            >
                <option value="">""</option>
                {options.into_iter().map(|option| view! {
                    <option value=option.id.to_string()>{option.name}</option>
                }).collect_view()}
            </select>
        </label>
    }
}

fn playlist_select(
    label: &'static str,
    name_attr: &'static str,
    value: RwSignal<String>,
    playlist_group_id: RwSignal<String>,
    options: Vec<FillerPresetPlaylistOption>,
) -> impl IntoView {
    view! {
        <label>
            <span>{label}</span>
            <select
                name=name_attr
                prop:value=move || value.get()
                on:change=move |event| value.set(event_target_value(&event))
            >
                <option value="">""</option>
                {move || {
                    let group_id = parse_optional_i64(&playlist_group_id.get()).unwrap_or(0);
                    options
                        .clone()
                        .into_iter()
                        .filter(move |option| group_id == 0 || option.playlist_group_id == group_id)
                        .map(|option| view! {
                            <option value=option.id.to_string()>{option.name}</option>
                        })
                        .collect_view()
                }}
            </select>
        </label>
    }
}

fn filler_kind_label(kind: i32) -> &'static str {
    match kind {
        FILLER_KIND_PRE_ROLL => "Pre-Roll",
        FILLER_KIND_MID_ROLL => "Mid-Roll",
        FILLER_KIND_POST_ROLL => "Post-Roll",
        FILLER_KIND_FALLBACK => "Fallback",
        FILLER_KIND_TAIL => "Tail",
        _ => "None",
    }
}

fn effective_filler_mode(filler_kind: i32, collection_type: i32, filler_mode: i32) -> i32 {
    if matches!(filler_kind, FILLER_KIND_FALLBACK | FILLER_KIND_TAIL) {
        FILLER_MODE_NONE
    } else if collection_type == COLLECTION_TYPE_PLAYLIST {
        FILLER_MODE_COUNT
    } else {
        filler_mode
    }
}

fn display_for(show: bool) -> &'static str {
    if show {
        ""
    } else {
        "none"
    }
}

fn parse_optional_i32(value: &str) -> Option<i32> {
    value.trim().parse().ok()
}

fn parse_optional_i64(value: &str) -> Option<i64> {
    value.trim().parse::<i64>().ok().filter(|value| *value != 0)
}
