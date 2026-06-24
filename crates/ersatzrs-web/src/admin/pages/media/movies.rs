//! Movie browser — port of `Pages/MovieList.razor` (delegates to the shared
//! [`MultiSelectBrowserPage`]) plus the per-item `Pages/Movie.razor` details
//! page (`/media/movies/:id`).
//!
//! The list shell, multi-select toolbar, letter-bar, pager, and Add-to-
//! Collection / Add-to-Playlist / Show-Media-Info modals are shared across
//! movies/shows/artists/other-videos via
//! [`crate::admin::pages::media::multi_select_browser`].
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::admin::i18n::{content_variant_badge_label, tr, TextKey};
use crate::admin::pages::media::multi_select_browser::{BrowserKindConfig, MultiSelectBrowserPage};
use crate::admin::server_fns::media_browser::{get_movie_details, MovieDetailRow};
use crate::admin::ui::primitives::card::{MediaCard, MediaItemState};
use crate::admin::ui::primitives::icon::{icon_style, IconName};

#[allow(dead_code)]
mod detail_style {
    stylance::import_crate_style!(s, "src/admin/pages/media/movie_detail.module.css");
    pub use s::*;
}

#[component]
pub fn MoviesPage() -> impl IntoView {
    view! { <MultiSelectBrowserPage config=BrowserKindConfig::MOVIES/> }
}

// ---------------------------------------------------------------------------
// `/media/movies/:id` — details page port of `Pages/Movie.razor`.
// ---------------------------------------------------------------------------

#[component]
pub fn MoviePage() -> impl IntoView {
    let params = use_params_map();
    let id = params.with_untracked(|p| {
        p.get("id")
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or_default()
    });

    let data: Resource<Option<MovieDetailRow>> = Resource::new_blocking(
        move || id,
        |id| async move { get_movie_details(id).await.ok().flatten() },
    );

    view! {
        <Suspense fallback=move || view! { <p class=detail_style::loading>{tr(TextKey::Loading)}</p> }>
                {move || {
                    match data.get() {
                        Some(Some(movie)) => view! { <MovieDetailView movie/> }.into_any(),
                        Some(None) => view! {
                            <div class=detail_style::not_found>
                                <h1>{tr(TextKey::MovieNotFound)}</h1>
                                <a href="/media/movies">{tr(TextKey::BackToMovies)}</a>
                            </div>
                        }
                        .into_any(),
                        None => view! { <p class=detail_style::loading>{tr(TextKey::Loading)}</p> }.into_any(),
                    }
                }}
            </Suspense>
    }
}

