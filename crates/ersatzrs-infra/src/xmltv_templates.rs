//! XMLTV `<programme>`/`<channel>` rendering via user-customizable templates.
//!
//! Ports ErsatzTV's Scriban (`.sbntxt`) channel-guide template system
//! (`ErsatzTV/Resources/Templates/_*.sbntxt`, extracted at startup into
//! `FileSystemLayout.ChannelGuideTemplatesFolder`) onto `minijinja` — the
//! pattern, not the syntax, per the porting workflow. Context variable names
//! mirror ErsatzTV's own (`movie_title`, `movie_plot`, `show_title`, ...) so
//! an existing ErsatzTV custom template needs only trivial syntax
//! translation (`{{ if x }}` -> `{% if x %}`, `{{ end }}` -> `{% endif %}`/
//! `{% endfor %}`, `{{ for x in y }}` -> `{% for x in y %}`).
//!
//! Unlike ErsatzTV, bundled defaults are compiled into the binary and served
//! directly rather than extracted to disk at startup, mirroring the
//! `music_video_credits` module's existing template convention in this
//! codebase: a custom `movie.jinja` in [`channel_guide_templates_folder`]
//! overrides the bundled `_movie.jinja` when present, with no extraction
//! step required for a fresh install to render the bundled defaults.
//!
//! Five deliberate deviations from ErsatzTV's own templates:
//! - `<rating>` splitting (the `/`-delimited multi-value `ContentRating`
//!   convention, `us:` prefix -> `MPAA`/`VCHIP` system) is resolved in Rust
//!   ([`split_ratings`]) rather than via Scriban's `string.split`/
//!   `string.replace` filter pipeline, since minijinja does not ship an
//!   equivalent filter set; templates just loop a pre-built list.
//! - The episode template drops ErsatzTV's malformed `<image type="poster">`
//!   element: the upstream template has a stray trailing quote
//!   (`{{ episode_artwork_url }}"`) that emits malformed XML. The `<icon>`
//!   carries the same show artwork, and the valid episode still is retained.
//! - Movie icons fall back to thumbnail artwork when no poster exists, giving
//!   guide clients a usable image instead of an empty artwork slot.
//! - The bundled Remote Stream template emits persisted genres that ErsatzTV
//!   exposes to custom templates but omits from its own bundled template.
//! - The Remote Stream template also emits provider publication date,
//!   categories, Kijkwijzer rating, and credits when a rich playlist add-on
//!   supplied them. ErsatzTV's YAML contract has no corresponding fields.

use std::path::PathBuf;

use ersatzrs_core::xmltv::{XmltvItemKind, XmltvProgrammeContent};
use minijinja::AutoEscape;
use serde::Serialize;

use crate::file_system_layout::data_path;

const EXTENSION: &str = "jinja";

const BUILTIN_CHANNEL_GUIDE_TEMPLATES: &[(&str, &str)] = &[
    (
        "movie",
        include_str!("../../../assets/templates/channel-guide/_movie.jinja"),
    ),
    (
        "episode",
        include_str!("../../../assets/templates/channel-guide/_episode.jinja"),
    ),
    (
        "musicVideo",
        include_str!("../../../assets/templates/channel-guide/_musicVideo.jinja"),
    ),
    (
        "song",
        include_str!("../../../assets/templates/channel-guide/_song.jinja"),
    ),
    (
        "otherVideo",
        include_str!("../../../assets/templates/channel-guide/_otherVideo.jinja"),
    ),
    (
        "remoteStream",
        include_str!("../../../assets/templates/channel-guide/_remoteStream.jinja"),
    ),
    (
        "channel",
        include_str!("../../../assets/templates/channel-guide/_channel.jinja"),
    ),
];

/// `<data>/templates/channel-guide` — user-editable XMLTV template
/// overrides. Mirrors `FileSystemLayout.ChannelGuideTemplatesFolder`.
#[must_use]
pub fn channel_guide_templates_folder() -> PathBuf {
    data_path(std::path::Path::new("templates").join("channel-guide"))
}

