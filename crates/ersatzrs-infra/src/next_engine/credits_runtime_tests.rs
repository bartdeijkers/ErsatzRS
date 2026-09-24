//! Real Next proof for sought generated music-video credits and audio selection.

use super::*;
use ersatzrs_core::repositories::PlayoutRepository;

pub(super) async fn configure_credits(pool: &crate::db::DbPool, convert: bool) {
    for sql in [
        r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (31, 1, 0)"#,
        r#"INSERT INTO "Artist" ("Id") VALUES (31)"#,
        r#"INSERT INTO "ArtistMetadata" ("Id", "ArtistId", "Title", "MetadataKind", "DateAdded", "DateUpdated") VALUES (32, 31, 'Solo Artist', 0, '2026-09-24', '2026-09-24')"#,
        r#"INSERT INTO "MusicVideo" ("Id", "ArtistId") VALUES (10, 31)"#,
        r#"UPDATE "MediaVersion" SET "MovieId"=NULL,"MusicVideoId"=10,"Duration"='00:01:00' WHERE "Id"=100"#,
        r#"DELETE FROM "Movie" WHERE "Id"=10"#,
        r#"INSERT INTO "MusicVideoMetadata" ("Id", "MusicVideoId", "Title", "Album", "MetadataKind", "DateAdded", "DateUpdated") VALUES (33, 10, 'Credit Song', 'Credit Album', 0, '2026-09-24', '2026-09-24')"#,
        r#"UPDATE "Channel" SET "MusicVideoCreditsMode"=1,"MusicVideoCreditsTemplate"='_ArtistTitleAlbum_CenterTop',"SubtitleMode"=2 WHERE "Id"=1"#,
    ] {
        sqlx::query(sql)
            .execute(pool)
            .await
            .unwrap_or_else(|error| panic!("credits fixture SQL failed: {sql}: {error}"));
    }
    sqlx::query(r#"UPDATE "Channel" SET "NextEngineTextSubtitleMode"=? WHERE "Id"=1"#)
        .bind(i32::from(convert))
        .execute(pool)
        .await
        .unwrap();
    let mut item = selection::tests::item();
    item.id = 1;
    item.playout_id = 1;
    item.in_point = chrono::Duration::seconds(5);
    item.out_point = chrono::Duration::seconds(55);
    item.finish = item.start + chrono::Duration::seconds(50);
    crate::repositories::make_repositories(pool.clone())
        .playout
        .upsert_item(&item)
        .await
        .unwrap();
}

pub(super) async fn top_white_pixels(ffmpeg: &Path, segment: &Path) -> usize {
    let mut command = Command::new(ffmpeg);
    command
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(segment)
        .args([
            "-ss",
            "0.5",
            "-map",
            "0:v:0",
            "-frames:v",
            "1",
            "-vf",
            "crop=320:80:0:0",
            "-pix_fmt",
            "rgb24",
            "-f",
            "rawvideo",
            "pipe:1",
        ]);
    let pixels = tool(command).await;
    assert_eq!(pixels.len(), 320 * 80 * 3);
    pixels
        .chunks_exact(3)
        .filter(|pixel| pixel.iter().all(|channel| *channel > 180))
        .count()
}

async fn assert_dutch_audio(ffmpeg: &Path, segment: &Path) {
    let mut command = Command::new(ffmpeg);
    command
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(segment)
        .args([
            "-map", "0:a:0", "-t", "1", "-ac", "1", "-ar", "8000", "-f", "s16le", "pipe:1",
        ]);
    let pcm = tool(command).await;
    let samples: Vec<_> = pcm
        .chunks_exact(2)
        .map(|bytes| i16::from_le_bytes([bytes[0], bytes[1]]))
        .collect();
    assert!(samples.len() >= 7000);
    let crossings = samples
        .windows(2)
        .filter(|pair| pair[0] <= 0 && pair[1] > 0)
        .count();
    let hz = f64::from(u32::try_from(crossings).unwrap()) * 8000.0
        / f64::from(u32::try_from(samples.len()).unwrap());
    assert!((820.0..940.0).contains(&hz), "Dutch audio changed: {hz}");
}

#[tokio::test]
#[ignore = "requires official Next and FFmpeg; uses disposable synthetic media"]
#[allow(clippy::too_many_lines)] // One paired Burn/Convert lifecycle fixture.
async fn real_next_music_video_credits_burn_convert_seek_and_audio() {
    let binary = runtime_path("ERSATZRS_NEXT_TEST_BIN");
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    let root = tempfile::tempdir().unwrap();
    let clip = root.path().join("music-video.mkv");
    generate_clip(&ffmpeg, &clip).await;
    for convert in [false, true] {
        let (pool, locker) = prepare_schedule(&clip, false, &text_profile()).await;
        configure_credits(&pool, convert).await;
        let host = std::sync::Arc::new(host::NextHost::new(
            pool,
            locker,
            root.path().join(if convert {
                "convert-playout"
            } else {
                "burn-playout"
            }),
        ));
        let mut callback = callback_listener(host.clone()).await;
        let output = root.path().join(if convert {
            "convert-output"
        } else {
            "burn-output"
        });
        let registry = std::sync::Arc::new(crate::hls::InMemoryHlsSessionRegistry::new());
        let task = host
            .start(
                registry.clone(),
                host::NextStart {
                    channel_id: 1,
                    preview: false,
                    number: "1".into(),
                    executable: binary.clone(),
                    output: output.clone(),
                    ffmpeg: ffmpeg.clone(),
                    ffprobe: ffprobe.clone(),
                    replacements: Vec::new(),
                    idle_timeout: None,
                    deadline: Instant::now() + Duration::from_secs(45),
                },
            )
            .await
            .unwrap();
        let handle = registry.try_get("1").await.unwrap();
        timeout(Duration::from_secs(45), async {
            loop {
                match handle.startup_readiness() {
                    HlsStartupReadiness::Ready => break,
                    HlsStartupReadiness::Failed => {
                        panic!("credits session failed: convert={convert}")
                    }
                    _ => tokio::time::sleep(Duration::from_millis(10)).await,
                }
            }
        })
        .await
        .unwrap();
        let mut checked = std::collections::HashSet::new();
        let proof = timeout(Duration::from_secs(45), async {
            loop {
                if let Some(playlist) = handle.trim_playlist().await {
                    for name in playlist.lines().filter(|line| {
                        !line.starts_with('#')
                            && Path::new(line)
                                .extension()
                                .is_some_and(|extension| extension.eq_ignore_ascii_case("ts"))
                    }) {
                        let segment = output.join(name);
                        if checked.contains(name) || !segment.exists() {
                            continue;
                        }
                        let positive = if convert {
                            let vtt = segment.with_extension("vtt");
                            let Ok(body) = tokio::fs::read_to_string(vtt).await else {
                                continue;
                            };
                            body.contains("Credit Song")
                        } else {
                            top_white_pixels(&ffmpeg, &segment).await > 20
                        };
                        checked.insert(name.to_owned());
                        if positive {
                            return segment;
                        }
                    }
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .expect("credits must appear after the five-second seek");
        let white = top_white_pixels(&ffmpeg, &proof).await;
        if convert {
            assert_eq!(white, 0, "Convert must keep credits out of video pixels");
        } else {
            assert!(white > 20, "Burn must draw the generated credits");
        }
        assert_dutch_audio(&ffmpeg, &proof).await;
        assert!(registry.stop_channel("1").await);
        task.await.unwrap();
        callback.stop().await;
        assert!(!output.exists());
    }
}
