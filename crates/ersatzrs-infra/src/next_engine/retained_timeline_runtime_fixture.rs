//! Unix integration fixture; Python 3 parses the actual v4 request, `FFmpeg` applies it.
use super::*;
use ersatzrs_core::repositories::AddonMediaListRevisionFile;

pub(super) const PRODUCER: &str = r#"#!/bin/sh
set -eu
printf '%s\n' "$1" >> ../producer-starts
cat > ../request.json
exec python3 -c '
import json, os
with open("../request.json", encoding="utf-8") as stream:
    request = json.load(stream)
ranges = request["retained_intervals_milliseconds"]
n = len(ranges)
filters = ["[0:v]split=" + str(n) + "".join("[v%d]" % i for i in range(n))]
for a in range(2):
    filters.append("[0:a:%d]asplit=%d" % (a,n) + "".join("[a%d_%d]" % (a,i) for i in range(n)))
for i, interval in enumerate(ranges):
    start, end = interval["start"] / 1000, interval["end"] / 1000
    filters.append("[v%d]trim=start=%.3f:end=%.3f,setpts=PTS-STARTPTS[cv%d]" % (i,start,end,i))
    for a in range(2):
        filters.append("[a%d_%d]atrim=start=%.3f:end=%.3f,asetpts=PTS-STARTPTS[ca%d_%d]" % (a,i,start,end,a,i))
filters.append("".join("[cv%d][ca0_%d][ca1_%d]" % (i,i,i) for i in range(n)) + "concat=n=%d:v=1:a=2[v][a0][a1]" % n)
args = [os.environ["FFMPEG_BIN"], "-v", "error", "-nostdin", "-i", "../source.mkv", "-filter_complex", ";".join(filters), "-map", "[v]", "-map", "[a0]", "-map", "[a1]", "-c:v", "rawvideo", "-pix_fmt", "yuv420p", "-threads", "1", "-c:a", "pcm_f32le", "-metadata:s:a:0", "language=eng", "-metadata:s:a:0", "title=English", "-metadata:s:a:1", "language=nld", "-metadata:s:a:1", "title=Dutch", "-f", "nut", "pipe:1"]
os.execv(args[0], args)
'
"#;

pub(super) async fn generate_source(ffmpeg: &Path, root: &Path) -> PathBuf {
    let original = root.join("original.mkv");
    generate_clip(ffmpeg, &original).await;
    let path = root.join("source.mkv");
    let mut command = Command::new(ffmpeg);
    command.args(["-v","error","-nostdin","-i"]).arg(original)
        .args(["-map","0","-t","30","-vf","drawbox=color=red:t=fill:enable='lt(t,6)',drawbox=color=yellow:t=fill:enable='gte(t,12)*lt(t,18)',drawbox=color=green:t=fill:enable='gte(t,18)*lt(t,24)',drawbox=color=cyan:t=fill:enable='gte(t,24)'","-c:v","libx264","-preset","ultrafast","-threads","2","-c:a","copy"])
        .arg(&path);
    tool(command).await;
    path
}

const SOURCE_CUES:&str="1\n00:00:00,000 --> 00:00:08,000\nBEFORE SEEK\n\n2\n00:00:10,000 --> 00:00:20,000\nACROSS CUT\n\n3\n00:00:12,000 --> 00:00:18,000\nREMOVED CUT\n\n4\n00:00:20,000 --> 00:00:28,000\nAFTER CUT\n";

fn digest(bytes: &[u8]) -> String {
    let mut text = String::new();
    for byte in Sha256::digest(bytes) {
        write!(text, "{byte:02x}").unwrap();
    }
    text
}

