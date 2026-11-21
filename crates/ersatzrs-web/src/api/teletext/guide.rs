//! Channel guide shortcut. The preview uses the normal multi-source compiler.
use super::{
    fields, installation, mutation, optional_positive, owner, prepare_response, redirect_channel,
    rejection, respond, take, ControlResult, Pairs,
};
use crate::admin::server_fns::teletext::{
    effective_setting, load_channel_teletext_for_application, ChannelTeletextData, GUIDE_ADDON,
};
use crate::admin::server_fns::teletext::{TeletextAddonData, TeletextProfileData};
use crate::host::state::AppState;
use axum::{
    extract::{Form, State},
    response::Response,
};
use ersatzrs_app::ApplicationServices;
use ersatzrs_core::repositories::{
    ChannelTeletextBindingSave, TeletextProfileMutation, TeletextProfileSave,
};
use ersatzrs_domain::TeletextInstallationSnapshot;
use ersatzrs_domain::{TeletextPageAddress, TeletextPageNumber, TeletextSubcode};

pub async fn setup_guide(State(state): State<AppState>, Form(pairs): Form<Pairs>) -> Response {
    let channel = redirect_channel(&pairs);
    let deadline =
        std::time::Instant::now() + ersatzrs_app::teletext_channel_documents::PREPARATION_TIMEOUT;
    match setup(&state.application, channel, pairs).await {
        Ok((request, owner)) => {
            prepare_response(&state, request, Some(owner), deadline, true).await
        }
        Err(code) => respond(channel, "draft", Err(code)),
    }
}

async fn channel_data(
    app: &ApplicationServices,
    channel: i64,
) -> ControlResult<ChannelTeletextData> {
    load_channel_teletext_for_application(app, channel)
        .await
        .map_err(|_| "unavailable")?
        .ok_or("invalid-input")
}

async fn setup(
    app: &ApplicationServices,
    channel: i64,
    pairs: Pairs,
) -> ControlResult<(
    crate::host::teletext_authoring::TeletextAuthoringPrepare,
    TeletextPageAddress,
)> {
    let mut fields = fields(pairs)?;
    let (id, addon_id) = owner(&mut fields)?;
    let install = installation(&mut fields)?;
    let binding_revision = optional_positive(&take(&mut fields, "expected_binding_revision")?)?;
    let profile_revision = optional_positive(&take(&mut fields, "expected_profile_revision")?)?;
    if channel != id || addon_id != GUIDE_ADDON || !fields.is_empty() {
        return Err("invalid-input");
    }
    let mut data = channel_data(app, channel).await?;
    let addon = data
        .addons
        .iter()
        .find(|a| a.addon_id == GUIDE_ADDON)
        .ok_or("unavailable")?;
    if data.guide_channels.is_none()
        || !data.catalog_available
        || addon.availability != "configured"
    {
        return Err("unavailable");
    }
    let binding = addon.binding.as_ref();
    let profile = binding.and_then(|b| addon.profiles.iter().find(|p| p.id == b.profile_id));
    if addon.installation.clone().map(Into::into).as_ref() != Some(&install)
        || binding.map(|b| b.revision) != binding_revision
        || profile.map(|p| p.revision) != profile_revision
    {
        return Err("conflict");
    }

    if !binding.is_some_and(|b| b.enabled && b.availability == "configured") {
        let (profile_id, revision) =
            profile_to_bind(app, channel, addon, profile, &install).await?;
        mutation(
            app.save_channel_teletext_binding(&ChannelTeletextBindingSave {
                channel_id: channel,
                addon_id,
                profile_id,
                enabled: true,
                expected_installation: install.clone(),
                expected_profile_revision: revision,
                expected_binding_revision: binding_revision,
            })
            .await,
        )?;
        data = channel_data(app, channel).await?;
        let rebound = data
            .addons
            .iter()
            .find(|a| a.addon_id == GUIDE_ADDON)
            .ok_or("conflict")?;
        if !rebound.binding.as_ref().is_some_and(|b| {
            b.enabled && b.profile_id == profile_id && b.availability == "configured"
        }) || !rebound
            .profiles
            .iter()
            .any(|p| p.id == profile_id && p.revision == revision)
            || rebound.installation.clone().map(Into::into).as_ref() != Some(&install)
        {
            return Err("conflict");
        }
    }
    let addon = data
        .addons
        .iter()
        .find(|a| a.addon_id == GUIDE_ADDON)
        .ok_or("conflict")?;
    let page = effective_setting(addon, &["INDEX_PAGE", "PAGE_START"])
        .and_then(|s| s.parse().ok())
        .ok_or("invalid-input")?;
    let owner = TeletextPageAddress {
        page: TeletextPageNumber::new(page).map_err(|_| "invalid-input")?,
        subcode: TeletextSubcode::new(0).map_err(|_| "invalid-input")?,
    };
    let ids = data
        .guide_selection
        .filter(|ids| !ids.is_empty())
        .unwrap_or_else(|| vec![channel]);
    let input = serde_json::json!({"input_version":2,"channel_id":channel,"entry_addon_id":GUIDE_ADDON,
        "entry":owner,"v1_region":"western_latin","resource_transmissions":[],"guide_channel_ids":ids});
    Ok((
        crate::host::teletext_authoring::TeletextAuthoringPrepare {
            scope: crate::host::teletext_authoring::TeletextAuthoringScope::Base,
            channel_id: channel,
            name: format!("TV Guide · {}", data.channel_number),
            selection_digest: data.authoring_selection.ok_or("unavailable")?,
            input_json: input.to_string(),
        },
        owner,
    ))
}

async fn profile_to_bind(
    app: &ApplicationServices,
    channel: i64,
    addon: &TeletextAddonData,
    profile: Option<&TeletextProfileData>,
    install: &TeletextInstallationSnapshot,
) -> ControlResult<(i64, i64)> {
    if let Some(profile) = profile.filter(|p| !p.deleted && p.availability == "configured") {
        return Ok((profile.id, profile.revision));
    }
    // Scoped and stable across retries. Never rewrite a shared or stale profile.
    let name = format!("TV Guide · {channel} · {}", install.state_revision);
    if let Some(p) = addon.profiles.iter().find(|p| !p.deleted && p.name == name) {
        if !p.settings.is_empty() || p.availability != "configured" {
            return Err("conflict");
        }
        Ok((p.id, p.revision))
    } else {
        let save = TeletextProfileSave {
            addon_id: addon.addon_id.clone(),
            name,
            expected_installation: install.clone(),
            existing: None,
            settings: vec![],
        };
        match app
            .save_teletext_profile(&save)
            .await
            .map_err(|_| "storage")?
        {
            TeletextProfileMutation::Saved(p) => Ok((p.profile.id, p.profile.revision)),
            TeletextProfileMutation::Rejected(reason) => Err(rejection(reason)),
        }
    }
}
