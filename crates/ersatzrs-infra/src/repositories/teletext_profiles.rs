//! Atomic ADR 0122 configuration foundation; no package or page activation.

use async_trait::async_trait;
use ersatzrs_core::repositories::{
    ChannelTeletextBindingSave, RepositoryError, Result, TeletextProfileMutation as M,
    TeletextProfileRejection as R, TeletextProfileRepository, TeletextProfileSave,
    TeletextProfileSaveResult,
};
use ersatzrs_domain::{
    validate_teletext_profile_values, ChannelTeletextBinding, ChannelTeletextBindingRead, EntityId,
    TeletextInstallationRead, TeletextInstallationSnapshot, TeletextProfile,
    TeletextProfileAvailability as A, TeletextProfileRead, TeletextProfileSetting,
    TeletextProfileValue, TeletextSecretReferenceKind, TeletextSelectionSnapshot,
};
use sqlx::{Row as _, Transaction};

use super::SqlxResultExt as _;
use crate::{
    addons::teletext_profile_settings::{configuration, configuration_with_settings},
    db::{Db, DbPool, DbRow},
};

/// Transactional named profile/base binding adapter. No runtime readiness claim.
#[derive(Clone)]
pub struct SqliteTeletextProfileRepository(pub DbPool);

#[async_trait]
impl TeletextProfileRepository for SqliteTeletextProfileRepository {
    async fn installation_snapshot(&self, addon_id: &str) -> Result<TeletextInstallationRead> {
        // Read-only transaction: observe installation and dependencies consistently.
        let mut tx = self.0.begin().await.repository()?;
        configuration(&mut tx, addon_id, None).await
    }

    async fn list_profiles(&self, addon_id: &str) -> Result<Vec<TeletextProfileRead>> {
        // Read-only transaction: profiles and effective settings share one snapshot.
        let mut tx = self.0.begin().await.repository()?;
        let rows = sqlx::query(
            r#"SELECT * FROM "TeletextProfile" WHERE "AddonId" = ? ORDER BY "Name", "Id""#,
        )
        .bind(addon_id)
        .fetch_all(&mut *tx)
        .await
        .repository()?;
        let mut profiles = Vec::with_capacity(rows.len());
        for row in rows {
            let profile = profile_from_row(&mut tx, &row).await?;
            let availability = profile_availability(&mut tx, &profile).await?;
            profiles.push(TeletextProfileRead {
                profile,
                availability,
            });
        }
        Ok(profiles)
    }

    async fn channel_bindings(
        &self,
        channel_id: EntityId,
    ) -> Result<Vec<ChannelTeletextBindingRead>> {
        // Read-only transaction: retained bindings and profile validity are coherent.
        let mut tx = self.0.begin().await.repository()?;
        let rows = sqlx::query(
            r#"SELECT * FROM "ChannelTeletextBinding" WHERE "ChannelId" = ? ORDER BY "AddonId""#,
        )
        .bind(channel_id)
        .fetch_all(&mut *tx)
        .await
        .repository()?;
        let mut bindings = Vec::with_capacity(rows.len());
        for row in rows {
            let binding = binding_from_row(&row)?;
            let profile = load_profile(&mut tx, binding.profile_id)
                .await?
                .ok_or_else(|| decode_error("missing selected profile"))?;
            let availability = profile_availability(&mut tx, &profile).await?;
            bindings.push(ChannelTeletextBindingRead {
                binding,
                availability,
            });
        }
        Ok(bindings)
    }

