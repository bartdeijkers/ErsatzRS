//! Source-bound finite frame-end evidence; this does not run a Next successor.

use super::*;
use crate::ffprobe::{FfprobeExtractor, LocalProbeCleanupScope};
use ersatzrs_core::locking::TeletextWorkGuard;
use std::sync::Arc;

#[tokio::test]
#[ignore = "requires pinned FFmpeg9/FFprobe9, Python3 and fresh proof directory; ERSATZRS_NEXT_MAPPED_SOURCE optionally supplies the exact incident fixture"]
async fn finite_video_cut_qualifies_actual_frames_and_refuses_unproven_eof_and_stale_source() {
    let root = fixture::proof_root();
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let clip = fixture::finite_video_cut_source(&ffmpeg, &root).await;
    let (pool, locker) = prepare_schedule(&clip, false, &text_profile()).await;
    fixture::acquire_and_store(&pool, &ffprobe, &clip, &root).await;
    let candidate = crate::teletext_source::discover_caption_requests(
        &pool,
        10,
        100,
        1,
        clip.to_str().unwrap(),
    )
    .await
    .unwrap()
    .into_iter()
    .map(|request| request.candidate)
    .find(|candidate| candidate.request.page.get() == 889)
    .unwrap();
    let extractor = FfprobeExtractor::new(ffprobe.to_str().unwrap());
    let work = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
    let scope = LocalProbeCleanupScope::new();
    let deadline = Instant::now() + Duration::from_secs(15);
    let source = scope
        .scope(extractor.prepare_local_teletext_source(
            candidate.request.clone(),
            clip.clone(),
            clip.clone(),
            deadline,
            work.hold_work(),
        ))
        .await
        .map(Arc::new)
        .unwrap();
    let cut = scope
        .scope(extractor.prepare_local_teletext_video_cut(
            source.clone(),
            0,
            9200,
            deadline,
            work.hold_work(),
        ))
        .await
        .unwrap()
        .expect("literal fixture contains complete and excluded frames around the outpoint");
    assert!(Arc::ptr_eq(cut.source(), &source));
    assert_eq!(cut.source().request(), &candidate.request);
    assert_eq!(cut.psi_fingerprint(), candidate.psi_fingerprint);
    assert_eq!(cut.stream_index(), 0);
    assert_eq!(cut.out_ms(), 9200);
    // Independent full decoded fixture: pts901890+duration3600-origin77544
    // ends827946<=828000; pts905490+duration3600-origin77544 ends831546>828000.
    // The excluded frame starts827946. No nominal25fps inference is used.
    assert_eq!(cut.cut_pts_90k(), 827_946);
    Box::pin(assert_refusals(
        &extractor, &source, &scope, &work, deadline,
    ))
    .await;
    tokio::fs::OpenOptions::new()
        .append(true)
        .open(&clip)
        .await
        .unwrap()
        .set_len(tokio::fs::metadata(&clip).await.unwrap().len() + 1)
        .await
        .unwrap();
    assert!(scope
        .scope(source.check_source(deadline, work.hold_work()))
        .await
        .is_err());
    scope.close();
    scope.wait_cleanup().await.unwrap();
    drop(work);
    assert_eq!(locker.active_teletext_work(), 0);
    write_evidence(&root, cut.cut_pts_90k()).await;
}

async fn assert_refusals(
    extractor: &FfprobeExtractor,
    source: &Arc<crate::ffprobe::teletext_decode::PreparedLocalTeletextSource>,
    scope: &LocalProbeCleanupScope,
    work: &TeletextWorkGuard,
    deadline: Instant,
) {
    assert!(
        scope
            .scope(extractor.prepare_local_teletext_video_cut(
                source.clone(),
                0,
                20_559,
                deadline,
                work.hold_work(),
            ))
            .await
            .unwrap()
            .is_none(),
        "decoder success with no following witness does not certify natural EOF"
    );
    assert!(
        scope
            .scope(extractor.prepare_local_teletext_video_cut(
                source.clone(),
                1,
                9200,
                deadline,
                work.hold_work(),
            ))
            .await
            .is_err(),
        "an audio stream cannot supply a video endpoint"
    );
    assert!(
        extractor
            .prepare_local_teletext_video_cut(source.clone(), 0, 9200, deadline, work.hold_work())
            .await
            .is_err(),
        "no cleanup scope may launch a worker"
    );
    assert!(scope
        .scope(extractor.prepare_local_teletext_video_cut(
            source.clone(),
            0,
            9200,
            Instant::now(),
            work.hold_work(),
        ))
        .await
        .is_err());
}

async fn write_evidence(root: &Path, cut_pts: i64) {
    tokio::fs::write(
        root.join("finite-video-cut.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "selected_video":0,"program":1,"source_out_ms":9200,
            "last_complete_source_end_ticks":827_946,"source_out_ticks":828_000,
            "first_excluded_source_start_ticks":cut_pts,
            "first_excluded_source_end_ticks":831_546,
            "held_source_identity_and_psi_match":true,
            "unproven_eof_refused":true,"audio_as_video_refused":true,
            "missing_scope_refused":true,"expired_deadline_refused":true,
            "stale_original_refused":true,"joined_cleanup_active_work":0,
            "next_successor_or_public_stream_executed":false,
        }))
        .unwrap(),
    )
    .await
    .unwrap();
}