#[derive(Debug, thiserror::Error)]
pub enum ChannelGuideTemplateError {
    #[error("failed to read channel guide template: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to render channel guide template: {0}")]
    Render(#[from] minijinja::Error),
}

/// The seven per-kind templates (custom override, else bundled default),
/// compiled once per guide render and reused across every channel/programme
/// in that render — recompiling per item would scale with schedule size
/// instead of staying constant per request.
pub struct ChannelGuideTemplates {
    env: minijinja::Environment<'static>,
}

/// Load all seven channel-guide templates, preferring a custom override
/// file (`<name>.jinja` in [`channel_guide_templates_folder`]) over the
/// bundled default when present.
///
/// # Errors
///
/// Returns an I/O error when a custom override file exists but cannot be
/// read (a missing file is not an error — the bundled default is used), or
/// a template syntax error for a malformed custom override.
pub async fn load_channel_guide_templates(
) -> Result<ChannelGuideTemplates, ChannelGuideTemplateError> {
    load_channel_guide_templates_in(&channel_guide_templates_folder()).await
}

/// Like [`load_channel_guide_templates`] but reading overrides from an
/// explicit folder, for tests that must not touch the real app-data folder.
///
/// # Errors
///
/// See [`load_channel_guide_templates`].
pub async fn load_channel_guide_templates_in(
    folder: &std::path::Path,
) -> Result<ChannelGuideTemplates, ChannelGuideTemplateError> {
    let mut env = minijinja::Environment::new();
    // The registered names intentionally omit a `.xml` suffix, so MiniJinja's
    // default callback would not escape them. Every template renders XML and
    // must escape metadata and provider URLs before fragment validation.
    env.set_auto_escape_callback(|_| AutoEscape::Html);
    for name in [
        "movie",
        "episode",
        "musicVideo",
        "song",
        "otherVideo",
        "remoteStream",
        "channel",
    ] {
        let (source, is_custom) = load_channel_guide_template(folder, name).await?;
        if is_custom {
            // A broken custom override degrades to the bundled default for
            // that one kind rather than failing the entire guide request.
            if let Err(error) = env.add_template_owned(name, source) {
                tracing::warn!(
                    %error,
                    template = name,
                    "custom channel guide template failed to compile; using the bundled default"
                );
                env.add_template_owned(name, bundled_default(name))?;
            }
        } else {
            env.add_template_owned(name, source)?;
        }
    }
    Ok(ChannelGuideTemplates { env })
}

fn bundled_default(name: &str) -> &'static str {
    BUILTIN_CHANNEL_GUIDE_TEMPLATES
        .iter()
        .find_map(|(candidate, template)| (*candidate == name).then_some(*template))
        .unwrap_or_default()
}

/// Returns the loaded template source and whether it came from a custom
/// override file (`true`) or the bundled default (`false`).
async fn load_channel_guide_template(
    folder: &std::path::Path,
    name: &str,
) -> Result<(String, bool), ChannelGuideTemplateError> {
    let custom_path = folder.join(format!("{name}.{EXTENSION}"));
    match tokio::fs::read_to_string(&custom_path).await {
        Ok(template) => return Ok((template, true)),
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.into()),
        Err(_) => {}
    }
    Ok((bundled_default(name).to_owned(), false))
}

/// A single `<rating>` element's resolved `system` attribute and value,
/// after splitting a `/`-delimited multi-value `ContentRating` string and
/// mapping `us:`-prefixed segments to the given rating system.
#[derive(Debug, Clone, Serialize)]
pub struct RatingEntry {
    pub system: Option<String>,
    pub value: String,
}

/// Split a `/`-delimited multi-value `ContentRating` string into
/// `<rating>` entries. `case_insensitive` matches ErsatzTV's episode
/// template, which downcases before checking the `us:` prefix (movie/
/// other-video/remote-stream check the exact-case prefix).
#[must_use]
pub fn split_ratings(
    raw: Option<&str>,
    system_name: &str,
    case_insensitive: bool,
) -> Vec<RatingEntry> {
    let Some(raw) = raw else {
        return Vec::new();
    };
    raw.split('/')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            let is_us = if case_insensitive {
                segment
                    .get(..3)
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case("us:"))
            } else {
                segment.starts_with("us:")
            };
            if is_us {
                RatingEntry {
                    system: Some(system_name.to_owned()),
                    value: segment[3..].to_owned(),
                }
            } else if segment
                .get(..3)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("nl:"))
            {
                RatingEntry {
                    system: Some("Kijkwijzer".to_owned()),
                    value: segment[3..].to_owned(),
                }
            } else {
                RatingEntry {
                    system: None,
                    value: segment.to_owned(),
                }
            }
        })
        .collect()
}

/// Pre-resolved input to [`render_programme`]. Artwork paths must already
/// be resolved to servable URLs (via `ersatzrs-web`'s `api::artwork`) —
/// this crate has no knowledge of HTTP routes.
pub struct RenderProgrammeInput<'a> {
    pub programme_start: &'a str,
    pub programme_stop: &'a str,
    pub channel_id: &'a str,
    pub channel_number: &'a str,
    pub custom_title: Option<&'a str>,
    pub title: &'a str,
    pub content: &'a XmltvProgrammeContent,
    pub icon_url: Option<&'a str>,
    pub thumbnail_url: Option<&'a str>,
}

/// Pre-resolved channel values exposed to the channel template. The raw
/// artwork fields preserve ErsatzTV's customization context while
/// `channel_icon_url` lets the bundled template use the already resolved URL.
pub struct RenderChannelInput<'a> {
    pub channel_id: &'a str,
    pub channel_number: &'a str,
    pub channel_name: &'a str,
    pub channel_categories: &'a [String],
    pub channel_artwork_path: Option<&'a str>,
    pub channel_name_encoded: &'a str,
    pub channel_icon_url: &'a str,
}

