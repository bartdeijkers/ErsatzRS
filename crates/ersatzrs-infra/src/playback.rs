//! Shared helpers for building FFmpeg playback pipelines from persisted media
//! metadata, used by both the MPEG-TS and HLS transcode paths.

use ersatzrs_domain::VideoScanKind;
use ersatzrs_ffmpeg::{PixelFormat, SourceColorParams, SourceVideoCodec, SourceVideoStream};
use sqlx::Row;

use crate::db::DbPool;

/// Load the source video-stream metadata the FFmpeg pipeline needs to make
/// ErsatzTV-parity decisions (decoder selection, deinterlace, tonemap, and the
/// 10-bit → 8-bit pixel-format conversion handled by `SetPixelFormat`).
///
/// Returns `None` when the media version has no recorded video stream, in
/// which case the pipeline falls back to its format-agnostic defaults.
pub async fn load_source_video_stream(
    pool: &DbPool,
    media_version_id: i64,
) -> Option<SourceVideoStream> {
    let row = sqlx::query(
        r#"SELECT ms."Codec" AS "Codec",
                  ms."PixelFormat" AS "PixelFormat",
                  ms."ColorTransfer" AS "ColorTransfer",
                  ms."ColorPrimaries" AS "ColorPrimaries",
                  ms."ColorSpace" AS "ColorSpace",
                  mv."VideoScanKind" AS "VideoScanKind"
           FROM "MediaVersion" mv
           JOIN "MediaStream" ms
               ON ms."MediaVersionId" = mv."Id" AND ms."MediaStreamKind" = 1
           WHERE mv."Id" = ?
           ORDER BY ms."Index"
           LIMIT 1"#,
    )
    .bind(media_version_id)
    .fetch_optional(pool)
    .await
    .ok()??;

    let codec: String = row.try_get("Codec").ok()?;
    let pixel_format: Option<String> = row.try_get("PixelFormat").ok().flatten();
    let color_transfer: Option<String> = row.try_get("ColorTransfer").ok().flatten();
    let color_primaries: Option<String> = row.try_get("ColorPrimaries").ok().flatten();
    let color_space: Option<String> = row.try_get("ColorSpace").ok().flatten();
    let scan_kind = row
        .try_get::<i64, _>("VideoScanKind")
        .ok()
        .map_or(VideoScanKind::Unknown, scan_kind_from_db);

    let mut stream = SourceVideoStream::new(SourceVideoCodec::from_ffmpeg_name(&codec));
    stream.pixel_format = pixel_format
        .as_deref()
        .and_then(PixelFormat::from_ffmpeg_name);
    stream.scan_kind = scan_kind;
    stream.color_params = derive_color_params(
        pixel_format.as_deref(),
        color_transfer.as_deref(),
        color_primaries.as_deref(),
        color_space.as_deref(),
    );

    Some(stream)
}

const fn scan_kind_from_db(value: i64) -> VideoScanKind {
    match value {
        1 => VideoScanKind::Progressive,
        2 => VideoScanKind::Interlaced,
        _ => VideoScanKind::Unknown,
    }
}

/// Derive HDR color metadata from ffprobe color tags, mirroring ErsatzTV's
/// HDR/BT.2020 detection (PQ/HLG transfer => HDR; BT.2020 primaries on a 10-bit
/// stream => the BT.2020 ten-bit colorspace path).
fn derive_color_params(
    pixel_format: Option<&str>,
    color_transfer: Option<&str>,
    color_primaries: Option<&str>,
    color_space: Option<&str>,
) -> SourceColorParams {
    let transfer = color_transfer.unwrap_or_default().to_ascii_lowercase();
    let is_hdr = transfer.contains("smpte2084") || transfer.contains("arib-std-b67");

    let is_ten_bit = pixel_format
        .unwrap_or_default()
        .to_ascii_lowercase()
        .contains("10");
    let is_bt2020 = color_primaries
        .unwrap_or_default()
        .to_ascii_lowercase()
        .contains("bt2020")
        || color_space
            .unwrap_or_default()
            .to_ascii_lowercase()
            .contains("bt2020");

    SourceColorParams {
        is_hdr,
        is_bt2020_ten: is_bt2020 && is_ten_bit,
    }
}
