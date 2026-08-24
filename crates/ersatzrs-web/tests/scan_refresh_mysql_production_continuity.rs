#![cfg(all(feature = "mysql", feature = "mysql-integration"))]

use std::collections::BTreeSet;
use std::fs;
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration as StdDuration;

use chrono::{DateTime, Duration, TimeZone, Utc};
use ersatzrs_app::ScanRuntime;
use ersatzrs_core::locking::EntityLocker;
use ersatzrs_core::repositories::ProgramScheduleRepository;
use ersatzrs_core::scheduling::{PlayoutBuildMode, PlayoutBuildRequest};
use ersatzrs_domain::{
    CollectionType, EntityId, MediaSourceKind, PlaybackOrder, PlayoutMode, ProgramSchedule,
    ProgramScheduleItem, ScanPlayoutRefresh, ScanPlayoutRefreshReason,
};
use ersatzrs_infra::db::{self, DatabaseBackend, DatabaseSchemaStatus, DbPool};
use ersatzrs_infra::maintenance::run_requested_playout_build;
use ersatzrs_infra::repositories::{make_repositories, Repositories};
use ersatzrs_web::host::scanning::NativeScanRuntime;
use ersatzrs_web::host::state::{AppState, IptvAccessTokenVerifier, SystemStartup};
use futures_util::FutureExt as _;
use sqlx::Row as _;
use uuid::Uuid;

type MysqlIntegrationResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

static MYSQL_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

const RELATED_PROVIDER_IDS: &[&str] = &[
    "related-01",
    "related-02",
    "related-03",
    "related-04",
    "related-05",
    "related-06",
    "related-07",
    "related-08",
];
const UNRELATED_PROVIDER_IDS: &[&str] = &[
    "unrelated-01",
    "unrelated-02",
    "unrelated-03",
    "unrelated-04",
    "unrelated-05",
    "unrelated-06",
];

/// `crf-02`: the production Local/Remote Streams scan path preserves one
/// metadata-only Classic Refresh across a real MySQL storage restart.
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn mysql_production_scan_refresh_survives_restart_and_preserves_unrelated_rows() {
    #[allow(clippy::disallowed_methods)]
    let Ok(database_url) = std::env::var("ERSATZRS_MYSQL_TEST_URL") else {
        eprintln!(
            "skipping MySQL production scan-refresh continuity test: \
             ERSATZRS_MYSQL_TEST_URL is not set"
        );
        return;
    };
    let _test_guard = MYSQL_TEST_LOCK.lock().await;
    let random = Uuid::new_v4().simple().to_string();
    let tag = format!("crf02-web-{}", &random[..12]);
    let directory = tempfile::tempdir().expect("create MySQL acceptance fixture directory");

    let outcome = AssertUnwindSafe(run_mysql_production_continuity(
        &database_url,
        &tag,
        directory.path(),
    ))
    .catch_unwind()
    .await;
    let cleanup = cleanup_mysql_production_fixture(&database_url, &tag).await;

    match (outcome, cleanup) {
        (Ok(Ok(())), Ok(())) => {}
        (Ok(Err(_)), Ok(())) => {
            panic!("MySQL production scan-refresh continuity test failed (details redacted)")
        }
        (Err(payload), Ok(())) => std::panic::resume_unwind(payload),
        (Ok(Ok(())), Err(_)) => {
            panic!("MySQL production scan-refresh fixture cleanup failed (details redacted)")
        }
        (Ok(Err(_)), Err(_)) => panic!(
            "MySQL production scan-refresh test and fixture cleanup failed (details redacted)"
        ),
        (Err(_), Err(_)) => panic!(
            "MySQL production scan-refresh test panicked and fixture cleanup failed \
             (details redacted)"
        ),
    }
}

#[derive(Clone, Copy, Debug)]
struct ManagedLibrary {
    library_id: EntityId,
    list_id: EntityId,
    playlist_id: EntityId,
}

#[derive(Clone, Copy, Debug)]
struct ClassicFixture {
    related: ManagedLibrary,
    target_playout_id: EntityId,
    oracle_playout_id: EntityId,
    unrelated_playout_id: EntityId,
}

impl ClassicFixture {
    const fn playout_ids(self) -> [EntityId; 3] {
        [
            self.target_playout_id,
            self.oracle_playout_id,
            self.unrelated_playout_id,
        ]
    }

    fn related_refreshes(self) -> Vec<ScanPlayoutRefresh> {
        let mut refreshes = vec![
            ScanPlayoutRefresh {
                playout_id: self.target_playout_id,
                reason: ScanPlayoutRefreshReason::RemoteStreamMembershipReplaced,
            },
            ScanPlayoutRefresh {
                playout_id: self.oracle_playout_id,
                reason: ScanPlayoutRefreshReason::RemoteStreamMembershipReplaced,
            },
        ];
        refreshes.sort_by_key(|refresh| refresh.playout_id);
        refreshes
    }
}

#[derive(Clone, Debug)]
struct ManagedLibraryPaths {
    related_root: PathBuf,
    related_manifest: PathBuf,
    unrelated_root: PathBuf,
    unrelated_manifest: PathBuf,
}

impl ManagedLibraryPaths {
    fn new(root: &Path, tag: &str, addon_id: &str) -> MysqlIntegrationResult<Self> {
        let related_root = root.join("related");
        let unrelated_root = root.join("unrelated");
        fs::create_dir_all(&related_root)?;
        fs::create_dir_all(&unrelated_root)?;
        let related_manifest = related_root.join("Related.playlist.yml");
        let unrelated_manifest = unrelated_root.join("Unrelated.playlist.yml");
        fs::write(
            &related_manifest,
            playlist_manifest(
                &format!("{tag} related managed list"),
                "related-items",
                &format!("{tag}/related"),
                addon_id,
            ),
        )?;
        fs::write(
            &unrelated_manifest,
            playlist_manifest(
                &format!("{tag} unrelated managed list"),
                "unrelated-items",
                &format!("{tag}/unrelated"),
                addon_id,
            ),
        )?;
        Ok(Self {
            related_root,
            related_manifest,
            unrelated_root,
            unrelated_manifest,
        })
    }
}

