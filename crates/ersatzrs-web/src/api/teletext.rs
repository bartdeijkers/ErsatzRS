//! Native channel teletext controls share the application's transactional CAS.

use std::collections::{BTreeMap, BTreeSet};

use axum::{
    extract::{Form, State},
    response::{IntoResponse, Redirect, Response},
};
use ersatzrs_app::ApplicationServices;
use ersatzrs_core::repositories::{
    ChannelTeletextBindingSave, TeletextProfileMutation, TeletextProfileRejection,
    TeletextProfileSave,
};
use ersatzrs_domain::{
    TeletextInstallationSnapshot, TeletextProfileSetting, TeletextProfileValue,
    TeletextSecretReferenceKind, MAX_TELETEXT_PROFILE_KEY_BYTES, MAX_TELETEXT_PROFILE_SETTINGS,
    MAX_TELETEXT_PROFILE_VALUE_BYTES,
};

use crate::{admin::server_fns::teletext::TeletextInstallationData, host::state::AppState};

/// Includes URL-encoding overhead for the existing bounded complete override bag.
pub const MAX_TELETEXT_FORM_BYTES: usize = 512 * 1024;
type Fields = BTreeMap<String, String>;
type Pairs = Vec<(String, String)>;
type ControlResult<T> = Result<T, &'static str>;

pub async fn save_profile(State(state): State<AppState>, Form(pairs): Form<Pairs>) -> Response {
    let channel = redirect_channel(&pairs);
    let result = profile_command(pairs)
        .and_then(|command| (command.0 == channel).then_some(command).ok_or("invalid"));
    let result = match result {
        Ok((channel, save)) => {
            save_profile_with_application(&state.application, channel, save).await
        }
        Err(code) => Err(code),
    };
    respond(channel, "profile", result)
}

pub async fn save_binding(State(state): State<AppState>, Form(pairs): Form<Pairs>) -> Response {
    let channel = redirect_channel(&pairs);
    let result = match binding_command(pairs) {
        Ok(save) => mutation(state.application.save_channel_teletext_binding(&save).await),
        Err(code) => Err(code),
    };
    respond(channel, "binding", result)
}

pub async fn off(State(state): State<AppState>, Form(pairs): Form<Pairs>) -> Response {
    let channel = redirect_channel(&pairs);
    let result = match off_command(pairs) {
        Ok((channel, addon, revision)) => mutation(
            state
                .application
                .disable_channel_teletext_binding(channel, &addon, revision)
                .await,
        ),
        Err(code) => Err(code),
    };
    respond(channel, "off", result)
}

fn respond(channel: i64, action: &str, result: ControlResult<()>) -> Response {
    let target = if channel > 0 {
        format!("/channels/{channel}/teletext")
    } else {
        "/channels".to_owned()
    };
    let query = match result {
        Ok(()) => format!("saved={action}"),
        Err(code) => format!("error={code}"),
    };
    Redirect::to(&format!("{target}?{query}")).into_response()
}

fn redirect_channel(pairs: &Pairs) -> i64 {
    pairs
        .iter()
        .find(|(key, _)| key == "channel_id")
        .and_then(|(_, value)| positive(value).ok())
        .unwrap_or(0)
}

fn fields(pairs: Pairs) -> ControlResult<Fields> {
    if pairs.len() > MAX_TELETEXT_PROFILE_SETTINGS * 5 + 7 {
        return Err("limit");
    }
    let mut fields = Fields::new();
    let mut bytes = 0_usize;
    for (key, value) in pairs {
        bytes = bytes.saturating_add(key.len()).saturating_add(value.len());
        if bytes > MAX_TELETEXT_FORM_BYTES || key.len() > 256 || value.contains('\0') {
            return Err("invalid");
        }
        if fields.insert(key, value).is_some() {
            return Err("invalid");
        }
    }
    Ok(fields)
}

fn take(fields: &mut Fields, key: &str) -> ControlResult<String> {
    fields.remove(key).ok_or("invalid")
}

