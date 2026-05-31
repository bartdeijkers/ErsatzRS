use std::collections::HashSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{
    atomic::{AtomicI64, Ordering},
    Arc,
};
use std::time::Duration;

use crate::db::DbPool;
use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, Utc};
use ersatzrs_core::hls::{HlsSessionHandle, HlsSessionRegistry};
use ersatzrs_core::path_replacement::{replace_path, PathPlatform, PathReplacement};
use ersatzrs_domain::{ChannelSubtitleMode, StreamingMode};
use ersatzrs_ffmpeg::{
    DesiredVideoState, FfmpegCommand, HlsOutput, InputFile, OutputFormat, PipelineBuilder,
    PipelineRequest, SoftwarePipelineBuilder, SourceVideoStream,
};

use crate::playback::{
    load_source_video_stream, select_playback_subtitle, subtitle_fonts_dir,
    subtitle_input_for_stream,
};
use sqlx::Row;
use tokio::process::Command;
use tokio::sync::Notify;

const PLAYLIST_MAX_SEGMENTS: usize = 10;
const PLAYLIST_DELETE_INTERVAL_MS: i64 = 30_000;
const PLAYLIST_FILTER_BEFORE_SECONDS: i64 = 30;

/// Concrete HLS segmenter worker backed by `FFmpeg` CLI output in a temp folder.
pub struct HlsSessionWorker {
    channel_number: String,
    channel_id: i64,
    working_dir: PathBuf,
    output_format: OutputFormat,
    path_replacements: Vec<PathReplacement>,
    last_access_ms: Arc<AtomicI64>,
    last_delete_ms: Arc<AtomicI64>,
    cancel: Arc<Notify>,
}

impl HlsSessionWorker {
    #[must_use]
    pub fn new(
        channel_number: String,
        channel_id: i64,
        working_dir: PathBuf,
        output_format: OutputFormat,
        path_replacements: Vec<PathReplacement>,
    ) -> Arc<Self> {
        Arc::new(Self {
            channel_number,
            channel_id,
            working_dir,
            output_format,
            path_replacements,
            last_access_ms: Arc::new(AtomicI64::new(0)),
            last_delete_ms: Arc::new(AtomicI64::new(0)),
            cancel: Arc::new(Notify::new()),
        })
    }

    pub fn start(
        self: Arc<Self>,
        pool: DbPool,
        registry: Arc<dyn HlsSessionRegistry>,
        idle_timeout: Option<Duration>,
    ) {
        tokio::spawn(async move {
            self.run(pool, registry, idle_timeout).await;
        });
    }

    async fn run(
        self: Arc<Self>,
        pool: DbPool,
        registry: Arc<dyn HlsSessionRegistry>,
        idle_timeout: Option<Duration>,
    ) {
        let _ = tokio::fs::create_dir_all(&self.working_dir).await;
        self.touch();

        let mut is_first = true;
        loop {
            if let Some(timeout) = idle_timeout {
                let timeout_ms = i64::try_from(timeout.as_millis()).unwrap_or(i64::MAX);
                let idle_for_ms =
                    Utc::now().timestamp_millis() - self.last_access_ms.load(Ordering::Relaxed);
                if idle_for_ms > timeout_ms {
                    break;
                }
            }

            let Some(active) =
                load_active_hls_item(&pool, self.channel_id, &self.path_replacements).await
            else {
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            };

            let source_stream = load_source_video_stream(&pool, active.media_version_id).await;
            let subtitle_mode = active.subtitle_mode.unwrap_or(active.channel_subtitle_mode);
            let preferred_subtitle_language = active
                .preferred_subtitle_language_code
                .as_deref()
                .or(active.channel_preferred_subtitle_language_code.as_deref());
            let subtitle_input = match select_playback_subtitle(
                &pool,
                active.media_item_id,
                preferred_subtitle_language,
                subtitle_mode,
                StreamingMode::HttpLiveStreamingSegmenter,
            )
            .await
            {
                Ok(Some(subtitle)) => subtitle_input_for_stream(
                    &subtitle,
                    &active.media_path,
                    self.output_format,
                    StreamingMode::HttpLiveStreamingSegmenter,
                ),
                Ok(None) => None,
                Err(error) => {
                    tracing::warn!(%error, media_item_id = active.media_item_id, "failed to select HLS subtitle");
                    None
                }
            };
            let command = build_hls_command(
                &active.media_path,
                &self.working_dir,
                self.output_format,
                is_first,
                source_stream,
                subtitle_input,
            );
            let mut process = Command::new(&command.executable);
            process
                .args(&command.arguments)
                .current_dir(&self.working_dir)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .kill_on_drop(true);

            for variable in command.environment {
                process.env(variable.name, variable.value);
            }

            let Ok(mut child) = process.spawn() else {
                break;
            };

            tokio::select! {
                result = child.wait() => {
                    match result {
                        Ok(status) if status.success() => {}
                        Ok(_) | Err(_) => break,
                    }
                }
                () = self.cancel.notified() => {
                    break;
                }
            }

            is_first = false;
        }

        let _ = tokio::fs::remove_dir_all(&self.working_dir).await;
        registry.remove(&self.channel_number).await;
    }
}

