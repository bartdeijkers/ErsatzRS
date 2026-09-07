use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, TimeZone, Utc};
use ersatzrs_app::ScanRuntime;
use ersatzrs_core::locking::EntityLocker;
use ersatzrs_core::repositories::{
    ChannelRepository, FfmpegProfileRepository, ProgramScheduleRepository,
};
use ersatzrs_core::scheduling::{PlayoutBuildMode, PlayoutBuildRequest};
use ersatzrs_domain::{
    Channel, CollectionType, EntityId, FFmpegProfile, MediaSourceKind, PlaybackOrder, PlayoutMode,
    ProgramSchedule, ProgramScheduleItem, ScanPlayoutRefresh, ScanPlayoutRefreshReason,
};
use ersatzrs_infra::maintenance::run_requested_playout_build;
use ersatzrs_infra::repositories::{make_repositories, Repositories};
use ersatzrs_web::host::scanning::NativeScanRuntime;
use ersatzrs_web::host::state::{AppState, IptvAccessTokenVerifier, SystemStartup};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Executor, Row, SqlitePool};
use uuid::Uuid;

const ADDON_ID: &str = "org.ersatzrs.addon.scan-refresh-fixture";
const RELATED_LIBRARY_ID: EntityId = 101;
const UNRELATED_LIBRARY_ID: EntityId = 102;
const RELATED_LIST_ID: EntityId = 201;
const UNRELATED_LIST_ID: EntityId = 202;
const RELATED_PLAYOUT_ID: EntityId = 901;
const UNRELATED_PLAYOUT_ID: EntityId = 902;
const SHARED_MEDIA_PLAYOUT_ID: EntityId = 903;
const RELATED_PROVIDER_ONE: &str = "related-one";
const RELATED_PROVIDER_TWO: &str = "related-two";
const RELATED_PROVIDER_THREE: &str = "related-three";
const RELATED_PROVIDER_FOUR: &str = "related-four";
const UNRELATED_PROVIDER_ONE: &str = "unrelated-one";
const UNRELATED_PROVIDER_TWO: &str = "unrelated-two";

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn production_local_scan_refresh_survives_restart_and_preserves_unrelated_playout() {
    let directory = tempfile::tempdir().expect("temporary acceptance directory");
    let package_root = directory.path().join("addon-package");
    prepare_non_executable_addon_package(&package_root);

    let baseline_paths = ManagedLibraryPaths::new(directory.path().join("baseline-libraries"));
    baseline_paths.write_manifests();
    let baseline_database = directory.path().join("baseline.sqlite");
    let baseline = open_database(&baseline_database, true).await;
    seed_addon_installation(&baseline, &package_root).await;
    seed_managed_library(
        &baseline,
        RELATED_LIBRARY_ID,
        RELATED_LIST_ID,
        &baseline_paths.related_root,
        &baseline_paths.related_manifest,
        "Related managed list",
        "related/list",
        &[
            RELATED_PROVIDER_ONE,
            RELATED_PROVIDER_TWO,
            RELATED_PROVIDER_THREE,
            RELATED_PROVIDER_FOUR,
        ],
    )
    .await;
    seed_managed_library(
        &baseline,
        UNRELATED_LIBRARY_ID,
        UNRELATED_LIST_ID,
        &baseline_paths.unrelated_root,
        &baseline_paths.unrelated_manifest,
        "Unrelated managed list",
        "unrelated/list",
        &[UNRELATED_PROVIDER_ONE, UNRELATED_PROVIDER_TWO],
    )
    .await;

    let runtime = NativeScanRuntime::new(baseline.clone());
    for library_id in [RELATED_LIBRARY_ID, UNRELATED_LIBRARY_ID] {
        let report = <NativeScanRuntime as ScanRuntime>::scan_library(
            &runtime,
            MediaSourceKind::Local,
            library_id,
            true,
            &(),
        )
        .await
        .expect("materialize the initial staged Remote Streams snapshot");
        assert!(
            report.playout_refreshes.is_empty(),
            "initial materialization precedes the Classic playouts"
        );
    }
    let related_playlist_id = managed_playlist_id(&baseline, RELATED_LIBRARY_ID).await;
    let unrelated_playlist_id = managed_playlist_id(&baseline, UNRELATED_LIBRARY_ID).await;
    baseline
        .execute(
            r#"UPDATE "AddonMediaList"
               SET "ReviewState" = 2
               WHERE "Id" IN (201, 202)"#,
        )
        .await
        .expect("complete the initial staged publications");
    assert!(durable_refresh_rows(&baseline).await.is_empty());

    let repositories = make_repositories(baseline.clone());
    seed_profile_and_channel(&repositories, RELATED_PLAYOUT_ID).await;
    seed_profile_and_channel(&repositories, UNRELATED_PLAYOUT_ID).await;
    seed_classic_playout(
        &baseline,
        &repositories,
        RELATED_PLAYOUT_ID,
        301,
        401,
        related_playlist_id,
    )
    .await;
    seed_classic_playout(
        &baseline,
        &repositories,
        UNRELATED_PLAYOUT_ID,
        302,
        402,
        unrelated_playlist_id,
    )
    .await;

    let build_at = fixed_time();
    for playout_id in [RELATED_PLAYOUT_ID, UNRELATED_PLAYOUT_ID] {
        run_classic_build(&baseline, playout_id, PlayoutBuildMode::Reset, build_at).await;
    }
    let related_before = persisted_classic_rows(&baseline, RELATED_PLAYOUT_ID).await;
    let unrelated_before = persisted_classic_rows(&baseline, UNRELATED_PLAYOUT_ID).await;
    let uninterrupted_timeline = playout_timeline(&baseline, RELATED_PLAYOUT_ID).await;
    assert!(!related_before.playout_items.is_empty());
    assert!(!related_before.playout_anchor.is_empty());
    assert!(!related_before.program_schedule_anchors.is_empty());
    assert!(!related_before.classic_checkpoints.is_empty());
    assert!(!unrelated_before.playout_items.is_empty());
    assert!(!unrelated_before.playout_anchor.is_empty());
    assert!(!unrelated_before.classic_checkpoints.is_empty());
    assert!(
        uninterrupted_timeline.len() > 4,
        "the shuffled continuity fixture must span multiple media selections"
    );

    baseline.close().await;

    let oracle_database = directory.path().join("oracle.sqlite");
    let restarted_database = directory.path().join("restarted.sqlite");
    fs::copy(&baseline_database, &oracle_database).expect("copy uninterrupted oracle database");
    fs::copy(&baseline_database, &restarted_database).expect("copy restart database");

    let oracle_paths = ManagedLibraryPaths::new(directory.path().join("oracle-libraries"));
    let restarted_paths = ManagedLibraryPaths::new(directory.path().join("restarted-libraries"));
    copy_tree(&baseline_paths.root, &oracle_paths.root);
    copy_tree(&baseline_paths.root, &restarted_paths.root);

    let refresh_at = build_at + Duration::minutes(1);
    let oracle = open_database(&oracle_database, false).await;
    rebase_managed_library_paths(&oracle, &oracle_paths).await;
    let oracle_report =
        edit_related_nfo_and_scan_both(&oracle, &oracle_paths.related_root, &unrelated_before)
            .await;
    for refresh in oracle_report.playout_refreshes {
        run_classic_build(
            &oracle,
            refresh.playout_id,
            PlayoutBuildMode::Refresh,
            refresh_at,
        )
        .await;
    }
    assert!(durable_refresh_rows(&oracle).await.is_empty());
    let oracle_rows = persisted_classic_rows(&oracle, RELATED_PLAYOUT_ID).await;
    assert!(
        oracle_rows != related_before,
        "the related Classic playout must actually refresh"
    );
    assert_uninterrupted_timeline(
        &playout_timeline(&oracle, RELATED_PLAYOUT_ID).await,
        &uninterrupted_timeline,
        "the uninterrupted metadata-only Refresh must retain the shuffled timeline",
    );
    assert_unrelated_rows_unchanged(
        &unrelated_before,
        &persisted_classic_rows(&oracle, UNRELATED_PLAYOUT_ID).await,
    );
    oracle.close().await;

    let restarted = open_database(&restarted_database, false).await;
    rebase_managed_library_paths(&restarted, &restarted_paths).await;
    let committed_report = edit_related_nfo_and_scan_both(
        &restarted,
        &restarted_paths.related_root,
        &unrelated_before,
    )
    .await;
    assert_eq!(
        committed_report.playout_refreshes,
        [expected_refresh()],
        "the committed production scan exposes exactly one admission candidate"
    );
    assert_pending_related_refresh(
        &restarted,
        ScanPlayoutRefreshReason::RemoteStreamMembershipReplaced,
    )
    .await;
    assert_unrelated_rows_unchanged(
        &unrelated_before,
        &persisted_classic_rows(&restarted, UNRELATED_PLAYOUT_ID).await,
    );
    restarted.close().await;

    let reopened = open_database(&restarted_database, false).await;
    let (restarted_state, mut playout_build_rx) = restarted_app_state(reopened.clone());
    let recovered = ersatzrs_web::host::tasks::run_background_task_preflight(
        restarted_state.application.clone(),
    )
    .await
    .expect("run composition-root startup recovery before worker admission");
    assert_eq!(
        recovered, 1,
        "startup recovery must admit exactly one related Refresh"
    );
    let request = tokio::time::timeout(std::time::Duration::from_secs(1), playout_build_rx.recv())
        .await
        .expect("startup recovery must queue without delay")
        .expect("startup recovery queue must remain connected");
    assert_eq!(request.playout_id, RELATED_PLAYOUT_ID);
    assert_eq!(request.mode, PlayoutBuildMode::Refresh);
    assert!(
        playout_build_rx.try_recv().is_err(),
        "startup recovery must coalesce duplicate admissions"
    );
    run_classic_build(&reopened, request.playout_id, request.mode, refresh_at).await;

    assert!(durable_refresh_rows(&reopened).await.is_empty());
    let restarted_rows = persisted_classic_rows(&reopened, RELATED_PLAYOUT_ID).await;
    assert!(
        restarted_rows == oracle_rows,
        "recovery must persist the oracle's Classic items, anchors, enumerators, and checkpoints"
    );
    assert_uninterrupted_timeline(
        &playout_timeline(&reopened, RELATED_PLAYOUT_ID).await,
        &uninterrupted_timeline,
        "startup recovery must retain the uninterrupted shuffled timeline without replay",
    );
    assert_unrelated_rows_unchanged(
        &unrelated_before,
        &persisted_classic_rows(&reopened, UNRELATED_PLAYOUT_ID).await,
    );

    prove_pure_reorder_is_exactly_playlist_scoped(&reopened, related_playlist_id, refresh_at).await;
}

