//! Actual callback refusal during replacement, followed by fresh programme output.
//! The executable decoder gate is Unix-only; no Windows shell analogue is claimed.

use super::*;
use sha2::{Digest as _, Sha256};
use std::os::unix::fs::PermissionsExt as _;

struct Fixture {
    root: PathBuf,
    ffmpeg: PathBuf,
    ffprobe: PathBuf,
    clip: PathBuf,
    pool: crate::db::DbPool,
    locker: ersatzrs_core::locking::EntityLocker,
}

fn hash(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut output, byte| {
            write!(output, "{byte:02x}").expect("writing to a string");
            output
        })
}

async fn revision(pool: &crate::db::DbPool) -> String {
    sqlx::query_scalar(r#"SELECT "SourceRevision" FROM "SourceTeletextMetadata""#)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn wait_file(path: &Path) {
    timeout(Duration::from_secs(10), async {
        while !path.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("bounded decoder gate did not publish {}", path.display()));
}

fn decoder_gate(root: &Path, ffmpeg: &Path) -> PathBuf {
    let script = root.join("gated-ffmpeg.py");
    let settings = serde_json::to_string(&serde_json::json!({
        "ffmpeg": std::fs::canonicalize(ffmpeg).unwrap().to_str().unwrap(),
        "root": std::fs::canonicalize(root).unwrap().to_str().unwrap(),
    }))
    .unwrap();
    let source = format!(
        r#"#!/usr/bin/env python3
import hashlib,json,os,pathlib,subprocess,sys,time
config=json.loads({settings:?})
root=pathlib.Path(config['root'])
args=sys.argv[1:]
selected=('-txt_page:2' in args and args[args.index('-txt_page:2')+1]=='889'
          and args[-3:]==['-f','srt','pipe:1'])
if not selected:
    os.execv(config['ffmpeg'],[config['ffmpeg'],*args])
def publish(name,data):
    target=root/name
    temporary=root/(name+'.closed')
    with temporary.open('xb') as output:
        output.write(data)
    os.link(temporary,target)
    temporary.unlink()
publish('decoder-entered.json',b'{{"stage":"owned-text-command","page":889}}')
deadline=time.monotonic()+10
while not (root/'decoder-release').is_file():
    if time.monotonic()>=deadline:
        sys.exit(124)
    time.sleep(0.01)
completed=subprocess.run([config['ffmpeg'],*args],stdin=subprocess.DEVNULL,
                         stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=5)
assert len(completed.stdout)<=131072 and len(completed.stderr)<=131072
publish('old-decoder.srt',completed.stdout)
publish('decoder-exit.json',json.dumps({{'exit':completed.returncode,
    'bytes':len(completed.stdout),'sha256':hashlib.sha256(completed.stdout).hexdigest()}}).encode())
sys.stdout.buffer.write(completed.stdout)
sys.stderr.buffer.write(completed.stderr)
sys.exit(completed.returncode)
"#
    );
    std::fs::write(&script, source).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::canonicalize(script).unwrap()
}

fn fresh_bytes(original: &[u8]) -> Vec<u8> {
    let wire = |bytes: &[u8]| {
        bytes
            .iter()
            .map(|byte| {
                let parity = if byte.count_ones() % 2 == 0 { 0x80 } else { 0 };
                (byte | parity).reverse_bits()
            })
            .collect::<Vec<_>>()
    };
    let marker = wire(b"NEDERLANDSE PAGINA 889 FINAL");
    // Keep FINAL, both box-end controls and every original cue time. Move the
    // controls over six formerly blank cells to append a new visible literal.
    let padding = wire(b"\x0a\x0a      ");
    let addition = wire(b" FRESH\x0a\x0a");
    let offsets: Vec<_> = original
        .windows(marker.len())
        .enumerate()
        .filter_map(|(index, bytes)| (bytes == marker).then_some(index + marker.len()))
        .collect();
    assert_eq!(offsets.len(), 1, "one actual FINAL subtitle row required");
    let mut changed = original.to_vec();
    for offset in offsets {
        assert_eq!(&original[offset..offset + padding.len()], &padding);
        changed[offset..offset + addition.len()].copy_from_slice(&addition);
    }
    assert_ne!(changed, original);
    assert_eq!(changed.len(), original.len());
    changed
}

async fn replace_and_rescan(fixture: &Fixture, original: &[u8]) {
    let old_revision = revision(&fixture.pool).await;
    let changed = fresh_bytes(original);
    let replacement = fixture.root.join("replacement.ts");
    tokio::fs::write(&replacement, &changed).await.unwrap();
    // Atomic replacement of this owned fixture leaves the original held file
    // and its decoder snapshot intact, while the actual source path changes.
    tokio::fs::rename(replacement, &fixture.clip).await.unwrap();
    fixture::acquire_and_store_observed(
        &fixture.pool,
        &fixture.ffprobe,
        &fixture.clip,
        &fixture.root,
        &[888, 889],
    )
    .await;
    let current_revision = revision(&fixture.pool).await;
    assert_ne!(old_revision, current_revision);
    tokio::fs::write(
        fixture.root.join("source-revisions.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "old_revision":old_revision,"current_revision":current_revision,
            "old_sha256":hash(original),"current_sha256":hash(&changed),
            "same_pool":true,"same_source_path":true,
            "managed_acquisition_before_decoder_release":true,
        }))
        .unwrap(),
    )
    .await
    .unwrap();
}