#[component]
fn MovieDetailView(movie: MovieDetailRow) -> impl IntoView {
    let MovieDetailRow {
        id,
        title,
        year,
        plot,
        tagline: _,
        content_ratings,
        release_date: _,
        poster_url,
        fanart_url,
        file_path,
        local_path,
        media_item_state,
        genres,
        tags,
        languages,
        studios,
        directors,
        writers,
        actors,
        extras,
    } = movie;

    let title_for_poster = title.clone();
    let poster_state = media_state(media_item_state);
    let fanart_src = fanart_url.unwrap_or_default();
    let fanart_style = if fanart_src.trim().is_empty() {
        "display:none"
    } else {
        ""
    };
    let poster_src = poster_url.unwrap_or_default();
    let poster_style = if poster_src.trim().is_empty() {
        "display:none"
    } else {
        ""
    };
    let year_text = year.map(|y| y.to_string()).unwrap_or_default();
    let year_style = if year_text.is_empty() {
        "display:none"
    } else {
        ""
    };
    let plot_text = plot.unwrap_or_default();
    let plot_style = if plot_text.trim().is_empty() {
        "display:none"
    } else {
        ""
    };
    let warning_style = if matches!(
        poster_state,
        MediaItemState::FileNotFound | MediaItemState::Unavailable
    ) {
        ""
    } else {
        "display:none"
    };
    let warning_label = match poster_state {
        MediaItemState::FileNotFound => tr(TextKey::FileNotFound),
        MediaItemState::Unavailable => tr(TextKey::Unavailable),
        MediaItemState::Normal | MediaItemState::RemoteOnly => "",
    };
    let warning_path = match poster_state {
        MediaItemState::FileNotFound => file_path.unwrap_or_default(),
        MediaItemState::Unavailable => local_path.unwrap_or_default(),
        MediaItemState::Normal | MediaItemState::RemoteOnly => String::new(),
    };
    let file_not_found_style = if poster_state == MediaItemState::FileNotFound {
        ""
    } else {
        "display:none"
    };
    let unavailable_style = if poster_state == MediaItemState::Unavailable {
        ""
    } else {
        "display:none"
    };
    let actors_style = if actors.is_empty() {
        "display:none"
    } else {
        ""
    };
    let extras_style = if extras.is_empty() {
        "display:none"
    } else {
        ""
    };

    view! {
        <div class=detail_style::page>
            <div class=detail_style::fanart_container>
                <div class=detail_style::fanart_tint/>
                <img class=detail_style::fanart src=fanart_src alt=tr(TextKey::FanArt) style=fanart_style/>
            </div>

            <div class=detail_style::content>
                <div class=detail_style::hero>
                    <div class=detail_style::poster_wrap style=poster_style>
                        <img class=detail_style::poster src=poster_src alt=title_for_poster/>
                        <div
                            class=format!("{} {}", detail_style::state_badge, detail_style::state_badge_error)
                            aria-label=tr(TextKey::FileNotFound)
                            style=file_not_found_style
                        >
                            <span style=icon_style(IconName::Warning, "1.4rem") aria-hidden="true"></span>
                        </div>
                        <div
                            class=format!("{} {}", detail_style::state_badge, detail_style::state_badge_warning)
                            aria-label=tr(TextKey::Unavailable)
                            style=unavailable_style
                        >
                            <span style=icon_style(IconName::Warning, "1.4rem") aria-hidden="true"></span>
                        </div>
                    </div>
                    <div class=detail_style::header>
                        <h1 class=detail_style::title>{title.clone()}</h1>
                        <p class=detail_style::year style=year_style>{year_text}</p>
                        <div class=detail_style::plot_card style=plot_style>
                            <p class=detail_style::plot>{plot_text}</p>
                        </div>
                    </div>
                </div>
                <MovieActions id/>
                <div class=detail_style::warning style=warning_style>
                    <span style=icon_style(IconName::Warning, "1.2rem") aria-hidden="true"></span>
                    <span>{format!("{warning_label}: ")}</span>
                    <code>{warning_path}</code>
                </div>
                <dl class=detail_style::facts>
                    {linked_fact_row(tr(TextKey::ContentRatings), "content_rating", &content_ratings, false)}
                    {linked_fact_row(tr(TextKey::Languages), "language", &languages, true)}
                    {linked_fact_row(tr(TextKey::Studios), "studio", &studios, true)}
                    {linked_fact_row(tr(TextKey::Directors), "director", &directors, true)}
                    {linked_fact_row(tr(TextKey::Writers), "writer", &writers, true)}
                    {linked_fact_row(tr(TextKey::Genres), "genre", &genres, true)}
                    {linked_fact_row(tr(TextKey::Tags), "tag", &tags, true)}
                </dl>

                <section class=detail_style::actors_section style=extras_style>
                    <h2 class=detail_style::section_title>
                        {tr(TextKey::ContentVariantSpecialFeatures)}
                    </h2>
                    <div class=detail_style::actor_grid>
                        {extras.into_iter().map(|extra| {
                            let extra_id = extra.id;
                            let title = extra.title.clone();
                            let placeholder = crate::admin::ui::primitives::card::first_letter_placeholder(&extra.sort_title);
                            view! {
                                <MediaCard
                                    title=title
                                    subtitle=extra.subtitle
                                    artwork_url=extra.artwork_path
                                    href=Some(format!("/media/other/videos/{extra_id}"))
                                    placeholder=placeholder
                                    state=MediaItemState::Normal
                                    badge=content_variant_badge_label(
                                        extra.variant.content_kind,
                                        extra.variant.sub_kind.as_deref(),
                                    )
                                />
                            }
                        }).collect_view()}
                    </div>
                </section>

                <section class=detail_style::actors_section style=actors_style>
                    <h2 class=detail_style::section_title>{tr(TextKey::Actors)}</h2>
                    <div class=detail_style::actor_grid>
                        {actors.into_iter().map(|actor| {
                            let href = search_link("actor", &actor.name, true);
                            view! {
                                <MediaCard
                                    title=actor.name.clone()
                                    subtitle=actor.role.filter(|role| !role.trim().is_empty())
                                    artwork_url=actor.thumb_url
                                    href=Some(href)
                                    placeholder=actor.name
                                    state=MediaItemState::Normal
                                />
                            }
                        }).collect_view()}
                    </div>
                </section>
            </div>

        </div>
    }
}

