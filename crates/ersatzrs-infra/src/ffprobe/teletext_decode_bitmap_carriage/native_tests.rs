//! Actual source-bound bitmap preparation/isolation; no AV seek or delivery admission.

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
    SourceTeletextBitmapArtifact, SourceTeletextBitmapArtifactObservation,
    SourceTeletextBitmapFragments, SourceTeletextPageDecodeRequest,
};

struct InitializationCase {
    name: &'static str,
    start: Option<i64>,
    bounds: (i64, i64),
    bootstrap: &'static [(i64, usize, usize)],
    following: &'static [(i64, usize, usize)],
    delay: Option<i64>,
}

const INITIALIZATIONS: [InitializationCase; 5] = [
    InitializationCase {
        name: "before-first",
        start: Some(-61_933),
        bounds: (-61_933, -61_933),
        bootstrap: &[(0, 0, 1)],
        following: &[(0, 1, 0), (24_520_000, 1, 1), (39_520_000, 0, 1)],
        delay: Some(621_933),
    },
    InitializationCase {
        name: "active-A",
        start: Some(17_520_000),
        bounds: (17_520_000, 17_520_000),
        bootstrap: &[(0, 1, 0)],
        following: &[(0, 1, 1), (15_000_000, 0, 1)],
        delay: Some(7_560_000),
    },
    InitializationCase {
        name: "exact-B",
        start: Some(25_080_000),
        bounds: (25_080_000, 25_080_000),
        bootstrap: &[(0, 1, 1)],
        following: &[(0, 0, 1)],
        delay: Some(15_000_000),
    },
    InitializationCase {
        name: "after-clear",
        start: Some(50_000_000),
        bounds: (50_000_000, 50_000_000),
        bootstrap: &[(0, 0, 1)],
        following: &[],
        delay: None,
    },
    InitializationCase {
        name: "selected-AV",
        start: None,
        bounds: (17_458_066, 17_458_068),
        bootstrap: &[(0, 1, 0)],
        following: &[(0, 1, 1), (15_000_000, 0, 1)],
        delay: Some(7_622_000),
    },
];

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

fn groups(observation: &SourceTeletextBitmapArtifactObservation) -> Vec<(i64, usize, usize)> {
    assert!(observation
        .groups()
        .iter()
        .all(|group| group.all_packets_key()));
    observation
        .groups()
        .iter()
        .map(|group| {
            (
                group.pts_microseconds(),
                group.nonempty_frames(),
                group.clear_frames(),
            )
        })
        .collect()
}

async fn retain_fragment(
    directory: &Path,
    name: &str,
    fragment: &super::isolator::OwnedFragment,
) -> serde_json::Value {
    let bytes = tokio::fs::read(fragment.file.path()).await.unwrap();
    assert!(!bytes.is_empty());
    assert!(bytes.len() <= crate::ffprobe::MAX_REMOTE_FFPROBE_OUTPUT_BYTES);
    assert_eq!(fragment.byte_count, bytes.len());
    assert_eq!(fragment.sha256, <[u8; 32]>::from(Sha256::digest(&bytes)));
    tokio::fs::write(directory.join(name), &bytes)
        .await
        .unwrap();
    serde_json::json!({
        "file": name,
        "sha256": hex_text(&fragment.sha256),
        "byte_count": bytes.len(),
        "observed_groups_pts_nonempty_clear": groups(&fragment.observation),
    })
}

