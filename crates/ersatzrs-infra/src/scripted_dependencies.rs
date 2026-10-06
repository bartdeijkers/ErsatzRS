//! Actual Scripted source selections and their library-scoped change projections.
use std::collections::{BTreeMap, BTreeSet};

use ersatzrs_core::{
    repositories::{RepositoryError, Result},
    scheduling::{
        SchedulingContentSource, SchedulingMediaItem, SchedulingPlaylistItem,
        ScriptedSourceDependency, ScriptedSourceResolution,
    },
};
use ersatzrs_domain::EntityId;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::Transaction;

use crate::{
    db::{Db, DbPool},
    repositories::{SqliteSchedulingContentRepository, SqlxResultExt as _},
};

mod publication;
mod refresh;
pub(crate) use publication::{PublicationFence, PUBLICATION_OPERATION};
pub use refresh::{record_successful_scan, resolve_pending};

#[derive(Serialize)]
pub(crate) enum ResolvedSource {
    Media(Vec<SchedulingMediaItem>),
    Playlist(Option<Vec<SchedulingPlaylistItem>>),
}

impl ResolvedSource {
    fn items(&self) -> Box<dyn Iterator<Item = &SchedulingMediaItem> + '_> {
        match self {
            Self::Media(items) => Box::new(items.iter()),
            Self::Playlist(children) => Box::new(
                children
                    .iter()
                    .flatten()
                    .flat_map(|child| &child.media_items),
            ),
        }
    }

    fn library_projection(
        &self,
        owners: &BTreeMap<EntityId, EntityId>,
        library_id: EntityId,
    ) -> Self {
        let selected = |item: &&SchedulingMediaItem| owners.get(&item.id) == Some(&library_id);
        match self {
            Self::Media(items) => Self::Media(items.iter().filter(selected).cloned().collect()),
            Self::Playlist(children) => Self::Playlist(children.as_ref().map(|children| {
                children
                    .iter()
                    .filter_map(|child| {
                        let media_items = child
                            .media_items
                            .iter()
                            .filter(selected)
                            .cloned()
                            .collect::<Vec<_>>();
                        (!media_items.is_empty()).then(|| SchedulingPlaylistItem {
                            media_items,
                            ..child.clone()
                        })
                    })
                    .collect()
            })),
        }
    }
}

fn invalid() -> RepositoryError {
    RepositoryError::Conflict(
        "Scripted source dependency state is invalid or changed during lookup".to_owned(),
    )
}

fn fingerprint(value: &impl Serialize) -> Result<String> {
    use std::fmt::Write as _;
    let bytes = serde_json::to_vec(value).map_err(|_| invalid())?;
    let mut hash = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(hash, "{byte:02x}").map_err(|_| invalid())?;
    }
    Ok(hash)
}

pub(crate) fn source_key(dependency: &ScriptedSourceDependency) -> Result<String> {
    fingerprint(&(&dependency.source, dependency.resolution))
}

fn validate_dependency(dependency: &ScriptedSourceDependency) -> Result<()> {
    let valid_hash = |hash: &str| {
        hash.len() == 64
            && hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    };
    if !valid_hash(&dependency.fingerprint)
        || dependency
            .library_fingerprints
            .iter()
            .any(|(library_id, hash)| *library_id <= 0 || !valid_hash(hash))
    {
        return Err(invalid());
    }
    Ok(())
}