    async fn invocation_snapshot(
        &self,
        channel_id: EntityId,
        addon_id: &str,
        expected_binding_revision: i64,
    ) -> Result<M<TeletextSelectionSnapshot>> {
        // Read-only transaction: observe one coherent binding, profile and settings snapshot.
        let mut tx = self.0.begin().await.repository()?;
        if !channel_exists(&mut tx, channel_id).await? {
            return Ok(M::Rejected(R::MissingChannel));
        }
        let Some(binding) = load_binding(&mut tx, channel_id, addon_id).await? else {
            return Ok(M::Rejected(R::MissingBinding));
        };
        if binding.revision != expected_binding_revision {
            return Ok(M::Rejected(R::Conflict));
        }
        if !binding.enabled {
            return Ok(M::Rejected(R::DisabledBinding));
        }
        let Some(profile) = load_profile(&mut tx, binding.profile_id).await? else {
            return Ok(M::Rejected(R::MissingProfile));
        };
        if profile.addon_id != addon_id {
            return Ok(M::Rejected(R::WrongAddon));
        }
        if profile.deleted {
            return Ok(M::Rejected(R::DeletedProfile));
        }
        if validate_teletext_profile_values(&profile.name, &profile.settings).is_err() {
            return Ok(M::Rejected(R::InvalidSettings));
        }
        let configuration =
            configuration_with_settings(&mut tx, addon_id, Some(&profile.settings)).await?;
        match configuration.installation.availability {
            A::Configured => {}
            A::InvalidSettings => return Ok(M::Rejected(R::InvalidSettings)),
            _ => return Ok(M::Rejected(R::InstallationUnavailable)),
        }
        if configuration.installation.snapshot.as_ref() != Some(&profile.validated_installation) {
            return Ok(M::Rejected(R::Conflict));
        }
        let effective_settings = configuration
            .effective_settings
            .ok_or_else(|| decode_error("effective settings missing"))?;
        Ok(M::Saved(TeletextSelectionSnapshot {
            channel_id,
            addon_id: addon_id.to_owned(),
            binding_revision: binding.revision,
            profile_id: profile.id,
            profile_revision: profile.revision,
            installation: profile.validated_installation,
            effective_settings,
        }))
    }