#[derive(Clone)]
struct ManagedLibraryPaths {
    root: PathBuf,
    related_root: PathBuf,
    related_manifest: PathBuf,
    unrelated_root: PathBuf,
    unrelated_manifest: PathBuf,
}

impl ManagedLibraryPaths {
    fn new(root: PathBuf) -> Self {
        let related_root = root.join("related");
        let unrelated_root = root.join("unrelated");
        Self {
            related_manifest: related_root.join("Related.playlist.yml"),
            unrelated_manifest: unrelated_root.join("Unrelated.playlist.yml"),
            root,
            related_root,
            unrelated_root,
        }
    }

    fn write_manifests(&self) {
        fs::create_dir_all(&self.related_root).expect("create related library root");
        fs::create_dir_all(&self.unrelated_root).expect("create unrelated library root");
        fs::write(
            &self.related_manifest,
            playlist_manifest("Related managed list", "related-items", "related/list"),
        )
        .expect("write related managed playlist manifest");
        fs::write(
            &self.unrelated_manifest,
            playlist_manifest(
                "Unrelated managed list",
                "unrelated-items",
                "unrelated/list",
            ),
        )
        .expect("write unrelated managed playlist manifest");
    }
}

fn playlist_manifest(name: &str, folder: &str, provider_id: &str) -> String {
    format!(
        "name: {name}\nfolder: {folder}\nurl: https://example.invalid/{provider_id}\naddon: {ADDON_ID}\nsync:\n  interval: 24h\n  max_items: 10\n  order: upstream\n  on_removed: trash\ncreate_playlist: true\n"
    )
}