#[allow(clippy::too_many_lines)]
async fn run_mysql_production_continuity(
    database_url: &str,
    tag: &str,
    fixture_root: &Path,
) -> MysqlIntegrationResult {
    let addon_id = fixture_addon_id(tag);
    let package_root = fixture_root.join("addon-package");
    prepare_non_executable_addon_package(&package_root)?;
    let paths = ManagedLibraryPaths::new(&fixture_root.join("libraries"), tag, &addon_id)?;
    let pool = db::init(database_url).await?;

    seed_addon_installation(&pool, &package_root, tag, &addon_id).await?;
    let mut related = seed_managed_library(
        &pool,
        tag,
        &addon_id,
        "related",
        &paths.related_root,
        &paths.related_manifest,
        "related-items",
        RELATED_PROVIDER_IDS,
    )
    .await?;
    let mut unrelated = seed_managed_library(
        &pool,
        tag,
        &addon_id,
        "unrelated",
        &paths.unrelated_root,
        &paths.unrelated_manifest,
        "unrelated-items",
        UNRELATED_PROVIDER_IDS,
    )
    .await?;

    let runtime = NativeScanRuntime::new(pool.clone());
    for library_id in [related.library_id, unrelated.library_id] {
        let report = <NativeScanRuntime as ScanRuntime>::scan_library(
            &runtime,
            MediaSourceKind::Local,
            library_id,
            true,
            &(),
        )
        .await?;
        assert!(
            report.playout_refreshes.is_empty(),
            "initial staged materialization must precede every fixture playout"
        );
    }
    related.playlist_id = managed_playlist_id(&pool, related.library_id).await?;
    unrelated.playlist_id = managed_playlist_id(&pool, unrelated.library_id).await?;
    sqlx::query(r#"UPDATE "AddonMediaList" SET "ReviewState" = 2 WHERE "AddonId" = ?"#)
        .bind(&addon_id)
        .execute(&pool)
        .await?;
    let review_states = sqlx::query_as::<_, (EntityId, i64)>(
        r#"SELECT "Id", "ReviewState" FROM "AddonMediaList"
           WHERE "AddonId" = ? ORDER BY "Id""#,
    )
    .bind(&addon_id)
    .fetch_all(&pool)
    .await?;
    assert_eq!(
        review_states,
        [(related.list_id, 2), (unrelated.list_id, 2),],
        "both real staged publications must enter their stable published state"
    );

    let related_membership_before = playlist_membership(&pool, related.playlist_id).await?;
    let unrelated_membership_before = playlist_rows(&pool, unrelated.playlist_id).await?;
    assert_eq!(related_membership_before.len(), RELATED_PROVIDER_IDS.len());
    assert_eq!(
        unrelated_membership_before.len(),
        UNRELATED_PROVIDER_IDS.len()
    );

    let repositories = make_repositories(pool.clone());
    let fixture = seed_classic_fixture(&pool, &repositories, tag, related, unrelated).await?;
    let build_at = fixed_build_time();
    let initial_locker = EntityLocker::new();
    for playout_id in fixture.playout_ids() {
        run_classic_build(
            &pool,
            &initial_locker,
            PlayoutBuildRequest {
                playout_id,
                mode: PlayoutBuildMode::Reset,
            },
            build_at,
        )
        .await?;
    }

    let target_before = persisted_classic_rows(&pool, fixture.target_playout_id, true).await?;
    let oracle_before = persisted_classic_rows(&pool, fixture.oracle_playout_id, true).await?;
    let unrelated_before =
        persisted_classic_rows(&pool, fixture.unrelated_playout_id, false).await?;
    let target_timeline_before = playout_timeline(&pool, fixture.target_playout_id).await?;
    assert_eq!(
        target_before, oracle_before,
        "target and oracle must begin from equivalent fresh Shuffle state"
    );
    assert!(
        target_timeline_before.len() > RELATED_PROVIDER_IDS.len(),
        "the finite Shuffle fixture must span more than one full collection pass"
    );
    assert!(
        target_timeline_before
            .iter()
            .map(|row| row.0)
            .collect::<BTreeSet<_>>()
            .len()
            >= RELATED_PROVIDER_IDS.len(),
        "every finite related item must participate in the fresh Shuffle timeline"
    );
    assert!(!target_before.playout_anchor.is_empty());
    assert!(!target_before.program_schedule_anchors.is_empty());
    assert!(!target_before.collection_enumerators.is_empty());
    assert!(!target_before.classic_checkpoints.is_empty());
    assert!(!unrelated_before.playout_items.is_empty());
    assert!(!unrelated_before.playout_anchor.is_empty());
    assert!(!unrelated_before.classic_checkpoints.is_empty());

    let unrelated_resolution_before = mysql_keyed_rows(
        &pool,
        "ScanRefreshResolution",
        "LibraryId",
        unrelated.library_id,
        &[],
    )
    .await?;
    let edited_provider_id = RELATED_PROVIDER_IDS[0];
    let media_id_before = remote_stream_media_id(&pool, &addon_id, edited_provider_id).await?;
    let metadata_title_before = remote_stream_title(&pool, &addon_id, edited_provider_id).await?;
    let edited_title = format!("{tag} operator NFO metadata");
    edit_generated_nfo_title(
        &paths.related_root,
        "related-items",
        edited_provider_id,
        &edited_title,
    )?;

    let unchanged = <NativeScanRuntime as ScanRuntime>::scan_library(
        &runtime,
        MediaSourceKind::Local,
        unrelated.library_id,
        true,
        &(),
    )
    .await?;
    assert!(
        unchanged.playout_refreshes.is_empty(),
        "the unchanged unrelated production scan must admit no Refresh"
    );
    assert!(
        fixture_refresh_rows(&pool, fixture).await?.is_empty(),
        "the unchanged unrelated scan must persist no fixture Refresh intent"
    );
    assert_eq!(
        unrelated_membership_before,
        playlist_rows(&pool, unrelated.playlist_id).await?,
        "the unchanged unrelated scan must preserve every managed playlist row"
    );
    assert_eq!(
        unrelated_resolution_before,
        mysql_keyed_rows(
            &pool,
            "ScanRefreshResolution",
            "LibraryId",
            unrelated.library_id,
            &[]
        )
        .await?,
        "the unchanged unrelated scan must preserve its durable resolution row"
    );
    assert_eq!(
        unrelated_before,
        persisted_classic_rows(&pool, fixture.unrelated_playout_id, false).await?,
        "the unchanged unrelated scan must preserve every persisted Classic row"
    );

    let mut changed = <NativeScanRuntime as ScanRuntime>::scan_library(
        &runtime,
        MediaSourceKind::Local,
        related.library_id,
        true,
        &(),
    )
    .await?
    .playout_refreshes;
    changed.sort_by_key(|refresh| refresh.playout_id);
    assert_eq!(
        changed,
        fixture.related_refreshes(),
        "the NFO-only production scan must scope exact Refreshes to both related consumers"
    );
    assert_eq!(
        related_membership_before,
        playlist_membership(&pool, related.playlist_id).await?,
        "the effective NFO metadata edit must not change ordered playlist membership"
    );
    assert_eq!(
        remote_stream_media_id(&pool, &addon_id, edited_provider_id).await?,
        media_id_before,
        "the effective NFO metadata edit must retain stable media identity"
    );
    assert_ne!(
        metadata_title_before.as_deref(),
        Some(edited_title.as_str())
    );
    assert_eq!(
        remote_stream_title(&pool, &addon_id, edited_provider_id)
            .await?
            .as_deref(),
        Some(edited_title.as_str()),
        "the changed scan must materialize the operator NFO title"
    );
    assert_exact_pending_refreshes(&pool, fixture).await?;
    assert_eq!(
        unrelated_membership_before,
        playlist_rows(&pool, unrelated.playlist_id).await?,
        "the changed related scan must not rewrite the unrelated managed playlist"
    );
    assert_eq!(
        unrelated_before,
        persisted_classic_rows(&pool, fixture.unrelated_playout_id, false).await?,
        "the changed related scan must not alter any unrelated Classic row"
    );

    let refresh_at = build_at + Duration::minutes(1);
    run_classic_build(
        &pool,
        &EntityLocker::new(),
        PlayoutBuildRequest {
            playout_id: fixture.oracle_playout_id,
            mode: PlayoutBuildMode::Refresh,
        },
        refresh_at,
    )
    .await?;
    let oracle_after = persisted_classic_rows(&pool, fixture.oracle_playout_id, true).await?;
    assert_ne!(
        oracle_after, oracle_before,
        "the oracle must execute a real Refresh"
    );
    let oracle_timeline_after = playout_timeline(&pool, fixture.oracle_playout_id).await?;
    assert_uninterrupted_timeline(
        &oracle_timeline_after,
        &target_timeline_before,
        "the uninterrupted metadata-only Refresh must retain the Shuffle timeline",
    );
    assert_eq!(
        fixture_refresh_rows(&pool, fixture).await?,
        [pending_refresh_row(
            fixture.target_playout_id,
            related.library_id,
            scan_resolution_generation(&pool, related.library_id).await?,
        )],
        "the uninterrupted oracle must consume only its own durable intent"
    );
    assert_eq!(
        unrelated_before,
        persisted_classic_rows(&pool, fixture.unrelated_playout_id, false).await?,
        "oracle processing must preserve every unrelated persisted row"
    );

    pool.close().await;
    let reopened = db::init(database_url).await?;
    let (state, mut playout_build_rx) = restarted_app_state(reopened, EntityLocker::new());
    let recovered =
        ersatzrs_web::host::tasks::run_background_task_preflight(Arc::clone(&state.application))
            .await?;
    assert_eq!(
        recovered, 1,
        "startup recovery must admit exactly one still-pending Refresh"
    );
    let queued = tokio::time::timeout(StdDuration::from_secs(1), playout_build_rx.recv())
        .await?
        .ok_or_else(|| std::io::Error::other("startup playout queue closed"))?;
    assert_eq!(
        queued,
        PlayoutBuildRequest {
            playout_id: fixture.target_playout_id,
            mode: PlayoutBuildMode::Refresh,
        },
        "startup recovery must queue only the related target Refresh"
    );
    assert!(
        matches!(
            playout_build_rx.try_recv(),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty)
        ),
        "startup recovery must queue exactly one Refresh token"
    );

    let activity = state.application.operations().playout_activity();
    let (queued, activity_guard) = activity.start_guard(queued);
    run_classic_build(
        &state.pool,
        &state.application.operations().entity_locker(),
        queued,
        refresh_at,
    )
    .await?;
    drop(activity_guard);
    assert!(activity.queued_playouts().is_empty());
    assert!(activity.building_playouts().is_empty());

    assert!(
        fixture_refresh_rows(&state.pool, fixture).await?.is_empty(),
        "the recovered worker must consume the final durable intent"
    );
    let target_after = persisted_classic_rows(&state.pool, fixture.target_playout_id, true).await?;
    assert_ne!(
        target_after, target_before,
        "the recovered target must refresh"
    );
    assert_eq!(
        target_after, oracle_after,
        "restart recovery must match the uninterrupted oracle timeline, anchors, checkpoints, and runtime rows"
    );
    let target_timeline_after = playout_timeline(&state.pool, fixture.target_playout_id).await?;
    assert_uninterrupted_timeline(
        &target_timeline_after,
        &target_timeline_before,
        "startup recovery must retain every prior Shuffle row exactly",
    );
    assert_eq!(
        target_timeline_after, oracle_timeline_after,
        "target and oracle timelines must be identical after recovery"
    );
    assert_eq!(target_after.playout_anchor, oracle_after.playout_anchor);
    assert_eq!(
        target_after.program_schedule_anchors,
        oracle_after.program_schedule_anchors
    );
    assert_eq!(
        target_after.classic_checkpoints,
        oracle_after.classic_checkpoints
    );
    assert_eq!(
        target_after.collection_enumerators,
        oracle_after.collection_enumerators
    );
    assert_eq!(
        unrelated_before,
        persisted_classic_rows(&state.pool, fixture.unrelated_playout_id, false).await?,
        "every unrelated persisted Classic and child row must remain byte-for-byte unchanged"
    );
    assert_eq!(
        unrelated_membership_before,
        playlist_rows(&state.pool, unrelated.playlist_id).await?,
        "the unrelated managed playlist rows must remain byte-for-byte unchanged"
    );
    assert_eq!(
        unrelated_resolution_before,
        mysql_keyed_rows(
            &state.pool,
            "ScanRefreshResolution",
            "LibraryId",
            unrelated.library_id,
            &[],
        )
        .await?,
        "the unrelated durable scan-resolution row must remain byte-for-byte unchanged"
    );

    state.pool.close().await;
    Ok(())
}

