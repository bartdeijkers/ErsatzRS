//! Scoped unresolved settings checks. No global writes or process/tool resolution.

use std::collections::{BTreeMap, BTreeSet};

use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_domain::{
    validate_teletext_profile_values, TeletextInstallationRead, TeletextInstallationSnapshot,
    TeletextPageContract, TeletextProfileAvailability as A, TeletextProfileSetting,
    TeletextProfileValue, TeletextSecretReferenceKind, MAX_TELETEXT_PROFILE_KEY_BYTES,
    MAX_TELETEXT_PROFILE_SETTINGS, MAX_TELETEXT_PROFILE_SETTINGS_BYTES,
    MAX_TELETEXT_PROFILE_VALUE_BYTES,
};

use super::{
    AddonError, AddonManifest, AddonSecretReferenceKind, AddonSettingValue, InstalledAddon,
};
use crate::db::{Db, DbRow};
use crate::repositories::SqlxResultExt as _;
use sha2::{Digest as _, Sha256};
use sqlx::{Row as _, Transaction};

/// Check current installed configuration within the caller's one transaction.
/// `None` observes installation identity only; `Some` validates an effective bag.
pub(crate) async fn configuration(
    tx: &mut Transaction<'_, Db>,
    addon_id: &str,
    overrides: Option<&[TeletextProfileSetting]>,
) -> Result<TeletextInstallationRead> {
    Ok(configuration_with_settings(tx, addon_id, overrides)
        .await?
        .installation)
}

pub(crate) struct ScopedConfiguration {
    pub installation: TeletextInstallationRead,
    pub effective_settings: Option<Vec<TeletextProfileSetting>>,
    pub contract: Option<TeletextPageContract>,
}

/// Prefer the highest host-supported contract declared by this exact manifest.
pub(crate) fn preferred_contract(manifest: &AddonManifest) -> Option<TeletextPageContract> {
    [
        TeletextPageContract::V3,
        TeletextPageContract::V2,
        TeletextPageContract::V1,
    ]
    .into_iter()
    .find(|contract| manifest.has_capability(contract.capability()))
}

/// Resolve the same bounded values used by writes and invocation admission.
pub(crate) async fn configuration_with_settings(
    tx: &mut Transaction<'_, Db>,
    addon_id: &str,
    overrides: Option<&[TeletextProfileSetting]>,
) -> Result<ScopedConfiguration> {
    let installation = match super::load_installed_addon_in_transaction(tx, addon_id).await {
        Ok(Some(installation)) => installation,
        Ok(None) => return Ok(state(None, A::MissingInstallation)),
        Err(AddonError::Database(_)) => return Err(storage_error()),
        Err(_) => return Ok(state(None, A::IncompatibleInstallation)),
    };
    let snapshot = installation_identity(tx, &installation).await?;
    if !installation.enabled {
        return Ok(state(Some(snapshot), A::DisabledInstallation));
    }
    let contract = preferred_contract(&installation.manifest);
    if contract.is_none()
        || !installation
            .manifest
            .host_version
            .matches(&super::host_version().map_err(|_| storage_error())?)
        || !dependencies_available(tx, &installation.manifest).await?
    {
        return Ok(state(Some(snapshot), A::IncompatibleInstallation));
    }
    let mut result = state(Some(snapshot), A::Configured);
    result.contract = contract;
    if let Some(overrides) = overrides {
        let globals = load_bounded_globals(tx, addon_id).await?;
        result.effective_settings = globals.and_then(|globals| {
            resolve_effective_settings(&installation.manifest, &globals, overrides).ok()
        });
        if result.effective_settings.is_none() {
            result.installation.availability = A::InvalidSettings;
        }
    }
    Ok(result)
}

