//! Shared song pictures, mapped from ErsatzTV `SongVideoGenerator` at
//! 481840fbd47ebfe57ab9919ffabec0b9f69e9874: occurrence-local choices, custom
//! resources, thumbnail/BlurHash backgrounds and two multilingual text layouts.
//! `FFmpeg` CLI composition/libass reuse the native graphics/subtitle boundary
//! (ADRs 0005/0105). Native progress is applied by the playback pipeline, never
//! baked into Next's static PNG.

use crate::{
    db::DbPool,
    remote_stream::RemoteStreamCommand,
    repositories::{SqliteConfigRepository, SqliteMediaSourceRepository, SqlxResultExt as _},
};
use ersatzrs_core::repositories::{
    ConfigRepository, MediaSourceRepository, RepositoryError, Result,
};
use ersatzrs_domain::{ChannelSongVideoMode, Resolution, WatermarkLocation};
use ersatzrs_ffmpeg::{
    FrameSize, InputFile, SourceVideoCodec, SourceVideoStream, WatermarkOverlay,
};
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use tokio::{io::AsyncReadExt as _, sync::oneshot, time::Instant};

const MAX_IMAGE_BYTES: usize = 64 * 1024 * 1024;
const MAX_ARTWORK_BYTES: usize = crate::addon_artwork::MAX_ARTWORK_BYTES;
const THUMBNAIL_SIDE: u32 = 32;

