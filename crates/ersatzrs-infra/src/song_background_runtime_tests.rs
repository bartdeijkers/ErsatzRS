//! Native acceptance using the production HLS builder: sought tone, captions and
//! moving progress. Synthetic PCM/subtitles contain no personal media.
use super::*;
use std::process::Stdio;
use tokio::process::Command;

async fn tool(mut command: Command) -> Vec<u8> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = crate::addons::spawn_managed_addon_process(command).unwrap();
    let mut stdout = child.stdout().take().unwrap().take(32 * 1024 * 1024);
    let mut stderr = child.stderr().take().unwrap().take(64 * 1024);
    let mut bytes = Vec::new();
    let mut errors = Vec::new();
    let result = tokio::time::timeout(Duration::from_secs(20), async {
        let (out, err, status) = tokio::join!(
            stdout.read_to_end(&mut bytes),
            stderr.read_to_end(&mut errors),
            child.wait()
        );
        out.unwrap();
        err.unwrap();
        status.unwrap()
    })
    .await;
    if result.is_err() {
        child.kill().await.unwrap();
    }
    assert!(
        result.unwrap().success(),
        "runtime error: {}",
        String::from_utf8_lossy(&errors)
    );
    bytes
}

fn audio_fixture() -> Vec<u8> {
    let samples: u32 = 48_000 * 5;
    let length = samples * 2;
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + length).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&48_000_u32.to_le_bytes());
    wav.extend_from_slice(&96_000_u32.to_le_bytes());
    wav.extend_from_slice(&2_u16.to_le_bytes());
    wav.extend_from_slice(&16_u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&length.to_le_bytes());
    for index in 0..samples {
        let frequency = if index < 96_000 { 440.0 } else { 880.0 };
        let value =
            (f64::from(index) / 48_000.0 * frequency * std::f64::consts::TAU).sin() * 8000.0;
        #[expect(
            clippy::cast_possible_truncation,
            reason = "Synthetic PCM amplitude is bounded to +/-8000"
        )]
        let sample = value.round() as i16;
        wav.extend_from_slice(&sample.to_le_bytes());
    }
    wav
}

async fn frame(ffmpeg: &Path, segment: &Path, at: &str) -> Vec<u8> {
    let mut command = Command::new(ffmpeg);
    command
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(segment)
        .args([
            "-ss",
            at,
            "-frames:v",
            "1",
            "-pix_fmt",
            "rgb24",
            "-f",
            "rawvideo",
            "pipe:1",
        ]);
    let pixels = tool(command).await;
    assert_eq!(pixels.len(), 320 * 180 * 3);
    pixels
}

#[tokio::test]
#[ignore = "Requires exact managed ErsatzRS-ffmpeg; manual native song acceptance"]
async fn native_sought_song_decodes_audio_captions_and_moving_progress() {
    let ffmpeg = PathBuf::from(ersatzrs_ffmpeg::ffmpeg_path());
    let root = tempfile::tempdir().unwrap();
    let (pool, audio, subtitle) = fixture_assets(root.path()).await;
    let resolution = Resolution {
        width: 320,
        height: 180,
        ..Resolution::default()
    };
    let (_sender, receiver) = oneshot::channel();
    let song = prepare_native(
        &pool,
        42,
        7,
        &resolution,
        99,
        &ffmpeg,
        Instant::now() + Duration::from_secs(15),
        receiver,
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        song.used_artwork_fallback.load(Ordering::Acquire),
        "decoder-invalid cover gets one supervised default retry"
    );
    let profile = native_profile(resolution);
    let source_audio =
        ersatzrs_ffmpeg::SourceAudioStream::new(ersatzrs_ffmpeg::SourceAudioCodec::Unknown)
            .with_index(0);
    let command = crate::hls_worker::build_hls_command_with_opacity_clock_and_song(
        audio.to_string_lossy().as_ref(),
        root.path(),
        ersatzrs_ffmpeg::OutputFormat::Hls,
        true,
        false,
        false,
        Duration::from_secs(2),
        Some(Duration::from_secs(3)),
        None,
        &profile,
        Some(ersatzrs_ffmpeg::FrameRate::new(25, 1)),
        ersatzrs_ffmpeg::OutputMetadata::default(),
        ersatzrs_ffmpeg::FfmpegCapabilityReport::default(),
        None,
        vec![source_audio],
        crate::playback::PlaybackAudioSelection::Exact(0),
        subtitle,
        Vec::new(),
        Vec::new(),
        None,
        Some(&song),
    )
    .unwrap();
    let mut render = Command::new(&ffmpeg);
    render.current_dir(root.path()).args(&command.arguments);
    song.mark_in_use();
    tool(render).await;
    song.confirm_reaped();
    let playlist = tokio::fs::read_to_string(root.path().join("live.m3u8"))
        .await
        .expect("successful HLS encoder must publish its manifest");
    let uri = playlist
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .unwrap_or_else(|| panic!("HLS manifest has no media URI: {playlist:.4096}"));
    assert!(
        Path::new(uri).components().count() == 1
            && Path::new(uri)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("ts")),
        "native HLS fixture expects a local MPEG-TS segment URI: {uri:.256}"
    );
    let segment = root.path().join(uri);
    assert!(
        segment.is_file(),
        "HLS manifest refers to an absent segment {uri:.256}: {playlist:.4096}"
    );
    let first = frame(&ffmpeg, &segment, "0.5").await;
    let last = frame(&ffmpeg, &segment, "2.5").await;
    capture_artifacts(
        &first,
        &last,
        &command.arguments,
        &playlist,
        &segment,
        &song.path,
    )
    .await;
    assert_caption_and_progress(&first, &last);
    assert_sought_audio(&ffmpeg, &segment).await;
    let folder = song.path.parent().unwrap().to_path_buf();
    song.remove_after_reap().await;
    assert!(!folder.exists());
}

