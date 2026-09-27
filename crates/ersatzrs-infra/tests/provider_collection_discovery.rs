mod common;

use ersatzrs_core::repositories::{EmbyConnectionConfig, JellyfinConnectionConfig, Result};
use ersatzrs_infra::{db::assert_sql_safe, emby, jellyfin, plex};
use httpmock::{prelude::*, Mock};
use serde_json::{json, Value};
use sqlx::{Executor, SqlitePool};

#[derive(Clone, Copy)]
enum Provider {
    Plex,
    Jellyfin,
    Emby,
}

impl Provider {
    fn name(self) -> &'static str {
        match self {
            Self::Plex => "Plex",
            Self::Jellyfin => "Jellyfin",
            Self::Emby => "Emby",
        }
    }
    fn collection_key(self) -> &'static str {
        if matches!(self, Self::Plex) {
            "900"
        } else {
            "collection-1"
        }
    }
    fn list(self, root: &str, total: i32, present: bool, name: &str) -> Value {
        if matches!(self, Self::Plex) {
            let items = if present {
                vec![
                    json!({"ratingKey": "900", "title": name, "librarySectionId": root.parse::<i32>().unwrap()}),
                ]
            } else {
                vec![]
            };
            json!({"MediaContainer": {"totalSize": total, "Metadata": items}})
        } else {
            let items = if present {
                vec![json!({"Id": "collection-1", "Name": name})]
            } else {
                vec![]
            };
            json!({"TotalRecordCount": total, "Items": items})
        }
    }
    fn members(self, total: i32) -> Value {
        if matches!(self, Self::Plex) {
            json!({"MediaContainer": {"totalSize": total, "Metadata": [{"key": "/library/metadata/400", "type": "movie"}]}})
        } else {
            json!({"TotalRecordCount": total, "Items": [{"Id": "movie-1", "Type": "Movie", "LocationType": "FileSystem"}]})
        }
    }
}

struct Fixture {
    provider: Provider,
    pool: SqlitePool,
    server: MockServer,
    database_dir: tempfile::TempDir,
}