/// Native preparation distinguishes ordinary render failure from an unsafe
/// process teardown. Callers must quarantine admission on unconfirmed teardown.
#[derive(Debug, thiserror::Error)]
pub enum NativeSongPreparationError {
    #[error(transparent)]
    Repository(#[from] RepositoryError),
    #[error("native song renderer termination could not be confirmed")]
    TerminationUnconfirmed,
}

impl NativeSongPreparationError {
    #[must_use]
    pub const fn termination_unconfirmed(&self) -> bool {
        matches!(self, Self::TerminationUnconfirmed)
    }
}
const FONT: &[u8] = include_bytes!("../../../assets/fonts/OPTIKabel-Heavy.otf");
const COVER: &[u8] = include_bytes!("../../../assets/_song_album_cover_512.png");
const BACKGROUNDS: [&[u8]; 3] = [
    include_bytes!("../../../assets/_song_background_1.png"),
    include_bytes!("../../../assets/_song_background_2.png"),
    include_bytes!("../../../assets/_song_background_3.png"),
];

/// Finite PNG producer; its caller owns directory and process supervision.
pub struct SongBackgroundRecipe {
    pub command: RemoteStreamCommand,
    /// One default-art retry after confirmed renderer failure, same deadline.
    pub fallback_command: Option<RemoteStreamCommand>,
    pub stream: SourceVideoStream,
}

#[derive(Debug, Default)]
struct Metadata {
    title: String,
    album: String,
    artists: Vec<String>,
    album_artists: Vec<String>,
    artwork: Option<String>,
    hashes: Vec<String>,
}

struct Choices {
    seed: [u8; 16],
    position: u64,
}
impl Choices {
    fn new(seed: Option<i64>) -> Self {
        let mut bytes = *uuid::Uuid::new_v4().as_bytes();
        if let Some(seed) = seed {
            bytes = [0; 16];
            bytes[..8].copy_from_slice(&seed.to_le_bytes());
        }
        Self {
            seed: bytes,
            position: 0,
        }
    }
    fn next(&mut self, count: usize) -> usize {
        let mut digest = Sha256::new();
        digest.update(self.seed);
        digest.update(self.position.to_le_bytes());
        self.position += 1;
        let bytes: [u8; 8] = digest.finalize()[..8]
            .try_into()
            .expect("eight-byte digest prefix");
        usize::try_from(
            u64::from_le_bytes(bytes) % u64::try_from(count).expect("small choice count"),
        )
        .expect("choice fits usize")
    }
}

fn unavailable() -> RepositoryError {
    RepositoryError::unavailable("song background", "image preparation failed")
}
fn dimensions(resolution: &Resolution) -> Result<FrameSize> {
    let width = u32::try_from(resolution.width).map_err(|_| unavailable())?;
    let height = u32::try_from(resolution.height).map_err(|_| unavailable())?;
    if width == 0 || height == 0 || width > 8192 || height > 8192 {
        return Err(unavailable());
    }
    Ok(FrameSize::new(width, height))
}

async fn metadata(pool: &DbPool, id: i64) -> Result<Metadata> {
    let row = sqlx::query(r#"SELECT "Id", "Title", "Album", "Artists", "AlbumArtists" FROM "SongMetadata" WHERE "SongId" = ? ORDER BY "Id" LIMIT 1"#).bind(id).fetch_optional(pool).await.repository()?;
    let Some(row) = row else {
        return Ok(Metadata::default());
    };
    let metadata_id: i64 = row.try_get("Id").repository()?;
    let mut result = Metadata {
        title: row
            .try_get::<Option<String>, _>("Title")
            .repository()?
            .unwrap_or_default(),
        album: row
            .try_get::<Option<String>, _>("Album")
            .repository()?
            .unwrap_or_default(),
        artists: artist_names(
            &row.try_get::<Option<String>, _>("Artists")
                .repository()?
                .unwrap_or_default(),
        ),
        album_artists: artist_names(
            &row.try_get::<Option<String>, _>("AlbumArtists")
                .repository()?
                .unwrap_or_default(),
        ),
        ..Metadata::default()
    };
    // Independent thumbnail lookup avoids multiplying metadata by related rows.
    let artwork = sqlx::query(r#"SELECT "Path", "BlurHash43", "BlurHash54", "BlurHash64" FROM "Artwork" WHERE "SongMetadataId" = ? AND "ArtworkKind" = 1 ORDER BY "Id" LIMIT 1"#).bind(metadata_id).fetch_optional(pool).await.repository()?;
    if let Some(row) = artwork {
        result.artwork = row.try_get("Path").repository()?;
        for column in ["BlurHash43", "BlurHash54", "BlurHash64"] {
            if let Some(hash) = row
                .try_get::<Option<String>, _>(column)
                .repository()?
                .filter(|s| !s.trim().is_empty())
            {
                result.hashes.push(hash);
            }
        }
    }
    Ok(result)
}

fn artist_names(value: &str) -> Vec<String> {
    // Imported upstream rows serialize lists as JSON; native scanner persists
    // artist/album-artist tag lists with "; " (scanner update_song_metadata).
    serde_json::from_str::<Vec<String>>(value).unwrap_or_else(|_| {
        value
            .split(';')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect()
    })
}

async fn resource(name: &str, builtin: &[u8], destination: &Path) -> Result<()> {
    let root = crate::file_system_layout::resources_cache_folder();
    resource_from(&root, name, builtin, destination).await
}

async fn resource_from(root: &Path, name: &str, builtin: &[u8], destination: &Path) -> Result<()> {
    for candidate in [root.join(name), root.join(format!("_{name}"))] {
        if candidate.is_file() {
            tokio::fs::copy(candidate, destination)
                .await
                .map_err(|_| unavailable())?;
            return Ok(());
        }
    }
    tokio::fs::write(destination, builtin)
        .await
        .map_err(|_| unavailable())
}

fn decoded_hash(hashes: &[String], choices: &mut Choices) -> Option<Vec<u8>> {
    if hashes.is_empty() {
        return None;
    }
    let offset = choices.next(hashes.len());
    // A corrupt imported hash falls back to real cover blur, never a panic.
    hashes
        .iter()
        .cycle()
        .skip(offset)
        .take(hashes.len())
        .find_map(|hash| {
            (hash.len() <= 166 && hash.is_ascii())
                .then(|| blurhash::decode(hash, THUMBNAIL_SIDE, THUMBNAIL_SIDE, 1.0).ok())
                .flatten()
        })
}
fn escaped_text(value: &str) -> String {
    value
        .replace('\r', "")
        // libass has no doubled-backslash escape: a word joiner prevents a
        // metadata \N/\n/\h from becoming a control while adding no visible glyph.
        .replace('\\', "\\\u{2060}")
        .replace('\n', r"\N")
        .replace('{', r"\{")
        .replace('}', r"\}")
}
fn subtitle(
    metadata: &Metadata,
    size: FrameSize,
    left_cover: bool,
    details: bool,
    progress: bool,
) -> String {
    let small = (f64::from(size.height) / 20.0).round();
    let large = (f64::from(size.height) / 10.0).round();
    let artists = escaped_text(&metadata.artists.join(", "));
    let title = escaped_text(&metadata.title);
    let text = if details {
        [
            format!("{{\\fs{large}}}{title}"),
            format!("{{\\fs{small}}}{artists}"),
        ]
        .join(r"\N")
    } else {
        let album_artists = metadata
            .album_artists
            .iter()
            .filter(|a| !metadata.artists.contains(a))
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        [
            artists,
            if title.is_empty() {
                String::new()
            } else {
                format!("\"{title}\"")
            },
            escaped_text(&album_artists),
            escaped_text(&metadata.album),
        ]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(r"\N")
    };
    let left = (f64::from(size.width) * if left_cover { 0.31 } else { 0.03 }).round();
    let right = (f64::from(size.width) * if left_cover { 0.03 } else { 0.31 }).round();
    let bottom = (f64::from(size.height) * if progress { 0.15 } else { 0.05 }).round();
    format!(
        "[Script Info]\nScriptType: v4.00+\nWrapStyle: 0\nScaledBorderAndShadow: yes\nPlayResX: {}\nPlayResY: {}\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Song,OPTIKabel-Heavy,{small},&H00FFFFFF,&H00FFFFFF,&H00444444,&H00000000,0,0,0,0,100,100,0,0,1,1,3,1,{left},{right},{bottom},1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:00.00,1:00:00.00,Song,,0,0,0,,{text}\n",
        size.width, size.height
    )
}

/// Prepare assets and a target-sized RGB PNG command.
/// # Errors
/// Returns redacted preparation errors; caller owns an existing directory.
pub async fn prepare(
    pool: &DbPool,
    media_item_id: i64,
    resolution: &Resolution,
    mode: ChannelSongVideoMode,
    seed: Option<i64>,
    directory: &Path,
    ffmpeg: &Path,
) -> Result<SongBackgroundRecipe> {
    let size = dimensions(resolution)?;
    let metadata = metadata(pool, media_item_id).await?;
    let mut choices = Choices::new(seed);
    let background = choices.next(BACKGROUNDS.len());
    let left_cover = choices.next(2) == 0;
    let details = choices.next(2) == 0;
    let layout = PictureLayout {
        size,
        left_cover,
        progress: mode == ChannelSongVideoMode::WithProgress,
    };
    write_picture_assets(&metadata, background, &layout, details, directory).await?;
    let has_cover = if let Some(path) = &metadata.artwork {
        match copy_artwork(pool, path, &directory.join("cover.png")).await {
            Ok(available) => available,
            Err(RepositoryError::Unavailable {
                operation: "song background",
                ..
            }) => {
                // Upstream's image callback keeps playback available when real
                // artwork cannot be read. Never log provider URLs/credentials.
                tracing::debug!(
                    media_item_id,
                    "song artwork unavailable; using bundled cover"
                );
                false
            }
            Err(error) => return Err(error),
        }
    } else {
        false
    };
    let hash = if has_cover {
        decoded_hash(&metadata.hashes, &mut choices)
    } else {
        None
    };
    if let Some(bytes) = &hash {
        tokio::fs::write(directory.join("blur.rgba"), bytes)
            .await
            .map_err(|_| unavailable())?;
    }
    let args = arguments(&layout, has_cover, hash.is_some());
    let mut stream = SourceVideoStream::new(SourceVideoCodec::Unknown);
    stream.is_still_image = true;
    stream.frame_size = Some(size);
    stream.sample_aspect_ratio = Some("1:1".to_owned());
    let command = RemoteStreamCommand {
        tool_paths: Vec::new(),
        program: ffmpeg.to_string_lossy().into_owned(),
        args,
        working_directory: directory.to_path_buf(),
        environment: BTreeMap::new(),
    };
    let fallback_command = has_cover.then(|| {
        let mut fallback = command.clone();
        fallback.args = arguments(&layout, false, false)
            .into_iter()
            .map(|arg| {
                if arg == "cover.png" {
                    "default-cover.png".to_owned()
                } else {
                    arg
                }
            })
            .collect();
        fallback
    });
    Ok(SongBackgroundRecipe {
        command,
        fallback_command,
        stream,
    })
}

struct PictureLayout {
    size: FrameSize,
    left_cover: bool,
    progress: bool,
}

async fn write_picture_assets(
    metadata: &Metadata,
    background: usize,
    layout: &PictureLayout,
    details: bool,
    directory: &Path,
) -> Result<()> {
    resource(
        &format!("song_background_{}.png", background + 1),
        BACKGROUNDS[background],
        &directory.join("background.png"),
    )
    .await?;
    resource(
        "song_album_cover_512.png",
        COVER,
        &directory.join("cover.png"),
    )
    .await?;
    tokio::fs::copy(
        directory.join("cover.png"),
        directory.join("default-cover.png"),
    )
    .await
    .map_err(|_| unavailable())?;
    tokio::fs::write(directory.join("font.otf"), FONT)
        .await
        .map_err(|_| unavailable())?;
    tokio::fs::write(
        directory.join("song.ass"),
        subtitle(
            metadata,
            layout.size,
            layout.left_cover,
            details,
            layout.progress,
        ),
    )
    .await
    .map_err(|_| unavailable())
}

fn arguments(layout: &PictureLayout, has_cover: bool, hash: bool) -> Vec<String> {
    let PictureLayout {
        size,
        left_cover,
        progress,
    } = *layout;
    let mut args = [
        "-nostdin",
        "-hide_banner",
        "-loglevel",
        "error",
        "-filter_complex_threads",
        "1",
    ]
    .map(str::to_owned)
    .to_vec();
    if hash {
        args.extend(
            [
                "-f",
                "rawvideo",
                "-pixel_format",
                "rgba",
                "-video_size",
                "32x32",
                "-i",
                "blur.rgba",
            ]
            .map(str::to_owned),
        );
    } else {
        args.extend(
            [
                "-i",
                if has_cover {
                    "cover.png"
                } else {
                    "background.png"
                },
            ]
            .map(str::to_owned),
        );
    }
    args.extend(["-i", "cover.png"].map(str::to_owned));
    let width = (f64::from(size.width) * 0.25).round().max(1.0);
    let x = if left_cover { "W*0.03" } else { "W-w-W*0.03" };
    let y = if progress { "H-h-H*0.15" } else { "H-h-H*0.05" };
    let blur = if has_cover && !hash {
        ",boxblur=20:1"
    } else {
        ""
    };
    args.extend(["-filter_complex".to_owned(), format!("[0:0]scale={}:{},setsar=1{blur}[bg];[1:0]scale={width}:{width},setsar=1[cover];[bg][cover]overlay=x={x}:y={y},subtitles=filename=song.ass:fontsdir=.,format=rgb24[v]", size.width, size.height)]);
    args.extend(
        [
            "-map",
            "[v]",
            "-frames:v",
            "1",
            "-threads",
            "1",
            "-c:v",
            "png",
            "-pix_fmt",
            "rgb24",
            "-f",
            "image2pipe",
            "pipe:1",
        ]
        .map(str::to_owned),
    );
    args
}

async fn copy_artwork(pool: &DbPool, stored: &str, destination: &Path) -> Result<bool> {
    if let Some(request) = artwork_request(pool, stored).await? {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| unavailable())?;
        let mut response = request.send(&client).await?;
        if !response.status().is_success() {
            return Err(unavailable());
        }
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|h| h.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| unavailable())? {
            if bytes.len().saturating_add(chunk.len()) > MAX_ARTWORK_BYTES {
                return Err(unavailable());
            }
            bytes.extend_from_slice(&chunk);
        }
        if bytes.is_empty() {
            return Err(unavailable());
        }
        crate::addon_artwork::validate_image(&content_type, &bytes).map_err(|_| unavailable())?;
        // Private staging never creates a directory or detaches work. A dropped
        // preparation cannot recreate assets after the owning TempDir is gone.
        let staging = destination.with_extension("pending");
        tokio::fs::write(&staging, bytes)
            .await
            .map_err(|_| unavailable())?;
        tokio::fs::rename(&staging, destination)
            .await
            .map_err(|_| unavailable())?;
        return Ok(true);
    }
    let path = Path::new(stored);
    let root = crate::file_system_layout::data_path("artwork/thumbnails");
    let mut candidates = vec![path.to_path_buf()];
    if !path.is_absolute() && path.components().count() == 1 {
        candidates.push(root.join(path));
        if stored.len() >= 2 && stored.bytes().all(|b| b.is_ascii_hexdigit()) {
            candidates.push(root.join(&stored[..2]).join(path));
            candidates.push(root.join(&stored[..2]).join(format!("{stored}.jpg")));
        }
    }
    for path in candidates {
        if path.is_file() {
            if tokio::fs::metadata(&path)
                .await
                .map_err(|_| unavailable())?
                .len()
                > u64::try_from(MAX_ARTWORK_BYTES).expect("bounded artwork size")
            {
                return Err(unavailable());
            }
            tokio::fs::copy(path, destination)
                .await
                .map_err(|_| unavailable())?;
            return Ok(true);
        }
    }
    Ok(false)
}

struct ArtworkRequest {
    url: reqwest::Url,
    header: Option<(&'static str, String)>,
}
impl ArtworkRequest {
    async fn send(&self, client: &reqwest::Client) -> Result<reqwest::Response> {
        let mut url = self.url.clone();
        let mut credentials = true;
        for hop in 0..=5 {
            if !matches!(url.scheme(), "http" | "https")
                || !url.username().is_empty()
                || url.password().is_some()
            {
                return Err(unavailable());
            }
            let mut request = client.get(url.clone());
            if let Some((key, value)) = self.header.as_ref().filter(|_| credentials) {
                request = request.header(*key, value);
            }
            let response = request.send().await.map_err(|_| unavailable())?;
            if !matches!(response.status().as_u16(), 301 | 302 | 303 | 307 | 308) {
                return Ok(response);
            }
            if hop == 5 {
                return Err(unavailable());
            }
            let next = url
                .join(
                    response
                        .headers()
                        .get(reqwest::header::LOCATION)
                        .and_then(|h| h.to_str().ok())
                        .ok_or_else(unavailable)?,
                )
                .map_err(|_| unavailable())?;
            // Once credentials cross an origin boundary they never return, matching
            // the existing public artwork proxy's redirect policy.
            credentials &= next.origin() == url.origin();
            url = next;
        }
        Err(unavailable())
    }
}

async fn artwork_request(pool: &DbPool, stored: &str) -> Result<Option<ArtworkRequest>> {
    if stored.starts_with("http://") || stored.starts_with("https://") {
        return Ok(Some(ArtworkRequest {
            url: reqwest::Url::parse(stored).map_err(|_| unavailable())?,
            header: None,
        }));
    }
    let sources = SqliteMediaSourceRepository(pool.clone());
    let config = SqliteConfigRepository(pool.clone());
    let lower = stored.to_ascii_lowercase();
    let (provider, rest) = if lower.starts_with("jellyfin://") {
        ("jellyfin", &stored[11..])
    } else if lower.starts_with("emby://") {
        ("emby", &stored[7..])
    } else if lower.starts_with("plex://") || lower.starts_with("plex/") {
        (
            "plex",
            &stored[if lower.starts_with("plex://") { 7 } else { 5 }..],
        )
    } else {
        return Ok(None);
    };
    if rest.starts_with('/') || rest.contains('\\') {
        return Err(unavailable());
    }
    let (address, rest, header) = match provider {
        "plex" => {
            let (source, path) = rest.split_once('/').ok_or_else(unavailable)?;
            let source = source.parse::<i64>().map_err(|_| unavailable())?;
            let connection = sources
                .plex_artwork_connection_admin(source)
                .await?
                .ok_or_else(unavailable)?;
            (
                connection.address,
                path.to_owned(),
                ("X-Plex-Token", connection.token),
            )
        }
        "jellyfin" => (
            sources
                .first_jellyfin_address_admin()
                .await?
                .ok_or_else(unavailable)?,
            rest.to_owned(),
            (
                "Authorization",
                crate::jellyfin::format_jellyfin_auth_header(
                    config
                        .get("jellyfin.current_server_api_key")
                        .await?
                        .ok_or_else(unavailable)?,
                ),
            ),
        ),
        _ => (
            sources
                .first_emby_address_admin()
                .await?
                .ok_or_else(unavailable)?,
            rest.to_owned(),
            (
                "X-Emby-Token",
                config
                    .get("emby.current_server_api_key")
                    .await?
                    .ok_or_else(unavailable)?,
            ),
        ),
    };
    let url = reqwest::Url::parse(&format!("{}/{rest}", address.trim_end_matches('/')))
        .map_err(|_| unavailable())?;
    Ok(Some(ArtworkRequest {
        url,
        header: Some(header),
    }))
}

/// Owned native assets. Dropped futures retain files until reaping is confirmed.
pub struct NativeSongBackground {
    #[cfg(test)]
    used_artwork_fallback: AtomicBool,
    pub path: PathBuf,
    pub stream: SourceVideoStream,
    pub with_progress: bool,
    directory: Option<tempfile::TempDir>,
    progress_mask: PathBuf,
    retain_on_drop: AtomicBool,
    rendered: AtomicBool,
    render_commands: std::sync::Mutex<Option<(RemoteStreamCommand, Option<RemoteStreamCommand>)>>,
}
impl Drop for NativeSongBackground {
    fn drop(&mut self) {
        if self.retain_on_drop.load(Ordering::Acquire) {
            if let Some(directory) = self.directory.take() {
                let _ = directory.keep();
            }
        }
    }
}
impl NativeSongBackground {
    /// Hold assets before creating each native encoder attempt.
    pub fn mark_in_use(&self) {
        self.retain_on_drop.store(true, Ordering::Release);
    }
    /// Confirm the last consuming process has been reaped before the owner drops.
    pub fn confirm_reaped(&self) {
        self.retain_on_drop.store(false, Ordering::Release);
    }
    /// Whether a renderer/consumer may still own these assets after its
    /// supervisor returns. Admission must remain quarantined in that case.
    #[must_use]
    pub fn cleanup_unconfirmed(&self) -> bool {
        self.retain_on_drop.load(Ordering::Acquire)
    }
    /// Remove assets after the consuming encoder has been reaped.
    pub async fn remove_after_reap(mut self) {
        if let Some(directory) = self.directory.take() {
            let _ = tokio::fs::remove_dir_all(directory.keep()).await;
        }
    }
    /// Upstream replaces ordinary native watermarks with this progress mask.
    #[must_use]
    pub fn watermarks(&self) -> Vec<(InputFile, WatermarkOverlay)> {
        if !self.with_progress {
            return Vec::new();
        }
        vec![(
            InputFile::watermark(self.progress_mask.to_string_lossy()),
            WatermarkOverlay {
                location: WatermarkLocation::TopLeft,
                width_percent: 100.0,
                horizontal_margin_percent: 0.0,
                vertical_margin_percent: 0.0,
                ..WatermarkOverlay::default()
            },
        )]
    }