fn positive(value: &str) -> ControlResult<i64> {
    value
        .parse::<i64>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or("invalid")
}

fn optional_positive(value: &str) -> ControlResult<Option<i64>> {
    if value.is_empty() {
        Ok(None)
    } else {
        positive(value).map(Some)
    }
}

fn owner(fields: &mut Fields) -> ControlResult<(i64, String)> {
    let channel = positive(&take(fields, "channel_id")?)?;
    let addon = take(fields, "addon_id")?;
    ersatzrs_addon_contract::validate_addon_id(&addon).map_err(|_| "invalid")?;
    Ok((channel, addon))
}

fn installation(fields: &mut Fields) -> ControlResult<TeletextInstallationSnapshot> {
    let raw = take(fields, "expected_installation")?;
    if raw.len() > 8192 {
        return Err("limit");
    }
    let token: TeletextInstallationData = serde_json::from_str(&raw).map_err(|_| "invalid")?;
    Ok(token.into())
}

fn profile_command(pairs: Pairs) -> ControlResult<(i64, TeletextProfileSave)> {
    let mut fields = fields(pairs)?;
    let (channel, addon_id) = owner(&mut fields)?;
    let name = take(&mut fields, "name")?;
    let profile = optional_positive(&take(&mut fields, "profile_id")?)?;
    let revision = optional_positive(&take(&mut fields, "expected_profile_revision")?)?;
    let existing = match (profile, revision) {
        (None, None) => None,
        (Some(id), Some(revision)) => Some((id, revision)),
        _ => return Err("invalid"),
    };
    let expected_installation = installation(&mut fields)?;
    let count = take(&mut fields, "setting_count")?
        .parse::<usize>()
        .map_err(|_| "invalid")?;
    if count > MAX_TELETEXT_PROFILE_SETTINGS {
        return Err("limit");
    }
    let settings = settings(&mut fields, count)?;
    if !fields.is_empty() {
        return Err("invalid");
    }
    ersatzrs_domain::validate_teletext_profile_values(&name, &settings).map_err(|_| "invalid")?;
    Ok((
        channel,
        TeletextProfileSave {
            addon_id,
            name,
            settings,
            expected_installation,
            existing,
        },
    ))
}

fn settings(fields: &mut Fields, count: usize) -> ControlResult<Vec<TeletextProfileSetting>> {
    let mut settings = Vec::with_capacity(count);
    let mut keys = BTreeSet::new();
    for index in 0..count {
        let prefix = format!("setting.{index}");
        let key = take(fields, &format!("{prefix}.key"))?;
        if key.is_empty() || key.len() > MAX_TELETEXT_PROFILE_KEY_BYTES || !keys.insert(key.clone())
        {
            return Err("invalid");
        }
        let mode = take(fields, &format!("{prefix}.mode"))?;
        let value = take(fields, &format!("{prefix}.value"))?;
        let kind = take(fields, &format!("{prefix}.reference_kind"))?;
        let reference = take(fields, &format!("{prefix}.reference"))?;
        if value.len() > MAX_TELETEXT_PROFILE_VALUE_BYTES
            || reference.len() > MAX_TELETEXT_PROFILE_VALUE_BYTES
        {
            return Err("limit");
        }
        let value = match mode.as_str() {
            "inherit" => continue,
            "literal" => TeletextProfileValue::Literal(value),
            "reference" => TeletextProfileValue::SecretReference {
                kind: match kind.as_str() {
                    "environment" => TeletextSecretReferenceKind::Environment,
                    "file" => TeletextSecretReferenceKind::File,
                    _ => return Err("invalid"),
                },
                reference,
            },
            _ => return Err("invalid"),
        };
        settings.push(TeletextProfileSetting { key, value });
    }
    Ok(settings)
}

