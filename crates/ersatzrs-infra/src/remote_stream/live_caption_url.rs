//! One stream-copy producer for advertised native Live URL teletext input.
//! Stored metadata admits inspection only. Page selection still requires the
//! existing fresh, bounded producer-prefix qualification (ADR 0121).

use ersatzrs_core::repositories::Result;
use ersatzrs_domain::RemoteStreamLiveness;

use super::{
    remote_url_input_options, url_scheme, RemoteStreamCommand, RemoteStreamSource,
    ResolvedRemoteStream,
};
use crate::{db::DbPool, repositories::SqlxResultExt as _};

/// Keep ordinary URL playback unchanged unless this exact resolved version
/// advertises embedded teletext. The returned command opens the URL once and
/// copies every track, without selecting captions, seeking or transcoding.
/// Fresh producer observations, not these stored rows, authorize page selection.
///
/// # Errors
/// Returns database errors while checking the resolved version's inventory.
pub async fn native_live_url_caption_producer(
    pool: &DbPool,
    media_item_id: i64,
    media_version_id: i64,
    stream: &ResolvedRemoteStream,
    capabilities: &ersatzrs_ffmpeg::FfmpegCapabilityReport,
) -> Result<Option<RemoteStreamCommand>> {
    let RemoteStreamSource::Url(url) = &stream.source else {
        return Ok(None);
    };
    if stream.liveness != RemoteStreamLiveness::Live
        || !url_scheme(url).is_some_and(|scheme| {
            ["http", "https", "rtsp", "rtsps"]
                .iter()
                .any(|candidate| scheme.eq_ignore_ascii_case(candidate))
        })
    {
        return Ok(None);
    }
    if !crate::ffmpeg_config::use_embedded_subtitles(pool).await? {
        return Ok(None);
    }
    let advertised: bool = sqlx::query_scalar(
        r#"SELECT EXISTS (
            SELECT 1 FROM "MediaStream" ms
            JOIN "MediaVersion" mv ON mv."Id"=ms."MediaVersionId"
            WHERE mv."Id"=? AND mv."RemoteStreamId"=?
                AND ms."MediaStreamKind"=3 AND LOWER(ms."Codec")='dvb_teletext'
        )"#,
    )
    .bind(media_version_id)
    .bind(media_item_id)
    .fetch_one(pool)
    .await
    .repository()?;
    Ok(advertised.then(|| copy_producer(url, capabilities)))
}

fn copy_producer(
    url: &str,
    capabilities: &ersatzrs_ffmpeg::FfmpegCapabilityReport,
) -> RemoteStreamCommand {
    let program = capabilities
        .runtime_identity
        .as_ref()
        .map_or_else(ersatzrs_ffmpeg::ffmpeg_path, |identity| {
            identity.ffmpeg_path.clone()
        });
    let mut args = [
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostdin",
        "-copyts",
        "-copy_unknown",
    ]
    .map(str::to_owned)
    .to_vec();
    if capabilities.teletext_decoder.preserves_original_pts() {
        args.extend(["-fflags".to_owned(), "+preserve_teletext_pts".to_owned()]);
    }
    args.extend(remote_url_input_options(url));
    args.extend(["-i", url, "-map", "0", "-c", "copy"].map(str::to_owned));
    // Reuse the established sparse-subtitle mux bound; it does not authorize
    // caption timing, which requires the separate original-PES input contract.
    args.extend(
        ersatzrs_ffmpeg::sparse_subtitle_interleave()
            .output_options()
            .iter()
            .cloned(),
    );
    args.extend(["-mpegts_copyts", "1", "-f", "mpegts", "pipe:1"].map(str::to_owned));
    RemoteStreamCommand {
        tool_paths: vec![std::path::PathBuf::from(&program)],
        program,
        args,
        working_directory: std::env::temp_dir(),
        environment: native_input_environment(crate::startup_config::environment_variable),
    }
}