fn playlist_manifest(name: &str, folder: &str, provider_id: &str, addon_id: &str) -> String {
    format!(
        "name: {name}\nfolder: {folder}\nurl: https://example.invalid/{provider_id}\n\
         addon: {addon_id}\nsync:\n  interval: 24h\n  max_items: 32\n  order: upstream\n  \
         on_removed: trash\ncreate_playlist: true\n"
    )
}

fn fixture_addon_id(tag: &str) -> String {
    format!("org.ersatzrs.addon.{tag}")
}

fn prepare_non_executable_addon_package(package_root: &Path) -> MysqlIntegrationResult {
    fs::create_dir_all(package_root)?;
    fs::write(
        package_root.join("must-not-execute.fixture"),
        b"The staged publication path must never execute this file.\n",
    )?;
    Ok(())
}

async fn seed_addon_installation(
    pool: &DbPool,
    package_root: &Path,
    tag: &str,
    addon_id: &str,
) -> MysqlIntegrationResult {
    let manifest = serde_json::json!({
        "manifest_version": 1,
        "id": addon_id,
        "version": "1.0.0",
        "name": {"en-US": format!("{tag} scan refresh fixture")},
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
    let content_sha256 =
        ersatzrs_infra::addons::calculate_addon_content_sha256(package_root).await?;
    sqlx::query(
        r#"INSERT INTO "AddonInstallation" (
               "AddonId", "RepositoryId", "Version", "Enabled", "PackageSha256",
               "ContentSha256", "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt"
           ) VALUES (?, NULL, '1.0.0', 1, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(addon_id)
    .bind("a".repeat(64))
    .bind(content_sha256)
    .bind(manifest.to_string())
    .bind(package_root.to_string_lossy().into_owned())
    .bind("2030-01-15T00:00:00Z")
    .bind("2030-01-15T00:00:00Z")
    .execute(pool)
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn seed_managed_library(
    pool: &DbPool,
    tag: &str,
    addon_id: &str,
    role: &str,
    library_root: &Path,
    manifest_path: &Path,
    folder_name: &str,
    provider_ids: &[&str],
) -> MysqlIntegrationResult<ManagedLibrary> {
    let media_source = sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (NULL)"#)
        .execute(pool)
        .await?;
    let media_source_id = db::last_insert_id(media_source);
    sqlx::query(r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (?)"#)
        .bind(media_source_id)
        .execute(pool)
        .await?;
    let library = sqlx::query(
        r#"INSERT INTO "Library" ("Name", "MediaKind", "MediaSourceId")
           VALUES (?, 7, ?)"#,
    )
    .bind(format!("{tag}-library-{role}"))
    .bind(media_source_id)
    .execute(pool)
    .await?;
    let library_id = db::last_insert_id(library);
    sqlx::query(r#"INSERT INTO "LocalLibrary" ("Id") VALUES (?)"#)
        .bind(library_id)
        .execute(pool)
        .await?;
    sqlx::query(r#"INSERT INTO "LibraryPath" ("LibraryId", "Path") VALUES (?, ?)"#)
        .bind(library_id)
        .bind(library_root.to_string_lossy().as_ref())
        .execute(pool)
        .await?;

    let list = sqlx::query(
        r#"INSERT INTO "AddonMediaList" (
               "AddonId", "ProviderId", "SourceUrl", "Name", "ItemCount",
               "ReviewState", "PublishedRevision"
           ) VALUES (?, ?, ?, ?, ?, 3, 1)"#,
    )
    .bind(addon_id)
    .bind(format!("{tag}/{role}"))
    .bind(format!("https://example.invalid/{tag}/{role}"))
    .bind(format!("{tag} {role} managed list"))
    .bind(i32::try_from(provider_ids.len())?)
    .execute(pool)
    .await?;
    let list_id = db::last_insert_id(list);
    sqlx::query(
        r#"INSERT INTO "AddonMediaListLibrary" (
               "AddonMediaListId", "LibraryId", "FolderName", "ManifestPath"
           ) VALUES (?, ?, ?, ?)"#,
    )
    .bind(list_id)
    .bind(library_id)
    .bind(folder_name)
    .bind(manifest_path.to_string_lossy().as_ref())
    .execute(pool)
    .await?;

    for (rank, provider_id) in provider_ids.iter().enumerate() {
        let title = format!("{tag} {}", provider_id.replace('-', " "));
        sqlx::query(
            r#"INSERT INTO "AddonMediaListItem" (
                   "AddonMediaListId", "ProviderId", "DisplayTitle", "Kind", "Rank",
                   "Title", "SourceUrl", "Availability", "DurationSeconds", "Included",
                   "Published", "ContentKind", "ProviderMetadataJson", "ProviderLiveness",
                   "EffectiveLiveness", "PublishedLiveness"
               ) VALUES (?, ?, ?, 4, ?, ?, ?, 0, 600, 1, 1, 4, ?, 1, 1, 1)"#,
        )
        .bind(list_id)
        .bind(provider_id)
        .bind(&title)
        .bind(i32::try_from(rank)?)
        .bind(&title)
        .bind(format!("https://media.example.invalid/{tag}/{provider_id}"))
        .bind(serde_json::json!({"title": title}).to_string())
        .execute(pool)
        .await?;
    }

    Ok(ManagedLibrary {
        library_id,
        list_id,
        playlist_id: 0,
    })
}

async fn managed_playlist_id(
    pool: &DbPool,
    library_id: EntityId,
) -> MysqlIntegrationResult<EntityId> {
    Ok(sqlx::query_scalar(
        r#"SELECT "Id" FROM "Playlist"
           WHERE "ManagedSource" LIKE ?
           ORDER BY "Id" LIMIT 1"#,
    )
    .bind(format!("remote-stream-playlist:v1:{library_id}:%"))
    .fetch_one(pool)
    .await?)
}

async fn seed_classic_fixture(
    pool: &DbPool,
    repositories: &Repositories,
    tag: &str,
    related: ManagedLibrary,
    unrelated: ManagedLibrary,
) -> MysqlIntegrationResult<ClassicFixture> {
    let resolution = sqlx::query(
        r#"INSERT INTO "Resolution" ("Name", "IsCustom", "Width", "Height")
           VALUES (?, 0, 1920, 1080)"#,
    )
    .bind(format!("{tag}-resolution"))
    .execute(pool)
    .await?;
    let resolution_id = db::last_insert_id(resolution);
    let profile = sqlx::query(
        r#"INSERT INTO "FFmpegProfile" (
               "Name", "ThreadCount", "TranscodeVideo", "TranscodeAudio",
               "ResolutionId", "ScalingBehavior", "VideoFormat", "AudioFormat",
               "VideoBitrate", "VideoBufferSize", "AudioBitrate", "AudioBufferSize",
               "NormalizeLoudnessMode", "AudioChannels", "AudioSampleRate",
               "HardwareAcceleration", "QsvExtraHardwareFrames", "VaapiDriver",
               "TonemapAlgorithm", "AllowBFrames", "BitDepth", "NormalizeFramerate",
               "NormalizeColors", "DeinterlaceVideo", "PadMode", "VideoPreset",
               "VideoTune"
           ) VALUES (
               ?, 0, 1, 1, ?, 0, 1, 1, 4000, 8000, 192, 384, 0, 2, 48000,
               0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 'p7', 'll'
           )"#,
    )
    .bind(format!("{tag}-profile"))
    .bind(resolution_id)
    .execute(pool)
    .await?;
    let profile_id = db::last_insert_id(profile);

    let target_channel = insert_channel(pool, profile_id, tag, "target", 1.0).await?;
    let oracle_channel = insert_channel(pool, profile_id, tag, "oracle", 2.0).await?;
    let unrelated_channel = insert_channel(pool, profile_id, tag, "unrelated", 3.0).await?;
    let related_schedule = seed_shuffle_schedule(
        pool,
        repositories,
        &format!("{tag}-schedule-related"),
        related.playlist_id,
    )
    .await?;
    let unrelated_schedule = seed_shuffle_schedule(
        pool,
        repositories,
        &format!("{tag}-schedule-unrelated"),
        unrelated.playlist_id,
    )
    .await?;

    Ok(ClassicFixture {
        related,
        target_playout_id: insert_classic_playout(pool, target_channel, related_schedule).await?,
        oracle_playout_id: insert_classic_playout(pool, oracle_channel, related_schedule).await?,
        unrelated_playout_id: insert_classic_playout(pool, unrelated_channel, unrelated_schedule)
            .await?,
    })
}

async fn insert_channel(
    pool: &DbPool,
    profile_id: EntityId,
    tag: &str,
    role: &str,
    sort_number: f64,
) -> MysqlIntegrationResult<EntityId> {
    let channel = sqlx::query(
        r#"INSERT INTO "Channel" (
               "UniqueId", "Number", "SortNumber", "Name", "Group",
               "FFmpegProfileId", "StreamingMode", "StreamSelectorMode",
               "SubtitleMode", "MusicVideoCreditsMode", "SongVideoMode",
               "PlayoutSource", "PlayoutMode", "TranscodeMode", "IdleBehavior",
               "IsEnabled", "ShowInEpg"
           ) VALUES (?, ?, ?, ?, 'ErsatzTV', ?, 1, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1)"#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(format!("{tag}-{role}"))
    .bind(sort_number)
    .bind(format!("{tag}-channel-{role}"))
    .bind(profile_id)
    .execute(pool)
    .await?;
    Ok(db::last_insert_id(channel))
}

async fn seed_shuffle_schedule(
    pool: &DbPool,
    repositories: &Repositories,
    name: &str,
    playlist_id: EntityId,
) -> MysqlIntegrationResult<EntityId> {
    let schedule = sqlx::query(
        r#"INSERT INTO "ProgramSchedule" (
               "FixedStartTimeBehavior", "KeepMultiPartEpisodesTogether", "Name",
               "RandomStartPoint", "ShuffleScheduleItems", "TreatCollectionsAsShows"
           ) VALUES (0, 1, ?, 0, 0, 0)"#,
    )
    .bind(name)
    .execute(pool)
    .await?;
    let schedule_id = db::last_insert_id(schedule);
    let item = sqlx::query(
        r#"INSERT INTO "ProgramScheduleItem" (
               "ProgramScheduleId", "CollectionType", "FillWithGroupMode", "GuideMode",
               "Index", "MarathonGroupBy", "MarathonShuffleGroups", "MarathonShuffleItems",
               "PlaybackOrder", "PlaylistId"
           ) VALUES (?, 6, 0, 0, 0, 0, 0, 0, 2, ?)"#,
    )
    .bind(schedule_id)
    .bind(playlist_id)
    .execute(pool)
    .await?;
    let schedule_item_id = db::last_insert_id(item);
    repositories
        .program_schedule
        .upsert_item(&ProgramScheduleItem {
            id: schedule_item_id,
            program_schedule_id: schedule_id,
            collection_type: CollectionType::Playlist,
            playlist_id: Some(playlist_id),
            playout_mode: PlayoutMode::One,
            playback_order: PlaybackOrder::Shuffle,
            sort_order: 0,
            ..ProgramScheduleItem::default()
        })
        .await?;
    repositories
        .program_schedule
        .upsert(&ProgramSchedule {
            id: schedule_id,
            name: name.to_owned(),
            item_ids: vec![schedule_item_id],
            ..ProgramSchedule::default()
        })
        .await?;
    Ok(schedule_id)
}

async fn insert_classic_playout(
    pool: &DbPool,
    channel_id: EntityId,
    schedule_id: EntityId,
) -> MysqlIntegrationResult<EntityId> {
    let playout = sqlx::query(
        r#"INSERT INTO "Playout" (
               "ChannelId", "ProgramScheduleId", "ScheduleKind", "Seed"
           ) VALUES (?, ?, 1, 123)"#,
    )
    .bind(channel_id)
    .bind(schedule_id)
    .execute(pool)
    .await?;
    Ok(db::last_insert_id(playout))
}