#[component]
fn MovieActions(id: i64) -> impl IntoView {
    let add_button_id = format!("movie-add-button-{id}");
    let menu_id = format!("movie-add-menu-{id}");
    let collection_open_id = format!("movie-collection-open-{id}");
    let collection_modal_id = format!("movie-collection-modal-{id}");
    let collection_select_id = format!("movie-collection-select-{id}");
    let collection_name_id = format!("movie-collection-name-{id}");
    let collection_message_id = format!("movie-collection-message-{id}");
    let collection_cancel_id = format!("movie-collection-cancel-{id}");
    let collection_submit_id = format!("movie-collection-submit-{id}");
    let playlist_open_id = format!("movie-playlist-open-{id}");
    let playlist_modal_id = format!("movie-playlist-modal-{id}");
    let playlist_group_id = format!("movie-playlist-group-{id}");
    let playlist_select_id = format!("movie-playlist-select-{id}");
    let playlist_message_id = format!("movie-playlist-message-{id}");
    let playlist_cancel_id = format!("movie-playlist-cancel-{id}");
    let playlist_submit_id = format!("movie-playlist-submit-{id}");
    let media_info_open_id = format!("movie-media-info-open-{id}");
    let media_info_modal_id = format!("movie-media-info-modal-{id}");
    let media_info_title_id = format!("movie-media-info-title-{id}");
    let media_info_body_id = format!("movie-media-info-body-{id}");
    let media_info_close_id = format!("movie-media-info-close-{id}");
    let troubleshoot_href = format!("/system/troubleshooting/playback?mediaItem={id}");
    let title_for_info = format!("{} {id}", tr(TextKey::Movie));
    let script_ids = MovieActionScriptIds {
        media_item_id: id,
        title: &title_for_info,
        add_button: &add_button_id,
        menu: &menu_id,
        collection_open: &collection_open_id,
        collection_modal: &collection_modal_id,
        collection_select: &collection_select_id,
        collection_name: &collection_name_id,
        collection_message: &collection_message_id,
        collection_cancel: &collection_cancel_id,
        collection_submit: &collection_submit_id,
        playlist_open: &playlist_open_id,
        playlist_modal: &playlist_modal_id,
        playlist_group: &playlist_group_id,
        playlist_select: &playlist_select_id,
        playlist_message: &playlist_message_id,
        playlist_cancel: &playlist_cancel_id,
        playlist_submit: &playlist_submit_id,
        media_info_open: &media_info_open_id,
        media_info_modal: &media_info_modal_id,
        media_info_title: &media_info_title_id,
        media_info_body: &media_info_body_id,
        media_info_close: &media_info_close_id,
    };
    let script = movie_actions_script(&script_ids);

    view! {
        <div class=detail_style::actions>
            <div class=detail_style::add_menu>
                <button
                    id=add_button_id
                    type="button"
                    class=detail_style::btn_add
                    aria-label=tr(TextKey::AddToEllipsis)
                >
                    <span>{tr(TextKey::AddToEllipsis)}</span>
                </button>
                <div
                    class=detail_style::add_menu_popover
                    role="menu"
                    id=menu_id
                    style="display:none"
                >
                    <button
                        id=collection_open_id
                        type="button"
                        class=detail_style::add_menu_entry
                        role="menuitem"
                    >
                        <span>{tr(TextKey::AddToCollection)}</span>
                    </button>
                    <button
                        id=playlist_open_id
                        type="button"
                        class=detail_style::add_menu_entry
                        role="menuitem"
                    >
                        <span>{tr(TextKey::AddToPlaylist)}</span>
                    </button>
                </div>
            </div>
            <button
                id=media_info_open_id
                type="button"
                class=detail_style::btn_secondary
            >
                <span>{tr(TextKey::ShowMediaInfo)}</span>
            </button>
            <a
                class=detail_style::btn_secondary
                href=troubleshoot_href
            >
                <span>{tr(TextKey::TroubleshootPlayback)}</span>
            </a>
            <a class=detail_style::btn_back href="/media/movies">
                <span>{tr(TextKey::Back)}</span>
            </a>

            <div class=detail_style::modal_backdrop id=collection_modal_id.clone() style="display:none">
                <div class=detail_style::modal_panel>
                    <h2>{tr(TextKey::AddToCollection)}</h2>
                    <p class=detail_style::modal_help>{tr(TextKey::SelectCollectionToAddThisMovie)}</p>
                    <label for=collection_select_id.clone()>{tr(TextKey::Collection)}</label>
                    <select id=collection_select_id.clone()></select>
                    <label for=collection_name_id.clone()>{tr(TextKey::NewCollectionName)}</label>
                    <input id=collection_name_id.clone() type="text"/>
                    <p class=detail_style::modal_message id=collection_message_id.clone()></p>
                    <div class=detail_style::modal_actions>
                        <button
                            id=collection_cancel_id
                            type="button"
                            class=detail_style::btn_back
                        >
                            {tr(TextKey::Cancel)}
                        </button>
                        <button
                            id=collection_submit_id
                            type="button"
                            class=detail_style::btn_add
                        >
                            {tr(TextKey::AddToCollection)}
                        </button>
                    </div>
                </div>
            </div>

            <div class=detail_style::modal_backdrop id=playlist_modal_id.clone() style="display:none">
                <div class=detail_style::modal_panel>
                    <h2>{tr(TextKey::AddToPlaylist)}</h2>
                    <p class=detail_style::modal_help>{tr(TextKey::SelectPlaylistToAddThisMovie)}</p>
                    <label for=playlist_group_id.clone()>{tr(TextKey::PlaylistGroup)}</label>
                    <select id=playlist_group_id.clone()></select>
                    <label for=playlist_select_id.clone()>{tr(TextKey::Playlist)}</label>
                    <select id=playlist_select_id.clone()></select>
                    <p class=detail_style::modal_message id=playlist_message_id.clone()></p>
                    <div class=detail_style::modal_actions>
                        <button
                            id=playlist_cancel_id
                            type="button"
                            class=detail_style::btn_back
                        >
                            {tr(TextKey::Cancel)}
                        </button>
                        <button
                            id=playlist_submit_id
                            type="button"
                            class=detail_style::btn_add
                        >
                            {tr(TextKey::AddToPlaylist)}
                        </button>
                    </div>
                </div>
            </div>

            <div class=detail_style::modal_backdrop id=media_info_modal_id.clone() style="display:none">
                <div class=detail_style::modal_panel>
                    <h2 id=media_info_title_id.clone()>{tr(TextKey::MediaInfo)}</h2>
                    <pre class=detail_style::media_info_pre><code id=media_info_body_id.clone()></code></pre>
                    <div class=detail_style::modal_actions>
                        <button
                            id=media_info_close_id
                            type="button"
                            class=detail_style::btn_back
                        >
                            {tr(TextKey::Close)}
                        </button>
                    </div>
                </div>
            </div>
            <script inner_html=script></script>
        </div>
    }
}