fn prepare_non_executable_addon_package(package_root: &Path) {
    fs::create_dir_all(package_root).expect("create fixture add-on package");
    fs::write(
        package_root.join("must-not-execute.fixture"),
        b"The staged publication path must never execute this file.\n",
    )
    .expect("write deliberately non-executable add-on entrypoint");
}

async fn open_database(path: &Path, create: bool) -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(create)
                .foreign_keys(true),
        )
        .await
        .expect("open file-backed SQLite database");
    if create {
        sqlx::migrate!("../../migrations")
            .run(&pool)
            .await
            .expect("migrate file-backed SQLite database");
        pool.execute(
            r#"INSERT OR IGNORE INTO "Resolution" ("Id", "Name", "Width", "Height")
               VALUES (1, '1920x1080', 1920, 1080)"#,
        )
        .await
        .expect("seed default resolution");
        pool.execute(
            r#"INSERT INTO "ConfigElement" ("Key", "Value")
               VALUES ('playout.days_to_build', '1')"#,
        )
        .await
        .expect("bound the Classic build horizon");
    }
    pool
}

async fn seed_addon_installation(pool: &SqlitePool, package_root: &Path) {
    let manifest = serde_json::json!({
        "manifest_version": 1,
        "id": ADDON_ID,
        "version": "1.0.0",
        "name": {"en-US": "Scan refresh fixture"},
        "summary": {"en-US": "A staged publication fixture that is never executed"},
        "license": "GPL-3.0-or-later",
        "source_url": "https://example.invalid/source",
        "host_version": "*",
        "capabilities": [
            {"id": "addon.check.v1"},
            {"id": "remote-stream.list.v4"},
            {"id": "remote-stream.play.v1"}
        ],
        "entrypoints": {
            "any": {"path": "must-not-execute.fixture", "kind": "native"}
        },
        "permissions": {},
        "settings": [],
        "panels": []
    });
    let content_sha256 = ersatzrs_infra::addons::calculate_addon_content_sha256(package_root)
        .await
        .expect("hash fixture add-on package");
    sqlx::query(
        r#"INSERT INTO "AddonInstallation" (
               "AddonId", "RepositoryId", "Version", "Enabled", "PackageSha256",
               "ContentSha256", "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt"
           ) VALUES (?, NULL, '1.0.0', 1, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(ADDON_ID)
    .bind("a".repeat(64))
    .bind(content_sha256)
    .bind(manifest.to_string())
    .bind(package_root.to_string_lossy().into_owned())
    .bind("2026-04-15T00:00:00Z")
    .bind("2026-04-15T00:00:00Z")
    .execute(pool)
    .await
    .expect("seed staged-publication add-on installation");
}

#[allow(clippy::too_many_arguments)]
async fn seed_managed_library(
    pool: &SqlitePool,
    library_id: EntityId,
    list_id: EntityId,
    library_root: &Path,
    manifest_path: &Path,
    name: &str,
    list_provider_id: &str,
    item_provider_ids: &[&str],
) {
    let source_id = library_id + 1_000;
    let path_id = library_id + 2_000;
    sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (?)"#)
        .bind(source_id)
        .execute(pool)
        .await
        .expect("seed Local media source");
    sqlx::query(r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (?)"#)
        .bind(source_id)
        .execute(pool)
        .await
        .expect("seed Local media-source subtype");
    sqlx::query(
        r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
           VALUES (?, ?, 7, ?)"#,
    )
    .bind(library_id)
    .bind(name)
    .bind(source_id)
    .execute(pool)
    .await
    .expect("seed Remote Streams library");
    sqlx::query(r#"INSERT INTO "LocalLibrary" ("Id") VALUES (?)"#)
        .bind(library_id)
        .execute(pool)
        .await
        .expect("seed Local library subtype");
    sqlx::query(r#"INSERT INTO "LibraryPath" ("Id", "LibraryId", "Path") VALUES (?, ?, ?)"#)
        .bind(path_id)
        .bind(library_id)
        .bind(library_root.to_string_lossy().as_ref())
        .execute(pool)
        .await
        .expect("seed Local library path");
    // Staging requires a journal even when the publication has no playback
    // selections; an absent journal is not a valid staged fixture.
    let publication_journal = serde_json::json!({
        "previous_review_state": ersatzrs_domain::AddonMediaListReviewState::Ready,
        "previous_presentation_kind": ersatzrs_domain::AddonMediaListPresentationKind::RemoteStreams,
        "previous_items": [],
        "previous_playlist": null
    });
    sqlx::query(
        r#"INSERT INTO "AddonMediaList" (
               "Id", "AddonId", "ProviderId", "SourceUrl", "Name", "ItemCount",
               "ReviewState", "PublishedRevision", "PublicationJournalJson"
           ) VALUES (?, ?, ?, ?, ?, ?, 3, 1, ?)"#,
    )
    .bind(list_id)
    .bind(ADDON_ID)
    .bind(list_provider_id)
    .bind(format!("https://example.invalid/{list_provider_id}"))
    .bind(name)
    .bind(i32::try_from(item_provider_ids.len()).expect("small fixture item count"))
    .bind(publication_journal.to_string())
    .execute(pool)
    .await
    .expect("seed staged add-on media list");
    sqlx::query(
        r#"INSERT INTO "AddonMediaListLibrary" (
               "AddonMediaListId", "LibraryId", "FolderName", "ManifestPath"
           ) VALUES (?, ?, ?, ?)"#,
    )
    .bind(list_id)
    .bind(library_id)
    .bind(if library_id == RELATED_LIBRARY_ID {
        "related-items"
    } else {
        "unrelated-items"
    })
    .bind(manifest_path.to_string_lossy().as_ref())
    .execute(pool)
    .await
    .expect("link staged list to its Local library");
    for (rank, provider_id) in item_provider_ids.iter().enumerate() {
        insert_staged_provider_item(
            pool,
            list_id * 10 + i64::try_from(rank).expect("small fixture rank"),
            list_id,
            provider_id,
            i32::try_from(rank).expect("small fixture rank"),
        )
        .await;
    }
}

async fn insert_staged_provider_item(
    pool: &SqlitePool,
    item_id: EntityId,
    list_id: EntityId,
    provider_id: &str,
    rank: i32,
) {
    let title = provider_id.replace('-', " ");
    sqlx::query(
        r#"INSERT INTO "AddonMediaListItem" (
               "Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Kind", "Rank",
               "Title", "SourceUrl", "Availability", "DurationSeconds", "Included",
               "Published", "ContentKind", "ProviderMetadataJson", "ProviderLiveness",
               "EffectiveLiveness", "PublishedLiveness"
           ) VALUES (?, ?, ?, ?, 4, ?, ?, ?, 0, 600, 1, 1, 4, ?, 1, 1, 1)"#,
    )
    .bind(item_id)
    .bind(list_id)
    .bind(provider_id)
    .bind(&title)
    .bind(rank)
    .bind(&title)
    .bind(format!("https://media.example.invalid/{provider_id}"))
    .bind(serde_json::json!({"title": title}).to_string())
    .execute(pool)
    .await
    .expect("seed staged provider item");
}

async fn seed_profile_and_channel(repositories: &Repositories, channel_id: EntityId) {
    repositories
        .ffmpeg_profile
        .upsert(&FFmpegProfile {
            id: 1,
            name: "default".to_owned(),
            resolution_id: 1,
            ..FFmpegProfile::default()
        })
        .await
        .expect("seed FFmpeg profile");
    repositories
        .channel
        .upsert(&Channel {
            id: channel_id,
            unique_id: Uuid::from_u128(u128::try_from(channel_id).expect("positive channel id")),
            number: channel_id.to_string(),
            sort_number: f64::from(i32::try_from(channel_id).expect("small channel id")),
            name: format!("Channel {channel_id}"),
            ffmpeg_profile_id: 1,
            is_enabled: true,
            ..Channel::default()
        })
        .await
        .expect("seed channel");
}

async fn seed_classic_playout(
    pool: &SqlitePool,
    repositories: &Repositories,
    playout_id: EntityId,
    schedule_id: EntityId,
    schedule_item_id: EntityId,
    playlist_id: EntityId,
) {
    repositories
        .program_schedule
        .upsert(&ProgramSchedule {
            id: schedule_id,
            name: format!("Schedule {schedule_id}"),
            item_ids: vec![schedule_item_id],
            ..ProgramSchedule::default()
        })
        .await
        .expect("seed program schedule");
    repositories
        .program_schedule
        .upsert_item(&ProgramScheduleItem {
            id: schedule_item_id,
            program_schedule_id: schedule_id,
            collection_type: CollectionType::Playlist,
            playlist_id: Some(playlist_id),
            playout_mode: PlayoutMode::One,
            playback_order: PlaybackOrder::Shuffle,
            sort_order: 1,
            ..ProgramScheduleItem::default()
        })
        .await
        .expect("seed playlist schedule item");
    sqlx::query(
        r#"INSERT INTO "Playout" (
               "Id", "ChannelId", "ProgramScheduleId", "ScheduleKind", "Seed"
           ) VALUES (?, ?, ?, 'Classic', 123)"#,
    )
    .bind(playout_id)
    .bind(playout_id)
    .bind(schedule_id)
    .execute(pool)
    .await
    .expect("seed Classic playout");
}

