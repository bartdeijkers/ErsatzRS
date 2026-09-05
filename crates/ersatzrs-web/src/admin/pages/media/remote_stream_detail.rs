//! Read-only Remote Stream details opened from search and media-list filters.
//!
//! The page stays server-rendered: all actions are ordinary links, so there is
//! no island or JavaScript-only fallback to keep in sync.

use std::collections::BTreeMap;

use ersatzrs_addon_contract::AddonMediaListOptionDefinition;
use ersatzrs_domain::{
    AddonMediaListContentKind, AddonMediaListItemAvailability, AddonMediaListOptionValue,
    RemoteStreamLiveness,
};
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::admin::i18n::{episode_display_subtitle, text, tr, use_i18n, TextKey, UiLanguage};
use crate::admin::server_fns::media_browser::{
    get_remote_stream_details, RemoteStreamChannelRow, RemoteStreamChapterRow,
    RemoteStreamDetailRow, RemoteStreamOwnerRow, RemoteStreamPlaybackHealthStatusRow,
    RemoteStreamTrackRow,
};
use crate::admin::ui::primitives::resource_state::AdminErrorState;

#[allow(dead_code)]
mod style {
    stylance::import_crate_style!(s, "src/admin/pages/media/movie_detail.module.css");
    pub use s::*;
}

#[component]
pub fn RemoteStreamDetailPage() -> impl IntoView {
    let params = use_params_map();
    let id = params.with_untracked(|params| {
        params
            .get("id")
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or_default()
    });
    let detail = Resource::new_blocking(
        move || id,
        |id| async move { get_remote_stream_details(id).await },
    );

    view! {
        <Suspense fallback=move || view! { <p class=style::loading>{tr(TextKey::Loading)}</p> }>
            {move || match detail.get() {
                Some(Ok(Some(detail))) => view! { <RemoteStreamDetailView detail/> }.into_any(),
                Some(Ok(None)) => view! {
                    <div class=style::not_found>
                        <h1>{format!("{} — {}", tr(TextKey::RemoteStream), tr(TextKey::NotFound))}</h1>
                        <a href="/media/remote/streams">{tr(TextKey::Back)}</a>
                    </div>
                }.into_any(),
                Some(Err(error)) => view! { <AdminErrorState error=error.to_string()/> }.into_any(),
                None => view! { <p class=style::loading>{tr(TextKey::Loading)}</p> }.into_any(),
            }}
        </Suspense>
    }
}

