//! Real finite song producers and exact callback-context lifecycle.

use super::*;
use chrono::{TimeDelta, Utc};
use ersatzrs_core::repositories::{ChannelRepository as _, FfmpegProfileRepository as _};
use ersatzrs_domain::{Channel, ChannelPlayoutSource, FFmpegProfile, StreamingEngine};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::io::AsyncReadExt as _;

struct Fixture {
    root: tempfile::TempDir,
    host: Arc<NextHost>,
    instance: Uuid,
    binding: SongBinding,
    ffmpeg: PathBuf,
}

#[allow(clippy::disallowed_methods)] // Opt-in pinned package gate only.
fn ffmpeg() -> PathBuf {
    std::env::var_os("ERSATZRS_NEXT_TEST_FFMPEG")
        .map(PathBuf::from)
        .expect("set ERSATZRS_NEXT_TEST_FFMPEG to the verified runtime")
}

impl Fixture {
    async fn new() -> Self {
        let (pool, locker, mut channel, start) = super::super::schedule::tests::fixture().await;
        for sql in [
            r#"INSERT INTO "Song" ("Id") VALUES (10)"#,
            r#"UPDATE "MediaVersion" SET "MovieId"=NULL,"SongId"=10 WHERE "Id"=100"#,
            r#"DELETE FROM "Movie" WHERE "Id"=10"#,
            r#"INSERT INTO "SongMetadata" ("Id","SongId","Title","Artists","MetadataKind","DateAdded","DateUpdated") VALUES (20,10,'Synthetic Song','["Synthetic Artist"]',0,'2026-09-22','2026-09-22')"#,
        ] {
            sqlx::query(sql).execute(&pool).await.unwrap();
        }
        channel.streaming_engine = StreamingEngine::Next;
        crate::repositories::make_repositories(pool.clone())
            .channel
            .upsert(&channel)
            .await
            .unwrap();
        let snapshot = super::super::schedule::read_locked(
            &pool,
            &locker,
            &channel,
            start,
            start + TimeDelta::minutes(1),
        )
        .await
        .unwrap()
        .unwrap();
        let media = snapshot.items[0].clone();
        drop(snapshot);
        let binding = SongBinding {
            occurrence: super::super::canvas::CanvasBinding::new(
                channel.id,
                &media,
                Utc::now() + TimeDelta::hours(1),
            ),
            width: 320,
            height: 180,
            mode: channel.song_video_mode,
        };
        let root = tempfile::tempdir().unwrap();
        let ffmpeg = ffmpeg();
        let host = Arc::new(NextHost::new(pool, locker, root.path().join("next")));
        let instance = Uuid::new_v4();
        host.dynamic_contexts
            .lock()
            .await
            .insert(instance, context(&channel, &ffmpeg, root.path()));
        Self {
            root,
            host,
            instance,
            binding,
            ffmpeg,
        }
    }

    async fn png(&self, instance: Uuid, binding: &SongBinding) -> Vec<u8> {
        self.png_output(instance, binding).await.0
    }