async fn retain_initialization(
    proof: &Path,
    case: &InitializationCase,
    fragments: &SourceTeletextBitmapFragments,
) {
    assert_eq!(fragments.start_artifact_microseconds(), case.start);
    assert_eq!(fragments.artifact_instep_bounds_microseconds(), case.bounds);
    assert_eq!(groups(fragments.bootstrap_observation()), case.bootstrap);
    assert_eq!(fragments.following_delay_microseconds(), case.delay);
    assert_eq!(
        fragments
            .following_observation()
            .map(groups)
            .unwrap_or_default(),
        case.following
    );
    let directory = proof.join(case.name);
    tokio::fs::create_dir(&directory).await.unwrap();
    let bootstrap = retain_fragment(&directory, "bootstrap.mks", &fragments.bootstrap).await;
    let following = match &fragments.following {
        Some(fragment) => Some(retain_fragment(&directory, "following.mks", fragment).await),
        None => None,
    };
    // The retained recipe changes only private locators; it is not playback authority.
    let expected_private = ersatzrs_ffmpeg::concat::SourceBitmapConcat::new(
        fragments.bootstrap.file.path(),
        fragments
            .following
            .as_ref()
            .zip(case.delay)
            .map(|(fragment, delay)| (fragment.file.path(), delay)),
    )
    .unwrap()
    .render();
    assert_eq!(fragments.recipe.render(), expected_private);
    let recipe = ersatzrs_ffmpeg::concat::SourceBitmapConcat::new(
        Path::new("bootstrap.mks"),
        case.delay.map(|delay| (Path::new("following.mks"), delay)),
    )
    .unwrap()
    .render();
    assert!(!recipe.contains("inpoint"));
    assert!(!recipe.contains("outpoint"));
    tokio::fs::write(directory.join("bootstrap.ffconcat"), &recipe)
        .await
        .unwrap();
    let metadata = serde_json::json!({
        "probe_version": 1,
        "start_artifact_microseconds": case.start,
        "artifact_instep_bounds_microseconds": case.bounds,
        "following_delay_microseconds": case.delay,
        "source_sha256": hex_text(&fragments.artifact.source.window().decoder_sha256),
        "artifact_sha256": hex_text(fragments.artifact.sha256()),
        "bootstrap": bootstrap,
        "following": following,
        "recipe_sha256": hex_text(&Sha256::digest(recipe.as_bytes())),
        "actual_selected_av_origin_or_host_delivery_claim": false,
        "assembled_timeline_or_pixel_decode_performed_by_this_test": false,
        "private_file_cleanup_requires_native_test_success": true,
    });
    tokio::fs::write(
        directory.join("fragments.json"),
        serde_json::to_vec_pretty(&metadata).unwrap(),
    )
    .await
    .unwrap();
}