/// Resolve exactly the source API used by Lua and retain semantic library projections.
///
/// # Errors
/// Returns a storage error, or refuses a selection whose media ownership changed.
pub(crate) async fn resolve(
    repository: &SqliteSchedulingContentRepository,
    source: &SchedulingContentSource,
    resolution: ScriptedSourceResolution,
) -> Result<(ResolvedSource, ScriptedSourceDependency)> {
    let pool = &repository.0;
    let resolved = match resolution {
        ScriptedSourceResolution::Media => {
            ResolvedSource::Media(repository.media_items_for_source(source).await?)
        }
        ScriptedSourceResolution::Playlist => {
            ResolvedSource::Playlist(repository.playlist_items_for_source(source).await?)
        }
    };
    let ids = resolved
        .items()
        .map(|item| item.id)
        .collect::<BTreeSet<_>>();
    let owners: BTreeMap<EntityId, EntityId> = sqlx::query_as::<_, (EntityId, EntityId)>(
        r#"SELECT mi."Id", lp."LibraryId" FROM "MediaItem" mi
           JOIN "LibraryPath" lp ON lp."Id" = mi."LibraryPathId"
           WHERE mi."Id" IN (SELECT value FROM json_each(?))"#,
    )
    .bind(serde_json::json!(ids).to_string())
    .fetch_all(pool)
    .await
    .repository()?
    .into_iter()
    .collect();
    if owners.len() != ids.len() {
        return Err(invalid());
    }
    let library_fingerprints = owners
        .values()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|library_id| {
            Ok((
                library_id,
                fingerprint(&resolved.library_projection(&owners, library_id))?,
            ))
        })
        .collect::<Result<_>>()?;
    let dependency = ScriptedSourceDependency {
        source: source.clone(),
        resolution,
        fingerprint: fingerprint(&resolved)?,
        library_fingerprints,
    };
    Ok((resolved, dependency))
}

#[derive(Serialize, Deserialize)]
struct StoredDependency {
    version: u32,
    dependency: ScriptedSourceDependency,
}

fn decode(key: &str, context: &str) -> Result<ScriptedSourceDependency> {
    let stored: StoredDependency = serde_json::from_str(context).map_err(|_| invalid())?;
    if stored.version != 1 || source_key(&stored.dependency)? != key {
        return Err(invalid());
    }
    validate_dependency(&stored.dependency)?;
    Ok(stored.dependency)
}

/// Read the sources that belong to the currently published Scripted timeline.
///
/// # Errors
/// Returns an error for unavailable storage or incompatible persisted state.
pub async fn load(pool: &DbPool, playout_id: EntityId) -> Result<Vec<ScriptedSourceDependency>> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        r#"SELECT "SourceKey", "Context" FROM "PlayoutScriptedSource" WHERE "PlayoutId" = ? ORDER BY "SourceKey""#,
    ).bind(playout_id).fetch_all(pool).await.repository()?;
    rows.into_iter()
        .map(|(key, context)| decode(&key, &context))
        .collect()
}

pub(crate) async fn publish(
    tx: &mut Transaction<'_, Db>,
    playout_id: EntityId,
    dependencies: &[ScriptedSourceDependency],
    replace_existing: bool,
) -> Result<()> {
    let mut encoded = BTreeMap::new();
    for dependency in dependencies {
        validate_dependency(dependency)?;
        let key = source_key(dependency)?;
        let context = serde_json::to_string(&StoredDependency {
            version: 1,
            dependency: dependency.clone(),
        })
        .map_err(|_| invalid())?;
        let observed =
            serde_json::to_string(&dependency.library_fingerprints).map_err(|_| invalid())?;
        if encoded.insert(key, (context, observed)).is_some() {
            return Err(invalid());
        }
    }
    if replace_existing {
        sqlx::query(r#"DELETE FROM "PlayoutScriptedSource" WHERE "PlayoutId" = ?"#)
            .bind(playout_id)
            .execute(&mut **tx)
            .await
            .repository()?;
    }
    for (key, (context, observed)) in encoded {
        // Continue leaves the existing future intact. Preserve its baseline,
        // including changes awaiting scan-impact resolution; only a replacing
        // build can acknowledge a new selection for that existing future.
        sqlx::query(r#"INSERT INTO "PlayoutScriptedSource" ("PlayoutId", "SourceKey", "Context", "ObservedLibraries") VALUES (?, ?, ?, ?)
                       ON CONFLICT("PlayoutId", "SourceKey") DO NOTHING"#)
            .bind(playout_id).bind(key).bind(context).bind(observed).execute(&mut **tx).await.repository()?;
    }
    Ok(())
}
