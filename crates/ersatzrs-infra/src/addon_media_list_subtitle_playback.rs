//! Verify published caption provenance before accepting the retained programme clock.
use std::path::Path;

use ersatzrs_core::{
    addon_media_list_playback::AddonMediaListPlaybackTimelinePlan,
    repositories::{AddonMediaListLifecycleRepository as _, RepositoryError, Result},
};
use sha2::{Digest as _, Sha256};
use sqlx::Row as _;
use tokio::io::AsyncReadExt as _;

use crate::{
    addon_media_list_artifacts::confined_file,
    addon_media_list_subtitles::read_verified_subtitle_artifact,
    db::DbPool,
    metadata::subtitle_kind,
    playback::{load_subtitles_for_media_item, PlaybackSubtitle},
    repositories::{SqliteAddonMediaListRepository, SqlxResultExt as _},
};

/// Accept only a digest-covered published sidecar derived from its pinned source
/// artifact using the exact frozen interval plan. No provider work is performed.
///
/// # Errors
/// Returns redacted storage/validation errors or database errors; callers must
/// refuse retained playback on both `false` and errors, without dropping cleanup.
pub async fn verify_retained_subtitle(
    pool: &DbPool,
    media_item_id: i64,
    subtitle: &PlaybackSubtitle,
    plan: &AddonMediaListPlaybackTimelinePlan,
) -> Result<bool> {
    verify_retained_subtitle_with_root(
        pool,
        media_item_id,
        subtitle,
        plan,
        &crate::file_system_layout::addon_processed_media_folder(),
    )
    .await
}

async fn verify_retained_subtitle_with_root(
    pool: &DbPool,
    media_item_id: i64,
    subtitle: &PlaybackSubtitle,
    plan: &AddonMediaListPlaybackTimelinePlan,
    artifact_root: &Path,
) -> Result<bool> {
    Ok(
        read_verified_retained_subtitle(pool, media_item_id, subtitle, plan, artifact_root)
            .await?
            .is_some(),
    )
}

/// Verify and rebase exactly the checked SRT bytes to the producer's millisecond
/// programme join. Valid exhaustion returns no burn input, never an old-clock file.
///
/// # Errors
/// Refuses invalid publication provenance, out-of-range joins and cache failures.
pub async fn prepare_retained_subtitle_input(
    pool: &DbPool,
    media_item_id: i64,
    subtitle: &PlaybackSubtitle,
    plan: &AddonMediaListPlaybackTimelinePlan,
    programme_seek: std::time::Duration,
) -> Result<Option<ersatzrs_ffmpeg::InputFile>> {
    let seek = u64::try_from(programme_seek.as_millis()).map_err(|_| invalid())?;
    if seek > plan.duration_milliseconds() {
        return Err(invalid());
    }
    let content = read_verified_retained_subtitle(
        pool,
        media_item_id,
        subtitle,
        plan,
        &crate::file_system_layout::addon_processed_media_folder(),
    )
    .await?
    .ok_or_else(invalid)?;
    let path = crate::subtitles::rebase_verified_srt(&content, seek).await?;
    Ok(path.map(|path| {
        ersatzrs_ffmpeg::InputFile::text_subtitle(path, ersatzrs_ffmpeg::SubtitleMethod::Burn)
            .with_subtitle_codec("srt")
    }))
}