/// Fingerprint the raw stored manifest associated with this loaded object in
/// the SAME transaction. Never mix a later manifest with earlier settings.
pub(crate) async fn installation_identity(
    tx: &mut Transaction<'_, Db>,
    installation: &InstalledAddon,
) -> Result<TeletextInstallationSnapshot> {
    let manifest: String =
        sqlx::query_scalar(r#"SELECT "ManifestJson" FROM "AddonInstallation" WHERE "AddonId" = ?"#)
            .bind(&installation.addon_id)
            .fetch_one(&mut **tx)
            .await
            .repository()?;
    Ok(TeletextInstallationSnapshot {
        addon_id: installation.addon_id.clone(),
        state_revision: installation.state_revision.clone(),
        version: installation.version.to_string(),
        package_sha256: installation.package_sha256.clone(),
        content_sha256: installation.content_sha256.clone(),
        manifest_sha256: super::hex_digest(Sha256::digest(manifest.as_bytes())),
    })
}

async fn dependencies_available(
    tx: &mut Transaction<'_, Db>,
    manifest: &AddonManifest,
) -> Result<bool> {
    let mut dependencies = Vec::new();
    for dependency in &manifest.dependencies {
        match super::load_installed_addon_in_transaction(tx, &dependency.id).await {
            Ok(Some(installation)) => dependencies.push(installation),
            Err(AddonError::Database(_)) => return Err(storage_error()),
            Err(_) if !dependency.optional => return Ok(false),
            Ok(None) | Err(_) => {}
        }
    }
    Ok(super::validate_dependencies_from_installations(&dependencies, manifest).is_ok())
}

fn storage_error() -> RepositoryError {
    RepositoryError::unavailable(
        "teletext profile configuration",
        "installed configuration unavailable",
    )
}

fn state(snapshot: Option<TeletextInstallationSnapshot>, availability: A) -> ScopedConfiguration {
    ScopedConfiguration {
        installation: TeletextInstallationRead {
            snapshot,
            availability,
        },
        effective_settings: None,
        contract: None,
    }
}

async fn load_bounded_globals(
    tx: &mut Transaction<'_, Db>,
    addon_id: &str,
) -> Result<Option<Vec<AddonSettingValue>>> {
    let budget = sqlx::query(
        r#"SELECT COUNT(*) AS "Count", COALESCE(SUM(
            LENGTH(CAST("Key" AS BLOB)) + COALESCE(LENGTH(CAST("Value" AS BLOB)), 0)
            + COALESCE(LENGTH(CAST("Reference" AS BLOB)), 0)), 0) AS "Bytes"
            FROM "AddonSetting" WHERE "AddonId" = ?"#,
    )
    .bind(addon_id)
    .fetch_one(&mut **tx)
    .await
    .repository()?;
    if !within_budget(&budget)? {
        return Ok(None);
    }
    let rows = sqlx::query(
        r#"SELECT "Key", "Value", "ReferenceKind", "Reference"
            FROM "AddonSetting" WHERE "AddonId" = ? ORDER BY "Key""#,
    )
    .bind(addon_id)
    .fetch_all(&mut **tx)
    .await
    .repository()?;
    Ok(rows
        .iter()
        .map(super::setting_from_row)
        .collect::<std::result::Result<_, _>>()
        .ok())
}

fn within_budget(row: &DbRow) -> Result<bool> {
    let count: i64 = row.try_get("Count").repository()?;
    let bytes: i64 = row.try_get("Bytes").repository()?;
    Ok(
        usize::try_from(count).is_ok_and(|count| count <= MAX_TELETEXT_PROFILE_SETTINGS)
            && usize::try_from(bytes)
                .is_ok_and(|bytes| bytes <= MAX_TELETEXT_PROFILE_SETTINGS_BYTES),
    )
}

