//! Shared media-server (Plex/Jellyfin/Emby) scanner helpers.
//!
//! Ports the per-scanner "missing item" passes from ErsatzTV's
//! `MediaServerXLibraryScanner` family. After the add/update pass, ErsatzTV
//! diffs the set of item ids it just saw against what is stored and trashes
//! the leftovers (`existing.Except(incomingItemIds)` →
//! `FlagFileNotFound`), and separately flags items whose backing local file
//! has gone missing as `Unavailable`/`RemoteOnly`. None of that was ported, so
//! moved/deleted media-server items stayed `Normal` forever — never surfacing
//! in `/media/trash` or the File Not Found / Unavailable health checks.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use crate::repositories::SqlxResultExt as _;
use ersatzrs_core::path_replacement::{replace_path, PathPlatform, PathReplacement};
use ersatzrs_domain::{
    classify_alternate_version, classify_extra_file_stem, classify_extra_video_path,
    normalize_group_key, normalize_sub_kind_token, ContentKind,
};
use sqlx::Row;

use crate::db::{assert_sql_safe, DbPool};

pub(crate) mod jellyfin_emby;
pub(crate) mod teletext;

/// State values mirroring `ErsatzTV.Core.Domain.MediaItemState`.
pub(crate) const STATE_NORMAL: i64 = 0;
pub(crate) const STATE_FILE_NOT_FOUND: i64 = 1;
pub(crate) const STATE_UNAVAILABLE: i64 = 2;
pub(crate) const STATE_REMOTE_ONLY: i64 = 3;