#[component]
#[allow(clippy::too_many_lines)]
fn RemoteStreamDetailView(detail: RemoteStreamDetailRow) -> impl IntoView {
    let language = use_i18n();
    let artwork = detail
        .poster_url
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            detail
                .thumbnail_url
                .filter(|value| !value.trim().is_empty())
        })
        .unwrap_or_default();
    let artwork_style = if artwork.is_empty() {
        "display:none"
    } else {
        ""
    };
    let fanart = detail.fanart_url.unwrap_or_default();
    let fanart_style = if fanart.trim().is_empty() {
        "display:none"
    } else {
        ""
    };
    let subtitle = detail.tagline.unwrap_or_default();
    let subtitle_style = if subtitle.trim().is_empty() {
        "display:none"
    } else {
        ""
    };
    let plot = detail.plot.or(detail.outline).unwrap_or_default();
    let plot_style = if plot.trim().is_empty() {
        "display:none"
    } else {
        ""
    };
    let episode_context = episode_display_subtitle(
        detail.show_title.as_deref(),
        detail.season_number,
        detail.episode_number,
    )
    .filter(|_| {
        detail.show_title.is_some()
            || detail.season_number.is_some()
            || detail.episode_number.is_some()
    })
    .unwrap_or_default();
    let broadcasters = if detail.broadcasters.is_empty() {
        detail.original_broadcasters
    } else {
        detail.broadcasters
    };
    let duration = detail.effective_duration.unwrap_or_default();
    let source_addon = detail.source_addon_id.unwrap_or_default();
    let tracks_style = if detail.tracks.is_empty() {
        "display:none"
    } else {
        ""
    };
    let chapters_style = if detail.chapters.is_empty() {
        "display:none"
    } else {
        ""
    };
    let owners_style = if detail.owners.is_empty() {
        "display:none"
    } else {
        ""
    };
    let channels_style = if detail.channels.is_empty() {
        "display:none"
    } else {
        ""
    };
    let health_token = playback_health_token(detail.playback_health.status);
    let health_label = tr(playback_health_text_key(detail.playback_health.status));
    let title = detail.title;
    let id = detail.id;

    view! {
        <div class=style::page data-remote-stream-detail=id>
            <div class=style::fanart_container>
                <div class=style::fanart_tint/>
                <img class=style::fanart src=fanart alt=tr(TextKey::FanArt) style=fanart_style/>
            </div>
            <div class=style::content>
                <div class=style::hero>
                    <div class=style::poster_wrap style=artwork_style>
                        <img class=style::poster src=artwork alt=title.clone()/>
                    </div>
                    <div class=style::header>
                        <h1 class=style::title>{title}</h1>
                        <p class=style::year style=subtitle_style>{subtitle}</p>
                        <div class=style::plot_card style=plot_style><p class=style::plot>{plot}</p></div>
                    </div>
                </div>

                <div class=style::actions>
                    <a
                        class=style::btn_secondary
                        href=format!("/system/troubleshooting/playback?mediaItem={id}")
                    >{tr(TextKey::TroubleshootPlayback)}</a>
                    <a class=style::btn_back href="/media/remote/streams">{tr(TextKey::Back)}</a>
                </div>

                <dl class=style::facts>
                    {text_fact_row(TextKey::OriginalTitle, detail.original_title.unwrap_or_default())}
                    {text_fact_row(TextKey::SortTitle, detail.sort_title)}
                    {text_fact_row(TextKey::Year, detail.year.map(|value| value.to_string()).unwrap_or_default())}
                    {text_fact_row(TextKey::Episode, episode_context)}
                    {text_fact_row(TextKey::ReleaseDate, detail.release_date.unwrap_or_default())}
                    {text_fact_row(TextKey::Collection, detail.collection_name.unwrap_or_default())}
                    {text_fact_row(TextKey::MediaKind, tr(content_kind_text_key(detail.content_kind)).to_owned())}
                    {text_fact_row(TextKey::Duration, duration)}
                    {text_fact_row(TextKey::Available, tr(availability_text_key(detail.availability)).to_owned())}
                    {text_fact_row(TextKey::FailureReason, detail.availability_reason.unwrap_or_default())}
                    {text_fact_row(TextKey::PlaybackBehavior, tr(liveness_text_key(detail.liveness)).to_owned())}
                    {text_fact_row(TextKey::Addon, source_addon)}
                    {list_fact_row(TextKey::Networks, &broadcasters)}
                    {list_fact_row(TextKey::ContentRatings, &detail.facets.content_ratings)}
                    {list_fact_row(TextKey::Languages, &detail.facets.languages)}
                    {list_fact_row(TextKey::Studios, &detail.facets.studios)}
                    {list_fact_row(TextKey::MusicLabels, &detail.facets.music_labels)}
                    {list_fact_row(TextKey::Genres, &detail.facets.genres)}
                    {list_fact_row(TextKey::Tags, &detail.facets.tags)}
                </dl>

                <section class=style::actors_section data-playback-health=health_token>
                    <h2 class=style::section_title>{tr(TextKey::PlaybackHealth)}</h2>
                    <div class=style::commentary_card><strong>{health_label}</strong></div>
                </section>

                <section class=style::actors_section style=tracks_style>
                    <h2 class=style::section_title>{tr(TextKey::Playback)}</h2>
                    <div class=style::commentary_grid>
                        {detail.tracks.into_iter().map(track_card).collect_view()}
                    </div>
                </section>

                <section class=style::actors_section style=chapters_style>
                    <h2 class=style::section_title>{tr(TextKey::ChapterInput)}</h2>
                    <div class=style::commentary_grid>
                        {detail.chapters.into_iter().enumerate().map(chapter_card).collect_view()}
                    </div>
                </section>

                <section class=style::actors_section style=owners_style>
                    <h2 class=style::section_title>{tr(TextKey::AddonMediaLists)}</h2>
                    <div class=style::commentary_grid>
                        {detail.owners.into_iter().map(|owner| owner_card(owner, language)).collect_view()}
                    </div>
                </section>

                <section class=style::actors_section style=channels_style>
                    <h2 class=style::section_title>{tr(TextKey::Channels)}</h2>
                    <div class=style::commentary_grid>
                        {detail.channels.iter().map(channel_card).collect_view()}
                    </div>
                </section>
            </div>
        </div>
    }
}

fn text_fact_row(label: TextKey, value: String) -> AnyView {
    let hidden = if value.trim().is_empty() {
        "display:none"
    } else {
        ""
    };
    view! {
        <div class=style::fact_row style=hidden>
            <dt>{tr(label)}</dt><dd>{value}</dd>
        </div>
    }
    .into_any()
}

