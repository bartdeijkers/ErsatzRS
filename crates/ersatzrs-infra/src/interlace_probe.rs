//! Bounded, selected-stream interlace sampling for native playback.
use std::{path::Path, process::Stdio, time::Duration};

use ersatzrs_domain::VideoScanKind;
use ersatzrs_ffmpeg::SourceVideoStream;
use sqlx::Row;
use tokio::{io::AsyncReadExt, process::Command, sync::Mutex, time::Instant};

use crate::{addons::spawn_managed_addon_process, db::DbPool};

// Serialize optional probes and re-read cached state after admission. The same
// deadline bounds admission, database reads, sampling and publication.
static PROBE_GATE: Mutex<()> = Mutex::const_new(());
const OUTPUT_LIMIT: u64 = 128 * 1024;

pub(crate) async fn enrich(
    pool: &DbPool,
    version_id: i64,
    path: &str,
    deinterlace: bool,
    stream: &mut SourceVideoStream,
) {
    if !deinterlace || stream.scan_kind == VideoScanKind::Interlaced || stream.is_still_image {
        return;
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    let result = tokio::time::timeout_at(
        deadline,
        resolve(
            pool,
            version_id,
            path,
            stream.index,
            deadline,
            &ersatzrs_ffmpeg::ffmpeg_path(),
        ),
    )
    .await;
    match result {
        Ok(Ok(Some(ratio))) => {
            stream.scan_kind = if ratio > 0.05 {
                VideoScanKind::Interlaced
            } else {
                VideoScanKind::Progressive
            };
        }
        Ok(Ok(None)) => {}
        _ => tracing::debug!("interlace sampling unavailable; preserving container scan kind"),
    }
}

async fn resolve(
    pool: &DbPool,
    version_id: i64,
    path: &str,
    stream_index: usize,
    deadline: Instant,
    executable: &str,
) -> Result<Option<f64>, ProbeError> {
    let enabled: Option<String> =
        sqlx::query_scalar(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#)
            .bind(ersatzrs_domain::CONFIG_KEY_FFMPEG_PROBE_FOR_INTERLACED_FRAMES)
            .fetch_optional(pool)
            .await
            .map_err(|_| ProbeError)?;
    if !enabled.is_some_and(|value| value.eq_ignore_ascii_case("true") || value == "1") {
        return Ok(None);
    }
    // Protocol inputs are excluded. Unsupported local formats (including an
    // unavailable AviSynth runtime) fail the bounded FFmpeg probe without
    // publishing a ratio.
    let local = Path::new(path);
    if !local.is_absolute() {
        return Ok(None);
    }
    let Ok(before) = tokio::fs::metadata(local).await else {
        return Ok(None);
    };
    if !before.is_file() {
        return Ok(None);
    }
    let _guard = PROBE_GATE.lock().await;
    let Some(identity) = Identity::load(pool, version_id).await? else {
        return Ok(None);
    };
    let duration = crate::repositories::common::try_decode_duration_text(&identity.duration)
        .map_err(|_| ProbeError)?
        .to_std()
        .map_err(|_| ProbeError)?;
    if duration < Duration::from_secs(3) {
        return Ok(None);
    }
    let cacheable = identity.file_count == 1
        && identity.path.as_deref() == Some(path)
        && usize::try_from(identity.first_stream).ok() == Some(stream_index);
    if cacheable {
        if let Some(ratio) = identity
            .ratio
            .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
        {
            return Ok(Some(ratio));
        }
    }
    let ratio = probe(executable, path, stream_index, duration / 3, deadline).await?;
    let after = tokio::fs::metadata(local).await.map_err(|_| ProbeError)?;
    if before.len() != after.len() || before.modified().ok() != after.modified().ok() {
        return Ok(None);
    }
    let Some(current) = Identity::load(pool, version_id).await? else {
        return Ok(None);
    };
    if !identity.same_source(&current) {
        return Ok(None);
    }
    if cacheable && !identity.publish(pool, version_id, path, ratio).await? {
        return Ok(None);
    }
    Ok(Some(ratio))
}

#[derive(Debug)]
struct ProbeError;

struct Identity {
    updated: String,
    duration: String,
    ratio: Option<f64>,
    path: Option<String>,
    file_count: i64,
    first_stream: i64,
}

impl Identity {
    async fn load(pool: &DbPool, id: i64) -> Result<Option<Self>, ProbeError> {
        let row = sqlx::query(r#"SELECT "DateUpdated", "Duration", "InterlacedRatio",
            (SELECT MIN("Path") FROM "MediaFile" WHERE "MediaVersionId" = mv."Id") AS "Path",
            (SELECT COUNT(*) FROM "MediaFile" WHERE "MediaVersionId" = mv."Id") AS "FileCount",
            (SELECT MIN("Index") FROM "MediaStream" WHERE "MediaVersionId" = mv."Id" AND "MediaStreamKind" = 1) AS "FirstStream"
            FROM "MediaVersion" mv WHERE mv."Id" = ?"#)
            .bind(id).fetch_optional(pool).await.map_err(|_| ProbeError)?;
        row.map(|row| {
            Ok(Self {
                updated: row.try_get("DateUpdated").map_err(|_| ProbeError)?,
                duration: row.try_get("Duration").map_err(|_| ProbeError)?,
                ratio: row.try_get("InterlacedRatio").map_err(|_| ProbeError)?,
                path: row.try_get("Path").map_err(|_| ProbeError)?,
                file_count: row.try_get("FileCount").map_err(|_| ProbeError)?,
                first_stream: row.try_get("FirstStream").map_err(|_| ProbeError)?,
            })
        })
        .transpose()
    }

    // The final write must independently reject changes after the preceding
    // identity read, including a scanner refresh or another cache publisher.
    async fn publish(
        &self,
        pool: &DbPool,
        version_id: i64,
        path: &str,
        ratio: f64,
    ) -> Result<bool, ProbeError> {
        let updated = sqlx::query(r#"UPDATE "MediaVersion" SET "InterlacedRatio" = ?
            WHERE "Id" = ? AND "DateUpdated" = ? AND "Duration" = ?
            AND "InterlacedRatio" IS NULL
            AND (SELECT COUNT(*) FROM "MediaFile" WHERE "MediaVersionId" = ?) = 1
            AND EXISTS (SELECT 1 FROM "MediaFile" WHERE "MediaVersionId" = ? AND "Path" = ?)
            AND (SELECT MIN("Index") FROM "MediaStream" WHERE "MediaVersionId" = ? AND "MediaStreamKind" = 1) = ?"#)
            .bind(ratio).bind(version_id).bind(&self.updated).bind(&self.duration)
            .bind(version_id).bind(version_id).bind(path).bind(version_id).bind(self.first_stream)
            .execute(pool).await.map_err(|_| ProbeError)?;
        Ok(updated.rows_affected() == 1)
    }

    fn same_source(&self, other: &Self) -> bool {
        self.updated == other.updated
            && self.duration == other.duration
            && self.path == other.path
            && self.file_count == other.file_count
            && self.first_stream == other.first_stream
    }
}

async fn probe(
    executable: &str,
    path: &str,
    index: usize,
    seek: Duration,
    deadline: Instant,
) -> Result<f64, ProbeError> {
    let mut command = Command::new(executable);
    command
        .args(ersatzrs_ffmpeg::interlace_probe_arguments(
            path, index, seek,
        ))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let mut child = spawn_managed_addon_process(command).map_err(|_| ProbeError)?;
    let stderr = child.stderr().take().ok_or(ProbeError)?;
    let operation = async {
        let mut bytes = Vec::new();
        stderr
            .take(OUTPUT_LIMIT + 1)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| ProbeError)?;
        if bytes.len() as u64 > OUTPUT_LIMIT {
            return Err(ProbeError);
        }
        if !child.wait().await.map_err(|_| ProbeError)?.success() {
            return Err(ProbeError);
        }
        parse_ratio(std::str::from_utf8(&bytes).map_err(|_| ProbeError)?).ok_or(ProbeError)
    };
    if let Ok(Ok(ratio)) = tokio::time::timeout_at(deadline, operation).await {
        Ok(ratio)
    } else {
        let _ = tokio::time::timeout(Duration::from_secs(2), child.kill()).await;
        Err(ProbeError)
    }
}

fn parse_ratio(stderr: &str) -> Option<f64> {
    fn counts(line: &str) -> Option<(u32, u32)> {
        let tokens: Vec<_> = line.split_whitespace().collect();
        let value = |name| {
            tokens
                .windows(2)
                .find(|pair| pair[0] == name)?
                .get(1)?
                .parse::<u32>()
                .ok()
        };
        let interlaced = value("TFF:")?.checked_add(value("BFF:")?)?;
        Some((interlaced, interlaced.checked_add(value("Progressive:")?)?))
    }
    let last = |prefix| {
        stderr
            .lines()
            .rfind(|line| line.contains(prefix))
            .and_then(counts)
    };
    let (single_i, single_n) = last("Single frame detection:")?;
    let (multi_i, multi_n) = last("Multi frame detection:")?;
    let total = single_n.checked_add(multi_n)?;
    Some(if total == 0 {
        0.0
    } else {
        f64::from(single_i.checked_add(multi_i)?) / f64::from(total)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn fixture() -> (tempfile::TempDir, DbPool, String) {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("source.mkv");
        tokio::fs::write(&path, b"cached ratio fixture")
            .await
            .unwrap();
        let pool = crate::db::init("sqlite::memory:").await.unwrap();
        sqlx::query(r#"INSERT INTO "MediaVersion" ("Id", "DateAdded", "DateUpdated", "Duration", "Height", "Width", "VideoScanKind", "InterlacedRatio") VALUES (1, '2026-01-01', '2026-01-01', '150:00:00', 16, 16, 1, 0.06)"#)
            .execute(&pool).await.unwrap();
        sqlx::query(r#"INSERT INTO "MediaFile" ("MediaVersionId", "Path", "PathHash") VALUES (1, ?, 'fixture')"#)
            .bind(path.to_str().unwrap()).execute(&pool).await.unwrap();
        for index in [0, 3] {
            sqlx::query(r#"INSERT INTO "MediaStream" ("Index", "Codec", "MediaVersionId", "MediaStreamKind", "Channels", "Default", "Forced") VALUES (?, 'h264', 1, 1, 0, 0, 0)"#)
                .bind(index).execute(&pool).await.unwrap();
        }
        (root, pool, path.to_string_lossy().into_owned())
    }

    #[tokio::test]
    async fn setting_and_selected_source_gate_cached_ratios() {
        let (root, pool, path) = fixture().await;
        let deadline = Instant::now() + Duration::from_secs(10);
        let executable = root.path().join("absent-probe");
        let executable = executable.to_str().unwrap();
        assert_eq!(
            resolve(&pool, 1, &path, 0, deadline, executable)
                .await
                .unwrap(),
            None
        );
        sqlx::query(r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES ('ffmpeg.probe_for_interlaced_frames', 'True') ON CONFLICT ("Key") DO UPDATE SET "Value" = excluded."Value""#)
            .execute(&pool).await.unwrap();
        assert_eq!(
            resolve(&pool, 1, &path, 0, deadline, executable)
                .await
                .unwrap(),
            Some(0.06)
        );
        assert!(resolve(&pool, 1, &path, 3, deadline, executable)
            .await
            .is_err());
        let other = root.path().join("replacement.mkv");
        tokio::fs::write(&other, b"different source").await.unwrap();
        assert!(
            resolve(&pool, 1, other.to_str().unwrap(), 0, deadline, executable)
                .await
                .is_err()
        );
        assert_eq!(
            resolve(
                &pool,
                1,
                "https://example.invalid/media",
                0,
                deadline,
                executable
            )
            .await
            .unwrap(),
            None
        );
        let ratio: Option<f64> =
            sqlx::query_scalar(r#"SELECT "InterlacedRatio" FROM "MediaVersion" WHERE "Id" = 1"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(ratio, Some(0.06));
        sqlx::query(r#"UPDATE "MediaVersion" SET "Duration" = '00:00:02' WHERE "Id" = 1"#)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            resolve(&pool, 1, &path, 0, deadline, executable)
                .await
                .unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn publication_rejects_changes_after_identity_read() {
        let mutations = [
            r#"UPDATE "MediaVersion" SET "DateUpdated" = '2026-01-02' WHERE "Id" = 1"#,
            r#"UPDATE "MediaVersion" SET "Duration" = '151:00:00' WHERE "Id" = 1"#,
            r#"UPDATE "MediaVersion" SET "InterlacedRatio" = 0.9 WHERE "Id" = 1"#,
            r#"UPDATE "MediaFile" SET "Path" = 'replacement.mkv' WHERE "MediaVersionId" = 1"#,
            r#"INSERT INTO "MediaFile" ("MediaVersionId", "Path", "PathHash") VALUES (1, 'second.mkv', 'second')"#,
            r#"DELETE FROM "MediaStream" WHERE "MediaVersionId" = 1 AND "Index" = 0"#,
        ];
        for mutation in mutations {
            let (_root, pool, path) = fixture().await;
            sqlx::query(r#"UPDATE "MediaVersion" SET "InterlacedRatio" = NULL WHERE "Id" = 1"#)
                .execute(&pool)
                .await
                .unwrap();
            let identity = Identity::load(&pool, 1).await.unwrap().unwrap();
            sqlx::query(mutation).execute(&pool).await.unwrap();
            let expected: Option<f64> = sqlx::query_scalar(
                r#"SELECT "InterlacedRatio" FROM "MediaVersion" WHERE "Id" = 1"#,
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            assert!(
                !identity.publish(&pool, 1, &path, 0.7).await.unwrap(),
                "{mutation}"
            );
            let actual: Option<f64> = sqlx::query_scalar(
                r#"SELECT "InterlacedRatio" FROM "MediaVersion" WHERE "Id" = 1"#,
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(actual, expected, "{mutation}");
        }
        let (_root, pool, path) = fixture().await;
        sqlx::query(r#"UPDATE "MediaVersion" SET "InterlacedRatio" = NULL WHERE "Id" = 1"#)
            .execute(&pool)
            .await
            .unwrap();
        let identity = Identity::load(&pool, 1).await.unwrap().unwrap();
        assert!(identity.publish(&pool, 1, &path, 0.7).await.unwrap());
        assert!(!identity.publish(&pool, 1, &path, 0.2).await.unwrap());
        assert_eq!(
            Identity::load(&pool, 1).await.unwrap().unwrap().ratio,
            Some(0.7)
        );
    }

    // Unix shell fixtures exercise process I/O and cancellation. Production
    // process ownership uses the existing Unix group / Windows job abstraction.
    #[cfg(unix)]
    #[tokio::test]
    async fn failed_noisy_and_stalled_probes_are_bounded() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().unwrap();
        let executable = root.path().join("probe.sh");
        for script in [
            "#!/bin/sh\nexit 1\n",
            "#!/bin/sh\nwhile :; do printf 'unbounded probe diagnostics\\n' >&2; done\n",
            "#!/bin/sh\nexec sleep 60\n",
        ] {
            tokio::fs::write(&executable, script).await.unwrap();
            tokio::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700))
                .await
                .unwrap();
            let result = tokio::time::timeout(
                Duration::from_secs(4),
                probe(
                    executable.to_str().unwrap(),
                    "fixture.mkv",
                    0,
                    Duration::ZERO,
                    Instant::now() + Duration::from_millis(250),
                ),
            )
            .await
            .expect("probe and cleanup must be bounded");
            assert!(result.is_err());
        }
    }

    // Shell process-tree fixture is Unix-only; Windows ownership is exercised
    // by the managed process job tests in addon_process_job.
    #[cfg(unix)]
    #[tokio::test]
    async fn dropped_probe_future_terminates_descendants() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().unwrap();
        let executable = root.path().join("probe.sh");
        let ready = root.path().join("ready");
        let marker = root.path().join("orphan");
        // The probe input path is argv[6]. Use it as the fixture directory so
        // generated temporary paths never need interpolation into shell code.
        tokio::fs::write(&executable,
            "#!/bin/sh\n(sleep 1; printf orphan > \"$6/orphan\") &\nprintf ready > \"$6/ready\"\nwait\n",
        ).await.unwrap();
        tokio::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700))
            .await
            .unwrap();
        let path = root.path().to_string_lossy().into_owned();
        let invocation = tokio::spawn(async move {
            probe(
                executable.to_str().unwrap(),
                &path,
                0,
                Duration::ZERO,
                Instant::now() + Duration::from_secs(10),
            )
            .await
        });
        tokio::time::timeout(Duration::from_secs(2), async {
            while !ready.exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("probe process must start");
        invocation.abort();
        assert!(invocation.await.unwrap_err().is_cancelled());
        tokio::time::sleep(Duration::from_millis(1200)).await;
        assert!(!marker.exists(), "dropped probe left a running descendant");
    }

    // Unix shell fixture; the production worker uses the cross-platform managed owner.
    #[cfg(unix)]
    #[tokio::test]
    async fn stopping_hls_during_probe_cleans_worker_and_cache() {
        use ersatzrs_core::hls::HlsSessionRegistry;
        use std::{os::unix::fs::PermissionsExt, sync::Arc};

        let (root, pool, path) = fixture().await;
        seed_hls_probe_channel(&pool).await;
        let executable = root.path().join("probe.sh");
        let ready = root.path().join("source.mkv.ready");
        let orphan = root.path().join("source.mkv.orphan");
        tokio::fs::write(&executable,
            "#!/bin/sh\n(sleep 1; printf orphan > \"$6.orphan\") &\nprintf ready > \"$6.ready\"\nwait\n",
        ).await.unwrap();
        tokio::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700))
            .await
            .unwrap();
        let previous_ffmpeg = ersatzrs_ffmpeg::ffmpeg_path();
        let previous_ffprobe = ersatzrs_ffmpeg::ffprobe_path();
        ersatzrs_ffmpeg::set_resolved_ffmpeg_tools(executable.to_string_lossy(), &previous_ffprobe);
        let working_dir = root.path().join("hls");
        let worker = crate::hls_worker::HlsSessionWorker::new(
            "1".to_owned(),
            1,
            working_dir.clone(),
            ersatzrs_ffmpeg::OutputFormat::Hls,
            Vec::new(),
            Arc::new(tokio::sync::RwLock::new(
                ersatzrs_ffmpeg::FfmpegRuntimeReport::for_tests(),
            )),
        );
        let registry = Arc::new(crate::hls::InMemoryHlsSessionRegistry::new());
        assert!(registry.try_insert(worker.clone()).await);
        worker.start(pool.clone(), registry.clone(), None);
        tokio::time::timeout(Duration::from_secs(5), async {
            while !ready.exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("HLS worker must enter interlace sampling");
        assert!(registry.stop_channel("1").await);
        tokio::time::timeout(Duration::from_secs(2), async {
            while working_dir.exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("Stop must clean the worker without waiting for the probe budget");
        assert!(registry.try_get("1").await.is_none());
        assert_eq!(Identity::load(&pool, 1).await.unwrap().unwrap().ratio, None);
        tokio::time::sleep(Duration::from_millis(1200)).await;
        assert!(!orphan.exists(), "stopped HLS probe left a descendant");
        assert!(Path::new(&path).exists());
        ersatzrs_ffmpeg::set_resolved_ffmpeg_tools(previous_ffmpeg, previous_ffprobe);
    }

    #[cfg(unix)]
    async fn seed_hls_probe_channel(pool: &DbPool) {
        sqlx::raw_sql(r#"
            INSERT INTO "Resolution" ("Id","Name","Width","Height") VALUES (801,'fixture',320,180);
            INSERT INTO "FFmpegProfile" (
                "Id","Name","ThreadCount","TranscodeVideo","TranscodeAudio","ResolutionId",
                "ScalingBehavior","VideoFormat","AudioFormat","VideoBitrate","VideoBufferSize",
                "AudioBitrate","AudioBufferSize","NormalizeLoudnessMode","AudioChannels","AudioSampleRate",
                "HardwareAcceleration","QsvExtraHardwareFrames","VaapiDriver","TonemapAlgorithm",
                "AllowBFrames","BitDepth","NormalizeFramerate","NormalizeColors","DeinterlaceVideo","PadMode")
                VALUES (802,'fixture',0,1,1,801,0,1,1,4000,8000,192,384,0,2,48,0,64,0,0,0,0,1,1,1,0);
            INSERT INTO "MediaSource" ("Id") VALUES (1);
            INSERT INTO "LocalMediaSource" ("Id") VALUES (1);
            INSERT INTO "Library" ("Id","MediaKind","MediaSourceId") VALUES (1,1,1);
            INSERT INTO "LocalLibrary" ("Id") VALUES (1);
            INSERT INTO "LibraryPath" ("Id","LibraryId","Path") VALUES (1,1,'fixture');
            INSERT INTO "MediaItem" ("Id","LibraryPathId","State") VALUES (1,1,0);
            INSERT INTO "Movie" ("Id") VALUES (1);
            UPDATE "MediaVersion" SET "MovieId"=1,"InterlacedRatio"=NULL,"RFrameRate"='25/1' WHERE "Id"=1;
            INSERT INTO "Channel" ("Id","UniqueId","Number","SortNumber","Name","FFmpegProfileId","StreamingMode","StreamSelectorMode","PlayoutSource","PlayoutMode","TranscodeMode","IdleBehavior","MusicVideoCreditsMode","SongVideoMode","SubtitleMode")
                VALUES (1,'probe-channel','1',1,'Probe',802,5,0,0,0,0,0,0,0,0);
            INSERT INTO "Playout" ("Id","ChannelId","ScheduleKind","Seed") VALUES (1,1,0,1);
            INSERT INTO "ConfigElement" ("Key","Value") VALUES ('ffmpeg.probe_for_interlaced_frames','True');
        "#).execute(pool).await.unwrap();
        let now = chrono::Utc::now();
        sqlx::query(r#"INSERT INTO "PlayoutItem" ("DisableWatermarks","FillerKind","Finish","GuideGroup","InPoint","MediaItemId","OutPoint","PlayoutId","Start") VALUES (1,0,?,0,'00:00:00',1,'01:00:00',1,?)"#)
            .bind(crate::repositories::common::encode_datetime(now + chrono::Duration::hours(1)))
            .bind(crate::repositories::common::encode_datetime(now - chrono::Duration::seconds(1)))
            .execute(pool).await.unwrap();
    }

    #[tokio::test]
    #[ignore = "requires ERSATZRS_NEXT_TEST_FFMPEG pointing to verified FFmpeg 9"]
    async fn real_idet_selected_stream_preserves_primary_ratio() {
        let executable = crate::startup_config::environment_variable("ERSATZRS_NEXT_TEST_FFMPEG")
            .expect("set the verified FFmpeg 9 executable");
        let executable = executable.to_str().unwrap();
        let (_root, pool, path) = fixture().await;
        let mut command = Command::new(executable);
        command.args([
            "-hide_banner", "-loglevel", "error", "-nostdin", "-y",
            "-f", "lavfi", "-i", "testsrc=size=320x240:rate=25:duration=12",
            "-f", "lavfi", "-i", "testsrc2=size=320x240:rate=50:duration=12,tinterlace=mode=interleave_top,setfield=prog",
            "-map", "0:v", "-map", "1:v", "-c:v", "ffv1", &path,
        ]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        let mut child = spawn_managed_addon_process(command).unwrap();
        assert!(tokio::time::timeout(Duration::from_secs(20), child.wait())
            .await
            .unwrap()
            .unwrap()
            .success());
        sqlx::query(r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES ('ffmpeg.probe_for_interlaced_frames', 'True')"#).execute(&pool).await.unwrap();
        sqlx::query(r#"UPDATE "MediaVersion" SET "Duration" = '00:00:12', "InterlacedRatio" = NULL WHERE "Id" = 1"#).execute(&pool).await.unwrap();
        sqlx::query(r#"UPDATE "MediaStream" SET "Index" = 1 WHERE "Index" = 3"#)
            .execute(&pool)
            .await
            .unwrap();
        let primary = resolve(
            &pool,
            1,
            &path,
            0,
            Instant::now() + Duration::from_secs(10),
            executable,
        )
        .await
        .unwrap()
        .unwrap();
        assert!(primary <= 0.05, "progressive ratio: {primary}");
        let alternate = resolve(
            &pool,
            1,
            &path,
            1,
            Instant::now() + Duration::from_secs(10),
            executable,
        )
        .await
        .unwrap()
        .unwrap();
        assert!(alternate > 0.05, "interlaced ratio: {alternate}");
        let stored: Option<f64> =
            sqlx::query_scalar(r#"SELECT "InterlacedRatio" FROM "MediaVersion" WHERE "Id" = 1"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(stored, Some(primary));
        // Reuse persisted data even when no probe executable can be started.
        assert_eq!(
            resolve(
                &pool,
                1,
                &path,
                0,
                Instant::now() + Duration::from_secs(10),
                "absent-idet-fixture"
            )
            .await
            .unwrap(),
            stored
        );
    }

    #[test]
    fn latest_complete_idet_statistics_define_ratio() {
        let log = "Single frame detection: TFF: 200 BFF: 0 Progressive: 0\nSingle frame detection: TFF: 3 BFF: 2 Progressive: 95\nMulti frame detection: TFF: 4 BFF: 3 Progressive: 93";
        assert_eq!(parse_ratio(log), Some(0.06));
        assert_eq!(parse_ratio(""), None);
        assert_eq!(
            parse_ratio("Single frame detection: TFF: 1 BFF: 0 Progressive: 0"),
            None
        );
        assert_eq!(parse_ratio("Single frame detection: TFF: 0 BFF: 0 Progressive: 0\nMulti frame detection: TFF: 0 BFF: 0 Progressive: 0"), Some(0.0));
    }
}
