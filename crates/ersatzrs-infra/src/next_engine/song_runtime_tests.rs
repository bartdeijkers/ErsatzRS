//! Decoded song proof. Harness-only materialization replaces the signed PNG
//! callback source; song host tests cover its authenticated HTTP lifecycle.
use super::*;
use crate::next_engine::{media::MediaRelay, protocol as wire};
use ersatzrs_core::repositories::{ChannelRepository as _, PlayoutRepository as _};
use ersatzrs_domain::{ChannelSongVideoMode, ChannelSubtitleMode, PlayoutItem};

#[tokio::test]
#[ignore = "requires pinned published Next and FFmpeg 9; song seek/lyrics/handoff"]
async fn real_next_song_preserves_alternate_audio_midpoint_seek_lyrics_and_handoff() {
    run(false).await;
}
#[tokio::test]
#[ignore = "requires pinned published Next and FFmpeg 9; song silence/WithProgress"]
async fn real_next_song_preserves_published_silence_and_omits_next_progress_bar() {
    run(true).await;
}
fn resolution() -> Resolution {
    Resolution {
        width: 320,
        height: 180,
        ..Default::default()
    }
}
fn local(path: &Path) -> wire::Source {
    wire::Source::Local {
        path: path.to_str().unwrap().into(),
        probe_hint: None,
        in_point_ms: None,
        out_point_ms: None,
    }
}
async fn generate_song(ffmpeg: &Path, root: &Path) -> PathBuf {
    let cover = root.join("embedded-cover.png");
    let mut command = Command::new(ffmpeg);
    command
        .args([
            "-v",
            "error",
            "-nostdin",
            "-f",
            "lavfi",
            "-i",
            "color=c=red:s=64x64",
            "-frames:v",
            "1",
            "-threads",
            "1",
        ])
        .arg(&cover);
    tool(command).await;
    let path = root.join("song.m4a");
    let mut command = Command::new(ffmpeg);
    command
        .args([
            "-v",
            "error",
            "-nostdin",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=880:sample_rate=48000",
            "-i",
        ])
        .arg(&cover)
        .args([
            "-filter_complex",
            "[1:a]volume=0:enable='lt(t,6)'[dutch]",
            "-map",
            "0:a",
            "-map",
            "[dutch]",
            "-map",
            "2:v",
            "-t",
            "12",
            "-c:a",
            "aac",
            "-b:a",
            "96k",
            "-c:v",
            "copy",
            "-disposition:v:0",
            "attached_pic",
            "-metadata:s:a:0",
            "language=eng",
            "-metadata:s:a:1",
            "language=nld",
        ])
        .arg(&path);
    tool(command).await;
    let mut probe = Command::new(runtime_path("ERSATZRS_NEXT_TEST_FFPROBE"));
    probe
        .args(["-v", "error", "-show_streams", "-of", "json"])
        .arg(&path);
    let value: serde_json::Value = serde_json::from_slice(&tool(probe).await).unwrap();
    let streams = value["streams"].as_array().unwrap();
    assert_eq!(
        streams
            .iter()
            .filter(|s| s["codec_type"] == "audio")
            .count(),
        2
    );
    let video: Vec<_> = streams
        .iter()
        .filter(|s| s["codec_type"] == "video")
        .collect();
    assert_eq!(video.len(), 1);
    assert_eq!(
        video[0]["disposition"]["attached_pic"], 1,
        "cover is not real video"
    );
    assert_eq!(video[0]["index"], 2);
    path
}
async fn render(
    pool: &crate::db::DbPool,
    root: &Path,
    ffmpeg: &Path,
    mode: ChannelSongVideoMode,
) -> Vec<u8> {
    tokio::fs::create_dir_all(root).await.unwrap();
    let recipe =
        crate::song_background::prepare(pool, 10, &resolution(), mode, Some(1), root, ffmpeg)
            .await
            .unwrap();
    assert!(recipe.stream.is_still_image);
    let mut command = Command::new(&recipe.command.program);
    command
        .args(&recipe.command.args)
        .current_dir(&recipe.command.working_directory)
        .envs(&recipe.command.environment);
    let png = tool(command).await;
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), 320);
    assert_eq!(u32::from_be_bytes(png[20..24].try_into().unwrap()), 180);
    png
}
#[allow(clippy::too_many_lines)] // One owned finite song-to-video lifecycle for both track policies.
async fn run(silent: bool) {
    let root = tempfile::tempdir().unwrap();
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let song = generate_song(&ffmpeg, root.path()).await;
    let (pool, locker, mut channel, _) = schedule::tests::fixture().await;
    for sql in [
        r#"INSERT INTO "Song" ("Id") VALUES (10)"#,
        r#"UPDATE "MediaVersion" SET "MovieId"=NULL,"SongId"=10,"Width"=0,"Height"=0,"Duration"='00:00:12' WHERE "Id"=100"#,
        r#"DELETE FROM "Movie" WHERE "Id"=10"#,
        r#"DELETE FROM "Subtitle""#,
        r#"DELETE FROM "MediaStream""#,
        r#"INSERT INTO "SongMetadata" ("Id","SongId","Title","Album","Artists","MetadataKind","DateAdded","DateUpdated") VALUES (20,10,'Synthetic Song','Synthetic Album','["Synthetic Artist"]',0,'2026-09-22','2026-09-22')"#,
        r#"INSERT INTO "MediaStream" ("Index","Codec","MediaStreamKind","Language","Channels","Default","Forced","AttachedPic","MediaVersionId") VALUES (0,'aac',2,'eng',1,1,0,0,100),(1,'aac',2,'nld',1,0,0,0,100),(2,'png',1,NULL,0,0,0,1,100)"#,
        r#"INSERT INTO "Subtitle" ("SongMetadataId","SubtitleKind","Title","StreamIndex","Codec","Default","Forced","SDH","Language","IsExtracted","Path","DateAdded","DateUpdated") VALUES (20,1,'Dutch lyrics',0,'subrip',1,0,0,'nld',0,'song.nl.srt','2026-09-22','2026-09-22')"#,
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    sqlx::query(r#"UPDATE "MediaFile" SET "Path"=? WHERE "Id"=1"#)
        .bind(song.to_str().unwrap())
        .execute(&pool)
        .await
        .unwrap();
    tokio::fs::write(root.path().join("song.nl.srt"),
        "1\n00:00:00,000 --> 00:00:06,000\nBEFORE SEEK\n\n2\n00:00:06,000 --> 00:00:10,000\nSYNTHETIC LYRIC\n").await.unwrap();
    channel.preferred_audio_language_code = Some("nld".into());
    channel.preferred_subtitle_language_code = Some("nld".into());
    channel.subtitle_mode = if silent {
        ChannelSubtitleMode::None
    } else {
        ChannelSubtitleMode::Any
    };
    channel.song_video_mode = if silent {
        ChannelSongVideoMode::WithProgress
    } else {
        ChannelSongVideoMode::Default
    };
    crate::repositories::make_repositories(pool.clone())
        .channel
        .upsert(&channel)
        .await
        .unwrap();
    let start = chrono::DateTime::from_timestamp(chrono::Utc::now().timestamp(), 0).unwrap();
    let second_start = start + chrono::Duration::seconds(6);
    let finish = second_start + chrono::Duration::seconds(2);
    let item = PlayoutItem {
        id: 1,
        playout_id: 1,
        media_item_id: 10,
        start,
        finish: second_start,
        in_point: chrono::Duration::seconds(6),
        out_point: chrono::Duration::seconds(12),
        ..Default::default()
    };
    crate::repositories::make_repositories(pool.clone())
        .playout
        .upsert_item(&item)
        .await
        .unwrap();
    let mut projection = selection::project_deferred_item_with_audio(
        &pool,
        &channel,
        &item,
        100,
        local(&song),
        &resolution(),
        if silent {
            selection::AudioDirective::Published(None)
        } else {
            selection::AudioDirective::Select
        },
    )
    .await
    .unwrap();
    assert!(
        projection.song.is_some(),
        "embedded cover must use generated song projection"
    );
    assert!(
        projection.pending_burn.is_none(),
        "local lyrics are directly readable"
    );
    let relay = MediaRelay::new();
    relay.configure(([127, 0, 0, 1], 1).into()).unwrap();
    projection
        .publish_graphics(&relay, Some(uuid::Uuid::new_v4()), start)
        .unwrap();
    let before = serde_json::to_value(&projection.item).unwrap();
    let video = &before["tracks"]["video"];
    assert_eq!(video["stream_index"], 0);
    assert_eq!(video["source"]["probe_hint"]["format_name"], "png_pipe");
    assert_eq!(video["source"]["probe_hint"]["video"][0]["width"], 320);
    assert_eq!(video["source"]["probe_hint"]["duration_ms"], 6000);
    assert!(
        video["source"]["in_point_ms"].is_null(),
        "generated picture is not sought into cover/audio"
    );
    if silent {
        assert!(before["tracks"]["audio"]["source"]["params"]
            .as_str()
            .unwrap()
            .starts_with("anullsrc"));
        assert_eq!(
            before["tracks"]["audio"]["source"]["probe_hint"]["duration_ms"],
            6000
        );
    } else {
        assert_eq!(before["tracks"]["audio"]["stream_index"], 1);
        assert_eq!(before["tracks"]["audio"]["source"]["in_point_ms"], 6000);
        assert_eq!(before["tracks"]["subtitle"]["source"]["in_point_ms"], 6000);
    }
    let png = render(
        &pool,
        &root.path().join("render"),
        &ffmpeg,
        channel.song_video_mode,
    )
    .await;
    assert_eq!(
        png,
        render(
            &pool,
            &root.path().join("repeat"),
            &ffmpeg,
            channel.song_video_mode
        )
        .await,
        "same occurrence reconnects with the same generated background"
    );
    if silent {
        assert_ne!(
            png,
            render(
                &pool,
                &root.path().join("default-layout"),
                &ffmpeg,
                ChannelSongVideoMode::Default
            )
            .await,
            "WithProgress still reserves upstream layout space"
        );
    }
    let picture = root.path().join("generated-song.png");
    tokio::fs::write(&picture, png).await.unwrap();
    let track = projection
        .item
        .tracks
        .as_mut()
        .unwrap()
        .video
        .as_mut()
        .unwrap();
    let Some(wire::Source::Http { probe_hint, .. }) = track.source.take() else {
        panic!("signed callback source required");
    };
    // Harness-only materialization retains the exact callback probe hint.
    track.source = Some(wire::Source::Local {
        path: picture.to_str().unwrap().into(),
        probe_hint,
        in_point_ms: None,
        out_point_ms: None,
    });
    let second = video_handoff(&ffmpeg, root.path(), second_start, finish).await;
    let playout = root.path().join("playout");
    tokio::fs::create_dir(&playout).await.unwrap();
    tokio::fs::write(
        playout.join(format!(
            "{}_{}.json",
            start.timestamp_millis(),
            finish.timestamp_millis()
        )),
        serde_json::to_vec(&wire::Playout {
            version: wire::PLAYOUT_VERSION.into(),
            items: vec![projection.item, second],
        })
        .unwrap(),
    )
    .await
    .unwrap();
    let mut config = super::super::tests::config();
    config.playout.folder = playout.to_str().unwrap().into();
    config.playout.virtual_start = Some(start);
    config.ffmpeg.ffmpeg_path = ffmpeg.to_str().unwrap().into();
    config.ffmpeg.ffprobe_path = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE")
        .to_str()
        .unwrap()
        .into();
    config.normalization.audio.sample_rate_hz = Some(48_000);
    let output = root.path().join("output");
    let mut process = NextProcess::start_troubleshooting(
        &runtime_path("ERSATZRS_NEXT_TEST_BIN"),
        "1",
        &output,
        &config,
        None,
        Instant::now() + Duration::from_secs(30),
    )
    .await
    .unwrap();
    let status = timeout(Duration::from_secs(30), process.wait()).await;
    if status.is_err() {
        process.stop().await.unwrap();
    }
    let retained = root.keep();
    eprintln!(
        "decoded song seek/audio/lyrics/handoff, silent={silent}: {}",
        retained.display()
    );
    let status = status
        .expect("song schedule must terminate naturally")
        .unwrap();
    assert!(
        status.success(),
        "song process failed: {status}; {}",
        process.diagnostics()
    );
    let playlist = output.join("ffmpeg.m3u8");
    assert!(tokio::fs::read_to_string(&playlist)
        .await
        .unwrap()
        .contains("#EXT-X-ENDLIST"));
    assert_decoded(&ffmpeg, &playlist, &picture, silent).await;
    let mut probe = Command::new(runtime_path("ERSATZRS_NEXT_TEST_FFPROBE"));
    probe
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=nw=1:nk=1",
        ])
        .arg(&playlist);
    let duration = String::from_utf8(tool(probe).await)
        .unwrap()
        .trim()
        .parse::<f64>()
        .unwrap();
    assert!(
        (duration - 8.0).abs() < 0.3,
        "finite song/video schedule: {duration}s"
    );
    // wait() and tool() reap the exact owned Next and fixture processes.
    drop(locker);
}
async fn video_handoff(
    ffmpeg: &Path,
    root: &Path,
    start: chrono::DateTime<chrono::Utc>,
    finish: chrono::DateTime<chrono::Utc>,
) -> wire::PlayoutItem {
    let path = root.join("handoff.mkv");
    let mut command = Command::new(ffmpeg);
    command
        .args([
            "-v",
            "error",
            "-nostdin",
            "-f",
            "lavfi",
            "-i",
            "color=c=lime:s=320x180:r=25",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=880:sample_rate=48000",
            "-map",
            "0:v",
            "-map",
            "1:a",
            "-map",
            "2:a",
            "-t",
            "2",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-threads",
            "2",
            "-c:a",
            "aac",
        ])
        .arg(&path);
    tool(command).await;
    let pool = selection::tests::fixture_pool().await;
    let item = PlayoutItem {
        id: 2,
        media_item_id: 10,
        start,
        finish,
        out_point: chrono::Duration::seconds(2),
        ..Default::default()
    };
    selection::project_item(
        &pool,
        &ersatzrs_domain::Channel::default(),
        &item,
        100,
        local(&path),
        &resolution(),
    )
    .await
    .unwrap()
}
async fn frame(ffmpeg: &Path, path: &Path, seek: &str) -> Vec<u8> {
    let mut command = Command::new(ffmpeg);
    command
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(path)
        .args([
            "-ss",
            seek,
            "-map",
            "0:v:0",
            "-frames:v",
            "1",
            "-vf",
            "scale=160:90",
            "-pix_fmt",
            "rgb24",
            "-f",
            "rawvideo",
            "pipe:1",
        ]);
    let bytes = tool(command).await;
    assert_eq!(bytes.len(), 160 * 90 * 3);
    bytes
}
fn difference(a: &[u8], b: &[u8]) -> f64 {
    assert_eq!(a.len(), b.len());
    let sum: u32 = a
        .iter()
        .zip(b)
        .map(|(a, b)| u32::from(a.abs_diff(*b)))
        .sum();
    f64::from(sum) / f64::from(u32::try_from(a.len()).unwrap())
}
async fn assert_decoded(ffmpeg: &Path, playlist: &Path, picture: &Path, silent: bool) {
    let expected = frame(ffmpeg, picture, "0").await;
    assert!(
        expected.chunks_exact(3).any(|p| p != &expected[..3]),
        "generated image is not a frozen solid cover"
    );
    assert!(
        expected
            .chunks_exact(3)
            .filter(|p| p[0] > 180 && p[1] < 40 && p[2] < 40)
            .count()
            < 160 * 90 / 2,
        "embedded red cover must not replace the generated picture"
    );
    let early = frame(ffmpeg, playlist, "1").await;
    let late = frame(ffmpeg, playlist, "5.25").await;
    assert!(
        difference(&late, &expected) < 6.0,
        "decoded background must match the common renderer"
    );
    if silent {
        let reserved_bottom = 160 * 78 * 3;
        assert!(
            difference(&early[reserved_bottom..], &late[reserved_bottom..]) < 1.5
                && difference(&late[reserved_bottom..], &expected[reserved_bottom..]) < 4.0,
            "Next WithProgress must not add a moving native bar"
        );
    } else {
        let bottom = 160 * 60 * 3;
        assert!(
            difference(&early[bottom..], &late[bottom..]) > 0.5,
            "sought lyric must burn during its cue and disappear afterwards"
        );
    }
    for seek in ["1", "4.5", "6.5", "7.25"] {
        let mut command = Command::new(ffmpeg);
        command
            .args(["-v", "error", "-nostdin", "-i"])
            .arg(playlist)
            .args([
                "-ss", seek, "-map", "0:a:0", "-t", "0.5", "-ac", "1", "-ar", "8000", "-f",
                "s16le", "pipe:1",
            ]);
        let pcm = tool(command).await;
        let samples: Vec<i16> = pcm
            .chunks_exact(2)
            .map(|p| i16::from_le_bytes([p[0], p[1]]))
            .collect();
        assert!(samples.len() >= 3500);
        let song_time = seek.parse::<f64>().unwrap() < 6.0;
        if silent && song_time {
            assert!(
                samples.iter().all(|s| s.unsigned_abs() <= 10),
                "explicit song silence must not restore language tracks"
            );
        } else {
            let crossings = samples.windows(2).filter(|p| p[0] <= 0 && p[1] > 0).count();
            let hz = f64::from(u32::try_from(crossings).unwrap()) * 8000.0
                / f64::from(u32::try_from(samples.len()).unwrap());
            let expected = if song_time { 880.0 } else { 440.0 };
            assert!(
                (hz - expected).abs() < 50.0,
                "audio at {seek}: expected {expected}Hz, got {hz}Hz"
            );
        }
    }
    for seek in ["6.5", "7.25"] {
        let decoded = frame(ffmpeg, playlist, seek).await;
        assert!(
            decoded
                .chunks_exact(3)
                .all(|p| p[1] > 180 && p[0] < 40 && p[2] < 40),
            "different-colour video replaces the song at its scheduled boundary"
        );
    }
}
