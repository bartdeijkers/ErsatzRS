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
    pub effective_authoring: Option<TeletextEffectiveAuthoringData>,
    pub effective_authoring_error: Option<String>,
    /// Explicit source choices only; absence means the source list is unavailable.
    pub guide_channels: Option<Vec<TeletextGuideChannelData>>,
    /// Saved guide source channels for this channel; `None` when never saved.
    #[serde(default)]
    pub guide_selection: Option<Vec<i64>>,
    /// Read-only preview availability; never publication or delivery authority.
    #[serde(default)]
    pub current_guide_available: Option<bool>,
    pub addons: Vec<TeletextAddonData>,
    /// Ordered readiness facts; absent when an older server rendered the page.
    #[serde(default)]
    pub readiness: Option<TeletextReadinessData>,
    /// Custom pages from the teletext element library; `None` when the
    /// library is unavailable or an older server rendered the page.
    #[serde(default)]
    pub custom_pages: Option<TeletextCustomPagesData>,
}

/// Teletext elements linked to this channel plus the library to add from.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeletextCustomPagesData {
    pub links: Vec<TeletextCustomPageData>,
    pub library: Vec<TeletextElementOptionData>,
}

/// One teletext element linked to this channel.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeletextCustomPageData {
    pub link_id: i64,
    pub element_id: i64,
    pub name: String,
    /// Display page number the channel uses, for example `"150"`.
    pub page: String,
    /// Channel override in display form; empty when the element's own page is used.
    pub page_override: String,
    /// The element's own page number in display form.
    pub default_page: String,
    pub enabled: bool,
    /// Why this enabled page cannot go out with the next Check, as a stable
    /// code (`unreadable`, `page-number`, `unset-field`, `duplicate-page`,
    /// `double-height`, `addon-range`); `None` when it can.
    #[serde(default)]
    pub issue: Option<String>,
    /// Add-on id for an `addon-range` issue.
    #[serde(default)]
    pub issue_addon: Option<String>,
}

/// A library element that can be linked to a channel.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeletextElementOptionData {
    pub id: i64,
    pub name: String,
    pub page: String,
}

/// Separately observed preconditions for teletext on this channel. Each fact
/// is configuration or durable state; none claims receiver/client display.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeletextReadinessData {
    /// `absent`, `profile`, `custom`, `invalid` or `unreadable`.
    pub authoring_policy: String,
    pub output_policy: String,
    /// Named capacity profile when both settings equal it exactly.
    pub capacity_profile: Option<String>,
    pub profiles: Vec<TeletextCapacityProfileData>,
    /// `legacy`, `hybrid-builtin`, `hybrid-script` or `hls`.
    pub route: String,
    /// The global MPEG-TS script id that takes over muxing, when one is set.
    pub script: Option<String>,
    /// Accepted (published) generation id, if any.
    pub published_generation: Option<i64>,
    /// A validated revision awaits explicit publication.
    pub publication_pending: bool,
    /// Active outputs confirming authored delivery, from live observations.
    pub delivering_outputs: usize,
    pub active_outputs: usize,
    /// Last terminal transport failure on this channel since teletext was last
    /// delivered: `transport_capacity`, `deadline`, `source` or `transport`.
    #[serde(default)]
    pub last_failure: Option<String>,
    /// RFC 3339 UTC time of that failure.
    #[serde(default)]
    pub last_failure_at: Option<String>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeletextCapacityProfileData {
    pub id: String,
    pub max_pages: u64,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeletextGuideChannelData {
    pub id: i64,
    pub number: String,
    pub name: String,
}

/// Redacted current Scripted scope, observed without invoking producers.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeletextEffectiveAuthoringData {
    pub selection_digest: String,
    pub producer_addon_ids: Vec<String>,
    pub valid_until: Option<String>,
    pub is_absent: bool,
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
                || addon.manifest.has_capability("teletext.pages.v3")
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
    let (effective_authoring, effective_authoring_error) =
        match effective_authoring_selection(application, channel_id).await {
            Ok(selection) => (Some(selection), None),
            Err(code) => (None, Some(code.to_owned())),
        };
    // Missing guide policy or source reads cannot disable static authoring/Off.
    let guide_channels =
        if crate::host::teletext_authoring::guide_preparation_configured(application).await {
            match application.list_channel_numbers().await {
                Ok(rows) => Some(
                    rows.into_iter()
                        .map(|row| TeletextGuideChannelData {
                            id: row.id,
                            number: row.number,
                            name: row.name,
                        })
                        .collect(),
                ),
                // None renders an unavailable section, not a successful empty
                // list or permission to substitute static guide content.
                Err(_) => None,
            }
        } else {
            None
        };
    let current_guide_available = current_guide_available(application).await;
    let readiness = Some(readiness(application, &channel.form, channel_id).await);
    Ok(Some(ChannelTeletextData {
        channel_id,
        channel_number: channel.form.number,
        channel_name: channel.form.name,
        catalog_available,
        authoring_selection,
        authoring_error,
        effective_authoring,
        effective_authoring_error,
        guide_channels,
        guide_selection: load_guide_selection(application, channel_id).await?,
        current_guide_available,
        addons,
        readiness,
        custom_pages: Some(load_custom_pages(application, channel_id).await?),
    }))
}