#[allow(clippy::too_many_lines)] // Verify the complete revision, artifact and sidecar provenance together.
async fn read_verified_retained_subtitle(
    pool: &DbPool,
    media_item_id: i64,
    subtitle: &PlaybackSubtitle,
    plan: &AddonMediaListPlaybackTimelinePlan,
    artifact_root: &Path,
) -> Result<Option<String>> {
    if subtitle.subtitle_kind != subtitle_kind::SIDECAR
        || subtitle.is_extracted
        || !matches!(subtitle.codec.as_deref(), Some("srt" | "subrip"))
    {
        return Ok(None);
    }
    let Some(selected_path) = subtitle.path.as_deref().filter(|path| !path.is_empty()) else {
        return Ok(None);
    };
    if !load_subtitles_for_media_item(pool, media_item_id)
        .await?
        .contains(subtitle)
    {
        return Ok(None);
    }
    let owner = sqlx::query(r#"SELECT "list"."Id" AS "ListId", "list"."PublishedRevision", "list"."AddonId",
        "stream"."AddonId" AS "StreamAddonId", "stream"."ProviderId", "stream"."Url", "binding"."ManifestPath"
        FROM "RemoteStream" AS "stream"
        JOIN "MediaItem" AS "media" ON "media"."Id"="stream"."Id"
        JOIN "LibraryPath" AS "path" ON "path"."Id"="media"."LibraryPathId"
        JOIN "AddonMediaListLibrary" AS "binding" ON "binding"."LibraryId"="path"."LibraryId"
        JOIN "AddonMediaList" AS "list" ON "list"."Id"="binding"."AddonMediaListId"
        WHERE "stream"."Id"=?"#).bind(media_item_id).fetch_optional(pool).await.repository()?;
    let Some(owner) = owner else {
        return Ok(None);
    };
    let list_id: i64 = owner.try_get("ListId").repository()?;
    let revision: i32 = owner.try_get("PublishedRevision").repository()?;
    let addon: String = owner.try_get("AddonId").repository()?;
    if revision <= 0
        || owner
            .try_get::<Option<String>, _>("StreamAddonId")
            .repository()?
            .as_deref()
            != Some(&addon)
    {
        return Ok(None);
    }
    let provider: String = owner
        .try_get::<Option<String>, _>("ProviderId")
        .repository()?
        .ok_or_else(invalid)?;
    let source: Option<String> = owner.try_get("Url").repository()?;
    let repository = SqliteAddonMediaListRepository(pool.clone());
    let Some(snapshot) = repository.revision_snapshot(list_id, revision).await? else {
        return Ok(None);
    };
    let Some(item) = snapshot.items.iter().find(|item| {
        item.published
            && item.provider_id == provider
            && item.published_media_item_id == Some(media_item_id)
    }) else {
        return Ok(None);
    };
    if item.playback_timeline.as_ref() != Some(plan)
        || item.source_url != source
        || item.prepared_artifact_id.is_some()
        || item.replacement_media_item_id.is_some()
        || item.subtitle_artifact_ids.is_empty()
        || item.subtitle_artifact_ids.len() > 2
    {
        return Ok(None);
    }
    let manifest: String = owner.try_get("ManifestPath").repository()?;
    let manifest = Path::new(&manifest);
    let configured_root = if manifest.is_dir() {
        manifest
    } else {
        manifest.parent().ok_or_else(invalid)?
    };
    let root = tokio::fs::canonicalize(configured_root)
        .await
        .map_err(|_| invalid())?;
    let path = Path::new(selected_path);
    let relative = path
        .strip_prefix(configured_root)
        .or_else(|_| path.strip_prefix(&root))
        .map_err(|_| invalid())?;
    let relative = relative
        .to_str()
        .ok_or_else(invalid)?
        .replace(std::path::MAIN_SEPARATOR, "/");
    let file = snapshot
        .files
        .iter()
        .find(|file| {
            file.provider_id.as_deref().is_none_or(|id| id == provider)
                && file.published_relative_path == relative
        })
        .ok_or_else(invalid)?;
    let limit = 8 * 1024 * 1024_u64;
    if file.byte_length == 0 || file.byte_length > limit {
        return Ok(None);
    }
    let selected_path = confined_file(&root, &relative).await?;
    let mut bytes = Vec::new();
    tokio::fs::File::open(selected_path)
        .await
        .map_err(|_| invalid())?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| invalid())?;
    let hash = Sha256::digest(&bytes)
        .iter()
        .fold(String::with_capacity(64), |mut text, byte| {
            use std::fmt::Write as _;
            write!(text, "{byte:02x}").expect("string write");
            text
        });
    if u64::try_from(bytes.len()).map_err(|_| invalid())? != file.byte_length || hash != file.sha256
    {
        return Ok(None);
    }
    let Some(language) = subtitle.language.as_deref() else {
        return Ok(None);
    };
    let selected_primary =
        ersatzrs_addon_contract::subtitle_logical_language(language).map_err(|_| invalid())?;
    let selected_language =
        ersatzrs_core::language_codes::canonical_language_code(&selected_primary)
            .ok_or_else(invalid)?;
    let artifact_root = tokio::fs::canonicalize(artifact_root)
        .await
        .map_err(|_| invalid())?;
    let timeline = plan.timeline().map_err(|_| invalid())?;
    let mut verified = false;
    for id in &item.subtitle_artifact_ids {
        if !snapshot.artifact_ids.contains(id) {
            return Ok(None);
        }
        let artifact = read_verified_subtitle_artifact(pool, list_id, *id, &artifact_root).await?;
        if artifact.prepared {
            return Ok(None);
        }
        let primary = ersatzrs_addon_contract::subtitle_logical_language(&artifact.language)
            .map_err(|_| invalid())?;
        if ersatzrs_core::language_codes::canonical_language_code(&primary).as_deref()
            != Some(selected_language.as_str())
        {
            continue;
        }
        let mapped = crate::subtitles::map_srt_through_timeline(
            &artifact.content,
            &timeline,
            usize::try_from(limit).map_err(|_| invalid())?,
        )
        .map_err(|_| invalid())?;
        if mapped.as_bytes() == bytes {
            verified = true;
        }
    }
    if !verified {
        return Ok(None);
    }
    let current: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(SELECT 1 FROM "AddonMediaList" AS "list"
        JOIN "AddonMediaListRevision" AS "revision" ON "revision"."AddonMediaListId"="list"."Id"
        AND "revision"."Revision"="list"."PublishedRevision"
        WHERE "list"."Id"=? AND "list"."PublishedRevision"=? AND "revision"."SnapshotDigest"=?)"#,
    )
    .bind(list_id)
    .bind(revision)
    .bind(&snapshot.snapshot_digest)
    .fetch_one(pool)
    .await
    .repository()?;
    if current {
        Ok(Some(String::from_utf8(bytes).map_err(|_| invalid())?))
    } else {
        Ok(None)
    }
}

