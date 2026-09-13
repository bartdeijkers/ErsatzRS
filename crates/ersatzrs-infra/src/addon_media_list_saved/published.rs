//! Capture the validated last publication, never the mutable current draft.

use std::collections::BTreeMap;

use ersatzrs_addon_contract::MediaListSubtitleTimeline;
use ersatzrs_core::repositories::{
    AddonMediaListRevisionFile, AddonMediaListRevisionSnapshot, RepositoryError, Result,
};
use ersatzrs_domain::EntityId;
use serde_json::{Map, Value};
use sqlx::{Row as _, Transaction};

use super::capture::{decode_json, item_content, option_schema, CapturedReplacement};
use super::model::{
    SavedFile, SavedListPresentation, SavedListPublication, SavedOptions, SavedPlayback,
    SavedPlaybackTimeline, SavedPublishedItem, SavedReplacement, SavedSubtitle,
    SAVED_ASSETS_DIRECTORY,
};
use crate::db::{Db, DbRow};
use crate::repositories::{load_valid_revision_snapshot_in_tx, SqlxResultExt as _};

/// Portable publication plus local byte sources awaiting verification/copying.
/// Deliberately not serializable: blob/artifact/replacement identities stay local.
pub struct CapturedPublication {
    pub publication: SavedListPublication,
    pub revision_files: Vec<AddonMediaListRevisionFile>,
    pub artifacts: Vec<CapturedArtifact>,
    pub replacements: Vec<CapturedReplacement>,
}

/// Local artifact source; only `file` is part of the saved content.
pub struct CapturedArtifact {
    pub artifact_id: EntityId,
    pub storage_relative_path: String,
    pub file: SavedFile,
}