async fn run_classic_build(
    pool: &DbPool,
    locker: &EntityLocker,
    request: PlayoutBuildRequest,
    now: DateTime<Utc>,
) -> MysqlIntegrationResult {
    assert!(
        run_requested_playout_build(pool, locker, request, now, None).await?,
        "the MySQL production fixture must acquire the scheduler playout lock"
    );
    Ok(())
}

fn fixed_build_time() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2030, 1, 15, 10, 0, 0)
        .single()
        .expect("valid fixed MySQL build time")
}

fn edit_generated_nfo_title(
    library_root: &Path,
    folder_name: &str,
    provider_id: &str,
    edited_title: &str,
) -> MysqlIntegrationResult {
    let folder = library_root.join(folder_name);
    let sidecar: serde_json::Value =
        serde_json::from_slice(&fs::read(folder.join(".ersatzrs-playlist.json"))?)?;
    let nfo_file = sidecar
        .get("entries")
        .and_then(serde_json::Value::as_array)
        .and_then(|entries| {
            entries.iter().find(|entry| {
                entry.get("provider_id").and_then(serde_json::Value::as_str) == Some(provider_id)
            })
        })
        .and_then(|entry| entry.get("nfo_file"))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| std::io::Error::other("managed sidecar has no matching NFO"))?;
    let path = folder.join(nfo_file);
    let original = fs::read_to_string(&path)?;
    let title_start = original
        .find("<title>")
        .map(|index| index + "<title>".len())
        .ok_or_else(|| std::io::Error::other("generated NFO has no title"))?;
    let title_finish = original[title_start..]
        .find("</title>")
        .map(|index| index + title_start)
        .ok_or_else(|| std::io::Error::other("generated NFO title is not closed"))?;
    let mut edited = original.clone();
    edited.replace_range(title_start..title_finish, edited_title);
    assert_ne!(
        edited, original,
        "the NFO edit must change effective metadata"
    );
    fs::write(path, edited)?;
    Ok(())
}