/// Return whether an unchanged provider item may skip metadata refresh.
///
/// Mirrors `MediaServerMovieLibraryScanner.ShouldScanItem` in ErsatzTV:
/// `Normal` and `RemoteOnly` rows short-circuit when their provider `ETag` is
/// unchanged, while `FileNotFound` and `Unavailable` rows rescan so they can
/// recover when the provider or local file becomes usable again.
pub(crate) fn provider_item_state_allows_etag_skip(state: i64) -> bool {
    matches!(state, STATE_NORMAL | STATE_REMOTE_ONLY)
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum EpisodeProvider {
    Plex,
    Jellyfin,
    Emby,
}

/// Repair parent membership even when provider metadata has not changed.
/// Both identities are scoped to the scanned library; other episode data stays intact.
pub(crate) async fn reconcile_episode_season(
    pool: &DbPool,
    provider: EpisodeProvider,
    library_id: i64,
    provider_id: &str,
    season_id: i64,
) -> Result<u64, sqlx::Error> {
    let (table, key) = match provider {
        EpisodeProvider::Plex => ("PlexEpisode", "Key"),
        EpisodeProvider::Jellyfin => ("JellyfinEpisode", "ItemId"),
        EpisodeProvider::Emby => ("EmbyEpisode", "ItemId"),
    };
    let sql = format!(
        r#"UPDATE "Episode" SET "SeasonId" = ?
           WHERE "SeasonId" != ? AND "Id" IN (
               SELECT provider."Id" FROM "{table}" provider
               JOIN "MediaItem" mi ON mi."Id" = provider."Id"
               JOIN "LibraryPath" lp ON lp."Id" = mi."LibraryPathId"
               WHERE provider."{key}" = ? AND lp."LibraryId" = ?
           ) AND EXISTS (
               SELECT 1 FROM "Season" s
               JOIN "MediaItem" mi ON mi."Id" = s."Id"
               JOIN "LibraryPath" lp ON lp."Id" = mi."LibraryPathId"
               WHERE s."Id" = ? AND s."Id" > 0 AND lp."LibraryId" = ?
           )"#,
    );
    Ok(sqlx::query(assert_sql_safe(&sql))
        .bind(season_id)
        .bind(season_id)
        .bind(provider_id)
        .bind(library_id)
        .bind(season_id)
        .bind(library_id)
        .execute(pool)
        .await?
        .rows_affected())
}

#[derive(Debug, Clone)]
pub(crate) struct ProviderVariantCandidate {
    pub item_id: i64,
    pub title: String,
    pub path: Option<String>,
    pub disambiguator: String,
    pub tags: Vec<String>,
}

/// Platform whose separator conventions the running host uses, for resolving
/// a replaced local path before the existence check.
#[cfg(windows)]
const HOST_PLATFORM: PathPlatform = PathPlatform::Windows;
#[cfg(not(windows))]
const HOST_PLATFORM: PathPlatform = PathPlatform::Unix;

/// Flag media-server items whose server-side id is no longer present in the
/// just-completed scan as `MediaItemState::FileNotFound` (`State = 1`).
///
/// `table` is the per-kind media-server table (`JellyfinMovie`, `EmbyEpisode`,
/// `PlexShow`, …) and `id_column` its server-id column (`ItemId` for
/// Jellyfin/Emby, `Key` for Plex). Rows join `MediaItem` on the shared primary
/// key and are scoped to `library_id` through `LibraryPath`. Items already at
/// `FileNotFound` are skipped so the log and the write stay meaningful.
///
/// Returns the number of items newly flagged. Port of the
/// `existing.Except(incomingItemIds)` → `FlagFileNotFound` pass.
///
/// # Errors
///
/// Returns an error when a database read or write fails.
pub async fn flag_missing_media_server_items<S: std::hash::BuildHasher>(
    pool: &DbPool,
    table: &str,
    id_column: &str,
    library_id: i64,
    present_item_ids: &HashSet<String, S>,
) -> Result<u64, sqlx::Error> {
    let select = format!(
        r#"SELECT "t"."Id" AS "Id", "t"."{id_column}" AS "ItemId"
           FROM "{table}" AS "t"
           JOIN "MediaItem" AS "mi" ON "mi"."Id" = "t"."Id"
           JOIN "LibraryPath" AS "lp" ON "lp"."Id" = "mi"."LibraryPathId"
           WHERE "lp"."LibraryId" = ? AND "mi"."State" <> {STATE_FILE_NOT_FOUND}"#
    );
    let rows = sqlx::query(assert_sql_safe(&select))
        .bind(library_id)
        .fetch_all(pool)
        .await?;

    let mut missing_ids: Vec<i64> = Vec::new();
    for row in &rows {
        let item_id: Option<String> = row.try_get("ItemId")?;
        let present = item_id
            .as_deref()
            .is_some_and(|id| present_item_ids.contains(id));
        if !present {
            missing_ids.push(row.try_get("Id")?);
        }
    }

    flag_media_items(pool, &missing_ids, STATE_FILE_NOT_FOUND).await?;
    Ok(missing_ids.len() as u64)
}

/// Flag media-server items that are still present on the server but whose
/// resolved local file is missing.
///
/// With `supports_remote_streaming` the item becomes `RemoteOnly` (`State = 3`,
/// still playable via the server); otherwise `Unavailable` (`State = 2`). A
/// present local file restores a previously `RemoteOnly`/`Unavailable` item to
/// `Normal`. Items absent from `present_item_ids` are skipped — the
/// [`flag_missing_media_server_items`] trash pass already owns them. Only leaf
/// tables with media files (`*Movie`, `*Episode`) should be passed; `table`'s
/// rows join `MediaVersion`/`MediaFile` through the shared primary key.
///
/// Port of the `ShouldScanItem` `FlagRemoteOnly`/`FlagUnavailable` branch in
/// ErsatzTV's `MediaServerXLibraryScanner`.
///
/// # Errors
///
/// Returns an error when a database read or write fails.
pub async fn flag_unavailable_media_server_items<S: std::hash::BuildHasher>(
    pool: &DbPool,
    table: &str,
    id_column: &str,
    library_id: i64,
    present_item_ids: &HashSet<String, S>,
    replacements: &[PathReplacement],
    supports_remote_streaming: bool,
) -> Result<(), sqlx::Error> {
    let select = format!(
        r#"SELECT "t"."Id" AS "Id", "t"."{id_column}" AS "ItemId",
                  "mf"."Path" AS "Path", "mi"."State" AS "State"
           FROM "{table}" AS "t"
           JOIN "MediaItem" AS "mi" ON "mi"."Id" = "t"."Id"
           JOIN "LibraryPath" AS "lp" ON "lp"."Id" = "mi"."LibraryPathId"
           JOIN "MediaVersion" AS "mv"
             ON "t"."Id" = COALESCE("mv"."MovieId", "mv"."EpisodeId")
           JOIN "MediaFile" AS "mf" ON "mv"."Id" = "mf"."MediaVersionId"
           WHERE "lp"."LibraryId" = ?"#
    );
    let rows = sqlx::query(assert_sql_safe(&select))
        .bind(library_id)
        .fetch_all(pool)
        .await?;

    let missing_state = if supports_remote_streaming {
        STATE_REMOTE_ONLY
    } else {
        STATE_UNAVAILABLE
    };

    let mut to_missing: Vec<i64> = Vec::new();
    let mut to_normal: Vec<i64> = Vec::new();
    for row in &rows {
        let item_id: Option<String> = row.try_get("ItemId")?;
        // Only touch items the server still reports; trashed items are handled
        // by the FileNotFound pass and must not be clobbered here.
        if !item_id
            .as_deref()
            .is_some_and(|id| present_item_ids.contains(id))
        {
            continue;
        }

        let id: i64 = row.try_get("Id")?;
        let state: i64 = row.try_get("State")?;
        let server_path: Option<String> = row.try_get("Path")?;
        let exists = server_path.as_deref().is_some_and(|path| {
            let local = replace_path(replacements, path, HOST_PLATFORM);
            Path::new(&local).exists()
        });

        if exists {
            if state == STATE_REMOTE_ONLY || state == STATE_UNAVAILABLE {
                to_normal.push(id);
            }
        } else if state != missing_state {
            tracing::info!(
                table,
                id,
                path = server_path.as_deref().unwrap_or_default(),
                "flagging media-server item with missing local file"
            );
            to_missing.push(id);
        }
    }

    flag_media_items(pool, &to_missing, missing_state).await?;
    flag_media_items(pool, &to_normal, STATE_NORMAL).await?;
    Ok(())
}

/// Load a media server's path replacements as a `PathReplacement` set, mirroring
/// the IPTV stream loader (`source_platform` inferred from each source path).
///
/// `table`/`path_column`/`fk_column` vary per server
/// (`PlexPathReplacement`/`PlexPath`/`PlexMediaSourceId`, etc.).
///
/// # Errors
///
/// Returns an error when the read fails.
pub async fn load_media_server_path_replacements(
    pool: &DbPool,
    table: &str,
    path_column: &str,
    fk_column: &str,
    media_source_id: i64,
) -> Result<Vec<PathReplacement>, sqlx::Error> {
    load_path_replacements_from(pool, table, path_column, fk_column, media_source_id).await
}

pub(crate) async fn load_path_replacements_from<'e, E>(
    executor: E,
    table: &str,
    path_column: &str,
    fk_column: &str,
    media_source_id: i64,
) -> Result<Vec<PathReplacement>, sqlx::Error>
where
    E: sqlx::Executor<'e, Database = crate::db::Db>,
{
    let sql = format!(
        r#"SELECT "{path_column}" AS "SourcePath", "LocalPath" AS "LocalPath"
           FROM "{table}" WHERE "{fk_column}" = ? ORDER BY "Id""#
    );
    let rows = sqlx::query(assert_sql_safe(&sql))
        .bind(media_source_id)
        .fetch_all(executor)
        .await?;

    let mut replacements = Vec::new();
    for row in &rows {
        let source_path: Option<String> = row.try_get("SourcePath")?;
        let replacement_path: Option<String> = row.try_get("LocalPath")?;
        if let (Some(source_path), Some(replacement_path)) = (source_path, replacement_path) {
            let source_platform = if source_path.contains('\\') || source_path.contains(':') {
                PathPlatform::Windows
            } else {
                PathPlatform::Unix
            };
            replacements.push(PathReplacement {
                source_platform,
                source_path,
                replacement_path,
            });
        }
    }
    Ok(replacements)
}

