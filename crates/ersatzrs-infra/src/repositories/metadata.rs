use async_trait::async_trait;
use ersatzrs_core::repositories::{MetadataRepository, Result};
use ersatzrs_domain::{EntityId, EpisodeMetadata, MovieMetadata, MusicVideoMetadata};
use sqlx::sqlite::SqliteRow;
use sqlx::{Pool, Row, Sqlite};

use super::common::{
    decode_datetime, decode_enum_column, decode_optional_datetime, encode_datetime, encode_enum,
    encode_optional_datetime,
};

#[derive(Clone)]
pub struct SqliteMetadataRepository(pub Pool<Sqlite>);

#[async_trait]
impl MetadataRepository for SqliteMetadataRepository {
    async fn get_movie_metadata(&self, item_id: EntityId) -> Result<MovieMetadata> {
        let row = sqlx::query(r#"SELECT * FROM "MovieMetadata" WHERE "MovieId" = ?"#)
            .bind(item_id)
            .fetch_one(&self.0)
            .await?;
        map_movie_metadata(&row)
    }

    async fn get_episode_metadata(&self, item_id: EntityId) -> Result<EpisodeMetadata> {
        let row = sqlx::query(
            r#"SELECT em.*, s."SeasonNumber"
             FROM "EpisodeMetadata" em
             JOIN "Episode" e ON e."Id" = em."EpisodeId"
             JOIN "Season" s ON s."Id" = e."SeasonId"
             WHERE em."EpisodeId" = ?"#,
        )
        .bind(item_id)
        .fetch_one(&self.0)
        .await?;
        map_episode_metadata(&row)
    }

    async fn get_music_video_metadata(&self, item_id: EntityId) -> Result<MusicVideoMetadata> {
        let row = sqlx::query(r#"SELECT * FROM "MusicVideoMetadata" WHERE "MusicVideoId" = ?"#)
            .bind(item_id)
            .fetch_one(&self.0)
            .await?;
        map_music_video_metadata(&row)
    }

    async fn upsert_movie_metadata(&self, metadata: &MovieMetadata) -> Result<()> {
        sqlx::query(
            r#"INSERT OR REPLACE INTO "MovieMetadata" (
                "Id", "MovieId", "MetadataKind", "Title", "OriginalTitle", "SortTitle", "Year",
                "ReleaseDate", "DateAdded", "DateUpdated", "Outline", "Plot", "Tagline"
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
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
        .await?;
        Ok(())
    }

    async fn upsert_episode_metadata(&self, metadata: &EpisodeMetadata) -> Result<()> {
        // EpisodeMetadata in ErsatzTV does not have SeasonNumber — that lives on "Season".
        // We store it here using ErsatzRS TEXT convention, but the column won't exist in the
        // schema unless we added it. Since ErsatzTV schema has no season_number on EpisodeMetadata,
        // we omit it from the INSERT and store only what the schema supports.
        sqlx::query(
            r#"INSERT OR REPLACE INTO "EpisodeMetadata" (
                "Id", "EpisodeId", "MetadataKind", "Title", "OriginalTitle", "SortTitle", "Year",
                "ReleaseDate", "DateAdded", "DateUpdated", "EpisodeNumber",
                "Outline", "Plot"
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
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
        .await?;
        Ok(())
    }
}

fn map_movie_metadata(row: &SqliteRow) -> Result<MovieMetadata> {
    Ok(MovieMetadata {
        metadata: map_common_metadata(row, "MovieId")?,
        outline: row.try_get("Outline")?,
        plot: row.try_get("Plot")?,
        tagline: row.try_get("Tagline")?,
    })
}

fn map_episode_metadata(row: &SqliteRow) -> Result<EpisodeMetadata> {
    // SeasonNumber comes from the joined "Season" row
    let season_number: i32 = row.try_get("SeasonNumber").unwrap_or(0);
    Ok(EpisodeMetadata {
        metadata: map_common_metadata(row, "EpisodeId")?,
        season_number,
        episode_number: row.try_get("EpisodeNumber").unwrap_or(0),
        outline: row.try_get("Outline")?,
        plot: row.try_get("Plot")?,
    })
}

fn map_music_video_metadata(row: &SqliteRow) -> Result<MusicVideoMetadata> {
    Ok(MusicVideoMetadata {
        metadata: map_common_metadata(row, "MusicVideoId")?,
        album: row.try_get("Album")?,
        track: row.try_get("Track")?,
        artist_ids: Vec::new(),
    })
}

fn map_common_metadata(
    row: &SqliteRow,
    _item_id_column: &str,
) -> Result<ersatzrs_domain::Metadata> {
    Ok(ersatzrs_domain::Metadata {
        id: row.try_get("Id")?,
        metadata_kind: decode_enum_column("metadata kind", row, "MetadataKind")?,
        title: row.try_get("Title")?,
        original_title: row.try_get("OriginalTitle")?,
        sort_title: row.try_get("SortTitle")?,
        year: row.try_get("Year")?,
        release_date: decode_optional_datetime(
            "metadata release_date",
            row.try_get("ReleaseDate")?,
        )?,
        date_added: decode_datetime(
            "metadata date_added",
            &row.try_get::<String, _>("DateAdded")?,
        )?,
        date_updated: decode_datetime(
            "metadata date_updated",
            &row.try_get::<String, _>("DateUpdated")?,
        )?,
        artwork_ids: Vec::new(),
        genre_ids: Vec::new(),
        tag_ids: Vec::new(),
        studio_ids: Vec::new(),
        actor_ids: Vec::new(),
        guid_ids: Vec::new(),
        subtitle_ids: Vec::new(),
    })
}