struct MovieActionScriptIds<'a> {
    media_item_id: i64,
    title: &'a str,
    add_button: &'a str,
    menu: &'a str,
    collection_open: &'a str,
    collection_modal: &'a str,
    collection_select: &'a str,
    collection_name: &'a str,
    collection_message: &'a str,
    collection_cancel: &'a str,
    collection_submit: &'a str,
    playlist_open: &'a str,
    playlist_modal: &'a str,
    playlist_group: &'a str,
    playlist_select: &'a str,
    playlist_message: &'a str,
    playlist_cancel: &'a str,
    playlist_submit: &'a str,
    media_info_open: &'a str,
    media_info_modal: &'a str,
    media_info_title: &'a str,
    media_info_body: &'a str,
    media_info_close: &'a str,
}

#[allow(clippy::too_many_lines)]
fn movie_actions_script(ids: &MovieActionScriptIds<'_>) -> String {
    format!(
        r#"
(() => {{
  const mediaItemId = {media_item_id};
  const title = {title};
  const labels = {{
    requestFailed: {request_failed},
    loadingCollections: {loading_collections},
    newCollection: {new_collection},
    addingItem: {adding_item},
    loadingPlaylistGroups: {loading_playlist_groups},
    loadingPlaylists: {loading_playlists},
    noPlaylistsInGroup: {no_playlists_in_group},
    pleaseChoosePlaylist: {please_choose_playlist},
    addedItemToPlaylist: {added_item_to_playlist},
    loadingMediaInfo: {loading_media_info},
    unableToLoadMediaInfo: {unable_to_load_media_info},
  }};
  const byId = (id) => document.getElementById(id);
  const show = (id) => {{ const el = byId(id); if (el) el.style.display = ""; }};
  const hide = (id) => {{ const el = byId(id); if (el) el.style.display = "none"; }};
  const text = (id, value) => {{ const el = byId(id); if (el) el.textContent = value; }};
  const json = async (url, options) => {{
    const response = await fetch(url, options);
    if (!response.ok) {{
      let message = labels.requestFailed;
      try {{
        const body = await response.json();
        message = body.error || message;
      }} catch (_) {{}}
      throw new Error(message);
    }}
    return response.json();
  }};
  const fillSelect = (id, rows, first) => {{
    const select = byId(id);
    if (!select) return;
    select.innerHTML = "";
    const all = first ? [first, ...rows] : rows;
    for (const row of all) {{
      const option = document.createElement("option");
      option.value = String(row.id);
      option.textContent = row.name;
      select.appendChild(option);
    }}
  }};

  byId({add_button})?.addEventListener("click", () => {{
    const menu = byId({menu});
    if (menu) menu.style.display = menu.style.display === "none" ? "" : "none";
  }});
  byId({collection_open})?.addEventListener("click", async () => {{
    hide({menu});
    show({collection_modal});
    const name = byId({collection_name});
    if (name) name.value = "";
    text({collection_message}, labels.loadingCollections);
    try {{
      const collections = await json("/api/media/actions/collections");
      fillSelect({collection_select}, collections, {{ id: -1, name: labels.newCollection }});
      text({collection_message}, "");
    }} catch (error) {{
      text({collection_message}, error.message);
    }}
  }});
  byId({collection_cancel})?.addEventListener("click", () => hide({collection_modal}));
  byId({collection_submit})?.addEventListener("click", async () => {{
    const select = byId({collection_select});
    const name = byId({collection_name});
    const selected = Number(select?.value || -1);
    text({collection_message}, labels.addingItem);
    try {{
      const result = await json("/api/media/actions/collections/add", {{
        method: "POST",
        headers: {{ "Content-Type": "application/json" }},
        body: JSON.stringify({{
          media_item_id: mediaItemId,
          collection_id: selected > 0 ? selected : null,
          new_name: selected > 0 ? null : (name?.value || "")
        }})
      }});
      window.location.href = `/media/collections/${{result.id}}`;
    }} catch (error) {{
      text({collection_message}, error.message);
    }}
  }});

  byId({playlist_open})?.addEventListener("click", async () => {{
    hide({menu});
    show({playlist_modal});
    text({playlist_message}, labels.loadingPlaylistGroups);
    try {{
      const groups = await json("/api/media/actions/playlist-groups");
      fillSelect({playlist_group}, groups);
      text({playlist_message}, "");
      byId({playlist_group})?.dispatchEvent(new Event("change"));
    }} catch (error) {{
      text({playlist_message}, error.message);
    }}
  }});
  byId({playlist_group})?.addEventListener("change", async () => {{
    const groupId = Number(byId({playlist_group})?.value || 0);
    text({playlist_message}, labels.loadingPlaylists);
    try {{
      const playlists = groupId > 0 ? await json(`/api/media/actions/playlists?group_id=${{groupId}}`) : [];
      fillSelect({playlist_select}, playlists);
      text({playlist_message}, playlists.length ? "" : labels.noPlaylistsInGroup);
    }} catch (error) {{
      text({playlist_message}, error.message);
    }}
  }});
  byId({playlist_cancel})?.addEventListener("click", () => hide({playlist_modal}));
  byId({playlist_submit})?.addEventListener("click", async () => {{
    const playlistId = Number(byId({playlist_select})?.value || 0);
    if (!playlistId) {{
      text({playlist_message}, labels.pleaseChoosePlaylist);
      return;
    }}
    text({playlist_message}, labels.addingItem);
    try {{
      await json("/api/media/actions/playlists/add", {{
        method: "POST",
        headers: {{ "Content-Type": "application/json" }},
        body: JSON.stringify({{ media_item_id: mediaItemId, playlist_id: playlistId }})
      }});
      text({playlist_message}, labels.addedItemToPlaylist);
      setTimeout(() => hide({playlist_modal}), 600);
    }} catch (error) {{
      text({playlist_message}, error.message);
    }}
  }});

  byId({media_info_open})?.addEventListener("click", async () => {{
    show({media_info_modal});
    text({media_info_title}, title);
    text({media_info_body}, labels.loadingMediaInfo);
    try {{
      const response = await fetch(`/api/media/actions/media-info/${{mediaItemId}}`);
      if (!response.ok) throw new Error(labels.unableToLoadMediaInfo);
      text({media_info_body}, await response.text());
    }} catch (error) {{
      text({media_info_body}, error.message);
    }}
  }});
  byId({media_info_close})?.addEventListener("click", () => hide({media_info_modal}));
}})();
"#,
        media_item_id = ids.media_item_id,
        title = js_string(ids.title),
        add_button = js_string(ids.add_button),
        menu = js_string(ids.menu),
        collection_open = js_string(ids.collection_open),
        collection_modal = js_string(ids.collection_modal),
        collection_select = js_string(ids.collection_select),
        collection_name = js_string(ids.collection_name),
        collection_message = js_string(ids.collection_message),
        collection_cancel = js_string(ids.collection_cancel),
        collection_submit = js_string(ids.collection_submit),
        playlist_open = js_string(ids.playlist_open),
        playlist_modal = js_string(ids.playlist_modal),
        playlist_group = js_string(ids.playlist_group),
        playlist_select = js_string(ids.playlist_select),
        playlist_message = js_string(ids.playlist_message),
        playlist_cancel = js_string(ids.playlist_cancel),
        playlist_submit = js_string(ids.playlist_submit),
        media_info_open = js_string(ids.media_info_open),
        media_info_modal = js_string(ids.media_info_modal),
        media_info_title = js_string(ids.media_info_title),
        media_info_body = js_string(ids.media_info_body),
        media_info_close = js_string(ids.media_info_close),
        request_failed = js_string(tr(TextKey::RequestFailed)),
        loading_collections = js_string(tr(TextKey::LoadingCollections)),
        new_collection = js_string(tr(TextKey::NewCollection)),
        adding_item = js_string(tr(TextKey::AddingItem)),
        loading_playlist_groups = js_string(tr(TextKey::LoadingPlaylistGroups)),
        loading_playlists = js_string(tr(TextKey::LoadingPlaylists)),
        no_playlists_in_group = js_string(tr(TextKey::NoPlaylistsInGroup)),
        please_choose_playlist = js_string(tr(TextKey::PleaseChoosePlaylist)),
        added_item_to_playlist = js_string(tr(TextKey::AddedItemToPlaylist)),
        loading_media_info = js_string(tr(TextKey::LoadingMediaInfo)),
        unable_to_load_media_info = js_string(tr(TextKey::UnableToLoadMediaInfo)),
    )
}

