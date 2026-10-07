use super::super::{
    async_trait, sqlx_error, Db, HealthCheck, HealthCheckResult, HealthCheckStatus, Pool, Result,
};
use std::{
    io,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::io::AsyncReadExt as _;

// Inspection budgets only: exceeding either limit leaves validation incomplete,
// without imposing a new limit on what the Next engine may load.
const MAX_OVERLAY_BYTES: u64 = 1024 * 1024;
const OVERLAY_CHECK_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) struct NextRuntimeHealthCheck {
    pool: Pool<Db>,
    resolve: fn() -> io::Result<PathBuf>,
    overlays: PathBuf,
}

impl NextRuntimeHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self {
            pool,
            resolve: crate::next_engine::host::channel_binary,
            overlays: crate::file_system_layout::next_channel_config_overlays_folder(),
        }
    }
}

#[async_trait]
impl HealthCheck for NextRuntimeHealthCheck {
    fn title(&self) -> &'static str {
        "Next Runtime Availability"
    }
    async fn check(&self) -> Result<HealthCheckResult> {
        if (self.resolve)().is_err() {
            return Ok(HealthCheckResult::new(self.title(), HealthCheckStatus::Fail,
                "ErsatzRS requires the accepted bundled or managed Next runtime, or a valid ETV_NEXT_FOLDER override",
                "Next executable is unavailable").with_link("/channels"));
        }
        if crate::next_engine::manager::validate_published()
            .await
            .is_err()
        {
            return Ok(HealthCheckResult::new(self.title(), HealthCheckStatus::Fail,
                "The accepted Next runtime has changed or no longer passes its bounded version probe",
                "Next runtime validation failed").with_link("/channels"));
        }
        let channels: Vec<(i64, String)> = sqlx::query_as(r#"SELECT "Id", "Number" FROM "Channel" WHERE "StreamingEngine"=1 AND "IsEnabled"=1 ORDER BY "Id""#)
            .fetch_all(&self.pool).await.map_err(sqlx_error)?;
        if channels.is_empty() {
            return Ok(HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::Pass,
                "The required Next runtime is available; no enabled channel needs profile validation",
                "Next runtime is available",
            ));
        }
        let mut invalid = 0;
        for (channel, _) in &channels {
            let profile =
                crate::playback::load_channel_playback_profile(&self.pool, *channel).await;
            let supported = matches!(profile, Ok(Some(ref profile)) if crate::next_engine::config::channel_config(profile, ersatzrs_domain::NextEngineTextSubtitleMode::Burn,
                Path::new("next-playouts"), Path::new("ffmpeg"), Path::new("ffprobe")).is_ok());
            if !supported {
                invalid += 1;
            }
        }
        if invalid > 0 {
            return Ok(HealthCheckResult::new(self.title(), HealthCheckStatus::Fail,
                format!("{invalid} enabled Next channel profile(s) are unavailable or unsupported; review codecs, HDR, resolution and acceleration"),
                "Next channel profile is unavailable or unsupported").with_link("/channels"));
        }
        let checked = tokio::time::timeout(
            OVERLAY_CHECK_TIMEOUT,
            overlays_available(&self.overlays, &channels),
        )
        .await;
        let (status, message) = match checked {
            Ok(Ok(())) => return Ok(HealthCheckResult::pass(self.title())),
            Ok(Err(OverlayCheckError::Invalid)) => (
                HealthCheckStatus::Fail,
                "Next configuration overlay is unavailable or invalid",
            ),
            Ok(Err(OverlayCheckError::Schema)) => (
                HealthCheckStatus::Fail,
                "Next configuration overlay needs a supported version URI (0.1.0 or 0.1.1); review copy settings before upgrading an older overlay",
            ),
            Ok(Err(OverlayCheckError::Incomplete)) | Err(_) => (
                HealthCheckStatus::Warning,
                "Next configuration overlay validation is incomplete",
            ),
        };
        Ok(HealthCheckResult::new(self.title(), status, message, message).with_link("/channels"))
    }
}

#[derive(Debug, PartialEq, Eq)]
enum OverlayCheckError {
    Invalid,
    Schema,
    Incomplete,
}

impl From<io::Error> for OverlayCheckError {
    fn from(_: io::Error) -> Self {
        Self::Invalid
    }
}

// Check availability, shape and the pinned runtime's version boundary. Next
// owns merging and effective field validation; this does not prove playback.
async fn overlays_available(
    folder: &Path,
    channels: &[(i64, String)],
) -> std::result::Result<(), OverlayCheckError> {
    check_overlay(&folder.join("default.json")).await?;
    for (_, number) in channels {
        if number.is_empty()
            || number.matches('.').count() > 1
            || !number
                .split('.')
                .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
        {
            return Err(OverlayCheckError::Invalid);
        }
        check_overlay(&folder.join(format!("{number}.json"))).await?;
    }
    Ok(())
}

