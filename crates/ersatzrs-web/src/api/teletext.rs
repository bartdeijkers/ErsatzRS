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

/// Validate selected static producers and save a new original draft. Publication
/// is never part of this action, including when a subsequent preview fails.
pub async fn prepare_design(State(state): State<AppState>, Form(pairs): Form<Pairs>) -> Response {
    use crate::host::teletext_authoring::prepare_static_teletext_design;
    let channel = redirect_channel(&pairs);
    // Establish the original budget before parsing or reading host policy.
    let deadline = std::time::Instant::now()
        .checked_add(ersatzrs_app::teletext_channel_documents::PREPARATION_TIMEOUT);
    let parsed = prepare_command(pairs);
    let (request, owner) = match parsed {
        Ok(value) => value,
        Err(code) => return respond(channel, "draft", Err(code)),
    };
    let Some(deadline) = deadline else {
        return respond(channel, "draft", Err("expired"));
    };
    match prepare_static_teletext_design(&state.application, request, deadline).await {
        Ok(saved) => {
            let identity = saved.identity;
            let mut query = format!(
                "saved=draft&design_id={}&revision={}&page={}&subcode={:04x}",
                identity.design_id,
                identity.revision,
                owner.page,
                owner.subcode.get(),
            );
            if let Err(error) = &saved.preview {
                query.push_str(&authoring_error_query(error));
            }
            Redirect::to(&format!("/channels/{channel}/teletext?{query}")).into_response()
        }
        Err(error) => {
            let query = authoring_error_query(&error);
            Redirect::to(&format!(
                "/channels/{channel}/teletext?{}",
                query.trim_start_matches('&')
            ))
            .into_response()
        }
    }
}

/// Requesting intent and selecting output are separate explicit native actions.
pub async fn request_publication(
    State(state): State<AppState>,
    Form(pairs): Form<Pairs>,
) -> Response {
    publication_action(&state.application, pairs, PublicationAction::Request).await
}

pub async fn publish_design(State(state): State<AppState>, Form(pairs): Form<Pairs>) -> Response {
    publication_action(&state.application, pairs, PublicationAction::Publish).await
}

#[derive(Clone, Copy)]
enum PublicationAction {
    Request,
    Publish,
}

struct PublicationCommand {
    target: crate::host::teletext_authoring::TeletextAuthoringPublicationTarget,
    expected_generation_id: Option<i64>,
    owner: ersatzrs_domain::TeletextPageAddress,
}

async fn publication_action(
    application: &ApplicationServices,
    pairs: Pairs,
    action: PublicationAction,
) -> Response {
    use crate::host::teletext_authoring::{
        publish_static_teletext_generation, request_static_teletext_publication,
        TeletextAuthoringPublishTarget,
    };
    let channel = redirect_channel(&pairs);
    let deadline = std::time::Instant::now()
        .checked_add(ersatzrs_app::teletext_channel_documents::PREPARATION_TIMEOUT);
    let command = match publication_command(pairs, action) {
        Ok(command) => command,
        Err(code) => return respond(channel, "publication", Err(code)),
    };
    let Some(deadline) = deadline else {
        return respond(channel, "publication", Err("expired"));
    };
    let identity = command.target.identity;
    let result = match action {
        PublicationAction::Request => {
            request_static_teletext_publication(application, command.target, deadline)
                .await
                .map(|_| "publication-requested")
        }
        PublicationAction::Publish => publish_static_teletext_generation(
            application,
            TeletextAuthoringPublishTarget {
                publication: command.target,
                expected_generation_id: command.expected_generation_id,
            },
            deadline,
        )
        .await
        .map(|_| "publication-selected"),
    };
    let mut query = format!(
        "design_id={}&revision={}&page={}&subcode={:04x}",
        identity.design_id,
        identity.revision,
        command.owner.page,
        command.owner.subcode.get(),
    );
    match result {
        Ok(code) => {
            query.push_str("&saved=");
            query.push_str(code);
        }
        Err(error) if error.code() == "conflict" => {
            query.push_str("&error=publication-conflict");
        }
        Err(error) => query.push_str(&authoring_error_query(&error)),
    }
    Redirect::to(&format!("/channels/{channel}/teletext?{query}")).into_response()
}

