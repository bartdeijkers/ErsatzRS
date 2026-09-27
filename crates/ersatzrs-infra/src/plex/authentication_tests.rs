use super::*;
use httpmock::prelude::*;
use serde_json::json;

#[tokio::test]
async fn explicit_sign_out_invalidates_in_flight_authorization() {
    use ersatzrs_core::repositories::MediaSourceRepository;
    let pool = pool().await;
    let pending = flow();
    let encoded = write_flow(&pool, &pending).await;
    crate::repositories::make_repositories(pool.clone())
        .media_source
        .delete_all_plex_sources_admin()
        .await
        .unwrap();
    assert!(
        commit_credentials(&pool, &encoded, &pending, "synthetic-late-token")
            .await
            .is_err()
    );
    assert!(setting(&pool, PLEX_USER_AUTH_TOKEN_KEY).await.is_none());
    assert!(setting(&pool, PENDING_FLOW_KEY).await.is_none());
}

#[tokio::test]
async fn successful_account_refresh_marks_absence_and_restores_returning_server() {
    let pool = pool().await;
    let server = MockServer::start_async().await;
    let absent = server
        .mock_async(|when, then| {
            when.method(GET).path("/api/v2/resources");
            then.status(200).json_body(json!([]));
        })
        .await;
    assert!(service(&server)
        .refresh_account_servers(&pool)
        .await
        .unwrap());
    let missing: Option<String> =
        sqlx::query_scalar(r#"SELECT "MissingSince" FROM "PlexMediaSource" WHERE "Id" = 1"#)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(missing.is_some());
    absent.delete_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/api/v2/resources");
            then.status(200).json_body(json!([{
                "name": "Returned server",
                "clientIdentifier": "synthetic-server",
                "accessToken": "synthetic-server-token",
                "provides": "server", "owned": true,
                "connections": [{"uri": "http://plex.example.test:32400"}]
            }]));
        })
        .await;
    assert!(service(&server)
        .refresh_account_servers(&pool)
        .await
        .unwrap());
    let row: (String, Option<String>) = sqlx::query_as(
        r#"SELECT "ServerName", "MissingSince" FROM "PlexMediaSource" WHERE "Id" = 1"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row, ("Returned server".to_owned(), None));
    assert_old_credentials(&pool).await;
}

#[tokio::test]
async fn discovery_without_account_credentials_does_not_mark_servers_missing() {
    let pool = pool().await;
    let server = MockServer::start_async().await;
    let probe = server
        .mock_async(|when, then| {
            when.method(GET);
            then.status(500);
        })
        .await;
    sqlx::query(r#"DELETE FROM "ConfigElement" WHERE "Key" = 'plex.user_auth_token'"#)
        .execute(&pool)
        .await
        .unwrap();
    assert!(!service(&server)
        .refresh_account_servers(&pool)
        .await
        .unwrap());
    let missing: Option<String> =
        sqlx::query_scalar(r#"SELECT "MissingSince" FROM "PlexMediaSource" WHERE "Id" = 1"#)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(missing.is_none());
    probe.assert_calls_async(0).await;
}

#[tokio::test]
async fn unauthorized_discovery_clears_credentials_but_preserves_sources_and_pending_sign_in() {
    let pool = pool().await;
    let server = MockServer::start_async().await;
    let encoded = write_flow(&pool, &flow()).await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/api/v2/resources");
            then.status(401);
        })
        .await;
    assert!(service(&server)
        .refresh_account_servers(&pool)
        .await
        .is_err());
    assert!(setting(&pool, PLEX_USER_AUTH_TOKEN_KEY).await.is_none());
    assert!(setting(&pool, PLEX_CLIENT_IDENTIFIER_KEY).await.is_none());
    assert_eq!(setting(&pool, PENDING_FLOW_KEY).await, Some(encoded));
    let missing: Option<String> =
        sqlx::query_scalar(r#"SELECT "MissingSince" FROM "PlexMediaSource" WHERE "Id" = 1"#)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(missing.is_none());
}

#[tokio::test]
async fn unavailable_discovery_preserves_credentials_and_inventory() {
    let pool = pool().await;
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/api/v2/resources");
            then.status(503);
        })
        .await;
    assert!(service(&server)
        .refresh_account_servers(&pool)
        .await
        .is_err());
    assert_old_credentials(&pool).await;
    let missing: Option<String> =
        sqlx::query_scalar(r#"SELECT "MissingSince" FROM "PlexMediaSource" WHERE "Id" = 1"#)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(missing.is_none());
}