async fn remote_stream_media_id(
    pool: &DbPool,
    addon_id: &str,
    provider_id: &str,
) -> MysqlIntegrationResult<EntityId> {
    Ok(sqlx::query_scalar(
        r#"SELECT "Id" FROM "RemoteStream"
           WHERE "AddonId" = ? AND "ProviderId" = ?"#,
    )
    .bind(addon_id)
    .bind(provider_id)
    .fetch_one(pool)
    .await?)
}

async fn remote_stream_title(
    pool: &DbPool,
    addon_id: &str,
    provider_id: &str,
) -> MysqlIntegrationResult<Option<String>> {
    Ok(sqlx::query_scalar(
        r#"SELECT "metadata"."Title"
           FROM "RemoteStreamMetadata" AS "metadata"
           JOIN "RemoteStream" AS "stream"
             ON "stream"."Id" = "metadata"."RemoteStreamId"
           WHERE "stream"."AddonId" = ? AND "stream"."ProviderId" = ?"#,
    )
    .bind(addon_id)
    .bind(provider_id)
    .fetch_one(pool)
    .await?)
}

async fn playlist_membership(
    pool: &DbPool,
    playlist_id: EntityId,
) -> MysqlIntegrationResult<Vec<(i64, EntityId)>> {
    Ok(sqlx::query_as(
        r#"SELECT "Index", "MediaItemId" FROM "PlaylistItem"
           WHERE "PlaylistId" = ? ORDER BY "Index", "Id""#,
    )
    .bind(playlist_id)
    .fetch_all(pool)
    .await?)
}

