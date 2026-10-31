//! Maintenance reporting is bounded/redacted and does not certify live output.
use super::*;
use crate::health::TeletextGuideRefreshHealthCheck;
use ersatzrs_core::health::{HealthCheck as _, HealthCheckStatus};

#[tokio::test]
async fn guide_maintenance_health_separates_review_from_serving_caps_and_bounds_redacted_state() {
    let fixture = Fixture::new().await;
    let repo = SqliteTeletextGenerationRepository(fixture.repo.0.clone());
    let health = TeletextGuideRefreshHealthCheck::new(repo.0.clone());
    assert_eq!(
        health.check().await.unwrap().status,
        HealthCheckStatus::Info
    );
    sqlx::query(r#"INSERT OR REPLACE INTO "ConfigElement" ("Key","Value") VALUES ('teletext.output.policy','{"version":4}')"#)
        .execute(&repo.0).await.unwrap();
    assert!(health
        .check()
        .await
        .unwrap()
        .message
        .contains("No registered"));
    check_legacy_base_authorization().await;
    let save = base_save(&fixture).await;
    accepted(repo.accept_guide(&save, &check(), MAXIMUM).await.unwrap());
    let at = save.qualification.source.captured_at;
    let result = health.check_at(at).await.unwrap();
    assert_eq!(result.status, HealthCheckStatus::Pass);
    assert!(result.message.contains("fresh=1"));
    let port = repo.guide_refresh().unwrap();
    let seed = base_current(port).await.seed;
    sqlx::query(r#"UPDATE "TeletextGuideRefreshCurrent" SET "Failure"=3 WHERE "SeedId"=?"#)
        .bind(seed.id)
        .execute(&repo.0)
        .await
        .unwrap();
    let review = health.check_at(at).await.unwrap();
    assert_eq!(review.status, HealthCheckStatus::Warning);
    assert!(review.message.contains("fresh=1") && review.message.contains("needs-review=1"));
    let loss = at + ChronoDuration::seconds(1);
    assert_eq!(port.changed(seed.id, loss).await.unwrap(), M::Accepted(()));
    let stale = health.check_at(loss).await.unwrap();
    assert!(stale.message.contains("stale=1") && stale.message.contains("needs-review=1"));
    let reopened = TeletextGuideRefreshHealthCheck::new(
        open(&fixture.root.path().join("requests.sqlite")).await,
    );
    assert_eq!(reopened.check_at(loss).await.unwrap(), stale);
    let expiry = base_current(port).await.available_until();
    assert!(health
        .check_at(expiry)
        .await
        .unwrap()
        .message
        .contains("unavailable=1"));
    assert_eq!(port.revoke(seed.id, loss).await.unwrap(), M::Accepted(()));
    assert!(health
        .check_at(loss)
        .await
        .unwrap()
        .message
        .contains("revoked=1"));
    sqlx::query(r#"UPDATE "TeletextGuideRefreshCurrent" SET "LastSuccessAt"='private-invalid-clock' WHERE "SeedId"=?"#)
        .bind(seed.id).execute(&repo.0).await.unwrap();
    let invalid = health.check_at(at).await.unwrap();
    assert_eq!(invalid.status, HealthCheckStatus::Warning);
    assert!(invalid.message.contains("invalid metadata=1"));
    assert!(!invalid.message.contains("private-invalid-clock"));
    add_bounded_health_origins(&repo, seed.id, at).await;
    let bounded = health.check_at(at).await.unwrap();
    assert_eq!(bounded.status, HealthCheckStatus::Warning);
    assert!(bounded.message.contains("incomplete=true"));
    assert!(!bounded.message.contains("private"));
}

async fn add_bounded_health_origins(
    repo: &SqliteTeletextGenerationRepository,
    seed_id: i64,
    at: chrono::DateTime<Utc>,
) {
    // Schema-valid synthetic maintenance rows prove bounded reporting only;
    // they are not native qualification, runtime adoption or stream authority.
    sqlx::query(
        r#"WITH RECURSIVE n(i) AS (SELECT 1001 UNION ALL SELECT i+1 FROM n WHERE i<1256)
        INSERT INTO "TeletextPublicationRequest" ("Id","ChannelId","DesignId","Revision","Digest")
        SELECT n.i,n.i,r."DesignId",r."Revision",r."Digest" FROM n
        JOIN "TeletextGuideRefreshSeed" s ON s."Id"=?
        JOIN "TeletextAcceptedGeneration" g ON g."Id"=s."GenerationId"
        JOIN "TeletextPublicationRequest" r ON r."Id"=g."RequestId""#,
    )
    .bind(seed_id)
    .execute(&repo.0)
    .await
    .unwrap();
    sqlx::query(r#"INSERT INTO "TeletextAcceptedGeneration" ("Id","ChannelId","RequestId","ArtifactVersion","ArtifactDigest")
        SELECT r."Id",r."ChannelId",r."Id",5,g."ArtifactDigest" FROM "TeletextPublicationRequest" r
        JOIN "TeletextGuideRefreshSeed" s ON s."Id"=? JOIN "TeletextAcceptedGeneration" g ON g."Id"=s."GenerationId"
        WHERE r."Id" BETWEEN 1001 AND 1256"#).bind(seed_id).execute(&repo.0).await.unwrap();
    sqlx::query(r#"INSERT INTO "TeletextGuideRefreshSeed" ("ChannelId","Role","GenerationId","ExpectedDesignHead","Source","ValidUntil")
        SELECT g."ChannelId",1,g."Id",s."ExpectedDesignHead",s."Source",s."ValidUntil"
        FROM "TeletextAcceptedGeneration" g JOIN "TeletextGuideRefreshSeed" s ON s."Id"=?
        WHERE g."Id" BETWEEN 1001 AND 1256"#).bind(seed_id).execute(&repo.0).await.unwrap();
    sqlx::query(r#"INSERT INTO "TeletextGuideRefreshCurrent" ("ChannelId","Role","SeedId","Revision","NextDueAt","LastSuccessAt")
        SELECT "ChannelId",1,"Id",1,?,? FROM "TeletextGuideRefreshSeed" WHERE "GenerationId" BETWEEN 1001 AND 1256"#)
        .bind(at+ChronoDuration::minutes(5)).bind(at).execute(&repo.0).await.unwrap();
}

async fn check_legacy_base_authorization() {
    let fixture = Fixture::new().await;
    let repo = SqliteTeletextGenerationRepository(fixture.repo.0.clone());
    let save = base_save(&fixture).await;
    // Replay the old accepted/pointer SQL path, not new acceptance followed by
    // deleting its seed. No native/runtime or client qualification is inferred.
    legacy_pointer(&repo, &save.original.request, 5).await;
    sqlx::query(r#"INSERT OR REPLACE INTO "ConfigElement" ("Key","Value") VALUES ('teletext.output.policy','{"version":4}')"#)
        .execute(&repo.0).await.unwrap();
    let health = TeletextGuideRefreshHealthCheck::new(repo.0.clone());
    let at = save.qualification.source.captured_at;
    let warning = health.check_at(at).await.unwrap();
    assert_eq!(warning.status, HealthCheckStatus::Warning);
    assert!(warning
        .message
        .contains("Missing Base guide refresh authorization: 1."));
    assert!(warning.message.contains("publish explicitly"));
    assert!(warning
        .message
        .contains("ordinary audio/video playback remains available"));
    for table_count in [
        r#"SELECT COUNT(*) FROM "TeletextGuideRefreshSeed""#,
        r#"SELECT COUNT(*) FROM "TeletextGuideRefreshCurrent""#,
    ] {
        assert_eq!(
            sqlx::query_scalar::<_, i64>(table_count)
                .fetch_one(&repo.0)
                .await
                .unwrap(),
            0
        );
    }
    sqlx::query(r#"UPDATE "ChannelTeletextBinding" SET "Enabled"=0 WHERE "ChannelId"=1"#)
        .execute(&repo.0)
        .await
        .unwrap();
    assert_eq!(
        health.check_at(at).await.unwrap().status,
        HealthCheckStatus::Info
    );
    sqlx::query(r#"UPDATE "ChannelTeletextBinding" SET "Enabled"=1 WHERE "ChannelId"=1"#)
        .execute(&repo.0)
        .await
        .unwrap();
    let static_revision = SqliteTeletextDesignRepository(repo.0.clone())
        .revision(fixture.design_id, 1, MAXIMUM)
        .await
        .unwrap()
        .unwrap();
    // Legacy role classification only: frozen1..4 must not acquire a new guide
    // authorization requirement. Scope/client/native acceptance is unchanged.
    for version in 1_u16..=4 {
        let request = ersatzrs_domain::TeletextPublicationRequest {
            id: 2000 + i64::from(version),
            channel_id: 1,
            design_id: fixture.design_id,
            revision: 1,
            digest: static_revision.digest.clone(),
        };
        sqlx::query(r#"INSERT INTO "TeletextPublicationRequest" ("Id","ChannelId","DesignId","Revision","Digest") VALUES (?,?,?,?,?)"#)
            .bind(request.id).bind(request.channel_id).bind(request.design_id).bind(request.revision).bind(&request.digest)
            .execute(&repo.0).await.unwrap();
        legacy_pointer(&repo, &request, version).await;
        let result = health.check_at(at).await.unwrap();
        assert_eq!(result.status, HealthCheckStatus::Info);
        assert!(!result
            .message
            .contains("Missing Base guide refresh authorization"));
    }
}

async fn legacy_pointer(
    repo: &SqliteTeletextGenerationRepository,
    request: &ersatzrs_domain::TeletextPublicationRequest,
    version: u16,
) {
    let inserted = sqlx::query(r#"INSERT INTO "TeletextAcceptedGeneration" ("ChannelId","RequestId","ArtifactVersion","ArtifactDigest") VALUES (?,?,?,?)"#)
        .bind(request.channel_id).bind(request.id).bind(i64::from(version)).bind("a".repeat(64))
        .execute(&repo.0).await.unwrap();
    let sql = if matches!(version, 1 | 3 | 5) {
        r#"INSERT INTO "TeletextAcceptedGenerationCurrent" ("ChannelId","GenerationId") VALUES (?,?) ON CONFLICT("ChannelId") DO UPDATE SET "GenerationId"=excluded."GenerationId""#
    } else {
        r#"INSERT INTO "TeletextEffectiveAcceptedGenerationCurrent" ("ChannelId","GenerationId") VALUES (?,?) ON CONFLICT("ChannelId") DO UPDATE SET "GenerationId"=excluded."GenerationId""#
    };
    sqlx::query(sql)
        .bind(request.channel_id)
        .bind(inserted.last_insert_rowid())
        .execute(&repo.0)
        .await
        .unwrap();
}