#[tokio::test]
async fn late_unauthorized_response_cannot_clear_replacement_credentials() {
    let pool = pool().await;
    clear_rejected_credentials(
        &pool,
        &(
            Some("earlier-client".to_owned()),
            Some("earlier-token".to_owned()),
        ),
    )
    .await
    .unwrap();
    assert_old_credentials(&pool).await;
}

async fn pool() -> DbPool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    sqlx::raw_sql(
        r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES
        ('plex.client_identifier', 'synthetic-old-client'),
        ('plex.user_auth_token', 'synthetic-old-token');
        INSERT INTO "MediaSource" ("Id") VALUES (1);
        INSERT INTO "PlexMediaSource" ("Id", "ClientIdentifier") VALUES (1, 'synthetic-server');"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    pool
}

fn service(server: &MockServer) -> PlexService {
    PlexService {
        client: Client::new(),
        plex_tv_base: server.base_url(),
    }
}

async fn setting(pool: &DbPool, key: &str) -> Option<String> {
    sqlx::query_scalar(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#)
        .bind(key)
        .fetch_optional(pool)
        .await
        .unwrap()
}

async fn write_flow(pool: &DbPool, flow: &PendingFlow) -> String {
    let encoded = serde_json::to_string(flow).unwrap();
    sqlx::query(r#"REPLACE INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)"#)
        .bind(PENDING_FLOW_KEY)
        .bind(&encoded)
        .execute(pool)
        .await
        .unwrap();
    encoded
}

fn flow() -> PendingFlow {
    PendingFlow {
        id: "42".to_owned(),
        code: "synthetic-pin".to_owned(),
        client_identifier: "synthetic-new-client".to_owned(),
        expires_at: Utc::now().timestamp() + PIN_LIFETIME_SECONDS,
    }
}

async fn assert_old_credentials(pool: &DbPool) {
    assert_eq!(
        setting(pool, PLEX_CLIENT_IDENTIFIER_KEY).await.as_deref(),
        Some("synthetic-old-client")
    );
    assert_eq!(
        setting(pool, PLEX_USER_AUTH_TOKEN_KEY).await.as_deref(),
        Some("synthetic-old-token")
    );
    let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "PlexMediaSource""#)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn forced_sign_in_keeps_old_credentials_until_validated_completion_after_restart() {
    let pool = pool().await;
    let server = MockServer::start_async().await;
    let start = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/api/v2/pins")
                .query_param("strong", "true")
                .header("X-Plex-Product", "ErsatzRS")
                .header("X-Plex-Version", env!("CARGO_PKG_VERSION"));
            then.status(200)
                .json_body(json!({"id":42,"code":"synthetic-pin"}));
        })
        .await;
    let pin = service(&server)
        .start_account_authentication(&pool, true)
        .await
        .unwrap();
    assert_old_credentials(&pool).await;
    let encoded = setting(&pool, PENDING_FLOW_KEY).await.unwrap();
    let pending: PendingFlow = serde_json::from_str(&encoded).unwrap();
    assert_ne!(pending.client_identifier, "synthetic-old-client");
    assert!(pin.auth_url.contains(&pending.client_identifier));
    let pin_request = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/api/v2/pins/42")
                .query_param("code", "synthetic-pin")
                .header("X-Plex-Client-Identifier", &pending.client_identifier);
            then.status(200)
                .json_body(json!({"authToken":"synthetic-pin-token"}));
        })
        .await;
    let user = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/api/v2/user")
                .header("X-Plex-Token", "synthetic-pin-token")
                .header("X-Plex-Client-Identifier", &pending.client_identifier)
                .header("X-Plex-Product", "ErsatzRS")
                .header("X-Plex-Version", env!("CARGO_PKG_VERSION"));
            then.status(200)
                .json_body(json!({"authToken":"synthetic-account-token"}));
        })
        .await;
    // A new adapter has no in-memory PIN state: the persisted flow binds identity.
    assert!(service(&server)
        .complete_account_authentication(&pool, &pin.id)
        .await
        .unwrap());
    assert_eq!(
        setting(&pool, PLEX_CLIENT_IDENTIFIER_KEY).await,
        Some(pending.client_identifier)
    );
    assert_eq!(
        setting(&pool, PLEX_USER_AUTH_TOKEN_KEY).await.as_deref(),
        Some("synthetic-account-token")
    );
    assert!(setting(&pool, PENDING_FLOW_KEY).await.is_none());
    start.assert_async().await;
    pin_request.assert_async().await;
    user.assert_async().await;
}