/// Classify and link ADR 0018 content variants for provider-backed rows.
///
/// This intentionally runs after the normal provider sync pass and uses only
/// server metadata already present in the page response. It keeps existing
/// browse/search defaults intact by writing `MediaItem.ContentKind` /
/// `SubKind` / `ParentMediaItemId`, the same columns the local scanner uses.
///
/// # Errors
///
/// Returns an error when a database write fails.
pub(crate) async fn apply_provider_content_variants(
    pool: &DbPool,
    candidates: &[ProviderVariantCandidate],
) -> Result<(), sqlx::Error> {
    let mut alternate_rows = Vec::new();

    for candidate in candidates {
        if let Some(sub_kind) = provider_extra_sub_kind(candidate) {
            update_content_variant(
                pool,
                candidate.item_id,
                ContentKind::Extra,
                Some(&sub_kind),
                None,
            )
            .await?;
            continue;
        }

        let path_ref = candidate.path.as_deref().map(Path::new);
        let classification = classify_alternate_version(Path::new("/"), &candidate.title, path_ref);
        alternate_rows.push((candidate, classification));
    }

    let mut groups = BTreeMap::<(String, String), Vec<_>>::new();
    for (candidate, classification) in alternate_rows {
        let key = (
            normalize_group_key(&candidate.disambiguator),
            normalize_group_key(&classification.base_title),
        );
        if key.1.is_empty() {
            update_content_variant(pool, candidate.item_id, ContentKind::Primary, None, None)
                .await?;
            continue;
        }
        groups
            .entry(key)
            .or_default()
            .push((candidate, classification));
    }

    for (_, mut group) in groups {
        group.sort_by(|left, right| {
            left.0
                .path
                .cmp(&right.0.path)
                .then_with(|| left.0.item_id.cmp(&right.0.item_id))
        });
        apply_alternate_group(pool, &group).await?;
    }

    Ok(())
}