impl Fixture {
    async fn new(provider: Provider) -> Self {
        let database_dir = tempfile::tempdir().unwrap();
        let url = format!(
            "sqlite://{}?mode=rwc",
            database_dir.path().join("discovery.db").display()
        );
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
        common::seed_media_path(&pool, 1, 1, 1).await;
        pool.execute(r#"DELETE FROM "LocalMediaSource" WHERE "Id" = 1"#)
            .await
            .unwrap();
        common::seed_media_item(&pool, 1, 1).await;
        pool.execute(r#"INSERT INTO "Movie" ("Id") VALUES (1)"#)
            .await
            .unwrap();
        pool.execute(r#"INSERT INTO "MovieMetadata" ("Id", "MovieId", "MetadataKind", "DateAdded", "DateUpdated") VALUES (1, 1, 1, '2026-09-27T00:00:00Z', '2026-09-27T00:00:00Z')"#).await.unwrap();
        let (column, key) = if matches!(provider, Provider::Plex) {
            ("Key", "/library/metadata/400")
        } else {
            ("ItemId", "movie-1")
        };
        sqlx::query(assert_sql_safe(format!(
            r#"INSERT INTO "{}Movie" ("Id", "{column}") VALUES (1, ?)"#,
            provider.name()
        )))
        .bind(key)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(r#"INSERT INTO "Tag" ("MovieMetadataId", "Name", "ExternalTypeId", "ExternalCollectionId") VALUES (1, 'Other owner', '319', ?)"#)
            .bind(provider.collection_key()).execute(&pool).await.unwrap();
        pool.execute(r#"INSERT INTO "MediaVersion" ("Id", "MovieId", "DateAdded", "DateUpdated", "Duration", "Height", "Width", "VideoScanKind") VALUES (1, 1, '2026-09-27T00:00:00Z', '2026-09-27T00:00:00Z', '00:01:00', 1080, 1920, 0)"#).await.unwrap();
        pool.execute(r#"INSERT INTO "MediaStream" ("Id", "MediaVersionId", "Index", "MediaStreamKind", "Codec", "Language", "Channels", "Default", "Forced") VALUES (1, 1, 1, 1, 'aac', 'fra', 2, 1, 0), (2, 1, 2, 2, 'subrip', 'nld', 0, 0, 1)"#).await.unwrap();
        Self {
            provider,
            pool,
            server: MockServer::start_async().await,
            database_dir,
        }
    }

    async fn restart(&mut self) {
        self.pool.close().await;
        let url = format!(
            "sqlite://{}",
            self.database_dir.path().join("discovery.db").display()
        );
        self.pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .unwrap();
        sqlx::migrate!("../../migrations")
            .run(&self.pool)
            .await
            .unwrap();
    }

    async fn sync(&self, root: &str) -> Result<usize> {
        self.sync_at(&self.server.base_url(), root).await
    }

    async fn sync_at(&self, address: &str, root: &str) -> Result<usize> {
        match self.provider {
            Provider::Plex => {
                plex::sync_plex_collection_library(
                    &self.pool,
                    &plex::PlexService::new(),
                    &plex::PlexConnectionConfig {
                        uri: address.to_owned(),
                        token: "synthetic".into(),
                    },
                    1,
                    root,
                )
                .await
            }
            Provider::Jellyfin => {
                jellyfin::sync_jellyfin_collection_library(
                    &self.pool,
                    &jellyfin::JellyfinService::new(),
                    &JellyfinConnectionConfig {
                        address: address.to_owned(),
                        authorization_header: "synthetic".into(),
                    },
                    1,
                    root,
                )
                .await
            }
            Provider::Emby => {
                emby::sync_emby_collection_library(
                    &self.pool,
                    &emby::EmbyService::new(),
                    &EmbyConnectionConfig {
                        address: address.to_owned(),
                        api_key: "synthetic".into(),
                    },
                    1,
                    root,
                )
                .await
            }
        }
    }

    async fn page(&self, root: &str, members: bool, status: u16, body: Value) -> Mock<'_> {
        self.page_at(root, members, 0, status, body).await
    }

    async fn page_at(
        &self,
        root: &str,
        members: bool,
        start: i32,
        status: u16,
        body: Value,
    ) -> Mock<'_> {
        self.server
            .mock_async(|when, then| {
                let when = when.method(GET).query_param(
                    if matches!(self.provider, Provider::Plex) {
                        "X-Plex-Container-Start"
                    } else {
                        "startIndex"
                    },
                    start.to_string(),
                );
                if matches!(self.provider, Provider::Plex) {
                    if members {
                        when.path("/library/collections/900/children");
                    } else {
                        when.path("/library/all").query_param("type", "18");
                    }
                } else {
                    when.path("/Items")
                        .query_param("parentId", if members { "collection-1" } else { root })
                        .query_param(
                            "includeItemTypes",
                            if members {
                                "Movie,Series,Season,Episode"
                            } else {
                                "BoxSet"
                            },
                        );
                }
                then.status(status).json_body(body);
            })
            .await
    }

    async fn state(&self) -> Vec<String> {
        sqlx::query_scalar(assert_sql_safe(format!(r#"
            SELECT 'collection:' || json_object('id', "Id", 'name', "Name") FROM "Collection"
            UNION ALL SELECT 'provider:' || json_object('id', "Id", 'source', "MediaSourceId", 'name', "Name", 'etag', "Etag") FROM "{}Collection"
            UNION ALL SELECT 'member:' || json_object('id', "Id", 'collection', "CollectionId", 'media', "MediaItemId", 'index', "CustomIndex", 'audio', "PreferredAudioLanguageCode", 'subtitle', "PreferredSubtitleLanguageCode", 'mode', "SubtitleMode") FROM "CollectionItem"
            UNION ALL SELECT 'tag:' || json_object('id', "Id", 'name', "Name", 'type', "ExternalTypeId", 'key', "ExternalCollectionId", 'owner', "MovieMetadataId") FROM "Tag"
            UNION ALL SELECT 'root:' || json_object('provider', "Provider", 'source', "MediaSourceId", 'root', "RootId", 'collection', "CollectionId") FROM "ProviderCollectionDiscovery"
            UNION ALL SELECT 'refresh:' || json_object('id', "Id", 'media', "ChangedMediaIds", 'playouts', "PreviousPlayoutIds") FROM "ProviderCollectionRefreshResolution"
            UNION ALL SELECT 'track:' || json_object('id', "Id", 'version', "MediaVersionId", 'index', "Index", 'kind', "MediaStreamKind", 'codec', "Codec", 'language', "Language", 'channels', "Channels", 'default', "Default", 'forced', "Forced") FROM "MediaStream"
            ORDER BY 1"#, self.provider.name())))
            .fetch_all(&self.pool).await.unwrap()
    }

    async fn count(&self, table: &'static str) -> i64 {
        sqlx::query_scalar(assert_sql_safe(format!(
            r#"SELECT COUNT(*) FROM "{table}""#
        )))
        .fetch_one(&self.pool)
        .await
        .unwrap()
    }
}

async fn preserve_failed_roots(f: &Fixture, before: &[String]) {
    for (members, status, body) in [
        (false, 503, json!({})),
        (false, 200, f.provider.list("4", 2, true, "Changed")),
        (false, 200, json!({})),
        (true, 503, json!({})),
        (true, 200, f.provider.members(2)),
        (
            true,
            200,
            if matches!(f.provider, Provider::Plex) {
                json!({"MediaContainer": {"totalSize": 1, "Metadata": [{"type": "movie"}]}})
            } else {
                json!({"TotalRecordCount": 1, "Items": [{"Type": "Movie", "LocationType": "FileSystem"}]})
            },
        ),
    ] {
        let list = f
            .page(
                "4",
                false,
                if members { 200 } else { status },
                if members {
                    f.provider.list("4", 1, true, "Changed")
                } else {
                    body.clone()
                },
            )
            .await;
        let items = f
            .page(
                "4",
                true,
                if members { status } else { 200 },
                if members { body } else { f.provider.members(1) },
            )
            .await;
        assert!(f.sync("4").await.is_err());
        assert_eq!(f.state().await, before);
        list.delete_async().await;
        items.delete_async().await;
    }
    let list = f
        .page("4", false, 200, f.provider.list("4", 1, true, "Changed"))
        .await;
    let items = f.page("4", true, 200, f.provider.members(1)).await;
    f.pool.execute(r#"CREATE TRIGGER "FailDiscoveryWrite" BEFORE DELETE ON "ProviderCollectionDiscovery" BEGIN SELECT RAISE(ABORT, 'synthetic failure'); END"#).await.unwrap();
    assert!(f.sync("4").await.is_err());
    assert_eq!(f.state().await, before);
    f.pool
        .execute(r#"DROP TRIGGER "FailDiscoveryWrite""#)
        .await
        .unwrap();
    f.pool.execute(r#"CREATE TRIGGER "FailRefreshPublication" BEFORE INSERT ON "ProviderCollectionRefreshResolution"
        BEGIN SELECT RAISE(ABORT, 'synthetic handoff failure'); END"#).await.unwrap();
    assert!(f.sync("4").await.is_err());
    assert_eq!(
        f.state().await,
        before,
        "handoff failure must roll back collection publication"
    );
    f.pool
        .execute(r#"DROP TRIGGER "FailRefreshPublication""#)
        .await
        .unwrap();
    list.delete_async().await;
    items.delete_async().await;
}

async fn lifecycle(provider: Provider) {
    let mut f = Fixture::new(provider).await;
    let list = f
        .page("4", false, 200, provider.list("4", 1, true, "Original"))
        .await;
    let members = f.page("4", true, 200, provider.members(1)).await;
    assert_eq!(f.sync("4").await.unwrap(), 1);
    let id: i64 = sqlx::query_scalar(r#"SELECT "Id" FROM "Collection""#)
        .fetch_one(&f.pool)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "RerunCollection" ("CollectionId", "CollectionType", "FirstRunPlaybackOrder", "RerunPlaybackOrder", "Name") VALUES (?, 0, 0, 0, 'Keep reference')"#)
        .bind(id).execute(&f.pool).await.unwrap();
    f.pool.execute(r#"UPDATE "CollectionItem" SET "PreferredAudioLanguageCode" = 'fra', "PreferredSubtitleLanguageCode" = 'nld', "SubtitleMode" = 1"#).await.unwrap();
    let before = f.state().await;
    assert_eq!(f.count("ProviderCollectionRefreshResolution").await, 1);
    f.sync("4").await.unwrap();
    assert_eq!(
        f.state().await,
        before,
        "unchanged discovery preserves membership identity and playback choices"
    );
    list.delete_async().await;
    members.delete_async().await;
    f.restart().await;
    assert_eq!(f.state().await, before);
    preserve_failed_roots(&f, &before).await;
    let empty = f
        .page("4", false, 200, provider.list("4", 0, false, ""))
        .await;
    assert_eq!(f.sync("4").await.unwrap(), 0);
    assert_eq!(f.count("CollectionItem").await, 0);
    assert_eq!(f.count("ProviderCollectionDiscovery").await, 0);
    assert_eq!(f.count("ProviderCollectionRefreshResolution").await, 2);
    assert_eq!(f.count("Collection").await, 1);
    assert_eq!(f.count("RerunCollection").await, 1);
    assert_eq!(f.count("MovieMetadata").await, 1);
    let tracks: Vec<_> = f
        .state()
        .await
        .into_iter()
        .filter(|row| row.starts_with("track:"))
        .collect();
    assert_eq!(
        tracks,
        before
            .iter()
            .filter(|row| row.starts_with("track:"))
            .cloned()
            .collect::<Vec<_>>()
    );
    assert_eq!(f.count("Tag").await, 1);
    empty.delete_async().await;
    let _list = f
        .page("4", false, 200, provider.list("4", 1, true, "Returned"))
        .await;
    let _members = f.page("4", true, 200, provider.members(1)).await;
    assert_eq!(f.sync("4").await.unwrap(), 1);
    let restored: i64 =
        sqlx::query_scalar(r#"SELECT "Id" FROM "Collection" WHERE "Name" = 'Returned'"#)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert_eq!(restored, id);
    assert_eq!(f.count("ProviderCollectionRefreshResolution").await, 3);
    assert_eq!(f.count("CollectionItem").await, 1);
    assert_eq!(f.count("Tag").await, 2);
    assert_eq!(f.count("RerunCollection").await, 1);
}

async fn multiple_roots(provider: Provider) {
    let f = Fixture::new(provider).await;
    let _members = f.page("4", true, 200, provider.members(1)).await;
    for root in ["4", "5"] {
        let list = f
            .page(root, false, 200, provider.list(root, 1, true, "Shared"))
            .await;
        f.sync(root).await.unwrap();
        list.delete_async().await;
    }
    assert_eq!(f.count("ProviderCollectionDiscovery").await, 2);
    let tags = common::collection_tag_rows(
        &f.pool,
        &format!("{}/collection", provider.name().to_lowercase()),
    )
    .await;
    let first = f
        .page("4", false, 200, provider.list("4", 0, false, ""))
        .await;
    f.sync("4").await.unwrap();
    assert_eq!(f.count("CollectionItem").await, 1);
    assert_eq!(f.count("ProviderCollectionDiscovery").await, 1);
    assert_eq!(
        common::collection_tag_rows(
            &f.pool,
            &format!("{}/collection", provider.name().to_lowercase())
        )
        .await,
        tags
    );
    first.delete_async().await;
    let _last = f
        .page("5", false, 200, provider.list("5", 0, false, ""))
        .await;
    f.sync("5").await.unwrap();
    assert_eq!(f.count("CollectionItem").await, 0);
    assert_eq!(f.count("Tag").await, 1);
}

#[tokio::test]
async fn plex_collection_discovery_lifecycle() {
    lifecycle(Provider::Plex).await;
}
#[tokio::test]
async fn jellyfin_collection_discovery_lifecycle() {
    lifecycle(Provider::Jellyfin).await;
}
#[tokio::test]
async fn emby_collection_discovery_lifecycle() {
    lifecycle(Provider::Emby).await;
}
#[tokio::test]
async fn plex_collection_discovery_multiple_roots() {
    multiple_roots(Provider::Plex).await;
}
#[tokio::test]
async fn jellyfin_collection_discovery_multiple_roots() {
    multiple_roots(Provider::Jellyfin).await;
}
#[tokio::test]
async fn emby_collection_discovery_multiple_roots() {
    multiple_roots(Provider::Emby).await;
}

async fn paged_members(provider: Provider) {
    let f = Fixture::new(provider).await;
    let _list = f
        .page("4", false, 200, provider.list("4", 1, true, "Paged"))
        .await;
    let first_body = if matches!(provider, Provider::Plex) {
        let items: Vec<_> = (0..100)
            .map(|id| json!({"key": format!("artist-{id}"), "type": "artist"}))
            .collect();
        json!({"MediaContainer": {"totalSize": 101, "Metadata": items}})
    } else {
        let items: Vec<_> = (0..100).map(|id| json!({"Id": format!("artist-{id}"), "Type": "MusicArtist", "LocationType": "FileSystem"})).collect();
        json!({"TotalRecordCount": 101, "Items": items})
    };
    let first = f.page_at("4", true, 0, 200, first_body).await;
    let last = f.page_at("4", true, 100, 200, provider.members(101)).await;
    assert_eq!(f.sync("4").await.unwrap(), 1);
    assert_eq!(f.count("CollectionItem").await, 1);
    first.assert_calls_async(1).await;
    last.assert_calls_async(1).await;
    let before = f.state().await;
    last.delete_async().await;
    let _changed = f.page_at("4", true, 100, 200, provider.members(102)).await;
    assert!(f.sync("4").await.is_err());
    assert_eq!(f.state().await, before);
}

#[tokio::test]
async fn plex_collection_discovery_validates_raw_pagination() {
    paged_members(Provider::Plex).await;
}
#[tokio::test]
async fn jellyfin_collection_discovery_validates_raw_pagination() {
    paged_members(Provider::Jellyfin).await;
}
#[tokio::test]
async fn emby_collection_discovery_validates_raw_pagination() {
    paged_members(Provider::Emby).await;
}

// A controlled two-request HTTP fixture holds the member response until the
// test aborts its own task. No sleep or provider timeout is needed for cancellation.
async fn read_fixture_headers(stream: &mut tokio::net::TcpStream) {
    use tokio::io::AsyncReadExt as _;
    let mut header = Vec::new();
    loop {
        let mut buffer = [0_u8; 512];
        let count = stream.read(&mut buffer).await.unwrap();
        assert_ne!(count, 0, "fixture request ended before its headers");
        header.extend_from_slice(&buffer[..count]);
        assert!(
            header.len() <= 8192,
            "fixture request headers exceed the bound"
        );
        if header.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
            break;
        }
    }
}

async fn cancellation(provider: Provider) {
    use tokio::io::AsyncWriteExt as _;
    let mut f = Fixture::new(provider).await;
    let _list = f
        .page("4", false, 200, provider.list("4", 1, true, "Original"))
        .await;
    let _members = f.page("4", true, 200, provider.members(1)).await;
    f.sync("4").await.unwrap();
    let before = f.state().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let (reached, waiting) = tokio::sync::oneshot::channel();
    let response = provider.list("4", 1, true, "Cancelled").to_string();
    let server = tokio::spawn(async move {
        let (mut listing, _) = listener.accept().await.unwrap();
        read_fixture_headers(&mut listing).await;
        let headers = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", response.len());
        listing.write_all(headers.as_bytes()).await.unwrap();
        listing.write_all(response.as_bytes()).await.unwrap();
        listing.shutdown().await.unwrap();
        let (mut members, _) = listener.accept().await.unwrap();
        read_fixture_headers(&mut members).await;
        reached.send(()).unwrap();
        std::future::pending::<()>().await;
    });
    {
        let sync = f.sync_at(&address, "4");
        tokio::pin!(sync);
        tokio::select! {
            result = &mut sync => panic!("sync completed before cancellation: {result:?}"),
            reached = tokio::time::timeout(std::time::Duration::from_secs(10), waiting) => {
                reached.expect("member request reached the fixture").unwrap();
            }
        }
        // Dropping the in-flight sync cancels it while a member response is pending.
    }
    server.abort();
    assert!(server.await.unwrap_err().is_cancelled());
    assert_eq!(f.state().await, before);
    f.restart().await;
    assert_eq!(f.state().await, before);
}

#[tokio::test]
async fn plex_collection_discovery_cancel_preserves_published_state() {
    cancellation(Provider::Plex).await;
}
#[tokio::test]
async fn jellyfin_collection_discovery_cancel_preserves_published_state() {
    cancellation(Provider::Jellyfin).await;
}
#[tokio::test]
async fn emby_collection_discovery_cancel_preserves_published_state() {
    cancellation(Provider::Emby).await;
}

#[tokio::test]
async fn collection_refresh_removes_stranded_owned_tags_outside_previous_membership() {
    for provider in [Provider::Plex, Provider::Jellyfin, Provider::Emby] {
        let mut f = Fixture::new(provider).await;
        let _list = f
            .page("4", false, 200, provider.list("4", 1, true, "Current"))
            .await;
        let _members = f.page("4", true, 200, provider.members(1)).await;
        f.sync("4").await.unwrap();
        common::seed_media_item(&f.pool, 2, 1).await;
        f.pool.execute(r#"INSERT INTO "Movie" ("Id") VALUES (2);
            INSERT INTO "MovieMetadata" ("Id", "MovieId", "MetadataKind", "DateAdded", "DateUpdated")
            VALUES (2, 2, 1, '2026-09-27T00:00:00Z', '2026-09-27T00:00:00Z')"#).await.unwrap();
        let kind = format!("{}/collection", provider.name().to_lowercase());
        for owner in [kind.as_str(), "319"] {
            sqlx::query(r#"INSERT INTO "Tag" ("MovieMetadataId", "Name", "ExternalTypeId", "ExternalCollectionId")
                VALUES (2, 'Stranded', ?, ?)"#).bind(owner).bind(provider.collection_key())
                .execute(&f.pool).await.unwrap();
        }
        f.sync("4").await.unwrap();
        f.restart().await;
        let types: Vec<String> = sqlx::query_scalar(
            r#"SELECT "ExternalTypeId" FROM "Tag" WHERE "MovieMetadataId" = 2 ORDER BY "Id""#,
        )
        .fetch_all(&f.pool)
        .await
        .unwrap();
        assert_eq!(
            types,
            ["319"],
            "only the stranded provider-owned tag should be removed"
        );
    }
}

#[tokio::test]
async fn admin_collection_delete_removes_owned_tags_and_rolls_back_as_one_transaction() {
    use ersatzrs_core::repositories::CollectionRepository;
    for provider in [Provider::Plex, Provider::Jellyfin, Provider::Emby] {
        let mut f = Fixture::new(provider).await;
        let _list = f
            .page("4", false, 200, provider.list("4", 1, true, "Current"))
            .await;
        let _members = f.page("4", true, 200, provider.members(1)).await;
        f.sync("4").await.unwrap();
        let id: i64 = sqlx::query_scalar(r#"SELECT "Id" FROM "Collection""#)
            .fetch_one(&f.pool)
            .await
            .unwrap();
        let before = f.state().await;
        let repo = ersatzrs_infra::repositories::SqliteCollectionRepository(f.pool.clone());
        f.pool
            .execute(
                r#"CREATE TRIGGER fail_admin_collection_delete BEFORE DELETE ON "Collection"
            BEGIN SELECT RAISE(ABORT, 'synthetic delete failure'); END"#,
            )
            .await
            .unwrap();
        assert!(repo.delete_admin(id).await.is_err());
        assert_eq!(f.state().await, before);
        f.pool
            .execute("DROP TRIGGER fail_admin_collection_delete")
            .await
            .unwrap();
        repo.delete_admin(id).await.unwrap();
        f.restart().await;
        assert_eq!(f.count("Collection").await, 0);
        assert_eq!(f.count("CollectionItem").await, 0);
        assert_eq!(f.count("ProviderCollectionDiscovery").await, 0);
        assert_eq!(
            f.count("Tag").await,
            1,
            "other producers survive explicit deletion"
        );
        assert_eq!(f.count("MovieMetadata").await, 1);
        assert_eq!(f.count("MediaStream").await, 2);
        assert_eq!(f.count("ProviderCollectionRefreshResolution").await, 2);
    }
}