fn binding_command(pairs: Pairs) -> ControlResult<ChannelTeletextBindingSave> {
    let mut fields = fields(pairs)?;
    let (channel_id, addon_id) = owner(&mut fields)?;
    let choice = take(&mut fields, "profile_choice")?;
    let (id, revision) = choice.split_once(':').ok_or("invalid")?;
    let profile_id = positive(id)?;
    let expected_profile_revision = positive(revision)?;
    let expected_binding_revision =
        optional_positive(&take(&mut fields, "expected_binding_revision")?)?;
    let expected_installation = installation(&mut fields)?;
    if !fields.is_empty() {
        return Err("invalid");
    }
    Ok(ChannelTeletextBindingSave {
        channel_id,
        addon_id,
        profile_id,
        enabled: true,
        expected_installation,
        expected_profile_revision,
        expected_binding_revision,
    })
}

fn off_command(pairs: Pairs) -> ControlResult<(i64, String, i64)> {
    let mut fields = fields(pairs)?;
    let (channel, addon) = owner(&mut fields)?;
    let revision = positive(&take(&mut fields, "expected_binding_revision")?)?;
    if !fields.is_empty() {
        return Err("invalid");
    }
    Ok((channel, addon, revision))
}

async fn save_profile_with_application(
    application: &ApplicationServices,
    channel: i64,
    save: TeletextProfileSave,
) -> ControlResult<()> {
    // Profiles are shared; the page channel is validated without saving it.
    if application
        .get_channel_editor(ersatzrs_app::queries::GetChannelEditor {
            channel_id: channel,
        })
        .await
        .map_err(|_| "storage")?
        .is_none()
    {
        return Err("invalid");
    }
    supported_edit(application, &save).await?;
    mutation(application.save_teletext_profile(&save).await)
}

fn mutation<T>(
    result: ersatzrs_core::repositories::Result<TeletextProfileMutation<T>>,
) -> ControlResult<()> {
    match result.map_err(|_| "storage")? {
        TeletextProfileMutation::Saved(_) => Ok(()),
        TeletextProfileMutation::Rejected(reason) => Err(rejection(reason)),
    }
}

fn rejection(reason: TeletextProfileRejection) -> &'static str {
    use TeletextProfileRejection as R;
    match reason {
        R::Conflict | R::NameConflict => "conflict",
        R::InstallationUnavailable => "unavailable",
        R::WrongAddon => "wrong-addon",
        R::MissingProfile => "missing-profile",
        R::DeletedProfile => "deleted-profile",
        R::BindingLimit | R::RevisionLimit => "limit",
        R::MissingChannel
        | R::MissingBinding
        | R::DisabledBinding
        | R::InvalidSettings
        | R::InvalidValues => "invalid",
    }
}

async fn supported_edit(
    application: &ApplicationServices,
    save: &TeletextProfileSave,
) -> ControlResult<()> {
    let Some((id, revision)) = save.existing else {
        return Ok(());
    };
    let profiles = application
        .list_teletext_profiles(&save.addon_id)
        .await
        .map_err(|_| "storage")?;
    let Some(profile) = profiles.iter().find(|read| read.profile.id == id) else {
        // The transaction distinguishes absent profiles from foreign ownership.
        return Ok(());
    };
    if profile.profile.deleted {
        return Err("deleted-profile");
    }
    if profile.profile.revision != revision {
        return Err("conflict");
    }
    let overview = application
        .addon_overview()
        .await
        .map_err(|_| "unavailable")?;
    let installed = overview
        .installed
        .iter()
        .find(|addon| addon.addon_id == save.addon_id)
        .ok_or("unavailable")?;
    for setting in &profile.profile.settings {
        let Some(definition) = installed
            .manifest
            .settings
            .iter()
            .find(|item| item.key == setting.key)
        else {
            return Err("invalid");
        };
        let supported = match &setting.value {
            TeletextProfileValue::Literal(_) => {
                definition.kind != ersatzrs_addon_contract::AddonSettingKind::SecretReference
            }
            TeletextProfileValue::SecretReference { .. } => {
                definition.kind == ersatzrs_addon_contract::AddonSettingKind::SecretReference
            }
        };
        if !supported {
            return Err("invalid");
        }
    }
    Ok(())
}
