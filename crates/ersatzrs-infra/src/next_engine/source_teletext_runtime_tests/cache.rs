//! Actual finite-page cache hits retain source and current SQL qualification.

use super::*;
use crate::{
    ffprobe::{FfprobeExtractor, LocalProbeCleanupScope},
    next_engine::{burn_cache::BurnCache, dynamic_projection, media::MediaRelay, source_teletext},
};
use ersatzrs_core::locking::EntityLocker;
use std::{
    collections::BTreeMap,
    future::Future,
    io::Write as _,
    sync::atomic::{AtomicUsize, Ordering},
};

struct Fixture {
    pool: crate::db::DbPool,
    locker: EntityLocker,
    relay: MediaRelay,
    cache: BurnCache,
    start: chrono::DateTime<chrono::Utc>,
    ffmpeg: PathBuf,
    ffprobe: PathBuf,
    clip: PathBuf,
}

enum CaptionScenario {
    Regular,
    Late,
    Untransmitted,
    Incident,
}

#[path = "cache/video_cut.rs"]
mod video_cut;

pub(super) async fn assert_complete_text_preparation(
    pool: &crate::db::DbPool,
    locker: &EntityLocker,
    ffmpeg: &Path,
    ffprobe: &Path,
    clip: &Path,
) {
    let text = prepare_complete_text(pool, locker, ffmpeg, ffprobe, clip)
        .await
        .unwrap_or_else(|error| panic!("complete selected-page preparation: {error:?}"));
    assert!(text.contains("NEDERLANDSE PAGINA 889 FINAL"));
    assert!(!text.contains("ENGLISH PAGE 888"));
}

pub(super) async fn prepare_complete_text(
    pool: &crate::db::DbPool,
    locker: &EntityLocker,
    ffmpeg: &Path,
    ffprobe: &Path,
    clip: &Path,
) -> Result<String, crate::ffprobe::teletext_decode::SourceTeletextPageDecodeError> {
    let candidate =
        crate::teletext_source::discover_caption_requests(pool, 10, 100, 1, clip.to_str().unwrap())
            .await
            .unwrap()
            .into_iter()
            .map(|request| request.candidate)
            .find(|candidate| candidate.request.page.get() == 889)
            .unwrap();
    let work = ersatzrs_core::locking::TeletextWorkGuard::try_acquire(locker, 1).unwrap();
    let scope = LocalProbeCleanupScope::new();
    let extractor = FfprobeExtractor::new(ffprobe.to_str().unwrap());
    let deadline = Instant::now() + Duration::from_secs(15);
    let result = scope
        .scope(async {
            let source = extractor
                .prepare_local_teletext_source(
                    candidate.request,
                    clip.into(),
                    clip.into(),
                    deadline,
                    work.hold_work(),
                )
                .await?;
            extractor
                .prepare_local_teletext_text(
                    std::sync::Arc::new(source),
                    ffmpeg,
                    Some(0),
                    deadline,
                    work.hold_work(),
                )
                .await
        })
        .await;
    scope.close();
    scope.wait_cleanup().await.unwrap();
    drop(work);
    assert_eq!(locker.active_teletext_work(), 0);
    result.and_then(|prepared| match prepared {
        crate::ffprobe::teletext_decode::SourceTeletextTextPreparation::Ready(artifact) => {
            Ok(artifact.content)
        }
        crate::ffprobe::teletext_decode::SourceTeletextTextPreparation::Absent {
            psi_fingerprint,
            ..
        } => {
            assert_eq!(psi_fingerprint, candidate.psi_fingerprint);
            Err(crate::ffprobe::teletext_decode::SourceTeletextPageDecodeError::UnobservedPage)
        }
    })
}

impl Fixture {
    async fn new(root: &Path) -> Self {
        Self::new_with_caption_timing(root, false).await
    }

    async fn new_with_caption_timing(root: &Path, late: bool) -> Self {
        Self::new_with_scenario(
            root,
            if late {
                CaptionScenario::Late
            } else {
                CaptionScenario::Regular
            },
        )
        .await
    }

