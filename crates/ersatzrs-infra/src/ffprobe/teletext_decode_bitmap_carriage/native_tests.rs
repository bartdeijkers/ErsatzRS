//! Actual source-bound finite bitmap preparation; no seek or delivery admission.

use std::{
    fmt::Write as _,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use ersatzrs_core::locking::{EntityLocker, TeletextWorkGuard};
use ersatzrs_domain::{MediaSourceKind, SourceTeletextBindingIdentity, TeletextPageNumber};
use sha2::{Digest as _, Sha256};
use tokio::time::Instant;

use super::{
    DecodedLocalTeletextPage, FfprobeExtractor, LocalProbeCleanupScope,
    SourceTeletextBitmapArtifact, SourceTeletextPageDecodeRequest,
};

#[allow(
    clippy::disallowed_methods,
    reason = "Explicit opt-in synthetic native fixture path, never product configuration"
)]
fn selected_path(variable: &str) -> PathBuf {
    let path = PathBuf::from(
        std::env::var(variable).expect("set explicit native proof input/output path"),
    );
    assert!(path.is_absolute());
    path
}

fn hex_text(bytes: &[u8]) -> String {
    let mut value = String::new();
    for byte in bytes {
        write!(value, "{byte:02x}").unwrap();
    }
    value
}

async fn retain_proof(proof: &Path, artifact: &SourceTeletextBitmapArtifact, bytes: &[u8]) {
    let groups = artifact.observation().groups();
    tokio::fs::write(proof.join("native-artifact.mks"), bytes)
        .await
        .unwrap();
    let metadata = serde_json::json!({
        "probe_version":1,
        "source_sha256":hex_text(&artifact.source.window().decoder_sha256),
        "artifact_sha256":hex_text(artifact.sha256()),
        "byte_count":artifact.byte_count(),
        "source_bitmap_count":artifact.source.events().count,
        "stream_index":artifact.request().stream_index,
        "pid":artifact.request().pid,
        "program_number":artifact.request().program_number,
        "page":artifact.request().page.get(),
        "caption_origin_microseconds":artifact.caption_origin_microseconds(),
        "groups":groups.iter().map(|group| serde_json::json!({
            "pts_microseconds":group.pts_microseconds(),
            "nonempty_frames":group.nonempty_frames(),
            "clear_frames":group.clear_frames(),
            "all_packets_key":group.all_packets_key(),
        })).collect::<Vec<_>>(),
        "private_file_cleanup_requires_native_test_success":true,
        "actual_selected_av_origin_or_host_delivery_claim":false,
    });
    tokio::fs::write(
        proof.join("native-artifact.json"),
        serde_json::to_vec_pretty(&metadata).unwrap(),
    )
    .await
    .unwrap();
}

fn assert_observation(source: &DecodedLocalTeletextPage, artifact: &SourceTeletextBitmapArtifact) {
    assert_eq!(source.events().count, 2);
    assert_eq!(source.events().first_pts_microseconds, Some(2_040_600));
    assert_eq!(source.events().last_pts_microseconds, Some(26_560_600));
    assert!(source.window().covers_entire_source_bytes());
    assert_eq!(artifact.request(), source.request());
    assert_eq!(artifact.caption_origin_microseconds(), 1_480_600);
    let streams = artifact.caption_origin().streams();
    assert_eq!(streams.len(), 1);
    assert_eq!((streams[0].index(), streams[0].pid()), (2, 258));
    let observed = artifact.observation();
    assert!(observed
        .groups()
        .iter()
        .all(|group| group.all_packets_key()));
    assert_eq!(
        (observed.nonempty_frames(), observed.clear_frames()),
        (2, 2)
    );
    assert_eq!(
        observed
            .groups()
            .iter()
            .map(|group| (
                group.pts_microseconds(),
                group.nonempty_frames(),
                group.clear_frames()
            ))
            .collect::<Vec<_>>(),
        [(560_000, 1, 0), (25_080_000, 1, 1), (40_080_000, 0, 1)]
    );
    // Container packet end includes the clear packet's duration, not visible cue expiry.
    assert_eq!(observed.last_packet_end_microseconds(), 55_080_000);
}