async fn managed_playlist_id(pool: &SqlitePool, library_id: EntityId) -> EntityId {
    sqlx::query_scalar(
        r#"SELECT "Id" FROM "Playlist"
           WHERE "ManagedSource" LIKE ?
           ORDER BY "Id" LIMIT 1"#,
    )
    .bind(format!("remote-stream-playlist:v1:{library_id}:%"))
    .fetch_one(pool)
    .await
    .expect("load scanner-managed playlist")
}

async fn edit_related_nfo_and_scan_both(
    pool: &SqlitePool,
    related_root: &Path,
    unrelated_before: &PersistedClassicRows,
) -> ersatzrs_app::ScanRuntimeReport {
    edit_first_nfo_title(related_root);

    let runtime = NativeScanRuntime::new(pool.clone());
    let unchanged = <NativeScanRuntime as ScanRuntime>::scan_library(
        &runtime,
        MediaSourceKind::Local,
        UNRELATED_LIBRARY_ID,
        true,
        &(),
    )
    .await
    .expect("scan unchanged unrelated Remote Streams library");
    assert!(
        unchanged.playout_refreshes.is_empty(),
        "the unchanged production scan must queue nothing"
    );
    assert!(
        durable_refresh_rows(pool).await.is_empty(),
        "the unchanged scan must not persist a Refresh intent"
    );
    assert_unrelated_rows_unchanged(
        unrelated_before,
        &persisted_classic_rows(pool, UNRELATED_PLAYOUT_ID).await,
    );

    let changed = <NativeScanRuntime as ScanRuntime>::scan_library(
        &runtime,
        MediaSourceKind::Local,
        RELATED_LIBRARY_ID,
        true,
        &(),
    )
    .await
    .expect("scan changed related Remote Streams library");
    assert_eq!(
        changed.playout_refreshes,
        [expected_refresh()],
        "the changed scan must scope one coalesced Refresh to its dependent playout"
    );
    assert_pending_related_refresh(
        pool,
        ScanPlayoutRefreshReason::RemoteStreamMembershipReplaced,
    )
    .await;
    assert_unrelated_rows_unchanged(
        unrelated_before,
        &persisted_classic_rows(pool, UNRELATED_PLAYOUT_ID).await,
    );
    changed
}