/// Render a single `<programme>` element (without its surrounding
/// whitespace being guaranteed — callers should treat the result as an XML
/// fragment to be parsed, not concatenated blindly).
///
/// # Errors
///
/// Returns a template syntax/evaluation error, which can only happen for a
/// user-supplied custom override (the bundled defaults are covered by
/// tests).
#[allow(clippy::too_many_lines)] // one match arm per XmltvItemKind
pub fn render_programme(
    templates: &ChannelGuideTemplates,
    input: &RenderProgrammeInput<'_>,
) -> Result<String, ChannelGuideTemplateError> {
    let RenderProgrammeInput {
        programme_start,
        programme_stop,
        channel_id,
        channel_number,
        custom_title,
        title,
        content,
        icon_url,
        thumbnail_url,
    } = *input;

    let common = ProgrammeContext {
        programme_start,
        programme_stop,
        channel_id,
        channel_id_legacy: format!("{channel_number}.etv"),
        channel_number,
        has_custom_title: custom_title.is_some_and(|value| !value.trim().is_empty()),
        custom_title,
    };

    match content.kind {
        XmltvItemKind::Movie => render(
            templates,
            "movie",
            &MovieContext {
                common,
                movie_title: title,
                movie_has_plot: content.description.is_some(),
                movie_plot: content.description.as_deref(),
                movie_has_year: content.year.is_some(),
                movie_year: content.year,
                movie_genres: &content.genres,
                movie_has_artwork: icon_url.is_some(),
                movie_artwork_url: icon_url,
                movie_has_content_rating: content.content_rating.is_some(),
                movie_content_rating: content.content_rating.as_deref(),
                movie_ratings: split_ratings(content.content_rating.as_deref(), "MPAA", false),
                movie_guids: &content.guids,
            },
        ),
        XmltvItemKind::Episode => render(
            templates,
            "episode",
            &EpisodeContext {
                common,
                show_title: title,
                episode_has_title: content.sub_title.is_some(),
                episode_title: content.sub_title.as_deref(),
                episode_has_plot: content.description.is_some(),
                episode_plot: content.description.as_deref(),
                show_has_year: content.year.is_some(),
                show_year: content.year,
                show_genres: &content.genres,
                episode_has_artwork: icon_url.is_some(),
                episode_artwork_url: icon_url,
                episode_has_thumbnail: thumbnail_url.is_some(),
                episode_thumbnail_url: thumbnail_url,
                season_number: content.season_number.unwrap_or(0),
                episode_number: content.episode_number.unwrap_or(0),
                season_number_padded: format!("{:02}", content.season_number.unwrap_or(0)),
                episode_number_padded: format!("{:02}", content.episode_number.unwrap_or(0)),
                show_has_content_rating: content.content_rating.is_some(),
                show_content_rating: content.content_rating.as_deref(),
                show_ratings: split_ratings(content.content_rating.as_deref(), "VCHIP", true),
                show_guids: &content.guids,
                episode_guids: &content.episode_guids,
            },
        ),
        XmltvItemKind::MusicVideo => render(
            templates,
            "musicVideo",
            &MusicVideoContext {
                common,
                artist_title: title,
                music_video_title: content.sub_title.as_deref(),
                music_video_has_plot: content.description.is_some(),
                music_video_plot: content.description.as_deref(),
                music_video_has_year: content.year.is_some(),
                music_video_year: content.year,
                music_video_genres: &content.genres,
                artist_genres: &content.artist_genres,
                music_video_has_artwork: icon_url.is_some(),
                music_video_artwork_url: icon_url,
                music_video_has_track: content.track.is_some(),
                music_video_track: content.track.as_deref(),
                music_video_has_album: content.album.is_some(),
                music_video_album: content.album.as_deref(),
                music_video_has_release_date: content.release_date.is_some(),
                music_video_release_date: content.release_date.as_deref(),
                music_video_all_artists: &content.artists,
                music_video_studios: &content.studios,
                music_video_directors: &content.directors,
            },
        ),
        XmltvItemKind::Song => render(
            templates,
            "song",
            &SongContext {
                common,
                song_artists: &content.artists,
                song_album_artists: &content.album_artists,
                song_title: content.sub_title.as_deref(),
                song_has_year: content.year.is_some(),
                song_year: content.year,
                song_genres: &content.genres,
                song_has_artwork: icon_url.is_some(),
                song_artwork_url: icon_url,
                song_has_track: content.track.is_some(),
                song_track: content.track.as_deref(),
                song_has_comment: content.comment.is_some(),
                song_comment: content.comment.as_deref(),
                song_has_album: content.album.is_some(),
                song_album: content.album.as_deref(),
                song_has_release_date: content.release_date.is_some(),
                song_release_date: content.release_date.as_deref(),
                song_studios: &content.studios,
            },
        ),
        XmltvItemKind::OtherVideo => render(
            templates,
            "otherVideo",
            &OtherVideoContext {
                common,
                other_video_title: title,
                other_video_has_plot: content.description.is_some(),
                other_video_plot: content.description.as_deref(),
                other_video_has_year: content.year.is_some(),
                other_video_year: content.year,
                other_video_genres: &content.genres,
                other_video_has_artwork: icon_url.is_some(),
                other_video_artwork_url: icon_url,
                other_video_has_content_rating: content.content_rating.is_some(),
                other_video_content_rating: content.content_rating.as_deref(),
                other_video_ratings: split_ratings(
                    content.content_rating.as_deref(),
                    "MPAA",
                    false,
                ),
            },
        ),
        XmltvItemKind::RemoteStream => render(
            templates,
            "remoteStream",
            &RemoteStreamContext {
                common,
                remote_stream_title: title,
                remote_stream_has_plot: content.description.is_some(),
                remote_stream_plot: content.description.as_deref(),
                remote_stream_has_year: content.year.is_some(),
                remote_stream_year: content.year,
                remote_stream_has_release_date: xmltv_date(content.release_date.as_deref())
                    .is_some(),
                remote_stream_release_date: xmltv_date(content.release_date.as_deref()),
                remote_stream_categories: remote_stream_categories(content),
                remote_stream_collection: content.collection.as_deref(),
                remote_stream_people: content
                    .people
                    .iter()
                    .map(|person| CreditEntry {
                        name: &person.name,
                        role: &person.role,
                    })
                    .collect(),
                remote_stream_original_broadcasters: &content.original_broadcasters,
                remote_stream_broadcasters: &content.broadcasters,
                remote_stream_has_artwork: icon_url.is_some(),
                remote_stream_artwork_url: icon_url,
                remote_stream_has_content_rating: content.content_rating.is_some(),
                remote_stream_content_rating: content.content_rating.as_deref(),
                remote_stream_ratings: split_ratings(
                    content.content_rating.as_deref(),
                    "MPAA",
                    false,
                ),
            },
        ),
    }
}