#[cfg(feature = "ssr")]
async fn load_custom_pages(
    application: &ersatzrs_app::ApplicationServices,
    channel_id: i64,
) -> Result<TeletextCustomPagesData, ServerFnError> {
    use ersatzrs_domain::teletext_element_page_label;
    let links = application
        .channel_teletext_elements(channel_id)
        .await
        .map_err(|_| ServerFnError::new("unavailable"))?;
    let library = application
        .list_teletext_elements()
        .await
        .map_err(|_| ServerFnError::new("unavailable"))?;
    // The same plan the host producer runs at Check, so the page shows exactly
    // what would block it. A failed plan shows no per-page reasons.
    let issues = Box::pin(application.plan_teletext_custom_pages(channel_id, chrono::Utc::now()))
        .await
        .map(|plan| plan.issues)
        .unwrap_or_default();
    let issue_for = |link_id: i64| {
        use ersatzrs_app::teletext_elements::broadcast::CustomPageIssueKind as K;
        issues
            .iter()
            .find(|issue| issue.link_id == link_id)
            .map(|issue| match &issue.kind {
                K::Unreadable => ("unreadable", None),
                K::PageNumber => ("page-number", None),
                K::UnsetField => ("unset-field", None),
                K::DuplicatePage => ("duplicate-page", None),
                K::DoubleHeight => ("double-height", None),
                K::AddonRange { addon_id } => ("addon-range", Some(addon_id.clone())),
            })
    };
    let default_page = |element_id: i64| {
        library
            .iter()
            .find(|element| element.id == element_id)
            .map(|element| teletext_element_page_label(element.page_number))
            .unwrap_or_default()
    };
    Ok(TeletextCustomPagesData {
        links: links
            .iter()
            .map(|link| TeletextCustomPageData {
                link_id: link.id,
                element_id: link.element_id,
                name: link.element_name.clone(),
                page: teletext_element_page_label(link.effective_page_number),
                page_override: link
                    .page_number
                    .map(teletext_element_page_label)
                    .unwrap_or_default(),
                default_page: default_page(link.element_id),
                enabled: link.enabled,
                issue: issue_for(link.id).map(|(code, _)| code.to_owned()),
                issue_addon: issue_for(link.id).and_then(|(_, addon)| addon),
            })
            .collect(),
        library: library
            .iter()
            .map(|element| TeletextElementOptionData {
                id: element.id,
                name: element.name.clone(),
                page: teletext_element_page_label(element.page_number),
            })
            .collect(),
    })
}

/// Admin setting holding each channel's saved guide source channels as a JSON
/// object (`{"<channel id>": [<source channel id>, ...]}`). It only prefills
/// the channel page; every check still validates the submitted channel ids.
pub const TELETEXT_GUIDE_SELECTION_SETTING: &str = "teletext.guide_selection";

