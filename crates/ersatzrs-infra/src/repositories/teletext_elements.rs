//! `SQLite` storage for the teletext element library and channel links.

use async_trait::async_trait;
use ersatzrs_core::repositories::{RepositoryError, Result, TeletextElementRepository};
use ersatzrs_domain::{
    ChannelTeletextElement, EntityId, TeletextElement, TeletextElementBinding,
    TeletextElementBindingSource, TeletextElementSourceFormat, TeletextElementSummary,
    TeletextElementUsage,
};
use sqlx::{Pool, Row};

use crate::db::{Db, DbRow};
use crate::repositories::SqlxResultExt as _;

#[derive(Clone)]
pub struct SqliteTeletextElementRepository(pub Pool<Db>);

fn decode_error(value: &str) -> RepositoryError {
    RepositoryError::Decode {
        kind: "teletext element",
        value: value.to_owned(),
    }
}

fn page_number(value: i64) -> Result<u16> {
    u16::try_from(value).map_err(|_| decode_error("page number"))
}

fn bindings_from_json(value: &str) -> Result<Vec<TeletextElementBinding>> {
    serde_json::from_str(value).map_err(|_| decode_error("bindings"))
}

fn bindings_to_json(bindings: &[TeletextElementBinding]) -> Result<String> {
    serde_json::to_string(bindings).map_err(|_| decode_error("bindings"))
}

fn map_element(row: &DbRow) -> Result<TeletextElement> {
    let format: i32 = row.try_get("SourceFormat").repository()?;
    Ok(TeletextElement {
        id: row.try_get("Id").repository()?,
        name: row.try_get("Name").repository()?,
        source_format: TeletextElementSourceFormat::from_i32(format)
            .ok_or_else(|| decode_error("source format"))?,
        source_file_name: row.try_get("SourceFileName").repository()?,
        document: row.try_get("Document").repository()?,
        page_number: page_number(row.try_get("PageNumber").repository()?)?,
        subpage_count: u16::try_from(row.try_get::<i64, _>("SubpageCount").repository()?)
            .map_err(|_| decode_error("subpage count"))?,
        cycle_seconds: u16::try_from(row.try_get::<i64, _>("CycleSeconds").repository()?)
            .map_err(|_| decode_error("cycle seconds"))?,
        bindings: bindings_from_json(&row.try_get::<String, _>("Bindings").repository()?)?,
        revision: row.try_get("Revision").repository()?,
        updated_at: row.try_get("UpdatedAt").repository()?,
    })
}

async fn name_taken(pool: &Pool<Db>, name: &str, id: EntityId) -> Result<bool> {
    let count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "TeletextElement" WHERE "Name" = ? AND "Id" <> ?"#,
    )
    .bind(name)
    .bind(id)
    .fetch_one(pool)
    .await
    .repository()?;
    Ok(count > 0)
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

#[async_trait]
impl TeletextElementRepository for SqliteTeletextElementRepository {
    async fn list_summaries(&self) -> Result<Vec<TeletextElementSummary>> {
        let rows = sqlx::query(
            r#"SELECT e."Id", e."Name", e."SourceFormat", e."PageNumber", e."SubpageCount",
                      e."Bindings",
                      (SELECT COUNT(*) FROM "ChannelTeletextElement" c
                        WHERE c."TeletextElementId" = e."Id") AS "ChannelCount"
               FROM "TeletextElement" e
               ORDER BY e."Name" COLLATE NOCASE, e."Id""#,
        )
        .fetch_all(&self.0)
        .await
        .repository()?;
        rows.iter()
            .map(|row| {
                let format: i32 = row.try_get("SourceFormat").repository()?;
                let bindings =
                    bindings_from_json(&row.try_get::<String, _>("Bindings").repository()?)?;
                let unset = bindings
                    .iter()
                    .filter(|binding| binding.source == TeletextElementBindingSource::Unset)
                    .count();
                Ok(TeletextElementSummary {
                    id: row.try_get("Id").repository()?,
                    name: row.try_get("Name").repository()?,
                    source_format: TeletextElementSourceFormat::from_i32(format)
                        .ok_or_else(|| decode_error("source format"))?,
                    page_number: page_number(row.try_get("PageNumber").repository()?)?,
                    subpage_count: u16::try_from(
                        row.try_get::<i64, _>("SubpageCount").repository()?,
                    )
                    .map_err(|_| decode_error("subpage count"))?,
                    binding_count: u16::try_from(bindings.len()).unwrap_or(u16::MAX),
                    unset_binding_count: u16::try_from(unset).unwrap_or(u16::MAX),
                    channel_count: u16::try_from(
                        row.try_get::<i64, _>("ChannelCount").repository()?,
                    )
                    .unwrap_or(u16::MAX),
                })
            })
            .collect()
    }

