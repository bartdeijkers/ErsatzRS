//! Rerun collection list and editor. Ports `Pages/RerunCollections.razor`
//! and `Pages/RerunCollectionEditor.razor`.

use leptos::prelude::*;
use leptos_router::hooks::{use_params_map, use_query_map};

use crate::admin::i18n::{delete_failed, media_browser_error_message, save_failed, tr, TextKey};
use crate::admin::server_fns::media_browser::{
    delete_rerun_collection, get_rerun_collection_editor_options, get_rerun_collection_form,
    get_rerun_collections_page_size, list_rerun_collections, save_rerun_collection,
    RerunCollectionEditorOptions, RerunCollectionForm, RerunCollectionListRow,
    RerunCollectionSelectOption,
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
use crate::admin::ui::primitives::empty_state::EmptyState;
stylance::import_crate_style!(style, "src/admin/pages/media/rerun_collections.module.css");

const COLLECTION_TYPE_COLLECTION: i32 = 0;
const COLLECTION_TYPE_TELEVISION_SHOW: i32 = 1;
const COLLECTION_TYPE_TELEVISION_SEASON: i32 = 2;
const COLLECTION_TYPE_ARTIST: i32 = 3;
const COLLECTION_TYPE_MULTI_COLLECTION: i32 = 4;
const COLLECTION_TYPE_SMART_COLLECTION: i32 = 5;
const COLLECTION_TYPE_PLAYLIST: i32 = 6;
const PLAYBACK_ORDER_CHRONOLOGICAL: i32 = 1;
const PLAYBACK_ORDER_RANDOM: i32 = 2;
const PLAYBACK_ORDER_SHUFFLE: i32 = 3;
const PLAYBACK_ORDER_SEASON_EPISODE: i32 = 6;

#[cfg(target_arch = "wasm32")]
fn set_browser_location(path: &str) {
    if let Some(window) = web_sys::window() {
        let _ = window.location().set_href(path);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn set_browser_location(_path: &str) {}

#[component]
pub fn RerunCollectionsPage() -> impl IntoView {
    let query = use_query_map();
    let error = query.with_untracked(|params| params.get("error").unwrap_or_default());
    let initial = Resource::new_blocking(
        || (),
        |()| async {
            let page_size = get_rerun_collections_page_size().await?;
            let rows = list_rerun_collections(String::new(), 0, page_size).await?;
            Ok::<_, ServerFnError>((page_size, rows))
        },
    );

    view! {
        <Suspense fallback=admin_loading_resource_view>
            {move || {
                let error = error.clone();
                initial.get().map(move |result| admin_result_view(result, |(initial_page_size, (initial_rows, initial_total))| {
                    view! {
                        <RerunCollectionsIsland
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
fn RerunCollectionsIsland(
    initial_error: String,
    initial_page_size: u32,
    initial_rows: Vec<RerunCollectionListRow>,
    initial_total: u64,
) -> impl IntoView {
    view! {
        <RerunCollectionsList
            initial_error=initial_error
            initial_page_size=initial_page_size
            initial_rows=initial_rows
            initial_total=initial_total
        />
    }
}

#[component]
fn RerunCollectionsList(
    initial_error: String,
    initial_page_size: u32,
    initial_rows: Vec<RerunCollectionListRow>,
    initial_total: u64,
) -> impl IntoView {
    let search = RwSignal::new(String::new());
    let page = RwSignal::new(0u32);
    let page_size = RwSignal::new(initial_page_size);
    let current_data = RwSignal::new((initial_rows, initial_total));
    let toast = use_toast();

    let data: LocalResource<(Vec<RerunCollectionListRow>, u64)> = LocalResource::new(move || {
        let search = search.get();
        let page = page.get();
        let page_size = page_size.get();
        async move { load_or_notify(list_rerun_collections(search, page, page_size).await, toast) }
    });
    Effect::new(move |_| {
        if let Some(next_data) = data.get() {
            current_data.set(next_data);
        }
    });
    let delete_action = Action::new(|id: &i64| delete_rerun_collection(*id));
    let delete_target: RwSignal<Option<RerunCollectionListRow>> = RwSignal::new(None);
    let delete_open = RwSignal::new(false);
    Effect::new(move |_| {
        if let Some(result) = delete_action.value().get() {
            match result {
                Ok(()) => {
                    toast.push(tr(TextKey::RerunCollectionDeleted), ToastSeverity::Success);
                    data.refetch();
                }
                Err(error) => toast.push(
                    delete_failed(media_browser_error_message(error)),
                    ToastSeverity::Error,
                ),
            }
            delete_open.set(false);
        }
    });

    view! {
        <div class=style::page>
            <section class=style::content>
                <PageHeader title=tr(TextKey::RerunCollections)>
                    <PageAction href="/media/rerun-collections/add" label=tr(TextKey::AddRerunCollection)/>
                </PageHeader>
                <p class=style::error style:display=move || if initial_error.is_empty() { "none" } else { "" }>
                    {tr(TextKey::RerunCollectionActionFailed)}
                </p>

                {move || {
                    let (rows, total) = current_data.get();
                    let is_empty = rows.is_empty();
                    let searching = !search.get_untracked().trim().is_empty();
                    let hidden_when_empty = if is_empty { "none" } else { "" };
                    view! {
                        <div class=style::table_wrap>
                            <div class=style::table_toolbar>
                                <label class=style::search_field>
                                    <span style=icon_style(IconName::FilterList, "1.125rem") aria-hidden="true"></span>
                                    <input
                                        class=style::search
                                        type="search"
                                        placeholder=tr(TextKey::SearchForRerunCollections)
                                        prop:value=move || search.get()
                                        on:input=move |event| {
                                            search.set(event_target_value(&event));
                                            page.set(0);
                                        }
                                    />
                                </label>
                            </div>
                            <div style:display=if is_empty { "" } else { "none" }>
                                {if searching {
                                    view! { <EmptyState title=tr(TextKey::EmptyListTitle)/> }.into_any()
                                } else {
                                    view! {
                                        <EmptyState
                                            title=tr(TextKey::NoRerunCollections)
                                            help=tr(TextKey::NoRerunCollectionsHelp)
                                            action=("/media/rerun-collections/add".to_owned(), tr(TextKey::AddRerunCollection))
                                        />
                                    }.into_any()
                                }}
                            </div>
                            <table style:display=hidden_when_empty>
                                <tbody>
                                    {rows.into_iter().map(|row| {
                                        let id = row.id;
                                        let name = row.name.clone();
                                        let row_for_delete = row.clone();
                                        view! {
                                            <tr>
                                                <td>{name}</td>
                                                <td>
                                                    <div class=style::row_actions>
                                                        <a
                                                            class=style::icon_button
                                                            href=format!("/media/rerun-collections/{id}/edit")
                                                            title=tr(TextKey::EditRerunCollection)
                                                            aria-label=tr(TextKey::EditRerunCollection)
                                                        >
                                                            <span style=icon_style(IconName::Edit, "1.25rem") aria-hidden="true"></span>
                                                        </a>
                                                        <form method="post" action="/admin-api/rerun-collections/delete">
                                                            <input type="hidden" name="id" value=id.to_string()/>
                                                            <button
                                                                type="submit"
                                                                class=style::icon_button
                                                                title=tr(TextKey::DeleteRerunCollection)
                                                                aria-label=tr(TextKey::DeleteRerunCollection)
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
                                    }).collect_view()}
                                </tbody>
                            </table>
                            <div style:display=hidden_when_empty>
                            <PaginationControls
                                page=Signal::derive(move || page.get())
                                total=Signal::derive(move || Some(total))
                                page_size=Signal::derive(move || page_size.get())
                                page_size_options=Some(vec![10, 25, 50, 100])
                                on_page_size_change=Callback::new(move |value: u32| {
                                    page_size.set(value.max(1));
                                    page.set(0);
                                })
                                on_previous=Callback::new(move |()| {
                                    page.update(|value| *value = value.saturating_sub(1));
                                })
                                on_next=Callback::new(move |()| {
                                    let size = page_size.get().max(1);
                                    let max_page = u32::try_from(total.div_ceil(u64::from(size)))
                                        .unwrap_or(u32::MAX)
                                        .saturating_sub(1);
                                    page.update(|value| *value = value.saturating_add(1).min(max_page));
                                })
                            />
                            </div>
                        </div>
                    }
                    .into_any()
                }}
            </section>
            <DeleteDialog
                dialog_id="rerun-collection-delete-dialog"
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
pub fn RerunCollectionEditorPage() -> impl IntoView {
    let params = use_params_map();
    let query = use_query_map();
    let id = params.with(|p| p.get("id").and_then(|s| s.parse::<i64>().ok()).unwrap_or(0));
    let error = query.with_untracked(|params| params.get("error").unwrap_or_default());
    let new_collection_name = tr(TextKey::NewCollection).to_owned();
    let initial = Resource::new_blocking(
        move || (id, new_collection_name.clone()),
        move |(id, new_collection_name)| async move {
            let form = if id == 0 {
                RerunCollectionForm {
                    name: new_collection_name,
                    collection_type: COLLECTION_TYPE_COLLECTION,
                    first_run_playback_order: PLAYBACK_ORDER_CHRONOLOGICAL,
                    rerun_playback_order: PLAYBACK_ORDER_CHRONOLOGICAL,
                    ..RerunCollectionForm::default()
                }
            } else {
                get_rerun_collection_form(id).await?.unwrap_or_default()
            };
            let options = get_rerun_collection_editor_options().await?;
            Ok::<_, ServerFnError>((form, options))
        },
    );

    view! {
        <Suspense fallback=admin_loading_resource_view>
            {move || {
                let error = error.clone();
                initial.get().map(move |result| admin_result_view(result, |(initial_form, editor_options)| {
                    view! {
                        <RerunCollectionEditorIsland
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
fn RerunCollectionEditorIsland(
    initial_error: String,
    initial_form: RerunCollectionForm,
    editor_options: RerunCollectionEditorOptions,
) -> impl IntoView {
    view! {
        <RerunCollectionEditorForm
            initial_error=initial_error
            initial_form=initial_form
            editor_options=editor_options
        />
    }
}

#[component]
fn RerunCollectionEditorForm(
    initial_error: String,
    initial_form: RerunCollectionForm,
    editor_options: RerunCollectionEditorOptions,
) -> impl IntoView {
    let initial_collection_type = initial_form.collection_type;
    let form_id = RwSignal::new(initial_form.id);
    let name = RwSignal::new(initial_form.name);
    let collection_type = RwSignal::new(initial_collection_type);
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
    let first_run_playback_order = RwSignal::new(default_playback_order(
        initial_form.first_run_playback_order,
        initial_collection_type,
    ));
    let rerun_playback_order = RwSignal::new(default_playback_order(
        initial_form.rerun_playback_order,
        initial_collection_type,
    ));

    let error_message = RwSignal::new(initial_error);
    let save_action = Action::new(|form: &RerunCollectionForm| {
        let form = form.clone();
        async move { save_rerun_collection(form).await }
    });
    let toast = use_toast();
    Effect::new(move |_| {
        if let Some(result) = save_action.value().get() {
            match result {
                Ok(_) => {
                    toast.push(tr(TextKey::RerunCollectionSaved), ToastSeverity::Success);
                    set_browser_location("/media/rerun-collections");
                }
                Err(error) => {
                    let message = save_failed(media_browser_error_message(error));
                    error_message.set(message.clone());
                    toast.push(message, ToastSeverity::Error);
                }
            }
        }
    });

    let submit_form = move || {
        let collection_id_value = collection_id.get_untracked();
        let media_item_id_value = media_item_id.get_untracked();
        let multi_collection_id_value = multi_collection_id.get_untracked();
        let smart_collection_id_value = smart_collection_id.get_untracked();
        save_action.dispatch(normalize_form(
            form_id.get_untracked(),
            name.get_untracked(),
            collection_type.get_untracked(),
            &collection_id_value,
            &media_item_id_value,
            &multi_collection_id_value,
            &smart_collection_id_value,
            first_run_playback_order.get_untracked(),
            rerun_playback_order.get_untracked(),
        ));
    };

    view! {
        <div class=style::page>
            <form method="post" action="/admin-api/rerun-collections/save">
                <div class=style::editor_action_bar>
                    <button
                        type="submit"
                        class=style::btn_add
                        on:click=move |event| {
                            event.prevent_default();
                            submit_form();
                        }
                    >
                        <span style=icon_style(IconName::Save, "1.125rem") aria-hidden="true"></span>
                        <span>{if form_id.get_untracked() == 0 { tr(TextKey::AddRerunCollection) } else { tr(TextKey::SaveRerunCollection) }}</span>
                    </button>
                    <CancelLink href="/media/rerun-collections"/>
                </div>
                <section class=style::editor>
                    <h1 class=style::page_title>{tr(TextKey::RerunCollection)}</h1>
                    <p class=style::error style:display=move || if error_message.with(String::is_empty) { "none" } else { "" }>
                        {move || error_message.get()}
                    </p>
                    <input type="hidden" name="id" value=move || form_id.get().to_string()/>
                    <label>
                        <span>{tr(TextKey::Name)}</span>
                        <input
                            type="text"
                            name="name"
                            required
                            value=move || name.get()
                            on:input=move |event| name.set(event_target_value(&event))
                        />
                    </label>
                    <label>
                        <span>{tr(TextKey::CollectionType)}</span>
                        <select
                            name="collection_type"
                            prop:value=move || collection_type.get().to_string()
                            on:change=move |event| {
                                if let Ok(value) = event_target_value(&event).parse::<i32>() {
                                    collection_type.set(value);
                                    first_run_playback_order.set(default_playback_order(first_run_playback_order.get_untracked(), value));
                                    rerun_playback_order.set(default_playback_order(rerun_playback_order.get_untracked(), value));
                                }
                            }
                        >
                            <option value=COLLECTION_TYPE_COLLECTION>{tr(TextKey::Collection)}</option>
                            <option value=COLLECTION_TYPE_TELEVISION_SHOW>{tr(TextKey::TelevisionShow)}</option>
                            <option value=COLLECTION_TYPE_TELEVISION_SEASON>{tr(TextKey::TelevisionSeason)}</option>
                            <option value=COLLECTION_TYPE_ARTIST>{tr(TextKey::Artist)}</option>
                            <option value=COLLECTION_TYPE_MULTI_COLLECTION>{tr(TextKey::MultiCollection)}</option>
                            <option value=COLLECTION_TYPE_SMART_COLLECTION>{tr(TextKey::SmartCollection)}</option>
                            <option value=COLLECTION_TYPE_PLAYLIST>{tr(TextKey::Playlist)}</option>
                        </select>
                    </label>
                    {
                        let options = editor_options.clone();
                        view! {
                                    <TargetSelect
                                        label=TextKey::Collection
                                        name="collection_id"
                                        options=options.collections
                                        value=collection_id
                                        visible=Signal::derive(move || collection_type.get() == COLLECTION_TYPE_COLLECTION)
                                    />
                                    <TargetSelect
                                        label=TextKey::MultiCollection
                                        name="multi_collection_id"
                                        options=options.multi_collections
                                        value=multi_collection_id
                                        visible=Signal::derive(move || collection_type.get() == COLLECTION_TYPE_MULTI_COLLECTION)
                                    />
                                    <TargetSelect
                                        label=TextKey::SmartCollection
                                        name="smart_collection_id"
                                        options=options.smart_collections
                                        value=smart_collection_id
                                        visible=Signal::derive(move || collection_type.get() == COLLECTION_TYPE_SMART_COLLECTION)
                                    />
                                    <TargetSelect
                                        label=TextKey::TelevisionShow
                                        name="media_item_id"
                                        options=options.shows
                                        value=media_item_id
                                        visible=Signal::derive(move || collection_type.get() == COLLECTION_TYPE_TELEVISION_SHOW)
                                    />
                                    <TargetSelect
                                        label=TextKey::TelevisionSeason
                                        name="media_item_id"
                                        options=options.seasons
                                        value=media_item_id
                                        visible=Signal::derive(move || collection_type.get() == COLLECTION_TYPE_TELEVISION_SEASON)
                                    />
                                    <TargetSelect
                                        label=TextKey::Artist
                                        name="media_item_id"
                                        options=options.artists
                                        value=media_item_id
                                        visible=Signal::derive(move || collection_type.get() == COLLECTION_TYPE_ARTIST)
                                    />
                        }
                    }
                    <PlaybackOrderSelect
                        label=TextKey::FirstRunPlaybackOrder
                        name="first_run_playback_order"
                        value=first_run_playback_order
                        collection_type=collection_type
                    />
                    <PlaybackOrderSelect
                        label=TextKey::RerunPlaybackOrder
                        name="rerun_playback_order"
                        value=rerun_playback_order
                        collection_type=collection_type
                    />
                </section>
            </form>
        </div>
    }
}

#[component]
fn TargetSelect(
    label: TextKey,
    name: &'static str,
    options: Vec<RerunCollectionSelectOption>,
    value: RwSignal<String>,
    visible: Signal<bool>,
) -> impl IntoView {
    view! {
        <label style:display=move || if visible.get() { "" } else { "none" }>
            <span>{tr(label)}</span>
            <select
                name=name
                disabled=move || !visible.get()
                prop:value=move || value.get()
                on:change=move |event| value.set(event_target_value(&event))
            >
                <option value="">{tr(TextKey::TypeToSearch)}</option>
                {options.into_iter().map(|option| {
                    view! {
                        <option value=option.id.to_string()>{option.name}</option>
                    }
                }).collect_view()}
            </select>
        </label>
    }
}

#[component]
fn PlaybackOrderSelect(
    label: TextKey,
    name: &'static str,
    value: RwSignal<i32>,
    collection_type: RwSignal<i32>,
) -> impl IntoView {
    view! {
        <label>
            <span>{tr(label)}</span>
            <select
                name=name
                prop:value=move || value.get().to_string()
                on:change=move |event| {
                    if let Ok(parsed) = event_target_value(&event).parse::<i32>() {
                        value.set(default_playback_order(parsed, collection_type.get_untracked()));
                    }
                }
            >
                <option
                    value=PLAYBACK_ORDER_CHRONOLOGICAL
                    disabled=move || collection_type.get() == COLLECTION_TYPE_MULTI_COLLECTION
                >
                    {tr(TextKey::Chronological)}
                </option>
                <option
                    value=PLAYBACK_ORDER_SEASON_EPISODE
                    disabled=move || collection_type.get() != COLLECTION_TYPE_TELEVISION_SHOW
                    style:display=move || if collection_type.get() == COLLECTION_TYPE_TELEVISION_SHOW { "" } else { "none" }
                >
                    {tr(TextKey::SeasonEpisode)}
                </option>
                <option
                    value=PLAYBACK_ORDER_RANDOM
                    disabled=move || collection_type.get() == COLLECTION_TYPE_MULTI_COLLECTION
                >
                    {tr(TextKey::Random)}
                </option>
                <option value=PLAYBACK_ORDER_SHUFFLE>{tr(TextKey::Shuffle)}</option>
            </select>
        </label>
    }
}

#[allow(clippy::too_many_arguments)]
fn normalize_form(
    id: i64,
    name: String,
    collection_type: i32,
    collection_id: &str,
    media_item_id: &str,
    multi_collection_id: &str,
    smart_collection_id: &str,
    first_run_playback_order: i32,
    rerun_playback_order: i32,
) -> RerunCollectionForm {
    RerunCollectionForm {
        id,
        name,
        collection_type,
        collection_id: (collection_type == COLLECTION_TYPE_COLLECTION)
            .then(|| parse_id(collection_id))
            .flatten(),
        media_item_id: matches!(
            collection_type,
            COLLECTION_TYPE_TELEVISION_SHOW
                | COLLECTION_TYPE_TELEVISION_SEASON
                | COLLECTION_TYPE_ARTIST
        )
        .then(|| parse_id(media_item_id))
        .flatten(),
        multi_collection_id: (collection_type == COLLECTION_TYPE_MULTI_COLLECTION)
            .then(|| parse_id(multi_collection_id))
            .flatten(),
        smart_collection_id: (collection_type == COLLECTION_TYPE_SMART_COLLECTION)
            .then(|| parse_id(smart_collection_id))
            .flatten(),
        first_run_playback_order: default_playback_order(first_run_playback_order, collection_type),
        rerun_playback_order: default_playback_order(rerun_playback_order, collection_type),
    }
}

fn parse_id(value: &str) -> Option<i64> {
    value.trim().parse::<i64>().ok().filter(|value| *value != 0)
}

fn default_playback_order(value: i32, collection_type: i32) -> i32 {
    let allowed = match collection_type {
        COLLECTION_TYPE_MULTI_COLLECTION => value == PLAYBACK_ORDER_SHUFFLE,
        COLLECTION_TYPE_TELEVISION_SHOW => matches!(
            value,
            PLAYBACK_ORDER_CHRONOLOGICAL
                | PLAYBACK_ORDER_SEASON_EPISODE
                | PLAYBACK_ORDER_RANDOM
                | PLAYBACK_ORDER_SHUFFLE
        ),
        _ => matches!(
            value,
            PLAYBACK_ORDER_CHRONOLOGICAL | PLAYBACK_ORDER_RANDOM | PLAYBACK_ORDER_SHUFFLE
        ),
    };
    if allowed {
        value
    } else if collection_type == COLLECTION_TYPE_MULTI_COLLECTION {
        PLAYBACK_ORDER_SHUFFLE
    } else {
        PLAYBACK_ORDER_CHRONOLOGICAL
    }
}
