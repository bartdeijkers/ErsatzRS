use crate::admin::ui::primitives::pagination::page_size_limit;
use leptos::prelude::*;
use leptos_router::hooks::{use_params_map, use_query_map};

use crate::admin::i18n::{delete_failed, filler_error_message, save_failed, tr, TextKey};
use crate::admin::server_fns::filler::{
    delete_filler_preset, get_filler_preset_editor_options, get_filler_preset_form,
    get_filler_presets_page_size, list_filler_presets, save_filler_preset,
    FillerPresetEditorOptions, FillerPresetForm, FillerPresetListRow, FillerPresetPlaylistOption,
    FillerPresetSelectOption,
};
use crate::admin::ui::primitives::page_header::{PageAction, PageHeader};
use crate::admin::ui::primitives::{
    dialog::DeleteDialog,
    icon::{icon_style, IconName},
    pagination::PaginationControls,
    resource_state::{admin_loading_resource_view, admin_result_view},
    toast::{load_or_notify, use_toast, ToastSeverity},
};

use crate::admin::ui::primitives::button::CancelLink;
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
    let initial = Resource::new_blocking(
        || (),
        |()| async {
            let page_size = get_filler_presets_page_size().await?;
            let rows = list_filler_presets(0, page_size).await?;
            Ok::<_, ServerFnError>((page_size, rows))
        },
    );

    view! {
        <Suspense fallback=admin_loading_resource_view>
            {move || {
                let error = error.clone();
                initial.get().map(move |result| admin_result_view(result, |(initial_page_size, (initial_rows, initial_total))| {
                    view! {
                        <FillerPresetsIsland
                            initial_error=error.clone()
                            initial_page_size=initial_page_size
                            initial_rows=initial_rows
                            initial_total=initial_total
                        />
                    }
                    .into_any()
                }))
            }}
        </Suspense>
    }
}

#[island]
fn FillerPresetsIsland(
    initial_error: String,
    initial_page_size: u32,
    initial_rows: Vec<FillerPresetListRow>,
    initial_total: u64,
) -> impl IntoView {
    view! {
        <FillerPresetsList
            initial_error=initial_error
            initial_page_size=initial_page_size
            initial_rows=initial_rows
            initial_total=initial_total
        />
    }
}