#[async_trait]
impl HlsSessionHandle for HlsSessionWorker {
    fn channel_number(&self) -> &str {
        &self.channel_number
    }

    fn touch(&self) {
        self.last_access_ms
            .store(Utc::now().timestamp_millis(), Ordering::Relaxed);
    }

    async fn trim_playlist(&self) -> Option<String> {
        let playlist_path = self.working_dir.join("live.m3u8");
        let playlist = tokio::fs::read_to_string(playlist_path).await.ok()?;
        let filter_before = Utc::now() - chrono::Duration::seconds(PLAYLIST_FILTER_BEFORE_SECONDS);
        let trimmed = trim_playlist_contents(
            self.output_format,
            &playlist,
            filter_before,
            Some(PLAYLIST_MAX_SEGMENTS),
        )?;

        let now_ms = Utc::now().timestamp_millis();
        if now_ms - self.last_delete_ms.load(Ordering::Relaxed) >= PLAYLIST_DELETE_INTERVAL_MS {
            let _ = delete_old_segments(&self.working_dir, &trimmed.referenced_files).await;
            self.last_delete_ms.store(now_ms, Ordering::Relaxed);
        }

        Some(trimmed.playlist)
    }
}

fn build_hls_command(
    media_path: &str,
    _working_dir: &Path,
    output_format: OutputFormat,
    is_first_transcode: bool,
    source_stream: Option<SourceVideoStream>,
    subtitle_input: Option<InputFile>,
) -> FfmpegCommand {
    let input = InputFile::video(media_path.to_string());
    let mut request = PipelineRequest::new(input, DesiredVideoState::h264_1080p());
    if let Some(source_stream) = source_stream {
        request = request.with_source_video_stream(source_stream);
    }
    if let Some(subtitle_input) = subtitle_input {
        request.inputs.push(subtitle_input);
        request = request.with_subtitle_fonts_dir(subtitle_fonts_dir());
    }
    request.state.output_format = output_format;
    request.state.report_progress = false;
    request.state.hls = Some(HlsOutput {
        segment_template: match output_format {
            OutputFormat::HlsMp4 => "seg%05d.m4s".to_string(),
            _ => "seg%05d.ts".to_string(),
        },
        init_template: match output_format {
            OutputFormat::HlsMp4 => Some("init.mp4".to_string()),
            _ => None,
        },
        playlist_path: "live.m3u8".to_string(),
        segment_options: None,
        is_first_transcode,
        troubleshooting: false,
    });

    SoftwarePipelineBuilder
        .build(request.clone())
        .command(&request.inputs)
}