fn edit_first_nfo_title(root: &Path) {
    let mut queue = vec![root.to_path_buf()];
    let mut nfo_paths = Vec::new();
    while let Some(path) = queue.pop() {
        for entry in fs::read_dir(&path).expect("read managed library directory") {
            let entry = entry.expect("read managed library entry");
            let path = entry.path();
            if entry.file_type().expect("read managed entry type").is_dir() {
                queue.push(path);
            } else if path.extension().and_then(|value| value.to_str()) == Some("nfo") {
                nfo_paths.push(path);
            }
        }
    }
    nfo_paths.sort();
    let (path, original, title_start, title_finish) = nfo_paths
        .into_iter()
        .find_map(|path| {
            let original = fs::read_to_string(&path).ok()?;
            let title_start = original.find("<title>")? + "<title>".len();
            let title_finish = original[title_start..].find("</title>")? + title_start;
            Some((path, original, title_start, title_finish))
        })
        .expect("find a generated item NFO title");
    let mut edited = original.clone();
    edited.replace_range(title_start..title_finish, "Operator metadata replacement");
    assert_ne!(
        edited, original,
        "the NFO edit must change effective metadata"
    );
    fs::write(path, edited).expect("write operator-edited NFO");
}

const fn expected_refresh() -> ScanPlayoutRefresh {
    ScanPlayoutRefresh {
        playout_id: RELATED_PLAYOUT_ID,
        reason: ScanPlayoutRefreshReason::RemoteStreamMembershipReplaced,
    }
}

