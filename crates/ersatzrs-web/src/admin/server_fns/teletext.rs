//! Channel-scoped configuration reads; no preparation, activation or publication.

use ersatzrs_addon_contract::AddonSettingDefinition;
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelTeletextData {
    pub channel_id: i64,
    pub channel_number: String,
    pub channel_name: String,
    pub catalog_available: bool,
    /// Opaque full-membership observation; never unresolved settings or secrets.
    pub authoring_selection: Option<String>,
    pub authoring_error: Option<String>,
    pub addons: Vec<TeletextAddonData>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeletextAddonData {
    pub addon_id: String,
    pub name: String,
    pub name_labels: std::collections::BTreeMap<String, String>,
    pub installed: bool,
    pub availability: String,
    pub installation: Option<TeletextInstallationData>,
    pub settings: Vec<TeletextSettingData>,
    pub profiles: Vec<TeletextProfileData>,
    pub binding: Option<TeletextBindingData>,
}

/// Exact rendered CAS observation, never replaced with a later write-time read.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeletextInstallationData {
    pub addon_id: String,
    pub state_revision: String,
    pub version: String,
    pub package_sha256: String,
    pub content_sha256: String,
    pub manifest_sha256: String,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeletextSettingData {
    pub definition: AddonSettingDefinition,
    pub inherited: Option<TeletextSettingValueData>,
}

/// Unresolved configuration only; deliberately carries no Debug implementation.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeletextSettingValueData {
    pub key: String,
    pub mode: String,
    pub value: String,
    pub reference_kind: String,
    pub reference: String,
    pub redacted: bool,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeletextProfileData {
    pub id: i64,
    pub name: String,
    pub revision: i64,
    pub deleted: bool,
    pub availability: String,
    pub settings: Vec<TeletextSettingValueData>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeletextBindingData {
    pub profile_id: i64,
    pub enabled: bool,
    pub revision: i64,
    pub availability: String,
}

#[server(prefix = "/admin-api")]
pub async fn load_channel_teletext(
    channel_id: i64,
) -> Result<Option<ChannelTeletextData>, ServerFnError> {
    let state = expect_context::<crate::host::state::AppState>();
    load_channel_teletext_for_application(&state.application, channel_id).await
}

#[cfg(feature = "ssr")]
impl From<ersatzrs_domain::TeletextInstallationSnapshot> for TeletextInstallationData {
    fn from(value: ersatzrs_domain::TeletextInstallationSnapshot) -> Self {
        Self {
            addon_id: value.addon_id,
            state_revision: value.state_revision,
            version: value.version,
            package_sha256: value.package_sha256,
            content_sha256: value.content_sha256,
            manifest_sha256: value.manifest_sha256,
        }
    }
}

#[cfg(feature = "ssr")]
impl From<TeletextInstallationData> for ersatzrs_domain::TeletextInstallationSnapshot {
    fn from(value: TeletextInstallationData) -> Self {
        Self {
            addon_id: value.addon_id,
            state_revision: value.state_revision,
            version: value.version,
            package_sha256: value.package_sha256,
            content_sha256: value.content_sha256,
            manifest_sha256: value.manifest_sha256,
        }
    }
}

/// Project channel configuration through the same application boundary used by
/// SSR and native controls, without invoking packages or resolving secrets.
#[cfg(feature = "ssr")]
pub async fn load_channel_teletext_for_application(
    application: &ersatzrs_app::ApplicationServices,
    channel_id: i64,
) -> Result<Option<ChannelTeletextData>, ServerFnError> {
    use std::collections::{BTreeMap, BTreeSet};
    if channel_id <= 0 {
        return Ok(None);
    }
    let Some(channel) = application
        .get_channel_editor(ersatzrs_app::queries::GetChannelEditor { channel_id })
        .await
        .map_err(|_| ServerFnError::new("unavailable"))?
    else {
        return Ok(None);
    };
    let bindings = application
        .channel_teletext_bindings(channel_id)
        .await
        .map_err(|_| ServerFnError::new("unavailable"))?;
    // A malformed/unreadable catalog must not hide retained selections' Off form.
    let (installed, catalog_available) = match application.addon_overview().await {
        Ok(overview) => (overview.installed, true),
        Err(_) => (Vec::new(), false),
    };
    let mut installed: BTreeMap<_, _> = installed
        .into_iter()
        .filter(|addon| {
            addon.manifest.has_capability("teletext.pages.v1")
                || addon.manifest.has_capability("teletext.pages.v2")
        })
        .map(|addon| (addon.addon_id.clone(), addon))
        .collect();
    let ids: BTreeSet<_> = installed
        .keys()
        .cloned()
        .chain(bindings.iter().map(|read| read.binding.addon_id.clone()))
        .collect();
    let mut addons = Vec::with_capacity(ids.len());
    for addon_id in ids {
        let binding = bindings
            .iter()
            .find(|read| read.binding.addon_id == addon_id);
        addons
            .push(load_addon(application, &addon_id, installed.remove(&addon_id), binding).await?);
    }
    let (authoring_selection, authoring_error) =
        match authoring_selection(application, channel_id).await {
            Ok(selection) => (Some(selection), None),
            Err(code) => (None, Some(code.to_owned())),
        };
    Ok(Some(ChannelTeletextData {
        channel_id,
        channel_number: channel.form.number,
        channel_name: channel.form.name,
        catalog_available,
        authoring_selection,
        authoring_error,
        addons,
    }))
}

#[cfg(feature = "ssr")]
async fn authoring_selection(
    application: &ersatzrs_app::ApplicationServices,
    channel_id: i64,
) -> Result<String, &'static str> {
    let deadline = std::time::Instant::now()
        .checked_add(ersatzrs_app::teletext_channel_documents::PREPARATION_TIMEOUT)
        .ok_or("expired")?;
    crate::host::teletext_authoring::observe_teletext_authoring_selection(
        application,
        channel_id,
        deadline,
    )
    .await
    .map_err(|error| error.code())
}

#[cfg(feature = "ssr")]
async fn load_addon(
    application: &ersatzrs_app::ApplicationServices,
    addon_id: &str,
    installed: Option<ersatzrs_app::InstalledAddonRead>,
    binding: Option<&ersatzrs_domain::ChannelTeletextBindingRead>,
) -> Result<TeletextAddonData, ServerFnError> {
    let installation = application
        .teletext_installation_snapshot(addon_id)
        .await
        .map_err(|_| ServerFnError::new("unavailable"))?;
    let profiles = application
        .list_teletext_profiles(addon_id)
        .await
        .map_err(|_| ServerFnError::new("unavailable"))?;
    let definitions = installed
        .as_ref()
        .map(|addon| addon.manifest.settings.as_slice());
    let settings = installed.as_ref().map_or_else(Vec::new, |addon| {
        addon
            .manifest
            .settings
            .iter()
            .map(|definition| TeletextSettingData {
                definition: definition.clone(),
                inherited: addon
                    .settings
                    .iter()
                    .find(|setting| setting.key == definition.key)
                    .and_then(|setting| inherited_value(setting, definition)),
            })
            .collect()
    });
    Ok(TeletextAddonData {
        addon_id: addon_id.to_owned(),
        name: installed
            .as_ref()
            .and_then(|addon| addon.manifest.name.get("en-US"))
            .cloned()
            .unwrap_or_else(|| addon_id.to_owned()),
        name_labels: installed
            .as_ref()
            .map(|addon| addon.manifest.name.clone())
            .unwrap_or_default(),
        installed: installed.is_some(),
        availability: availability(installation.availability).to_owned(),
        installation: installation.snapshot.map(Into::into),
        settings,
        profiles: profiles
            .into_iter()
            .map(|read| TeletextProfileData {
                id: read.profile.id,
                name: read.profile.name,
                revision: read.profile.revision,
                deleted: read.profile.deleted,
                availability: availability(read.availability).to_owned(),
                settings: read
                    .profile
                    .settings
                    .iter()
                    .map(|setting| {
                        setting_value(
                            setting,
                            definitions.and_then(|items| {
                                items.iter().find(|item| item.key == setting.key)
                            }),
                        )
                    })
                    .collect(),
            })
            .collect(),
        binding: binding.map(|read| TeletextBindingData {
            profile_id: read.binding.profile_id,
            enabled: read.binding.enabled,
            revision: read.binding.revision,
            availability: availability(read.availability).to_owned(),
        }),
    })
}

#[cfg(feature = "ssr")]
fn availability(value: ersatzrs_domain::TeletextProfileAvailability) -> &'static str {
    use ersatzrs_domain::TeletextProfileAvailability as A;
    match value {
        A::Configured => "configured",
        A::MissingInstallation => "missing-installation",
        A::DisabledInstallation => "disabled-installation",
        A::IncompatibleInstallation => "incompatible-installation",
        A::InvalidSettings => "invalid-settings",
        A::DeletedProfile => "deleted-profile",
        A::StaleInstallation => "stale-installation",
    }
}