/// Read the exact completed revision in the caller's consistent transaction.
///
/// No file IO, current-draft substitution, publication or Save acknowledgement
/// occurs here. The caller must fence publication/reset, materialize assets and
/// validate the complete document before making it authoritative.
///
/// # Errors
/// Refuses missing/invalid retained revisions or inconsistent selected assets.
pub async fn in_tx(
    tx: &mut Transaction<'_, Db>,
    list_id: EntityId,
) -> Result<Option<CapturedPublication>> {
    let revision: i32 =
        sqlx::query_scalar(r#"SELECT "PublishedRevision" FROM "AddonMediaList" WHERE "Id"=?"#)
            .bind(list_id)
            .fetch_optional(&mut **tx)
            .await
            .repository()?
            .ok_or(RepositoryError::NotFound {
                kind: "add-on media list",
                id: list_id,
            })?;
    if revision == 0 {
        return Ok(None);
    }
    let snapshot = load_valid_revision_snapshot_in_tx(tx, list_id, revision)
        .await?
        .ok_or_else(inconsistent)?;
    let artwork = PublishedArtwork::load(tx, &snapshot).await?;
    let mut artifacts = ArtifactCapture::load(tx, list_id, revision).await?;
    let (items, replacements) = published_items(&snapshot, &artwork, &mut artifacts)?;
    let selected_artifacts = artifacts.finish();
    let mut files = snapshot.files.iter().map(saved_file).collect::<Vec<_>>();
    files.extend(
        selected_artifacts
            .iter()
            .map(|artifact| artifact.file.clone()),
    );
    files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    files.dedup();
    let publication = SavedListPublication {
        revision: snapshot.revision,
        source_draft_revision: snapshot.source_draft_revision,
        completed_at: snapshot.completed_at,
        presentation: SavedListPresentation {
            name: snapshot.name,
            description: snapshot.description,
            generate_playlist: snapshot.generate_playlist,
            content_kind_override: snapshot.content_kind_override,
            presentation_kind: snapshot.presentation_kind,
            requested_presentation_kind: snapshot.requested_presentation_kind,
        },
        metadata: artwork.metadata(&snapshot.metadata_json, None)?,
        options: SavedOptions {
            schema: option_schema(tx, snapshot.options.schema.as_ref()).await?,
            list_values: snapshot.options.list_values,
            item_values: snapshot
                .options
                .item_values
                .into_iter()
                .map(|item| (item.provider_id, item.values))
                .collect(),
        },
        items,
        files,
    };
    if serde_json::to_string(&publication)
        .map_err(|_| inconsistent())?
        .contains(crate::addon_artwork::UPLOAD_URL_PREFIX)
    {
        return Err(inconsistent());
    }
    Ok(Some(CapturedPublication {
        publication,
        revision_files: snapshot.files,
        artifacts: selected_artifacts,
        replacements,
    }))
}

fn published_items(
    snapshot: &AddonMediaListRevisionSnapshot,
    artwork: &PublishedArtwork,
    artifacts: &mut ArtifactCapture,
) -> Result<(Vec<SavedPublishedItem>, Vec<CapturedReplacement>)> {
    let mut items = Vec::new();
    let mut replacements = Vec::new();
    for item in snapshot.items.iter().filter(|item| item.published) {
        let playback = if let Some(media_item_id) = item.replacement_media_item_id {
            replacements.push(CapturedReplacement {
                provider_id: item.provider_id.clone(),
                media_item_id,
                media_version_id: item.replacement_media_version_id,
            });
            SavedPlayback::Replacement {
                replacement: SavedReplacement { file: None },
            }
        } else if let Some(id) = item.prepared_artifact_id {
            let (file, duration_milliseconds) = artifacts.media(id)?;
            SavedPlayback::DownloadAhead {
                file,
                duration_milliseconds,
            }
        } else {
            SavedPlayback::Stream {
                timeline: item
                    .playback_timeline
                    .as_ref()
                    .map(SavedPlaybackTimeline::from),
            }
        };
        let subtitles = item
            .subtitle_artifact_ids
            .iter()
            .map(|id| artifacts.subtitle(*id, item.prepared_artifact_id.is_some()))
            .collect::<Result<Vec<_>>>()?;
        items.push(SavedPublishedItem {
            content: item_content(item)?,
            metadata: artwork.metadata(&item.metadata_json, Some(&item.provider_id))?,
            playback,
            subtitles,
        });
    }
    Ok((items, replacements))
}

fn inconsistent() -> RepositoryError {
    RepositoryError::Conflict("saved publication content is inconsistent or unavailable".into())
}

fn saved_file(file: &AddonMediaListRevisionFile) -> SavedFile {
    SavedFile {
        relative_path: file.published_relative_path.clone(),
        sha256: file.sha256.clone(),
        byte_length: file.byte_length,
    }
}

struct ArtifactCapture {
    rows: BTreeMap<EntityId, DbRow>,
    used: BTreeMap<EntityId, CapturedArtifact>,
}

impl ArtifactCapture {
    async fn load(tx: &mut Transaction<'_, Db>, list_id: EntityId, revision: i32) -> Result<Self> {
        // Only pinned metadata, not the entire cache and never the media bytes.
        let rows = sqlx::query(r#"SELECT "artifact"."Id", "artifact"."Kind", "artifact"."State",
            "artifact"."RelativePath", "artifact"."Sha256", "artifact"."ByteLength",
            "artifact"."DurationMilliseconds", "artifact"."SubtitleLanguage",
            "artifact"."SubtitleTimeline", "artifact"."CompletedAt"
            FROM "AddonMediaListRevisionArtifact" AS "pin"
            JOIN "AddonMediaListRevision" AS "revision" ON "revision"."Id"="pin"."AddonMediaListRevisionId"
            JOIN "AddonMediaListArtifact" AS "artifact" ON "artifact"."Id"="pin"."AddonMediaListArtifactId"
                AND "artifact"."AddonMediaListId"="revision"."AddonMediaListId"
            WHERE "revision"."AddonMediaListId"=? AND "revision"."Revision"=?"#)
            .bind(list_id).bind(revision).fetch_all(&mut **tx).await.repository()?;
        let mut indexed = BTreeMap::new();
        for row in rows {
            indexed.insert(row.try_get("Id").repository()?, row);
        }
        Ok(Self {
            rows: indexed,
            used: BTreeMap::new(),
        })
    }

    fn file(&mut self, id: EntityId, kind: i32, extension: &str) -> Result<SavedFile> {
        let row = self.rows.get(&id).ok_or_else(inconsistent)?;
        if row.try_get::<i32, _>("Kind").repository()? != kind
            || row.try_get::<i32, _>("State").repository()? != 2
        {
            return Err(inconsistent());
        }
        let hash: String = row
            .try_get::<Option<String>, _>("Sha256")
            .repository()?
            .ok_or_else(inconsistent)?;
        let length = positive_u64(row, "ByteLength")?;
        let relative_path: String = row
            .try_get::<Option<String>, _>("RelativePath")
            .repository()?
            .ok_or_else(inconsistent)?;
        if relative_path.is_empty()
            || hash.len() != 64
            || !hash
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(inconsistent());
        }
        let file = SavedFile {
            relative_path: format!("{SAVED_ASSETS_DIRECTORY}/published/{hash}.{extension}"),
            sha256: hash,
            byte_length: length,
        };
        self.used.insert(
            id,
            CapturedArtifact {
                artifact_id: id,
                storage_relative_path: relative_path,
                file: file.clone(),
            },
        );
        Ok(file)
    }

    fn media(&mut self, id: EntityId) -> Result<(SavedFile, u64)> {
        let file = self.file(id, 1, "media")?;
        Ok((
            file,
            positive_u64(
                self.rows.get(&id).ok_or_else(inconsistent)?,
                "DurationMilliseconds",
            )?,
        ))
    }

    fn subtitle(&mut self, id: EntityId, prepared: bool) -> Result<SavedSubtitle> {
        let file = self.file(id, 0, "srt")?;
        let row = self.rows.get(&id).ok_or_else(inconsistent)?;
        let timeline = match row
            .try_get::<Option<i32>, _>("SubtitleTimeline")
            .repository()?
        {
            Some(0) if !prepared => MediaListSubtitleTimeline::Source,
            Some(1) if prepared => MediaListSubtitleTimeline::Prepared,
            _ => return Err(inconsistent()),
        };
        let language: String = row
            .try_get::<Option<String>, _>("SubtitleLanguage")
            .repository()?
            .ok_or_else(inconsistent)?;
        if language.trim().is_empty()
            || row
                .try_get::<Option<String>, _>("CompletedAt")
                .repository()?
                .is_none()
        {
            return Err(inconsistent());
        }
        Ok(SavedSubtitle {
            file,
            language,
            timeline,
        })
    }

    fn finish(self) -> Vec<CapturedArtifact> {
        self.used.into_values().collect()
    }
}

fn positive_u64(row: &DbRow, column: &'static str) -> Result<u64> {
    let value = row
        .try_get::<Option<i64>, _>(column)
        .repository()?
        .ok_or_else(inconsistent)?;
    u64::try_from(value)
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(inconsistent)
}

struct PublishedArtwork {
    entries: Vec<PublishedArtworkEntry>,
}

struct PublishedArtworkEntry {
    provider_id: Option<String>,
    role: i32,
    source: String,
    file: Option<SavedFile>,
}

impl PublishedArtwork {
    async fn load(
        tx: &mut Transaction<'_, Db>,
        snapshot: &AddonMediaListRevisionSnapshot,
    ) -> Result<Self> {
        let rows = sqlx::query(
            r#"SELECT "art"."ScopeItemId", "item"."ProviderId",
            "art"."Role", "art"."Source", "art"."FileName", "art"."Sha256"
            FROM "AddonMediaListArtworkMaterialization" AS "art"
            LEFT JOIN "AddonMediaListItem" AS "item" ON "item"."Id"="art"."ScopeItemId"
                AND "item"."AddonMediaListId"="art"."AddonMediaListId"
            WHERE "art"."AddonMediaListId"=? ORDER BY "art"."ScopeItemId", "art"."Role""#,
        )
        .bind(snapshot.list_id)
        .fetch_all(&mut **tx)
        .await
        .repository()?;
        let mut entries = Self::uploads(tx, snapshot).await?;
        for row in rows {
            let source: String = row.try_get("Source").repository()?;
            let provider_id: Option<String> = row.try_get("ProviderId").repository()?;
            if row.try_get::<EntityId, _>("ScopeItemId").repository()? != 0 && provider_id.is_none()
            {
                continue;
            }
            let name: String = row.try_get("FileName").repository()?;
            let hash: String = row.try_get("Sha256").repository()?;
            // Retention currently leaves file.provider_id unset. Match the
            // recorded basename AND bytes, not a guessed streams/root layout.
            let mut matches = snapshot.files.iter().filter(|file| {
                file.published_relative_path.rsplit('/').next() == Some(name.as_str())
                    && file.sha256 == hash
            });
            let mut file = matches.next().map(saved_file);
            if matches.next().is_some() {
                file = None;
            }
            let role = row.try_get("Role").repository()?;
            if source.starts_with(crate::addon_artwork::UPLOAD_URL_PREFIX) {
                // An operator-replaced image can differ from the original upload.
                // Only fill an owned handle's missing mapping when this exact
                // materialization source/scope/role still matches retained bytes.
                if let Some(owned) = entries.iter_mut().find(|entry| {
                    entry.source == source && entry.provider_id == provider_id && entry.role == role
                }) {
                    if owned.file.is_none() {
                        owned.file = file;
                    }
                }
                continue;
            }
            entries.push(PublishedArtworkEntry {
                provider_id,
                role,
                source,
                file,
            });
        }
        Ok(Self { entries })
    }

    async fn uploads(
        tx: &mut Transaction<'_, Db>,
        snapshot: &AddonMediaListRevisionSnapshot,
    ) -> Result<Vec<PublishedArtworkEntry>> {
        // Retained files are path-sorted. Multiple paths with identical bytes
        // are interchangeable image sources; pick one deterministically rather
        // than refusing a list that reuses its poster on several items.
        let mut retained = BTreeMap::new();
        for file in &snapshot.files {
            retained
                .entry((file.sha256.as_str(), file.byte_length))
                .or_insert(file);
        }
        let rows = sqlx::query(
            r#"SELECT "upload"."Id", "upload"."ScopeItemId",
            "item"."ProviderId", "upload"."Role", "upload"."Sha256",
            length("upload"."Bytes") AS "ByteLength"
            FROM "AddonMediaListArtworkUpload" AS "upload"
            LEFT JOIN "AddonMediaListItem" AS "item" ON "item"."Id"="upload"."ScopeItemId"
                AND "item"."AddonMediaListId"="upload"."AddonMediaListId"
            WHERE "upload"."AddonMediaListId"=? ORDER BY "upload"."Id""#,
        )
        .bind(snapshot.list_id)
        .fetch_all(&mut **tx)
        .await
        .repository()?;
        let mut entries = Vec::new();
        for row in rows {
            let provider_id: Option<String> = row.try_get("ProviderId").repository()?;
            if row.try_get::<EntityId, _>("ScopeItemId").repository()? != 0 && provider_id.is_none()
            {
                continue;
            }
            let id: EntityId = row.try_get("Id").repository()?;
            let hash: String = row.try_get("Sha256").repository()?;
            let length = positive_u64(&row, "ByteLength")?;
            entries.push(PublishedArtworkEntry {
                provider_id,
                role: row.try_get("Role").repository()?,
                source: format!("{}{id}", crate::addon_artwork::UPLOAD_URL_PREFIX),
                file: retained
                    .get(&(hash.as_str(), length))
                    .map(|file| saved_file(file)),
            });
        }
        Ok(entries)
    }

    fn metadata(&self, json: &str, provider_id: Option<&str>) -> Result<Map<String, Value>> {
        let mut metadata: Map<String, Value> = decode_json(json)?;
        for field in ["artwork", "artwork_urls", "people"] {
            if let Some(value) = metadata.get_mut(field) {
                if field == "artwork" {
                    if let Some(entries) = value.as_array_mut() {
                        for entry in entries {
                            let role = serde_json::from_value(
                                entry.get("role").cloned().ok_or_else(inconsistent)?,
                            )
                            .map_err(|_| inconsistent())?;
                            self.rewrite(
                                entry,
                                provider_id,
                                Some(crate::repositories::artwork_role_value(role)),
                            )?;
                        }
                        continue;
                    }
                }
                self.rewrite(value, provider_id, None)?;
            }
        }
        Ok(metadata)
    }

    fn rewrite(
        &self,
        value: &mut Value,
        provider_id: Option<&str>,
        role: Option<i32>,
    ) -> Result<()> {
        match value {
            Value::String(url) => {
                let entry = self.entries.iter().find(|entry| {
                    entry.provider_id.as_deref() == provider_id
                        && entry.source == *url
                        && role.is_none_or(|role| role == entry.role)
                });
                if let Some(file) = entry.and_then(|entry| entry.file.as_ref()) {
                    *url = format!("./{}", file.relative_path);
                } else if url.starts_with(crate::addon_artwork::UPLOAD_URL_PREFIX) {
                    return Err(inconsistent());
                }
            }
            Value::Array(values) => {
                for value in values {
                    self.rewrite(value, provider_id, role)?;
                }
            }
            Value::Object(values) => {
                for value in values.values_mut() {
                    self.rewrite(value, provider_id, role)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