fn xmltv_date(value: Option<&str>) -> Option<String> {
    value
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.format("%Y%m%d").to_string())
}

fn remote_stream_categories(content: &XmltvProgrammeContent) -> Vec<String> {
    let mut categories = Vec::new();
    for value in content
        .genres
        .iter()
        .chain(content.tags.iter())
        .chain(content.collection.iter())
    {
        if !value.trim().is_empty()
            && !categories
                .iter()
                .any(|existing: &String| existing.eq_ignore_ascii_case(value))
        {
            categories.push(value.clone());
        }
    }
    categories
}

/// Render the `<channel>` element.
///
/// # Errors
///
/// Returns a template syntax/evaluation error (see [`render_programme`]).
pub fn render_channel(
    templates: &ChannelGuideTemplates,
    input: &RenderChannelInput<'_>,
) -> Result<String, ChannelGuideTemplateError> {
    let artwork_path = input.channel_artwork_path;
    render(
        templates,
        "channel",
        &ChannelContext {
            channel_id: input.channel_id,
            channel_id_legacy: format!("{}.etv", input.channel_number),
            channel_number: input.channel_number,
            channel_name: input.channel_name,
            channel_categories: input.channel_categories,
            channel_has_artwork: artwork_path.is_some(),
            channel_has_external_artwork: artwork_path
                .is_some_and(|path| path.starts_with("http://") || path.starts_with("https://")),
            channel_artwork_path: artwork_path,
            channel_name_encoded: input.channel_name_encoded,
            channel_icon_url: input.channel_icon_url,
        },
    )
}

fn render<S: Serialize>(
    templates: &ChannelGuideTemplates,
    name: &str,
    context: &S,
) -> Result<String, ChannelGuideTemplateError> {
    Ok(templates.env.get_template(name)?.render(context)?)
}

#[derive(Serialize)]
struct ProgrammeContext<'a> {
    programme_start: &'a str,
    programme_stop: &'a str,
    channel_id: &'a str,
    channel_id_legacy: String,
    channel_number: &'a str,
    has_custom_title: bool,
    custom_title: Option<&'a str>,
}

#[derive(Serialize)]
#[allow(clippy::struct_excessive_bools)] // exact ErsatzTV template presence flags
struct MovieContext<'a> {
    #[serde(flatten)]
    common: ProgrammeContext<'a>,
    movie_title: &'a str,
    movie_has_plot: bool,
    movie_plot: Option<&'a str>,
    movie_has_year: bool,
    movie_year: Option<i32>,
    movie_genres: &'a [String],
    movie_has_artwork: bool,
    movie_artwork_url: Option<&'a str>,
    movie_has_content_rating: bool,
    movie_content_rating: Option<&'a str>,
    movie_ratings: Vec<RatingEntry>,
    movie_guids: &'a [String],
}

#[derive(Serialize)]
#[allow(clippy::struct_excessive_bools)] // exact ErsatzTV template presence flags
struct EpisodeContext<'a> {
    #[serde(flatten)]
    common: ProgrammeContext<'a>,
    show_title: &'a str,
    episode_has_title: bool,
    episode_title: Option<&'a str>,
    episode_has_plot: bool,
    episode_plot: Option<&'a str>,
    show_has_year: bool,
    show_year: Option<i32>,
    show_genres: &'a [String],
    episode_has_artwork: bool,
    episode_artwork_url: Option<&'a str>,
    episode_has_thumbnail: bool,
    episode_thumbnail_url: Option<&'a str>,
    season_number: i32,
    episode_number: i32,
    season_number_padded: String,
    episode_number_padded: String,
    show_has_content_rating: bool,
    show_content_rating: Option<&'a str>,
    show_ratings: Vec<RatingEntry>,
    show_guids: &'a [String],
    episode_guids: &'a [String],
}

