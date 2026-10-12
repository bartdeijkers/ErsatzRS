//! Captured SQL facts reach installed V1/V2 profiles and one inert native candidate.
//! The parent deliberately limits real POSIX producer fixtures to Unix.

use super::*;
use ersatzrs_core::repositories::PlayoutRepository as _;
use ersatzrs_domain::PlayoutItem;
use ersatzrs_infra::xmltv::{GuideExpansionLimits, GuideReadBudget, GuideReadLimits};
use ersatzrs_web::host::teletext_guide::{capture_teletext_guide, TeletextGuideCaptureError};
use serde_json::Value;

pub(super) async fn seed_guide(fixture: &Fixture) {
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
    let mut budget = read_budget();
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
    let original_usage = budget.usage();
    let candidate = captured
        .prepare_document_candidate(
            &fixture.state.application,
            1,
            Some(SECONDARY),
            GENERATION,
            channel_documents::policy(),
            &mut budget,
        )
        .await
        .unwrap();
    assert_eq!(budget.usage().records, original_usage.records * 3);
    assert_eq!(budget.usage().source_bytes, original_usage.source_bytes * 3);
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
        let contract = match schema {
            1 => ersatzrs_domain::TeletextPageContract::V1,
            2 => ersatzrs_domain::TeletextPageContract::V2,
            _ => unreachable!(),
        };
        assert_eq!(
            candidate.requests().request(contract),
            Some(bytes.as_slice())
        );
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

pub(super) fn read_budget() -> GuideReadBudget {
    GuideReadBudget::new_with_expansion(
        GuideReadLimits {
            max_records: 1000,
            max_source_bytes: 1024 * 1024,
        },
        GuideExpansionLimits {
            max_nodes: 10_000,
            max_bytes: 16 * 1024 * 1024,
        },
    )
}

#[tokio::test]
async fn changed_source_before_invocation_rejects_without_calling_producers() {
    for (sql, channel_id) in [
        (
            r#"UPDATE "MovieMetadata" SET "Title"='Changed Guide' WHERE "Id"=30"#,
            Some(2),
        ),
        (
            r#"UPDATE "Channel" SET "Name"='Changed Channel' WHERE "Id"=2"#,
            Some(2),
        ),
        // An alias still resolves to nl, but the original raw context changed.
        (
            r#"UPDATE "ConfigElement" SET "Value"='nl' WHERE "Key"='pages.language'"#,
            None,
        ),
    ] {
        let fixture = Fixture::new().await;
        channel_documents::configure_pair(
            &fixture,
            &channel_documents::response(documents::DocumentFixture::Base, "day", 123, 0, "nld"),
            &channel_documents::response(
                documents::DocumentFixture::Authored,
                "night",
                223,
                0,
                "fra",
            ),
        )
        .await;
        seed_guide(&fixture).await;
        let mut budget = read_budget();
        let captured = capture_teletext_guide(
            &fixture.state.pool,
            &[2],
            guide(&[]).captured_at,
            &mut budget,
            std::time::Instant::now() + Duration::from_secs(20),
        )
        .await
        .unwrap();
        sqlx::query(sql).execute(&fixture.state.pool).await.unwrap();
        let result = captured
            .prepare_document_candidate(
                &fixture.state.application,
                1,
                Some(SECONDARY),
                GENERATION,
                channel_documents::policy(),
                &mut budget,
            )
            .await;
        assert!(
            matches!(result, Err(TeletextGuideCaptureError::SourceChanged { channel_id: actual }) if actual == channel_id)
        );
        assert!(!fixture.root.path().join("channel-calls").exists());
        assert_no_publication(&fixture).await;
    }
}

async fn assert_no_publication(fixture: &Fixture) {
    let publications: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "TeletextPublicationRequest""#)
            .fetch_one(&fixture.state.pool)
            .await
            .unwrap();
    assert_eq!(publications, 0);
}

const GUIDE_PRODUCER_GATE: &str = r#"touch ../guide-producer-ready
attempts=0
while [ ! -f ../guide-producer-release ]; do
    attempts=$((attempts + 1))
    [ "$attempts" -lt 500 ] || exit 8
    sleep 0.01
done
cat ../channel-response-first.json"#;

#[tokio::test]
async fn changed_source_during_real_producer_rejects_the_complete_native_candidate() {
    let fixture = Fixture::new().await;
    channel_documents::configure_pair(
        &fixture,
        &channel_documents::response(documents::DocumentFixture::Base, "day", 123, 0, "nld"),
        &channel_documents::response(documents::DocumentFixture::Authored, "night", 223, 0, "fra"),
    )
    .await;
    seed_guide(&fixture).await;
    // Configure this bounded fixture before preflight; preserve normal installed
    // package hash validation instead of changing package bytes during execution.
    let binding_revision = fixture
        .state
        .application
        .channel_teletext_bindings(1)
        .await
        .unwrap()
        .into_iter()
        .find(|selection| selection.binding.addon_id == ADDON)
        .unwrap()
        .binding
        .revision;
    let package = fixture.root.path().join("package");
    let script = std::fs::read_to_string(package.join("fixture.sh")).unwrap();
    assert!(script.contains("cat ../channel-response-first.json"));
    std::fs::write(
        package.join("fixture.sh"),
        script.replace("cat ../channel-response-first.json", GUIDE_PRODUCER_GATE),
    )
    .unwrap();
    let hash = calculate_addon_content_sha256(&package).await.unwrap();
    sqlx::query(r#"UPDATE "AddonInstallation" SET "ContentSha256"=? WHERE "AddonId"=?"#)
        .bind(hash)
        .bind(ADDON)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    // Profiles bind an exact installation token. Revalidate and bind the new
    // fixture package before capture instead of bypassing that production fence.
    let profile = fixture
        .profile_for(ADDON, "Guide Gate", "day", "good")
        .await;
    fixture.bind(1, &profile, Some(binding_revision)).await;
    let mut budget = read_budget();
    let captured = capture_teletext_guide(
        &fixture.state.pool,
        &[2],
        guide(&[]).captured_at,
        &mut budget,
        std::time::Instant::now() + Duration::from_secs(20),
    )
    .await
    .unwrap();
    let mutate = async {
        let ready = tokio::time::timeout(Duration::from_secs(5), async {
            while !fixture.root.path().join("guide-producer-ready").exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await;
        if ready.is_err() {
            std::fs::write(fixture.root.path().join("guide-producer-release"), b"ready").unwrap();
            return false;
        }
        sqlx::query(
            r#"UPDATE "MovieMetadata" SET "Title"='Changed During Producer' WHERE "Id"=30"#,
        )
        .execute(&fixture.state.pool)
        .await
        .unwrap();
        std::fs::write(fixture.root.path().join("guide-producer-release"), b"ready").unwrap();
        true
    };
    let (result, mutated) = tokio::join!(
        captured.prepare_document_candidate(
            &fixture.state.application,
            1,
            Some(SECONDARY),
            GENERATION,
            channel_documents::policy(),
            &mut budget
        ),
        mutate,
    );
    assert!(mutated, "producer did not reach gate: {result:?}");
    assert!(matches!(
        result,
        Err(TeletextGuideCaptureError::SourceChanged {
            channel_id: Some(2)
        })
    ));
    assert_eq!(
        std::fs::read_to_string(fixture.root.path().join("channel-calls")).unwrap(),
        "first:teletext.pages.v1\nsecond:teletext.pages.v2\n"
    );
    assert_eq!(
        captured.snapshot().channels()[0].occurrences()[0]
            .programme
            .title,
        "Captured Guide"
    );
    assert_no_publication(&fixture).await;
}
