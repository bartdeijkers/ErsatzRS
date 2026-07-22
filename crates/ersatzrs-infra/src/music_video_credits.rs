use crate::repositories::SqlxResultExt as _;
use std::fmt::Write as _;
use std::path::PathBuf;

use chrono::Duration;
use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_domain::{Channel, ChannelMusicVideoCreditsMode, Resolution};
use sqlx::Row;
use uuid::Uuid;

use crate::db::DbPool;
use crate::file_system_layout::{data_path, subtitle_cache_folder};
use crate::metadata::subtitle_kind;
use crate::playback::PlaybackSubtitle;
use crate::repositories::common::decode_duration_text;

const EXTENSION: &str = "sbntxt";
const BUILTIN_TEMPLATES: &[(&str, &str)] = &[
    (
        "_default.ass",
        include_str!("../../../assets/templates/_default.ass.sbntxt"),
    ),
    (
        "_ArtistTitle_LeftMiddle",
        include_str!("../../../assets/templates/_ArtistTitle_LeftMiddle.sbntxt"),
    ),
    (
        "_ArtistTitleAlbum_CenterTop",
        include_str!("../../../assets/templates/_ArtistTitleAlbum_CenterTop.sbntxt"),
    ),
];

#[derive(Debug, Clone, PartialEq)]
pub struct MusicVideoCreditsData {
    pub resolution: Resolution,
    pub title: String,
    pub track: Option<i32>,
    pub album: Option<String>,
    pub plot: Option<String>,
    pub release_date: Option<String>,
    pub all_artists: Vec<String>,
    pub artist: String,
    pub studios: Vec<String>,
    pub directors: Vec<String>,
    pub duration: Duration,
    pub stream_seek: Duration,
}

#[must_use]
pub fn music_video_credits_templates_folder() -> PathBuf {
    data_path(std::path::Path::new("templates").join("music-video-credits"))
}

/// List editable music-video credits templates plus ErsatzTV's bundled presets.
///
/// The bundled names mirror `ResourceExtractorService` in ErsatzTV, but are
/// exposed directly so a fresh ErsatzRS install does not depend on a prior
/// extraction step before the channel editor can see the presets.
///
/// # Errors
///
/// Returns a decode error when the editable template directory exists but
/// cannot be read.
pub async fn list_music_video_credit_template_names() -> Result<Vec<String>> {
    let mut names = BUILTIN_TEMPLATES
        .iter()
        .map(|(name, _)| (*name).to_owned())
        .collect::<Vec<_>>();
    let Ok(mut entries) = tokio::fs::read_dir(music_video_credits_templates_folder()).await else {
        names.sort();
        names.dedup();
        return Ok(names);
    };
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|error| RepositoryError::Decode {
            kind: "music video credits template list",
            value: error.to_string(),
        })?
    {
        if !entry
            .file_type()
            .await
            .map_err(|error| RepositoryError::Decode {
                kind: "music video credits template file type",
                value: error.to_string(),
            })?
            .is_file()
        {
            continue;
        }
        if entry.path().extension().and_then(|value| value.to_str()) != Some(EXTENSION) {
            continue;
        }
        if let Some(name) = entry
            .path()
            .file_stem()
            .and_then(|value| value.to_str())
            .map(ToOwned::to_owned)
        {
            names.push(name);
        }
    }
    names.sort();
    names.dedup();
    Ok(names)
}