    async fn new_with_scenario(root: &Path, scenario: CaptionScenario) -> Self {
        let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
        let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
        let clip = match scenario {
            CaptionScenario::Regular => fixture::source(&ffmpeg, root).await,
            CaptionScenario::Late => fixture::source_with_late_caption(&ffmpeg, root).await,
            CaptionScenario::Untransmitted => {
                fixture::source_with_untransmitted_caption(&ffmpeg, root).await
            }
            CaptionScenario::Incident => fixture::finite_video_cut_source(&ffmpeg, root).await,
        };
        let (pool, locker) = prepare_schedule(&clip, false, &text_profile()).await;
        if matches!(scenario, CaptionScenario::Late) {
            fixture::independent_pages(&ffmpeg, &clip, root).await;
        }
        if matches!(
            scenario,
            CaptionScenario::Late | CaptionScenario::Untransmitted
        ) {
            fixture::acquire_and_store_observed(&pool, &ffprobe, &clip, root, &[888]).await;
        } else {
            fixture::acquire_and_store(&pool, &ffprobe, &clip, root).await;
        }
        sqlx::query(
            r#"UPDATE "Channel" SET "SubtitleMode"=3,"NextEngineTextSubtitleMode"=1 WHERE "Id"=1"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        let mut item = selection::tests::item();
        item.id = 1;
        item.playout_id = 1;
        item.start = chrono::Utc::now();
        item.finish = item.start + chrono::Duration::seconds(20);
        item.in_point = chrono::Duration::zero();
        item.out_point = chrono::Duration::seconds(20);
        crate::repositories::make_repositories(pool.clone())
            .playout
            .upsert_item(&item)
            .await
            .unwrap();
        Self {
            pool,
            locker,
            relay: MediaRelay::new(),
            cache: BurnCache::new(root.join("cache")),
            start: item.start,
            ffmpeg,
            ffprobe,
            clip,
        }
    }

    async fn snapshot(&self) -> ersatzrs_core::repositories::Result<dynamic_projection::Snapshot> {
        dynamic_projection::project_snapshot(
            &self.pool,
            &self.locker,
            &self.relay,
            1,
            1,
            self.start,
            self.start + chrono::Duration::seconds(20),
            &[],
            Some(3),
        )
        .await
    }

    async fn prepare(
        &self,
        before: &dynamic_projection::Snapshot,
        decoder: &Path,
        replace_revision: Option<&str>,
        callbacks: &AtomicUsize,
    ) -> ersatzrs_core::repositories::Result<PathBuf> {
        self.prepare_with_validation(before, decoder, || async {
            callbacks.fetch_add(1, Ordering::SeqCst);
            if let Some(revision) = replace_revision {
                sqlx::query(r#"UPDATE "SourceTeletextMetadata" SET "SourceRevision"=?"#)
                    .bind(revision)
                    .execute(&self.pool)
                    .await
                    .unwrap();
            }
            self.snapshot().await.is_ok_and(|current| {
                current.identity.descriptor == before.identity.descriptor
                    && current.identity.cache_key == before.identity.cache_key
            })
        })
        .await
        .and_then(|prepared| match prepared {
            source_teletext::PreparedText::Ready(caption) => Ok(caption.path),
            source_teletext::PreparedText::Absent(_) => {
                Err(ersatzrs_core::repositories::RepositoryError::unavailable(
                    "Next source teletext test",
                    "selected test page absent",
                ))
            }
        })
    }

    async fn prepare_with_validation<F, Fut>(
        &self,
        before: &dynamic_projection::Snapshot,
        decoder: &Path,
        validate: F,
    ) -> ersatzrs_core::repositories::Result<source_teletext::PreparedText>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = bool>,
    {
        self.prepare_with_video_cut(
            before,
            decoder,
            crate::next_engine::SourceVideoCutSupport::Unavailable,
            validate,
        )
        .await
    }

    async fn prepare_with_video_cut<F, Fut>(
        &self,
        before: &dynamic_projection::Snapshot,
        decoder: &Path,
        video_cut: crate::next_engine::SourceVideoCutSupport,
        validate: F,
    ) -> ersatzrs_core::repositories::Result<source_teletext::PreparedText>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = bool>,
    {
        self.prepare_with_video_cut_and_probe(before, decoder, &self.ffprobe, video_cut, validate)
            .await
    }

    async fn prepare_with_video_cut_and_probe<F, Fut>(
        &self,
        before: &dynamic_projection::Snapshot,
        decoder: &Path,
        ffprobe: &Path,
        video_cut: crate::next_engine::SourceVideoCutSupport,
        validate: F,
    ) -> ersatzrs_core::repositories::Result<source_teletext::PreparedText>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = bool>,
    {
        let pending = before.projection.pending_burn.as_ref().unwrap();
        let text = pending.source_text.as_deref().unwrap();
        assert_eq!(text.candidate().request.page.get(), 889);
        Box::pin(source_teletext::prepare(
            &self.pool,
            &self.locker,
            &self.cache,
            source_teletext::Preparation {
                video_cut,
                pending: text,
                source: &pending.source,
                video: before.projection.selected_video(),
                key: &before.identity.cache_key,
                expires: before.projection.item.finish,
                ffmpeg: decoder,
                ffprobe,
                deadline: Instant::now() + Duration::from_secs(15),
            },
            validate,
        ))
        .await
    }
}

async fn assert_no_cached_artifact(root: &Path) {
    let mut entries = tokio::fs::read_dir(root.join("cache")).await.unwrap();
    assert!(entries.next_entry().await.unwrap().is_none());
}

#[tokio::test]
#[ignore = "requires FFmpeg9, Python3, contained state/data and fresh ERSATZRS_NEXT_TELETEXT_PROOF_DIR"]
async fn complete_absence_retains_admission_and_revalidates_sql_and_source_without_decoder() {
    let root = fixture::proof_root();
    let fixture = Fixture::new_with_scenario(&root, CaptionScenario::Untransmitted).await;
    let before = fixture.snapshot().await.unwrap();
    let callbacks = AtomicUsize::new(0);
    let decoder = root.join("must-not-launch-for-complete-absence");
    assert!(!decoder.exists());
    let prepared = fixture
        .prepare_with_validation(&before, &decoder, || async {
            callbacks.fetch_add(1, Ordering::SeqCst);
            fixture.snapshot().await.is_ok_and(|current| {
                current.identity.descriptor == before.identity.descriptor
                    && current.identity.cache_key == before.identity.cache_key
            })
        })
        .await
        .unwrap();
    let source_teletext::PreparedText::Absent(absent) = prepared else {
        panic!("complete untransmitted page must yield owned absence, not a ready artifact");
    };
    assert_eq!(absent.page(), 889);
    assert_eq!(callbacks.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.locker.active_teletext_work(), 1);
    assert_no_cached_artifact(&root).await;
    absent
        .check_source(Instant::now() + Duration::from_secs(5))
        .await
        .unwrap();
    assert_eq!(fixture.locker.active_teletext_work(), 1);
    // The callback owns thirty seconds; each source operation still owns at
    // most the existing fifteen-second startup budget.
    absent
        .check_source(Instant::now() + Duration::from_secs(30))
        .await
        .unwrap();
    assert_eq!(fixture.locker.active_teletext_work(), 1);
    drop(absent);
    assert_eq!(fixture.locker.active_teletext_work(), 0);

    absence_refuses_stale_sql(&fixture, &before, &decoder, &callbacks, &root).await;
    assert_eq!(callbacks.load(Ordering::SeqCst), 2);
    assert_eq!(fixture.locker.active_teletext_work(), 0);
    assert_no_cached_artifact(&root).await;
    let changed = fixture
        .prepare_with_validation(&before, &decoder, || async {
            callbacks.fetch_add(1, Ordering::SeqCst);
            assert!(fixture.snapshot().await.is_ok_and(|current| {
                current.identity.descriptor == before.identity.descriptor
                    && current.identity.cache_key == before.identity.cache_key
            }));
            std::fs::OpenOptions::new()
                .append(true)
                .open(&fixture.clip)
                .unwrap()
                .write_all(&[0x47])
                .unwrap();
            true
        })
        .await;
    assert!(
        changed.is_err(),
        "source mutation during a successful SQL validator must be refused before returning absence"
    );
    assert_eq!(callbacks.load(Ordering::SeqCst), 3);
    assert_eq!(fixture.locker.active_teletext_work(), 0);
    assert_no_cached_artifact(&root).await;
    assert!(!decoder.exists());
    tokio::fs::write(
        root.join("owned-absence-validation.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "page":889,"validator_calls":3,"decoder_available":false,
            "complete_absence_retains_admission_until_drop":true,
            "joined_valid_source_recheck":true,"changed_sql_revision_refused":true,
            "source_mutated_after_valid_sql_refused":true,"negative_cache_artifacts":0,
            "final_teletext_admission":0,"next_process_or_http_executed":false,
        }))
        .unwrap(),
    )
    .await
    .unwrap();
}