    async fn save_profile(
        &self,
        save: &TeletextProfileSave,
    ) -> Result<M<TeletextProfileSaveResult>> {
        if validate_teletext_profile_values(&save.name, &save.settings).is_err() {
            return Ok(M::Rejected(R::InvalidValues));
        }
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        if let Some(rejection) =
            installation_rejection(&mut tx, &save.addon_id, &save.expected_installation).await?
        {
            return Ok(M::Rejected(rejection));
        }
        if let Some((id, revision)) = save.existing {
            if let Some(rejection) =
                profile_rejection(&mut tx, &save.addon_id, id, revision).await?
            {
                return Ok(M::Rejected(rejection));
            }
            if revision == i64::MAX {
                return Ok(M::Rejected(R::RevisionLimit));
            }
        }
        let conflict: bool = sqlx::query_scalar(
            r#"SELECT EXISTS(SELECT 1 FROM "TeletextProfile"
            WHERE "AddonId" = ? AND "Name" = ? AND "IsDeleted" = 0 AND "Id" != ?)"#,
        )
        .bind(&save.addon_id)
        .bind(&save.name)
        .bind(save.existing.map_or(0, |(id, _)| id))
        .fetch_one(&mut *tx)
        .await
        .repository()?;
        if conflict {
            return Ok(M::Rejected(R::NameConflict));
        }
        if configuration(&mut tx, &save.addon_id, Some(&save.settings))
            .await?
            .availability
            != A::Configured
        {
            return Ok(M::Rejected(R::InvalidSettings));
        }
        if let Some((id, _)) = save.existing {
            if dependent_revision_limit(&mut tx, id).await? {
                return Ok(M::Rejected(R::RevisionLimit));
            }
        }
        let id = write_profile(&mut tx, save).await?;
        replace_settings(&mut tx, id, &save.settings).await?;
        let affected = invalidate_bindings(&mut tx, id).await?;
        let profile = load_profile(&mut tx, id)
            .await?
            .ok_or_else(|| decode_error("saved profile missing"))?;
        tx.commit().await.repository()?;
        Ok(M::Saved(TeletextProfileSaveResult {
            profile,
            dependent_bindings_changed: affected,
        }))
    }

    async fn save_binding(
        &self,
        save: &ChannelTeletextBindingSave,
    ) -> Result<M<ChannelTeletextBinding>> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        if !channel_exists(&mut tx, save.channel_id).await? {
            return Ok(M::Rejected(R::MissingChannel));
        }
        if let Some(rejection) =
            installation_rejection(&mut tx, &save.addon_id, &save.expected_installation).await?
        {
            return Ok(M::Rejected(rejection));
        }
        if let Some(rejection) = profile_rejection(
            &mut tx,
            &save.addon_id,
            save.profile_id,
            save.expected_profile_revision,
        )
        .await?
        {
            return Ok(M::Rejected(rejection));
        }
        let profile = load_profile(&mut tx, save.profile_id)
            .await?
            .ok_or_else(|| decode_error("selected profile missing"))?;
        if profile_availability(&mut tx, &profile).await? != A::Configured {
            return Ok(M::Rejected(R::InvalidSettings));
        }
        let current = load_binding(&mut tx, save.channel_id, &save.addon_id).await?;
        if current.as_ref().map(|binding| binding.revision) != save.expected_binding_revision {
            return Ok(M::Rejected(R::Conflict));
        }
        if current
            .as_ref()
            .is_some_and(|binding| binding.revision == i64::MAX)
        {
            return Ok(M::Rejected(R::RevisionLimit));
        }
        sqlx::query(r#"INSERT INTO "ChannelTeletextBinding" ("ChannelId", "AddonId", "ProfileId", "Enabled", "Revision")
            VALUES (?, ?, ?, ?, 1) ON CONFLICT("ChannelId", "AddonId") DO UPDATE
            SET "ProfileId" = excluded."ProfileId", "Enabled" = excluded."Enabled", "Revision" = "Revision" + 1"#)
            .bind(save.channel_id).bind(&save.addon_id).bind(save.profile_id).bind(save.enabled)
            .execute(&mut *tx).await.repository()?;
        let binding = load_binding(&mut tx, save.channel_id, &save.addon_id)
            .await?
            .ok_or_else(|| decode_error("saved binding missing"))?;
        tx.commit().await.repository()?;
        Ok(M::Saved(binding))
    }

    async fn disable_binding(
        &self,
        channel_id: EntityId,
        addon_id: &str,
        expected_binding_revision: i64,
    ) -> Result<M<ChannelTeletextBinding>> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        if !channel_exists(&mut tx, channel_id).await? {
            return Ok(M::Rejected(R::MissingChannel));
        }
        let Some(binding) = load_binding(&mut tx, channel_id, addon_id).await? else {
            return Ok(M::Rejected(R::MissingBinding));
        };
        if binding.revision != expected_binding_revision {
            return Ok(M::Rejected(R::Conflict));
        }
        if binding.revision == i64::MAX {
            return Ok(M::Rejected(R::RevisionLimit));
        }
        sqlx::query(
            r#"UPDATE "ChannelTeletextBinding" SET "Enabled" = 0, "Revision" = "Revision" + 1
            WHERE "ChannelId" = ? AND "AddonId" = ? AND "Revision" = ?"#,
        )
        .bind(channel_id)
        .bind(addon_id)
        .bind(expected_binding_revision)
        .execute(&mut *tx)
        .await
        .repository()?;
        let updated = ChannelTeletextBinding {
            enabled: false,
            revision: binding.revision + 1,
            ..binding
        };
        tx.commit().await.repository()?;
        Ok(M::Saved(updated))
    }

    async fn tombstone_profile(
        &self,
        addon_id: &str,
        profile_id: EntityId,
        expected_revision: i64,
    ) -> Result<M<TeletextProfileSaveResult>> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        if let Some(rejection) =
            profile_rejection(&mut tx, addon_id, profile_id, expected_revision).await?
        {
            return Ok(M::Rejected(rejection));
        }
        if expected_revision == i64::MAX {
            return Ok(M::Rejected(R::RevisionLimit));
        }
        if dependent_revision_limit(&mut tx, profile_id).await? {
            return Ok(M::Rejected(R::RevisionLimit));
        }
        sqlx::query(r#"UPDATE "TeletextProfile" SET "IsDeleted" = 1, "Revision" = "Revision" + 1 WHERE "Id" = ?"#)
            .bind(profile_id).execute(&mut *tx).await.repository()?;
        let affected = invalidate_bindings(&mut tx, profile_id).await?;
        let profile = load_profile(&mut tx, profile_id)
            .await?
            .ok_or_else(|| decode_error("tombstoned profile missing"))?;
        tx.commit().await.repository()?;
        Ok(M::Saved(TeletextProfileSaveResult {
            profile,
            dependent_bindings_changed: affected,
        }))
    }
}

async fn installation_rejection(
    tx: &mut Transaction<'_, Db>,
    addon_id: &str,
    expected: &TeletextInstallationSnapshot,
) -> Result<Option<R>> {
    if expected.addon_id != addon_id {
        return Ok(Some(R::WrongAddon));
    }
    let current = configuration(tx, addon_id, None).await?;
    if current.availability != A::Configured {
        return Ok(Some(R::InstallationUnavailable));
    }
    Ok((current.snapshot.as_ref() != Some(expected)).then_some(R::Conflict))
}