// This is the trusted configured FFmpeg input, not an installed add-on. Keep
// its existing network trust/proxy and loader settings across the managed
// producer's cleared environment, without forwarding application credentials.
fn native_input_environment(
    mut lookup: impl FnMut(&str) -> Option<std::ffi::OsString>,
) -> std::collections::BTreeMap<String, String> {
    [
        "http_proxy",
        "https_proxy",
        "all_proxy",
        "no_proxy",
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "NO_PROXY",
        "SSL_CERT_FILE",
        "SSL_CERT_DIR",
        "OPENSSL_CONF",
        "LD_LIBRARY_PATH",
        "DYLD_LIBRARY_PATH",
        "DYLD_FALLBACK_LIBRARY_PATH",
    ]
    .into_iter()
    .filter_map(|name| {
        lookup(name)
            .and_then(|value| value.into_string().ok())
            .map(|value| (name.to_owned(), value))
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::{sqlite::SqlitePoolOptions, Executor as _};

    fn live(url: &str) -> ResolvedRemoteStream {
        ResolvedRemoteStream {
            source: RemoteStreamSource::Url(url.into()),
            liveness: RemoteStreamLiveness::Live,
            is_live: true,
            configured_duration: None,
            fallback_query: None,
            uses_addon: false,
        }
    }

    #[test]
    fn copy_producer_keeps_all_tracks_original_clocks_and_url_protocol_options() {
        let mut snapshots = Vec::new();
        for url in ["https://example.test/live", "rtsp://example.test/live"] {
            let mut capabilities = ersatzrs_ffmpeg::FfmpegCapabilityReport::default();
            capabilities.teletext_decoder.status = ersatzrs_ffmpeg::TeletextDecoderStatus::Ready;
            capabilities.teletext_decoder.original_pts = true;
            let command = copy_producer(url, &capabilities);
            snapshots.push(command.args.join("\n"));
            assert_eq!(
                command.tool_paths,
                [std::path::PathBuf::from(&command.program)]
            );
        }
        insta::assert_snapshot!(snapshots.join("\n---\n"));
    }

    #[test]
    fn older_runtime_keeps_copy_av_without_requesting_the_unknown_clock_flag() {
        let command = copy_producer(
            "https://example.test/live",
            &ersatzrs_ffmpeg::FfmpegCapabilityReport::default(),
        );
        assert!(!command
            .args
            .iter()
            .any(|arg| arg.contains("preserve_teletext_pts")));
        assert!(command.args.windows(2).any(|pair| pair == ["-map", "0"]));
        assert!(command.args.windows(2).any(|pair| pair == ["-c", "copy"]));
        assert!(command
            .args
            .windows(2)
            .any(|pair| pair == ["-max_interleave_delta", "100000"]));
    }

    #[test]
    fn native_input_forwards_only_network_and_runtime_environment_names() {
        let mut inspected = Vec::new();
        let environment = native_input_environment(|name| {
            inspected.push(name.to_owned());
            Some(std::ffi::OsString::from("<configured-value>"))
        });
        for name in ["http_proxy", "NO_PROXY", "SSL_CERT_FILE", "LD_LIBRARY_PATH"] {
            assert_eq!(environment[name], "<configured-value>");
        }
        assert!(!inspected.iter().any(|name| name == "APP_SECRET"));
        assert_eq!(environment.len(), inspected.len());
    }

    #[test]
    fn native_proxy_values_are_redacted_from_producer_failure_messages() {
        for name in ["http_proxy", "HTTPS_PROXY", "all_proxy", "NO_PROXY"] {
            let command = RemoteStreamCommand {
                program: "ffmpeg".into(),
                tool_paths: Vec::new(),
                args: Vec::new(),
                working_directory: std::path::PathBuf::new(),
                environment: [(name.to_owned(), "<configured-proxy>".to_owned())].into(),
            };
            let secrets = crate::remote_stream::remote_stream_environment_secrets(&command);
            let diagnostic = crate::remote_stream::producer_operation_diagnostic(
                "native input failed through <configured-proxy>",
                &secrets,
                false,
            );
            assert_eq!(diagnostic.message, "native input failed through [redacted]");
        }
    }

    #[tokio::test]
    async fn eligibility_requires_the_exact_remote_version_and_actual_subtitle_kind() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        pool.execute(
            r#"CREATE TABLE "ConfigElement" ("Key" TEXT, "Value" TEXT);
            CREATE TABLE "MediaVersion" ("Id" INTEGER, "RemoteStreamId" INTEGER);
            CREATE TABLE "MediaStream" ("MediaVersionId" INTEGER, "MediaStreamKind" INTEGER, "Codec" TEXT);
            INSERT INTO "MediaVersion" VALUES (10,20),(11,21),(12,20);
            INSERT INTO "MediaStream" VALUES (10,3,'dvb_teletext'),(11,3,'dvb_teletext'),
                (12,2,'aac'),(12,1,'dvb_teletext');"#,
        )
        .await
        .unwrap();
        let stream = live("https://example.test/live");
        assert!(native_live_url_caption_producer(
            &pool,
            20,
            10,
            &stream,
            &ersatzrs_ffmpeg::FfmpegCapabilityReport::default()
        )
        .await
        .unwrap()
        .is_some());
        for (item, version) in [(21, 10), (20, 11), (20, 12), (20, 99)] {
            assert!(native_live_url_caption_producer(
                &pool,
                item,
                version,
                &stream,
                &ersatzrs_ffmpeg::FfmpegCapabilityReport::default()
            )
            .await
            .unwrap()
            .is_none());
        }
        sqlx::query(r#"INSERT INTO "ConfigElement" VALUES (?,'false')"#)
            .bind(ersatzrs_domain::CONFIG_KEY_FFMPEG_USE_EMBEDDED_SUBTITLES)
            .execute(&pool)
            .await
            .unwrap();
        pool.execute(r#"DROP TABLE "MediaStream""#).await.unwrap();
        assert!(native_live_url_caption_producer(
            &pool,
            20,
            10,
            &stream,
            &ersatzrs_ffmpeg::FfmpegCapabilityReport::default()
        )
        .await
        .unwrap()
        .is_none());
        pool.close().await;
    }

    #[tokio::test]
    async fn finite_and_other_protocols_preserve_ordinary_playback_without_sql() {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        pool.close().await;
        let mut finite = live("https://example.test/finite");
        finite.liveness = RemoteStreamLiveness::Finite;
        finite.is_live = false;
        for stream in [finite, live("udp://example.test:1234")] {
            assert!(native_live_url_caption_producer(
                &pool,
                20,
                10,
                &stream,
                &ersatzrs_ffmpeg::FfmpegCapabilityReport::default()
            )
            .await
            .unwrap()
            .is_none());
        }
    }
}