fn invalid() -> RepositoryError {
    RepositoryError::Conflict("published edited subtitle provenance is unavailable".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(bytes: &[u8]) -> String {
        use std::fmt::Write as _;
        Sha256::digest(bytes)
            .iter()
            .fold(String::new(), |mut text, byte| {
                write!(text, "{byte:02x}").unwrap();
                text
            })
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)] // One publication fixture proves each provenance fence.
    async fn captured_unassigned_revision_sidecar_requires_exact_item_mapping() {
        use ersatzrs_core::repositories::{
            AddonMediaListRevisionArtifacts, AddonMediaListRevisionFile,
        };
        let temporary = tempfile::tempdir().unwrap();
        let pool = crate::db::init(&format!(
            "sqlite:{}",
            temporary.path().join("fixture.db").display()
        ))
        .await
        .unwrap();
        let managed = temporary.path().join("managed");
        let artifacts = temporary.path().join("artifacts");
        tokio::fs::create_dir_all(&managed).await.unwrap();
        tokio::fs::create_dir_all(artifacts.join("objects"))
            .await
            .unwrap();
        sqlx::raw_sql(r#"
            INSERT INTO "AddonInstallation" ("AddonId","Version","Enabled","PackageSha256","ContentSha256","ManifestJson","InstallPath","InstalledAt","UpdatedAt")
                VALUES ('org.example.fixture','1.0.0',1,'fixture','fixture','{}','addons/fixture','2026-09-05T00:00:00Z','2026-09-05T00:00:00Z');
            INSERT INTO "MediaSource" ("Id") VALUES (1);
            INSERT INTO "Library" ("Id","MediaKind","MediaSourceId") VALUES (1,4,1);
            INSERT INTO "LibraryPath" ("Id","LibraryId","Path") VALUES (1,1,'streams');
            INSERT INTO "MediaItem" ("Id","LibraryPathId") VALUES (100,1);
            INSERT INTO "RemoteStream" ("Id","AddonId","ProviderId","Url") VALUES (100,'org.example.fixture','item','https://example.test/item');
            INSERT INTO "RemoteStreamMetadata" ("Id","RemoteStreamId","MetadataKind","DateAdded","DateUpdated")
                VALUES (100,100,0,'2026-09-05T00:00:00Z','2026-09-05T00:00:00Z');
            INSERT INTO "AddonMediaList" ("Id","AddonId","ProviderId","SourceUrl","Name","PublishedRevision")
                VALUES (1,'org.example.fixture','list','https://example.test/list','Fixture',1);
            INSERT INTO "AddonMediaListItem" ("Id","AddonMediaListId","ProviderId","DisplayTitle","Kind","Rank","Title","SourceUrl","ProviderLiveness","EffectiveLiveness","Published","MediaItemId")
                VALUES (10,1,'item','Fixture',4,0,'Fixture','https://example.test/item',1,1,1,100);
        "#).execute(&pool).await.unwrap();
        sqlx::query(r#"INSERT INTO "AddonMediaListLibrary" ("AddonMediaListId","LibraryId","FolderName","ManifestPath") VALUES (1,1,'fixture',?)"#)
            .bind(managed.join("list.yml").to_str().unwrap()).execute(&pool).await.unwrap();
        let source = "1\n00:00:01,000 --> 00:00:05,000\nHéllo\n";
        let plan = AddonMediaListPlaybackTimelinePlan::new(
            10_000,
            0..10_000,
            &[2_000..3_000, 6_000..7_000],
            "a".repeat(64),
        )
        .unwrap();
        let mapped =
            crate::subtitles::map_srt_through_timeline(source, &plan.timeline().unwrap(), 4096)
                .unwrap();
        tokio::fs::write(artifacts.join("objects/source.srt"), source)
            .await
            .unwrap();
        sqlx::query(r#"INSERT INTO "AddonMediaListArtifact" ("Id","AddonMediaListId","AddonMediaListItemId","CacheKey","Kind","State","RelativePath","ByteLength","Sha256","SubtitleLanguage","SubtitleTimeline","CreatedAt","UpdatedAt","CompletedAt")
            VALUES (1,1,10,'captions',0,2,'objects/source.srt',?,?,'en-US',0,'2026-09-05T00:00:00Z','2026-09-05T00:00:00Z','2026-09-05T00:00:00Z')"#)
            .bind(i64::try_from(source.len()).unwrap()).bind(digest(source.as_bytes())).execute(&pool).await.unwrap();
        let repository = SqliteAddonMediaListRepository(pool.clone());
        let mut files = Vec::new();
        for (name, content) in [
            ("item.eng.srt", mapped.as_str()),
            (
                "unrelated.eng.srt",
                "1\n00:00:00,000 --> 00:00:01,000\nUnrelated\n",
            ),
        ] {
            tokio::fs::write(managed.join(name), content).await.unwrap();
            let hash = digest(content.as_bytes());
            let blob = repository
                .register_revision_file_blob(
                    1,
                    &format!("blobs/{hash}"),
                    &hash,
                    content.len() as u64,
                    chrono::Utc::now(),
                )
                .await
                .unwrap();
            files.push(AddonMediaListRevisionFile {
                blob_id: blob.id,
                published_relative_path: name.into(),
                provider_id: None,
                storage_relative_path: blob.storage_relative_path,
                sha256: hash,
                byte_length: content.len() as u64,
            });
        }
        // Exercise the real digest-covered revision capture, including the
        // current publisher's deliberately unassigned file provider IDs.
        repository
            .retain_published_revision(
                1,
                1,
                0,
                chrono::Utc::now(),
                &files,
                &AddonMediaListRevisionArtifacts {
                    ids: vec![1],
                    subtitles: std::collections::BTreeMap::from([("item".into(), vec![1])]),
                    streaming_plans: std::collections::BTreeMap::from([(
                        "item".into(),
                        plan.clone(),
                    )]),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        sqlx::query(r#"INSERT INTO "Subtitle" ("Id","RemoteStreamMetadataId","Codec","DateAdded","DateUpdated","Default","Forced","Language","Path","StreamIndex","SubtitleKind")
            VALUES (1,100,'srt','2026-09-05T00:00:00Z','2026-09-05T00:00:00Z',1,0,'eng',?,0,1)"#)
            .bind(managed.join("item.eng.srt").to_str().unwrap()).execute(&pool).await.unwrap();
        let selected = load_subtitles_for_media_item(&pool, 100)
            .await
            .unwrap()
            .remove(0);
        assert_eq!(
            read_verified_retained_subtitle(&pool, 100, &selected, &plan, &artifacts)
                .await
                .unwrap(),
            Some(mapped.clone())
        );
        assert!(
            verify_retained_subtitle_with_root(&pool, 100, &selected, &plan, &artifacts)
                .await
                .unwrap()
        );
        sqlx::query(r#"UPDATE "Subtitle" SET "Path"=? WHERE "Id"=1"#)
            .bind(managed.join("unrelated.eng.srt").to_str().unwrap())
            .execute(&pool)
            .await
            .unwrap();
        let unrelated = load_subtitles_for_media_item(&pool, 100)
            .await
            .unwrap()
            .remove(0);
        assert!(
            !verify_retained_subtitle_with_root(&pool, 100, &unrelated, &plan, &artifacts)
                .await
                .unwrap()
        );
        assert!(
            !verify_retained_subtitle_with_root(&pool, 100, &selected, &plan, &artifacts)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn retained_subtitle_verification_refuses_embedded_extracted_and_non_srt_inputs() {
        // Rejected descriptors must not even require a database lookup.
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect_lazy("sqlite::memory:")
            .unwrap();
        let plan = AddonMediaListPlaybackTimelinePlan::new(
            10_000,
            0..10_000,
            &[2_000..3_000, 5_000..6_000],
            "a".repeat(64),
        )
        .unwrap();
        let subtitle = PlaybackSubtitle {
            id: 1,
            subtitle_kind: subtitle_kind::EMBEDDED,
            title: None,
            stream_index: 0,
            codec: Some("srt".into()),
            default: true,
            forced: false,
            sdh: false,
            language: Some("en".into()),
            is_extracted: false,
            path: Some("captions.srt".into()),
        };
        assert!(!verify_retained_subtitle(&pool, 1, &subtitle, &plan)
            .await
            .unwrap());
        let mut extracted = subtitle.clone();
        extracted.subtitle_kind = subtitle_kind::SIDECAR;
        extracted.is_extracted = true;
        assert!(!verify_retained_subtitle(&pool, 1, &extracted, &plan)
            .await
            .unwrap());
        extracted.is_extracted = false;
        extracted.codec = Some("ass".into());
        assert!(!verify_retained_subtitle(&pool, 1, &extracted, &plan)
            .await
            .unwrap());
        extracted.codec = Some("subrip".into());
        extracted.path = None;
        assert!(!verify_retained_subtitle(&pool, 1, &extracted, &plan)
            .await
            .unwrap());
    }
}
