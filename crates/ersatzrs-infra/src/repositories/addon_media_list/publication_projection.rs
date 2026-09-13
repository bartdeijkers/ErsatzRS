//! Projection inputs keep retained publication content separate from draft rows.

use std::collections::BTreeMap;

use ersatzrs_core::repositories::{AddonMediaListRevisionSnapshot, RepositoryError, Result};
use ersatzrs_domain::{AddonMediaListItemAvailability, AddonMediaListItemKind, EntityId};
use sqlx::Row as _;

use crate::db::DbPool;
use crate::repositories::common::{decode_enum_column, encode_enum};
use crate::repositories::SqlxResultExt as _;

pub(super) struct ProjectionItem {
    pub id: EntityId,
    pub title: String,
    pub year: Option<i32>,
    pub season: Option<i32>,
    pub episode: Option<i32>,
    pub availability: AddonMediaListItemAvailability,
}

/// One input per published parentless Remote Stream, ordered by rank/local ID.
/// Only receiving owner IDs are read from current rows for a retained source.
pub(super) async fn load_items(
    pool: &DbPool,
    list_id: EntityId,
    source: Option<&AddonMediaListRevisionSnapshot>,
) -> Result<Vec<ProjectionItem>> {
    if let Some(snapshot) = source {
        if snapshot.list_id != list_id {
            return Err(invalid());
        }
        let owners: Vec<(String, EntityId)> = sqlx::query_as(
            r#"SELECT "ProviderId", "Id" FROM "AddonMediaListItem" WHERE "AddonMediaListId"=?"#,
        )
        .bind(list_id)
        .fetch_all(pool)
        .await
        .repository()?;
        let owners: BTreeMap<_, _> = owners.into_iter().collect();
        let mut selected = Vec::new();
        for item in snapshot.items.iter().filter(|item| {
            item.published
                && item.parent_provider_id.is_none()
                && item.kind == AddonMediaListItemKind::RemoteStream
        }) {
            let id = *owners.get(&item.provider_id).ok_or_else(invalid)?;
            selected.push((
                item.rank,
                ProjectionItem {
                    id,
                    title: item.title.clone(),
                    year: item.year,
                    season: item.season,
                    episode: item.episode,
                    availability: item.availability,
                },
            ));
        }
        selected.sort_by_key(|(rank, item)| (*rank, item.id));
        return Ok(selected.into_iter().map(|(_, item)| item).collect());
    }
    let rows = sqlx::query(
        r#"SELECT "Id", "Title", "Year", "Season", "Episode", "Availability"
           FROM "AddonMediaListItem"
           WHERE "AddonMediaListId"=? AND "ParentProviderId" IS NULL
             AND "Published"=1 AND "Kind"=? ORDER BY "Rank", "Id""#,
    )
    .bind(list_id)
    .bind(encode_enum(AddonMediaListItemKind::RemoteStream))
    .fetch_all(pool)
    .await
    .repository()?;
    rows.iter()
        .map(|row| {
            Ok(ProjectionItem {
                id: row.try_get("Id").repository()?,
                title: row.try_get("Title").repository()?,
                year: row.try_get("Year").repository()?,
                season: row.try_get("Season").repository()?,
                episode: row.try_get("Episode").repository()?,
                availability: decode_enum_column("item availability", row, "Availability")?,
            })
        })
        .collect()
}

fn invalid() -> RepositoryError {
    RepositoryError::Conflict("the publication projection has no matching receiving owner".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ersatzrs_core::repositories::{
        AddonMediaListLifecycleRepository as _, AddonMediaListRevisionArtifacts,
    };

    async fn fixture() -> (DbPool, AddonMediaListRevisionSnapshot) {
        let pool = crate::db::init("sqlite::memory:").await.unwrap();
        sqlx::raw_sql(
            r#"INSERT INTO "AddonInstallation"
               ("AddonId","Version","Enabled","PackageSha256","ContentSha256",
                "ManifestJson","InstallPath","InstalledAt","UpdatedAt")
               VALUES ('org.example.projection','1',1,'fixture','fixture','{}','addons/fixture',
                       '2026-09-13','2026-09-13');
               INSERT INTO "AddonMediaList"
               ("Id","AddonId","ProviderId","SourceUrl","Name","DraftRevision","PublishedRevision")
               VALUES (1,'org.example.projection','list','https://example.invalid/list','Fixture',4,2);
               INSERT INTO "AddonMediaListItem"
               ("Id","AddonMediaListId","ProviderId","DisplayTitle","Title","Kind","Rank",
                "Year","Season","Episode","Availability","Published") VALUES
               (10,1,'first','First','Retained first',4,5,2001,2,3,1,1),
               (20,1,'second','Second','Retained second',4,5,2002,4,5,0,1),
               (30,1,'third','Third','Retained third',4,0,2003,6,7,0,1);"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        let snapshot = crate::repositories::SqliteAddonMediaListRepository(pool.clone())
            .retain_published_revision(
                1,
                2,
                4,
                chrono::Utc::now(),
                &[],
                &AddonMediaListRevisionArtifacts::default(),
            )
            .await
            .unwrap();
        (pool, snapshot)
    }

    #[tokio::test]
    async fn retained_projection_inputs_keep_content_and_order_without_draft_membership() {
        let (pool, mut snapshot) = fixture().await;
        let current = load_items(&pool, 1, None).await.unwrap();
        assert_eq!(
            current.iter().map(|item| item.id).collect::<Vec<_>>(),
            [30, 10, 20]
        );
        assert_eq!(current[1].title, "Retained first");
        sqlx::query(
            r#"UPDATE "AddonMediaListItem" SET "InDraft"=0,"Published"=0,"Rank"=99,
               "Title"='New draft',"Year"=2026,"Season"=8,"Episode"=9,"Availability"=0
               WHERE "AddonMediaListId"=1"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        // Input order does not replace rank/local-owner ordering.
        snapshot.items.reverse();
        let items = load_items(&pool, 1, Some(&snapshot)).await.unwrap();
        assert_eq!(
            items.iter().map(|item| item.id).collect::<Vec<_>>(),
            [30, 10, 20]
        );
        let first = &items[1];
        assert_eq!(first.title, "Retained first");
        assert_eq!(
            (first.year, first.season, first.episode),
            (Some(2001), Some(2), Some(3))
        );
        assert_eq!(
            first.availability,
            AddonMediaListItemAvailability::Unavailable
        );
        assert!(load_items(&pool, 1, None).await.unwrap().is_empty());
        let unchanged: String =
            sqlx::query_scalar(r#"SELECT "Title" FROM "AddonMediaListItem" WHERE "Id"=10"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(unchanged, "New draft");
    }

    #[tokio::test]
    async fn retained_projection_inputs_filter_membership_and_refuse_missing_owners() {
        let (pool, mut snapshot) = fixture().await;
        snapshot.items[0].published = false;
        snapshot.items[1].parent_provider_id = Some("third".into());
        snapshot.items[2].kind = AddonMediaListItemKind::Movie;
        assert!(load_items(&pool, 1, Some(&snapshot))
            .await
            .unwrap()
            .is_empty());
        snapshot.items[0].published = true;
        snapshot.items[0].provider_id = "missing-owner".into();
        assert!(load_items(&pool, 1, Some(&snapshot)).await.is_err());
        snapshot.items.clear();
        snapshot.list_id = 2;
        assert!(load_items(&pool, 1, Some(&snapshot)).await.is_err());
    }
}
