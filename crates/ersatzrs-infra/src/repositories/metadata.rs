use crate::db::DbRow;
use crate::repositories::SqlxResultExt as _;
use async_trait::async_trait;
use ersatzrs_core::repositories::{MetadataRepository, Result};
use ersatzrs_domain::{EntityId, EpisodeMetadata, MovieMetadata, MusicVideoMetadata};
use sqlx::{Pool, Row};

use super::common::upsert_sql;
use super::common::{
    decode_datetime, decode_enum_column, decode_optional_datetime, encode_datetime, encode_enum,
    encode_optional_datetime,
};

#[derive(Clone)]
pub struct SqliteMetadataRepository(pub Pool<crate::db::Db>);

#[async_trait]
impl MetadataRepository for SqliteMetadataRepository {
    async fn get_movie_metadata(&self, item_id: EntityId) -> Result<MovieMetadata> {
        let row = sqlx::query(r#"SELECT * FROM "MovieMetadata" WHERE "MovieId" = ?"#)
            .bind(item_id)
            .fetch_one(&self.0)
            .await
            .repository()?;
        map_movie_metadata(&row)
    }

    async fn get_episode_metadata(&self, item_id: EntityId) -> Result<EpisodeMetadata> {
        let row = sqlx::query(
            r#"SELECT "em".*, "s"."SeasonNumber"
             FROM "EpisodeMetadata" AS "em"
             JOIN "Episode" AS "e" ON "e"."Id" = "em"."EpisodeId"
             JOIN "Season" AS "s" ON "s"."Id" = "e"."SeasonId"
             WHERE "em"."EpisodeId" = ?"#,
        )
        .bind(item_id)
        .fetch_one(&self.0)
        .await
        .repository()?;
        map_episode_metadata(&row)
    }

    async fn get_music_video_metadata(&self, item_id: EntityId) -> Result<MusicVideoMetadata> {
        let row = sqlx::query(r#"SELECT * FROM "MusicVideoMetadata" WHERE "MusicVideoId" = ?"#)
            .bind(item_id)
            .fetch_one(&self.0)
            .await
            .repository()?;
        map_music_video_metadata(&row)
    }

    async fn upsert_movie_metadata(&self, metadata: &MovieMetadata) -> Result<()> {
        sqlx::query(upsert_sql!(
            r#"INSERT INTO "MovieMetadata" (
                "Id", "MovieId", "MetadataKind", "Title", "OriginalTitle", "SortTitle", "Year",
                "ReleaseDate", "DateAdded", "DateUpdated", "Outline", "Plot", "Tagline"
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
            &[&["Id"]],
        ))
        .bind(metadata.metadata.id)
        .bind(metadata.metadata.id)
        .bind(encode_enum(metadata.metadata.metadata_kind))
        .bind(&metadata.metadata.title)
        .bind(&metadata.metadata.original_title)
        .bind(&metadata.metadata.sort_title)
        .bind(metadata.metadata.year)
        .bind(encode_optional_datetime(metadata.metadata.release_date))
        .bind(encode_datetime(metadata.metadata.date_added))
        .bind(encode_datetime(metadata.metadata.date_updated))
        .bind(&metadata.outline)
        .bind(&metadata.plot)
        .bind(&metadata.tagline)
        .execute(&self.0)
        .await
        .repository()?;
        Ok(())
    }

    async fn upsert_episode_metadata(&self, metadata: &EpisodeMetadata) -> Result<()> {
        // EpisodeMetadata in ErsatzTV does not have SeasonNumber — that lives on "Season".
        // We store it here using ErsatzRS TEXT convention, but the column won't exist in the
        // schema unless we added it. Since ErsatzTV schema has no season_number on EpisodeMetadata,
        // we omit it from the INSERT and store only what the schema supports.
        sqlx::query(upsert_sql!(
            r#"INSERT INTO "EpisodeMetadata" (
                "Id", "EpisodeId", "MetadataKind", "Title", "OriginalTitle", "SortTitle", "Year",
                "ReleaseDate", "DateAdded", "DateUpdated", "EpisodeNumber",
                "Outline", "Plot"
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
            &[&["Id"]],
        ))
        .bind(metadata.metadata.id)
        .bind(metadata.metadata.id)
        .bind(encode_enum(metadata.metadata.metadata_kind))
        .bind(&metadata.metadata.title)
        .bind(&metadata.metadata.original_title)
        .bind(&metadata.metadata.sort_title)
        .bind(metadata.metadata.year)
        .bind(encode_optional_datetime(metadata.metadata.release_date))
        .bind(encode_datetime(metadata.metadata.date_added))
        .bind(encode_datetime(metadata.metadata.date_updated))
        .bind(metadata.episode_number)
        .bind(&metadata.outline)
        .bind(&metadata.plot)
        .execute(&self.0)
        .await
        .repository()?;
        Ok(())
    }
}

fn map_movie_metadata(row: &DbRow) -> Result<MovieMetadata> {
    Ok(MovieMetadata {
        metadata: map_common_metadata(row, "MovieId")?,
        outline: row.try_get("Outline").repository()?,
        plot: row.try_get("Plot").repository()?,
        tagline: row.try_get("Tagline").repository()?,
    })
}

fn map_episode_metadata(row: &DbRow) -> Result<EpisodeMetadata> {
    // SeasonNumber comes from the joined "Season" row
    let season_number: i32 = row.try_get("SeasonNumber").unwrap_or(0);
    Ok(EpisodeMetadata {
        metadata: map_common_metadata(row, "EpisodeId")?,
        season_number,
        episode_number: row.try_get("EpisodeNumber").unwrap_or(0),
        outline: row.try_get("Outline").repository()?,
        plot: row.try_get("Plot").repository()?,
    })
}

fn map_music_video_metadata(row: &DbRow) -> Result<MusicVideoMetadata> {
    Ok(MusicVideoMetadata {
        metadata: map_common_metadata(row, "MusicVideoId")?,
        album: row.try_get("Album").repository()?,
        track: row.try_get("Track").repository()?,
        artist_ids: Vec::new(),
    })
}

fn map_common_metadata(row: &DbRow, _item_id_column: &str) -> Result<ersatzrs_domain::Metadata> {
    Ok(ersatzrs_domain::Metadata {
        id: row.try_get("Id").repository()?,
        metadata_kind: decode_enum_column("metadata kind", row, "MetadataKind")?,
        title: row.try_get("Title").repository()?,
        original_title: row.try_get("OriginalTitle").repository()?,
        sort_title: row.try_get("SortTitle").repository()?,
        year: row.try_get("Year").repository()?,
        release_date: decode_optional_datetime(
            "metadata release_date",
            row.try_get("ReleaseDate").repository()?,
        )?,
        date_added: decode_datetime(
            "metadata date_added",
            &row.try_get::<String, _>("DateAdded").repository()?,
        )?,
        date_updated: decode_datetime(
            "metadata date_updated",
            &row.try_get::<String, _>("DateUpdated").repository()?,
        )?,
        artwork_ids: Vec::new(),
        genre_ids: Vec::new(),
        tag_ids: Vec::new(),
        studio_ids: Vec::new(),
        music_label_ids: Vec::new(),
        language_codes: Vec::new(),
        content_ratings: row
            .try_get::<Option<String>, _>("ContentRating")
            .unwrap_or_default()
            .map(|value| ersatzrs_core::metadata_facets::normalize_content_ratings(&[value]))
            .unwrap_or_default(),
        actor_ids: Vec::new(),
        guid_ids: Vec::new(),
        subtitle_ids: Vec::new(),
    })
}