fn list_fact_row(label: TextKey, values: &[String]) -> AnyView {
    text_fact_row(
        label,
        values
            .iter()
            .filter(|value| !value.trim().is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join(", "),
    )
}

fn track_card(track: RemoteStreamTrackRow) -> AnyView {
    let kind = track_kind_text_key(track.kind);
    let title = track
        .title
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            track.index.map_or_else(
                || tr(kind).to_owned(),
                |index| format!("{} {index}", tr(kind)),
            )
        });
    let mut metadata = [track.language, track.codec, track.profile]
        .into_iter()
        .flatten()
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>();
    if let Some(channels) = track.channels {
        metadata.push(format!("{}: {channels}", tr(TextKey::AudioChannels)));
    }
    if track.is_default == Some(true) {
        metadata.push(tr(TextKey::DefaultOption).to_owned());
    }
    if track.is_forced == Some(true) {
        metadata.push(tr(TextKey::Forced).to_owned());
    }
    view! {
        <article class=style::commentary_card>
            <h3 class=style::variant_title>{title}</h3>
            <p class=style::variant_meta>{metadata.join(" · ")}</p>
        </article>
    }
    .into_any()
}

fn chapter_card((index, chapter): (usize, RemoteStreamChapterRow)) -> AnyView {
    let title = if chapter.title.trim().is_empty() {
        format!("{} {}", tr(TextKey::ChapterInput), index + 1)
    } else {
        chapter.title
    };
    view! {
        <article class=style::commentary_card>
            <h3 class=style::variant_title>{title}</h3>
            <p class=style::variant_meta>{format!("{} – {}", chapter.start_time, chapter.end_time)}</p>
        </article>
    }
    .into_any()
}

fn owner_card(owner: RemoteStreamOwnerRow, language: UiLanguage) -> AnyView {
    let options = owner
        .option_schema
        .as_ref()
        .map_or_else(Vec::new, |schema| {
            schema
                .definitions
                .iter()
                .filter_map(|definition| {
                    let value = owner.published_effective_options.get(&definition.key)?;
                    let label = localized_option_text(&definition.label, language)
                        .unwrap_or(&definition.key)
                        .to_owned();
                    let description = localized_option_text(&definition.description, language)
                        .unwrap_or_default()
                        .to_owned();
                    Some((
                        label,
                        option_value_text(definition, value, language),
                        description,
                    ))
                })
                .collect::<Vec<_>>()
        });
    let option_style = if options.is_empty() {
        "display:none"
    } else {
        ""
    };
    let list_href = format!("/media/addon-lists/{}", owner.list_id);
    view! {
        <article class=style::commentary_card data-addon-list-id=owner.list_id>
            <h3 class=style::variant_title><a href=list_href>{owner.list_name}</a></h3>
            <dl class=style::facts>
                {text_fact_row(TextKey::Addon, owner.addon_id)}
                {text_fact_row(TextKey::Published, owner.published_revision.to_string())}
            </dl>
            <section style=option_style>
                <h4 class=style::section_title>{tr(TextKey::PublishedOptions)}</h4>
                <dl class=style::facts>
                    {options.into_iter().map(|(label, value, description)| {
                        let description_style = if description.is_empty() { "display:none" } else { "display:block" };
                        view! {
                            <div class=style::fact_row>
                                <dt>{label}</dt>
                                <dd>{value}<small style=description_style>{description}</small></dd>
                            </div>
                        }
                    }).collect_view()}
                </dl>
            </section>
        </article>
    }
    .into_any()
}

fn channel_card(channel: &RemoteStreamChannelRow) -> AnyView {
    let label = if channel.number.trim().is_empty() {
        channel.name.clone()
    } else {
        format!("{} · {}", channel.number, channel.name)
    };
    let configure_href = format!("/channels/{}", channel.channel_id);
    let watch = channel.is_enabled.then(|| {
        view! {
            <a class=style::btn_add href=format!("/channels/{}/watch", channel.channel_id)>
                {tr(TextKey::Watch)}
            </a>
        }
    });
    view! {
        <article class=style::commentary_card data-channel-id=channel.channel_id>
            <h3 class=style::variant_title>{label}</h3>
            <div class=style::variant_actions>
                <a class=style::btn_secondary href=configure_href>{tr(TextKey::Configure)}</a>
                {watch}
            </div>
        </article>
    }
    .into_any()
}