#[tokio::test]
async fn pending_null_token_does_not_change_credentials_or_consume_flow() {
    let pool = pool().await;
    let server = MockServer::start_async().await;
    let encoded = write_flow(&pool, &flow()).await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/api/v2/pins/42");
            then.status(200).json_body(json!({"authToken":null}));
        })
        .await;
    assert!(!service(&server)
        .complete_account_authentication(&pool, "42")
        .await
        .unwrap());
    assert_eq!(setting(&pool, PENDING_FLOW_KEY).await, Some(encoded));
    assert_old_credentials(&pool).await;
}

#[tokio::test]
async fn expired_pin_and_wrong_pin_never_contact_provider_or_replace_credentials() {
    let pool = pool().await;
    let server = MockServer::start_async().await;
    let probe = server
        .mock_async(|when, then| {
            when.method(GET);
            then.status(500);
        })
        .await;
    let mut pending = flow();
    let encoded = write_flow(&pool, &pending).await;
    assert!(service(&server)
        .complete_account_authentication(&pool, "wrong")
        .await
        .is_err());
    assert_eq!(setting(&pool, PENDING_FLOW_KEY).await, Some(encoded));
    pending.expires_at = Utc::now().timestamp() - 1;
    write_flow(&pool, &pending).await;
    assert!(service(&server)
        .complete_account_authentication(&pool, "42")
        .await
        .is_err());
    assert!(setting(&pool, PENDING_FLOW_KEY).await.is_none());
    assert_old_credentials(&pool).await;
    probe.assert_calls_async(0).await;
}

#[tokio::test]
async fn rejected_account_clears_only_pending_flow_and_retains_working_credentials() {
    let pool = pool().await;
    let server = MockServer::start_async().await;
    write_flow(&pool, &flow()).await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/api/v2/pins/42");
            then.status(200)
                .json_body(json!({"authToken":"synthetic-rejected-token"}));
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/api/v2/user");
            then.status(401)
                .body("synthetic-rejected-token must not appear in an error");
        })
        .await;
    let error = service(&server)
        .complete_account_authentication(&pool, "42")
        .await
        .unwrap_err();
    assert!(!error.to_string().contains("synthetic-rejected-token"));
    assert!(setting(&pool, PENDING_FLOW_KEY).await.is_none());
    assert_old_credentials(&pool).await;
}

#[tokio::test]
async fn superseded_completion_cannot_consume_or_overwrite_newer_flow() {
    let pool = pool().await;
    let old = flow();
    let encoded = write_flow(&pool, &old).await;
    let mut newer = flow();
    newer.id = "43".to_owned();
    let newer_encoded = write_flow(&pool, &newer).await;
    assert!(
        commit_credentials(&pool, &encoded, &old, "synthetic-stale-token")
            .await
            .is_err()
    );
    clear_pending(&pool, &encoded).await.unwrap();
    assert_eq!(setting(&pool, PENDING_FLOW_KEY).await, Some(newer_encoded));
    assert_old_credentials(&pool).await;
}

#[tokio::test]
async fn failed_credential_write_rolls_back_identity_and_pending_claim() {
    let pool = pool().await;
    let pending = flow();
    let encoded = write_flow(&pool, &pending).await;
    sqlx::raw_sql(
        r#"CREATE TRIGGER "RejectSyntheticCredential" BEFORE INSERT ON "ConfigElement"
        WHEN NEW."Key" = 'plex.user_auth_token'
        BEGIN SELECT RAISE(ABORT, 'synthetic write failure'); END;"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    assert!(
        commit_credentials(&pool, &encoded, &pending, "synthetic-new-token")
            .await
            .is_err()
    );
    assert_eq!(setting(&pool, PENDING_FLOW_KEY).await, Some(encoded));
    assert_old_credentials(&pool).await;
}