fn resolve_effective_settings(
    manifest: &AddonManifest,
    globals: &[AddonSettingValue],
    overrides: &[TeletextProfileSetting],
) -> std::result::Result<Vec<TeletextProfileSetting>, ()> {
    if manifest.settings.len() > MAX_TELETEXT_PROFILE_SETTINGS {
        return Err(());
    }
    validate_teletext_profile_values("Profile", overrides).map_err(|_| ())?;
    if globals
        .iter()
        .any(|setting| setting_bytes(setting).is_none())
    {
        return Err(());
    }
    let known: BTreeSet<_> = manifest
        .settings
        .iter()
        .map(|definition| definition.key.as_str())
        .collect();
    let mut values: BTreeMap<_, _> = globals
        .iter()
        .map(|value| (value.key.as_str(), value.clone()))
        .collect();
    if globals.len() != values.len() || values.keys().any(|key| !known.contains(key)) {
        return Err(());
    }
    for setting in overrides {
        if !known.contains(setting.key.as_str()) {
            return Err(());
        }
        values.insert(&setting.key, to_addon_setting(setting));
    }
    let mut effective = Vec::new();
    let mut effective_bytes = 0_usize;
    for definition in &manifest.settings {
        if definition
            .default
            .as_ref()
            .is_some_and(|value| value.len() > MAX_TELETEXT_PROFILE_VALUE_BYTES)
        {
            return Err(());
        }
        // Legacy ordinary NULL rows are an unset value, matching existing
        // environment projection. Explicit empty strings remain supplied.
        let setting = values
            .get(definition.key.as_str())
            .filter(|setting| {
                definition.kind == super::AddonSettingKind::SecretReference
                    || setting.value.is_some()
                    || setting.reference_kind.is_some()
                    || setting.reference.is_some()
            })
            .cloned()
            .or_else(|| {
                definition.default.as_ref().map(|value| AddonSettingValue {
                    key: definition.key.clone(),
                    value: Some(value.clone()),
                    reference_kind: None,
                    reference: None,
                })
            });
        if let Some(setting) = setting {
            effective_bytes = effective_bytes.saturating_add(setting_bytes(&setting).ok_or(())?);
            if effective_bytes > MAX_TELETEXT_PROFILE_SETTINGS_BYTES {
                return Err(());
            }
            super::validate_setting_value(definition, &setting).map_err(|_| ())?;
            super::validate_storage_path_override(manifest, &setting).map_err(|_| ())?;
            if definition.required && setting.value.as_deref().is_some_and(str::is_empty) {
                return Err(());
            }
            effective.push(to_neutral_setting(&setting)?);
        } else if definition.required {
            return Err(());
        }
    }
    validate_teletext_profile_values("Profile", &effective).map_err(|_| ())?;
    Ok(effective)
}

fn setting_bytes(setting: &AddonSettingValue) -> Option<usize> {
    let value = setting.value.as_deref().unwrap_or_default();
    let reference = setting.reference.as_deref().unwrap_or_default();
    (setting.key.len() <= MAX_TELETEXT_PROFILE_KEY_BYTES
        && value.len() <= MAX_TELETEXT_PROFILE_VALUE_BYTES
        && reference.len() <= MAX_TELETEXT_PROFILE_VALUE_BYTES
        && !value.contains('\0')
        && !reference.contains('\0'))
    .then_some(setting.key.len() + value.len() + reference.len())
}

pub(crate) fn to_addon_setting(setting: &TeletextProfileSetting) -> AddonSettingValue {
    let (value, reference_kind, reference) = match &setting.value {
        TeletextProfileValue::Literal(value) => (Some(value.clone()), None, None),
        TeletextProfileValue::SecretReference { kind, reference } => (
            None,
            Some(match kind {
                TeletextSecretReferenceKind::Environment => AddonSecretReferenceKind::Environment,
                TeletextSecretReferenceKind::File => AddonSecretReferenceKind::File,
            }),
            Some(reference.clone()),
        ),
    };
    AddonSettingValue {
        key: setting.key.clone(),
        value,
        reference_kind,
        reference,
    }
}

fn to_neutral_setting(
    setting: &AddonSettingValue,
) -> std::result::Result<TeletextProfileSetting, ()> {
    let value = match (&setting.value, setting.reference_kind, &setting.reference) {
        (Some(value), None, None) => TeletextProfileValue::Literal(value.clone()),
        (None, Some(kind), Some(reference)) => TeletextProfileValue::SecretReference {
            kind: match kind {
                AddonSecretReferenceKind::Environment => TeletextSecretReferenceKind::Environment,
                AddonSecretReferenceKind::File => TeletextSecretReferenceKind::File,
            },
            reference: reference.clone(),
        },
        _ => return Err(()),
    };
    Ok(TeletextProfileSetting {
        key: setting.key.clone(),
        value,
    })
}