#[component]
fn FillerPresetsList(
    initial_error: String,
    initial_page_size: u32,
    initial_rows: Vec<FillerPresetListRow>,
    initial_total: u64,
) -> impl IntoView {
    let page = RwSignal::new(0u32);
    let page_size = RwSignal::new(initial_page_size);
    let current_data = RwSignal::new((initial_rows, initial_total));
    let toast = use_toast();

    let data: LocalResource<(Vec<FillerPresetListRow>, u64)> = LocalResource::new(move || {
        let page = page.get();
        let page_size = page_size.get();
        async move { load_or_notify(list_filler_presets(page, page_size).await, toast) }
    });
    Effect::new(move |_| {
        if let Some(next_data) = data.get() {
            current_data.set(next_data);
        }
    });
    let delete_action = Action::new(|id: &i64| delete_filler_preset(*id));
    let delete_target: RwSignal<Option<FillerPresetListRow>> = RwSignal::new(None);
    let delete_open = RwSignal::new(false);
    Effect::new(move |_| {
        if let Some(result) = delete_action.value().get() {
            match result {
                Ok(()) => {
                    toast.push(tr(TextKey::FillerPresetDeleted), ToastSeverity::Success);
                    data.refetch();
                }
                Err(error) => toast.push(
                    delete_failed(filler_error_message(error)),
                    ToastSeverity::Error,
                ),
            }
            delete_open.set(false);
        }
    });

    view! {
        <div class=style::page>
            <section class=style::content>
                <PageHeader title=tr(TextKey::FillerPresets)>
                    <PageAction href="/media/filler/presets/add" label=tr(TextKey::AddFillerPreset)/>
                </PageHeader>
                <p class=style::empty style:display=move || if initial_error.is_empty() { "none" } else { "" }>
                    {tr(TextKey::FillerPresetActionFailed)}
                </p>

                {move || {
                    let (rows, total) = current_data.get();
                    view! {
                        <div class=style::table_wrap>
                            <table>
                                <thead>
                                    <tr>
                                        <th>{tr(TextKey::Name)}</th>
                                        <th>{tr(TextKey::FillerKind)}</th>
                                        <th/>
                                    </tr>
                                </thead>
                                <tbody>
                                    {if rows.is_empty() {
                                        view! {
                                            <tr>
                                                <td class=style::empty_cell colspan="3">{tr(TextKey::NoFillerPresets)}</td>
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
                                                                title=tr(TextKey::EditFillerPreset)
                                                                aria-label=tr(TextKey::EditFillerPreset)
                                                            >
                                                                <span style=icon_style(IconName::Edit, "1.25rem") aria-hidden="true"></span>
                                                            </a>
                                                            <form method="post" action="/admin-api/filler-presets/delete">
                                                                <input type="hidden" name="id" value=id.to_string()/>
                                                                <button
                                                                    type="submit"
                                                                    class=style::icon_button
                                                                    title=tr(TextKey::DeleteFillerPreset)
                                                                    aria-label=tr(TextKey::DeleteFillerPreset)
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
                        <PaginationControls
                            page=Signal::derive(move || page.get())
                            total=Signal::derive(move || Some(total))
                            page_size=Signal::derive(move || page_size.get())
                            page_size_options=Some(vec![10, 25, 50, 100])
                            on_page_size_change=Callback::new(move |value: u32| {
                                page_size.set(value);
                                page.set(0);
                            })
                            on_previous=Callback::new(move |()| {
                                page.update(|value| *value = value.saturating_sub(1));
                            })
                            on_next=Callback::new(move |()| {
                                let size = page_size_limit(page_size.get());
                                let max_page = u32::try_from(total.div_ceil(size))
                                    .unwrap_or(u32::MAX)
                                    .saturating_sub(1);
                                page.update(|value| *value = value.saturating_add(1).min(max_page));
                            })
                        />
                    }
                    .into_any()
                }}
            </section>
            <DeleteDialog
                dialog_id="filler-preset-delete-dialog"
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
    let new_filler_preset_name = tr(TextKey::NewFillerPreset).to_owned();
    let initial = Resource::new_blocking(
        move || (id, new_filler_preset_name.clone()),
        move |(id, new_filler_preset_name)| async move {
            let form = if id == 0 {
                FillerPresetForm {
                    name: new_filler_preset_name,
                    ..FillerPresetForm::default()
                }
            } else {
                get_filler_preset_form(id).await?.unwrap_or_default()
            };
            let options = get_filler_preset_editor_options().await?;
            Ok::<_, ServerFnError>((form, options))
        },
    );

    view! {
        <Suspense fallback=admin_loading_resource_view>
            {move || {
                let error = error.clone();
                initial.get().map(move |result| admin_result_view(result, |(initial_form, editor_options)| {
                    view! {
                        <FillerPresetEditorIsland
                            id=id
                            initial_error=error.clone()
                            initial_form=initial_form
                            editor_options=editor_options
                        />
                    }
                    .into_any()
                }))
            }}
        </Suspense>
    }
}

#[island]
fn FillerPresetEditorIsland(
    id: i64,
    initial_error: String,
    initial_form: FillerPresetForm,
    editor_options: FillerPresetEditorOptions,
) -> impl IntoView {
    view! {
        <FillerPresetEditorForm
            id=id
            initial_error=initial_error
            initial_form=initial_form
            editor_options=editor_options
        />
    }
}

#[component]
fn FillerPresetEditorForm(
    id: i64,
    initial_error: String,
    initial_form: FillerPresetForm,
    editor_options: FillerPresetEditorOptions,
) -> impl IntoView {
    let form_id = RwSignal::new(initial_form.id);
    let name = RwSignal::new(initial_form.name);
    let filler_kind = RwSignal::new(initial_form.filler_kind);
    let filler_mode = RwSignal::new(initial_form.filler_mode);
    let duration_hours = RwSignal::new(initial_form.duration_hours);
    let duration_minutes = RwSignal::new(initial_form.duration_minutes);
    let duration_seconds = RwSignal::new(initial_form.duration_seconds);
    let count = RwSignal::new(
        initial_form
            .count
            .map(|value| value.to_string())
            .unwrap_or_default(),
    );
    let pad_to_nearest_minute = RwSignal::new(
        initial_form
            .pad_to_nearest_minute
            .map(|value| value.to_string())
            .unwrap_or_default(),
    );
    let allow_watermarks = RwSignal::new(initial_form.allow_watermarks);
    let collection_type = RwSignal::new(initial_form.collection_type);
    let collection_id = RwSignal::new(
        initial_form
            .collection_id
            .map(|value| value.to_string())
            .unwrap_or_default(),
    );
    let media_item_id = RwSignal::new(
        initial_form
            .media_item_id
            .map(|value| value.to_string())
            .unwrap_or_default(),
    );
    let multi_collection_id = RwSignal::new(
        initial_form
            .multi_collection_id
            .map(|value| value.to_string())
            .unwrap_or_default(),
    );
    let smart_collection_id = RwSignal::new(
        initial_form
            .smart_collection_id
            .map(|value| value.to_string())
            .unwrap_or_default(),
    );
    let playlist_group_id = RwSignal::new(String::new());
    let playlist_id = RwSignal::new(
        initial_form
            .playlist_id
            .map(|value| value.to_string())
            .unwrap_or_default(),
    );
    let expression = RwSignal::new(initial_form.expression);
    let use_chapters_as_media_items = RwSignal::new(initial_form.use_chapters_as_media_items);
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
    let options_for_group = editor_options.clone();
    Effect::new(move |_| {
        if !playlist_group_id.get().is_empty() {
            return;
        }
        let Some(playlist_id) = parse_optional_i64(&playlist_id.get()) else {
            return;
        };
        if let Some(playlist) = options_for_group
            .playlists
            .iter()
            .find(|playlist| playlist.id == playlist_id)
        {
            playlist_group_id.set(playlist.playlist_group_id.to_string());
        }
    });
    let options_for_playlist = editor_options.clone();
    Effect::new(move |_| {
        let group_id = parse_optional_i64(&playlist_group_id.get()).unwrap_or(0);
        let Some(selected_playlist_id) = parse_optional_i64(&playlist_id.get()) else {
            return;
        };
        let playlist_matches_group = options_for_playlist.playlists.iter().any(|playlist| {
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
                    toast.push(tr(TextKey::FillerPresetSaved), ToastSeverity::Success);
                }
                Err(error) => toast.push(
                    save_failed(filler_error_message(error)),
                    ToastSeverity::Error,
                ),
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
    let page_title = tr(TextKey::FillerPreset);
    let save_label = if is_edit {
        tr(TextKey::SaveFillerPreset)
    } else {
        tr(TextKey::AddFillerPreset)
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

    let opts = editor_options.clone();

    view! {
                    <div class=style::page>
                        <p class=style::empty style:display=move || if has_initial_error { "" } else { "none" }>
                            {tr(TextKey::FillerPresetCouldNotBeSaved)}
                        </p>
                        <form
                            method="post"
                            action="/admin-api/filler-presets/save"
                            on:submit=move |event| on_save.run(event)
                        >
                            <input type="hidden" name="id" value=move || form_id.get().to_string()/>
                            <div class=style::editor_action_bar>
                                <button type="submit" class=style::btn_add>
                                    <span aria-hidden="true">"+"</span>
                                    {save_label}
                                </button>
                                <CancelLink href="/media/filler/presets"/>
                            </div>
                            <div class=style::editor>
                                <h1 class=style::page_title>{page_title}</h1>
                                {text_input(tr(TextKey::Name), "name", name, Signal::derive(|| false))}
                                {select_i32(tr(TextKey::Kind), "filler_kind", filler_kind, vec![
                                    (FILLER_KIND_PRE_ROLL, tr(TextKey::PreRoll)),
                                    (FILLER_KIND_MID_ROLL, tr(TextKey::MidRoll)),
                                    (FILLER_KIND_POST_ROLL, tr(TextKey::PostRoll)),
                                    (FILLER_KIND_TAIL, tr(TextKey::Tail)),
                                    (FILLER_KIND_FALLBACK, tr(TextKey::Fallback)),
                                ], Signal::derive(move || is_edit))}
                                {select_i32(tr(TextKey::Mode), "filler_mode", filler_mode, vec![
                                    (FILLER_MODE_DURATION, tr(TextKey::Duration)),
                                    (FILLER_MODE_COUNT, tr(TextKey::Count)),
                                    (FILLER_MODE_PAD, tr(TextKey::Pad)),
                                    (FILLER_MODE_RANDOM_COUNT, tr(TextKey::RandomCount)),
                                ], mode_disabled)}
                                {number_input(tr(TextKey::Duration), "duration_hours", duration_hours, tr(TextKey::Hours), duration_disabled)}
                                {number_input("", "duration_minutes", duration_minutes, tr(TextKey::Minutes), duration_disabled)}
                                {number_input("", "duration_seconds", duration_seconds, tr(TextKey::Seconds), duration_disabled)}
                                {text_input(tr(TextKey::Count), "count", count, count_disabled)}
                                {select_string(tr(TextKey::PadToNearestMinute), "pad_to_nearest_minute", pad_to_nearest_minute, vec![
                                    ("5", tr(TextKey::PadNearestFiveMinutes)),
                                    ("10", tr(TextKey::PadNearestTenMinutes)),
                                    ("15", tr(TextKey::PadNearestFifteenMinutes)),
                                    ("30", tr(TextKey::PadNearestThirtyMinutes)),
                                ], pad_disabled)}
                                {checkbox_input(tr(TextKey::AllowWatermarks), "allow_watermarks", allow_watermarks, Signal::derive(|| false), None)}
                                {collection_type_select(tr(TextKey::CollectionType), "collection_type", collection_type, filler_kind)}
                                <div style:display=move || display_for(collection_type.get() == COLLECTION_TYPE_COLLECTION)>
                                    {option_select(tr(TextKey::Collection), "collection_id", collection_id, opts.collections.clone())}
                                </div>
                                <div style:display=move || display_for(collection_type.get() == COLLECTION_TYPE_MULTI_COLLECTION)>
                                    {option_select(tr(TextKey::MultiCollection), "multi_collection_id", multi_collection_id, opts.multi_collections.clone())}
                                </div>
                                <div style:display=move || display_for(collection_type.get() == COLLECTION_TYPE_SMART_COLLECTION)>
                                    {option_select(tr(TextKey::SmartCollection), "smart_collection_id", smart_collection_id, opts.smart_collections.clone())}
                                </div>
                                <div style:display=move || display_for(collection_type.get() == COLLECTION_TYPE_TELEVISION_SHOW)>
                                    {option_select(tr(TextKey::TelevisionShow), "media_item_id", media_item_id, opts.television_shows.clone())}
                                </div>
                                <div style:display=move || display_for(collection_type.get() == COLLECTION_TYPE_TELEVISION_SEASON)>
                                    {option_select(tr(TextKey::TelevisionSeason), "media_item_id", media_item_id, opts.television_seasons.clone())}
                                </div>
                                <div style:display=move || display_for(collection_type.get() == COLLECTION_TYPE_ARTIST)>
                                    {option_select(tr(TextKey::Artist), "media_item_id", media_item_id, opts.artists.clone())}
                                </div>
                                <div style:display=move || display_for(collection_type.get() == COLLECTION_TYPE_PLAYLIST)>
                                    {option_select(tr(TextKey::PlaylistGroup), "playlist_group_id", playlist_group_id, opts.playlist_groups.clone())}
                                    {playlist_select(tr(TextKey::Playlist), "playlist_id", playlist_id, playlist_group_id, opts.playlists.clone())}
                                </div>
                                {checkbox_input(
                                    tr(TextKey::UseChaptersAsMediaItems),
                                    "use_chapters_as_media_items",
                                    use_chapters_as_media_items,
                                    chapters_disabled,
                                    Some(tr(TextKey::UseChaptersAsMediaItemsHelp)),
                                )}
                                {text_input(tr(TextKey::Expression), "expression", expression, expression_disabled)}
                                <p class=style::expression_help>
                                    {tr(TextKey::FillerPresetExpressionHelp)}
                                    <br/><br/>
                                    <strong>"total_points"</strong>": " {tr(TextKey::TotalPointsHelp)}
                                    <br/>
                                    <strong>"matched_points"</strong>": " {tr(TextKey::MatchedPointsHelp)}
                                    <br/>
                                    <strong>"total_duration"</strong>": " {tr(TextKey::TotalDurationHelp)}
                                    <br/>
                                    <strong>"total_progress"</strong>": " {tr(TextKey::TotalProgressHelp)}
                                    <br/>
                                    <strong>"last_mid_filler"</strong>": " {tr(TextKey::LastMidFillerHelp)}
                                    <br/>
                                    <strong>"remaining_duration"</strong>": " {tr(TextKey::RemainingDurationHelp)}
                                    <br/>
                                    <strong>"point"</strong>": " {tr(TextKey::PointHelp)}
                                    <br/>
                                    <strong>"num"</strong>": " {tr(TextKey::NumHelp)}
                                </p>
                            </div>
                        </form>
                    </div>
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
                <option value=COLLECTION_TYPE_COLLECTION.to_string()>{tr(TextKey::Collection)}</option>
                <option value=COLLECTION_TYPE_TELEVISION_SHOW.to_string()>{tr(TextKey::TelevisionShow)}</option>
                <option value=COLLECTION_TYPE_TELEVISION_SEASON.to_string()>{tr(TextKey::TelevisionSeason)}</option>
                <option value=COLLECTION_TYPE_ARTIST.to_string()>{tr(TextKey::Artist)}</option>
                <option value=COLLECTION_TYPE_MULTI_COLLECTION.to_string()>{tr(TextKey::MultiCollection)}</option>
                <option value=COLLECTION_TYPE_SMART_COLLECTION.to_string()>{tr(TextKey::SmartCollection)}</option>
                <option
                    value=COLLECTION_TYPE_PLAYLIST.to_string()
                    disabled=move || matches!(
                        filler_kind.get(),
                        FILLER_KIND_FALLBACK | FILLER_KIND_TAIL
                    )
                    style:display=move || {
                        if matches!(filler_kind.get(), FILLER_KIND_FALLBACK | FILLER_KIND_TAIL) {
                            "none"
                        } else {
                            ""
                        }
                    }
                >
                    {tr(TextKey::Playlist)}
                </option>
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
        FILLER_KIND_PRE_ROLL => tr(TextKey::PreRoll),
        FILLER_KIND_MID_ROLL => tr(TextKey::MidRoll),
        FILLER_KIND_POST_ROLL => tr(TextKey::PostRoll),
        FILLER_KIND_FALLBACK => tr(TextKey::Fallback),
        FILLER_KIND_TAIL => tr(TextKey::Tail),
        _ => tr(TextKey::NoneOption),
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
