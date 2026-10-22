//! Negotiated occurrence evidence stays separate from the complete SRT cache.

use super::*;
use crate::next_engine::{protocol, SourceVideoCutSupport};

#[tokio::test]
#[ignore = "requires FFmpeg9, Python3 on Unix, exact ERSATZRS_NEXT_MAPPED_SOURCE and fresh contained proof directory"]
async fn finite_video_cut_preparation_preserves_srt_cache_and_installation_fences() {
    let root = fixture::proof_root();
    let fixture = Fixture::new_with_scenario(&root, CaptionScenario::Incident).await;
    set_case(&fixture, 0, 9200).await;
    let before = fixture.snapshot().await.unwrap();
    let first = prepare_caption(
        &fixture,
        &before,
        &fixture.ffmpeg,
        SourceVideoCutSupport::Version1,
    )
    .await;
    let path = first.path.clone();
    let content = tokio::fs::read(&path).await.unwrap();
    assert!(std::str::from_utf8(&content).unwrap().contains("889 FINAL"));
    let entries = cache_entries(&root).await;
    assert!(entries.contains_key(&format!("{}.srt", before.identity.cache_key)));
    assert_installation_fences(&fixture, &before, &first).await;
    drop(first);
    assert_eq!(fixture.locker.active_teletext_work(), 0);

    let missing_decoder = root.join("must-not-launch-ffmpeg-on-srt-hit");
    assert!(!missing_decoder.exists());
    let hit = prepare_caption(
        &fixture,
        &before,
        &missing_decoder,
        SourceVideoCutSupport::Version1,
    )
    .await;
    assert_eq!(hit.path, path);
    assert_eq!(tokio::fs::read(&hit.path).await.unwrap(), content);
    assert_installation_fences(&fixture, &before, &hit).await;
    assert_eq!(cache_entries(&root).await, entries);
    drop(hit);
    assert_eq!(fixture.locker.active_teletext_work(), 0);

    #[cfg(unix)]
    assert_optional_decoder_failure(&fixture, &before, &missing_decoder, &path, &root).await;
    assert_eq!(cache_entries(&root).await, entries);
    assert_non_cut_paths(&fixture, &before, &missing_decoder, &path).await;
    assert_eq!(cache_entries(&root).await, entries);
    assert_stale_source(&fixture, &missing_decoder).await;
    assert_eq!(cache_entries(&root).await, entries);
    assert_eq!(tokio::fs::read(&path).await.unwrap(), content);
    assert_eq!(fixture.locker.active_teletext_work(), 0);
    tokio::fs::write(
        root.join("negotiated-video-cut-cache.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "srt_cache_miss_and_hit_cut_pts_90k":827_946,"out_point_ms":9200,
            "cache_hit_without_ffmpeg":true,"same_cache_key_path_content_and_two_files":true,
            "selected_index_path_out_and_already_installed_mismatch_refused":true,
            "admission_held_until_caption_drop":true,
            "unavailable_convert_and_unqualified_eof_keep_srt_without_cut":true,
            "optional_cut_decoder_failure_preserves_srt_unix_only":cfg!(unix),
            "held_source_change_and_subsequent_preparation_refused":true,
            "final_teletext_admission":0,"next_process_or_http_executed":false,
        }))
        .unwrap(),
    )
    .await
    .unwrap();
}

