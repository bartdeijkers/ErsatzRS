use async_trait::async_trait;
use ersatzrs_core::repositories::{MetadataRepository, Result};
use ersatzrs_domain::{EntityId, EpisodeMetadata, MovieMetadata, MusicVideoMetadata};
use sqlx::sqlite::SqliteRow;
use sqlx::{Pool, Row, Sqlite};

use super::common::{
    decode_datetime, decode_optional_datetime, encode_datetime, encode_optional_datetime,
};

#[derive(Clone)]
pub struct SqliteMetadataRepository(pub Pool<Sqlite>);

#[async_trait]
impl MetadataRepository for SqliteMetadataRepository {
    async fn get_movie_metadata(&self, item_id: EntityId) -> Result<MovieMetadata> {
        let row = sqlx::query("SELECT * FROM movie_metadata WHERE item_id = ?")
            .bind(item_id)
            .fetch_one(&self.0)
            .await?;
        map_movie_metadata(&row)
    }

    async fn get_episode_metadata(&self, item_id: EntityId) -> Result<EpisodeMetadata> {
        let row = sqlx::query("SELECT * FROM episode_metadata WHERE item_id = ?")
            .bind(item_id)
            .fetch_one(&self.0)
            .await?;
        map_episode_metadata(&row)
    }

    async fn get_music_video_metadata(&self, item_id: EntityId) -> Result<MusicVideoMetadata> {
        let row = sqlx::query("SELECT * FROM music_video_metadata WHERE item_id = ?")
            .bind(item_id)
            .fetch_one(&self.0)
            .await?;
        map_music_video_metadata(&row)
    }

    async fn upsert_movie_metadata(&self, metadata: &MovieMetadata) -> Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO movie_metadata (
                id, item_id, metadata_kind, title, original_title, sort_title, year,
                release_date, date_added, date_updated, outline, plot, tagline
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(metadata.metadata.id)
        .bind(metadata.metadata.id)
        .bind(format!("{:?}", metadata.metadata.metadata_kind))
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
        sqlx::query(
            "INSERT OR REPLACE INTO episode_metadata (
                id, item_id, metadata_kind, title, original_title, sort_title, year,
                release_date, date_added, date_updated, season_number, episode_number,
                outline, plot
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(metadata.metadata.id)
        .bind(metadata.metadata.id)
        .bind(format!("{:?}", metadata.metadata.metadata_kind))
        .bind(&metadata.metadata.title)
        .bind(&metadata.metadata.original_title)
        .bind(&metadata.metadata.sort_title)
        .bind(metadata.metadata.year)
        .bind(encode_optional_datetime(metadata.metadata.release_date))
        .bind(encode_datetime(metadata.metadata.date_added))
        .bind(encode_datetime(metadata.metadata.date_updated))
        .bind(metadata.season_number)
        .bind(metadata.episode_number)
        .bind(&metadata.outline)
        .bind(&metadata.plot)
        .execute(&self.0)
        .await?;
        Ok(())
    }
}

#[allow(dead_code)]
async fn get_by_id(
    pool: &Pool<Sqlite>,
    table: &str,
    _kind: &'static str,
    id: EntityId,
) -> Result<String> {
    let row = sqlx::query(&format!("SELECT title FROM {table} WHERE id = ?"))
        .bind(id)
        .fetch_one(pool)
        .await?;
    row.try_get("title").map_err(Into::into)
}

fn map_movie_metadata(row: &SqliteRow) -> Result<MovieMetadata> {
    Ok(MovieMetadata {
        metadata: map_common_metadata(row)?,
        outline: row.try_get("outline")?,
        plot: row.try_get("plot")?,
        tagline: row.try_get("tagline")?,
    })
}

fn map_episode_metadata(row: &SqliteRow) -> Result<EpisodeMetadata> {
    Ok(EpisodeMetadata {
        metadata: map_common_metadata(row)?,
        season_number: row.try_get("season_number")?,
        episode_number: row.try_get("episode_number")?,
        outline: row.try_get("outline")?,
        plot: row.try_get("plot")?,
    })
}

fn map_music_video_metadata(row: &SqliteRow) -> Result<MusicVideoMetadata> {
    Ok(MusicVideoMetadata {
        metadata: map_common_metadata(row)?,
        album: row.try_get("album")?,
        track: row.try_get("track")?,
        artist_ids: Vec::new(),
    })
}

fn map_common_metadata(row: &SqliteRow) -> Result<ersatzrs_domain::Metadata> {
    Ok(ersatzrs_domain::Metadata {
        id: row.try_get("id")?,
        metadata_kind: super::common::decode_enum(
            "metadata kind",
            &row.try_get::<String, _>("metadata_kind")?,
        )?,
        title: row.try_get("title")?,
        original_title: row.try_get("original_title")?,
        sort_title: row.try_get("sort_title")?,
        year: row.try_get("year")?,
        release_date: decode_optional_datetime(
            "metadata release_date",
            row.try_get("release_date")?,
        )?,
        date_added: decode_datetime(
            "metadata date_added",
            &row.try_get::<String, _>("date_added")?,
        )?,
        date_updated: decode_datetime(
            "metadata date_updated",
            &row.try_get::<String, _>("date_updated")?,
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