async fn absence_refuses_stale_sql(
    fixture: &Fixture,
    before: &dynamic_projection::Snapshot,
    decoder: &Path,
    callbacks: &AtomicUsize,
    root: &Path,
) {
    let stale = copied_revision(&fixture.ffprobe, &fixture.clip, root).await;
    let original: String =
        sqlx::query_scalar(r#"SELECT "SourceRevision" FROM "SourceTeletextMetadata""#)
            .fetch_one(&fixture.pool)
            .await
            .unwrap();
    assert_ne!(stale, original);
    let result = fixture
        .prepare_with_validation(before, decoder, || async {
            callbacks.fetch_add(1, Ordering::SeqCst);
            sqlx::query(r#"UPDATE "SourceTeletextMetadata" SET "SourceRevision"=?"#)
                .bind(stale)
                .execute(&fixture.pool)
                .await
                .unwrap();
            fixture.snapshot().await.is_ok_and(|current| {
                current.identity.descriptor == before.identity.descriptor
                    && current.identity.cache_key == before.identity.cache_key
            })
        })
        .await;
    assert!(
        result.is_err(),
        "stale SQL cannot authorize ordinary fallback"
    );
    sqlx::query(r#"UPDATE "SourceTeletextMetadata" SET "SourceRevision"=?"#)
        .bind(original)
        .execute(&fixture.pool)
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "requires FFmpeg9, Python3, contained state/data and fresh ERSATZRS_NEXT_TELETEXT_PROOF_DIR"]
async fn finite_page_intent_selects_preferred_descriptor_before_first_late_header() {
    let root = fixture::proof_root();
    let fixture = Fixture::new_with_caption_timing(&root, true).await;
    let observed = crate::teletext_source::load_caption_candidates(
        &fixture.pool,
        10,
        100,
        1,
        fixture.clip.to_str().unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(
        observed
            .iter()
            .map(|candidate| candidate.request.page.get())
            .collect::<Vec<_>>(),
        [888]
    );
    let descriptors = crate::teletext_source::discover_caption_requests(
        &fixture.pool,
        10,
        100,
        1,
        fixture.clip.to_str().unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(
        descriptors
            .iter()
            .map(|request| request.candidate.request.page.get())
            .collect::<Vec<_>>(),
        [888, 889]
    );
    let before = fixture.snapshot().await.unwrap();
    let pending = before
        .projection
        .pending_burn
        .as_ref()
        .expect("private unprepared caption intent");
    let text = pending.source_text.as_ref().unwrap();
    assert_eq!(
        text.candidate().request.page.get(),
        889,
        "preferred language must not be replaced by the prefix-only page"
    );
    let track = before
        .projection
        .item
        .tracks
        .as_ref()
        .unwrap()
        .subtitle
        .as_ref()
        .unwrap();
    assert!(
        matches!(track.source.as_ref(), Some(crate::next_engine::protocol::Source::Local {path, ..}) if path == fixture.clip.to_str().unwrap()),
        "deferred placeholder must stay bound to the original source until preparation"
    );
    let callbacks = AtomicUsize::new(0);
    let path = fixture
        .prepare(&before, &fixture.ffmpeg, None, &callbacks)
        .await
        .unwrap();
    let text = tokio::fs::read_to_string(&path).await.unwrap();
    for label in ["889 EARLY", "889 LATE", "889 FINAL"] {
        assert!(text.contains(label));
    }
    assert!(!text.contains("ENGLISH PAGE 888"));
    assert_eq!(
        clock::cue_blocks(&text),
        clock::cue_blocks(
            &tokio::fs::read_to_string(root.join("independent-889.srt"))
                .await
                .unwrap()
        )
    );
    let absent_decoder = root.join("must-not-launch-on-current-cache-hit");
    assert_eq!(
        fixture
            .prepare(&before, &absent_decoder, None, &callbacks)
            .await
            .unwrap(),
        path
    );
    assert_eq!(callbacks.load(Ordering::SeqCst), 2);
    assert_eq!(fixture.locker.active_teletext_work(), 0);
    let still_observed = crate::teletext_source::load_caption_candidates(
        &fixture.pool,
        10,
        100,
        1,
        fixture.clip.to_str().unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(
        still_observed, observed,
        "preparation must not rewrite scanned headers/readiness"
    );
}

async fn cache_entries(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut entries = BTreeMap::new();
    let mut directory = tokio::fs::read_dir(root.join("cache")).await.unwrap();
    while let Some(entry) = directory.next_entry().await.unwrap() {
        let name = entry.file_name().into_string().unwrap();
        assert!(
            !name.starts_with(".burn-"),
            "no partial artifact may survive"
        );
        assert!(entry.file_type().await.unwrap().is_file());
        entries.insert(name, tokio::fs::read(entry.path()).await.unwrap());
    }
    assert_eq!(entries.len(), 2, "one complete sidecar and manifest");
    entries
}

async fn copied_revision(ffprobe: &Path, clip: &Path, root: &Path) -> String {
    let other = root.join("separately-acquired-source.ts");
    tokio::fs::copy(clip, &other).await.unwrap();
    let scope = LocalProbeCleanupScope::new();
    let result = scope
        .scope(FfprobeExtractor::new(ffprobe.to_str().unwrap()).extract_local_with_teletext(&other))
        .await;
    scope.close();
    scope.wait_cleanup().await.unwrap();
    result
        .unwrap()
        .teletext
        .unwrap()
        .revision()
        .as_str()
        .to_owned()
}

#[tokio::test]
#[ignore = "requires FFmpeg9, Python3, contained state/data and fresh ERSATZRS_NEXT_TELETEXT_PROOF_DIR"]
async fn finite_page_cache_hits_revalidate_sql_revision_and_observed_source_without_redecoding() {
    let root = fixture::proof_root();
    let fixture = Fixture::new(&root).await;
    let before = fixture.snapshot().await.unwrap();
    let callbacks = AtomicUsize::new(0);
    let cached = fixture
        .prepare(&before, &fixture.ffmpeg, None, &callbacks)
        .await
        .unwrap();
    assert_eq!(callbacks.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.locker.active_teletext_work(), 0);
    let complete = tokio::fs::read(&cached).await.unwrap();
    let text = std::str::from_utf8(&complete).unwrap();
    assert!(text.contains("NEDERLANDSE PAGINA 889 FINAL"));
    assert!(!text.contains("ENGLISH PAGE 888"));
    let entries = cache_entries(&root).await;

    let absent_decoder = root.join("decoder-must-not-be-launched");
    assert!(!absent_decoder.exists());
    assert_eq!(
        fixture
            .prepare(&before, &absent_decoder, None, &callbacks)
            .await
            .unwrap(),
        cached
    );
    assert_eq!(callbacks.load(Ordering::SeqCst), 2);
    assert_eq!(fixture.locker.active_teletext_work(), 0);
    assert_eq!(cache_entries(&root).await, entries);

    let stale = copied_revision(&fixture.ffprobe, &fixture.clip, &root).await;
    let original: String =
        sqlx::query_scalar(r#"SELECT "SourceRevision" FROM "SourceTeletextMetadata""#)
            .fetch_one(&fixture.pool)
            .await
            .unwrap();
    assert_ne!(stale, original);
    assert!(fixture
        .prepare(&before, &absent_decoder, Some(&stale), &callbacks)
        .await
        .is_err());
    assert_eq!(callbacks.load(Ordering::SeqCst), 3);
    assert_eq!(fixture.locker.active_teletext_work(), 0);
    assert_eq!(cache_entries(&root).await, entries);
    sqlx::query(r#"UPDATE "SourceTeletextMetadata" SET "SourceRevision"=?"#)
        .bind(original)
        .execute(&fixture.pool)
        .await
        .unwrap();

    std::fs::OpenOptions::new()
        .append(true)
        .open(&fixture.clip)
        .unwrap()
        .write_all(&[0x47])
        .unwrap();
    assert!(fixture
        .prepare(&before, &absent_decoder, None, &callbacks)
        .await
        .is_err());
    assert_eq!(callbacks.load(Ordering::SeqCst), 3);
    assert_eq!(fixture.locker.active_teletext_work(), 0);
    assert_eq!(cache_entries(&root).await, entries);
    assert_eq!(tokio::fs::read(cached).await.unwrap(), complete);
    tokio::fs::write(root.join("cache-hit-observation.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "warm_hit_without_available_decoder":true,"sql_revision_mutated_inside_hit_callback":true,
        "sql_hit_real_projection_refused":true,"source_mutation_refused_before_hit_callback":true,
        "validation_callback_count":3,"complete_cache_bytes_and_entries_unchanged":true,
        "joined_cleanup_and_admission_zero":true,"next_process_or_http_executed":false,
        "in_flight_final_handler_fence_proven":false,
    })).unwrap()).await.unwrap();
}