async fn fixture_assets(root: &Path) -> (DbPool, PathBuf, Option<InputFile>) {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    for sql in [
        r#"CREATE TABLE "Song" ("Id" INTEGER)"#,
        r#"CREATE TABLE "Channel" ("Id" INTEGER, "SongVideoMode" INTEGER)"#,
        r#"CREATE TABLE "SongMetadata" ("Id" INTEGER, "SongId" INTEGER, "Title" TEXT, "Album" TEXT, "Artists" TEXT, "AlbumArtists" TEXT)"#,
        r#"CREATE TABLE "Artwork" ("Id" INTEGER, "SongMetadataId" INTEGER, "ArtworkKind" INTEGER, "Path" TEXT, "BlurHash43" TEXT, "BlurHash54" TEXT, "BlurHash64" TEXT)"#,
        r#"INSERT INTO "Song" VALUES(42)"#,
        r#"INSERT INTO "Channel" VALUES(7,1)"#,
        r#"INSERT INTO "SongMetadata" VALUES(1,42,'Native Song','Album','Test','Test')"#,
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    // Accepted image signature, invalid PNG decoder input: optional artwork
    // must fail its first supervised renderer and use the single default retry.
    let bad_cover = root.join("bad.png");
    tokio::fs::write(&bad_cover, b"\x89PNG\r\n\x1a\ninvalid")
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "Artwork" VALUES(1,1,1,?,NULL,NULL,NULL)"#)
        .bind(bad_cover.to_string_lossy().as_ref())
        .execute(&pool)
        .await
        .unwrap();
    let audio = root.join("song.wav");
    tokio::fs::write(&audio, audio_fixture()).await.unwrap();
    let captions = root.join("song.ass");
    tokio::fs::write(&captions,"[Script Info]\nScriptType: v4.00+\nPlayResX: 320\nPlayResY: 180\n[V4+ Styles]\nFormat: Name,Fontname,Fontsize,PrimaryColour,SecondaryColour,OutlineColour,BackColour,Bold,Italic,Underline,StrikeOut,ScaleX,ScaleY,Spacing,Angle,BorderStyle,Outline,Shadow,Alignment,MarginL,MarginR,MarginV,Encoding\nStyle: Default,Arial,20,&H00FF00FF,&H00FF00FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,0,0,7,0,0,0,1\n[Events]\nFormat: Layer,Start,End,Style,Name,MarginL,MarginR,MarginV,Effect,Text\nDialogue: 0,0:00:00.00,0:00:02.00,Default,,0,0,0,,{\\pos(20,20)}WRONG\nDialogue: 0,0:00:02.00,0:00:05.00,Default,,0,0,0,,{\\pos(220,20)}SOUGHT\n").await.unwrap();
    let subtitle = InputFile::text_subtitle(
        captions.to_string_lossy(),
        ersatzrs_ffmpeg::SubtitleMethod::Burn,
    );
    let subtitle = crate::subtitles::rebase_burned_text_subtitle_input(Some(subtitle), 2).await;
    (pool, audio, subtitle)
}

fn native_profile(resolution: Resolution) -> crate::playback::PlaybackProfile {
    crate::playback::PlaybackProfile {
        resolution,
        profile: ersatzrs_domain::FFmpegProfile {
            video_format: ersatzrs_domain::FFmpegProfileVideoFormat::H264,
            audio_format: ersatzrs_domain::FFmpegProfileAudioFormat::Aac,
            video_preset: Some("veryfast".to_owned()),
            video_profile: Some("high".to_owned()),
            video_bitrate: 1000,
            video_buffer_size: 2000,
            audio_bitrate: 128,
            audio_buffer_size: 256,
            audio_channels: 2,
            audio_sample_rate: 48,
            thread_count: 1,
            ..ersatzrs_domain::FFmpegProfile::default()
        },
    }
}

#[expect(
    clippy::disallowed_methods,
    reason = "Optional artifact export for the explicit managed-runtime acceptance fixture"
)]
async fn capture_artifacts(
    first: &[u8],
    last: &[u8],
    arguments: &[String],
    playlist: &str,
    segment: &Path,
    background: &Path,
) {
    if let Some(directory) = std::env::var_os("ERSATZRS_TEST_NATIVE_SONG_ARTIFACTS") {
        let directory = PathBuf::from(directory);
        tokio::fs::create_dir_all(&directory).await.unwrap();
        tokio::fs::write(directory.join("first.rgb"), first)
            .await
            .unwrap();
        tokio::fs::write(directory.join("last.rgb"), last)
            .await
            .unwrap();
        tokio::fs::write(
            directory.join("command.json"),
            serde_json::to_vec_pretty(arguments).unwrap(),
        )
        .await
        .unwrap();
        tokio::fs::write(directory.join("playlist.m3u8"), playlist)
            .await
            .unwrap();
        tokio::fs::copy(segment, directory.join("segment.ts"))
            .await
            .unwrap();
        tokio::fs::copy(background, directory.join("background.png"))
            .await
            .unwrap();
    }
}

