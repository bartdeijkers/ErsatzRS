//! Two operator decisions: prepare the channel guide, then publish its preview.
use super::{hidden, source_on, source_usable, style};
use crate::admin::i18n::{tr, TextKey};
use crate::admin::server_fns::teletext::GUIDE_ADDON;
use crate::admin::server_fns::teletext::{ChannelTeletextData, TeletextAddonData};
use crate::admin::server_fns::teletext_drafts::TeletextDraftPreviewResult;
use crate::admin::server_fns::teletext_drafts::{
    TeletextDraftPreviewData, TeletextDraftPublicationState,
};
use crate::admin::ui::primitives::button::{btn_class, ButtonVariant};
use leptos::prelude::*;

pub(super) fn section(
    data: &ChannelTeletextData,
    preview: Option<&Result<TeletextDraftPreviewResult, ServerFnError>>,
) -> Option<AnyView> {
    let addon = data
        .addons
        .iter()
        .find(|addon| addon.addon_id == GUIDE_ADDON)?;
    let body = if !source_usable(addon) || !data.catalog_available {
        view! { <p>{tr(TextKey::TeletextSourceUnavailableHelp)}</p>
        <a href="/settings/addons">{tr(TextKey::TeletextSourceFixAddon)}</a> }
        .into_any()
    } else if let Some(result) = preview {
        match result
            .as_ref()
            .ok()
            .and_then(|result| result.preview.as_ref())
        {
            Some(draft) => preview_controls(data, draft),
            None => {
                view! { <p role="alert">{tr(TextKey::TeletextAuthoringRejected)}</p> }.into_any()
            }
        }
    } else if data.guide_channels.is_none() {
        view! { <p role="status">{tr(TextKey::TeletextGuideChannelsUnavailable)}</p>
        <a href="#teletext-capacity">{tr(TextKey::TeletextReadinessCapacity)}</a> }
        .into_any()
    } else {
        setup_form(data, addon)
    };
    Some(view! {
        <section class=style::addon data-testid="teletext-guide-setup" aria-labelledby="channel-guide-title">
            <h2 id="channel-guide-title">{tr(TextKey::ProgrammeGuide)}</h2>
            {body}
        </section>
    }.into_any())
}

fn setup_form(data: &ChannelTeletextData, addon: &TeletextAddonData) -> AnyView {
    let binding = addon.binding.as_ref();
    let profile = binding.and_then(|b| addon.profiles.iter().find(|p| p.id == b.profile_id));
    let repair = binding.is_some_and(|b| b.availability != "configured");
    let action = if repair {
        TextKey::TeletextGuideRepair
    } else if source_on(addon) {
        TextKey::TeletextGuidePreviewAction
    } else {
        TextKey::TeletextGuideSetup
    };
    let install = addon
        .installation
        .as_ref()
        .and_then(|i| serde_json::to_string(i).ok())
        .unwrap_or_default();
    let saved = data.guide_selection.as_deref().filter(|s| !s.is_empty());
    let names = data
        .guide_channels
        .iter()
        .flatten()
        .filter(|c| saved.map_or(c.id == data.channel_id, |ids| ids.contains(&c.id)))
        .map(|c| format!("{} · {}", c.number, c.name))
        .collect::<Vec<_>>()
        .join(", ");
    view! {
        <form method="post" action="/admin-api/teletext/guide/setup">
            {hidden("channel_id",data.channel_id.to_string())}
            {hidden("addon_id",addon.addon_id.clone())}
            {hidden("expected_installation",install)}
            {hidden("expected_binding_revision",binding.map(|b|b.revision.to_string()).unwrap_or_default())}
            {hidden("expected_profile_revision",profile.map(|p|p.revision.to_string()).unwrap_or_default())}
            <p>{tr(TextKey::TeletextGuideSourceSummary)}" "{names}</p>
            {repair.then(|| view! { <p>{tr(TextKey::TeletextGuideRepairHelp)}</p> })}
            <button type="submit" class=btn_class(ButtonVariant::Primary)>{tr(action)}</button>
        </form>
    }.into_any()
}

fn preview_controls(channel: &ChannelTeletextData, data: &TeletextDraftPreviewData) -> AnyView {
    let number = &channel.channel_number;
    let mut seen = std::collections::BTreeSet::new();
    let links = data.pages.iter().filter(|owner| seen.insert(owner.page))
        .map(|owner| view! {
            <a class=style::manage href=format!("/channels/{}/teletext?guide_setup=1&design_id={}&revision={}&page={}&subcode={:04X}",
                data.channel_id,data.design_id,data.revision,owner.page,owner.subcode.get())
                aria-current=(data.owner.is_some_and(|p| p.page == owner.page)).then_some("page")>
                {tr(TextKey::TeletextPreviewPage)}" "{owner.page.to_string()}
            </a>
        }).collect_view();
    let publish = data.publication.as_ref().map_or_else(|| view! { <p role="status">{tr(TextKey::TeletextPublicationUnavailable)}</p> }.into_any(), |obs| {
        if matches!(obs.state, TeletextDraftPublicationState::Selected) {
            return view! { <p role="status">{tr(TextKey::TeletextGuidePublished).replace("{number}", number)}</p> }.into_any();
        }
        if !(obs.can_request || obs.can_publish) || data.image_url.is_none() {
            return view! { <p role="status">{tr(TextKey::TeletextPublicationUnavailable)}</p> }.into_any();
        }
        let action = if obs.can_publish { "/admin-api/teletext/design/publish" } else { "/admin-api/teletext/guide/publish" };
        view! {
            <form method="post" action=action>
                {super::super::teletext_drafts::publication_fields(data,obs)}
                {hidden("expected_generation_id",obs.current_accepted_id.map(|id|id.to_string()).unwrap_or_default())}
                {obs.replaces_pending.then(|| view! { <p>{tr(TextKey::TeletextPublicationReplacePending)}</p> })}
                <button type="submit" class=btn_class(ButtonVariant::Primary)>{tr(TextKey::TeletextGuidePublish).replace("{number}",number)}</button>
            </form>
        }.into_any()
    });
    view! {
        <h3>{tr(TextKey::TeletextPreviewTitle)}</h3>
        <nav class=style::actions aria-label=tr(TextKey::TeletextPreviewPage)>{links}</nav>
        {data.image_url.as_ref().map(|url| view! {
            <img class=style::guide_raster src=url.clone() alt=format!("{} {}",tr(TextKey::TeletextPreviewPage),data.owner.map(|p|p.page.to_string()).unwrap_or_default())/>
        })}
        {publish}
    }.into_any()
}