async fn assert_pending_related_refresh(pool: &SqlitePool, reason: ScanPlayoutRefreshReason) {
    assert_eq!(
        durable_refresh_rows(pool).await,
        [(
            RELATED_PLAYOUT_ID,
            Some(RELATED_LIBRARY_ID),
            1,
            0,
            reason.as_str().to_owned(),
        )],
        "the scan commit must durably contain exactly one pending related Refresh"
    );
    let state: i32 =
        sqlx::query_scalar(r#"SELECT "State" FROM "ScanRefreshResolution" WHERE "LibraryId" = ?"#)
            .bind(RELATED_LIBRARY_ID)
            .fetch_one(pool)
            .await
            .expect("load related scan resolution state");
    assert_eq!(
        state, 1,
        "the membership and intent transaction must commit"
    );
}

async fn prove_pure_reorder_is_exactly_playlist_scoped(
    pool: &SqlitePool,
    related_playlist_id: EntityId,
    build_at: DateTime<Utc>,
) {
    let shared_media_id = media_id_for_provider(pool, RELATED_PROVIDER_ONE).await;
    let shared_playlist_id = seed_shared_media_playlist(pool, shared_media_id).await;
    let repositories = make_repositories(pool.clone());
    seed_profile_and_channel(&repositories, SHARED_MEDIA_PLAYOUT_ID).await;
    seed_classic_playout(
        pool,
        &repositories,
        SHARED_MEDIA_PLAYOUT_ID,
        303,
        403,
        shared_playlist_id,
    )
    .await;
    run_classic_build(
        pool,
        SHARED_MEDIA_PLAYOUT_ID,
        PlayoutBuildMode::Reset,
        build_at,
    )
    .await;
    let shared_before = persisted_classic_rows(pool, SHARED_MEDIA_PLAYOUT_ID).await;
    assert!(!shared_before.playout_items.is_empty());

    sqlx::query(
        r#"UPDATE "AddonMediaListItem"
           SET "Rank" = CASE "ProviderId"
               WHEN 'related-one' THEN 3
               WHEN 'related-two' THEN 2
               WHEN 'related-three' THEN 1
               WHEN 'related-four' THEN 0
               ELSE "Rank"
           END
           WHERE "AddonMediaListId" = ?"#,
    )
    .bind(RELATED_LIST_ID)
    .execute(pool)
    .await
    .expect("reverse the staged provider order");
    sqlx::query(r#"UPDATE "AddonMediaList" SET "ReviewState" = 3 WHERE "Id" = ?"#)
        .bind(RELATED_LIST_ID)
        .execute(pool)
        .await
        .expect("stage the pure reorder publication");

    let runtime = NativeScanRuntime::new(pool.clone());
    let report = <NativeScanRuntime as ScanRuntime>::scan_library(
        &runtime,
        MediaSourceKind::Local,
        RELATED_LIBRARY_ID,
        true,
        &(),
    )
    .await
    .expect("scan a pure managed-playlist reorder");
    assert_eq!(
        report.playout_refreshes,
        [ScanPlayoutRefresh {
            playout_id: RELATED_PLAYOUT_ID,
            reason: ScanPlayoutRefreshReason::RemoteStreamMembershipReordered,
        }],
        "pure reordering must refresh only the exact managed-playlist consumer"
    );
    assert_pending_related_refresh(
        pool,
        ScanPlayoutRefreshReason::RemoteStreamMembershipReordered,
    )
    .await;
    assert_eq!(
        managed_playlist_id(pool, RELATED_LIBRARY_ID).await,
        related_playlist_id,
        "reordering must retain the managed playlist identity"
    );
    assert_unrelated_rows_unchanged(
        &shared_before,
        &persisted_classic_rows(pool, SHARED_MEDIA_PLAYOUT_ID).await,
    );

    run_classic_build(
        pool,
        RELATED_PLAYOUT_ID,
        PlayoutBuildMode::Refresh,
        build_at + Duration::minutes(1),
    )
    .await;
    assert!(durable_refresh_rows(pool).await.is_empty());
    assert_unrelated_rows_unchanged(
        &shared_before,
        &persisted_classic_rows(pool, SHARED_MEDIA_PLAYOUT_ID).await,
    );
}

async fn seed_shared_media_playlist(pool: &SqlitePool, media_id: EntityId) -> EntityId {
    let group_id: EntityId = sqlx::query_scalar(
        r#"SELECT "Id" FROM "PlaylistGroup" WHERE "IsSystem" = 1 ORDER BY "Id" LIMIT 1"#,
    )
    .fetch_one(pool)
    .await
    .expect("load the scanner-created system playlist group");
    let result = sqlx::query(
        r#"INSERT INTO "Playlist" ("IsSystem", "ManagedSource", "Name", "PlaylistGroupId")
           VALUES (0, NULL, 'Shared media reorder control', ?)"#,
    )
    .bind(group_id)
    .execute(pool)
    .await
    .expect("seed shared-media control playlist");
    let playlist_id = ersatzrs_infra::db::last_insert_id(result);
    sqlx::query(
        r#"INSERT INTO "PlaylistItem" (
               "Index", "PlaylistId", "CollectionType", "CollectionId", "MediaItemId",
               "MultiCollectionId", "SmartCollectionId", "IncludeInProgramGuide",
               "PlaybackOrder", "PlayAll"
           ) VALUES (0, ?, 70, NULL, ?, NULL, NULL, 1, 1, 0)"#,
    )
    .bind(playlist_id)
    .bind(media_id)
    .execute(pool)
    .await
    .expect("seed shared-media control playlist item");
    playlist_id
}

async fn durable_refresh_rows(
    pool: &SqlitePool,
) -> Vec<(EntityId, Option<EntityId>, i64, i32, String)> {
    sqlx::query_as(
        r#"SELECT "PlayoutId", "LibraryId", "Generation", "State", "Reason"
           FROM "ScanPlayoutRefreshIntent" ORDER BY "PlayoutId""#,
    )
    .fetch_all(pool)
    .await
    .expect("load durable Refresh intents")
}

fn restarted_app_state(
    pool: SqlitePool,
) -> (
    AppState,
    tokio::sync::mpsc::UnboundedReceiver<PlayoutBuildRequest>,
) {
    let (playout_build_tx, playout_build_rx) = tokio::sync::mpsc::unbounded_channel();
    let (scan_work_tx, _scan_work_rx) = tokio::sync::mpsc::unbounded_channel();
    let state = AppState::new_with_scan_queue(
        pool,
        ersatzrs_infra::db::DatabaseBackend::Sqlite,
        SystemStartup {
            database_ready: true,
            schema_status: ersatzrs_infra::db::DatabaseSchemaStatus::default(),
        },
        ersatzrs_ffmpeg::FfmpegRuntimeReport::for_tests(),
        Uuid::nil(),
        1,
        IptvAccessTokenVerifier::disabled(),
        playout_build_tx,
        scan_work_tx,
        EntityLocker::new(),
    );
    (state, playout_build_rx)
}

async fn run_classic_build(
    pool: &SqlitePool,
    playout_id: EntityId,
    mode: PlayoutBuildMode,
    now: DateTime<Utc>,
) {
    assert!(
        run_requested_playout_build(
            pool,
            &EntityLocker::new(),
            PlayoutBuildRequest { playout_id, mode },
            now,
            None,
        )
        .await
        .expect("run scheduler-owned Classic build"),
        "the acceptance fixture must acquire the playout lock"
    );
}

fn fixed_time() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 4, 15, 10, 0, 0)
        .single()
        .expect("valid fixed build time")
}