    async fn get(&self, id: EntityId) -> Result<Option<TeletextElement>> {
        sqlx::query(r#"SELECT * FROM "TeletextElement" WHERE "Id" = ?"#)
            .bind(id)
            .fetch_optional(&self.0)
            .await
            .repository()?
            .as_ref()
            .map(map_element)
            .transpose()
    }

    async fn create(&self, element: &TeletextElement) -> Result<EntityId> {
        if name_taken(&self.0, &element.name, 0).await? {
            return Err(RepositoryError::Conflict(
                "teletext element name".to_owned(),
            ));
        }
        let result = sqlx::query(
            r#"INSERT INTO "TeletextElement" (
                   "Name", "SourceFormat", "SourceFileName", "Document", "PageNumber",
                   "SubpageCount", "CycleSeconds", "Bindings", "Revision", "UpdatedAt"
               ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1, ?)"#,
        )
        .bind(&element.name)
        .bind(element.source_format.as_i32())
        .bind(&element.source_file_name)
        .bind(&element.document)
        .bind(i64::from(element.page_number))
        .bind(i64::from(element.subpage_count))
        .bind(i64::from(element.cycle_seconds))
        .bind(bindings_to_json(&element.bindings)?)
        .bind(now())
        .execute(&self.0)
        .await
        .repository()?;
        Ok(crate::db::last_insert_id(result))
    }

    async fn update(&self, element: &TeletextElement, expected_revision: i64) -> Result<i64> {
        if name_taken(&self.0, &element.name, element.id).await? {
            return Err(RepositoryError::Conflict(
                "teletext element name".to_owned(),
            ));
        }
        let result = sqlx::query(
            r#"UPDATE "TeletextElement" SET
                   "Name" = ?, "SourceFormat" = ?, "SourceFileName" = ?, "Document" = ?,
                   "PageNumber" = ?, "SubpageCount" = ?, "CycleSeconds" = ?, "Bindings" = ?,
                   "Revision" = "Revision" + 1, "UpdatedAt" = ?
               WHERE "Id" = ? AND "Revision" = ?"#,
        )
        .bind(&element.name)
        .bind(element.source_format.as_i32())
        .bind(&element.source_file_name)
        .bind(&element.document)
        .bind(i64::from(element.page_number))
        .bind(i64::from(element.subpage_count))
        .bind(i64::from(element.cycle_seconds))
        .bind(bindings_to_json(&element.bindings)?)
        .bind(now())
        .bind(element.id)
        .bind(expected_revision)
        .execute(&self.0)
        .await
        .repository()?;
        if result.rows_affected() == 1 {
            return Ok(expected_revision + 1);
        }
        if self.get(element.id).await?.is_none() {
            return Err(RepositoryError::NotFound {
                kind: "teletext element",
                id: element.id,
            });
        }
        Err(RepositoryError::Conflict(
            "teletext element revision".to_owned(),
        ))
    }