async fn check_overlay(path: &Path) -> std::result::Result<(), OverlayCheckError> {
    let metadata = match tokio::fs::metadata(path).await {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_file() {
        return Err(OverlayCheckError::Invalid);
    }
    if metadata.len() > MAX_OVERLAY_BYTES {
        return Err(OverlayCheckError::Incomplete);
    }
    let mut bytes = Vec::new();
    tokio::fs::File::open(path)
        .await?
        .take(MAX_OVERLAY_BYTES + 1)
        .read_to_end(&mut bytes)
        .await?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_OVERLAY_BYTES {
        return Err(OverlayCheckError::Incomplete);
    }
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| OverlayCheckError::Invalid)?;
    if !value.is_object() {
        return Err(OverlayCheckError::Invalid);
    }
    // Mirrors Next VersionedSchema: major zero, matching breaking version and
    // no newer compatible revision. Never relabel an old format:null overlay:
    // that used to request copy, but now selects a transcode default.
    let version = value["version"]
        .as_str()
        .and_then(|uri| uri.strip_prefix("https://ersatztv.org/channel/version/0."))
        .and_then(|version| version.split_once('.'))
        .and_then(|(breaking, compatible)| {
            Some((
                breaking.parse::<u32>().ok()?,
                compatible.parse::<u32>().ok()?,
            ))
        });
    if !version.is_some_and(|(breaking, compatible)| breaking == 1 && compatible <= 1) {
        return Err(OverlayCheckError::Schema);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn next_runtime_is_always_required_and_only_selected_profiles_are_checked() {
        let (pool, _, _, _) = crate::next_engine::schedule::tests::fixture().await;
        let overlays = tempfile::tempdir().unwrap();
        let mut check = NextRuntimeHealthCheck {
            pool: pool.clone(),
            resolve: || Err(io::Error::from(io::ErrorKind::NotFound)),
            overlays: overlays.path().to_owned(),
        };
        let missing_unused = check.check().await.unwrap();
        assert_eq!(missing_unused.status, HealthCheckStatus::Fail);
        assert!(missing_unused.brief_message.contains("executable"));
        check.resolve = || Ok(PathBuf::from("synthetic-engine"));
        assert_eq!(
            check.check().await.unwrap().status,
            HealthCheckStatus::Pass,
            "runtime availability is mandatory before a Next channel exists; profile checks are not"
        );
        check.resolve = || Err(io::Error::from(io::ErrorKind::NotFound));
        sqlx::query(r#"UPDATE "Channel" SET "StreamingEngine"=1, "IsEnabled"=1"#)
            .execute(&pool)
            .await
            .unwrap();
        let missing = check.check().await.unwrap();
        assert_eq!(missing.status, HealthCheckStatus::Fail);
        assert!(missing.brief_message.contains("executable"));
        check.resolve = || Ok(PathBuf::from("synthetic-engine"));
        assert_eq!(check.check().await.unwrap().status, HealthCheckStatus::Fail);
        sqlx::query(r#"UPDATE "FFmpegProfile" SET "VideoFormat"=1, "AudioFormat"=1"#)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(check.check().await.unwrap().status, HealthCheckStatus::Pass);
        sqlx::query(
            r#"UPDATE "FFmpegProfile" SET "VideoFormat"=3, "BitDepth"=1, "VideoBitrate"=2000"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(
            check.check().await.unwrap().status,
            HealthCheckStatus::Pass,
            "MPEG-2 with a ten-bit preference is projected to supported eight-bit output"
        );
        sqlx::query(r#"UPDATE "FFmpegProfile" SET "VideoBitrate"=0"#)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            check.check().await.unwrap().status,
            HealthCheckStatus::Fail,
            "health must reject MPEG-2 without the bitrate required by Next"
        );
        sqlx::query(r#"UPDATE "FFmpegProfile" SET "VideoFormat"=99, "AudioFormat"=99"#)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            check.check().await.unwrap().status,
            HealthCheckStatus::Pass,
            "copy profiles retain supported transcode fallback settings"
        );
    }

    #[tokio::test]
    async fn overlays_require_the_pinned_runtimes_compatible_schema() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("default.json");
        for version in ["0.1.0", "0.1.1", "0.01.01"] {
            let value = serde_json::json!({
                "version": format!("https://ersatztv.org/channel/version/{version}")
            });
            std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
            check_overlay(&path).await.unwrap();
        }
        for value in [
            serde_json::json!({}),
            serde_json::json!({"version": null}),
            serde_json::json!({"version": 1}),
            serde_json::json!({"version": "https://ersatztv.org/channel/version/0.0.5"}),
            serde_json::json!({"version": "https://ersatztv.org/channel/version/0.1.2"}),
            serde_json::json!({"version": "https://ersatztv.org/channel/version/0.2.0"}),
            serde_json::json!({"version": "https://ersatztv.org/channel/version/1.1.0"}),
            serde_json::json!({"normalization": {"video": {"format": null}}}),
        ] {
            std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
            assert_eq!(check_overlay(&path).await, Err(OverlayCheckError::Schema));
        }
    }

    #[tokio::test]
    async fn active_overlays_are_checked_without_inspecting_other_channels() {
        let folder = tempfile::tempdir().unwrap();
        let channels = vec![(42, "12.3".to_owned())];
        overlays_available(&folder.path().join("missing"), &channels)
            .await
            .unwrap();
        std::fs::write(folder.path().join("42.json"), b"not JSON").unwrap();
        std::fs::write(folder.path().join("99.json"), b"not JSON").unwrap();
        std::fs::write(
            folder.path().join("default.json"),
            br#"{"version":"https://ersatztv.org/channel/version/0.1.0","normalization":{"video":{"width":640}}}"#,
        )
        .unwrap();
        std::fs::write(
            folder.path().join("12.3.json"),
            br#"{"version":"https://ersatztv.org/channel/version/0.1.0","normalization":{"video":{"width":1280}}}"#,
        )
        .unwrap();
        overlays_available(folder.path(), &channels).await.unwrap();
        for invalid in [b"not JSON".as_slice(), b"[]", b"null"] {
            std::fs::write(folder.path().join("12.3.json"), invalid).unwrap();
            assert!(overlays_available(folder.path(), &channels).await.is_err());
        }
        std::fs::remove_file(folder.path().join("12.3.json")).unwrap();
        std::fs::write(folder.path().join("default.json"), b"not JSON").unwrap();
        assert!(overlays_available(folder.path(), &channels).await.is_err());
    }

    #[tokio::test]
    async fn invalid_overlay_fails_health_without_exposing_contents_or_paths() {
        let (pool, _, _, _) = crate::next_engine::schedule::tests::fixture().await;
        sqlx::query(
            r#"UPDATE "Channel" SET "StreamingEngine"=1, "IsEnabled"=1;
            UPDATE "FFmpegProfile" SET "VideoFormat"=1, "AudioFormat"=1"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(
            folder.path().join("default.json"),
            b"synthetic-private-content",
        )
        .unwrap();
        let check = NextRuntimeHealthCheck {
            pool,
            resolve: || Ok(PathBuf::from("synthetic-engine")),
            overlays: folder.path().to_owned(),
        };
        let result = check.check().await.unwrap();
        assert_eq!(result.status, HealthCheckStatus::Fail);
        assert_eq!(
            result.brief_message,
            "Next configuration overlay is unavailable or invalid"
        );
        assert!(!result.message.contains("synthetic-private-content"));
        assert!(!result
            .message
            .contains(&folder.path().display().to_string()));
        std::fs::write(
            folder.path().join("default.json"),
            br#"{"version":"https://ersatztv.org/channel/version/0.1.0"}"#,
        )
        .unwrap();
        assert_eq!(check.check().await.unwrap().status, HealthCheckStatus::Pass);
        std::fs::File::create(folder.path().join("default.json"))
            .unwrap()
            .set_len(MAX_OVERLAY_BYTES + 1)
            .unwrap();
        let incomplete = check.check().await.unwrap();
        assert_eq!(incomplete.status, HealthCheckStatus::Warning);
        assert_eq!(
            incomplete.brief_message,
            "Next configuration overlay validation is incomplete"
        );
    }

    #[tokio::test]
    async fn overlay_inspection_is_bounded_and_rejects_non_files() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("default.json");
        std::fs::create_dir(&path).unwrap();
        assert!(check_overlay(&path).await.is_err());
        std::fs::remove_dir(&path).unwrap();
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(MAX_OVERLAY_BYTES + 1).unwrap();
        drop(file);
        assert_eq!(
            check_overlay(&path).await,
            Err(OverlayCheckError::Incomplete)
        );
        let channels = vec![(1, "../outside".to_owned())];
        std::fs::remove_file(&path).unwrap();
        assert!(overlays_available(folder.path(), &channels).await.is_err());
    }
}