    /// Keep existing audio/subtitle selection while replacing only the picture.
    pub fn apply(&self, request: &mut ersatzrs_ffmpeg::PipelineRequest) {
        // Native playback/troubleshooting already rebases extracted text inputs
        // (subtitles::rebase_burned_text_subtitle_input). Preserve that zero clock.
        request.state.song_text_subtitle_start = Some(Duration::ZERO);
        *request = request.clone().with_song_background(
            InputFile::video(self.path.to_string_lossy()),
            self.stream.clone(),
            self.with_progress,
        );
        if self.with_progress {
            request
                .inputs
                .retain(|input| input.kind != ersatzrs_ffmpeg::InputKind::Watermark);
            request.watermarks.clear();
            request.watermark = None;
            for (input, overlay) in self.watermarks() {
                request.inputs.push(input);
                request.watermarks.push(overlay);
            }
        }
    }
}

/// Prepare an owned native picture; non-song media return `None`.
/// # Errors
/// Returns redacted database, render, cancellation or deadline errors.
#[expect(
    clippy::too_many_arguments,
    reason = "One shared bounded adapter carries occurrence, target and cancellation ownership"
)]
pub async fn prepare_native(
    pool: &DbPool,
    media_item_id: i64,
    channel_id: i64,
    resolution: &Resolution,
    seed: i64,
    ffmpeg: &Path,
    deadline: Instant,
    cancellation: oneshot::Receiver<()>,
) -> std::result::Result<Option<NativeSongBackground>, NativeSongPreparationError> {
    prepare_native_with_reservation(
        pool,
        media_item_id,
        channel_id,
        resolution,
        seed,
        ffmpeg,
        deadline,
        cancellation,
        None,
    )
    .await
}

