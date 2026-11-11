//! `GroupIntoFakeCollections` from ErsatzTV a143968e: full source identities, one member per ID.
use super::SqliteSchedulingContentRepository;
use crate::repositories::{common::decode_message, SqlxResultExt as _};
use ersatzrs_core::{
    repositories::Result,
    scheduling::{
        classic_schedule_item_key, SchedulingCollectionKey, SchedulingMediaItem,
        SchedulingPlaylistItem,
    },
};
use ersatzrs_domain::{
    CollectionType, EntityId, FillWithGroupMode, PlaybackOrder, ProgramScheduleItem,
};
use sqlx::Row;
use std::collections::{BTreeMap, BTreeSet};

// Every outer row is one selected MediaItem. Metadata can only contribute one
// scalar first album artist; alternate/duplicate metadata cannot multiply it.
const GROUP_METADATA_SQL: &str = r#"SELECT "mi"."Id",
    EXISTS(SELECT 1 FROM "Movie" WHERE "Id"="mi"."Id") AS "IsMovie",
    EXISTS(SELECT 1 FROM "OtherVideo" WHERE "Id"="mi"."Id") AS "IsOtherVideo",
    EXISTS(SELECT 1 FROM "Song" WHERE "Id"="mi"."Id") AS "IsSong",
    (SELECT "artist"."value" FROM "SongMetadata" AS "sm",
        json_each(CASE WHEN json_valid("sm"."AlbumArtists") THEN "sm"."AlbumArtists" ELSE '[]' END) AS "artist"
     WHERE "sm"."SongId"="mi"."Id"
     ORDER BY "sm"."Id", CAST("artist"."key" AS INTEGER) LIMIT 1) AS "AlbumArtist"
    FROM "MediaItem" AS "mi"
    WHERE "mi"."Id" IN (SELECT "value" FROM json_each(?))
    ORDER BY "mi"."Id""#;

#[derive(Debug)]
struct GroupMetadata {
    movie: bool,
    other: bool,
    song: bool,
    album_artist: String,
}

impl SqliteSchedulingContentRepository {
    pub(super) async fn classic_fill_groups(
        &self,
        items: &[ProgramScheduleItem],
        media_by_key: &BTreeMap<String, Vec<SchedulingMediaItem>>,
    ) -> Result<BTreeMap<EntityId, Vec<SchedulingPlaylistItem>>> {
        let selected = items
            .iter()
            .filter(|item| item.fill_with_group_mode != FillWithGroupMode::None)
            .collect::<Vec<_>>();
        let ids = selected
            .iter()
            .filter_map(|item| media_by_key.get(&classic_schedule_item_key(item)))
            .flatten()
            .map(|media| media.id)
            .collect::<BTreeSet<_>>();
        if ids.is_empty() {
            return Ok(BTreeMap::new());
        }
        let json = serde_json::to_string(&ids)
            .map_err(|error| decode_message("Classic group membership", &error.to_string()))?;
        let rows = sqlx::query(GROUP_METADATA_SQL)
            .bind(json)
            .fetch_all(&self.0)
            .await
            .repository()?;
        let metadata = rows
            .iter()
            .map(|row| {
                Ok((
                    row.try_get::<EntityId, _>("Id").repository()?,
                    GroupMetadata {
                        movie: row.try_get("IsMovie").repository()?,
                        other: row.try_get("IsOtherVideo").repository()?,
                        song: row.try_get("IsSong").repository()?,
                        album_artist: row
                            .try_get::<Option<String>, _>("AlbumArtist")
                            .repository()?
                            .unwrap_or_default(),
                    },
                ))
            })
            .collect::<Result<BTreeMap<_, _>>>()?;
        let mut result = BTreeMap::new();
        for item in selected {
            let key = classic_schedule_item_key(item);
            let scope = format!("{}:{key}", item.id);
            let mut positions = BTreeMap::new();
            let mut groups: Vec<(u8, SchedulingPlaylistItem)> = Vec::new();
            let mut seen = BTreeSet::new();
            for media in media_by_key.get(&key).into_iter().flatten() {
                if !seen.insert(media.id) {
                    continue;
                }
                let Some(meta) = metadata.get(&media.id) else {
                    continue;
                };
                let Some((rank, collection_key)) = group_key(media, meta, &scope) else {
                    continue;
                };
                let identity = serde_json::to_string(&collection_key).map_err(|error| {
                    decode_message("Classic group identity", &error.to_string())
                })?;
                let position = *positions.entry(identity).or_insert_with(|| {
                    let position = groups.len();
                    groups.push((
                        rank,
                        SchedulingPlaylistItem {
                            collection_key,
                            playback_order: PlaybackOrder::Chronological,
                            play_all: true,
                            count: None,
                            media_items: Vec::new(),
                        },
                    ));
                    position
                });
                groups[position].1.media_items.push(media.clone());
            }
            groups.sort_by_key(|(rank, _)| *rank);
            result.insert(
                item.id,
                groups.into_iter().map(|(_, group)| group).collect(),
            );
        }
        Ok(result)
    }
}

fn group_key(
    media: &SchedulingMediaItem,
    metadata: &GroupMetadata,
    scope: &str,
) -> Option<(u8, SchedulingCollectionKey)> {
    let (rank, collection_type, media_item_id, fake_collection_key) =
        if let Some(id) = media.show_id.filter(|id| *id > 0) {
            (
                0,
                CollectionType::TelevisionShow,
                Some(id),
                scope.to_owned(),
            )
        } else if let Some(id) = media.artist_id.filter(|id| *id > 0) {
            (1, CollectionType::Artist, Some(id), scope.to_owned())
        } else if metadata.song {
            (
                2,
                CollectionType::FakeCollection,
                None,
                format!("{scope}:artist:{}", metadata.album_artist),
            )
        } else if metadata.movie {
            (3, CollectionType::FakeCollection, None, scope.to_owned())
        } else if metadata.other {
            (
                4,
                CollectionType::FakeCollection,
                None,
                format!("{scope}:other"),
            )
        } else {
            return None;
        };
    Some((
        rank,
        SchedulingCollectionKey {
            collection_type,
            media_item_id,
            fake_collection_key: Some(fake_collection_key),
            ..SchedulingCollectionKey::default()
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn classic_group_metadata_query_uses_selected_id_and_metadata_indexes() {
        let pool = crate::db::init("sqlite::memory:").await.unwrap();
        // Only a fixed prefix and the static audited query are concatenated.
        let explain = format!("EXPLAIN QUERY PLAN {GROUP_METADATA_SQL}");
        let rows = sqlx::query(crate::db::assert_sql_safe(&explain))
            .bind("[20,30,40]")
            .fetch_all(&pool)
            .await
            .unwrap();
        let details = rows
            .iter()
            .map(|row| row.try_get::<String, _>("detail").unwrap())
            .collect::<Vec<_>>();
        assert!(
            details
                .iter()
                .any(|detail| detail.contains("SEARCH mi USING INTEGER PRIMARY KEY")),
            "{details:?}"
        );
        assert!(
            details
                .iter()
                .any(|detail| detail.contains("IX_SongMetadata_SongId")),
            "{details:?}"
        );
        assert!(
            !details.iter().any(|detail| detail.starts_with("SCAN mi")),
            "{details:?}"
        );
    }
}