async fn assert_owned_files(
    source: Arc<DecodedLocalTeletextPage>,
    artifact: Arc<SourceTeletextBitmapArtifact>,
    fragments: Vec<SourceTeletextBitmapFragments>,
    proof: &Path,
) {
    let source_owner = Arc::downgrade(&source);
    let artifact_owner = Arc::downgrade(&artifact);
    assert!(Arc::ptr_eq(&source, &artifact.source));
    let path = artifact.file.path().to_path_buf();
    let bytes = tokio::fs::read(&path).await.unwrap();
    assert_eq!(artifact.byte_count(), bytes.len());
    assert_eq!(*artifact.sha256(), <[u8; 32]>::from(Sha256::digest(&bytes)));
    retain_proof(proof, &artifact, &bytes).await;
    assert_eq!(fragments.len(), INITIALIZATIONS.len());
    for (case, prepared) in INITIALIZATIONS.iter().zip(&fragments) {
        assert!(Arc::ptr_eq(&artifact, &prepared.artifact));
        assert_eq!(prepared.request(), source.request());
        retain_initialization(proof, case, prepared).await;
    }
    drop(source);
    drop(artifact);
    assert!(source_owner.upgrade().is_some());
    assert!(artifact_owner.upgrade().is_some());
    assert_eq!(tokio::fs::read(&path).await.unwrap(), bytes);
    for prepared in fragments {
        let paths: Vec<_> = std::iter::once(&prepared.bootstrap)
            .chain(prepared.following.as_ref())
            .map(|fragment| fragment.file.path().to_path_buf())
            .collect();
        let prepared = Arc::new(prepared);
        let retained = prepared.clone();
        drop(prepared);
        for fragment_path in &paths {
            assert!(tokio::fs::metadata(fragment_path).await.unwrap().is_file());
        }
        drop(retained);
        for fragment_path in paths {
            assert_eq!(
                tokio::fs::metadata(fragment_path).await.unwrap_err().kind(),
                std::io::ErrorKind::NotFound
            );
        }
    }
    assert!(source_owner.upgrade().is_none());
    assert!(artifact_owner.upgrade().is_none());
    assert_eq!(
        tokio::fs::metadata(&path).await.unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
}

async fn assert_clock_projection(
    proof: &Path,
    artifact: &SourceTeletextBitmapArtifact,
    av: &crate::ffprobe::teletext_consumer_origin::NativeSourceConsumerOrigin,
) {
    use crate::ffprobe::teletext_consumer_alignment::project_bitmap_consumer_origin;
    assert_eq!(
        av.streams()
            .iter()
            .map(|stream| (stream.index(), stream.pid()))
            .collect::<Vec<_>>(),
        [(0, 256), (3, 259)]
    );
    let positions: Vec<_> = artifact
        .observation()
        .groups()
        .iter()
        .map(|group| group.pts_microseconds())
        .collect();
    let projected = project_bitmap_consumer_origin(
        av,
        artifact.caption_origin(),
        artifact.caption_origin_microseconds(),
        &positions,
        17_520_000,
    )
    .expect("selected native video and alternate audio retain a correlated caption clock");
    assert_eq!(
        projected.artifact_instep_bounds_microseconds(),
        (17_458_066, 17_458_068)
    );
    assert_eq!(projected.active_group_index(), Some(0));
    assert_eq!(projected.following_group_index(), Some(1));
    assert_eq!(projected.following_delay_microseconds(), Some(7_622_000));
    assert_eq!(
        projected.following_delay_residual_bounds_microseconds(),
        Some((66, 68))
    );
    let metadata = serde_json::json!({
        "probe_version": 1,
        "source_sha256": hex_text(&artifact.source.window().decoder_sha256),
        "observed_enabled_indices": [0, 3],
        "observed_enabled_pids": [256, 259],
        "alternate_audio_language": "nld",
        "hypothetical_effective_seek_microseconds": 17_520_000,
        "caption_origin_microseconds": artifact.caption_origin_microseconds(),
        "artifact_instep_bounds_microseconds": projected.artifact_instep_bounds_microseconds(),
        "active_group_index": projected.active_group_index(),
        "following_group_index": projected.following_group_index(),
        "following_delay_microseconds": projected.following_delay_microseconds(),
        "following_delay_residual_bounds_microseconds": projected.following_delay_residual_bounds_microseconds(),
        "final_command_or_actual_consumed_seek_or_delivery_claim": false,
        "note": "The measured AV origin is projected without choosing a fictitious exact artifact instep. Four fragment cases use declared artifact coordinates; the fifth independently observes and projects the selected AV streams inside the owned isolator.",
    });
    tokio::fs::write(
        proof.join("selected-av-projection.json"),
        serde_json::to_vec_pretty(&metadata).unwrap(),
    )
    .await
    .unwrap();
}

async fn acquire_request(
    scope: &LocalProbeCleanupScope,
    extractor: &FfprobeExtractor,
    path: &Path,
) -> SourceTeletextPageDecodeRequest {
    let acquired = scope
        .scope(extractor.extract_local_with_teletext(path))
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
    let audio = acquired
        .media
        .streams
        .iter()
        .find(|stream| stream.index == 3)
        .unwrap();
    assert_eq!(audio.language.as_deref(), Some("nld"));
    SourceTeletextPageDecodeRequest {
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
    }
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
    let request = acquire_request(&scope, &extractor, &path).await;
    let locker = EntityLocker::new();
    let work = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
    let hold = work.hold_work();
    drop(work);
    assert_eq!(locker.active_teletext_work(), 1);
    assert!(TeletextWorkGuard::try_acquire(&locker, 1).is_none());
    let decode_path = path.clone();
    let decode_request = request.clone();
    let result = scope
        .scope(async move {
            let deadline = Instant::now() + Duration::from_secs(15);
            let source = Arc::new(
                extractor
                    .decode_local_teletext_page(
                        decode_request,
                        decode_path.clone(),
                        decode_path,
                        deadline,
                        hold.clone(),
                    )
                    .await?,
            );
            let artifact = Arc::new(
                extractor
                    .prepare_local_teletext_bitmap_artifact(source.clone(), deadline, hold.clone())
                    .await?,
            );
            let av = extractor
                .observe_local_teletext_consumer_origin(&source, &[0, 3], deadline, hold.clone())
                .await?;
            let mut fragments = Vec::new();
            for case in &INITIALIZATIONS {
                let prepared = if let Some(start) = case.start {
                    extractor
                        .isolate_local_teletext_bitmap_artifact(
                            artifact.clone(),
                            start,
                            deadline,
                            hold.clone(),
                        )
                        .await?
                } else {
                    extractor
                        .isolate_local_teletext_bitmap_artifact_for_consumer(
                            artifact.clone(),
                            &[0, 3],
                            17_520_000,
                            deadline,
                            hold.clone(),
                        )
                        .await?
                };
                fragments.push(prepared);
            }
            Ok::<_, super::SourceTeletextPageDecodeError>((source, artifact, fragments, av))
        })
        .await;
    scope.close();
    assert_eq!(scope.wait_cleanup().await, Ok(()));
    assert_eq!(locker.active_teletext_work(), 0);
    let (source, artifact, fragments, av) =
        result.expect("actual source-bound finite bitmap artifact preparation");
    assert_eq!(source.request(), &request);
    assert_eq!(
        source.window().decoder_sha256,
        <[u8; 32]>::from(Sha256::digest(&original))
    );
    assert_observation(&source, &artifact);
    assert_clock_projection(&proof, &artifact, &av).await;
    assert_owned_files(source, artifact, fragments, &proof).await;
    assert_eq!(tokio::fs::read(&path).await.unwrap(), original);
    eprintln!("native finite bitmap artifact: exact source request/digest, two source bitmap events, three artifact groups, two clears, private file retained through last owner then removed; preparation work drained; four exact artifact-coordinate initializers and one independently projected selected-AV initializer, caller guard dropped with owned work retained, no AV seek or playback admission");
}