async fn rebase_managed_library_paths(pool: &SqlitePool, paths: &ManagedLibraryPaths) {
    for (library_id, list_id, root, manifest) in [
        (
            RELATED_LIBRARY_ID,
            RELATED_LIST_ID,
            &paths.related_root,
            &paths.related_manifest,
        ),
        (
            UNRELATED_LIBRARY_ID,
            UNRELATED_LIST_ID,
            &paths.unrelated_root,
            &paths.unrelated_manifest,
        ),
    ] {
        sqlx::query(r#"UPDATE "LibraryPath" SET "Path" = ? WHERE "LibraryId" = ?"#)
            .bind(root.to_string_lossy().as_ref())
            .bind(library_id)
            .execute(pool)
            .await
            .expect("rebase cloned Local library path");
        sqlx::query(
            r#"UPDATE "AddonMediaListLibrary" SET "ManifestPath" = ?
               WHERE "AddonMediaListId" = ?"#,
        )
        .bind(manifest.to_string_lossy().as_ref())
        .bind(list_id)
        .execute(pool)
        .await
        .expect("rebase cloned managed manifest path");
    }
}

fn copy_tree(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).expect("create cloned managed-library tree");
    for entry in fs::read_dir(source).expect("read managed-library tree") {
        let entry = entry.expect("read managed-library entry");
        let target = destination.join(entry.file_name());
        if entry.file_type().expect("read entry type").is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).expect("copy managed-library file");
        }
    }
}

async fn media_id_for_provider(pool: &SqlitePool, provider_id: &str) -> EntityId {
    sqlx::query_scalar(
        r#"SELECT "Id" FROM "RemoteStream"
           WHERE "AddonId" = ? AND "ProviderId" = ?"#,
    )
    .bind(ADDON_ID)
    .bind(provider_id)
    .fetch_one(pool)
    .await
    .expect("load materialized provider media id")
}

type PlayoutTimelineRow = (EntityId, String, String, String, String, Option<String>);

async fn playout_timeline(pool: &SqlitePool, playout_id: EntityId) -> Vec<PlayoutTimelineRow> {
    sqlx::query_as(
        r#"SELECT "MediaItemId", "Start", "Finish", "InPoint", "OutPoint",
                  "SchedulingContext"
           FROM "PlayoutItem"
           WHERE "PlayoutId" = ?
           ORDER BY "Start", "Id""#,
    )
    .bind(playout_id)
    .fetch_all(pool)
    .await
    .expect("load normalized playout timeline")
}

fn assert_uninterrupted_timeline(
    actual: &[PlayoutTimelineRow],
    expected: &[PlayoutTimelineRow],
    message: &str,
) {
    if actual.starts_with(expected) {
        if let (Some(last_expected), Some(first_extension)) =
            (expected.last(), actual.get(expected.len()))
        {
            assert_eq!(
                first_extension.1, last_expected.2,
                "{message}: the refreshed horizon must extend contiguously"
            );
        }
        return;
    }
    let first_mismatch = actual
        .iter()
        .zip(expected)
        .position(|(actual, expected)| actual != expected)
        .unwrap_or_else(|| actual.len().min(expected.len()));
    panic!(
        "{message}: actual_len={}, expected_len={}, first_mismatch={}, actual={:?}, expected={:?}",
        actual.len(),
        expected.len(),
        first_mismatch,
        actual.get(first_mismatch),
        expected.get(first_mismatch),
    );
}

#[derive(PartialEq, Eq)]
struct PersistedClassicRows {
    playout: Vec<Vec<u8>>,
    playout_items: Vec<Vec<u8>>,
    playout_item_graphics: Vec<Vec<u8>>,
    playout_item_watermarks: Vec<Vec<u8>>,
    playout_gaps: Vec<Vec<u8>>,
    playout_history: Vec<Vec<u8>>,
    rerun_history: Vec<Vec<u8>>,
    playout_anchor: Vec<Vec<u8>>,
    schedule_item_enumerator: Vec<Vec<u8>>,
    program_schedule_anchors: Vec<Vec<u8>>,
    collection_enumerators: Vec<Vec<u8>>,
    fill_group_indices: Vec<Vec<u8>>,
    fill_group_enumerators: Vec<Vec<u8>>,
    classic_checkpoints: Vec<Vec<u8>>,
    playout_templates: Vec<Vec<u8>>,
    program_schedule_alternates: Vec<Vec<u8>>,
    build_status: Vec<Vec<u8>>,
    scan_refresh_intents: Vec<Vec<u8>>,
}

