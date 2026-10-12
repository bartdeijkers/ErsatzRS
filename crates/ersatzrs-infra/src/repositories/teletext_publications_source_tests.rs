//! Same-transaction semantic guide fences; no transmission/publication proof.

use super::*;
use chrono::{TimeZone as _, Utc};
use ersatzrs_core::repositories::PlayoutRepository as _;
use ersatzrs_domain::PlayoutItem;
use sqlx::Executor as _;

fn limits() -> TeletextGuideSourceCheckLimits {
    TeletextGuideSourceCheckLimits {
        max_records: 1000,
        max_source_bytes: 1024 * 1024,
        max_model_nodes: 100_000,
        max_model_bytes: 16 * 1024 * 1024,
        max_digest_fields: 100_000,
        max_digest_bytes: 16 * 1024 * 1024,
    }
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}

async fn guide(fixture: &Fixture) {
    for sql in [
        r#"INSERT INTO "MediaSource" ("Id") VALUES (10)"#,
        r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (10)"#,
        r#"INSERT INTO "Library" ("Id","MediaKind","MediaSourceId","Name") VALUES (10,1,10,'Synthetic')"#,
        r#"INSERT INTO "LocalLibrary" ("Id") VALUES (10)"#,
        r#"INSERT INTO "LibraryPath" ("Id","LibraryId","Path") VALUES (10,10,'media')"#,
        r#"INSERT INTO "MediaItem" ("Id","LibraryPathId") VALUES (20,10)"#,
        r#"INSERT INTO "Movie" ("Id") VALUES (20)"#,
        r#"INSERT INTO "MovieMetadata" ("Id","MovieId","DateAdded","DateUpdated","MetadataKind","Title","Plot") VALUES (30,20,'2026-01-01','2026-01-01',0,'Synthetic guide','Original plot')"#,
        r#"INSERT INTO "Playout" ("Id","ChannelId","ScheduleKind","Seed") VALUES (1,1,'Block',0)"#,
        r#"INSERT INTO "ConfigElement" ("Key","Value") VALUES ('pages.language','nld') ON CONFLICT("Key") DO UPDATE SET "Value"=excluded."Value""#,
        r#"INSERT INTO "ConfigElement" ("Key","Value") VALUES ('xmltv.block_behavior','UseActualTimes') ON CONFLICT("Key") DO UPDATE SET "Value"=excluded."Value""#,
    ] {
        fixture.repo.0.execute(sql).await.unwrap();
    }
    let start = Utc.with_ymd_and_hms(2026, 4, 15, 10, 0, 0).unwrap();
    super::super::super::make_repositories(fixture.repo.0.clone())
        .playout
        .upsert_item(&PlayoutItem {
            id: 31,
            playout_id: 1,
            media_item_id: 20,
            start,
            finish: start + chrono::Duration::hours(1),
            out_point: chrono::Duration::hours(1),
            ..PlayoutItem::default()
        })
        .await
        .unwrap();
}

async fn fence(fixture: &Fixture) -> TeletextGuideSourceFence {
    let clock = ProgrammeClock::system().unwrap();
    let captured_at = Utc.with_ymd_and_hms(2026, 4, 15, 10, 0, 0).unwrap();
    let (start, finish) = clock
        .calendar_day_bounds(captured_at, NonZeroU8::new(3).unwrap())
        .unwrap();
    let mut budget = GuideReadBudget::new_with_expansion(
        GuideReadLimits {
            max_records: limits().max_records,
            max_source_bytes: limits().max_source_bytes,
        },
        GuideExpansionLimits {
            max_nodes: limits().max_model_nodes,
            max_bytes: limits().max_model_bytes,
        },
    );
    // Read-only transaction captures the preparatory guide, without writes.
    let mut tx = fixture.repo.0.begin().await.unwrap();
    let snapshot = capture_guide_snapshot(&mut tx, &[1], start, finish, &mut budget)
        .await
        .unwrap();
    let digest = snapshot
        .context_digest(GuideSnapshotDigestLimits {
            max_fields: limits().max_digest_fields,
            max_bytes: limits().max_digest_bytes,
        })
        .unwrap();
    assert_eq!(snapshot.channel(1).unwrap().occurrences().len(), 1);
    TeletextGuideSourceFence {
        version: 1,
        context_digest_version: 1,
        context_digest: digest,
        channel_ids: vec![1],
        captured_at,
        start,
        finish,
        locale: "nl".into(),
        timezone: clock.iana_name().unwrap().into(),
    }
}