/// Resolve a provider media row id in a library from its server-side id.
///
/// # Errors
///
/// Returns an error when the database read fails.
pub(crate) async fn provider_media_item_id(
    pool: &DbPool,
    table: &str,
    id_column: &str,
    library_id: i64,
    provider_id: &str,
) -> Result<Option<i64>, sqlx::Error> {
    let sql = format!(
        r#"SELECT "t"."Id" AS "Id"
           FROM "{table}" AS "t"
           JOIN "MediaItem" AS "mi" ON "mi"."Id" = "t"."Id"
           JOIN "LibraryPath" AS "lp" ON "lp"."Id" = "mi"."LibraryPathId"
           WHERE "lp"."LibraryId" = ? AND "t"."{id_column}" = ?
           LIMIT 1"#
    );
    sqlx::query(assert_sql_safe(&sql))
        .bind(library_id)
        .bind(provider_id)
        .fetch_optional(pool)
        .await?
        .map(|row| row.try_get("Id"))
        .transpose()
}

fn provider_extra_sub_kind(candidate: &ProviderVariantCandidate) -> Option<String> {
    candidate
        .tags
        .iter()
        .find_map(|tag| known_extra_sub_kind(tag))
        .or_else(|| classify_extra_file_stem(&candidate.title).map(|(_, sub_kind)| sub_kind))
        .or_else(|| {
            candidate
                .path
                .as_deref()
                .and_then(|path| classify_extra_video_path(Path::new(path)))
        })
}

fn known_extra_sub_kind(value: &str) -> Option<String> {
    let token = normalize_sub_kind_token(value)?;
    matches!(
        token.as_str(),
        "trailer"
            | "behind_the_scenes"
            | "deleted_scene"
            | "featurette"
            | "interview"
            | "scene"
            | "short"
            | "other"
    )
    .then_some(token)
}