struct CancelNativePreparation(Option<oneshot::Sender<()>>);

impl Drop for CancelNativePreparation {
    fn drop(&mut self) {
        if let Some(sender) = self.0.take() {
            let _ = sender.send(());
        }
    }
}

struct NativePreparationAdmission {
    reservation: Option<crate::transcode_capacity::ActiveTranscodeReservation>,
    renderer_may_be_alive: bool,
}

impl Drop for NativePreparationAdmission {
    fn drop(&mut self) {
        if self.renderer_may_be_alive {
            if let Some(reservation) = &self.reservation {
                reservation.retain_until_restart();
            }
        }
    }
}

/// Prepare under the caller's admitted slot. Dropping the request cancels the
/// owned worker, which retains its lease until bounded renderer cleanup ends.
/// # Errors
/// Returns database/render errors or explicit unconfirmed process termination.
#[expect(
    clippy::too_many_arguments,
    reason = "Occurrence, target and owned admission form one bounded preparation"
)]
pub async fn prepare_native_with_reservation(
    pool: &DbPool,
    media_item_id: i64,
    channel_id: i64,
    resolution: &Resolution,
    seed: i64,
    ffmpeg: &Path,
    deadline: Instant,
    mut cancellation: oneshot::Receiver<()>,
    reservation: Option<crate::transcode_capacity::ActiveTranscodeReservation>,
) -> std::result::Result<Option<NativeSongBackground>, NativeSongPreparationError> {
    let pool = pool.clone();
    let resolution = resolution.clone();
    let ffmpeg = ffmpeg.to_owned();
    let (sender, receiver) = oneshot::channel();
    let mut cancel_on_drop = CancelNativePreparation(Some(sender));
    let mut task = tokio::spawn(async move {
        prepare_native_owned(
            &pool,
            media_item_id,
            channel_id,
            &resolution,
            seed,
            &ffmpeg,
            deadline,
            receiver,
            reservation,
        )
        .await
    });
    let result = tokio::select! {
        result = &mut task => result,
        _ = &mut cancellation => {
            if let Some(sender) = cancel_on_drop.0.take() { let _ = sender.send(()); }
            task.await
        }
    };
    result.map_err(|_| NativeSongPreparationError::TerminationUnconfirmed)?
}

