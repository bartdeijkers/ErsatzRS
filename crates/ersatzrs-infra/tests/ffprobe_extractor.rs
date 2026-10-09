use std::env;
use std::path::{Path, PathBuf};

use ersatzrs_infra::ffprobe::{parse_ffprobe_json, FfprobeExtractor};
use serde::Serialize;

#[test]
fn ffprobe_args_match_ersatztv_invocation_shape() {
    let command = FfprobeExtractor::new("ffprobe").command_for("/media/movie.mkv");

    insta::assert_json_snapshot!(SerializableCommand::from(command), @r###"
    {
      "program": "ffprobe",
      "args": [
        "-hide_banner",
        "-print_format",
        "json",
        "-show_format",
        "-show_streams",
        "-show_chapters",
        "-read_intervals",
        "%+#1",
        "-show_frames",
        "-i",
        "/media/movie.mkv"
      ]
    }
    "###);
}

#[test]
fn parses_ffprobe_json_to_media_version_snapshot() {
    let version = parse_ffprobe_json("/media/movie.mkv", SAMPLE_FFPROBE_JSON.as_bytes()).unwrap();

    insta::assert_json_snapshot!(SerializableVersion::from(version), @r###"
    {
      "name": "Main",
      "duration_seconds": 12,
      "sample_aspect_ratio": "1:1",
      "display_aspect_ratio": "16:9",
      "r_frame_rate": "24000/1001",
      "video_scan_kind": "Progressive",
      "width": 1920,
      "height": 1080,
      "streams": [
        {
          "index": 0,
          "kind": "Video",
          "codec": "h264",
          "language": null,
          "title": null,
          "default": true,
          "forced": false,
          "channels": null,
          "width": 1920,
          "height": 1080
        },
        {
          "index": 1,
          "kind": "Audio",
          "codec": "aac",
          "language": "eng",
          "title": "Stereo",
          "default": true,
          "forced": false,
          "channels": 2,
          "width": null,
          "height": null
        },
        {
          "index": 2,
          "kind": "Subtitle",
          "codec": "subrip",
          "language": "nld",
          "title": "Dutch",
          "default": false,
          "forced": true,
          "channels": null,
          "width": null,
          "height": null
        }
      ],
      "chapters": [
        {
          "chapter_id": 0,
          "title": "Intro",
          "start_seconds": 0,
          "end_seconds": 5
        },
        {
          "chapter_id": 1,
          "title": "Main",
          "start_seconds": 5,
          "end_seconds": 12
        }
      ]
    }
    "###);
}

fn assert_content_duration(probe: &serde_json::Value, expected: chrono::Duration) {
    let projected =
        parse_ffprobe_json("synthetic.mkv", &serde_json::to_vec(probe).unwrap()).unwrap();
    assert_eq!(projected.version.duration, expected, "probe: {probe}");
}

#[test]
fn content_duration_matches_upstream_video_audio_and_tag_fixtures() {
    // ErsatzTV fb75ce8c LocalStatisticsProviderTests.Duration_Should_Match_Content.
    for (probe, expected_ms) in [
        (
            serde_json::json!({"format":{"duration":"1249.413000"},"streams":[
                {"index":0,"codec_type":"video","tags":{"DURATION":"00:18:39.164000000"}},
                {"index":1,"codec_type":"audio","tags":{"language":"eng","DURATION":"00:18:40.132000000"}},
                {"index":2,"codec_type":"subtitle","duration":"1249.413000","tags":{"DURATION":"00:20:49.413000000"}}
            ]}),
            1_119_164,
        ),
        (
            serde_json::json!({"format":{"duration":"1249.413000"},"streams":[
                {"index":0,"codec_type":"video","tags":{"DURATION-eng":"25:00:01.500000000"}},
                {"index":1,"codec_type":"subtitle","duration":"1249.413000"}
            ]}),
            90_001_500,
        ),
        (
            serde_json::json!({"format":{"duration":"65.000000"},"streams":[
                {"index":0,"codec_type":"video","duration":"60.060000"},
                {"index":1,"codec_type":"audio","duration":"60.100000"}
            ]}),
            60_060,
        ),
        (
            serde_json::json!({"format":{"duration":"200.000000"},"streams":[
                {"index":0,"codec_type":"audio","duration":"180.500000"},
                {"index":1,"codec_type":"video","duration":"0.000000","disposition":{"attached_pic":1}}
            ]}),
            180_500,
        ),
        (
            serde_json::json!({"format":{"duration":"100.000000"},"streams":[
                {"index":0,"codec_type":"video"},
                {"index":1,"codec_type":"audio","duration":"90.000000"}
            ]}),
            100_000,
        ),
        (
            serde_json::json!({"format":{"duration":"100.000000"},"streams":[
                {"index":0,"codec_type":"audio","duration":"90.000000"},
                {"index":1,"codec_type":"audio"}
            ]}),
            100_000,
        ),
    ] {
        assert_content_duration(&probe, chrono::Duration::milliseconds(expected_ms));
    }
}

#[test]
fn content_duration_selects_primary_video_and_requires_all_audio_durations() {
    for (probe, seconds) in [
        (
            serde_json::json!({"format":{"duration":"100"},"streams":[
                {"codec_type":"video","bit_rate":"1000","duration":"11"},
                {"codec_type":"video","bit_rate":"invalid","tags":{"variant_bitrate":"2000"},"duration":"22"},
                {"codec_type":"video","bit_rate":"999999","duration":"99","disposition":{"attached_pic":1}},
                {"codec_type":"audio","duration":"80"},
                {"codec_type":"subtitle","duration":"100"}
            ]}),
            22,
        ),
        (
            serde_json::json!({"format":{"duration":"100"},"streams":[
                {"codec_type":"video","bit_rate":"1000","duration":"11"},
                {"codec_type":"video","bit_rate":"1000","duration":"22"}
            ]}),
            11,
        ),
        (
            serde_json::json!({"format":{"duration":"100"},"streams":[
                {"codec_type":"video","bit_rate":"1000","duration":"11"},
                {"codec_type":"video","bit_rate":"2000"},
                {"codec_type":"audio","duration":"80"}
            ]}),
            100,
        ),
        (
            serde_json::json!({"format":{"duration":"100"},"streams":[
                {"codec_type":"audio","duration":"30"},
                {"codec_type":"audio","tags":{"DURATION":"00:00:45.000000000"}},
                {"codec_type":"video","duration":"1","disposition":{"attached_pic":1}}
            ]}),
            45,
        ),
        (
            serde_json::json!({"format":{"duration":"100"},"streams":[
                {"codec_type":"audio","duration":"30"},
                {"codec_type":"audio","tags":{"DURATION":"invalid"}}
            ]}),
            100,
        ),
    ] {
        assert_content_duration(&probe, chrono::Duration::seconds(seconds));
    }
}

#[test]
fn content_duration_guards_matroska_container_estimates_and_preserves_numeric_precedence() {
    let mut probe = serde_json::json!({"format":{"format_name":"matroska,webm","duration":"20"},"streams":[
        {"index":0,"codec_type":"video","codec_name":"h264","duration":"20.000000","tags":{"DURATION":"00:00:01.000000000"}},
        {"index":1,"codec_type":"subtitle","codec_name":"subrip","tags":{"language":"nld","title":"Dutch","DURATION":"00:00:20.000000000"}}
    ]});
    assert_content_duration(&probe, chrono::Duration::seconds(1));
    let projected =
        parse_ffprobe_json("synthetic.mkv", &serde_json::to_vec(&probe).unwrap()).unwrap();
    assert_eq!(projected.streams[1].language.as_deref(), Some("nld"));
    assert_eq!(projected.streams[1].title.as_deref(), Some("Dutch"));

    for format in [serde_json::json!("mov,mp4,m4a"), serde_json::Value::Null] {
        probe["format"]["format_name"] = format;
        assert_content_duration(&probe, chrono::Duration::seconds(20));
    }
    probe["format"]["format_name"] = serde_json::json!("matroska,webm");
    probe["streams"][0]["duration"] = serde_json::json!("10");
    assert_content_duration(&probe, chrono::Duration::seconds(10));
    probe["streams"][0]["duration"] = serde_json::json!("20");
    probe["streams"][0]["tags"]["DURATION"] = serde_json::json!("00:00:25.000000000");
    assert_content_duration(&probe, chrono::Duration::seconds(20));
    probe["streams"][0]["tags"] = serde_json::json!({"duration-eng":"00:00:01.000000000"});
    assert_content_duration(&probe, chrono::Duration::seconds(1));
    probe["streams"][0]["tags"]["DURATION"] = serde_json::json!("00:00:02.000000000");
    assert_content_duration(&probe, chrono::Duration::seconds(2));
}

#[test]
fn content_duration_rejects_invalid_tags_and_unrepresentable_values_without_panicking() {
    let mut probe = serde_json::json!({"format":{"format_name":"matroska,webm","duration":"20"},"streams":[
        {"codec_type":"video","duration":"20","tags":{"DURATION":"invalid"}}
    ]});
    for invalid in [
        "invalid",
        "00:60:01",
        "00:00:60",
        "-1:00:00",
        "00:00:-1",
        "00:00:NaN",
        "00:00:inf",
        "00:00:00",
        "00:00:01.",
        "00:00:01.1234567890",
        "00:00:01.é",
        "00:00:01:00",
        "99999999999999999999:00:01",
        "9223372036854775807:00:01",
    ] {
        probe["streams"][0]["tags"]["DURATION"] = serde_json::json!(invalid);
        assert_content_duration(&probe, chrono::Duration::seconds(20));
    }
    probe["streams"][0]["tags"]["DURATION"] = serde_json::json!(123);
    assert_content_duration(&probe, chrono::Duration::seconds(20));
    probe["streams"][0]["tags"]["DURATION"] = serde_json::json!("25:00:01.123456789");
    for invalid in ["0", "-1", "NaN", "inf", "1.é", "9223372036854775807"] {
        probe["streams"][0]["duration"] = serde_json::json!(invalid);
        assert_content_duration(
            &probe,
            chrono::Duration::seconds(90_001) + chrono::Duration::nanoseconds(123_456_789),
        );
    }
    probe["streams"][0]["tags"] = serde_json::json!({});
    assert_content_duration(&probe, chrono::Duration::seconds(20));
    probe["format"]["duration"] = serde_json::json!("9223372036854775807");
    assert_content_duration(&probe, chrono::Duration::zero());
}

#[tokio::test]
#[ignore = "requires documented Next test FFmpeg/ffprobe paths; generates synthetic long-subtitle Matroska"]
#[allow(clippy::disallowed_methods)] // Explicit opt-in test tools, never production configuration.
async fn real_content_duration_excludes_subtitles_past_video_eof() {
    let ffmpeg = env::var_os("ERSATZRS_NEXT_TEST_FFMPEG").expect("set test FFmpeg path");
    let ffprobe = env::var("ERSATZRS_NEXT_TEST_FFPROBE").expect("set test ffprobe path");
    let root = tempfile::tempdir().unwrap();
    let subtitle = root.path().join("synthetic.srt");
    let clip = root.path().join("synthetic.mkv");
    tokio::fs::write(
        &subtitle,
        "1\n00:00:00,000 --> 00:00:20,000\nSynthetic duration fixture\n",
    )
    .await
    .unwrap();
    let generated = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        tokio::process::Command::new(ffmpeg)
            .kill_on_drop(true)
            .args([
                "-v",
                "error",
                "-nostdin",
                "-f",
                "lavfi",
                "-i",
                "color=blue:s=64x64:r=2:d=1",
                "-i",
            ])
            .arg(&subtitle)
            .args([
                "-map",
                "0:v",
                "-map",
                "1:s",
                "-c:v",
                "libx264",
                "-threads",
                "1",
                "-c:s",
                "srt",
                "-metadata:s:s:0",
                "language=nld",
            ])
            .arg(&clip)
            .output(),
    )
    .await
    .expect("synthetic mux timed out")
    .unwrap();
    assert!(generated.status.success(), "synthetic mux must succeed");
    let projected = FfprobeExtractor::new(ffprobe).extract(&clip).await.unwrap();
    assert_eq!(projected.version.duration, chrono::Duration::seconds(1));
    assert!(projected.streams.iter().any(|stream| {
        stream.kind == ersatzrs_domain::MediaStreamKind::Subtitle
            && stream.language.as_deref() == Some("nld")
    }));
}

#[tokio::test]
async fn real_ffprobe_invocation_is_env_gated() {
    let Some(ffprobe) = ffprobe_binary() else {
        eprintln!("skipping: ffprobe not found and ERSATZRS_FFPROBE is unset");
        return;
    };
    let Some(fixture) = big_buck_bunny_fixture() else {
        eprintln!("skipping: Big Buck Bunny testdata fixture not found");
        return;
    };

    let version = FfprobeExtractor::new(ffprobe)
        .extract(fixture)
        .await
        .unwrap();

    assert!(version.version.duration.num_seconds() > 0);
    assert!(version.streams.iter().any(|stream| stream.kind.video()));
}

#[allow(clippy::disallowed_methods)]
fn ffprobe_binary() -> Option<String> {
    env::var("ERSATZRS_FFPROBE")
        .ok()
        .or_else(|| find_on_path("ffprobe").map(|path| path.display().to_string()))
}

#[allow(clippy::disallowed_methods)]
fn find_on_path(binary: &str) -> Option<PathBuf> {
    env::var_os("PATH")?
        .to_string_lossy()
        .split(':')
        .find_map(|dir| {
            let path = Path::new(dir).join(binary);
            path.is_file().then_some(path)
        })
}

fn big_buck_bunny_fixture() -> Option<PathBuf> {
    [
        "testdata/bbb_sunflower_1080p_30fps_normal.mp4",
        "testdata/bbb_sunflower_1080p_60fps_normal.mp4",
        "testdata/bbb_sunflower_2160p_30fps_normal.mp4",
        "testdata/bbb_sunflower_2160p_60fps_normal.mp4",
    ]
    .into_iter()
    .map(PathBuf::from)
    .find(|path| path.is_file())
}

#[derive(Serialize)]
struct SerializableCommand {
    program: String,
    args: Vec<String>,
}

impl From<ersatzrs_infra::ffprobe::FfprobeCommand> for SerializableCommand {
    fn from(command: ersatzrs_infra::ffprobe::FfprobeCommand) -> Self {
        Self {
            program: command.program,
            args: command.args,
        }
    }
}

#[derive(Serialize)]
struct SerializableVersion {
    name: String,
    duration_seconds: i64,
    sample_aspect_ratio: Option<String>,
    display_aspect_ratio: Option<String>,
    r_frame_rate: Option<String>,
    video_scan_kind: String,
    width: i32,
    height: i32,
    streams: Vec<SerializableStream>,
    chapters: Vec<SerializableChapter>,
}

impl From<ersatzrs_infra::ffprobe::FfprobeMediaVersion> for SerializableVersion {
    fn from(probed: ersatzrs_infra::ffprobe::FfprobeMediaVersion) -> Self {
        Self {
            name: probed.version.name,
            duration_seconds: probed.version.duration.num_seconds(),
            sample_aspect_ratio: probed.version.sample_aspect_ratio,
            display_aspect_ratio: probed.version.display_aspect_ratio,
            r_frame_rate: probed.version.r_frame_rate,
            video_scan_kind: format!("{:?}", probed.version.video_scan_kind),
            width: probed.version.width,
            height: probed.version.height,
            streams: probed.streams.into_iter().map(Into::into).collect(),
            chapters: probed.chapters.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Serialize)]
struct SerializableStream {
    index: i32,
    kind: String,
    codec: Option<String>,
    language: Option<String>,
    title: Option<String>,
    default: bool,
    forced: bool,
    channels: Option<i32>,
    width: Option<i32>,
    height: Option<i32>,
}

impl From<ersatzrs_domain::MediaStream> for SerializableStream {
    fn from(stream: ersatzrs_domain::MediaStream) -> Self {
        Self {
            index: stream.index,
            kind: format!("{:?}", stream.kind),
            codec: stream.codec,
            language: stream.language,
            title: stream.title,
            default: stream.default,
            forced: stream.forced,
            channels: stream.channels,
            width: stream.width,
            height: stream.height,
        }
    }
}

#[derive(Serialize)]
struct SerializableChapter {
    chapter_id: i64,
    title: String,
    start_seconds: i64,
    end_seconds: i64,
}

impl From<ersatzrs_domain::MediaChapter> for SerializableChapter {
    fn from(chapter: ersatzrs_domain::MediaChapter) -> Self {
        Self {
            chapter_id: chapter.chapter_id,
            title: chapter.title,
            start_seconds: chapter.start_time.num_seconds(),
            end_seconds: chapter.end_time.num_seconds(),
        }
    }
}

trait StreamKindExt {
    fn video(self) -> bool;
}

impl StreamKindExt for ersatzrs_domain::MediaStreamKind {
    fn video(self) -> bool {
        matches!(self, Self::Video)
    }
}

const SAMPLE_FFPROBE_JSON: &str = r#"
{
  "format": {
    "duration": "12.345000"
  },
  "streams": [
    {
      "index": 0,
      "codec_name": "h264",
      "codec_type": "video",
      "width": 1920,
      "height": 1080,
      "sample_aspect_ratio": "1:1",
      "display_aspect_ratio": "16:9",
      "field_order": "progressive",
      "r_frame_rate": "24000/1001",
      "bit_rate": "1200000",
      "disposition": { "default": 1, "forced": 0 }
    },
    {
      "index": 1,
      "codec_name": "aac",
      "codec_type": "audio",
      "channels": 2,
      "disposition": { "default": 1, "forced": 0 },
      "tags": { "language": "eng", "title": "Stereo" }
    },
    {
      "index": 2,
      "codec_name": "subrip",
      "codec_type": "subtitle",
      "disposition": { "default": 0, "forced": 1 },
      "tags": { "language": "nld", "title": "Dutch" }
    }
  ],
  "chapters": [
    {
      "id": 99,
      "start_time": "0.000000",
      "end_time": "5.000000",
      "tags": { "title": "Intro" }
    },
    {
      "id": 100,
      "start_time": "5.000000",
      "end_time": "11.000000",
      "tags": { "title": "Main" }
    }
  ]
}
"#;