async fn profile_rejection(
    tx: &mut Transaction<'_, Db>,
    addon_id: &str,
    id: EntityId,
    expected_revision: i64,
) -> Result<Option<R>> {
    let Some(profile) = load_profile(tx, id).await? else {
        return Ok(Some(R::MissingProfile));
    };
    Ok(if profile.addon_id != addon_id {
        Some(R::WrongAddon)
    } else if profile.deleted {
        Some(R::DeletedProfile)
    } else if profile.revision != expected_revision {
        Some(R::Conflict)
    } else {
        None
    })
}

async fn profile_availability(
    tx: &mut Transaction<'_, Db>,
    profile: &TeletextProfile,
) -> Result<A> {
    if profile.deleted {
        return Ok(A::DeletedProfile);
    }
    if validate_teletext_profile_values(&profile.name, &profile.settings).is_err() {
        return Ok(A::InvalidSettings);
    }
    let current = configuration(tx, &profile.addon_id, Some(&profile.settings)).await?;
    if current.availability != A::Configured {
        return Ok(current.availability);
    }
    Ok(
        if current.snapshot.as_ref() == Some(&profile.validated_installation) {
            A::Configured
        } else {
            A::StaleInstallation
        },
    )
}

async fn channel_exists(tx: &mut Transaction<'_, Db>, id: EntityId) -> Result<bool> {
    sqlx::query_scalar(r#"SELECT EXISTS(SELECT 1 FROM "Channel" WHERE "Id" = ?)"#)
        .bind(id)
        .fetch_one(&mut **tx)
        .await
        .repository()
}

async fn dependent_revision_limit(tx: &mut Transaction<'_, Db>, id: EntityId) -> Result<bool> {
    sqlx::query_scalar(r#"SELECT EXISTS(SELECT 1 FROM "ChannelTeletextBinding" WHERE "ProfileId" = ? AND "Revision" = ?)"#)
        .bind(id).bind(i64::MAX).fetch_one(&mut **tx).await.repository()
}

async fn invalidate_bindings(tx: &mut Transaction<'_, Db>, id: EntityId) -> Result<u64> {
    Ok(sqlx::query(
        r#"UPDATE "ChannelTeletextBinding" SET "Revision" = "Revision" + 1 WHERE "ProfileId" = ?"#,
    )
    .bind(id)
    .execute(&mut **tx)
    .await
    .repository()?
    .rows_affected())
}

async fn write_profile(
    tx: &mut Transaction<'_, Db>,
    save: &TeletextProfileSave,
) -> Result<EntityId> {
    let token = &save.expected_installation;
    if let Some((id, _)) = save.existing {
        sqlx::query(r#"UPDATE "TeletextProfile" SET "Name" = ?, "Revision" = "Revision" + 1,
            "InstallationStateRevision" = ?, "InstallationVersion" = ?, "InstallationPackageSha256" = ?,
            "InstallationContentSha256" = ?, "InstallationManifestSha256" = ? WHERE "Id" = ?"#)
            .bind(&save.name).bind(&token.state_revision).bind(&token.version).bind(&token.package_sha256)
            .bind(&token.content_sha256).bind(&token.manifest_sha256).bind(id).execute(&mut **tx).await.repository()?;
        Ok(id)
    } else {
        let result = sqlx::query(r#"INSERT INTO "TeletextProfile" ("AddonId", "Name", "InstallationStateRevision",
            "InstallationVersion", "InstallationPackageSha256", "InstallationContentSha256", "InstallationManifestSha256")
            VALUES (?, ?, ?, ?, ?, ?, ?)"#)
            .bind(&save.addon_id).bind(&save.name).bind(&token.state_revision).bind(&token.version)
            .bind(&token.package_sha256).bind(&token.content_sha256).bind(&token.manifest_sha256)
            .execute(&mut **tx).await.repository()?;
        Ok(result.last_insert_rowid())
    }
}

async fn replace_settings(
    tx: &mut Transaction<'_, Db>,
    id: EntityId,
    settings: &[TeletextProfileSetting],
) -> Result<()> {
    sqlx::query(r#"DELETE FROM "TeletextProfileSetting" WHERE "ProfileId" = ?"#)
        .bind(id)
        .execute(&mut **tx)
        .await
        .repository()?;
    for setting in settings {
        let (kind, value, reference) = match &setting.value {
            TeletextProfileValue::Literal(value) => (0, Some(value.as_str()), None),
            TeletextProfileValue::SecretReference { kind, reference } => (
                match kind {
                    TeletextSecretReferenceKind::Environment => 1,
                    TeletextSecretReferenceKind::File => 2,
                },
                None,
                Some(reference.as_str()),
            ),
        };
        sqlx::query(r#"INSERT INTO "TeletextProfileSetting" ("ProfileId", "Key", "Kind", "Value", "Reference") VALUES (?, ?, ?, ?, ?)"#)
            .bind(id).bind(&setting.key).bind(kind).bind(value).bind(reference).execute(&mut **tx).await.repository()?;
    }
    Ok(())
}

async fn load_profile(
    tx: &mut Transaction<'_, Db>,
    id: EntityId,
) -> Result<Option<TeletextProfile>> {
    let row = sqlx::query(r#"SELECT * FROM "TeletextProfile" WHERE "Id" = ?"#)
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .repository()?;
    match row {
        Some(row) => Ok(Some(profile_from_row(tx, &row).await?)),
        None => Ok(None),
    }
}

async fn profile_from_row(tx: &mut Transaction<'_, Db>, row: &DbRow) -> Result<TeletextProfile> {
    let id = row.try_get("Id").repository()?;
    let addon_id: String = row.try_get("AddonId").repository()?;
    let settings = sqlx::query(r#"SELECT "Key", "Kind", "Value", "Reference" FROM "TeletextProfileSetting" WHERE "ProfileId" = ? ORDER BY "Key""#)
        .bind(id).fetch_all(&mut **tx).await.repository()?.iter().map(setting_from_row).collect::<Result<_>>()?;
    Ok(TeletextProfile {
        id,
        addon_id: addon_id.clone(),
        name: row.try_get("Name").repository()?,
        revision: row.try_get("Revision").repository()?,
        deleted: row.try_get("IsDeleted").repository()?,
        settings,
        validated_installation: TeletextInstallationSnapshot {
            addon_id,
            state_revision: row.try_get("InstallationStateRevision").repository()?,
            version: row.try_get("InstallationVersion").repository()?,
            package_sha256: row.try_get("InstallationPackageSha256").repository()?,
            content_sha256: row.try_get("InstallationContentSha256").repository()?,
            manifest_sha256: row.try_get("InstallationManifestSha256").repository()?,
        },
    })
}

fn setting_from_row(row: &DbRow) -> Result<TeletextProfileSetting> {
    let kind: i64 = row.try_get("Kind").repository()?;
    let value = match kind {
        0 => TeletextProfileValue::Literal(row.try_get("Value").repository()?),
        1 | 2 => TeletextProfileValue::SecretReference {
            kind: if kind == 1 {
                TeletextSecretReferenceKind::Environment
            } else {
                TeletextSecretReferenceKind::File
            },
            reference: row.try_get("Reference").repository()?,
        },
        _ => return Err(decode_error("invalid setting kind")),
    };
    Ok(TeletextProfileSetting {
        key: row.try_get("Key").repository()?,
        value,
    })
}

fn binding_from_row(row: &DbRow) -> Result<ChannelTeletextBinding> {
    Ok(ChannelTeletextBinding {
        channel_id: row.try_get("ChannelId").repository()?,
        addon_id: row.try_get("AddonId").repository()?,
        profile_id: row.try_get("ProfileId").repository()?,
        enabled: row.try_get("Enabled").repository()?,
        revision: row.try_get("Revision").repository()?,
    })
}

async fn load_binding(
    tx: &mut Transaction<'_, Db>,
    channel_id: EntityId,
    addon_id: &str,
) -> Result<Option<ChannelTeletextBinding>> {
    sqlx::query(r#"SELECT * FROM "ChannelTeletextBinding" WHERE "ChannelId" = ? AND "AddonId" = ?"#)
        .bind(channel_id)
        .bind(addon_id)
        .fetch_optional(&mut **tx)
        .await
        .repository()?
        .as_ref()
        .map(binding_from_row)
        .transpose()
}

fn decode_error(value: &'static str) -> RepositoryError {
    RepositoryError::Decode {
        kind: "teletext profile configuration",
        value: value.to_owned(),
    }
}

#[cfg(test)]
#[path = "teletext_profiles_tests.rs"]
mod tests;