// The executable gate uses Unix permissions, as the existing in-flight source
// replacement fixture does. All other assertions above remain cross-platform.
#[cfg(unix)]
async fn assert_optional_decoder_failure(
    fixture: &Fixture,
    before: &dynamic_projection::Snapshot,
    decoder: &Path,
    path: &Path,
    root: &Path,
) {
    use std::os::unix::fs::PermissionsExt as _;

    let script = root.join("finite-video-query-refusal.py");
    let entered = root.join("finite-video-query-refusal.json");
    let real = serde_json::to_string(fixture.ffprobe.to_str().unwrap()).unwrap();
    let marker = serde_json::to_string(entered.to_str().unwrap()).unwrap();
    let program = format!(
        r"#!/usr/bin/env python3
import json, os, sys
real = {real}
args = sys.argv[1:]
if (args[:2] == ['-v', 'error'] and '-count_packets' in args and
        '-show_frames' in args and '-read_intervals' in args and
        args[args.index('-read_intervals') + 1] == '%+#4096'):
    with open({marker}, 'x', encoding='utf-8') as receipt:
        json.dump(args, receipt)
    sys.stderr.write('Bounded finite-frame qualification diagnostic\n')
    raise SystemExit(0)
os.execv(real, [real] + args)
"
    );
    tokio::fs::write(&script, program).await.unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    let prepared = fixture
        .prepare_with_video_cut_and_probe(
            before,
            decoder,
            &script,
            SourceVideoCutSupport::Version1,
            || async {
                fixture.snapshot().await.is_ok_and(|current| {
                    current.identity.descriptor == before.identity.descriptor
                        && current.identity.cache_key == before.identity.cache_key
                })
            },
        )
        .await
        .expect("optional cutoff diagnostic must preserve the validated cached SRT");
    let source_teletext::PreparedText::Ready(caption) = prepared else {
        panic!("optional frame observation cannot turn a transmitted page into absence");
    };
    assert_eq!(caption.path, path);
    assert!(caption.video_cut.is_none());
    assert_eq!(fixture.locker.active_teletext_work(), 0);
    let args: Vec<String> =
        serde_json::from_slice(&tokio::fs::read(entered).await.unwrap()).unwrap();
    assert!(args
        .windows(2)
        .any(|pair| pair == ["-read_intervals", "%+#4096"]));
    assert!(args.iter().any(|arg| arg == "-count_packets"));
    assert!(args.iter().any(|arg| arg == "-show_frames"));
}