#[cfg(feature = "ssr")]
fn setting_value(
    setting: &ersatzrs_domain::TeletextProfileSetting,
    definition: Option<&AddonSettingDefinition>,
) -> TeletextSettingValueData {
    use ersatzrs_domain::{TeletextProfileValue as V, TeletextSecretReferenceKind as K};
    let mut value = TeletextSettingValueData {
        key: setting.key.clone(),
        mode: String::new(),
        value: String::new(),
        reference_kind: String::new(),
        reference: String::new(),
        redacted: false,
    };
    match &setting.value {
        V::Literal(literal) => {
            "literal".clone_into(&mut value.mode);
            value.redacted = definition.is_none_or(|definition| {
                definition.kind == ersatzrs_addon_contract::AddonSettingKind::SecretReference
            });
            if !value.redacted {
                value.value.clone_from(literal);
            }
        }
        V::SecretReference { kind, reference } => {
            "reference".clone_into(&mut value.mode);
            match kind {
                K::Environment => "environment",
                K::File => "file",
            }
            .clone_into(&mut value.reference_kind);
            value.reference.clone_from(reference);
        }
    }
    value
}

#[cfg(feature = "ssr")]
fn inherited_value(
    setting: &ersatzrs_app::AddonSettingRead,
    definition: &AddonSettingDefinition,
) -> Option<TeletextSettingValueData> {
    use ersatzrs_domain::{
        TeletextProfileSetting, TeletextProfileValue, TeletextSecretReferenceKind,
    };
    let value = match (&setting.reference_kind, &setting.reference, &setting.value) {
        (Some(kind), Some(reference), _) => TeletextProfileValue::SecretReference {
            kind: match kind.as_str() {
                "environment" => TeletextSecretReferenceKind::Environment,
                "file" => TeletextSecretReferenceKind::File,
                _ => return None,
            },
            reference: reference.clone(),
        },
        (_, _, Some(value)) => TeletextProfileValue::Literal(value.clone()),
        _ => return None,
    };
    Some(setting_value(
        &TeletextProfileSetting {
            key: setting.key.clone(),
            value,
        },
        Some(definition),
    ))
}