async fn assert_owned_file(
    source: Arc<DecodedLocalTeletextPage>,
    artifact: SourceTeletextBitmapArtifact,
    proof: &Path,
) {
    let source_owner = Arc::downgrade(&source);
    assert!(Arc::ptr_eq(&source, &artifact.source));
    let SourceTeletextBitmapArtifact {
        _file: retained_file,
        ..
    } = &artifact;
    let path = retained_file.path().to_path_buf();
    let bytes = tokio::fs::read(&path).await.unwrap();
    assert!(!bytes.is_empty());
    assert!(bytes.len() <= crate::ffprobe::MAX_REMOTE_FFPROBE_OUTPUT_BYTES);
    assert_eq!(artifact.byte_count(), bytes.len());
    assert_eq!(*artifact.sha256(), <[u8; 32]>::from(Sha256::digest(&bytes)));
    retain_proof(proof, &artifact, &bytes).await;
    let artifact = Arc::new(artifact);
    let retained = artifact.clone();
    drop(source);
    drop(artifact);
    assert!(source_owner.upgrade().is_some());
    assert_eq!(tokio::fs::read(&path).await.unwrap(), bytes);
    drop(retained);
    assert!(source_owner.upgrade().is_none());
    assert_eq!(
        tokio::fs::metadata(&path).await.unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
}

#[tokio::test]
#[ignore = "requires explicit FFmpeg/ffprobe9 and the qualified finite-caption source"]
async fn native_prepared_bitmap_artifact_retains_exact_source_events_and_owned_file() {
    let path = selected_path("ERSATZRS_TELETEXT_BITMAP_ARTIFACT_SOURCE");
    let proof = selected_path("ERSATZRS_TELETEXT_BITMAP_ARTIFACT_PROOF_DIR");
    tokio::fs::create_dir(&proof)
        .await
        .expect("fresh owned native artifact proof root");
    let original = tokio::fs::read(&path).await.unwrap();
    assert_eq!(
        hex_text(&Sha256::digest(&original)),
        "b6ffa783e1cc96926a846b73fa8182755a11555296294ef9a740c09d9245c560"
    );
    assert!(std::path::Path::new(&ersatzrs_ffmpeg::ffmpeg_path()).is_absolute());
    let extractor = FfprobeExtractor::new(ersatzrs_ffmpeg::ffprobe_path());
    let scope = LocalProbeCleanupScope::new();
    let acquired = scope
        .scope(extractor.extract_local_with_teletext(&path))
        .await;
    if acquired.is_err() {
        scope.close();
        assert_eq!(scope.wait_cleanup().await, Ok(()));
    }
    let acquired = acquired.expect("actual native source acquisition");
    let subtitle = acquired
        .media
        .streams
        .iter()
        .find(|stream| stream.index == 2)
        .unwrap();
    assert_eq!(subtitle.codec.as_deref(), Some("dvb_teletext"));
    let request = SourceTeletextPageDecodeRequest {
        binding: SourceTeletextBindingIdentity::new(
            1,
            MediaSourceKind::Local,
            2,
            3,
            4,
            acquired.teletext.unwrap().revision().clone(),
        )
        .unwrap(),
        stream_index: subtitle.index,
        pid: 258,
        program_number: 1,
        page: TeletextPageNumber::new(888).unwrap(),
    };
    let locker = EntityLocker::new();
    let work = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
    let result = scope
        .scope(async {
            let deadline = Instant::now() + Duration::from_secs(15);
            let source = Arc::new(
                extractor
                    .decode_local_teletext_page(
                        request.clone(),
                        path.clone(),
                        path.clone(),
                        deadline,
                        work.hold_work(),
                    )
                    .await?,
            );
            let artifact = extractor
                .prepare_local_teletext_bitmap_artifact(source.clone(), deadline, work.hold_work())
                .await?;
            Ok::<_, super::SourceTeletextPageDecodeError>((source, artifact))
        })
        .await;
    scope.close();
    assert_eq!(scope.wait_cleanup().await, Ok(()));
    drop(work);
    assert_eq!(locker.active_teletext_work(), 0);
    let (source, artifact) =
        result.expect("actual source-bound finite bitmap artifact preparation");
    assert_eq!(source.request(), &request);
    assert_eq!(
        source.window().decoder_sha256,
        <[u8; 32]>::from(Sha256::digest(&original))
    );
    assert_observation(&source, &artifact);
    assert_owned_file(source, artifact, &proof).await;
    assert_eq!(tokio::fs::read(&path).await.unwrap(), original);
    eprintln!("native finite bitmap artifact: exact source request/digest, two source bitmap events, three artifact groups, two clears, private file retained through last owner then removed; preparation work drained; no seek or playback admission");
}