    async fn png_output(&self, instance: Uuid, binding: &SongBinding) -> (Vec<u8>, Vec<u8>) {
        let mut reader = self
            .host
            .render_song_background(instance, binding)
            .await
            .unwrap();
        let mut bytes = Vec::new();
        tokio::time::timeout(Duration::from_secs(20), reader.read_to_end(&mut bytes))
            .await
            .unwrap()
            .unwrap();
        assert!(bytes.len() < 4 * 1024 * 1024);
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(
            u32::from_be_bytes(bytes[16..20].try_into().unwrap()),
            u32::try_from(binding.width).unwrap()
        );
        assert_eq!(
            u32::from_be_bytes(bytes[20..24].try_into().unwrap()),
            u32::try_from(binding.height).unwrap()
        );
        let path = self.root.path().join("decoded-source.png");
        tokio::fs::write(&path, &bytes).await.unwrap();
        let decoded = tokio::time::timeout(
            Duration::from_secs(20),
            tokio::process::Command::new(&self.ffmpeg)
                .args(["-nostdin", "-v", "error", "-i"])
                .arg(path)
                .args([
                    "-frames:v",
                    "1",
                    "-pix_fmt",
                    "rgb24",
                    "-f",
                    "rawvideo",
                    "pipe:1",
                ])
                .kill_on_drop(true)
                .output(),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(decoded.status.success());
        assert_eq!(
            decoded.stdout.len(),
            usize::try_from(binding.width * binding.height * 3).unwrap()
        );
        assert!(decoded
            .stdout
            .chunks_exact(3)
            .any(|pixel| pixel != &decoded.stdout[..3]));
        (bytes, decoded.stdout)
    }
}

fn context(channel: &Channel, ffmpeg: &Path, root: &Path) -> super::super::host::DynamicContext {
    super::super::host::DynamicContext {
        started_at: Utc::now(),
        request: super::super::host::NextStart {
            channel_id: channel.id,
            number: channel.number.clone(),
            preview: false,
            executable: "unused-next".into(),
            output: root.join("output"),
            ffmpeg: ffmpeg.to_path_buf(),
            ffprobe: "unused-ffprobe".into(),
            replacements: Vec::new(),
            idle_timeout: None,
            deadline: Instant::now() + Duration::from_mins(1),
        },
        troubleshooting: None,
        transfers: Arc::default(),
        fallback: Arc::default(),
        gaps: Arc::default(),
    }
}

mod literal_oracle {
    use super::*;

    const WIDTH: usize = 640;
    const HEIGHT: usize = 360;

    struct Ink {
        pixels: Vec<bool>,
        left: usize,
        top: usize,
        width: usize,
        height: usize,
    }

    impl Ink {
        fn new(frame: &[u8], background: &[u8]) -> Self {
            assert_eq!(frame.len(), WIDTH * HEIGHT * 3);
            assert_eq!(frame.len(), background.len());
            let pixels: Vec<bool> = frame
                .chunks_exact(3)
                .zip(background.chunks_exact(3))
                .map(|(pixel, baseline)| {
                    pixel.iter().all(|value| *value > 220)
                        && pixel.iter().zip(baseline).any(|(a, b)| a.abs_diff(*b) > 20)
                })
                .collect();
            let points: Vec<(usize, usize)> = pixels
                .iter()
                .enumerate()
                .filter(|(_, visible)| **visible)
                .map(|(index, _)| (index % WIDTH, index / WIDTH))
                .collect();
            assert!(
                !points.is_empty(),
                "decoded metadata contains no visible glyphs"
            );
            let left = points.iter().map(|(x, _)| *x).min().unwrap();
            let right = points.iter().map(|(x, _)| *x).max().unwrap();
            let top = points.iter().map(|(_, y)| *y).min().unwrap();
            let bottom = points.iter().map(|(_, y)| *y).max().unwrap();
            Self {
                pixels,
                left,
                top,
                width: right - left + 1,
                height: bottom - top + 1,
            }
        }

        // Compare glyph shapes independently of the two renderers' position and
        // font metrics. A single slash and a doubled slash remain distinct.
        fn normalized(&self) -> [bool; 1024] {
            std::array::from_fn(|index| {
                let x = self.left + ((2 * (index % 32) + 1) * self.width / 64);
                let y = self.top + ((2 * (index / 32) + 1) * self.height / 64);
                self.pixels[y * WIDTH + x]
            })
        }
    }

    fn distance(first: &Ink, second: &Ink) -> usize {
        first
            .normalized()
            .iter()
            .zip(second.normalized())
            .filter(|(a, b)| **a != *b)
            .count()
    }