async fn load_active_hls_item(
    pool: &DbPool,
    channel_id: i64,
    path_replacements: &[PathReplacement],
) -> Option<ActiveHlsItem> {
    let now_text = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let row = sqlx::query(
        r#"SELECT mf."Path" AS "MediaPath",
                  pi."MediaItemId" AS "MediaItemId",
                  mv."Id" AS "MediaVersionId",
                  pi."PreferredSubtitleLanguageCode" AS "PreferredSubtitleLanguageCode",
                  pi."SubtitleMode" AS "SubtitleMode",
                  c."PreferredSubtitleLanguageCode" AS "ChannelPreferredSubtitleLanguageCode",
                  c."SubtitleMode" AS "ChannelSubtitleMode"
           FROM "PlayoutItem" pi
           JOIN "Playout" p ON p."Id" = pi."PlayoutId"
           JOIN "Channel" c ON c."Id" = p."ChannelId"
           JOIN "MediaVersion" mv
               ON mv."MovieId" = pi."MediaItemId"
               OR mv."EpisodeId" = pi."MediaItemId"
               OR mv."MusicVideoId" = pi."MediaItemId"
               OR mv."OtherVideoId" = pi."MediaItemId"
               OR mv."RemoteStreamId" = pi."MediaItemId"
               OR mv."SongId" = pi."MediaItemId"
               OR mv."ImageId" = pi."MediaItemId"
           JOIN "MediaFile" mf ON mf."MediaVersionId" = mv."Id"
           WHERE p."ChannelId" = ?
             AND pi."Start" <= ?
             AND pi."Finish" > ?
             AND mf."Path" IS NOT NULL
           ORDER BY pi."Start" DESC, pi."Id" DESC
           LIMIT 1"#,
    )
    .bind(channel_id)
    .bind(&now_text)
    .bind(&now_text)
    .fetch_optional(pool)
    .await
    .ok()??;

    let media_path = row.get::<String, _>("MediaPath");
    let resolved = if path_replacements.is_empty() {
        media_path
    } else {
        replace_path(path_replacements, &media_path, PathPlatform::Unix)
    };
    Some(ActiveHlsItem {
        media_path: resolved,
        media_item_id: row.get::<i64, _>("MediaItemId"),
        media_version_id: row.get::<i64, _>("MediaVersionId"),
        preferred_subtitle_language_code: row
            .try_get::<Option<String>, _>("PreferredSubtitleLanguageCode")
            .ok()
            .flatten(),
        subtitle_mode: row
            .try_get::<Option<i64>, _>("SubtitleMode")
            .ok()
            .flatten()
            .and_then(decode_subtitle_mode),
        channel_preferred_subtitle_language_code: row
            .try_get::<Option<String>, _>("ChannelPreferredSubtitleLanguageCode")
            .ok()
            .flatten(),
        channel_subtitle_mode: row
            .try_get::<i64, _>("ChannelSubtitleMode")
            .ok()
            .and_then(decode_subtitle_mode)
            .unwrap_or(ChannelSubtitleMode::None),
    })
}

#[derive(Debug, Clone)]
struct ActiveHlsItem {
    media_path: String,
    media_item_id: i64,
    media_version_id: i64,
    preferred_subtitle_language_code: Option<String>,
    subtitle_mode: Option<ChannelSubtitleMode>,
    channel_preferred_subtitle_language_code: Option<String>,
    channel_subtitle_mode: ChannelSubtitleMode,
}