async fn persisted_classic_rows(pool: &SqlitePool, playout_id: EntityId) -> PersistedClassicRows {
    PersistedClassicRows {
        playout: direct_rows(pool, "Playout", "Id", playout_id).await,
        playout_items: direct_rows(pool, "PlayoutItem", "PlayoutId", playout_id).await,
        playout_item_graphics: child_rows(
            pool,
            "PlayoutItemGraphicsElement",
            "PlayoutItemId",
            "PlayoutItem",
            "Id",
            playout_id,
        )
        .await,
        playout_item_watermarks: child_rows(
            pool,
            "PlayoutItemWatermark",
            "PlayoutItemId",
            "PlayoutItem",
            "Id",
            playout_id,
        )
        .await,
        playout_gaps: direct_rows(pool, "PlayoutGap", "PlayoutId", playout_id).await,
        playout_history: direct_rows(pool, "PlayoutHistory", "PlayoutId", playout_id).await,
        rerun_history: direct_rows(pool, "RerunHistory", "PlayoutId", playout_id).await,
        playout_anchor: direct_rows(pool, "PlayoutAnchor", "PlayoutId", playout_id).await,
        schedule_item_enumerator: direct_rows(
            pool,
            "ScheduleItemsEnumeratorState",
            "PlayoutAnchorPlayoutId",
            playout_id,
        )
        .await,
        program_schedule_anchors: direct_rows(
            pool,
            "PlayoutProgramScheduleAnchor",
            "PlayoutId",
            playout_id,
        )
        .await,
        collection_enumerators: child_rows(
            pool,
            "CollectionEnumeratorState",
            "PlayoutProgramScheduleAnchorId",
            "PlayoutProgramScheduleAnchor",
            "Id",
            playout_id,
        )
        .await,
        fill_group_indices: direct_rows(
            pool,
            "PlayoutScheduleItemFillGroupIndex",
            "PlayoutId",
            playout_id,
        )
        .await,
        fill_group_enumerators: child_rows(
            pool,
            "FillGroupEnumeratorState",
            "PlayoutScheduleItemFillGroupIndexId",
            "PlayoutScheduleItemFillGroupIndex",
            "Id",
            playout_id,
        )
        .await,
        classic_checkpoints: direct_rows(pool, "PlayoutClassicCheckpoint", "PlayoutId", playout_id)
            .await,
        playout_templates: direct_rows(pool, "PlayoutTemplate", "PlayoutId", playout_id).await,
        program_schedule_alternates: direct_rows(
            pool,
            "ProgramScheduleAlternate",
            "PlayoutId",
            playout_id,
        )
        .await,
        build_status: direct_rows(pool, "PlayoutBuildStatus", "PlayoutId", playout_id).await,
        scan_refresh_intents: direct_rows(
            pool,
            "ScanPlayoutRefreshIntent",
            "PlayoutId",
            playout_id,
        )
        .await,
    }
}

fn assert_unrelated_rows_unchanged(before: &PersistedClassicRows, after: &PersistedClassicRows) {
    assert!(
        before.playout == after.playout,
        "unrelated Playout row changed byte-for-byte"
    );
    assert!(
        before.playout_items == after.playout_items,
        "unrelated PlayoutItem rows changed byte-for-byte"
    );
    assert!(
        before.playout_anchor == after.playout_anchor,
        "unrelated PlayoutAnchor row changed byte-for-byte"
    );
    assert!(
        before.program_schedule_anchors == after.program_schedule_anchors,
        "unrelated PlayoutProgramScheduleAnchor rows changed byte-for-byte"
    );
    assert!(
        before.classic_checkpoints == after.classic_checkpoints,
        "unrelated PlayoutClassicCheckpoint rows changed byte-for-byte"
    );
    assert!(
        before == after,
        "some unrelated persisted Classic or child row changed"
    );
}

async fn direct_rows(
    pool: &SqlitePool,
    table: &str,
    playout_column: &str,
    playout_id: EntityId,
) -> Vec<Vec<u8>> {
    let columns = table_columns(pool, table).await;
    let projection = quoted_row_projection("row", &columns);
    let sql = format!(
        "SELECT CAST({projection} AS BLOB) FROM {} AS \"row\" WHERE \"row\".{} = ?",
        quote_identifier(table),
        quote_identifier(playout_column),
    );
    let mut rows = sqlx::query_scalar::<_, Vec<u8>>(ersatzrs_infra::db::assert_sql_safe(&sql))
        .bind(playout_id)
        .fetch_all(pool)
        .await
        .expect("serialize persisted table rows");
    rows.sort();
    rows
}

async fn child_rows(
    pool: &SqlitePool,
    child_table: &str,
    child_foreign_key: &str,
    parent_table: &str,
    parent_primary_key: &str,
    playout_id: EntityId,
) -> Vec<Vec<u8>> {
    let columns = table_columns(pool, child_table).await;
    let projection = quoted_row_projection("child", &columns);
    let sql = format!(
        "SELECT CAST({projection} AS BLOB) FROM {} AS \"child\" \
         JOIN {} AS \"parent\" ON \"parent\".{} = \"child\".{} \
         WHERE \"parent\".\"PlayoutId\" = ?",
        quote_identifier(child_table),
        quote_identifier(parent_table),
        quote_identifier(parent_primary_key),
        quote_identifier(child_foreign_key),
    );
    let mut rows = sqlx::query_scalar::<_, Vec<u8>>(ersatzrs_infra::db::assert_sql_safe(&sql))
        .bind(playout_id)
        .fetch_all(pool)
        .await
        .expect("serialize persisted child rows");
    rows.sort();
    rows
}

async fn table_columns(pool: &SqlitePool, table: &str) -> Vec<String> {
    let sql = format!("PRAGMA table_info({})", quote_identifier(table));
    let columns = sqlx::query(ersatzrs_infra::db::assert_sql_safe(&sql))
        .fetch_all(pool)
        .await
        .expect("load table columns")
        .into_iter()
        .map(|row| row.get("name"))
        .collect::<Vec<String>>();
    assert!(!columns.is_empty(), "table {table} must exist");
    columns
}

fn quoted_row_projection(alias: &str, columns: &[String]) -> String {
    columns
        .iter()
        .map(|column| {
            format!(
                "quote({}.{})",
                quote_identifier(alias),
                quote_identifier(column)
            )
        })
        .collect::<Vec<_>>()
        .join(" || char(31) || ")
}

fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}