/// Parse the saved guide selection map; malformed content reads as empty.
#[must_use]
pub fn parse_guide_selection(raw: &str) -> std::collections::BTreeMap<i64, Vec<i64>> {
    serde_json::from_str::<std::collections::BTreeMap<String, Vec<i64>>>(raw)
        .map(|map| {
            map.into_iter()
                .filter_map(|(key, ids)| {
                    let key = key.parse::<i64>().ok().filter(|key| *key > 0)?;
                    let ids = ids.into_iter().filter(|id| *id > 0).collect();
                    Some((key, ids))
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(feature = "ssr")]
async fn current_guide_available(application: &ersatzrs_app::ApplicationServices) -> Option<bool> {
    match application
        .get_optional_setting(
            crate::host::teletext_runtime_policy::NATIVE_TELETEXT_RUNTIME_POLICY_SETTING,
        )
        .await
    {
        Ok(None) => Some(false),
        Ok(Some(raw)) => {
            match crate::host::teletext_runtime_policy::parse_native_teletext_runtime_policy(&raw) {
                Ok(policy) => Some(policy.guide_refresh),
                Err(_) => None,
            }
        }
        Err(_) => None,
    }
}

#[cfg(feature = "ssr")]
async fn load_guide_selection(
    application: &ersatzrs_app::ApplicationServices,
    channel_id: i64,
) -> Result<Option<Vec<i64>>, ServerFnError> {
    let raw = application
        .get_optional_setting(TELETEXT_GUIDE_SELECTION_SETTING)
        .await
        .map_err(|_| ServerFnError::new("unavailable"))?;
    Ok(raw.and_then(|raw| parse_guide_selection(&raw).remove(&channel_id)))
}

/// Classify the channel's MPEG-TS route for authored teletext.
///
/// HLS playlists never carry the DVB teletext service; MPEG-TS (legacy concat)
/// uses the host mux; Hybrid uses the host wrapper unless a global MPEG-TS
/// script owns the final mux, which the host cannot extend.
#[cfg(feature = "ssr")]
pub(crate) fn teletext_output_route(
    streaming_mode: i32,
    streaming_engine: i32,
    script: Option<&str>,
) -> &'static str {
    let hybrid = match streaming_mode {
        2 | 4 => return "hls",
        // A Next channel's legacy transport request is served as Hybrid.
        1 => streaming_engine == 1,
        _ => true,
    };
    if !hybrid {
        "legacy"
    } else if script.is_some_and(|id| !id.trim().is_empty()) {
        "hybrid-script"
    } else {
        "hybrid-builtin"
    }
}

#[cfg(feature = "ssr")]
async fn readiness(
    application: &ersatzrs_app::ApplicationServices,
    form: &ersatzrs_core::repositories::ChannelEditorForm,
    channel_id: i64,
) -> TeletextReadinessData {
    use crate::host::teletext_capacity::{policy_status, TeletextPolicyState, PROFILES};
    let (authoring, output) = policy_status(application).await;
    let capacity_profile = match (&authoring, &output) {
        (TeletextPolicyState::Profile(left), TeletextPolicyState::Profile(right))
            if left == right =>
        {
            Some((*left).to_owned())
        }
        _ => None,
    };
    // Absence is the seeded `default` script, which is an external script.
    let script = match application
        .get_optional_setting(ersatzrs_domain::CONFIG_KEY_FFMPEG_DEFAULT_MPEG_TS_SCRIPT)
        .await
    {
        Ok(Some(value)) => Some(value),
        Ok(None) | Err(_) => Some("default".to_owned()),
    };
    let route = teletext_output_route(
        form.streaming_mode,
        form.streaming_engine,
        script.as_deref(),
    );
    let published_generation = match application.accepted_teletext_generation(channel_id).await {
        Ok(Some(generation)) => Some(generation.id),
        _ => match application
            .effective_teletext_accepted_generation(channel_id)
            .await
        {
            Ok(Some(generation)) => Some(generation.id),
            _ => None,
        },
    };
    let publication_pending = matches!(
        application.teletext_publication_request(channel_id).await,
        Ok(Some(_))
    ) || matches!(
        application
            .effective_teletext_publication_request(channel_id)
            .await,
        Ok(Some(_))
    );
    let observations = application.teletext_output_observations(channel_id);
    let failure = observations
        .as_ref()
        .and_then(|observed| observed.last_failure.clone());
    let (active_outputs, delivering_outputs) = observations.map_or((0, 0), |observed| {
        let delivering = observed
            .outputs
            .iter()
            .filter(|output| {
                output.phase == ersatzrs_domain::TeletextOutputObservationPhase::Applied
                    && output
                        .applied
                        .as_ref()
                        .is_some_and(|applied| applied.authored)
            })
            .count();
        (observed.outputs.len(), delivering)
    });
    TeletextReadinessData {
        authoring_policy: authoring.code().to_owned(),
        output_policy: output.code().to_owned(),
        capacity_profile,
        profiles: PROFILES
            .iter()
            .map(|profile| TeletextCapacityProfileData {
                id: profile.id.to_owned(),
                max_pages: profile.max_pages().unwrap_or(0),
            })
            .collect(),
        route: route.to_owned(),
        script: matches!(route, "hybrid-script").then(|| script.unwrap_or_default()),
        published_generation,
        publication_pending,
        delivering_outputs,
        active_outputs,
        last_failure: failure.as_ref().and_then(|failure| {
            serde_json::to_value(failure.kind)
                .ok()
                .and_then(|kind| kind.as_str().map(str::to_owned))
        }),
        last_failure_at: failure.map(|failure| {
            failure
                .at
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
        }),
    }
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
async fn effective_authoring_selection(
    application: &ersatzrs_app::ApplicationServices,
    channel_id: i64,
) -> Result<TeletextEffectiveAuthoringData, &'static str> {
    let deadline = std::time::Instant::now()
        .checked_add(ersatzrs_app::teletext_channel_documents::PREPARATION_TIMEOUT)
        .ok_or("expired")?;
    let observed = crate::host::teletext_authoring::observe_effective_teletext_authoring_selection(
        application,
        channel_id,
        deadline,
    )
    .await
    .map_err(|error| error.code())?;
    Ok(TeletextEffectiveAuthoringData {
        selection_digest: observed.selection_digest,
        producer_addon_ids: observed.producer_addon_ids,
        valid_until: observed.valid_until.map(|until| until.to_rfc3339()),
        is_absent: observed.is_absent,
    })
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
