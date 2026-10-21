//! Synthetic provider protocols using the web host's existing HTTP runtime.
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use axum::{
    extract::Query,
    http::{StatusCode, Uri},
    response::IntoResponse as _,
    routing::get,
    Json, Router,
};
use ersatzrs_domain::{MediaSourceKind, MediaStreamKind};
use ersatzrs_infra::ffprobe::LocalMediaProbe;
use serde_json::{json, Value};
use sqlx::{sqlite::SqlitePoolOptions, Executor as _, SqlitePool};
use tokio::{net::TcpListener, sync::oneshot, task::JoinHandle};

pub(super) const PROVIDER_PATH: &str = "/synthetic-provider/movie.ts";
const SHOW: &str = "synthetic-show";
const SEASON: &str = "synthetic-season";
pub(super) const PLEX_SHOW_KEY: &str = "/library/metadata/100";

#[derive(Clone, Copy)]
pub(super) enum Declaration {
    Exact,
    SelectedPathMismatch,
    NativeIndexMismatch,
}

pub(super) struct Provider {
    pub(super) address: String,
    counts: Arc<[AtomicUsize; 2]>,
    stop: oneshot::Sender<()>,
    task: JoinHandle<std::io::Result<()>>,
}
impl Provider {
    pub(super) async fn start(source: &LocalMediaProbe) -> Self {
        Self::with_declaration(source, Declaration::Exact).await
    }

    pub(super) async fn with_declaration(
        source: &LocalMediaProbe,
        declaration: Declaration,
    ) -> Self {
        Self::for_action(source, MediaSourceKind::Jellyfin, false, declaration).await
    }

    pub(super) async fn for_action(
        source: &LocalMediaProbe,
        provider: MediaSourceKind,
        show: bool,
        declaration: Declaration,
    ) -> Self {
        assert_ne!(provider, MediaSourceKind::Local);
        let replies = Replies {
            provider,
            show,
            ticks: source.media.version.duration.num_nanoseconds().unwrap() / 100,
            streams: streams(source, provider, declaration),
            selected_path: if matches!(declaration, Declaration::SelectedPathMismatch) {
                "/synthetic-provider/other.ts"
            } else {
                PROVIDER_PATH
            },
        };
        let counts = Arc::new([AtomicUsize::new(0), AtomicUsize::new(0)]);
        let requests = counts.clone();
        let app = Router::new().fallback(get(
            move |uri: Uri, Query(query): Query<BTreeMap<String, String>>| {
                let reply = replies.reply(uri.path(), &query);
                let counts = requests.clone();
                async move {
                    if let Some((body, counter)) = reply {
                        counts[counter].fetch_add(1, Ordering::SeqCst);
                        Json(body).into_response()
                    } else {
                        StatusCode::NOT_FOUND.into_response()
                    }
                }
            },
        ));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let (stop, shutdown) = oneshot::channel();
        let task = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = shutdown.await;
                })
                .await
        });
        Self {
            address,
            counts,
            stop,
            task,
        }
    }

    pub(super) async fn close(self) -> [usize; 2] {
        self.stop.send(()).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(3), self.task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        [
            self.counts[0].load(Ordering::SeqCst),
            self.counts[1].load(Ordering::SeqCst),
        ]
    }
}

fn streams(
    source: &LocalMediaProbe,
    provider: MediaSourceKind,
    declaration: Declaration,
) -> Vec<Value> {
    source
        .media
        .streams
        .iter()
        .map(|stream| {
            let index =
                if stream.index == 3 && matches!(declaration, Declaration::NativeIndexMismatch) {
                    4
                } else {
                    stream.index
                };
            if provider == MediaSourceKind::Plex {
                json!({
                    "id":1000+stream.index,"index":index,"streamType":match stream.kind {
                        MediaStreamKind::Video=>1,MediaStreamKind::Audio=>2,
                        MediaStreamKind::Subtitle=>3,_=>unreachable!(),
                    },"codec":stream.codec,"languageCode":stream.language,"default":stream.default,
                    "forced":stream.forced,"width":stream.width,"height":stream.height,
                    "channels":stream.channels
                })
            } else {
                json!({
                    "Index":index,"Type":match stream.kind {
                        MediaStreamKind::Video=>"Video",MediaStreamKind::Audio=>"Audio",
                        MediaStreamKind::Subtitle=>"Subtitle",_=>unreachable!(),
                    },"Codec":stream.codec,"Language":stream.language,"IsDefault":stream.default,
                    "IsForced":stream.forced,"Width":stream.width,"Height":stream.height,
                    "Channels":stream.channels
                })
            }
        })
        .collect()
}