#[derive(Serialize)]
#[allow(clippy::struct_excessive_bools)] // exact ErsatzTV template presence flags
struct MusicVideoContext<'a> {
    #[serde(flatten)]
    common: ProgrammeContext<'a>,
    artist_title: &'a str,
    music_video_title: Option<&'a str>,
    music_video_has_plot: bool,
    music_video_plot: Option<&'a str>,
    music_video_has_year: bool,
    music_video_year: Option<i32>,
    music_video_genres: &'a [String],
    artist_genres: &'a [String],
    music_video_has_artwork: bool,
    music_video_artwork_url: Option<&'a str>,
    music_video_has_track: bool,
    music_video_track: Option<&'a str>,
    music_video_has_album: bool,
    music_video_album: Option<&'a str>,
    music_video_has_release_date: bool,
    music_video_release_date: Option<&'a str>,
    music_video_all_artists: &'a [String],
    music_video_studios: &'a [String],
    music_video_directors: &'a [String],
}

#[derive(Serialize)]
#[allow(clippy::struct_excessive_bools)] // exact ErsatzTV template presence flags
struct SongContext<'a> {
    #[serde(flatten)]
    common: ProgrammeContext<'a>,
    song_artists: &'a [String],
    song_album_artists: &'a [String],
    song_title: Option<&'a str>,
    song_has_year: bool,
    song_year: Option<i32>,
    song_genres: &'a [String],
    song_has_artwork: bool,
    song_artwork_url: Option<&'a str>,
    song_has_track: bool,
    song_track: Option<&'a str>,
    song_has_comment: bool,
    song_comment: Option<&'a str>,
    song_has_album: bool,
    song_album: Option<&'a str>,
    song_has_release_date: bool,
    song_release_date: Option<&'a str>,
    song_studios: &'a [String],
}

#[derive(Serialize)]
#[allow(clippy::struct_excessive_bools)] // exact ErsatzTV template presence flags
struct OtherVideoContext<'a> {
    #[serde(flatten)]
    common: ProgrammeContext<'a>,
    other_video_title: &'a str,
    other_video_has_plot: bool,
    other_video_plot: Option<&'a str>,
    other_video_has_year: bool,
    other_video_year: Option<i32>,
    other_video_genres: &'a [String],
    other_video_has_artwork: bool,
    other_video_artwork_url: Option<&'a str>,
    other_video_has_content_rating: bool,
    other_video_content_rating: Option<&'a str>,
    other_video_ratings: Vec<RatingEntry>,
}

#[derive(Serialize)]
#[allow(clippy::struct_excessive_bools)] // exact ErsatzTV template presence flags
struct RemoteStreamContext<'a> {
    #[serde(flatten)]
    common: ProgrammeContext<'a>,
    remote_stream_title: &'a str,
    remote_stream_has_plot: bool,
    remote_stream_plot: Option<&'a str>,
    remote_stream_has_year: bool,
    remote_stream_year: Option<i32>,
    remote_stream_has_release_date: bool,
    remote_stream_release_date: Option<String>,
    remote_stream_categories: Vec<String>,
    remote_stream_collection: Option<&'a str>,
    remote_stream_people: Vec<CreditEntry<'a>>,
    remote_stream_original_broadcasters: &'a [String],
    remote_stream_broadcasters: &'a [String],
    remote_stream_has_artwork: bool,
    remote_stream_artwork_url: Option<&'a str>,
    remote_stream_has_content_rating: bool,
    remote_stream_content_rating: Option<&'a str>,
    remote_stream_ratings: Vec<RatingEntry>,
}

#[derive(Serialize)]
struct CreditEntry<'a> {
    name: &'a str,
    role: &'a str,
}