async fn set_case(fixture: &Fixture, mode: i64, out_ms: i64) {
    sqlx::query(r#"UPDATE "Channel" SET "NextEngineTextSubtitleMode"=? WHERE "Id"=1"#)
        .bind(mode)
        .execute(&fixture.pool)
        .await
        .unwrap();
    let mut item = selection::tests::item();
    item.id = 1;
    item.playout_id = 1;
    item.start = fixture.start;
    item.finish = item.start + chrono::Duration::seconds(20);
    item.in_point = chrono::Duration::zero();
    item.out_point = chrono::Duration::milliseconds(out_ms);
    crate::repositories::make_repositories(fixture.pool.clone())
        .playout
        .upsert_item(&item)
        .await
        .unwrap();
}

async fn prepare_caption(
    fixture: &Fixture,
    before: &dynamic_projection::Snapshot,
    decoder: &Path,
    support: SourceVideoCutSupport,
) -> source_teletext::PreparedCaption {
    let prepared = fixture
        .prepare_with_video_cut(before, decoder, support, || async {
            fixture.snapshot().await.is_ok_and(|current| {
                current.identity.descriptor == before.identity.descriptor
                    && current.identity.cache_key == before.identity.cache_key
            })
        })
        .await
        .unwrap();
    let source_teletext::PreparedText::Ready(caption) = prepared else {
        panic!("the incident source transmits the selected page");
    };
    caption
}

fn selected_hint(item: &mut protocol::PlayoutItem) -> &mut protocol::VideoProbeHint {
    let video = item.tracks.as_mut().unwrap().video.as_mut().unwrap();
    let Some(protocol::Source::Local {
        probe_hint: Some(hint),
        ..
    }) = &mut video.source
    else {
        panic!("the fixture must keep the actual local video route");
    };
    assert_eq!(hint.video.len(), 1);
    &mut hint.video[0]
}

async fn assert_installation_fences(
    fixture: &Fixture,
    before: &dynamic_projection::Snapshot,
    caption: &source_teletext::PreparedCaption,
) {
    let cut = caption.video_cut.as_ref().expect("negotiated Burn cut");
    assert_eq!(fixture.locker.active_teletext_work(), 1);
    cut.check_source().await.unwrap();
    let mut item = before.projection.item.clone();
    cut.install(&mut item).unwrap();
    let hint = selected_hint(&mut item).source_frame_cut.as_ref().unwrap();
    assert_eq!(hint.version, 1);
    assert_eq!(hint.out_point_ms, 9200);
    assert_eq!(hint.first_excluded_pts_90k, 827_946);
    let installed = serde_json::to_value(&item).unwrap();
    assert!(cut.install(&mut item).is_err());
    assert_eq!(serde_json::to_value(&item).unwrap(), installed);
    selected_hint(&mut item).source_frame_cut = None;
    assert_eq!(
        serde_json::to_value(item).unwrap(),
        serde_json::to_value(&before.projection.item).unwrap()
    );
    for scenario in 0..3 {
        let mut item = before.projection.item.clone();
        let video = item.tracks.as_mut().unwrap().video.as_mut().unwrap();
        if scenario == 0 {
            video.stream_index = Some(1);
        } else {
            let Some(protocol::Source::Local {
                path, out_point_ms, ..
            }) = &mut video.source
            else {
                unreachable!();
            };
            if scenario == 1 {
                path.push_str(".different");
            } else {
                *out_point_ms = Some(9201);
            }
        }
        let unchanged = serde_json::to_value(&item).unwrap();
        assert!(cut.install(&mut item).is_err(), "scenario {scenario}");
        assert_eq!(serde_json::to_value(item).unwrap(), unchanged);
        assert_eq!(fixture.locker.active_teletext_work(), 1);
    }
}

async fn assert_non_cut_paths(
    fixture: &Fixture,
    burn: &dynamic_projection::Snapshot,
    decoder: &Path,
    path: &Path,
) {
    let unsupported =
        prepare_caption(fixture, burn, decoder, SourceVideoCutSupport::Unavailable).await;
    assert_eq!(unsupported.path, path);
    assert!(unsupported.video_cut.is_none());
    drop(unsupported);
    assert_eq!(fixture.locker.active_teletext_work(), 0);
    for (mode, out_ms) in [(1, 9200), (0, 20_559)] {
        set_case(fixture, mode, out_ms).await;
        let current = fixture.snapshot().await.unwrap();
        assert_eq!(current.identity.cache_key, burn.identity.cache_key);
        let caption =
            prepare_caption(fixture, &current, decoder, SourceVideoCutSupport::Version1).await;
        assert_eq!(caption.path, path);
        assert!(caption.video_cut.is_none(), "mode={mode}, out={out_ms}");
        drop(caption);
        assert_eq!(fixture.locker.active_teletext_work(), 0);
    }
}

async fn assert_stale_source(fixture: &Fixture, decoder: &Path) {
    set_case(fixture, 0, 9200).await;
    let before = fixture.snapshot().await.unwrap();
    let caption = prepare_caption(fixture, &before, decoder, SourceVideoCutSupport::Version1).await;
    let cut = caption.video_cut.as_ref().unwrap();
    cut.check_source().await.unwrap();
    std::fs::OpenOptions::new()
        .append(true)
        .open(&fixture.clip)
        .unwrap()
        .write_all(&[0x47])
        .unwrap();
    assert!(cut.check_source().await.is_err());
    assert_eq!(fixture.locker.active_teletext_work(), 1);
    drop(caption);
    assert_eq!(fixture.locker.active_teletext_work(), 0);
    let callbacks = AtomicUsize::new(0);
    assert!(fixture
        .prepare_with_video_cut(
            &before,
            decoder,
            SourceVideoCutSupport::Version1,
            || async {
                callbacks.fetch_add(1, Ordering::SeqCst);
                true
            }
        )
        .await
        .is_err());
    assert_eq!(callbacks.load(Ordering::SeqCst), 0);
}