/// Generate an ephemeral ASS subtitle for music-video credits when the channel
/// requests `GenerateSubtitles`.
///
/// Returns `None` for non-music-video media or channels with credits disabled,
/// matching ErsatzTV's behavior of only adding this generated subtitle to
/// music-video playback.
///
/// # Errors
///
/// Returns database errors from music-video metadata queries or decode errors
/// when template files or the generated subtitle cache cannot be read/written.
pub async fn generate_music_video_credits_subtitle(
    pool: &DbPool,
    media_item_id: i64,
    channel: &Channel,
    resolution: &Resolution,
    stream_seek_seconds: i64,
) -> Result<Option<PlaybackSubtitle>> {
    if channel.music_video_credits_mode != ChannelMusicVideoCreditsMode::GenerateSubtitles {
        return Ok(None);
    }
    let Some(mut data) = load_music_video_credits_data(pool, media_item_id, resolution).await?
    else {
        return Ok(None);
    };
    data.stream_seek = Duration::seconds(stream_seek_seconds.max(0));
    let rendered = if let Some(template_name) = channel
        .music_video_credits_template
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        match load_template(template_name).await {
            Ok(template) => render_music_video_credits_template(&template, &data),
            Err(error) => {
                tracing::warn!(
                    %error,
                    template = template_name,
                    "music video credits template does not exist; falling back to built-in template"
                );
                render_builtin_default(&data)
            }
        }
    } else {
        render_builtin_default(&data)
    };
    let path = write_generated_subtitle(&rendered).await?;
    Ok(Some(PlaybackSubtitle {
        id: 0,
        subtitle_kind: subtitle_kind::GENERATED,
        title: Some("Music Video Credits".to_owned()),
        stream_index: 0,
        codec: Some("ass".to_owned()),
        default: true,
        forced: true,
        sdh: false,
        language: None,
        is_extracted: false,
        path: Some(path),
    }))
}

async fn load_music_video_credits_data(
    pool: &DbPool,
    media_item_id: i64,
    resolution: &Resolution,
) -> Result<Option<MusicVideoCreditsData>> {
    let row = sqlx::query(
        r#"SELECT mvm."Id" AS "MetadataId",
                  mvm."Title" AS "Title",
                  mvm."Track" AS "Track",
                  mvm."Album" AS "Album",
                  mvm."Plot" AS "Plot",
                  mvm."ReleaseDate" AS "ReleaseDate",
                  amd."Title" AS "Artist",
                  mv."Duration" AS "Duration"
           FROM "MusicVideo" music_video
           JOIN "MusicVideoMetadata" mvm ON mvm."MusicVideoId" = music_video."Id"
           LEFT JOIN "ArtistMetadata" amd ON amd."ArtistId" = music_video."ArtistId"
           LEFT JOIN "MediaVersion" mv ON mv."MusicVideoId" = music_video."Id"
           WHERE music_video."Id" = ?
           ORDER BY mvm."Id", amd."Id", mv."Id"
           LIMIT 1"#,
    )
    .bind(media_item_id)
    .fetch_optional(pool)
    .await
    .repository()?;
    let Some(row) = row else {
        return Ok(None);
    };
    let metadata_id: i64 = row.try_get("MetadataId").repository()?;
    let duration = row
        .try_get::<Option<String>, _>("Duration")
        .repository()?
        .as_deref()
        .map_or_else(Duration::zero, decode_duration_text);
    Ok(Some(MusicVideoCreditsData {
        resolution: resolution.clone(),
        title: row
            .try_get::<Option<String>, _>("Title")
            .repository()?
            .unwrap_or_default(),
        track: row.try_get("Track").repository()?,
        album: row.try_get("Album").repository()?,
        plot: row.try_get("Plot").repository()?,
        release_date: row.try_get("ReleaseDate").repository()?,
        all_artists: load_names(
            pool,
            "MusicVideoArtist",
            "MusicVideoMetadataId",
            metadata_id,
        )
        .await?,
        artist: row
            .try_get::<Option<String>, _>("Artist")
            .repository()?
            .unwrap_or_default(),
        studios: load_names(pool, "Studio", "MusicVideoMetadataId", metadata_id).await?,
        directors: load_names(pool, "Director", "MusicVideoMetadataId", metadata_id).await?,
        duration,
        stream_seek: Duration::zero(),
    }))
}