fn assert_caption_and_progress(first: &[u8], last: &[u8]) {
    // A saturated magenta caption cannot be confused with the generated
    // green/teal picture, its white metadata, or the native progress mask.
    let caption_pixels = |pixels: &[u8], left: usize, right: usize| {
        (18..46)
            .flat_map(|y| (left..right).map(move |x| y * 320 + x))
            .filter(|index| {
                let pixel = &pixels[index * 3..index * 3 + 3];
                pixel[0] > 170 && pixel[1] < 110 && pixel[2] > 170
            })
            .count()
    };
    assert!(
        caption_pixels(first, 210, 319) > 80,
        "sought caption must be on the right: right={}, left={}",
        caption_pixels(first, 210, 319),
        caption_pixels(first, 10, 105)
    );
    assert!(
        caption_pixels(first, 10, 105) < 5,
        "pre-seek caption must not be burned"
    );
    let changed = (164..172)
        .flat_map(|y| (170..290).map(move |x| y * 320 + x))
        .filter(|index| {
            first[index * 3..index * 3 + 3]
                .iter()
                .zip(&last[index * 3..index * 3 + 3])
                .any(|(a, b)| a.abs_diff(*b) > 20)
        })
        .count();
    assert!(changed > 100, "progress bar must move, changed={changed}");
}

async fn assert_sought_audio(ffmpeg: &Path, segment: &Path) {
    let mut decode = Command::new(ffmpeg);
    decode.args(["-v", "error", "-i"]).arg(segment).args([
        "-ss", "0.2", "-t", "0.5", "-vn", "-ac", "1", "-ar", "48000", "-f", "f32le", "pipe:1",
    ]);
    let pcm = tool(decode).await;
    let values = pcm
        .chunks_exact(4)
        .map(|sample| f32::from_le_bytes(sample.try_into().unwrap()))
        .collect::<Vec<_>>();
    assert!(values.len() > 20000);
    let crossings = values
        .windows(2)
        .filter(|pair| pair[0].is_sign_positive() != pair[1].is_sign_positive())
        .count();
    assert!(
        crossings > 700 && crossings < 1000,
        "sought 880Hz audio, crossings={crossings}"
    );
}
