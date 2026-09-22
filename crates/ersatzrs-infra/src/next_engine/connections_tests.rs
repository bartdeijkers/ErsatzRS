use super::*;
use crate::next_engine::source::tests::seed_provider;
use httpmock::MockServer;

async fn fixture(address: &str) -> DbPool {
    let pool = crate::next_engine::selection::tests::fixture_pool().await;
    // The shared provider seed expects the two synthetic file rows.
    sqlx::query(
        r#"INSERT INTO "MediaFile" ("Id", "MediaVersionId", "Path", "PathHash") VALUES
        (1, 100, 'media/first.mkv', 'first'), (2, 100, 'media/second.mkv', 'second')"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    seed_provider(&pool, RemoteMediaSourceKind::Plex).await;
    sqlx::query(r#"UPDATE "PlexConnection" SET "Uri" = ? WHERE "Id" = 1"#)
        .bind(address)
        .execute(&pool)
        .await
        .unwrap();
    pool
}

async fn alternate(pool: &DbPool, address: &str) {
    sqlx::query(r#"INSERT INTO "PlexConnection" ("Id", "PlexMediaSourceId", "IsActive", "Uri") VALUES (2, 1, 0, ?)"#)
        .bind(address).execute(pool).await.unwrap();
}

async fn active(pool: &DbPool) -> Vec<i64> {
    sqlx::query_scalar(r#"SELECT "Id" FROM "PlexConnection" WHERE "IsActive" = 1 ORDER BY "Id""#)
        .fetch_all(pool)
        .await
        .unwrap()
}

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}

#[tokio::test]
async fn active_identity_is_verified_and_concurrent_opens_share_the_cache() {
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method("GET")
                .path("/")
                .header("X-Plex-Token", "<token>");
            then.status(200)
                .body(r#"<MediaContainer machineIdentifier="fixture-server"/>"#);
        })
        .await;
    let pool = fixture(&server.base_url()).await;
    let selector = PlexConnections::new().unwrap();
    let (a, b) = tokio::join!(
        selector.resolve(&pool, 1, deadline()),
        selector.resolve(&pool, 1, deadline())
    );
    assert_eq!(a.unwrap().address, server.base_url());
    assert_eq!(b.unwrap().address, server.base_url());
    mock.assert_calls_async(1).await;
    assert_eq!(active(&pool).await, vec![1]);
}

#[tokio::test]
async fn wrong_identity_fails_over_and_only_the_verified_connection_is_activated() {
    let wrong = MockServer::start_async().await;
    let right = MockServer::start_async().await;
    let wrong_mock = wrong
        .mock_async(|when, then| {
            when.path("/");
            then.status(200)
                .body(r#"<MediaContainer machineIdentifier="another-server"/>"#);
        })
        .await;
    let right_mock = right
        .mock_async(|when, then| {
            when.path("/").header("X-Plex-Token", "<token>");
            then.status(200)
                .body(r#"<MediaContainer machineIdentifier="fixture-server"/>"#);
        })
        .await;
    let pool = fixture(&wrong.base_url()).await;
    alternate(&pool, &right.base_url()).await;
    let selector = PlexConnections::new().unwrap();
    assert_eq!(
        selector
            .resolve(&pool, 1, deadline())
            .await
            .unwrap()
            .address,
        right.base_url()
    );
    assert_eq!(active(&pool).await, vec![2]);
    selector.resolve(&pool, 1, deadline()).await.unwrap();
    wrong_mock.assert_calls_async(1).await;
    right_mock.assert_calls_async(1).await;
}

#[tokio::test]
async fn credential_and_address_changes_invalidate_a_warm_cache() {
    let old = MockServer::start_async().await;
    let new = MockServer::start_async().await;
    let old_mock = old
        .mock_async(|when, then| {
            when.path("/").header("X-Plex-Token", "<token>");
            then.status(200)
                .body(r#"<MediaContainer machineIdentifier="fixture-server"/>"#);
        })
        .await;
    let new_mock = new
        .mock_async(|when, then| {
            when.path("/").header("X-Plex-Token", "<replacement-token>");
            then.status(200)
                .body(r#"<MediaContainer machineIdentifier="fixture-server"/>"#);
        })
        .await;
    let pool = fixture(&old.base_url()).await;
    let selector = PlexConnections::new().unwrap();
    selector.resolve(&pool, 1, deadline()).await.unwrap();
    sqlx::query(r#"UPDATE "ConfigElement" SET "Value" = '<replacement-token>' WHERE "Key" = 'plex.server_auth_token.fixture-server'"#)
        .execute(&pool).await.unwrap();
    assert!(matches!(
        selector.resolve(&pool, 1, deadline()).await,
        Err(ConnectionError::Unreachable)
    ));
    sqlx::query(r#"UPDATE "PlexConnection" SET "Uri" = ? WHERE "Id" = 1"#)
        .bind(new.base_url())
        .execute(&pool)
        .await
        .unwrap();
    let result = selector.resolve(&pool, 1, deadline()).await.unwrap();
    assert_eq!(result.address, new.base_url());
    assert_eq!(result.token, "<replacement-token>");
    old_mock.assert_calls_async(1).await;
    new_mock.assert_calls_async(1).await;
}

#[tokio::test]
async fn invalid_or_oversized_identity_responses_leave_active_selection_unchanged() {
    for body in [
        r#"<Other machineIdentifier="fixture-server"/>"#.to_owned(),
        r#"<MediaContainer machineIdentifier="fixture-server""#.to_owned(),
        format!(
            r#"<MediaContainer machineIdentifier="fixture-server">{}</MediaContainer>"#,
            "x".repeat(MAX_RESPONSE_BYTES)
        ),
    ] {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.path("/");
                then.status(200).body(body);
            })
            .await;
        let pool = fixture(&server.base_url()).await;
        assert!(matches!(
            PlexConnections::new()
                .unwrap()
                .resolve(&pool, 1, deadline())
                .await,
            Err(ConnectionError::Unreachable)
        ));
        assert_eq!(active(&pool).await, vec![1]);
    }
}

#[tokio::test]
async fn expired_deadline_and_changed_probe_target_cannot_change_active_state() {
    let pool = fixture("https://media.example.test").await;
    let selector = PlexConnections::new().unwrap();
    assert!(matches!(
        selector
            .resolve(&pool, 1, Instant::now() - Duration::from_secs(1))
            .await,
        Err(ConnectionError::Timeout)
    ));
    let source = SqliteMediaSourceRepository(pool.clone())
        .get_plex(1)
        .await
        .unwrap();
    let selected = source.connections[0].clone();
    sqlx::query(
        r#"UPDATE "PlexConnection" SET "Uri" = 'https://changed.example.test' WHERE "Id" = 1"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    assert!(matches!(
        activate(&pool, &source, &selected).await,
        Err(ConnectionError::Changed)
    ));
    assert_eq!(active(&pool).await, vec![1]);
}

#[tokio::test]
async fn redirects_cannot_forward_credentials_and_a_missing_active_connection_can_recover() {
    let old = MockServer::start_async().await;
    let other = MockServer::start_async().await;
    let other_mock = other
        .mock_async(|when, then| {
            when.path("/");
            then.status(200)
                .body(r#"<MediaContainer machineIdentifier="fixture-server"/>"#);
        })
        .await;
    old.mock_async(|when, then| {
        when.path("/");
        then.status(302).header("Location", other.base_url());
    })
    .await;
    let pool = fixture(&old.base_url()).await;
    let selector = PlexConnections::new().unwrap();
    assert!(matches!(
        selector.resolve(&pool, 1, deadline()).await,
        Err(ConnectionError::Unreachable)
    ));
    other_mock.assert_calls_async(0).await;
    alternate(&pool, &other.base_url()).await;
    sqlx::query(r#"UPDATE "PlexConnection" SET "IsActive" = 0"#)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        selector
            .resolve(&pool, 1, deadline())
            .await
            .unwrap()
            .address,
        other.base_url()
    );
    assert_eq!(active(&pool).await, vec![2]);
}

#[tokio::test]
async fn a_timed_out_probe_releases_discovery_and_keeps_the_old_selection() {
    let server = MockServer::start_async().await;
    let slow = server
        .mock_async(|when, then| {
            when.path("/");
            then.status(200)
                .delay(Duration::from_secs(5))
                .body(r#"<MediaContainer machineIdentifier="fixture-server"/>"#);
        })
        .await;
    let pool = fixture(&server.base_url()).await;
    let selector = PlexConnections::new().unwrap();
    assert!(matches!(
        selector
            .resolve(&pool, 1, Instant::now() + Duration::from_millis(100))
            .await,
        Err(ConnectionError::Timeout)
    ));
    assert_eq!(active(&pool).await, vec![1]);
    slow.delete_async().await;
    let fast = server
        .mock_async(|when, then| {
            when.path("/");
            then.status(200)
                .body(r#"<MediaContainer machineIdentifier="fixture-server"/>"#);
        })
        .await;
    selector.resolve(&pool, 1, deadline()).await.unwrap();
    fast.assert_calls_async(1).await;
}