async fn load_names(
    pool: &DbPool,
    table: &'static str,
    owner_column: &'static str,
    owner_id: i64,
) -> Result<Vec<String>> {
    let query = match (table, owner_column) {
        ("MusicVideoArtist", "MusicVideoMetadataId") => {
            r#"SELECT "Name" FROM "MusicVideoArtist" WHERE "MusicVideoMetadataId" = ? ORDER BY "Id""#
        }
        ("Studio", "MusicVideoMetadataId") => {
            r#"SELECT "Name" FROM "Studio" WHERE "MusicVideoMetadataId" = ? ORDER BY "Id""#
        }
        ("Director", "MusicVideoMetadataId") => {
            r#"SELECT "Name" FROM "Director" WHERE "MusicVideoMetadataId" = ? ORDER BY "Id""#
        }
        _ => unreachable!("validated music-video credits name table"),
    };
    let rows = sqlx::query_scalar::<_, Option<String>>(crate::db::assert_sql_safe(query))
        .bind(owner_id)
        .fetch_all(pool)
        .await
        .repository()?;
    Ok(rows
        .into_iter()
        .flatten()
        .filter_map(|value| {
            let trimmed = value.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_owned())
        })
        .collect())
}

async fn load_template(name: &str) -> Result<String> {
    let file_name = format!("{name}.{EXTENSION}");
    let editable = music_video_credits_templates_folder().join(&file_name);
    match tokio::fs::read_to_string(&editable).await {
        Ok(template) => return Ok(template),
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            return Err(RepositoryError::Decode {
                kind: "music video credits template",
                value: error.to_string(),
            });
        }
        Err(_) => {}
    }
    BUILTIN_TEMPLATES
        .iter()
        .find_map(|(candidate, template)| (*candidate == name).then(|| (*template).to_owned()))
        .ok_or_else(|| RepositoryError::Decode {
            kind: "music video credits template",
            value: format!("{name} not found"),
        })
}

async fn write_generated_subtitle(rendered: &str) -> Result<String> {
    let dir = subtitle_cache_folder().join("generated");
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|error| RepositoryError::Decode {
            kind: "generated music video credits subtitle",
            value: error.to_string(),
        })?;
    let path = dir.join(format!("music-video-credits-{}.ass", Uuid::new_v4()));
    tokio::fs::write(&path, rendered)
        .await
        .map_err(|error| RepositoryError::Decode {
            kind: "generated music video credits subtitle",
            value: error.to_string(),
        })?;
    Ok(path.to_string_lossy().into_owned())
}

#[must_use]
pub fn render_music_video_credits_template(template: &str, data: &MusicVideoCreditsData) -> String {
    if template.contains("_first_start_ts") && template.contains("_fancy_title_t") {
        return render_interval_template(template, data);
    }
    render_simple_template(template, data)
}

fn render_builtin_default(data: &MusicVideoCreditsData) -> String {
    render_music_video_credits_template(BUILTIN_TEMPLATES[0].1, data)
}

fn render_simple_template(template: &str, data: &MusicVideoCreditsData) -> String {
    let mut rendered = template.to_owned();
    let artist_text = if data.all_artists.is_empty() {
        data.artist.clone()
    } else {
        data.all_artists.join(" / ")
    };
    let replacements = [
        ("resolution.width", data.resolution.width.to_string()),
        ("resolution.height", data.resolution.height.to_string()),
        (
            "resolution.height // 20.0",
            rounded(f64::from(data.resolution.height) / 20.0).to_string(),
        ),
        (
            "resolution.width * 0.03 | math.round",
            rounded(f64::from(data.resolution.width) * 0.03).to_string(),
        ),
        (
            "resolution.height * 0.05 | math.round",
            rounded(f64::from(data.resolution.height) * 0.05).to_string(),
        ),
        ("artist", data.artist.clone()),
        ("title", data.title.clone()),
        ("album", data.album.clone().unwrap_or_default()),
        (
            "track",
            data.track
                .map_or_else(String::new, |value| value.to_string()),
        ),
        ("plot", data.plot.clone().unwrap_or_default()),
        (
            "release_date",
            data.release_date.clone().unwrap_or_default(),
        ),
        ("all_artists | array.join \" / \"", artist_text.clone()),
        ("all_artists | array.join \" | \"", artist_text),
    ];
    for (name, value) in replacements {
        rendered = replace_scriban_value(&rendered, name, &value);
    }
    rendered = render_default_artist_condition(&rendered, data);
    rendered = render_optional_album_condition(&rendered, data);
    rendered
}