    async fn delete(&self, id: EntityId) -> Result<bool> {
        let mut transaction = self.0.begin().await.repository()?;
        sqlx::query(r#"DELETE FROM "ChannelTeletextElement" WHERE "TeletextElementId" = ?"#)
            .bind(id)
            .execute(&mut *transaction)
            .await
            .repository()?;
        let result = sqlx::query(r#"DELETE FROM "TeletextElement" WHERE "Id" = ?"#)
            .bind(id)
            .execute(&mut *transaction)
            .await
            .repository()?;
        transaction.commit().await.repository()?;
        Ok(result.rows_affected() == 1)
    }

    async fn usage(&self, element_id: EntityId) -> Result<Vec<TeletextElementUsage>> {
        let rows = sqlx::query(
            r#"SELECT c."Id" AS "ChannelId", c."Number", c."Name",
                      COALESCE(l."PageNumber", e."PageNumber") AS "EffectivePage",
                      l."IsEnabled"
               FROM "ChannelTeletextElement" l
               JOIN "Channel" c ON c."Id" = l."ChannelId"
               JOIN "TeletextElement" e ON e."Id" = l."TeletextElementId"
               WHERE l."TeletextElementId" = ?
               ORDER BY CAST(c."Number" AS REAL), c."Number""#,
        )
        .bind(element_id)
        .fetch_all(&self.0)
        .await
        .repository()?;
        rows.iter()
            .map(|row| {
                Ok(TeletextElementUsage {
                    channel_id: row.try_get("ChannelId").repository()?,
                    channel_number: row.try_get("Number").repository()?,
                    channel_name: row.try_get("Name").repository()?,
                    page_number: page_number(row.try_get("EffectivePage").repository()?)?,
                    enabled: row.try_get::<i64, _>("IsEnabled").repository()? != 0,
                })
            })
            .collect()
    }

    async fn list_channel_elements(
        &self,
        channel_id: EntityId,
    ) -> Result<Vec<ChannelTeletextElement>> {
        let rows = sqlx::query(
            r#"SELECT l."Id", l."ChannelId", l."TeletextElementId", l."PageNumber",
                      l."IsEnabled", e."Name",
                      COALESCE(l."PageNumber", e."PageNumber") AS "EffectivePage"
               FROM "ChannelTeletextElement" l
               JOIN "TeletextElement" e ON e."Id" = l."TeletextElementId"
               WHERE l."ChannelId" = ?
               ORDER BY "EffectivePage", l."Id""#,
        )
        .bind(channel_id)
        .fetch_all(&self.0)
        .await
        .repository()?;
        rows.iter()
            .map(|row| {
                Ok(ChannelTeletextElement {
                    id: row.try_get("Id").repository()?,
                    channel_id: row.try_get("ChannelId").repository()?,
                    element_id: row.try_get("TeletextElementId").repository()?,
                    element_name: row.try_get("Name").repository()?,
                    page_number: row
                        .try_get::<Option<i64>, _>("PageNumber")
                        .repository()?
                        .map(page_number)
                        .transpose()?,
                    effective_page_number: page_number(row.try_get("EffectivePage").repository()?)?,
                    enabled: row.try_get::<i64, _>("IsEnabled").repository()? != 0,
                })
            })
            .collect()
    }

    async fn save_channel_element(
        &self,
        channel_id: EntityId,
        element_id: EntityId,
        page_number: Option<u16>,
        enabled: bool,
    ) -> Result<EntityId> {
        sqlx::query(
            r#"INSERT INTO "ChannelTeletextElement"
                   ("ChannelId", "TeletextElementId", "PageNumber", "IsEnabled")
               VALUES (?, ?, ?, ?)
               ON CONFLICT ("ChannelId", "TeletextElementId")
               DO UPDATE SET "PageNumber" = excluded."PageNumber",
                             "IsEnabled" = excluded."IsEnabled""#,
        )
        .bind(channel_id)
        .bind(element_id)
        .bind(page_number.map(i64::from))
        .bind(i64::from(enabled))
        .execute(&self.0)
        .await
        .repository()?;
        sqlx::query_scalar(
            r#"SELECT "Id" FROM "ChannelTeletextElement"
               WHERE "ChannelId" = ? AND "TeletextElementId" = ?"#,
        )
        .bind(channel_id)
        .bind(element_id)
        .fetch_one(&self.0)
        .await
        .repository()
    }

    async fn remove_channel_element(
        &self,
        channel_id: EntityId,
        link_id: EntityId,
    ) -> Result<bool> {
        let result = sqlx::query(
            r#"DELETE FROM "ChannelTeletextElement" WHERE "Id" = ? AND "ChannelId" = ?"#,
        )
        .bind(link_id)
        .bind(channel_id)
        .execute(&self.0)
        .await
        .repository()?;
        Ok(result.rows_affected() == 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_pool() -> Pool<Db> {
        crate::db::init("sqlite::memory:")
            .await
            .expect("initialize database")
    }

    fn element(name: &str) -> TeletextElement {
        TeletextElement {
            id: 0,
            name: name.to_owned(),
            source_format: TeletextElementSourceFormat::Tti,
            source_file_name: "welcome.tti".to_owned(),
            document: b"PN,10000\r\nOL,1,HELLO\r\n".to_vec(),
            page_number: 0x100,
            subpage_count: 1,
            cycle_seconds: 10,
            bindings: vec![TeletextElementBinding {
                field: "NOW".to_owned(),
                subpage: 0,
                row: 4,
                column: 2,
                length: 10,
                source: TeletextElementBindingSource::Unset,
                static_text: String::new(),
            }],
            revision: 0,
            updated_at: String::new(),
        }
    }

    async fn seed_channel(pool: &Pool<Db>) -> EntityId {
        sqlx::query(
            r#"INSERT INTO "Resolution" ("Id", "Name", "Width", "Height")
               VALUES (1, '1920x1080', 1920, 1080)"#,
        )
        .execute(pool)
        .await
        .expect("resolution");
        sqlx::query(
            r#"INSERT INTO "FFmpegProfile" (
                   "Id", "AudioBitrate", "AudioBufferSize", "AudioChannels",
                   "AudioFormat", "AudioSampleRate", "BitDepth", "HardwareAcceleration",
                   "Name", "NormalizeLoudnessMode", "ResolutionId", "ScalingBehavior",
                   "ThreadCount", "VaapiDriver", "VideoBitrate", "VideoBufferSize",
                   "VideoFormat"
               ) VALUES (1, 192, 0, 2, 0, 48000, 8, 0, 'default', 0, 1, 0, 0, 0,
                         4000, 0, 0)"#,
        )
        .execute(pool)
        .await
        .expect("profile");
        sqlx::query_scalar(
            r#"INSERT INTO "Channel" (
                   "UniqueId", "Number", "SortNumber", "Name", "Group",
                   "FFmpegProfileId", "StreamingMode", "StreamSelectorMode", "SubtitleMode",
                   "MusicVideoCreditsMode", "SongVideoMode", "PlayoutSource", "PlayoutMode",
                   "TranscodeMode", "IdleBehavior", "IsEnabled", "ShowInEpg"
               ) VALUES ('00000000-0000-0000-0000-000000000001', '1', 1.0,
                         'Mijn Kanaal', 'ErsatzRS', 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1)
               RETURNING "Id""#,
        )
        .fetch_one(pool)
        .await
        .expect("channel")
    }

    #[tokio::test]
    async fn elements_round_trip_with_revision_and_unique_names() {
        let repository = SqliteTeletextElementRepository(test_pool().await);
        let id = repository.create(&element("Welkom")).await.expect("create");
        assert!(matches!(
            repository.create(&element("Welkom")).await,
            Err(RepositoryError::Conflict(_))
        ));
        let mut stored = repository.get(id).await.expect("get").expect("exists");
        assert_eq!(stored.revision, 1);
        assert_eq!(stored.bindings.len(), 1);
        stored.name = "Welkom 2".to_owned();
        assert_eq!(repository.update(&stored, 1).await.expect("update"), 2);
        assert!(matches!(
            repository.update(&stored, 1).await,
            Err(RepositoryError::Conflict(_))
        ));
        let summaries = repository.list_summaries().await.expect("list");
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].unset_binding_count, 1);
        assert!(repository.delete(id).await.expect("delete"));
        assert!(repository.get(id).await.expect("get").is_none());
    }

    #[tokio::test]
    async fn channel_links_upsert_report_usage_and_cascade() {
        let pool = test_pool().await;
        let channel_id = seed_channel(&pool).await;
        let repository = SqliteTeletextElementRepository(pool);
        let id = repository.create(&element("Welkom")).await.expect("create");
        let link = repository
            .save_channel_element(channel_id, id, None, true)
            .await
            .expect("link");
        let again = repository
            .save_channel_element(channel_id, id, Some(0x150), true)
            .await
            .expect("relink");
        assert_eq!(link, again);
        let links = repository
            .list_channel_elements(channel_id)
            .await
            .expect("links");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].effective_page_number, 0x150);
        let usage = repository.usage(id).await.expect("usage");
        assert_eq!(usage[0].channel_name, "Mijn Kanaal");
        assert_eq!(
            repository.list_summaries().await.expect("list")[0].channel_count,
            1
        );
        assert!(repository.delete(id).await.expect("delete"));
        assert!(repository
            .list_channel_elements(channel_id)
            .await
            .expect("links")
            .is_empty());
    }
}