    async fn title(fixture: &Fixture, title: &str) -> Vec<u8> {
        sqlx::query(r#"UPDATE "SongMetadata" SET "Title"=?,"Album"='',"Artists"='[]',"AlbumArtists"='[]' WHERE "Id"=20"#)
            .bind(title)
            .execute(&fixture.host.pool)
            .await
            .unwrap();
        fixture
            .png_output(fixture.instance, &fixture.binding)
            .await
            .1
    }

    async fn reference(fixture: &Fixture, text: &str) -> Ink {
        tokio::fs::write(fixture.root.path().join("reference.txt"), text)
            .await
            .unwrap();
        tokio::fs::write(
            fixture.root.path().join("reference.otf"),
            include_bytes!("../../../../assets/fonts/OPTIKabel-Heavy.otf"),
        )
        .await
        .unwrap();
        // drawtext's expansion=none reads literal plaintext rather than ASS.
        // This oracle neither calls escaped_text nor reproduces its encoding.
        let output = tokio::time::timeout(
            Duration::from_secs(20),
            tokio::process::Command::new(&fixture.ffmpeg)
                .current_dir(fixture.root.path())
                .args([
                    "-nostdin", "-v", "error", "-f", "lavfi", "-i", "color=c=black:s=640x360",
                    "-vf", "drawtext=fontfile=reference.otf:textfile=reference.txt:expansion=none:fontsize=36:fontcolor=white:x=10:y=10",
                    "-frames:v", "1", "-pix_fmt", "rgb24", "-f", "rawvideo", "pipe:1",
                ])
                .kill_on_drop(true)
                .output(),
        ).await.unwrap().unwrap();
        assert!(output.status.success(), "literal drawtext reference failed");
        Ink::new(&output.stdout, &vec![0; WIDTH * HEIGHT * 3])
    }

    #[tokio::test]
    #[ignore = "requires pinned FFmpeg; decoded literal metadata and independent glyph oracle"]
    async fn song_png_preserves_literal_glyphs_and_unicode_line_geometry() {
        let mut fixture = Fixture::new().await;
        // Seed 6 chooses the title/detail layout, with an empty second line.
        sqlx::query(r#"UPDATE "PlayoutItem" SET "Id"=6 WHERE "Id"=1"#)
            .execute(&fixture.host.pool)
            .await
            .unwrap();
        fixture.binding.occurrence.item = 6;
        sqlx::query(r#"UPDATE "Resolution" SET "Width"=640,"Height"=360 WHERE "Id"=1"#)
            .execute(&fixture.host.pool)
            .await
            .unwrap();
        fixture.binding.width = 640;
        fixture.binding.height = 360;
        let background = title(&fixture, "").await;
        let literal = Ink::new(
            &title(&fixture, r"{Live} literal\N 日本語").await,
            &background,
        );
        let newline = Ink::new(
            &title(&fixture, "{Live} literal\n日本語").await,
            &background,
        );
        assert!(
            newline.height > literal.height + 12,
            "literal \\N became a visible line break"
        );
        assert!(
            literal.width > newline.width,
            "literal title lost its second segment"
        );
        let brace = Ink::new(&title(&fixture, "{").await, &background);
        let literal_brace = reference(&fixture, "{").await;
        let replaced_brace = reference(&fixture, "(").await;
        assert!(
            distance(&brace, &literal_brace) < distance(&brace, &replaced_brace),
            "literal brace was replaced"
        );
        let slash = Ink::new(&title(&fixture, "\\").await, &background);
        let literal_slash = reference(&fixture, "\\").await;
        let doubled_slash = reference(&fixture, "\\\\").await;
        assert!(
            distance(&slash, &literal_slash) < distance(&slash, &doubled_slash),
            "literal slash was duplicated"
        );
        let context = fixture
            .host
            .dynamic_contexts
            .lock()
            .await
            .remove(&fixture.instance)
            .unwrap();
        context.transfers.shutdown().await.unwrap();
    }
}

#[tokio::test]
#[ignore = "requires pinned FFmpeg; real ordinary and mirror song PNG"]
async fn song_png_preserves_target_mirror_dimensions_and_reconnect_choice() {
    let fixture = Fixture::new().await;
    let first = fixture.png(fixture.instance, &fixture.binding).await;
    assert_eq!(first, fixture.png(fixture.instance, &fixture.binding).await);
    let repos = crate::repositories::make_repositories(fixture.host.pool.clone());
    let source = repos.channel.get_with_profile(1).await.unwrap();
    sqlx::query(
        r#"INSERT INTO "Resolution" ("Id","Name","Width","Height") VALUES (2,'Mirror',160,96)"#,
    )
    .execute(&fixture.host.pool)
    .await
    .unwrap();
    repos
        .ffmpeg_profile
        .upsert(&FFmpegProfile {
            id: 2,
            resolution_id: 2,
            ..Default::default()
        })
        .await
        .unwrap();
    let mirror = Channel {
        id: 2,
        number: "2".into(),
        ffmpeg_profile_id: 2,
        playout_source: ChannelPlayoutSource::Mirror,
        mirror_source_channel_id: Some(1),
        playout_offset: Some(TimeDelta::hours(1)),
        ..source
    };
    repos.channel.upsert(&mirror).await.unwrap();
    let start = fixture.binding.occurrence.start + TimeDelta::hours(1);
    let snapshot = super::super::schedule::read_locked(
        &fixture.host.pool,
        &fixture.host.locker,
        &mirror,
        start,
        start + TimeDelta::minutes(1),
    )
    .await
    .unwrap()
    .unwrap();
    let media = snapshot.items[0].clone();
    drop(snapshot);
    let binding = SongBinding {
        occurrence: super::super::canvas::CanvasBinding::new(
            2,
            &media,
            Utc::now() + TimeDelta::hours(1),
        ),
        width: 160,
        height: 96,
        mode: mirror.song_video_mode,
    };
    let instance = Uuid::new_v4();
    fixture.host.dynamic_contexts.lock().await.insert(
        instance,
        context(&mirror, &fixture.ffmpeg, fixture.root.path()),
    );
    let png = fixture.png(instance, &binding).await;
    assert_eq!(png, fixture.png(instance, &binding).await);
    assert!(fixture
        .host
        .validate_song_session(fixture.instance, &binding, None)
        .await
        .is_err());
    for instance in [fixture.instance, instance] {
        let context = fixture
            .host
            .dynamic_contexts
            .lock()
            .await
            .remove(&instance)
            .unwrap();
        context.transfers.shutdown().await.unwrap();
    }
}

#[tokio::test]
#[ignore = "requires pinned FFmpeg; confirmed malformed-cover retry produces default PNG"]
async fn song_png_retries_malformed_optional_cover_with_the_same_default_layout() {
    let fixture = Fixture::new().await;
    let expected = fixture.png(fixture.instance, &fixture.binding).await;
    let corrupt = fixture.root.path().join("malformed-cover.png");
    tokio::fs::write(&corrupt, b"\x89PNG\r\n\x1a\ntruncated")
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT INTO "Artwork" ("Id","SongMetadataId","ArtworkKind","Path","DateAdded","DateUpdated") VALUES (200,20,1,?,'2026-09-22','2026-09-22')"#,
    )
    .bind(corrupt.to_string_lossy().as_ref())
    .execute(&fixture.host.pool)
    .await
    .unwrap();
    assert_eq!(
        expected,
        fixture.png(fixture.instance, &fixture.binding).await
    );
    let context = fixture
        .host
        .dynamic_contexts
        .lock()
        .await
        .remove(&fixture.instance)
        .unwrap();
    context.transfers.shutdown().await.unwrap();
}

#[tokio::test]
#[ignore = "requires pinned FFmpeg; saved troubleshooting song context"]
async fn song_png_keeps_saved_preview_geometry_and_isolates_and_revokes_sessions() {
    let fixture = Fixture::new().await;
    let mut snapshot = fixture
        .host
        .occurrence_session(fixture.instance, &fixture.binding.occurrence)
        .await
        .unwrap();
    snapshot.media.item.id = 0;
    snapshot.profile.resolution.width = 160;
    snapshot.profile.resolution.height = 96;
    let preview = Arc::new(super::super::troubleshooting::Context::new(
        &super::super::troubleshooting::TroubleshootingPreview {
            media: &snapshot.media,
            channel: &snapshot.channel,
            profile: &snapshot.profile,
            subtitle_id: Some(0),
            selection_time: None,
            watermark_ids: &[],
            graphics_ids: &[],
            replacements: &[],
            ffmpeg: &fixture.ffmpeg,
            deadline: Instant::now() + Duration::from_secs(30),
            diagnostics: None,
        },
    ));
    let binding = SongBinding {
        occurrence: super::super::canvas::CanvasBinding::new(
            1,
            &snapshot.media,
            Utc::now() + TimeDelta::hours(1),
        )
        .for_session(fixture.instance),
        width: 160,
        height: 96,
        mode: snapshot.channel.song_video_mode,
    };
    fixture
        .host
        .dynamic_contexts
        .lock()
        .await
        .get_mut(&fixture.instance)
        .unwrap()
        .troubleshooting = Some(Arc::new(super::super::troubleshooting::Session::single(
        preview.clone(),
    )));
    let other = Uuid::new_v4();
    let mut other_context = context(&snapshot.channel, &fixture.ffmpeg, fixture.root.path());
    other_context.troubleshooting = Some(Arc::new(super::super::troubleshooting::Session::single(
        preview,
    )));
    fixture
        .host
        .dynamic_contexts
        .lock()
        .await
        .insert(other, other_context);
    let before: Vec<(i64, String, String)> =
        sqlx::query_as(r#"SELECT "Id","Start","Finish" FROM "PlayoutItem" ORDER BY "Id""#)
            .fetch_all(&fixture.host.pool)
            .await
            .unwrap();
    sqlx::query(r#"UPDATE "Resolution" SET "Width"=640,"Height"=360 WHERE "Id"=1"#)
        .execute(&fixture.host.pool)
        .await
        .unwrap();
    let first = fixture.png(fixture.instance, &binding).await;
    assert_eq!(first, fixture.png(fixture.instance, &binding).await);
    assert!(fixture
        .host
        .validate_song_session(other, &binding, None)
        .await
        .is_err());
    fixture
        .host
        .finish_troubleshooting_session(fixture.instance)
        .await
        .unwrap();
    assert!(fixture
        .host
        .validate_song_session(fixture.instance, &binding, None)
        .await
        .is_err());
    fixture
        .host
        .finish_troubleshooting_session(other)
        .await
        .unwrap();
    let after: Vec<(i64, String, String)> =
        sqlx::query_as(r#"SELECT "Id","Start","Finish" FROM "PlayoutItem" ORDER BY "Id""#)
            .fetch_all(&fixture.host.pool)
            .await
            .unwrap();
    assert_eq!(before, after);
}

// A disposable POSIX wrapper delays only this fixture's pinned PNG producer.
// Managed process-group cleanup must also reap its sleep descendants.
#[cfg(unix)]
mod cancellation {
    use super::*;
    use std::{os::unix::fs::PermissionsExt as _, process::Stdio};

    async fn controlled(fixture: &Fixture) -> PathBuf {
        let quoted = |path: &Path| format!("'{}'", path.to_str().unwrap().replace('\'', "'\\''"));
        let path = fixture.root.path().join("controlled-ffmpeg.sh");
        tokio::fs::write(&path, format!(
            "#!/bin/sh\nset -eu\nprintf '%s\\n%s\\n' \"$$\" \"$PWD\" > {}\nwhile [ ! -e {} ]; do sleep 0.02; done\nexec {} \"$@\"\n",
            quoted(&fixture.root.path().join("producer")), quoted(&fixture.root.path().join("release")), quoted(&fixture.ffmpeg)
        )).await.unwrap();
        tokio::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .await
            .unwrap();
        fixture
            .host
            .dynamic_contexts
            .lock()
            .await
            .get_mut(&fixture.instance)
            .unwrap()
            .request
            .ffmpeg = path.clone();
        path
    }

    async fn producer(fixture: &Fixture) -> (String, PathBuf) {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if let Ok(log) =
                    tokio::fs::read_to_string(fixture.root.path().join("producer")).await
                {
                    let lines: Vec<_> = log.lines().collect();
                    if lines.len() == 2 {
                        assert!(lines[0].parse::<u32>().unwrap() > 0);
                        let directory = PathBuf::from(lines[1]);
                        assert!(directory
                            .file_name()
                            .unwrap()
                            .to_str()
                            .unwrap()
                            .starts_with("ersatzrs-next-song-"));
                        return (lines[0].to_owned(), directory);
                    }
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap()
    }

    async fn reaped(producer: &(String, PathBuf)) {
        tokio::time::timeout(Duration::from_secs(8), async {
            loop {
                let alive = tokio::process::Command::new("kill")
                    .args(["-0", &producer.0])
                    .stderr(Stdio::null())
                    .status()
                    .await
                    .unwrap()
                    .success();
                if !alive && !producer.1.exists() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("owned song process and assets must be removed");
    }

    fn request(fixture: &Fixture) -> tokio::task::JoinHandle<Result<TransferReader>> {
        let host = fixture.host.clone();
        let instance = fixture.instance;
        let binding = fixture.binding.clone();
        tokio::spawn(async move { host.render_song_background(instance, &binding).await })
    }

    #[tokio::test]
    #[ignore = "requires pinned FFmpeg; song request and session cancellation cleanup"]
    async fn song_preparation_reaps_owned_process_and_assets_on_request_drop_and_session_stop() {
        let fixture = Fixture::new().await;
        controlled(&fixture).await;
        let task = request(&fixture);
        let first = producer(&fixture).await;
        assert!(first.1.exists());
        task.abort();
        let Err(cancelled) = task.await else {
            panic!("request task must be cancelled")
        };
        assert!(cancelled.is_cancelled());
        reaped(&first).await;
        fixture
            .host
            .validate_song_session(fixture.instance, &fixture.binding, None)
            .await
            .unwrap();
        tokio::fs::remove_file(fixture.root.path().join("producer"))
            .await
            .unwrap();
        let task = request(&fixture);
        let stopped = producer(&fixture).await;
        let context = fixture
            .host
            .dynamic_contexts
            .lock()
            .await
            .remove(&fixture.instance)
            .unwrap();
        context.transfers.shutdown().await.unwrap();
        assert!(task.await.unwrap().is_err());
        reaped(&stopped).await;
        assert!(fixture
            .host
            .validate_song_session(fixture.instance, &fixture.binding, None)
            .await
            .is_err());
    }

    #[tokio::test]
    #[ignore = "requires pinned FFmpeg; callback drift while PNG preparation is pending"]
    async fn song_preparation_refuses_schedule_and_profile_drift_and_reaps_generated_assets() {
        for profile in [false, true] {
            let fixture = Fixture::new().await;
            controlled(&fixture).await;
            let task = request(&fixture);
            let owned = producer(&fixture).await;
            if profile {
                sqlx::query(r#"UPDATE "Resolution" SET "Width"=640 WHERE "Id"=1"#)
                    .execute(&fixture.host.pool)
                    .await
                    .unwrap();
            } else {
                sqlx::query(r#"UPDATE "PlayoutItem" SET "Finish"=? WHERE "Id"=?"#)
                    .bind((fixture.binding.occurrence.finish + TimeDelta::seconds(1)).to_rfc3339())
                    .bind(fixture.binding.occurrence.item)
                    .execute(&fixture.host.pool)
                    .await
                    .unwrap();
            }
            tokio::fs::write(fixture.root.path().join("release"), b"release")
                .await
                .unwrap();
            assert!(tokio::time::timeout(Duration::from_secs(20), task)
                .await
                .unwrap()
                .unwrap()
                .is_err());
            reaped(&owned).await;
            let context = fixture
                .host
                .dynamic_contexts
                .lock()
                .await
                .remove(&fixture.instance)
                .unwrap();
            context.transfers.shutdown().await.unwrap();
        }
    }
}