async fn playlist_rows(
    pool: &DbPool,
    playlist_id: EntityId,
) -> MysqlIntegrationResult<Vec<String>> {
    mysql_keyed_rows(pool, "PlaylistItem", "PlaylistId", playlist_id, &[]).await
}

type TimelineRow = (EntityId, String, String, String, String, Option<String>);

async fn playout_timeline(
    pool: &DbPool,
    playout_id: EntityId,
) -> MysqlIntegrationResult<Vec<TimelineRow>> {
    Ok(sqlx::query_as(
        r#"SELECT "MediaItemId", "Start", "Finish", "InPoint", "OutPoint",
                  "SchedulingContext"
           FROM "PlayoutItem"
           WHERE "PlayoutId" = ?
           ORDER BY "Start", "Id""#,
    )
    .bind(playout_id)
    .fetch_all(pool)
    .await?)
}

fn assert_uninterrupted_timeline(actual: &[TimelineRow], expected: &[TimelineRow], message: &str) {
    assert!(
        actual.len() == expected.len() || actual.len() == expected.len() + 1,
        "{message}: only one horizon-extension row is allowed; actual_len={}, expected_len={}",
        actual.len(),
        expected.len(),
    );

    if !actual.starts_with(expected) {
        let first_mismatch = actual
            .iter()
            .zip(expected)
            .position(|(actual, expected)| actual != expected)
            .unwrap_or_else(|| actual.len().min(expected.len()));
        panic!(
            "{message}: first_mismatch={first_mismatch}, actual={:?}, expected={:?}",
            actual.get(first_mismatch),
            expected.get(first_mismatch),
        );
    }

    if let Some(extension) = actual.get(expected.len()) {
        let previous = expected
            .last()
            .expect("a horizon extension requires an existing timeline row");
        assert_eq!(
            extension.1, previous.2,
            "{message}: the one-item horizon extension must be contiguous"
        );
    }
}

type RefreshRow = (EntityId, Option<EntityId>, i64, i64, String);

fn pending_refresh_row(playout_id: EntityId, library_id: EntityId, generation: i64) -> RefreshRow {
    (
        playout_id,
        Some(library_id),
        generation,
        0,
        ScanPlayoutRefreshReason::RemoteStreamMembershipReplaced
            .as_str()
            .to_owned(),
    )
}

async fn fixture_refresh_rows(
    pool: &DbPool,
    fixture: ClassicFixture,
) -> MysqlIntegrationResult<Vec<RefreshRow>> {
    Ok(sqlx::query_as(
        r#"SELECT "PlayoutId", "LibraryId", "Generation", "State", "Reason"
           FROM "ScanPlayoutRefreshIntent"
           WHERE "PlayoutId" IN (?, ?, ?)
           ORDER BY "PlayoutId""#,
    )
    .bind(fixture.target_playout_id)
    .bind(fixture.oracle_playout_id)
    .bind(fixture.unrelated_playout_id)
    .fetch_all(pool)
    .await?)
}

async fn scan_resolution_generation(
    pool: &DbPool,
    library_id: EntityId,
) -> MysqlIntegrationResult<i64> {
    Ok(sqlx::query_scalar(
        r#"SELECT "Generation" FROM "ScanRefreshResolution" WHERE "LibraryId" = ?"#,
    )
    .bind(library_id)
    .fetch_one(pool)
    .await?)
}

async fn assert_exact_pending_refreshes(
    pool: &DbPool,
    fixture: ClassicFixture,
) -> MysqlIntegrationResult {
    let generation = scan_resolution_generation(pool, fixture.related.library_id).await?;
    let mut expected = vec![
        pending_refresh_row(
            fixture.target_playout_id,
            fixture.related.library_id,
            generation,
        ),
        pending_refresh_row(
            fixture.oracle_playout_id,
            fixture.related.library_id,
            generation,
        ),
    ];
    expected.sort_by_key(|row| row.0);
    assert_eq!(
        fixture_refresh_rows(pool, fixture).await?,
        expected,
        "the scan commit must contain exactly two pending related Refresh intents"
    );
    let resolution: (i64, i64, String, Option<String>) = sqlx::query_as(
        r#"SELECT "Generation", "State", "Reason", "Diagnostic"
           FROM "ScanRefreshResolution" WHERE "LibraryId" = ?"#,
    )
    .bind(fixture.related.library_id)
    .fetch_one(pool)
    .await?;
    assert_eq!(resolution.0, generation);
    assert_eq!(resolution.1, 1, "scan impact resolution must be committed");
    assert_eq!(
        resolution.2,
        ScanPlayoutRefreshReason::RemoteStreamMembershipReplaced.as_str()
    );
    assert_eq!(resolution.3, None);
    Ok(())
}

fn restarted_app_state(
    pool: DbPool,
    playout_locker: EntityLocker,
) -> (
    AppState,
    tokio::sync::mpsc::UnboundedReceiver<PlayoutBuildRequest>,
) {
    let (playout_build_tx, playout_build_rx) = tokio::sync::mpsc::unbounded_channel();
    let (scan_work_tx, _scan_work_rx) = tokio::sync::mpsc::unbounded_channel();
    let state = AppState::new_with_scan_queue(
        pool,
        DatabaseBackend::MySql,
        SystemStartup {
            database_ready: true,
            schema_status: DatabaseSchemaStatus::default(),
        },
        ersatzrs_ffmpeg::FfmpegRuntimeReport::for_tests(),
        Uuid::nil(),
        1,
        IptvAccessTokenVerifier::disabled(),
        playout_build_tx,
        scan_work_tx,
        playout_locker,
    );
    (state, playout_build_rx)
}