fn js_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

fn linked_fact_row(
    label: &'static str,
    search_field: &'static str,
    values: &[String],
    lowercase_query: bool,
) -> AnyView {
    let row_style = if values.iter().all(|s| s.trim().is_empty()) {
        "display:none"
    } else {
        ""
    };

    view! {
        <div class=detail_style::fact_row style=row_style>
            <dt>{label}</dt>
            <dd>
                {values.iter()
                    .filter(|s| !s.trim().is_empty())
                    .enumerate()
                    .map(|(index, value)| {
                        let href = search_link(search_field, value, lowercase_query);
                        let value = value.clone();
                        view! {
                            {if index > 0 { Some(view! { <span>", "</span> }) } else { None }}
                            <a href=href>{value}</a>
                        }
                    })
                    .collect_view()}
            </dd>
        </div>
    }
    .into_any()
}

fn media_state(state: i32) -> MediaItemState {
    match state {
        1 => MediaItemState::FileNotFound,
        2 => MediaItemState::Unavailable,
        3 => MediaItemState::RemoteOnly,
        _ => MediaItemState::Normal,
    }
}

pub(crate) fn search_link(field: &str, value: &str, lowercase_query: bool) -> String {
    let term = if lowercase_query {
        value.to_lowercase()
    } else {
        value.to_string()
    };
    let query = format!(r#"{field}:"{term}""#);
    format!("/search?query={}", urlencoding::encode(&query))
}