#[derive(Clone)]
struct Replies {
    provider: MediaSourceKind,
    show: bool,
    ticks: i64,
    streams: Vec<Value>,
    selected_path: &'static str,
}
impl Replies {
    fn reply(&self, path: &str, query: &BTreeMap<String, String>) -> Option<(Value, usize)> {
        if self.provider == MediaSourceKind::Plex {
            return self.plex(path).map(|reply| (reply, 0));
        }
        if path == "/Items/synthetic-movie/PlaybackInfo"
            || path == "/Items/synthetic-episode/PlaybackInfo"
        {
            return Some((
                json!({"MediaSources":[{
                    "Id":"synthetic-version","Path":self.selected_path,
                    "RunTimeTicks":self.ticks,"MediaStreams":self.streams
                }]}),
                1,
            ));
        }
        let kind = match path {
            "/Items" => query
                .get("includeItemTypes")
                .map_or("Movie", String::as_str),
            "/Shows/synthetic-show/Seasons" => "Season",
            "/Shows/synthetic-show/Episodes" => "Episode",
            _ => return None,
        };
        let item = match kind {
            "Series" if self.show => json!({"Id":SHOW,"Name":"Synthetic show","Type":"Series",
                "Etag":"synthetic-show-v1","DateCreated":"2026-01-01T00:00:00Z"}),
            "Season" if self.show => json!({"Id":SEASON,"Name":"Season 1","Type":"Season",
                "IndexNumber":1,"Etag":"synthetic-season-v1","DateCreated":"2026-01-01T00:00:00Z"}),
            "Episode" if self.show => self.item("synthetic-episode", "Episode"),
            "Movie" if !self.show => self.item("synthetic-movie", "Movie"),
            _ => return None,
        };
        Some((json!({"TotalRecordCount":1,"Items":[item]}), 0))
    }

    fn item(&self, id: &str, kind: &str) -> Value {
        json!({"Name":"Synthetic teletext movie","Id":id,"Type":kind,
            "SeasonId":SEASON,"IndexNumber":1,"Etag":"synthetic-v1","Path":PROVIDER_PATH,
            "DateCreated":"2026-01-01T00:00:00Z","RunTimeTicks":self.ticks,"LocationType":"FileSystem"})
    }

    fn plex(&self, path: &str) -> Option<Value> {
        if path == "/library/sections" {
            return Some(json!({"MediaContainer":{"Directory":[{
                "Key":"1","Title":"Synthetic fixture","type":if self.show { "show" } else { "other" },
                "Location":[{"path":"/synthetic-provider"}]
            }]}}));
        }
        if path == "/library/tags" {
            return Some(json!({"MediaContainer":{"totalSize":0,"Directory":[]}}));
        }
        let item = match path {
            "/library/sections/1/all" if !self.show => self.plex_item(102),
            "/library/sections/1/all" | PLEX_SHOW_KEY if self.show => json!({
                "key":PLEX_SHOW_KEY,"ratingKey":"100","title":"Synthetic show","updatedAt":1
            }),
            "/library/metadata/100/children" if self.show => json!({
                "key":"/library/metadata/101","ratingKey":"101","title":"Season 1",
                "index":1,"updatedAt":1
            }),
            "/library/metadata/101/children" if self.show => self.plex_item(102),
            _ => return None,
        };
        Some(json!({"MediaContainer":{"totalSize":1,"Metadata":[item]}}))
    }

    fn plex_item(&self, id: i32) -> Value {
        json!({"key":format!("/library/metadata/{id}"),"ratingKey":id.to_string(),
        "title":"Synthetic teletext movie","index":1,"addedAt":1,"updatedAt":1,
        "Media":[{"id":1,"duration":self.ticks/10_000,"Part":[{
            "id":7,"key":"/library/parts/7","file":self.selected_path,"Stream":self.streams
        }]}]})
    }
}

pub(super) async fn database(local: &std::path::Path, address: &str) -> SqlitePool {
    database_for(local, address, MediaSourceKind::Jellyfin, false).await
}