#[derive(Debug, PartialEq, Eq)]
struct PersistedClassicRows {
    playout: Vec<String>,
    playout_items: Vec<String>,
    playout_item_graphics: Vec<String>,
    playout_item_watermarks: Vec<String>,
    playout_gaps: Vec<String>,
    playout_history: Vec<String>,
    rerun_history: Vec<String>,
    playout_anchor: Vec<String>,
    schedule_item_enumerator: Vec<String>,
    program_schedule_anchors: Vec<String>,
    collection_enumerators: Vec<String>,
    fill_group_indices: Vec<String>,
    fill_group_enumerators: Vec<String>,
    classic_checkpoints: Vec<String>,
    playout_templates: Vec<String>,
    program_schedule_alternates: Vec<String>,
    build_status: Vec<String>,
    scan_refresh_intents: Vec<String>,
}

#[allow(clippy::too_many_lines)]
async fn persisted_classic_rows(
    pool: &DbPool,
    playout_id: EntityId,
    equivalent_playout: bool,
) -> MysqlIntegrationResult<PersistedClassicRows> {
    let playout_ignored: &[&str] = if equivalent_playout {
        &["Id", "ChannelId"]
    } else {
        &[]
    };
    let generated_ignored: &[&str] = if equivalent_playout {
        &["Id", "PlayoutId"]
    } else {
        &[]
    };
    let playout_id_ignored: &[&str] = if equivalent_playout {
        &["PlayoutId"]
    } else {
        &[]
    };
    let item_child_ignored: &[&str] = if equivalent_playout {
        &["PlayoutItemId"]
    } else {
        &[]
    };
    let anchor_ignored: &[&str] = if equivalent_playout {
        &["Id", "PlayoutId"]
    } else {
        &[]
    };
    let schedule_state_ignored: &[&str] = if equivalent_playout {
        &["PlayoutAnchorPlayoutId"]
    } else {
        &[]
    };
    let collection_state_ignored: &[&str] = if equivalent_playout {
        &["PlayoutProgramScheduleAnchorId"]
    } else {
        &[]
    };
    let fill_state_ignored: &[&str] = if equivalent_playout {
        &["PlayoutScheduleItemFillGroupIndexId"]
    } else {
        &[]
    };

    Ok(PersistedClassicRows {
        playout: mysql_keyed_rows(pool, "Playout", "Id", playout_id, playout_ignored).await?,
        playout_items: mysql_keyed_rows(
            pool,
            "PlayoutItem",
            "PlayoutId",
            playout_id,
            generated_ignored,
        )
        .await?,
        playout_item_graphics: mysql_child_rows(
            pool,
            "PlayoutItemGraphicsElement",
            "PlayoutItemId",
            "PlayoutItem",
            "Id",
            playout_id,
            item_child_ignored,
        )
        .await?,
        playout_item_watermarks: mysql_child_rows(
            pool,
            "PlayoutItemWatermark",
            "PlayoutItemId",
            "PlayoutItem",
            "Id",
            playout_id,
            item_child_ignored,
        )
        .await?,
        playout_gaps: mysql_keyed_rows(
            pool,
            "PlayoutGap",
            "PlayoutId",
            playout_id,
            generated_ignored,
        )
        .await?,
        playout_history: mysql_keyed_rows(
            pool,
            "PlayoutHistory",
            "PlayoutId",
            playout_id,
            generated_ignored,
        )
        .await?,
        rerun_history: mysql_keyed_rows(
            pool,
            "RerunHistory",
            "PlayoutId",
            playout_id,
            generated_ignored,
        )
        .await?,
        playout_anchor: mysql_keyed_rows(
            pool,
            "PlayoutAnchor",
            "PlayoutId",
            playout_id,
            playout_id_ignored,
        )
        .await?,
        schedule_item_enumerator: mysql_keyed_rows(
            pool,
            "ScheduleItemsEnumeratorState",
            "PlayoutAnchorPlayoutId",
            playout_id,
            schedule_state_ignored,
        )
        .await?,
        program_schedule_anchors: mysql_keyed_rows(
            pool,
            "PlayoutProgramScheduleAnchor",
            "PlayoutId",
            playout_id,
            anchor_ignored,
        )
        .await?,
        collection_enumerators: mysql_child_rows(
            pool,
            "CollectionEnumeratorState",
            "PlayoutProgramScheduleAnchorId",
            "PlayoutProgramScheduleAnchor",
            "Id",
            playout_id,
            collection_state_ignored,
        )
        .await?,
        fill_group_indices: mysql_keyed_rows(
            pool,
            "PlayoutScheduleItemFillGroupIndex",
            "PlayoutId",
            playout_id,
            generated_ignored,
        )
        .await?,
        fill_group_enumerators: mysql_child_rows(
            pool,
            "FillGroupEnumeratorState",
            "PlayoutScheduleItemFillGroupIndexId",
            "PlayoutScheduleItemFillGroupIndex",
            "Id",
            playout_id,
            fill_state_ignored,
        )
        .await?,
        classic_checkpoints: mysql_keyed_rows(
            pool,
            "PlayoutClassicCheckpoint",
            "PlayoutId",
            playout_id,
            playout_id_ignored,
        )
        .await?,
        playout_templates: mysql_keyed_rows(
            pool,
            "PlayoutTemplate",
            "PlayoutId",
            playout_id,
            generated_ignored,
        )
        .await?,
        program_schedule_alternates: mysql_keyed_rows(
            pool,
            "ProgramScheduleAlternate",
            "PlayoutId",
            playout_id,
            generated_ignored,
        )
        .await?,
        build_status: mysql_keyed_rows(
            pool,
            "PlayoutBuildStatus",
            "PlayoutId",
            playout_id,
            playout_id_ignored,
        )
        .await?,
        scan_refresh_intents: mysql_keyed_rows(
            pool,
            "ScanPlayoutRefreshIntent",
            "PlayoutId",
            playout_id,
            playout_id_ignored,
        )
        .await?,
    })
}

async fn mysql_keyed_rows(
    pool: &DbPool,
    table: &str,
    key_column: &str,
    key: EntityId,
    ignored_columns: &[&str],
) -> MysqlIntegrationResult<Vec<String>> {
    let columns = mysql_table_columns(pool, table, ignored_columns).await?;
    let projection = mysql_row_projection("fixture_row", &columns);
    let sql = format!(
        "SELECT {projection} FROM {} AS {} WHERE {}.{} = ?",
        mysql_quote_identifier(table),
        mysql_quote_identifier("fixture_row"),
        mysql_quote_identifier("fixture_row"),
        mysql_quote_identifier(key_column),
    );
    let mut rows = sqlx::query_scalar::<_, String>(db::assert_sql_safe(sql))
        .bind(key)
        .fetch_all(pool)
        .await?;
    rows.sort();
    Ok(rows)
}