fn publication_command(
    pairs: Pairs,
    action: PublicationAction,
) -> ControlResult<PublicationCommand> {
    use crate::host::teletext_authoring::{
        TeletextAuthoringIdentity, TeletextAuthoringPublicationTarget,
    };
    use ersatzrs_domain::{TeletextPageAddress, TeletextPageNumber, TeletextSubcode};
    let mut fields = fields(pairs)?;
    let identity = TeletextAuthoringIdentity {
        channel_id: positive(&take(&mut fields, "channel_id")?)?,
        design_id: positive(&take(&mut fields, "design_id")?)?,
        revision: positive(&take(&mut fields, "revision")?)?,
    };
    let digest = take(&mut fields, "digest")?;
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("invalid-input");
    }
    let expected_design_head = positive(&take(&mut fields, "expected_design_head")?)?;
    let expected_request_id = optional_positive(&take(&mut fields, "expected_request_id")?)?;
    let expected_generation_id = match action {
        PublicationAction::Request => None,
        PublicationAction::Publish => {
            optional_positive(&take(&mut fields, "expected_generation_id")?)?
        }
    };
    let page = take(&mut fields, "preview_page")?
        .parse::<u16>()
        .map_err(|_| "invalid-input")?;
    let subcode = take(&mut fields, "preview_subcode")?;
    if subcode.len() != 4 || !subcode.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("invalid-input");
    }
    let owner = TeletextPageAddress {
        page: TeletextPageNumber::new(page).map_err(|_| "invalid-input")?,
        subcode: TeletextSubcode::new(
            u16::from_str_radix(&subcode, 16).map_err(|_| "invalid-input")?,
        )
        .map_err(|_| "invalid-input")?,
    };
    if !fields.is_empty() {
        return Err("invalid-input");
    }
    Ok(PublicationCommand {
        target: TeletextAuthoringPublicationTarget {
            identity,
            digest,
            expected_design_head,
            expected_request_id,
        },
        expected_generation_id,
        owner,
    })
}

fn prepare_command(
    pairs: Pairs,
) -> ControlResult<(
    crate::host::teletext_authoring::TeletextAuthoringPrepare,
    ersatzrs_domain::TeletextPageAddress,
)> {
    use ersatzrs_domain::{TeletextPageAddress, TeletextPageNumber, TeletextSubcode};
    let mut fields = fields(pairs)?;
    let channel_id = positive(&take(&mut fields, "channel_id")?)?;
    let selection_digest = take(&mut fields, "selection_digest")?;
    if selection_digest.len() != 64
        || !selection_digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("invalid-input");
    }
    let name = take(&mut fields, "name")?;
    let addon = take(&mut fields, "entry_addon_id")?;
    ersatzrs_addon_contract::validate_addon_id(&addon).map_err(|_| "invalid-input")?;
    let page = take(&mut fields, "entry_page")?
        .parse::<u16>()
        .map_err(|_| "invalid-input")?;
    let subcode = take(&mut fields, "entry_subcode")?;
    if subcode.is_empty() || subcode.len() > 4 {
        return Err("invalid-input");
    }
    let subcode = u16::from_str_radix(&subcode, 16).map_err(|_| "invalid-input")?;
    let owner = TeletextPageAddress {
        page: TeletextPageNumber::new(page).map_err(|_| "invalid-input")?,
        subcode: TeletextSubcode::new(subcode).map_err(|_| "invalid-input")?,
    };
    let region = take(&mut fields, "v1_region")?;
    let lifecycle = take(&mut fields, "resource_transmissions")?;
    if lifecycle.len() > 65_536 {
        return Err("limit");
    }
    if !fields.is_empty() {
        return Err("invalid-input");
    }
    let mut input_json = serde_json::to_string(&serde_json::json!({
        "input_version": 1, "channel_id": channel_id, "entry_addon_id": addon,
        "entry": owner, "v1_region": region,
    }))
    .map_err(|_| "invalid-input")?;
    // Keep the submitted JSON untouched until the closed typed parser checks it;
    // a Value round-trip would silently discard duplicate lifecycle fields.
    input_json.pop();
    input_json.push_str(",\"resource_transmissions\":");
    input_json.push_str(&lifecycle);
    input_json.push('}');
    Ok((
        crate::host::teletext_authoring::TeletextAuthoringPrepare {
            channel_id,
            selection_digest,
            input_json,
            name,
        },
        owner,
    ))
}

fn authoring_error_query(
    error: &crate::host::teletext_authoring::TeletextAuthoringError,
) -> String {
    format!("&error={}{}", error.code(), authoring_error_location(error))
}

pub(super) fn authoring_error_location(
    error: &crate::host::teletext_authoring::TeletextAuthoringError,
) -> String {
    use std::fmt::Write as _;
    let mut query = String::new();
    if let Some(location) = &error.location {
        if let Some(resource) = location.resource {
            if let Ok(raw) = serde_json::to_string(&resource) {
                query.push_str("&error_resource=");
                query.push_str(&urlencoding::encode(&raw));
            }
        }
        if let Some(owner) = location.owner {
            let _ = write!(
                query,
                "&error_page={}&error_subcode={:04x}",
                owner.page,
                owner.subcode.get()
            );
        }
        if let Some((row, column)) = location.position {
            let _ = write!(query, "&error_row={row}&error_column={column}");
        }
    }
    query
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