#[derive(Serialize)]
#[allow(clippy::struct_field_names)] // field names are the ErsatzTV template variable names
struct ChannelContext<'a> {
    channel_id: &'a str,
    channel_id_legacy: String,
    channel_number: &'a str,
    channel_name: &'a str,
    channel_categories: &'a [String],
    channel_has_artwork: bool,
    channel_has_external_artwork: bool,
    channel_artwork_path: Option<&'a str>,
    channel_name_encoded: &'a str,
    channel_icon_url: &'a str,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ersatzrs_core::xmltv::XmltvCredit;

    fn content(kind: XmltvItemKind) -> XmltvProgrammeContent {
        XmltvProgrammeContent {
            kind,
            ..XmltvProgrammeContent::default()
        }
    }

    async fn load_templates_from_empty_temp_folder() -> ChannelGuideTemplates {
        let root = tempfile::tempdir().expect("temp templates root");
        load_channel_guide_templates_in(root.path())
            .await
            .expect("load bundled templates")
    }

    #[tokio::test]
    async fn bundled_defaults_load_without_a_custom_override() {
        let templates = load_templates_from_empty_temp_folder().await;
        let source = |name: &str| {
            templates
                .env
                .get_template(name)
                .unwrap()
                .source()
                .to_owned()
        };
        assert!(source("movie").contains("<title"));
        assert!(source("episode").contains("episode-num"));
        assert!(source("channel").contains("display-name"));
    }

    #[test]
    fn split_ratings_maps_us_prefix_to_the_given_system() {
        let entries = split_ratings(Some("us:PG-13/de:12"), "MPAA", false);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].system.as_deref(), Some("MPAA"));
        assert_eq!(entries[0].value, "PG-13");
        assert_eq!(entries[1].system, None);
        assert_eq!(entries[1].value, "de:12");
    }

    #[test]
    fn split_ratings_maps_nl_prefix_to_kijkwijzer() {
        let entries = split_ratings(Some("nl:12"), "MPAA", false);
        assert_eq!(entries[0].system.as_deref(), Some("Kijkwijzer"));
        assert_eq!(entries[0].value, "12");
    }

    #[test]
    fn split_ratings_case_insensitive_matches_upper_case_us_prefix() {
        let entries = split_ratings(Some("US:TV-14"), "VCHIP", true);
        assert_eq!(entries[0].system.as_deref(), Some("VCHIP"));
        assert_eq!(entries[0].value, "TV-14");

        let entries = split_ratings(Some("US:TV-14"), "MPAA", false);
        assert_eq!(entries[0].system, None);
        assert_eq!(entries[0].value, "US:TV-14");
    }

    #[test]
    fn split_ratings_of_none_is_empty() {
        assert!(split_ratings(None, "MPAA", false).is_empty());
    }

    #[test]
    fn split_ratings_handles_multibyte_prefixes_without_panicking() {
        let entries = split_ratings(Some("éé/us:TV-14"), "VCHIP", true);
        assert_eq!(entries[0].value, "éé");
        assert_eq!(entries[0].system, None);
        assert_eq!(entries[1].value, "TV-14");
        assert_eq!(entries[1].system.as_deref(), Some("VCHIP"));
    }

    #[tokio::test]
    async fn render_movie_includes_plot_genres_and_rating() {
        let templates = load_templates_from_empty_temp_folder().await;
        let movie_content = XmltvProgrammeContent {
            description: Some("A large rabbit has a larger problem.".to_owned()),
            year: Some(2008),
            genres: vec!["Animation".to_owned(), "Short".to_owned()],
            content_rating: Some("us:G".to_owned()),
            ..content(XmltvItemKind::Movie)
        };
        let input = RenderProgrammeInput {
            programme_start: "20260417200000 +0000",
            programme_stop: "20260417210000 +0000",
            channel_id: "C1.145.ersatztv.org",
            channel_number: "1",
            custom_title: None,
            title: "Big Buck Bunny",
            content: &movie_content,
            icon_url: Some("/artwork/posters/bbb.jpg"),
            thumbnail_url: None,
        };

        let xml = render_programme(&templates, &input).expect("render movie");

        assert!(xml.contains("<title lang=\"en\">Big Buck Bunny</title>"));
        assert!(xml.contains("<desc lang=\"en\">A large rabbit has a larger problem.</desc>"));
        assert!(xml.contains("<date>2008</date>"));
        assert!(xml.contains("<category lang=\"en\">Movie</category>"));
        assert!(xml.contains("<category lang=\"en\">Animation</category>"));
        assert!(xml.contains("<icon src=\"&#x2f;artwork&#x2f;posters&#x2f;bbb.jpg\" />"));
        assert!(xml.contains("<rating system=\"MPAA\"><value>G</value></rating>"));
        assert!(xml.contains("<previously-shown />"));
    }

    #[tokio::test]
    async fn render_movie_omits_optional_elements_when_absent() {
        let templates = load_templates_from_empty_temp_folder().await;
        let movie_content = content(XmltvItemKind::Movie);
        let input = RenderProgrammeInput {
            programme_start: "20260417200000 +0000",
            programme_stop: "20260417210000 +0000",
            channel_id: "C1.145.ersatztv.org",
            channel_number: "1",
            custom_title: None,
            title: "Unknown",
            content: &movie_content,
            icon_url: None,
            thumbnail_url: None,
        };

        let xml = render_programme(&templates, &input).expect("render movie");

        assert!(!xml.contains("<desc"));
        assert!(!xml.contains("<date>"));
        assert!(!xml.contains("<icon"));
        assert!(!xml.contains("<rating"));
    }

    #[tokio::test]
    async fn render_programme_xml_escapes_metadata_and_provider_urls() {
        let templates = load_templates_from_empty_temp_folder().await;
        let movie_content = XmltvProgrammeContent {
            description: Some("Fish & Chips <Tonight>".to_owned()),
            ..content(XmltvItemKind::Movie)
        };
        let input = RenderProgrammeInput {
            programme_start: "20260417200000 +0000",
            programme_stop: "20260417210000 +0000",
            channel_id: "C1.145.ersatztv.org",
            channel_number: "1",
            custom_title: None,
            title: "Rock & Roll",
            content: &movie_content,
            icon_url: Some("https://media.example/item?tag=a&fillHeight=440"),
            thumbnail_url: None,
        };

        let xml = render_programme(&templates, &input).expect("render escaped movie");

        assert!(xml.contains("Rock &amp; Roll"));
        assert!(xml.contains("Fish &amp; Chips &lt;Tonight&gt;"));
        assert!(xml.contains("tag=a&amp;fillHeight=440"));
    }

    #[tokio::test]
    async fn bundled_template_honors_custom_title() {
        let templates = load_templates_from_empty_temp_folder().await;
        let movie_content = content(XmltvItemKind::Movie);
        let input = RenderProgrammeInput {
            programme_start: "",
            programme_stop: "",
            channel_id: "C1.145.ersatztv.org",
            channel_number: "1",
            custom_title: Some("Friday & Friends"),
            title: "Original Movie",
            content: &movie_content,
            icon_url: None,
            thumbnail_url: None,
        };

        let xml = render_programme(&templates, &input).expect("render custom title");

        assert!(xml.contains("<title lang=\"en\">Friday &amp; Friends</title>"));
        assert!(!xml.contains("Original Movie"));
    }

    #[tokio::test]
    async fn render_episode_uses_show_title_and_zero_pads_episode_num() {
        let templates = load_templates_from_empty_temp_folder().await;
        let episode_content = XmltvProgrammeContent {
            sub_title: Some("Pilot".to_owned()),
            genres: vec!["Drama".to_owned()],
            content_rating: Some("us:TV-14".to_owned()),
            season_number: Some(1),
            episode_number: Some(3),
            ..content(XmltvItemKind::Episode)
        };
        let input = RenderProgrammeInput {
            programme_start: "20260417200000 +0000",
            programme_stop: "20260417203000 +0000",
            channel_id: "C1.145.ersatztv.org",
            channel_number: "1",
            custom_title: None,
            title: "Some Show",
            content: &episode_content,
            icon_url: None,
            thumbnail_url: None,
        };

        let xml = render_programme(&templates, &input).expect("render episode");

        assert!(xml.contains("<title lang=\"en\">Some Show</title>"));
        assert!(xml.contains("<sub-title lang=\"en\">Pilot</sub-title>"));
        assert!(xml.contains("<category lang=\"en\">Series</category>"));
        assert!(xml.contains("<episode-num system=\"onscreen\">S01E03</episode-num>"));
        assert!(xml.contains("<episode-num system=\"xmltv_ns\">0.2.0/1</episode-num>"));
        assert!(xml.contains("<rating system=\"VCHIP\"><value>TV-14</value></rating>"));
    }

    #[tokio::test]
    async fn bundled_remote_stream_template_emits_rich_consumer_metadata() {
        let templates = load_templates_from_empty_temp_folder().await;
        let remote_stream_content = XmltvProgrammeContent {
            genres: vec!["Documentary".to_owned(), "Culture".to_owned()],
            tags: vec!["Television".to_owned()],
            collection: Some("Public broadcasters".to_owned()),
            release_date: Some("1992-05-30T21:15:00Z".to_owned()),
            content_rating: Some("nl:12".to_owned()),
            people: vec![
                XmltvCredit {
                    name: "Gert-Jan Dröge".to_owned(),
                    role: "presenter".to_owned(),
                },
                XmltvCredit {
                    name: "Richard Rietveld".to_owned(),
                    role: "director".to_owned(),
                },
                XmltvCredit {
                    name: "John de Mol Produkties".to_owned(),
                    role: "producer".to_owned(),
                },
            ],
            original_broadcasters: vec!["AVRO".to_owned()],
            broadcasters: vec!["AVROTROS".to_owned()],
            ..content(XmltvItemKind::RemoteStream)
        };
        let input = RenderProgrammeInput {
            programme_start: "20260417200000 +0000",
            programme_stop: "20260417210000 +0000",
            channel_id: "C1.145.ersatztv.org",
            channel_number: "1",
            custom_title: None,
            title: "Live Museum Stream",
            content: &remote_stream_content,
            icon_url: None,
            thumbnail_url: None,
        };

        let xml = render_programme(&templates, &input).expect("render remote stream");

        assert!(xml.contains("<category lang=\"en\">Documentary</category>"));
        assert!(xml.contains("<category lang=\"en\">Culture</category>"));
        assert!(xml.contains("<category lang=\"en\">Television</category>"));
        assert!(xml.contains("<category lang=\"en\">Public broadcasters</category>"));
        assert!(xml.contains("<date>19920530</date>"));
        assert!(xml.contains("<presenter>Gert-Jan Dröge</presenter>"));
        assert!(xml.contains("<director>Richard Rietveld</director>"));
        assert!(xml.contains("<producer>John de Mol Produkties</producer>"));
        assert!(xml.contains("<rating system=\"Kijkwijzer\"><value>12</value></rating>"));
    }

    #[tokio::test]
    async fn render_channel_includes_three_display_name_variants() {
        let templates = load_templates_from_empty_temp_folder().await;
        let categories = ["Film".to_owned()];
        let xml = render_channel(
            &templates,
            &RenderChannelInput {
                channel_id: "C1.145.ersatztv.org",
                channel_number: "1",
                channel_name: "Movies",
                channel_categories: &categories,
                channel_artwork_path: Some("movies.png"),
                channel_name_encoded: "Movies",
                channel_icon_url: "/iptv/logos/movies.png",
            },
        )
        .expect("render channel");

        assert!(xml.contains("<display-name>1 Movies</display-name>"));
        assert!(xml.contains("<display-name>1</display-name>"));
        assert!(xml.contains("<display-name>Movies</display-name>"));
        assert!(xml.contains("<category lang=\"en\">Film</category>"));
        assert!(xml.contains("<icon src=\"&#x2f;iptv&#x2f;logos&#x2f;movies.png\" />"));
    }

    #[tokio::test]
    async fn custom_override_wins_over_bundled_default() {
        let root = tempfile::tempdir().expect("temp templates root");
        tokio::fs::write(root.path().join("movie.jinja"), "custom:{{ movie_title }}")
            .await
            .unwrap();

        let templates = load_channel_guide_templates_in(root.path()).await.unwrap();
        let movie_content = content(XmltvItemKind::Movie);
        let input = RenderProgrammeInput {
            programme_start: "",
            programme_stop: "",
            channel_id: "",
            channel_number: "",
            custom_title: None,
            title: "Big Buck Bunny",
            content: &movie_content,
            icon_url: None,
            thumbnail_url: None,
        };
        let xml = render_programme(&templates, &input).expect("render custom movie");

        assert_eq!(xml, "custom:Big Buck Bunny");
    }

    #[tokio::test]
    async fn custom_overrides_receive_the_full_ersatztv_context() {
        let root = tempfile::tempdir().expect("temp templates root");
        tokio::fs::write(
            root.path().join("episode.jinja"),
            "{{ channel_id_legacy }}|{{ has_custom_title }}|{{ custom_title }}|{{ show_has_year }}|{{ show_year }}|{{ episode_has_thumbnail }}|{{ episode_thumbnail_url }}|{{ show_guids | join(',') }}|{{ episode_guids | join(',') }}",
        )
        .await
        .unwrap();
        tokio::fs::write(
            root.path().join("musicVideo.jinja"),
            "{{ music_video_has_track }}|{{ music_video_track }}|{{ music_video_album }}|{{ music_video_release_date }}|{{ music_video_all_artists | join(',') }}|{{ music_video_studios | join(',') }}|{{ music_video_directors | join(',') }}",
        )
        .await
        .unwrap();
        let templates = load_channel_guide_templates_in(root.path()).await.unwrap();

        let episode_content = XmltvProgrammeContent {
            kind: XmltvItemKind::Episode,
            year: Some(2026),
            guids: vec!["show-guid".to_owned()],
            episode_guids: vec!["episode-guid".to_owned()],
            ..XmltvProgrammeContent::default()
        };
        let episode = render_programme(
            &templates,
            &RenderProgrammeInput {
                programme_start: "",
                programme_stop: "",
                channel_id: "C7.ersatztv.org",
                channel_number: "7",
                custom_title: Some("Special"),
                title: "Show",
                content: &episode_content,
                icon_url: Some("/show-poster.jpg"),
                thumbnail_url: Some("/episode-still.jpg"),
            },
        )
        .unwrap();
        assert_eq!(
            episode,
            "7.etv|true|Special|true|2026|true|&#x2f;episode-still.jpg|show-guid|episode-guid"
        );

        let music_content = XmltvProgrammeContent {
            kind: XmltvItemKind::MusicVideo,
            track: Some("4".to_owned()),
            album: Some("Album".to_owned()),
            release_date: Some("2026-08-02".to_owned()),
            artists: vec!["Artist A".to_owned(), "Artist B".to_owned()],
            studios: vec!["Studio".to_owned()],
            directors: vec!["Director".to_owned()],
            ..XmltvProgrammeContent::default()
        };
        let music_video = render_programme(
            &templates,
            &RenderProgrammeInput {
                programme_start: "",
                programme_stop: "",
                channel_id: "",
                channel_number: "7",
                custom_title: None,
                title: "Artist",
                content: &music_content,
                icon_url: None,
                thumbnail_url: None,
            },
        )
        .unwrap();
        assert_eq!(
            music_video,
            "true|4|Album|2026-08-02|Artist A,Artist B|Studio|Director"
        );
    }

    #[tokio::test]
    async fn malformed_custom_override_falls_back_to_the_bundled_default_for_that_kind_only() {
        let root = tempfile::tempdir().expect("temp templates root");
        tokio::fs::write(root.path().join("movie.jinja"), "{% if unterminated")
            .await
            .unwrap();

        let templates = load_channel_guide_templates_in(root.path())
            .await
            .expect("a broken custom override must not fail the whole load");
        let movie_content = content(XmltvItemKind::Movie);
        let input = RenderProgrammeInput {
            programme_start: "",
            programme_stop: "",
            channel_id: "",
            channel_number: "",
            custom_title: None,
            title: "Big Buck Bunny",
            content: &movie_content,
            icon_url: None,
            thumbnail_url: None,
        };

        let xml = render_programme(&templates, &input).expect("render falls back to default");

        assert!(xml.contains("<title lang=\"en\">Big Buck Bunny</title>"));
        // The other kinds must be unaffected by the broken movie override.
        let episode_content = content(XmltvItemKind::Episode);
        let episode_input = RenderProgrammeInput {
            title: "Some Show",
            content: &episode_content,
            ..input
        };
        let episode_xml =
            render_programme(&templates, &episode_input).expect("render unaffected episode");
        assert!(episode_xml.contains("<title lang=\"en\">Some Show</title>"));
    }
}