async fn apply_alternate_group(
    pool: &DbPool,
    group: &[(
        &ProviderVariantCandidate,
        ersatzrs_domain::AlternateVersionClassification,
    )],
) -> Result<(), sqlx::Error> {
    let alternate_count = group
        .iter()
        .filter(|(_, classification)| classification.sub_kind.is_some())
        .count();
    if alternate_count == 0 {
        for (candidate, _) in group {
            update_content_variant(pool, candidate.item_id, ContentKind::Primary, None, None)
                .await?;
        }
        return Ok(());
    }

    if group.len() == 1 {
        let (candidate, classification) = &group[0];
        if let Some(sub_kind) = &classification.sub_kind {
            update_content_variant(
                pool,
                candidate.item_id,
                ContentKind::AlternateVersion,
                Some(sub_kind),
                None,
            )
            .await?;
        } else {
            update_content_variant(pool, candidate.item_id, ContentKind::Primary, None, None)
                .await?;
        }
        return Ok(());
    }

    let canonical_id = group
        .iter()
        .find(|(_, classification)| {
            classification.sub_kind.is_none() && classification.canonical_hint
        })
        .or_else(|| {
            group
                .iter()
                .find(|(_, classification)| classification.sub_kind.is_none())
        })
        .unwrap_or(&group[0])
        .0
        .item_id;

    for (candidate, classification) in group {
        if candidate.item_id == canonical_id {
            update_content_variant(pool, candidate.item_id, ContentKind::Primary, None, None)
                .await?;
        } else if let Some(sub_kind) = &classification.sub_kind {
            update_content_variant(
                pool,
                candidate.item_id,
                ContentKind::AlternateVersion,
                Some(sub_kind),
                Some(canonical_id),
            )
            .await?;
        } else {
            update_content_variant(pool, candidate.item_id, ContentKind::Primary, None, None)
                .await?;
        }
    }

    Ok(())
}