fn render_interval_template(template: &str, data: &MusicVideoCreditsData) -> String {
    let margin_x = rounded(f64::from(data.resolution.width) * 0.03);
    let margin_y = rounded(f64::from(data.resolution.height) * 0.05);
    let seek = data.stream_seek.num_seconds().max(0);
    let duration = data.duration.num_seconds().max(0);
    let first_start = seek + 9;
    let first_end = seek + 16;
    let last_start = (duration - 16).max(0);
    let last_end = (duration - 9).max(last_start);
    let artists = if data.all_artists.is_empty() {
        data.artist.clone()
    } else {
        data.all_artists.join(" | ")
    };
    let fancy_title = format!(
        "{{%{{{{\\b1}}{{\\fscy150}}{{\\1c&HC2C1BF&}}}}%}}{}",
        data.title
    );
    let include_album = template.contains(r"\N{{ album }}") || template.contains("{{ album }}");
    let alignment = if include_album { 8 } else { 4 };
    let mut rendered = format!(
        "[Script Info]\nScriptType: v4.00+\nWrapStyle: 0\nScaledBorderAndShadow: yes\nYCbCr Matrix: None\nPlayResX: {}\nPlayResY: {}\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: RoboR,Roboto-Regular.ttf,{},&HFFFFFF,&HFFFFFF,&H444444,&H00000000,0,0,0,0,70,100,0,0,1,1,3,{alignment},10,10,10,1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n",
        data.resolution.width,
        data.resolution.height,
        rounded(f64::from(data.resolution.height) / 20.0)
    );
    if duration - seek > 60 {
        if include_album {
            append_dialogue_album(
                &mut rendered,
                first_start,
                first_end,
                margin_x,
                margin_y,
                &artists,
                &fancy_title,
                data.album.as_deref().unwrap_or_default(),
            );
        } else {
            append_dialogue_title_artist(
                &mut rendered,
                first_start,
                first_end,
                margin_x,
                margin_y,
                &fancy_title,
                &artists,
            );
        }
    }
    let mut i = seek + 300;
    while i < duration - 60 {
        let end = i + 7;
        let text = if include_album {
            format!(
                "{}\\N{}\\N{}",
                artists,
                data.title,
                data.album.as_deref().unwrap_or_default()
            )
        } else {
            format!("{artists}\\N{}", data.title)
        };
        append_dialogue(&mut rendered, i, end, margin_x, margin_y, 1200, 1200, &text);
        i += 300;
    }
    if include_album {
        append_dialogue_album(
            &mut rendered,
            last_start,
            last_end,
            margin_x,
            margin_y,
            &artists,
            &fancy_title,
            data.album.as_deref().unwrap_or_default(),
        );
    } else {
        append_dialogue_title_artist(
            &mut rendered,
            last_start,
            last_end,
            margin_x,
            margin_y,
            &fancy_title,
            &artists,
        );
    }
    rendered
}

fn append_dialogue_title_artist(
    out: &mut String,
    start: i64,
    end: i64,
    margin_x: i64,
    margin_y: i64,
    title: &str,
    artists: &str,
) {
    append_dialogue(out, start, end, margin_x, margin_y, 1800, 1200, title);
    append_dialogue(out, start, end, margin_x, margin_y, 1200, 1800, artists);
}

#[allow(clippy::too_many_arguments)]
fn append_dialogue_album(
    out: &mut String,
    start: i64,
    end: i64,
    margin_x: i64,
    margin_y: i64,
    artists: &str,
    title: &str,
    album: &str,
) {
    append_dialogue(out, start, end, margin_x, margin_y, 1200, 1200, artists);
    append_dialogue(out, start, end, margin_x, margin_y, 1200, 1200, title);
    append_dialogue(out, start, end, margin_x, margin_y, 1200, 1200, album);
}

#[allow(clippy::too_many_arguments)]
fn append_dialogue(
    out: &mut String,
    start: i64,
    end: i64,
    margin_x: i64,
    margin_y: i64,
    fade_in: i64,
    fade_out: i64,
    text: &str,
) {
    let _ = writeln!(
        out,
        "Dialogue: 0,{}.00,{}.00,RoboR,,{margin_x},{margin_x},{margin_y},,{{\\fad({fade_in}, {fade_out})}}{text}\n",
        ass_time(start),
        ass_time(end)
    );
}