const fn content_kind_text_key(kind: AddonMediaListContentKind) -> TextKey {
    match kind {
        AddonMediaListContentKind::Auto => TextKey::RemoteStream,
        AddonMediaListContentKind::TelevisionEpisode => TextKey::Episode,
        AddonMediaListContentKind::Movie => TextKey::Movie,
        AddonMediaListContentKind::MusicVideo => TextKey::MusicVideo,
        AddonMediaListContentKind::OtherVideo => TextKey::OtherVideo,
        AddonMediaListContentKind::Song => TextKey::Song,
        AddonMediaListContentKind::Image => TextKey::Image,
    }
}

const fn liveness_text_key(liveness: RemoteStreamLiveness) -> TextKey {
    match liveness {
        RemoteStreamLiveness::Unknown => TextKey::UnknownAvailability,
        RemoteStreamLiveness::Finite => TextKey::FiniteMedia,
        RemoteStreamLiveness::Live => TextKey::LiveSource,
    }
}

const fn availability_text_key(availability: AddonMediaListItemAvailability) -> TextKey {
    match availability {
        AddonMediaListItemAvailability::Available => TextKey::Available,
        AddonMediaListItemAvailability::Unavailable => TextKey::Unavailable,
        AddonMediaListItemAvailability::Unknown => TextKey::UnknownAvailability,
    }
}

const fn playback_health_text_key(status: RemoteStreamPlaybackHealthStatusRow) -> TextKey {
    match status {
        RemoteStreamPlaybackHealthStatusRow::Ready => TextKey::Available,
        RemoteStreamPlaybackHealthStatusRow::NeedsReview => TextKey::NeedReview,
        RemoteStreamPlaybackHealthStatusRow::Blocked => TextKey::Unavailable,
    }
}

const fn playback_health_token(status: RemoteStreamPlaybackHealthStatusRow) -> &'static str {
    match status {
        RemoteStreamPlaybackHealthStatusRow::Ready => "ready",
        RemoteStreamPlaybackHealthStatusRow::NeedsReview => "needs_review",
        RemoteStreamPlaybackHealthStatusRow::Blocked => "blocked",
    }
}

const fn track_kind_text_key(kind: i32) -> TextKey {
    match kind {
        1 => TextKey::Video,
        2 => TextKey::Audio,
        3 | 5 => TextKey::Subtitle,
        _ => TextKey::MediaInfo,
    }
}

fn localized_option_text(values: &BTreeMap<String, String>, language: UiLanguage) -> Option<&str> {
    values
        .iter()
        .find(|(locale, _)| option_locale_matches(locale, language))
        .or_else(|| {
            values
                .iter()
                .find(|(locale, _)| locale.eq_ignore_ascii_case("en-US"))
        })
        .or_else(|| values.iter().next())
        .map(|(_, value)| value.as_str())
}

fn option_locale_matches(locale: &str, language: UiLanguage) -> bool {
    let normalized = locale.trim().to_ascii_lowercase();
    if language == UiLanguage::English {
        normalized == "en" || normalized.starts_with("en-")
    } else {
        UiLanguage::from_config(&normalized) == language
    }
}

fn option_value_text(
    definition: &AddonMediaListOptionDefinition,
    value: &AddonMediaListOptionValue,
    language: UiLanguage,
) -> String {
    match value {
        AddonMediaListOptionValue::Boolean(true) => text(language, TextKey::Yes).to_owned(),
        AddonMediaListOptionValue::Boolean(false) => text(language, TextKey::No).to_owned(),
        AddonMediaListOptionValue::Enum(value) => definition
            .choices
            .iter()
            .find(|choice| choice.value == *value)
            .and_then(|choice| localized_option_text(&choice.label, language))
            .map_or_else(|| value.replace('_', " "), str::to_owned),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::localized_option_text;
    use crate::admin::i18n::UiLanguage;

    #[test]
    fn option_copy_falls_back_to_english_then_first_declared_locale() {
        let localized = BTreeMap::from([
            ("en-US".to_owned(), "English".to_owned()),
            ("nl-NL".to_owned(), "Nederlands".to_owned()),
        ]);
        assert_eq!(
            localized_option_text(&localized, UiLanguage::Dutch),
            Some("Nederlands")
        );
        assert_eq!(
            localized_option_text(&localized, UiLanguage::from_config("de-DE")),
            Some("English")
        );

        let first_only = BTreeMap::from([("fr-FR".to_owned(), "Français".to_owned())]);
        assert_eq!(
            localized_option_text(&first_only, UiLanguage::from_config("de-DE")),
            Some("Français")
        );
    }
}