async fn update_content_variant(
    pool: &DbPool,
    item_id: i64,
    content_kind: ContentKind,
    sub_kind: Option<&str>,
    parent_media_item_id: Option<i64>,
) -> Result<(), sqlx::Error> {
    let sub_kind = sub_kind.and_then(normalize_sub_kind_token);
    sqlx::query(
        r#"UPDATE "MediaItem"
           SET "ContentKind" = ?, "SubKind" = ?, "ParentMediaItemId" = ?
           WHERE "Id" = ?"#,
    )
    .bind(content_kind.db_value())
    .bind(sub_kind)
    .bind(parent_media_item_id)
    .bind(item_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Set `MediaItem.State` for the given primary keys, building an `IN (...)`
/// list of bound placeholders. No-op when `ids` is empty.
///
/// # Errors
///
/// Returns an error when the update fails.
pub(crate) async fn flag_media_items(
    pool: &DbPool,
    ids: &[i64],
    state: i64,
) -> Result<(), sqlx::Error> {
    if ids.is_empty() {
        return Ok(());
    }
    let placeholders = vec!["?"; ids.len()].join(", ");
    let sql = format!(r#"UPDATE "MediaItem" SET "State" = {state} WHERE "Id" IN ({placeholders})"#);
    let mut query = sqlx::query(assert_sql_safe(&sql));
    for id in ids {
        query = query.bind(id);
    }
    query.execute(pool).await?;
    Ok(())
}

// The administration contract also supports one legacy global credential.
// Never apply that fallback to one of several configured servers. In particular,
// a global Jellyfin Authorization value must not override a source-specific key.
pub(crate) async fn scoped_credential(
    pool: &DbPool,
    kind: ersatzrs_core::repositories::RemoteMediaSourceKind,
    keys: &[String],
    legacy: String,
) -> ersatzrs_core::repositories::Result<String> {
    for key in keys {
        if let Some(value) = sqlx::query_scalar::<_, String>(
            r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ? AND "Value" <> ''"#,
        )
        .bind(key)
        .fetch_optional(pool)
        .await
        .repository()?
        {
            return Ok(value);
        }
    }
    let query = match kind {
        ersatzrs_core::repositories::RemoteMediaSourceKind::Plex => {
            r#"SELECT COUNT(*) FROM "PlexMediaSource""#
        }
        ersatzrs_core::repositories::RemoteMediaSourceKind::Jellyfin => {
            r#"SELECT COUNT(*) FROM "JellyfinMediaSource""#
        }
        ersatzrs_core::repositories::RemoteMediaSourceKind::Emby => {
            r#"SELECT COUNT(*) FROM "EmbyMediaSource""#
        }
    };
    let count: i64 = sqlx::query_scalar(query)
        .fetch_one(pool)
        .await
        .repository()?;
    if count == 1 {
        Ok(legacy)
    } else {
        Err(ersatzrs_core::repositories::RepositoryError::Decode {
            kind: "media provider",
            value: "source-specific credential is required".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        provider_item_state_allows_etag_skip, STATE_FILE_NOT_FOUND, STATE_NORMAL,
        STATE_REMOTE_ONLY, STATE_UNAVAILABLE,
    };

    #[tokio::test]
    async fn episode_parent_repair_is_library_scoped_and_idempotent() {
        use super::{reconcile_episode_season, EpisodeProvider};
        use sqlx::Executor as _;
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        pool.execute("PRAGMA foreign_keys = ON").await.unwrap();
        sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
        pool.execute(r#"
            INSERT INTO "MediaSource" ("Id") VALUES (1);
            INSERT INTO "Library" ("Id", "MediaKind", "MediaSourceId") VALUES (1, 2, 1), (2, 2, 1);
            INSERT INTO "LibraryPath" ("Id", "LibraryId") VALUES (1, 1), (2, 2);
            INSERT INTO "MediaItem" ("Id", "LibraryPathId") VALUES
                (10, 1), (11, 1), (12, 1), (13, 1), (20, 2), (21, 2), (23, 2);
            INSERT INTO "Show" ("Id") VALUES (10), (20);
            INSERT INTO "Season" ("Id", "ShowId", "SeasonNumber") VALUES (11, 10, 1), (12, 10, 2), (21, 20, 1);
            INSERT INTO "Episode" ("Id", "SeasonId") VALUES (13, 11), (23, 21);
            INSERT INTO "PlexEpisode" ("Id", "Key") VALUES (13, 'same-id'), (23, 'same-id');
            INSERT INTO "JellyfinEpisode" ("Id", "ItemId") VALUES (13, 'same-id'), (23, 'same-id');
            INSERT INTO "EmbyEpisode" ("Id", "ItemId") VALUES (13, 'same-id'), (23, 'same-id');
        "#).await.unwrap();
        for provider in [
            EpisodeProvider::Plex,
            EpisodeProvider::Jellyfin,
            EpisodeProvider::Emby,
        ] {
            for invalid in [0, 21, 999] {
                assert_eq!(
                    reconcile_episode_season(&pool, provider, 1, "same-id", invalid)
                        .await
                        .unwrap(),
                    0
                );
            }
            assert_eq!(
                reconcile_episode_season(&pool, provider, 1, "same-id", 12)
                    .await
                    .unwrap(),
                1
            );
            assert_eq!(
                reconcile_episode_season(&pool, provider, 1, "same-id", 12)
                    .await
                    .unwrap(),
                0
            );
            let parents: Vec<(i64, i64)> =
                sqlx::query_as(r#"SELECT "Id", "SeasonId" FROM "Episode" ORDER BY "Id""#)
                    .fetch_all(&pool)
                    .await
                    .unwrap();
            assert_eq!(parents, vec![(13, 12), (23, 21)]);
            pool.execute(r#"UPDATE "Episode" SET "SeasonId" = 11 WHERE "Id" = 13"#)
                .await
                .unwrap();
        }
    }

    #[test]
    fn etag_skip_accepts_normal_and_remote_only_provider_items() {
        assert!(provider_item_state_allows_etag_skip(STATE_NORMAL));
        assert!(provider_item_state_allows_etag_skip(STATE_REMOTE_ONLY));
        assert!(!provider_item_state_allows_etag_skip(STATE_FILE_NOT_FOUND));
        assert!(!provider_item_state_allows_etag_skip(STATE_UNAVAILABLE));
    }
}