fn render_default_artist_condition(template: &str, data: &MusicVideoCreditsData) -> String {
    let Some(start) = template.find("{{ if all_artists | array.size == 0 }}") else {
        return template.to_owned();
    };
    let Some(else_pos) = template[start..].find("{{ else }}").map(|pos| start + pos) else {
        return template.to_owned();
    };
    let Some(end_pos) = template[else_pos..]
        .find("{{ end }}")
        .map(|pos| else_pos + pos)
    else {
        return template.to_owned();
    };
    let chosen = if data.all_artists.is_empty() {
        &template[start + "{{ if all_artists | array.size == 0 }}".len()..else_pos]
    } else {
        &template[else_pos + "{{ else }}".len()..end_pos]
    };
    let mut rendered = String::new();
    rendered.push_str(&template[..start]);
    rendered.push_str(chosen);
    rendered.push_str(&template[end_pos + "{{ end }}".len()..]);
    render_simple_template(&rendered, data)
}

fn render_optional_album_condition(template: &str, data: &MusicVideoCreditsData) -> String {
    let Some(start) = template.find("{{ if album }}") else {
        return template.to_owned();
    };
    let Some(end_pos) = template[start..].find("{{ end }}").map(|pos| start + pos) else {
        return template.to_owned();
    };
    let chosen = if data
        .album
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        &template[start + "{{ if album }}".len()..end_pos]
    } else {
        ""
    };
    let mut rendered = String::new();
    rendered.push_str(&template[..start]);
    rendered.push_str(chosen);
    rendered.push_str(&template[end_pos + "{{ end }}".len()..]);
    render_simple_template(&rendered, data)
}

fn replace_scriban_value(source: &str, expression: &str, value: &str) -> String {
    source
        .replace(&format!("{{{{ {expression} }}}}"), value)
        .replace(&format!("{{{{~ {expression} ~}}}}"), value)
}

#[allow(clippy::cast_possible_truncation)]
fn rounded(value: f64) -> i64 {
    value.round() as i64
}

fn ass_time(total_seconds: i64) -> String {
    let total_seconds = total_seconds.max(0);
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> MusicVideoCreditsData {
        MusicVideoCreditsData {
            resolution: Resolution {
                id: 1,
                name: "1080p".to_owned(),
                is_custom: false,
                width: 1920,
                height: 1080,
            },
            title: "The Song".to_owned(),
            track: Some(4),
            album: Some("The Album".to_owned()),
            plot: Some("Plot".to_owned()),
            release_date: Some("2026-06-11".to_owned()),
            all_artists: vec!["Artist A".to_owned(), "Artist B".to_owned()],
            artist: "Fallback Artist".to_owned(),
            studios: vec!["Studio".to_owned()],
            directors: vec!["Director".to_owned()],
            duration: Duration::seconds(720),
            stream_seek: Duration::seconds(60),
        }
    }

    #[test]
    fn default_template_renders_music_video_metadata() {
        let rendered = render_music_video_credits_template(BUILTIN_TEMPLATES[0].1, &data());
        assert!(rendered.contains("PlayResX: 1920"));
        assert!(rendered.contains(r#"Artist A / Artist B\N"The Song"\NThe Album"#));
        assert!(rendered.contains("58,58,54"));
    }

    #[test]
    fn interval_template_uses_seek_duration_and_album_layout() {
        let rendered = render_music_video_credits_template(BUILTIN_TEMPLATES[2].1, &data());
        assert!(rendered.contains("Style: RoboR,Roboto-Regular.ttf,54"));
        assert!(rendered.contains("Dialogue: 0,00:01:09.00,00:01:16.00"));
        assert!(rendered.contains("Dialogue: 0,00:06:00.00,00:06:07.00"));
        assert!(rendered.contains(r"Artist A | Artist B\NThe Song\NThe Album"));
        assert!(rendered.contains("Dialogue: 0,00:11:44.00,00:11:51.00"));
    }
}
