//! Small Jellyfin JSON fixture using the web host's existing HTTP runtime.
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use axum::{routing::get, Json, Router};
use ersatzrs_domain::MediaStreamKind;
use ersatzrs_infra::ffprobe::LocalMediaProbe;
use serde_json::json;
use sqlx::{sqlite::SqlitePoolOptions, Executor as _, SqlitePool};
use tokio::{net::TcpListener, sync::oneshot, task::JoinHandle};

pub(super) const PROVIDER_PATH: &str = "/synthetic-provider/movie.ts";

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
        let ticks = source.media.version.duration.num_nanoseconds().unwrap() / 100;
        let mut streams = source
            .media
            .streams
            .iter()
            .map(|stream| {
                json!({
                    "Index":stream.index,"Type":match stream.kind {
                        MediaStreamKind::Video=>"Video",MediaStreamKind::Audio=>"Audio",
                        MediaStreamKind::Subtitle=>"Subtitle",_=>unreachable!(),
                    },"Codec":stream.codec,"Language":stream.language,"IsDefault":stream.default,
                    "IsForced":stream.forced,"Width":stream.width,"Height":stream.height,
                    "Channels":stream.channels
                })
            })
            .collect::<Vec<_>>();
        if matches!(declaration, Declaration::NativeIndexMismatch) {
            streams[3]["Index"] = json!(4);
        }
        let selected_path = if matches!(declaration, Declaration::SelectedPathMismatch) {
            "/synthetic-provider/other.ts"
        } else {
            PROVIDER_PATH
        };
        let items = json!({"TotalRecordCount":1,"Items":[{
            "Name":"Synthetic teletext movie","Id":"synthetic-movie","Type":"Movie",
            "Etag":"synthetic-v1","Path":PROVIDER_PATH,"DateCreated":"2026-01-01T00:00:00Z",
            "RunTimeTicks":ticks,"LocationType":"FileSystem"
        }]});
        let playback = json!({"MediaSources":[{"Id":"synthetic-version","Path":selected_path,
            "RunTimeTicks":ticks,"MediaStreams":streams}]});
        let counts = Arc::new([AtomicUsize::new(0), AtomicUsize::new(0)]);
        let items_count = counts.clone();
        let playback_count = counts.clone();
        let app = Router::new()
            .route(
                "/Items",
                get(move || {
                    let body = items.clone();
                    let counts = items_count.clone();
                    async move {
                        counts[0].fetch_add(1, Ordering::SeqCst);
                        Json(body)
                    }
                }),
            )
            .route(
                "/Items/synthetic-movie/PlaybackInfo",
                get(move || {
                    let body = playback.clone();
                    let counts = playback_count.clone();
                    async move {
                        counts[1].fetch_add(1, Ordering::SeqCst);
                        Json(body)
                    }
                }),
            );
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

pub(super) async fn database(local: &std::path::Path, address: &str) -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    pool.execute("PRAGMA foreign_keys=ON").await.unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    sqlx::raw_sql(r#"
        INSERT INTO "MediaSource" ("Id") VALUES (1);
        INSERT INTO "JellyfinMediaSource" ("Id","ServerName") VALUES (1,'Synthetic fixture');
        INSERT INTO "Library" ("Id","Name","MediaKind","MediaSourceId") VALUES (1,'Synthetic',1,1);
        INSERT INTO "JellyfinLibrary" ("Id","ItemId","ShouldSyncItems") VALUES (1,'synthetic-library',1);
        INSERT INTO "ConfigElement" ("Key","Value") VALUES ('jellyfin.current_server_api_key','synthetic-test-only');
    "#).execute(&pool).await.unwrap();
    sqlx::query(
        r#"INSERT INTO "JellyfinConnection" ("Address","JellyfinMediaSourceId") VALUES (?,1)"#,
    )
    .bind(address)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(r#"INSERT INTO "JellyfinPathReplacement" ("JellyfinPath","LocalPath","JellyfinMediaSourceId")
        VALUES ('/synthetic-provider',?,1)"#).bind(local.to_str().unwrap()).execute(&pool).await.unwrap();
    pool
}