#[allow(clippy::too_many_arguments)]
async fn mysql_child_rows(
    pool: &DbPool,
    child_table: &str,
    child_foreign_key: &str,
    parent_table: &str,
    parent_primary_key: &str,
    playout_id: EntityId,
    ignored_columns: &[&str],
) -> MysqlIntegrationResult<Vec<String>> {
    let columns = mysql_table_columns(pool, child_table, ignored_columns).await?;
    let projection = mysql_row_projection("child", &columns);
    let sql = format!(
        "SELECT {projection} FROM {} AS {} JOIN {} AS {} ON {}.{} = {}.{} \
         WHERE {}.{} = ?",
        mysql_quote_identifier(child_table),
        mysql_quote_identifier("child"),
        mysql_quote_identifier(parent_table),
        mysql_quote_identifier("parent"),
        mysql_quote_identifier("parent"),
        mysql_quote_identifier(parent_primary_key),
        mysql_quote_identifier("child"),
        mysql_quote_identifier(child_foreign_key),
        mysql_quote_identifier("parent"),
        mysql_quote_identifier("PlayoutId"),
    );
    let mut rows = sqlx::query_scalar::<_, String>(db::assert_sql_safe(sql))
        .bind(playout_id)
        .fetch_all(pool)
        .await?;
    rows.sort();
    Ok(rows)
}

async fn mysql_table_columns(
    pool: &DbPool,
    table: &str,
    ignored_columns: &[&str],
) -> MysqlIntegrationResult<Vec<String>> {
    let columns = sqlx::query_scalar::<_, String>(
        r#"SELECT "COLUMN_NAME"
           FROM "INFORMATION_SCHEMA"."COLUMNS"
           WHERE "TABLE_SCHEMA" = DATABASE() AND "TABLE_NAME" = ?
           ORDER BY "ORDINAL_POSITION""#,
    )
    .bind(table)
    .fetch_all(pool)
    .await?
    .into_iter()
    .filter(|column| !ignored_columns.contains(&column.as_str()))
    .collect::<Vec<_>>();
    assert!(!columns.is_empty(), "MySQL table {table} must exist");
    Ok(columns)
}

fn mysql_row_projection(alias: &str, columns: &[String]) -> String {
    let values = columns
        .iter()
        .map(|column| {
            format!(
                "COALESCE(HEX(CAST({}.{} AS BINARY)), '<NULL>')",
                mysql_quote_identifier(alias),
                mysql_quote_identifier(column),
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("CONCAT_WS(CHAR(31), {values})")
}

fn mysql_quote_identifier(identifier: &str) -> String {
    format!("`{}`", identifier.replace('`', "``"))
}

#[allow(clippy::too_many_lines)]
async fn cleanup_mysql_production_fixture(database_url: &str, tag: &str) -> MysqlIntegrationResult {
    let pool = db::init(database_url).await?;
    let addon_id = fixture_addon_id(tag);
    let library_pattern = format!("{tag}-library-%");
    let channel_pattern = format!("{tag}-channel-%");
    let schedule_pattern = format!("{tag}-schedule-%");
    let libraries =
        sqlx::query(r#"SELECT "Id", "MediaSourceId" FROM "Library" WHERE "Name" LIKE ?"#)
            .bind(&library_pattern)
            .fetch_all(&pool)
            .await?
            .into_iter()
            .map(|row| {
                Ok::<_, sqlx::Error>((
                    row.try_get::<EntityId, _>("Id")?,
                    row.try_get::<EntityId, _>("MediaSourceId")?,
                ))
            })
            .collect::<Result<Vec<_>, _>>()?;

    let mut tx = db::begin_write(&pool).await?;
    sqlx::query(
        r#"DELETE FROM "Playout"
           WHERE "ChannelId" IN (SELECT "Id" FROM "Channel" WHERE "Name" LIKE ?)
              OR "ProgramScheduleId" IN (
                     SELECT "Id" FROM "ProgramSchedule" WHERE "Name" LIKE ?
                 )"#,
    )
    .bind(&channel_pattern)
    .bind(&schedule_pattern)
    .execute(&mut *tx)
    .await?;
    sqlx::query(r#"DELETE FROM "ProgramSchedule" WHERE "Name" LIKE ?"#)
        .bind(&schedule_pattern)
        .execute(&mut *tx)
        .await?;
    sqlx::query(r#"DELETE FROM "Channel" WHERE "Name" LIKE ?"#)
        .bind(&channel_pattern)
        .execute(&mut *tx)
        .await?;
    sqlx::query(r#"DELETE FROM "FFmpegProfile" WHERE "Name" = ?"#)
        .bind(format!("{tag}-profile"))
        .execute(&mut *tx)
        .await?;
    sqlx::query(r#"DELETE FROM "Resolution" WHERE "Name" = ?"#)
        .bind(format!("{tag}-resolution"))
        .execute(&mut *tx)
        .await?;
    for (library_id, _) in &libraries {
        sqlx::query(r#"DELETE FROM "Playlist" WHERE "ManagedSource" LIKE ?"#)
            .bind(format!("remote-stream-playlist:v1:{library_id}:%"))
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query(r#"DELETE FROM "AddonMediaList" WHERE "AddonId" = ?"#)
        .bind(&addon_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(r#"DELETE FROM "Library" WHERE "Name" LIKE ?"#)
        .bind(&library_pattern)
        .execute(&mut *tx)
        .await?;
    for (_, media_source_id) in &libraries {
        sqlx::query(r#"DELETE FROM "MediaSource" WHERE "Id" = ?"#)
            .bind(media_source_id)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query(r#"DELETE FROM "AddonInstallation" WHERE "AddonId" = ?"#)
        .bind(&addon_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    let remaining: i64 = sqlx::query_scalar(
        r#"SELECT
             (SELECT COUNT(*) FROM "AddonInstallation" WHERE "AddonId" = ?) +
             (SELECT COUNT(*) FROM "AddonMediaList" WHERE "AddonId" = ?) +
             (SELECT COUNT(*) FROM "Library" WHERE "Name" LIKE ?) +
             (SELECT COUNT(*) FROM "Channel" WHERE "Name" LIKE ?) +
             (SELECT COUNT(*) FROM "ProgramSchedule" WHERE "Name" LIKE ?) +
             (SELECT COUNT(*) FROM "FFmpegProfile" WHERE "Name" = ?) +
             (SELECT COUNT(*) FROM "Resolution" WHERE "Name" = ?)"#,
    )
    .bind(&addon_id)
    .bind(&addon_id)
    .bind(&library_pattern)
    .bind(&channel_pattern)
    .bind(&schedule_pattern)
    .bind(format!("{tag}-profile"))
    .bind(format!("{tag}-resolution"))
    .fetch_one(&pool)
    .await?;
    if remaining != 0 {
        return Err(std::io::Error::other("tagged MySQL fixture rows remain after cleanup").into());
    }
    pool.close().await;
    Ok(())
}