#[expect(
    clippy::too_many_arguments,
    reason = "Owned counterpart of the bounded preparation adapter"
)]
async fn prepare_native_owned(
    pool: &DbPool,
    media_item_id: i64,
    channel_id: i64,
    resolution: &Resolution,
    seed: i64,
    ffmpeg: &Path,
    deadline: Instant,
    mut cancellation: oneshot::Receiver<()>,
    reservation: Option<crate::transcode_capacity::ActiveTranscodeReservation>,
) -> std::result::Result<Option<NativeSongBackground>, NativeSongPreparationError> {
    let Some(owned) = stage_native_owned(
        pool,
        media_item_id,
        channel_id,
        resolution,
        seed,
        ffmpeg,
        deadline,
        &mut cancellation,
    )
    .await?
    else {
        return Ok(None);
    };
    let owned = std::sync::Arc::new(owned);
    render_native_owned(
        std::sync::Arc::clone(&owned),
        deadline,
        &mut cancellation,
        reservation,
    )
    .await?;
    std::sync::Arc::try_unwrap(owned)
        .map(Some)
        .map_err(|_| unavailable().into())
}

/// Stage native assets without starting a renderer. Capacity wait streams may
/// build a command from this owner, then render after admission is acquired.
/// # Errors
/// Returns bounded asset preparation or database errors.
#[expect(
    clippy::too_many_arguments,
    reason = "Same occurrence and target as native preparation"
)]
pub async fn stage_native(
    pool: &DbPool,
    media_item_id: i64,
    channel_id: i64,
    resolution: &Resolution,
    seed: i64,
    ffmpeg: &Path,
    deadline: Instant,
    mut cancellation: oneshot::Receiver<()>,
) -> std::result::Result<Option<NativeSongBackground>, NativeSongPreparationError> {
    stage_native_owned(
        pool,
        media_item_id,
        channel_id,
        resolution,
        seed,
        ffmpeg,
        deadline,
        &mut cancellation,
    )
    .await
}