const fn decode_subtitle_mode(value: i64) -> Option<ChannelSubtitleMode> {
    match value {
        0 => Some(ChannelSubtitleMode::None),
        1 => Some(ChannelSubtitleMode::Forced),
        2 => Some(ChannelSubtitleMode::Default),
        3 => Some(ChannelSubtitleMode::Any),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PlaylistItem {
    Discontinuity,
    Segment(PlaylistSegment),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PlaylistSegment {
    extinf_line: String,
    program_date_time_line: Option<String>,
    program_date_time: Option<DateTime<FixedOffset>>,
    uri: String,
    map_uri: Option<String>,
    sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedPlaylist {
    version: String,
    target_duration: u32,
    items: Vec<PlaylistItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TrimmedPlaylist {
    playlist: String,
    referenced_files: HashSet<String>,
}

fn trim_playlist_contents(
    output_format: OutputFormat,
    playlist: &str,
    filter_before: DateTime<Utc>,
    maybe_max_segments: Option<usize>,
) -> Option<TrimmedPlaylist> {
    let parsed = parse_playlist(playlist)?;
    let all_segments: Vec<PlaylistSegment> = parsed
        .items
        .iter()
        .filter_map(|item| match item {
            PlaylistItem::Segment(segment) => Some(segment.clone()),
            PlaylistItem::Discontinuity => None,
        })
        .collect();
    if all_segments.is_empty() {
        return None;
    }

    let retained_segments =
        select_retained_segments(&all_segments, filter_before, maybe_max_segments);
    render_trimmed_playlist(output_format, &parsed, &retained_segments)
}

fn select_retained_segments(
    all_segments: &[PlaylistSegment],
    filter_before: DateTime<Utc>,
    maybe_max_segments: Option<usize>,
) -> Vec<PlaylistSegment> {
    if let Some(max_segments) = maybe_max_segments {
        if all_segments.len() > max_segments {
            let after_filter: Vec<PlaylistSegment> = all_segments
                .iter()
                .filter(|segment| segment_is_after_filter(segment, filter_before))
                .cloned()
                .collect();

            if after_filter.len() >= max_segments {
                return after_filter.into_iter().take(max_segments).collect();
            }

            let keep_from = all_segments.len().saturating_sub(max_segments);
            return all_segments[keep_from..].to_vec();
        }

        return all_segments.to_vec();
    }

    all_segments
        .iter()
        .filter(|segment| segment_is_after_filter(segment, filter_before))
        .cloned()
        .collect()
}

fn segment_is_after_filter(segment: &PlaylistSegment, filter_before: DateTime<Utc>) -> bool {
    segment
        .program_date_time
        .is_some_and(|pdt| pdt.with_timezone(&Utc) >= filter_before)
}

fn render_trimmed_playlist(
    output_format: OutputFormat,
    parsed: &ParsedPlaylist,
    retained_segments: &[PlaylistSegment],
) -> Option<TrimmedPlaylist> {
    let first_segment = retained_segments.first()?;
    let last_segment = retained_segments.last()?;
    let retained_uris: HashSet<&str> = retained_segments
        .iter()
        .map(|segment| segment.uri.as_str())
        .collect();
    let first_index = parsed.items.iter().position(
        |item| matches!(item, PlaylistItem::Segment(segment) if segment.uri == first_segment.uri),
    )?;
    let last_index = parsed.items.iter().rposition(
        |item| matches!(item, PlaylistItem::Segment(segment) if segment.uri == last_segment.uri),
    )?;
    let leading_discontinuities = parsed.items[..first_index]
        .iter()
        .filter(|item| matches!(item, PlaylistItem::Discontinuity))
        .count();

    let mut out = String::new();
    out.push_str("#EXTM3U\n");
    let _ = writeln!(out, "#EXT-X-VERSION:{}", parsed.version);
    let _ = writeln!(out, "#EXT-X-TARGETDURATION:{}", parsed.target_duration);
    let _ = writeln!(out, "#EXT-X-MEDIA-SEQUENCE:{}", first_segment.sequence);
    if leading_discontinuities > 1 {
        let _ = writeln!(
            out,
            "#EXT-X-DISCONTINUITY-SEQUENCE:{leading_discontinuities}"
        );
    }
    out.push_str("#EXT-X-INDEPENDENT-SEGMENTS\n");
    if leading_discontinuities == 1 {
        out.push_str("#EXT-X-DISCONTINUITY\n");
    }

    let mut referenced_files = HashSet::new();
    let mut current_map_uri = None::<String>;
    if output_format == OutputFormat::HlsMp4 {
        if let Some(map_uri) = &first_segment.map_uri {
            let _ = writeln!(out, "#EXT-X-MAP:URI=\"{map_uri}\"");
            referenced_files.insert(map_uri.clone());
            current_map_uri = Some(map_uri.clone());
        }
    }

    for (index, item) in parsed.items[first_index..=last_index].iter().enumerate() {
        match item {
            PlaylistItem::Discontinuity => {
                let next_retained = parsed.items[first_index + index + 1..=last_index]
                    .iter()
                    .find_map(|next_item| match next_item {
                        PlaylistItem::Segment(segment)
                            if retained_uris.contains(segment.uri.as_str()) =>
                        {
                            Some(segment)
                        }
                        PlaylistItem::Segment(_) | PlaylistItem::Discontinuity => None,
                    });
                if let Some(next_segment) = next_retained {
                    if next_segment.uri != first_segment.uri {
                        out.push_str("#EXT-X-DISCONTINUITY\n");
                        if output_format == OutputFormat::HlsMp4
                            && next_segment.map_uri != current_map_uri
                        {
                            if let Some(map_uri) = &next_segment.map_uri {
                                let _ = writeln!(out, "#EXT-X-MAP:URI=\"{map_uri}\"");
                                referenced_files.insert(map_uri.clone());
                                current_map_uri = Some(map_uri.clone());
                            }
                        }
                    }
                }
            }
            PlaylistItem::Segment(segment) if retained_uris.contains(segment.uri.as_str()) => {
                out.push_str(&segment.extinf_line);
                out.push('\n');
                if let Some(program_date_time_line) = &segment.program_date_time_line {
                    out.push_str(program_date_time_line);
                    out.push('\n');
                }
                out.push_str(&segment.uri);
                out.push('\n');
                referenced_files.insert(segment.uri.clone());
                if let Some(map_uri) = &segment.map_uri {
                    referenced_files.insert(map_uri.clone());
                    current_map_uri = Some(map_uri.clone());
                }
            }
            PlaylistItem::Segment(_) => {}
        }
    }

    Some(TrimmedPlaylist {
        playlist: out,
        referenced_files,
    })
}

fn parse_playlist(playlist: &str) -> Option<ParsedPlaylist> {
    let lines: Vec<&str> = playlist.lines().collect();
    let mut version = "7".to_string();
    let mut target_duration = None;
    let mut current_map_uri = None::<String>;
    let mut items = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let line = lines[index].trim();
        if line.is_empty() {
            index += 1;
            continue;
        }

        if let Some(value) = line.strip_prefix("#EXT-X-VERSION:") {
            version = value.to_string();
            index += 1;
            continue;
        }
        if let Some(value) = line.strip_prefix("#EXT-X-TARGETDURATION:") {
            target_duration = value.parse::<u32>().ok();
            index += 1;
            continue;
        }
        if line.eq_ignore_ascii_case("#EXT-X-DISCONTINUITY") {
            items.push(PlaylistItem::Discontinuity);
            index += 1;
            continue;
        }
        if let Some(value) = line.strip_prefix("#EXT-X-MAP:URI=") {
            current_map_uri = Some(value.trim_matches('"').to_string());
            index += 1;
            continue;
        }
        if !line.starts_with("#EXTINF:") {
            index += 1;
            continue;
        }

        let extinf_line = line.to_string();
        index += 1;

        let mut program_date_time_line = None;
        let mut program_date_time = None;
        while index < lines.len() {
            let next_line = lines[index].trim();
            if next_line.is_empty() {
                index += 1;
                continue;
            }

            if let Some(value) = next_line.strip_prefix("#EXT-X-PROGRAM-DATE-TIME:") {
                program_date_time_line = Some(next_line.to_string());
                program_date_time = DateTime::parse_from_rfc3339(value).ok();
                index += 1;
                continue;
            }

            if let Some(value) = next_line.strip_prefix("#EXT-X-MAP:URI=") {
                current_map_uri = Some(value.trim_matches('"').to_string());
                index += 1;
                continue;
            }

            if next_line.starts_with('#') {
                index += 1;
                continue;
            }

            let uri = next_line.to_string();
            let sequence = parse_segment_sequence(&uri)?;
            items.push(PlaylistItem::Segment(PlaylistSegment {
                extinf_line,
                program_date_time_line,
                program_date_time,
                uri,
                map_uri: current_map_uri.clone(),
                sequence,
            }));
            index += 1;
            break;
        }
    }

    Some(ParsedPlaylist {
        version,
        target_duration: target_duration?,
        items,
    })
}

fn parse_segment_sequence(uri: &str) -> Option<u64> {
    let stem = Path::new(uri).file_stem()?.to_str()?;
    let digits: String = stem
        .chars()
        .rev()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse::<u64>().ok()
    }
}

async fn delete_old_segments(
    working_dir: &Path,
    referenced_files: &HashSet<String>,
) -> std::io::Result<()> {
    let mut entries = tokio::fs::read_dir(working_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        if !entry.file_type().await?.is_file() {
            continue;
        }

        let file_name = entry.file_name().to_string_lossy().into_owned();
        let extension = Path::new(&file_name)
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_ascii_lowercase);
        let is_segment_file = matches!(extension.as_deref(), Some("ts" | "mp4" | "m4s"));
        if is_segment_file && !referenced_files.contains(&file_name) {
            let _ = tokio::fs::remove_file(entry.path()).await;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn build_hls_command_produces_hls_flags() {
        let command = build_hls_command(
            "/media/test.mkv",
            Path::new("/tmp/hls/1"),
            OutputFormat::Hls,
            true,
            None,
            None,
        );
        insta::assert_snapshot!(command.arguments.join(" "));
    }

    #[test]
    fn build_hls_command_produces_fmp4_flags() {
        let command = build_hls_command(
            "/media/test.mkv",
            Path::new("/tmp/hls/1"),
            OutputFormat::HlsMp4,
            true,
            None,
            None,
        );
        let arguments = command.arguments.join(" ");
        assert!(arguments.contains("-hls_segment_type fmp4"));
        assert!(arguments.contains("-hls_fmp4_init_filename init.mp4"));
        assert!(arguments.contains("-hls_segment_filename seg%05d.m4s"));
    }

    #[test]
    fn trim_playlist_contents_keeps_recent_segments() {
        let playlist = concat!(
            "#EXTM3U\n",
            "#EXT-X-VERSION:7\n",
            "#EXT-X-TARGETDURATION:4\n",
            "#EXT-X-MEDIA-SEQUENCE:1\n",
            "#EXT-X-INDEPENDENT-SEGMENTS\n",
            "#EXTINF:4.000000,\n",
            "#EXT-X-PROGRAM-DATE-TIME:2026-04-20T20:00:00.000+00:00\n",
            "seg00001.ts\n",
            "#EXTINF:4.000000,\n",
            "#EXT-X-PROGRAM-DATE-TIME:2026-04-20T20:00:40.000+00:00\n",
            "seg00002.ts\n",
            "#EXT-X-DISCONTINUITY\n",
            "#EXTINF:4.000000,\n",
            "#EXT-X-PROGRAM-DATE-TIME:2026-04-20T20:01:20.000+00:00\n",
            "seg00003.ts\n"
        );

        let trimmed = trim_playlist_contents(
            OutputFormat::Hls,
            playlist,
            DateTime::parse_from_rfc3339("2026-04-20T20:00:30.000+00:00")
                .expect("valid timestamp")
                .with_timezone(&Utc),
            None,
        )
        .expect("trimmed playlist");

        assert!(!trimmed.playlist.contains("seg00001.ts"));
        assert!(trimmed.playlist.contains("#EXT-X-DISCONTINUITY"));
        assert!(trimmed.playlist.contains("seg00002.ts"));
        assert!(trimmed.playlist.contains("seg00003.ts"));
        assert_eq!(trimmed.referenced_files.len(), 2);
    }

    #[test]
    fn trim_playlist_contents_preserves_map_for_fmp4() {
        let playlist = concat!(
            "#EXTM3U\n",
            "#EXT-X-VERSION:7\n",
            "#EXT-X-TARGETDURATION:4\n",
            "#EXT-X-MEDIA-SEQUENCE:1\n",
            "#EXT-X-INDEPENDENT-SEGMENTS\n",
            "#EXT-X-MAP:URI=\"init.mp4\"\n",
            "#EXTINF:4.000000,\n",
            "#EXT-X-PROGRAM-DATE-TIME:2026-04-20T20:01:20.000+00:00\n",
            "seg00003.m4s\n"
        );

        let trimmed = trim_playlist_contents(
            OutputFormat::HlsMp4,
            playlist,
            DateTime::parse_from_rfc3339("2026-04-20T20:00:30.000+00:00")
                .expect("valid timestamp")
                .with_timezone(&Utc),
            Some(10),
        )
        .expect("trimmed playlist");

        assert!(trimmed.playlist.contains("#EXT-X-MAP:URI=\"init.mp4\""));
        assert!(trimmed.referenced_files.contains("init.mp4"));
        assert!(trimmed.referenced_files.contains("seg00003.m4s"));
    }

    #[tokio::test]
    async fn delete_old_segments_removes_unreferenced_files() {
        let working_dir =
            std::env::temp_dir().join(format!("ersatzrs-hls-test-{}", Uuid::new_v4()));
        tokio::fs::create_dir_all(&working_dir)
            .await
            .expect("create temp dir");
        tokio::fs::write(working_dir.join("seg00001.ts"), b"old")
            .await
            .expect("write old segment");
        tokio::fs::write(working_dir.join("seg00002.ts"), b"keep")
            .await
            .expect("write retained segment");
        tokio::fs::write(working_dir.join("live.m3u8"), b"playlist")
            .await
            .expect("write playlist");

        let mut referenced_files = HashSet::new();
        referenced_files.insert("seg00002.ts".to_string());

        delete_old_segments(&working_dir, &referenced_files)
            .await
            .expect("delete old segments");

        assert!(!working_dir.join("seg00001.ts").exists());
        assert!(working_dir.join("seg00002.ts").exists());
        assert!(working_dir.join("live.m3u8").exists());

        let _ = tokio::fs::remove_dir_all(&working_dir).await;
    }
}