async fn rejected_callback(fixture: &Fixture, original: &[u8]) {
    let host = std::sync::Arc::new(host::NextHost::new(
        fixture.pool.clone(),
        fixture.locker.clone(),
        fixture.root.join("old-playout"),
    ));
    let items = std::sync::Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let statuses = std::sync::Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let mut callback =
        observed_callback_status_listener(host.clone(), items.clone(), statuses.clone()).await;
    let registry = std::sync::Arc::new(crate::hls::InMemoryHlsSessionRegistry::new());
    let output = fixture.root.join("old-output");
    let task = host
        .start(
            registry.clone(),
            host::NextStart {
                channel_id: 1,
                preview: false,
                number: "1".into(),
                executable: runtime_path("ERSATZRS_NEXT_TEST_BIN"),
                output: output.clone(),
                ffmpeg: decoder_gate(&fixture.root, &fixture.ffmpeg),
                ffprobe: fixture.ffprobe.clone(),
                replacements: Vec::new(),
                idle_timeout: None,
                deadline: Instant::now() + Duration::from_secs(30),
            },
        )
        .await
        .unwrap();
    wait_file(&fixture.root.join("decoder-entered.json")).await;
    assert!(fixture.locker.active_teletext_work() > 0);
    replace_and_rescan(fixture, original).await;
    tokio::fs::write(fixture.root.join("decoder-release"), b"release")
        .await
        .unwrap();
    let refused = timeout(Duration::from_secs(15), async {
        loop {
            if statuses.lock().await.contains(&503) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    registry.stop_channel("1").await;
    task.await.unwrap();
    callback.stop().await;
    assert!(!output.exists());
    assert!(registry.active_channels().await.is_empty());
    assert_eq!(fixture.locker.active_teletext_work(), 0);
    assert_eq!(
        crate::transcode_capacity::active_transcode_snapshot(1).active,
        0
    );
    assert!(refused.is_ok(), "actual callback must send HTTP503");
    let statuses = statuses.lock().await;
    assert!(statuses.iter().all(|status| *status == 503));
    assert!(
        items.lock().await.is_empty(),
        "no stale successful caption DTO"
    );
    assert_old_decode_and_empty_cache(fixture).await;
    tokio::fs::write(
        fixture.root.join("rejected-callback.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "actual_written_http_statuses":*statuses,"successful_items":0,
            "source_replaced_while_decoder_owned":true,"rescan_same_pool":true,
            "old_decoder_exit":0,"old_cache_artifacts":0,
            "joined_cleanup":true,"programme_delivery_claim":false,
        }))
        .unwrap(),
    )
    .await
    .unwrap();
}

async fn assert_old_decode_and_empty_cache(fixture: &Fixture) {
    let exit: serde_json::Value = serde_json::from_slice(
        &tokio::fs::read(fixture.root.join("decoder-exit.json"))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(exit["exit"], 0);
    let old = tokio::fs::read(fixture.root.join("old-decoder.srt"))
        .await
        .unwrap();
    assert_eq!(exit["sha256"], hash(&old));
    let old = std::str::from_utf8(&old).unwrap();
    assert!(old.contains("NEDERLANDSE PAGINA 889 FINAL"));
    assert!(!old.contains("FRESH"));
    let cache = fixture.root.join("old-playout/burn-subtitles");
    if cache.exists() {
        let mut entries = tokio::fs::read_dir(cache).await.unwrap();
        assert!(entries.next_entry().await.unwrap().is_none());
    }
}

#[tokio::test]
#[ignore = "requires pinned Next, FFmpeg9, Python3, Unix and fresh ERSATZRS_NEXT_TELETEXT_PROOF_DIR"]
async fn real_next_source_replacement_during_caption_callback_refuses_stale_then_delivers_fresh() {
    let root = fixture::proof_root();
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    let clip = fixture::source(&ffmpeg, &root).await;
    let profile = FFmpegProfile {
        video_format: FFmpegProfileVideoFormat::Copy,
        audio_format: FFmpegProfileAudioFormat::Copy,
        audio_sample_rate: 48,
        audio_channels: 1,
        ..text_profile()
    };
    let (pool, locker) = prepare_schedule(&clip, false, &profile).await;
    fixture::acquire_and_store_observed(&pool, &ffprobe, &clip, &root, &[888, 889]).await;
    sqlx::query(
        r#"UPDATE "Channel" SET "SubtitleMode"=3,"NextEngineTextSubtitleMode"=1 WHERE "Id"=1"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    let original = tokio::fs::read(&clip).await.unwrap();
    tokio::fs::write(root.join("old-source.ts"), &original)
        .await
        .unwrap();
    let fixture = Fixture {
        root,
        ffmpeg,
        ffprobe,
        clip,
        pool,
        locker,
    };
    rejected_callback(&fixture, &original).await;
    assert_eq!(
        tokio::fs::read(fixture.root.join("old-source.ts"))
            .await
            .unwrap(),
        original
    );
    fixture::independent_pages(&fixture.ffmpeg, &fixture.clip, &fixture.root).await;
    let host = std::sync::Arc::new(host::NextHost::new(
        fixture.pool.clone(),
        fixture.locker.clone(),
        fixture.root.join("playout"),
    ));
    let sidecar = run_case(
        host,
        &fixture.pool,
        (&fixture.ffmpeg, &fixture.ffprobe),
        &fixture.root,
        &fixture.clip,
        Case {
            name: "fresh-source",
            start_ms: 2000,
            end_ms: 9200,
            captions: true,
            callback_required: true,
            caption_track_required: true,
        },
    )
    .await;
    assert!(tokio::fs::read_to_string(sidecar)
        .await
        .unwrap()
        .contains("889 FINAL FRESH"));
    assert_eq!(fixture.locker.active_teletext_work(), 0);
    assert_eq!(
        crate::transcode_capacity::active_transcode_snapshot(1).active,
        0
    );
}
