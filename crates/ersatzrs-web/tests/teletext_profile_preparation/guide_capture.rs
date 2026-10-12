//! Captured SQL facts reach installed V1/V2 profiles and one inert native candidate.
//! The parent deliberately limits real POSIX producer fixtures to Unix.

use super::*;
use ersatzrs_core::repositories::PlayoutRepository as _;
use ersatzrs_domain::PlayoutItem;
use ersatzrs_infra::xmltv::{GuideExpansionLimits, GuideReadBudget, GuideReadLimits};
use ersatzrs_web::host::teletext_guide::capture_teletext_guide;
use serde_json::Value;

async fn seed_guide(fixture: &Fixture) {
    for sql in [
        r#"INSERT INTO "ConfigElement" ("Key","Value") VALUES ('pages.language','NL_NL')"#,
        r#"INSERT INTO "MediaSource" ("Id") VALUES (10)"#,
        r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (10)"#,
        r#"INSERT INTO "Library" ("Id","MediaKind","MediaSourceId","Name") VALUES (10,1,10,'Synthetic')"#,
        r#"INSERT INTO "LocalLibrary" ("Id") VALUES (10)"#,
        r#"INSERT INTO "LibraryPath" ("Id","LibraryId","Path") VALUES (10,10,'media')"#,
        r#"INSERT INTO "MediaItem" ("Id","LibraryPathId") VALUES (20,10)"#,
        r#"INSERT INTO "Movie" ("Id") VALUES (20)"#,
        r#"INSERT INTO "MovieMetadata" ("Id","MovieId","DateAdded","DateUpdated","MetadataKind","Title") VALUES (30,20,'2026-01-01','2026-01-01',0,'Captured Guide')"#,
        r#"INSERT INTO "Playout" ("Id","ChannelId","ScheduleKind","Seed") VALUES (20,2,'Classic',0)"#,
    ] {
        sqlx::query(sql).execute(&fixture.state.pool).await.unwrap();
    }
    let repositories = make_repositories(fixture.state.pool.clone());
    let at = guide(&[]).captured_at;
    for (id, source, offset) in [(40, 0, -5), (41, 10, 5)] {
        let item = PlayoutItem {
            id,
            playout_id: 20,
            media_item_id: 20,
            guide_group: 7,
            start: at + chrono::Duration::minutes(offset),
            finish: at + chrono::Duration::minutes(offset + 10),
            in_point: chrono::Duration::minutes(source),
            out_point: chrono::Duration::minutes(source + 10),
            ..PlayoutItem::default()
        };
        repositories.playout.upsert_item(&item).await.unwrap();
    }
}

#[tokio::test]
async fn captured_sql_guide_reaches_installed_mixed_profiles_without_publication_or_source_reselection(
) {
    let fixture = Fixture::new().await;
    let first = channel_documents::response(documents::DocumentFixture::Base, "day", 123, 0, "nld");
    let second =
        channel_documents::response(documents::DocumentFixture::Authored, "night", 223, 0, "fra");
    channel_documents::configure_pair(&fixture, &first, &second).await;
    seed_guide(&fixture).await;
    let mut budget = GuideReadBudget::new_with_expansion(
        GuideReadLimits {
            max_records: 1000,
            max_source_bytes: 1024 * 1024,
        },
        GuideExpansionLimits {
            max_nodes: 10_000,
            max_bytes: 16 * 1024 * 1024,
        },
    );
    let capture_deadline = std::time::Instant::now() + Duration::from_secs(20);
    let captured = capture_teletext_guide(
        &fixture.state.pool,
        &[2],
        guide(&[]).captured_at,
        &mut budget,
        capture_deadline,
    )
    .await
    .unwrap();
    let candidate = captured
        .prepare_document_candidate(
            &fixture.state.application,
            1,
            Some(SECONDARY),
            GENERATION,
            channel_documents::policy(),
        )
        .await
        .unwrap();
    assert_eq!(candidate.deadline(), capture_deadline);
    assert_eq!(candidate.selection().channel_id, 1);
    assert!(candidate.encoded().is_some());
    assert_eq!(captured.snapshot().channels().len(), 1);
    assert_eq!(captured.snapshot().channels()[0].id(), 2);
    assert_eq!(
        captured.snapshot().channels()[0].occurrences()[0]
            .segments
            .len(),
        2
    );
    for (label, schema) in [("first", 1), ("second", 2)] {
        let bytes = std::fs::read(
            fixture
                .root
                .path()
                .join(format!("channel-input-{label}.json")),
        )
        .unwrap();
        assert_eq!(bytes, captured.request_bytes(GENERATION, schema).unwrap());
        let request: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(request["locale"], "nl");
        assert_eq!(request["timezone"], captured.timezone());
        assert_eq!(request["channels"][0]["id"], 2);
        assert_eq!(
            request["channels"][0]["programmes"][0]["title"],
            "Captured Guide"
        );
    }
    assert!(fixture
        .state
        .application
        .channel_teletext_bindings(2)
        .await
        .unwrap()
        .is_empty());
    let publications: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "TeletextPublicationRequest""#)
            .fetch_one(&fixture.state.pool)
            .await
            .unwrap();
    assert_eq!(publications, 0);
}