async fn save_document(fixture: &Fixture, document: &[u8]) -> TeletextPublicationRequestSave {
    let repository = SqliteTeletextDesignRepository(fixture.repo.0.clone());
    let TeletextDesignMutation::Saved(head) = repository
        .save(
            &TeletextDesignSave {
                name: "Source-bound originals".into(),
                document_version: 1,
                document: document.to_vec(),
                existing: None,
            },
            document.len(),
        )
        .await
        .unwrap()
    else {
        panic!("valid stored fixture");
    };
    let revision = repository
        .revision(head.id, 1, document.len())
        .await
        .unwrap()
        .unwrap();
    TeletextPublicationRequestSave {
        design_id: head.id,
        digest: revision.digest,
        ..fixture.save(None)
    }
}

async fn source_save(
    fixture: &Fixture,
    fence: &TeletextGuideSourceFence,
) -> TeletextPublicationRequestSave {
    save_document(
        fixture,
        &serde_json::to_vec(
            &serde_json::json!({"guide_source":fence,"ignored":{"guide_source":null}}),
        )
        .unwrap(),
    )
    .await
}

#[tokio::test]
async fn exact_source_target_requires_source_path_and_keeps_existing_pointer_on_change() {
    let fixture = Fixture::new().await;
    guide(&fixture).await;
    let fence = fence(&fixture).await;
    let mut save = source_save(&fixture, &fence).await;
    assert_eq!(
        fixture.repo.request(&save, 8192).await.unwrap(),
        M::Rejected(R::GuideFenceRequired)
    );
    let accepted = requested(
        fixture
            .repo
            .request_with_source(&save, &fence, limits(), 8192, deadline())
            .await
            .unwrap(),
    );
    assert_eq!(fixture.count().await, 1);
    save.expected_request_id = Some(accepted.id);
    fixture
        .repo
        .0
        .execute(r#"UPDATE "MovieMetadata" SET "Plot"='Changed plot' WHERE "Id"=30"#)
        .await
        .unwrap();
    assert_eq!(
        fixture
            .repo
            .request_with_source(&save, &fence, limits(), 8192, deadline())
            .await
            .unwrap(),
        M::Rejected(R::GuideChanged)
    );
    assert_eq!(fixture.count().await, 1);
    assert_eq!(fixture.repo.current(1).await.unwrap(), Some(accepted));
}

#[tokio::test]
async fn current_segments_paths_configuration_and_mirror_identity_are_fenced() {
    let fixture = Fixture::new().await;
    guide(&fixture).await;
    let mut previous = fence(&fixture).await;
    for sql in [
        r#"UPDATE "PlayoutItem" SET "OutPoint"='00:30:00' WHERE "Id"=31"#,
        r#"UPDATE "Playout" SET "ScheduleFile"='configured-only.json' WHERE "Id"=1"#,
        r#"UPDATE "ConfigElement" SET "Value"='deu' WHERE "Key"='pages.language'"#,
        r#"UPDATE "Channel" SET "Name"='Different source' WHERE "Id"=1"#,
        r#"UPDATE "Channel" SET "PlayoutSource"=1,"MirrorSourceChannelId"=1,"PlayoutOffset"='01:00:00' WHERE "Id"=1"#,
    ] {
        let save = source_save(&fixture, &previous).await;
        fixture.repo.0.execute(sql).await.unwrap();
        assert_eq!(
            fixture
                .repo
                .request_with_source(&save, &previous, limits(), 8192, deadline())
                .await
                .unwrap(),
            M::Rejected(R::GuideChanged)
        );
        previous = fence(&fixture).await;
    }
    assert_eq!(fixture.count().await, 0);
    assert!(fixture.repo.current(1).await.unwrap().is_none());
}

#[tokio::test]
async fn association_cas_deadline_and_all_six_admission_limits_fail_without_writes() {
    let fixture = Fixture::new().await;
    guide(&fixture).await;
    let fence = fence(&fixture).await;
    let save = source_save(&fixture, &fence).await;
    let mut foreign = fence.clone();
    foreign.context_digest[0] ^= 1;
    assert_eq!(
        fixture
            .repo
            .request_with_source(&save, &foreign, limits(), 8192, deadline())
            .await
            .unwrap(),
        M::Rejected(R::GuideFenceMismatch)
    );
    assert_eq!(
        fixture
            .repo
            .request_with_source(&save, &fence, limits(), 8192, Instant::now())
            .await
            .unwrap(),
        M::Rejected(R::GuideTimedOut)
    );
    let mut stale = save.clone();
    stale.expected_request_id = Some(99);
    assert_eq!(
        fixture
            .repo
            .request_with_source(&stale, &fence, limits(), 8192, deadline())
            .await
            .unwrap(),
        M::Rejected(R::RequestConflict)
    );
    let base = limits();
    for bounded in [
        TeletextGuideSourceCheckLimits {
            max_records: 0,
            ..base
        },
        TeletextGuideSourceCheckLimits {
            max_source_bytes: 0,
            ..base
        },
        TeletextGuideSourceCheckLimits {
            max_model_nodes: 0,
            ..base
        },
        TeletextGuideSourceCheckLimits {
            max_model_bytes: 0,
            ..base
        },
        TeletextGuideSourceCheckLimits {
            max_digest_fields: 0,
            ..base
        },
        TeletextGuideSourceCheckLimits {
            max_digest_bytes: 0,
            ..base
        },
    ] {
        assert_eq!(
            fixture
                .repo
                .request_with_source(&save, &fence, bounded, 8192, deadline())
                .await
                .unwrap(),
            M::Rejected(R::GuideUnavailable)
        );
    }
    assert_eq!(fixture.count().await, 0);
}

#[tokio::test]
async fn source_writer_before_immediate_admission_cannot_race_the_pending_insert() {
    let fixture = Fixture::new().await;
    guide(&fixture).await;
    let fence = fence(&fixture).await;
    let save = source_save(&fixture, &fence).await;
    let writer_pool = open(&fixture.root.path().join("requests.sqlite")).await;
    let mut writer = crate::db::begin_write(&writer_pool).await.unwrap();
    sqlx::query(r#"UPDATE "MovieMetadata" SET "Plot"='Writer committed change' WHERE "Id"=30"#)
        .execute(&mut *writer)
        .await
        .unwrap();
    let (actual, ()) = tokio::join!(
        fixture
            .repo
            .request_with_source(&save, &fence, limits(), 8192, deadline()),
        async {
            tokio::time::sleep(Duration::from_millis(20)).await;
            writer.commit().await.unwrap();
        },
    );
    assert_eq!(actual.unwrap(), M::Rejected(R::GuideChanged));
    assert_eq!(fixture.count().await, 0);
    assert!(fixture.repo.current(1).await.unwrap().is_none());
}

#[tokio::test]
async fn fresh_deadline_expires_while_writer_lock_is_held_without_pending_insert() {
    let fixture = Fixture::new().await;
    guide(&fixture).await;
    let fence = fence(&fixture).await;
    let save = source_save(&fixture, &fence).await;
    let writer_pool = open(&fixture.root.path().join("requests.sqlite")).await;
    let writer = crate::db::begin_write(&writer_pool).await.unwrap();
    let result = tokio::time::timeout(
        Duration::from_secs(1),
        fixture.repo.request_with_source(
            &save,
            &fence,
            limits(),
            8192,
            Instant::now() + Duration::from_millis(20),
        ),
    )
    .await
    .expect("fresh deadline must not wait for the SQLite busy timeout")
    .unwrap();
    assert_eq!(result, M::Rejected(R::GuideTimedOut));
    writer.rollback().await.unwrap();
    assert_eq!(fixture.count().await, 0);
    assert!(fixture.repo.current(1).await.unwrap().is_none());
}

#[tokio::test]
async fn different_valid_named_clock_cannot_replace_actual_system_civil_context() {
    let fixture = Fixture::new().await;
    guide(&fixture).await;
    let mut fence = fence(&fixture).await;
    let other = ProgrammeClock::named(if fence.timezone == "Asia/Tokyo" {
        "Europe/Amsterdam"
    } else {
        "Asia/Tokyo"
    })
    .unwrap();
    fence.timezone = other.iana_name().unwrap().into();
    (fence.start, fence.finish) = other
        .calendar_day_bounds(fence.captured_at, NonZeroU8::new(3).unwrap())
        .unwrap();
    fence.validate().unwrap();
    let save = source_save(&fixture, &fence).await;
    assert_eq!(
        fixture
            .repo
            .request_with_source(&save, &fence, limits(), 8192, deadline())
            .await
            .unwrap(),
        M::Rejected(R::GuideChanged)
    );
    assert_eq!(fixture.count().await, 0);
}

#[tokio::test]
async fn accepted_revision_and_exact_path_associations_change_with_equal_programmes() {
    let fixture = Fixture::new().await;
    guide(&fixture).await;
    fixture.repo.0.execute(r#"UPDATE "Playout" SET "ScheduleKind"=20,"ScheduleFile"='configured.json' WHERE "Id"=1"#).await.unwrap();
    fixture.repo.0.execute(r#"INSERT INTO "PlayoutExternalJsonSnapshot" ("PlayoutId","Revision","SchemaVersion","ScheduleFile","Document","PublishedAt") VALUES (1,7,1,'accepted.json','{"startTime":"2026-04-15T10:00:00Z","programs":[{"title":"Synthetic","duration":3600000}]}','synthetic')"#).await.unwrap();
    let mut previous = fence(&fixture).await;
    for sql in [
        r#"UPDATE "PlayoutExternalJsonSnapshot" SET "Revision"=8 WHERE "PlayoutId"=1"#,
        r#"UPDATE "PlayoutExternalJsonSnapshot" SET "ScheduleFile"='accepted-only.json' WHERE "PlayoutId"=1"#,
        r#"UPDATE "Playout" SET "ScheduleFile"='configured-only.json' WHERE "Id"=1"#,
    ] {
        let save = source_save(&fixture, &previous).await;
        fixture.repo.0.execute(sql).await.unwrap();
        assert_eq!(
            fixture
                .repo
                .request_with_source(&save, &previous, limits(), 8192, deadline())
                .await
                .unwrap(),
            M::Rejected(R::GuideChanged)
        );
        previous = fence(&fixture).await;
    }
    assert_eq!(fixture.count().await, 0);
}

#[tokio::test]
async fn stored_root_is_closed_against_source_bypass_but_preserves_opaque_static_data() {
    let fixture = Fixture::new().await;
    let accepted = fixture.request(None).await;
    guide(&fixture).await;
    let fence = fence(&fixture).await;
    let value = serde_json::to_string(&fence).unwrap();
    for document in [
        "{broken".to_owned(),
        "[]".to_owned(),
        "{\"guide_source\":null}".to_owned(),
        format!("{{\"guide_source\":{value},\"guide_source\":{value}}}"),
        format!("{{\"guide_source\":{value}}}trailing"),
    ] {
        let mut save = save_document(&fixture, document.as_bytes()).await;
        save.expected_request_id = Some(accepted.id);
        assert_eq!(
            fixture.repo.request(&save, 8192).await.unwrap(),
            M::Rejected(R::GuideUnavailable)
        );
    }
    let mut escaped = save_document(
        &fixture,
        format!("{{\"guide_\\u0073ource\":{value}}}").as_bytes(),
    )
    .await;
    escaped.expected_request_id = Some(accepted.id);
    assert_eq!(
        fixture.repo.request(&escaped, 8192).await.unwrap(),
        M::Rejected(R::GuideFenceRequired)
    );
    let mut absent = save_document(&fixture, b"{\"nested\":{\"guide_source\":null}}").await;
    absent.expected_request_id = Some(accepted.id);
    assert_eq!(
        fixture
            .repo
            .request_with_source(&absent, &fence, limits(), 8192, deadline())
            .await
            .unwrap(),
        M::Rejected(R::GuideFenceMismatch)
    );
    assert_eq!(fixture.count().await, 1);
    assert_eq!(fixture.repo.current(1).await.unwrap(), Some(accepted));
}