pub(super) async fn database_for(
    local: &std::path::Path,
    address: &str,
    provider: MediaSourceKind,
    show: bool,
) -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    pool.execute("PRAGMA foreign_keys=ON").await.unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    seed_existing_pool(&pool, local, address, provider, show).await;
    pool
}

pub(super) async fn seed_existing_pool(
    pool: &SqlitePool,
    local: &std::path::Path,
    address: &str,
    provider: MediaSourceKind,
    show: bool,
) {
    sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#)
        .execute(pool)
        .await
        .unwrap();
    let (schema, path_sql) = match provider {
        MediaSourceKind::Jellyfin => (
            r#"
            INSERT INTO "JellyfinMediaSource" ("Id","ServerName") VALUES (1,'Synthetic fixture');
            INSERT INTO "JellyfinLibrary" ("Id","ItemId","ShouldSyncItems") VALUES (1,'synthetic-library',1);
            INSERT INTO "JellyfinConnection" ("JellyfinMediaSourceId") VALUES (1);
            INSERT INTO "ConfigElement" ("Key","Value") VALUES ('jellyfin.current_server_api_key','synthetic-test-only');
        "#,
            r#"INSERT INTO "JellyfinPathReplacement" ("JellyfinPath","LocalPath","JellyfinMediaSourceId") VALUES ('/synthetic-provider',?,1)"#,
        ),
        MediaSourceKind::Emby => (
            r#"
            INSERT INTO "EmbyMediaSource" ("Id","ServerName") VALUES (1,'Synthetic fixture');
            INSERT INTO "EmbyLibrary" ("Id","ItemId","ShouldSyncItems") VALUES (1,'synthetic-library',1);
            INSERT INTO "EmbyConnection" ("EmbyMediaSourceId") VALUES (1);
            INSERT INTO "ConfigElement" ("Key","Value") VALUES ('emby.current_server_api_key','synthetic-test-only');
        "#,
            r#"INSERT INTO "EmbyPathReplacement" ("EmbyPath","LocalPath","EmbyMediaSourceId") VALUES ('/synthetic-provider',?,1)"#,
        ),
        MediaSourceKind::Plex => (
            r#"
            INSERT INTO "PlexMediaSource" ("Id","ClientIdentifier","ServerName") VALUES (1,'synthetic-client','Synthetic fixture');
            INSERT INTO "PlexLibrary" ("Id","Key","ShouldSyncItems") VALUES (1,'1',1);
            INSERT INTO "PlexConnection" ("IsActive","PlexMediaSourceId") VALUES (1,1);
            INSERT INTO "ConfigElement" ("Key","Value") VALUES ('plex.server_auth_token.synthetic-client','synthetic-test-only');
        "#,
            r#"INSERT INTO "PlexPathReplacement" ("PlexPath","LocalPath","PlexMediaSourceId") VALUES ('/synthetic-provider',?,1)"#,
        ),
        MediaSourceKind::Local => panic!("fixture supports provider protocols only"),
    };
    // Provider library subtypes refer to this parent; source subtypes refer to
    // the already inserted MediaSource parent.
    sqlx::query(r#"INSERT INTO "Library" ("Id","Name","MediaKind","MediaSourceId") VALUES (1,'Synthetic',?,1)"#)
        .bind(if show { 2 } else { 1 }).execute(pool).await.unwrap();
    sqlx::raw_sql(schema).execute(pool).await.unwrap();
    sqlx::query(path_sql)
        .bind(local.to_str().unwrap())
        .execute(pool)
        .await
        .unwrap();
    update_connection(pool, address, provider).await;
}

pub(super) async fn update_connection(pool: &SqlitePool, address: &str, provider: MediaSourceKind) {
    let query = match provider {
        MediaSourceKind::Jellyfin => {
            r#"UPDATE "JellyfinConnection" SET "Address"=? WHERE "JellyfinMediaSourceId"=1"#
        }
        MediaSourceKind::Emby => {
            r#"UPDATE "EmbyConnection" SET "Address"=? WHERE "EmbyMediaSourceId"=1"#
        }
        MediaSourceKind::Plex => {
            r#"UPDATE "PlexConnection" SET "Uri"=? WHERE "PlexMediaSourceId"=1"#
        }
        MediaSourceKind::Local => panic!("fixture supports provider protocols only"),
    };
    assert_eq!(
        sqlx::query(query)
            .bind(address)
            .execute(pool)
            .await
            .unwrap()
            .rows_affected(),
        1
    );
}