pub(super) async fn subtitles(
    pool: &crate::db::DbPool,
    root: &Path,
    plan: &ersatzrs_core::addon_media_list_playback::AddonMediaListPlaybackTimelinePlan,
    convert: bool,
) -> Vec<AddonMediaListRevisionFile> {
    let source = root.join("source.srt");
    tokio::fs::write(&source, SOURCE_CUES).await.unwrap();
    let mapped =
        crate::subtitles::map_srt_through_timeline(SOURCE_CUES, &plan.timeline().unwrap(), 4096)
            .unwrap();
    let sidecar = root.join("published.nl.srt");
    tokio::fs::write(&sidecar, &mapped).await.unwrap();
    let relative = source
        .strip_prefix(crate::file_system_layout::addon_processed_media_folder())
        .unwrap()
        .to_str()
        .unwrap();
    sqlx::query(r#"INSERT INTO "AddonMediaListArtifact" ("Id","AddonMediaListId","AddonMediaListItemId","CacheKey","Kind","State","RelativePath","ByteLength","Sha256","SubtitleLanguage","SubtitleTimeline","CreatedAt","UpdatedAt","CompletedAt") VALUES (2,1,11,'captions',0,2,?,?,?,'nl-NL',0,'2026-09-23','2026-09-23','2026-09-23')"#)
        .bind(relative).bind(i64::try_from(SOURCE_CUES.len()).unwrap()).bind(digest(SOURCE_CUES.as_bytes())).execute(pool).await.unwrap();
    sqlx::query(
        r#"UPDATE "AddonMediaListLibrary" SET "ManifestPath"=? WHERE "AddonMediaListId"=1"#,
    )
    .bind(root.join("manifest.yml").to_str().unwrap())
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(r#"DELETE FROM "Subtitle""#)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "Subtitle" ("RemoteStreamMetadataId","SubtitleKind","Title","StreamIndex","Codec","Default","Forced","SDH","Language","IsExtracted","Path","DateAdded","DateUpdated") VALUES (30,1,'Dutch',0,'srt',1,0,0,'nld',0,?,'2026-09-23','2026-09-23')"#)
        .bind(sidecar.to_str().unwrap()).execute(pool).await.unwrap();
    sqlx::query(
        r#"UPDATE "Channel" SET "SubtitleMode"=2,"NextEngineTextSubtitleMode"=? WHERE "Id"=1"#,
    )
    .bind(i32::from(convert))
    .execute(pool)
    .await
    .unwrap();
    let hash = digest(mapped.as_bytes());
    let blob = crate::repositories::SqliteAddonMediaListRepository(pool.clone())
        .register_revision_file_blob(
            1,
            &format!("blobs/{hash}"),
            &hash,
            u64::try_from(mapped.len()).unwrap(),
            chrono::Utc::now(),
        )
        .await
        .unwrap();
    vec![AddonMediaListRevisionFile {
        blob_id: blob.id,
        published_relative_path: "published.nl.srt".into(),
        provider_id: Some("item".into()),
        storage_relative_path: blob.storage_relative_path,
        sha256: hash,
        byte_length: u64::try_from(mapped.len()).unwrap(),
    }]
}

pub(super) async fn capture_program(
    handle: &dyn ersatzrs_core::hls::HlsSessionHandle,
    output: &Path,
    saved: &Path,
) -> Result<Vec<(String, f64)>, String> {
    timeout(Duration::from_secs(30), async {
        loop {
            if handle.startup_readiness() == HlsStartupReadiness::Failed {
                return Err("retained session failed".into());
            }
            if let Some(playlist) = handle.trim_playlist().await {
                let mut segments = Vec::new();
                let mut duration = 0.0;
                for line in playlist.lines() {
                    if let Some(value) = line.strip_prefix("#EXTINF:") {
                        duration = value
                            .trim_end_matches(',')
                            .parse::<f64>()
                            .map_err(|e| e.to_string())?;
                    } else if !line.starts_with('#')
                        && Path::new(line)
                            .extension()
                            .is_some_and(|ext| ext.eq_ignore_ascii_case("ts"))
                    {
                        segments.push((line.to_owned(), duration));
                    }
                }
                if segments.iter().map(|(_, d)| d).sum::<f64>() >= 10.0 {
                    let mut bytes = Vec::new();
                    let mut captions = Vec::new();
                    for (name, duration) in segments {
                        bytes.extend(
                            tokio::fs::read(output.join(&name))
                                .await
                                .map_err(|e| e.to_string())?,
                        );
                        let caption =
                            tokio::fs::read_to_string(output.join(name).with_extension("vtt"))
                                .await
                                .map_err(|e| e.to_string())?;
                        captions.push((caption, duration));
                    }
                    tokio::fs::write(saved, bytes)
                        .await
                        .map_err(|e| e.to_string())?;
                    return Ok(captions);
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .map_err(|_| "retained programme capture timed out".to_owned())?
}

pub(super) async fn assert_timeline(
    ffmpeg: &Path,
    saved: &Path,
    captions: &[(String, f64)],
    convert: bool,
) {
    for (time, expected) in [(0.5, [0, 0, 255]), (3.0, [0, 128, 0]), (9.0, [0, 255, 255])] {
        let mut decode = Command::new(ffmpeg);
        decode
            .args(["-v", "error", "-nostdin", "-i"])
            .arg(saved)
            .args([
                "-ss",
                &time.to_string(),
                "-map",
                "0:v:0",
                "-frames:v",
                "1",
                "-vf",
                "crop=160:60:0:0,scale=1:1",
                "-pix_fmt",
                "rgb24",
                "-f",
                "rawvideo",
                "pipe:1",
            ]);
        let pixel = tool(decode).await;
        assert_eq!(pixel.len(), 3);
        for (actual, expected) in pixel.iter().zip(expected) {
            assert!(
                (i32::from(*actual) - expected).abs() < 35,
                "retained source color at {time}: {pixel:?}"
            );
        }
    }
    assert_text_video_and_audio(ffmpeg, saved, !convert).await;
    let all = captions
        .iter()
        .map(|(body, _)| body.as_str())
        .collect::<String>();
    assert!(!all.contains("BEFORE SEEK"));
    assert!(!all.contains("REMOVED CUT"));
    if convert {
        assert!(all.contains("ACROSS CUT"));
        assert!(all.contains("AFTER CUT"));
        for (body, duration) in captions {
            for (start, end) in body.lines().filter_map(|line| line.split_once(" --> ")) {
                assert!(vtt_seconds(start) >= 0.0);
                assert!(
                    vtt_seconds(end) <= duration + 0.002,
                    "retained cue escapes segment"
                );
            }
        }
    } else {
        assert!(!all.contains(" --> "));
    }
}
