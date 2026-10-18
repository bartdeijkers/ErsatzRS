//! Actual finite-page cache hits retain source and current SQL qualification.

use super::*;
use crate::{
    ffprobe::{FfprobeExtractor, LocalProbeCleanupScope},
    next_engine::{burn_cache::BurnCache, dynamic_projection, media::MediaRelay, source_teletext},
};
use ersatzrs_core::locking::EntityLocker;
use std::{
    collections::BTreeMap,
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

pub(super) async fn assert_complete_text_preparation(
    pool: &crate::db::DbPool,
    locker: &EntityLocker,
    ffmpeg: &Path,
    ffprobe: &Path,
    clip: &Path,
) {
    let candidate =
        crate::teletext_source::load_caption_candidates(pool, 10, 100, 1, clip.to_str().unwrap())
            .await
            .unwrap()
            .into_iter()
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
    let artifact =
        result.unwrap_or_else(|error| panic!("complete selected-page preparation: {error:?}"));
    assert!(artifact.content.contains("NEDERLANDSE PAGINA 889 FINAL"));
    assert!(!artifact.content.contains("ENGLISH PAGE 888"));
}

impl Fixture {
    async fn new(root: &Path) -> Self {
        let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
        let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
        let clip = fixture::source(&ffmpeg, root).await;
        let (pool, locker) = prepare_schedule(&clip, false, &text_profile()).await;
        fixture::acquire_and_store(&pool, &ffprobe, &clip, root).await;
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
        let pending = before.projection.pending_burn.as_ref().unwrap();
        let text = pending.source_text.as_deref().unwrap();
        assert_eq!(text.candidate.request.page.get(), 889);
        Box::pin(source_teletext::prepare(
            &self.pool,
            &self.locker,
            &self.cache,
            source_teletext::Preparation {
                pending: text,
                source: &pending.source,
                key: &before.identity.cache_key,
                expires: before.projection.item.finish,
                ffmpeg: decoder,
                ffprobe: &self.ffprobe,
                deadline: Instant::now() + Duration::from_secs(15),
            },
            || async {
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
            },
        ))
        .await
    }
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
