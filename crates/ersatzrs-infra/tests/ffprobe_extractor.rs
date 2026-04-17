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