#[expect(
    clippy::too_many_arguments,
    reason = "Private staging counterpart of native preparation"
)]
async fn stage_native_owned(
    pool: &DbPool,
    media_item_id: i64,
    channel_id: i64,
    resolution: &Resolution,
    seed: i64,
    ffmpeg: &Path,
    deadline: Instant,
    cancellation: &mut oneshot::Receiver<()>,
) -> std::result::Result<Option<NativeSongBackground>, NativeSongPreparationError> {
    let prepare_assets = async {
        let song: i64 = sqlx::query_scalar(r#"SELECT EXISTS(SELECT 1 FROM "Song" WHERE "Id" = ?)"#)
            .bind(media_item_id)
            .fetch_one(pool)
            .await
            .repository()?;
        if song == 0 {
            return Ok::<_, RepositoryError>(None);
        }
        let mode: i64 =
            sqlx::query_scalar(r#"SELECT "SongVideoMode" FROM "Channel" WHERE "Id" = ?"#)
                .bind(channel_id)
                .fetch_optional(pool)
                .await
                .repository()?
                .unwrap_or_default();
        let mode = if mode == 1 {
            ChannelSongVideoMode::WithProgress
        } else {
            ChannelSongVideoMode::Default
        };
        let directory = tempfile::Builder::new()
            .prefix("ersatzrs-native-song-")
            .tempdir()
            .map_err(|_| unavailable())?;
        let size = dimensions(resolution)?;
        let mut owned = NativeSongBackground {
            #[cfg(test)]
            used_artwork_fallback: AtomicBool::new(false),
            path: directory.path().join("song.png"),
            stream: SourceVideoStream::new(SourceVideoCodec::Unknown),
            with_progress: mode == ChannelSongVideoMode::WithProgress,
            progress_mask: directory.path().join("progress.png"),
            directory: Some(directory),
            retain_on_drop: AtomicBool::new(false),
            rendered: AtomicBool::new(false),
            render_commands: std::sync::Mutex::new(None),
        };
        let directory = owned
            .directory
            .as_ref()
            .expect("owned private directory")
            .path();
        let recipe = prepare(
            pool,
            media_item_id,
            resolution,
            mode,
            Some(seed),
            directory,
            ffmpeg,
        )
        .await?;
        owned.stream = recipe.stream;
        let (name, builtin): (&str, &[u8]) =
            if (f64::from(size.width) / f64::from(size.height) - 4.0 / 3.0).abs() < 0.01 {
                (
                    "song_progress_overlay_43.png",
                    include_bytes!("../../../assets/song_progress_overlay_43.png"),
                )
            } else {
                (
                    "song_progress_overlay.png",
                    include_bytes!("../../../assets/song_progress_overlay.png"),
                )
            };
        resource(name, builtin, &owned.progress_mask).await?;
        *owned
            .render_commands
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            Some((recipe.command, recipe.fallback_command));
        Ok(Some(owned))
    };
    let prepared = tokio::select! {
        result = tokio::time::timeout_at(deadline, prepare_assets) => result.map_err(|_| unavailable())??,
        _ = &mut *cancellation => return Err(unavailable().into()),
    };
    Ok(prepared)
}

/// Render a staged picture under the existing consumer admission. Cancellation
/// or request drop delegates bounded process reaping to the owned worker.
/// # Errors
/// Returns renderer failure or explicit unconfirmed termination.
pub async fn render_native(
    owned: std::sync::Arc<NativeSongBackground>,
    deadline: Instant,
    mut cancellation: oneshot::Receiver<()>,
    reservation: Option<crate::transcode_capacity::ActiveTranscodeReservation>,
) -> std::result::Result<(), NativeSongPreparationError> {
    let (sender, mut receiver) = oneshot::channel();
    let mut cancel_on_drop = CancelNativePreparation(Some(sender));
    let mut task = tokio::spawn(async move {
        render_native_owned(owned, deadline, &mut receiver, reservation).await
    });
    let result = tokio::select! {
        result = &mut task => result,
        _ = &mut cancellation => {
            if let Some(sender) = cancel_on_drop.0.take() { let _ = sender.send(()); }
            task.await
        }
    };
    result.map_err(|_| NativeSongPreparationError::TerminationUnconfirmed)?
}

async fn render_native_owned(
    owned: std::sync::Arc<NativeSongBackground>,
    deadline: Instant,
    cancellation: &mut oneshot::Receiver<()>,
    reservation: Option<crate::transcode_capacity::ActiveTranscodeReservation>,
) -> std::result::Result<(), NativeSongPreparationError> {
    if owned.rendered.load(Ordering::Acquire) {
        return Ok(());
    }
    let (command, fallback) = owned
        .render_commands
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take()
        .ok_or_else(unavailable)?;
    let mut admission = NativePreparationAdmission {
        reservation,
        renderer_may_be_alive: false,
    };
    for (attempt, command) in std::iter::once(command).chain(fallback).enumerate() {
        #[cfg(not(test))]
        let _ = attempt;
        if Instant::now() >= deadline
            || !matches!(
                cancellation.try_recv(),
                Err(oneshot::error::TryRecvError::Empty)
            )
        {
            return Err(unavailable().into());
        }
        let mut child = crate::remote_stream::spawn_remote_stream_producer(&command)
            .map_err(|_| unavailable())?;
        owned.mark_in_use();
        admission.renderer_may_be_alive = true;
        let mut reaped = false;
        let rendered = {
            let (Some(stdout), Some(stderr)) = (child.stdout().take(), child.stderr().take())
            else {
                if matches!(
                    tokio::time::timeout(Duration::from_secs(5), child.kill()).await,
                    Ok(Ok(()))
                ) {
                    owned.confirm_reaped();
                    admission.renderer_may_be_alive = false;
                    return Err(unavailable().into());
                }
                return Err(NativeSongPreparationError::TerminationUnconfirmed);
            };
            let mut stdout =
                stdout.take(u64::try_from(MAX_IMAGE_BYTES + 1).expect("bounded image size"));
            let mut stderr = stderr.take(64 * 1024);
            let render = async {
                let mut image = Vec::new();
                let mut errors = Vec::new();
                let (image_result, error_result, status) = tokio::join!(
                    stdout.read_to_end(&mut image),
                    stderr.read_to_end(&mut errors),
                    child.wait()
                );
                reaped = status.is_ok();
                if image_result.is_err()
                    || error_result.is_err()
                    || !status.is_ok_and(|s| s.success())
                    || image.len() > MAX_IMAGE_BYTES
                    || !image.starts_with(b"\x89PNG\r\n\x1a\n")
                {
                    return Err(unavailable());
                }
                tokio::fs::write(&owned.path, image)
                    .await
                    .map_err(|_| unavailable())
            };
            tokio::pin!(render);
            tokio::select! { result = &mut render => Some(result), () = tokio::time::sleep_until(deadline) => None, _ = &mut *cancellation => None }
        };
        if matches!(rendered, Some(Ok(()))) {
            #[cfg(test)]
            {
                owned
                    .used_artwork_fallback
                    .store(attempt > 0, Ordering::Release);
            }
            owned.confirm_reaped();
            admission.renderer_may_be_alive = false;
            owned.rendered.store(true, Ordering::Release);
            return Ok(());
        }
        if !reaped {
            reaped = matches!(
                tokio::time::timeout(Duration::from_secs(5), child.kill()).await,
                Ok(Ok(()))
            );
        }
        if !reaped {
            return Err(NativeSongPreparationError::TerminationUnconfirmed);
        }
        owned.confirm_reaped();
        admission.renderer_may_be_alive = false;
        if rendered.is_none() {
            return Err(unavailable().into());
        }
        // A decoder-invalid but signature-valid optional cover gets exactly one
        // bounded default-art retry. The first managed child is already reaped.
    }
    Err(unavailable().into())
}

#[cfg(test)]
#[path = "song_background_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "song_background_runtime_tests.rs"]
mod runtime_tests;
