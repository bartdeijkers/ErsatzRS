use async_trait::async_trait;
use ersatzrs_core::repositories::{
    AdminSettingsRepository, Result, SettingsMutation, SettingsOptionKind, SettingsSelectOption,
};
use ersatzrs_domain::EntityId;

#[derive(Clone)]
pub struct SqliteAdminSettingsRepository(pub crate::db::DbPool);

#[async_trait]
impl AdminSettingsRepository for SqliteAdminSettingsRepository {
    async fn get_optional_string(&self, key: &str) -> Result<Option<String>> {
        crate::admin_settings::load_optional_string(&self.0, key).await
    }

    async fn set_string(&self, key: &str, value: &str) -> Result<()> {
        crate::admin_settings::save_config_value(&self.0, key, value).await
    }

    async fn delete_key(&self, key: &str) -> Result<()> {
        crate::admin_settings::save_optional_i64(&self.0, key, None).await
    }

    async fn apply_mutations(&self, mutations: Vec<SettingsMutation>) -> Result<()> {
        use crate::repositories::SqlxResultExt as _;

        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        for mutation in mutations {
            match mutation {
                SettingsMutation::Set { key, value } => {
                    let updated =
                        sqlx::query(r#"UPDATE "ConfigElement" SET "Value" = ? WHERE "Key" = ?"#)
                            .bind(&value)
                            .bind(&key)
                            .execute(&mut *tx)
                            .await
                            .repository()?;
                    if updated.rows_affected() == 0 {
                        sqlx::query(
                            r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)"#,
                        )
                        .bind(&key)
                        .bind(&value)
                        .execute(&mut *tx)
                        .await
                        .repository()?;
                    }
                }
                SettingsMutation::Delete { key } => {
                    sqlx::query(r#"DELETE FROM "ConfigElement" WHERE "Key" = ?"#)
                        .bind(key)
                        .execute(&mut *tx)
                        .await
                        .repository()?;
                }
            }
        }
        tx.commit().await.repository()
    }

    async fn list_options(&self, kind: SettingsOptionKind) -> Result<Vec<SettingsSelectOption>> {
        let kind = match kind {
            SettingsOptionKind::FfmpegProfile => {
                crate::admin_settings::SettingsOptionKind::FfmpegProfile
            }
            SettingsOptionKind::Watermark => crate::admin_settings::SettingsOptionKind::Watermark,
            SettingsOptionKind::FallbackFiller => {
                crate::admin_settings::SettingsOptionKind::FallbackFiller
            }
            SettingsOptionKind::CustomResolution => {
                crate::admin_settings::SettingsOptionKind::CustomResolution
            }
        };
        Ok(crate::admin_settings::load_select_options(&self.0, kind)
            .await?
            .into_iter()
            .map(|option| SettingsSelectOption {
                id: option.id,
                name: option.name,
            })
            .collect())
    }

    async fn list_playout_ids(&self) -> Result<Vec<EntityId>> {
        crate::admin_settings::load_playout_ids(&self.0).await
    }

    async fn create_custom_resolution(&self, width: i32, height: i32) -> Result<()> {
        crate::admin_settings::create_custom_resolution(&self.0, width, height).await
    }

    async fn delete_custom_resolution(&self, id: EntityId) -> Result<()> {
        crate::admin_settings::delete_custom_resolution(&self.0, id).await
    }
}
