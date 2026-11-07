//! Saved channel base configuration; page validation/publication are separate.

use leptos::prelude::*;
use leptos_router::hooks::{use_params_map, use_query_map};

use crate::admin::i18n::{
    teletext_admin_error_text, teletext_profile_availability_text, tr, use_i18n, TextKey,
};
use crate::admin::server_fns::teletext::{
    load_channel_teletext, ChannelTeletextData, TeletextAddonData, TeletextProfileData,
    TeletextSettingData, TeletextSettingValueData,
};
use crate::admin::server_fns::teletext_drafts::{
    load_current_teletext_guide_preview, load_teletext_draft_preview,
    TeletextCurrentGuidePreviewData, TeletextDraftErrorLocation, TeletextDraftPreviewResult,
};
use crate::admin::ui::primitives::button::{btn_class, ButtonVariant};
use crate::admin::ui::primitives::resource_state::{
    admin_loading_resource_view, admin_result_view,
};
use crate::admin::ui::primitives::teletext::{
    island, status_badge, step_bar, TeletextIsland, TeletextStepState, TeletextTone,
};

stylance::import_crate_style!(pub(super) style, "src/admin/pages/channels/teletext.module.css");

#[path = "teletext_readiness.rs"]
mod readiness;

#[component]
pub fn ChannelTeletextPage() -> impl IntoView {
    let params = use_params_map();
    let query = use_query_map();
    let channel_id = move || {
        params.with(|values| {
            values
                .get("id")
                .and_then(|id| id.parse::<i64>().ok())
                .unwrap_or(0)
        })
    };
    let error = query.with_untracked(|values| values.get("error").unwrap_or_default());
    let saved = query.with_untracked(|values| values.get("saved").unwrap_or_default());
    let data = Resource::new_blocking(
        move || {
            (
                channel_id(),
                query.with(super::teletext_drafts::preview_query),
                query.with(super::teletext_current_guide::query),
            )
        },
        load_channel_page,
    );
    view! {
        <Suspense fallback=admin_loading_resource_view>
            {move || {
                let error = error.clone();
                let saved = saved.clone();
                data.get().map(|result| admin_result_view(
                    result.map_err(|_| tr(TextKey::TeletextAdminUnavailable).to_owned()),
                    |data| match data {
                        Some((data, preview, current)) => channel_page(
                            &data, &error, &saved, preview, current,
                            query.with(|values| values.get("design_id").is_some()),
                            query.with(super::teletext_drafts::redirected_error_location),
                        ),
                        None => view! { <p role="alert">{tr(TextKey::NotFound)}</p> }.into_any(),
                    },
                ))
            }}
        </Suspense>
    }
}

type ChannelPageData = (
    ChannelTeletextData,
    Option<Result<TeletextDraftPreviewResult, ServerFnError>>,
    Option<Result<TeletextCurrentGuidePreviewData, ServerFnError>>,
);

async fn load_channel_page(
    (channel_id, preview, current): (
        i64,
        Option<super::teletext_drafts::PreviewQuery>,
        Option<Result<ersatzrs_domain::TeletextPageAddress, ()>>,
    ),
) -> Result<Option<ChannelPageData>, ServerFnError> {
    let Some(channel) = load_channel_teletext(channel_id).await? else {
        return Ok(None);
    };
    // Finish channel observation (and release its admission) before preview.
    // Resolve every blocking load before the in-order SSR boundary is rendered.
    let preview = match preview {
        Some(query) => Some(
            load_teletext_draft_preview(channel_id, query.design_id, query.revision, query.owner)
                .await,
        ),
        None => None,
    };
    let current = match current {
        Some(Ok(owner)) => Some(load_current_teletext_guide_preview(channel_id, owner).await),
        Some(Err(())) => Some(Err(ServerFnError::new("invalid-input"))),
        None => None,
    };
    Ok(Some((channel, preview, current)))
}

#[allow(clippy::too_many_lines)]
fn channel_page(
    data: &ChannelTeletextData,
    error: &str,
    saved: &str,
    preview: Option<Result<TeletextDraftPreviewResult, ServerFnError>>,
    current: Option<Result<TeletextCurrentGuidePreviewData, ServerFnError>>,
    requested: bool,
    redirected: Option<TeletextDraftErrorLocation>,
) -> AnyView {
    let language = use_i18n();
    let draft_code = matches!(
        error,
        "busy"
            | "expired"
            | "missing-policy"
            | "invalid-policy"
            | "invalid-input"
            | "missing-design"
            | "foreign-channel"
            | "worker"
            | "preview"
            | "encoding"
            | "publication-conflict"
            | "editorial-mismatch"
            | "editorial-protected"
            | "editorial-no-changes"
            | "editorial-no-pages"
            | "editorial-syntax"
            | "editorial-missing-receipt"
            | "editorial-limit"
            | "object-copy-invalid"
            | "object-remove-invalid"
            | "secondary-default-invalid"
            | "object-add-invalid"
    );
    let (code, message, role) = if !error.is_empty() {
        if draft_code {
            (
                error,
                super::teletext_drafts::draft_status_text(error),
                "alert",
            )
        } else {
            let code = match error {
                "conflict" | "invalid" | "unavailable" | "wrong-addon" | "missing-profile"
                | "deleted-profile" | "limit" | "storage" | "policy-exists"
                | "scope-unconfirmed" => error,
                _ => "invalid",
            };
            (code, teletext_admin_error_text(language, code), "alert")
        }
    } else if matches!(
        saved,
        "draft" | "publication-requested" | "publication-selected"
    ) {
        (
            saved,
            super::teletext_drafts::draft_status_text(saved),
            "status",
        )
    } else if saved == "capacity" {
        (saved, tr(TextKey::TeletextCapacitySaved), "status")
    } else if matches!(saved, "profile" | "binding" | "off") {
        (saved, tr(TextKey::TeletextAdminSaved), "status")
    } else {
        ("", "", "status")
    };
    let channel_id = data.channel_id;
    let status = data
        .readiness
        .as_ref()
        .map(|readiness| status_line(readiness, &data.addons));
    let readiness = data
        .readiness
        .as_ref()
        .map(|readiness| readiness::view(channel_id, readiness, &data.addons));
    let guide_available = data.current_guide_available == Some(true);
    let guide_unreadable = data.current_guide_available.is_none();
    let current_guide = (guide_available || current.is_some() || guide_unreadable).then(|| {
        super::teletext_current_guide::view(
            channel_id,
            guide_available,
            current.or_else(|| guide_unreadable.then(|| Err(ServerFnError::new("unavailable")))),
        )
    });
    let page_set = page_set_island(data);
    let sources = data
        .addons
        .iter()
        .filter(|addon| source_visible(addon))
        .map(|addon| addon_section(data, addon))
        .collect_view();
    let no_sources = !data.addons.iter().any(source_visible);
    let custom_pages = super::teletext_custom_pages::custom_pages_island(data);
    let publish = publish_island(data, preview, requested, redirected, current_guide);
    view! {
        <div class=style::page data-testid="channel-teletext-page" data-channel-id=channel_id>
            <header class=style::header>
                <a href=format!("/channels/{channel_id}")>{tr(TextKey::EditChannel)}</a>
                <h1>{tr(TextKey::TeletextChannelTitle)}</h1>
                <p>{data.channel_number.clone()}" · "{data.channel_name.clone()}</p>
            </header>
            <p class=style::help>{tr(TextKey::TeletextChannelBaseHelp)}</p>
            <p role="alert" hidden=data.catalog_available>{tr(TextKey::TeletextAdminUnavailable)}</p>
            <p class=style::feedback role=role data-testid="teletext-feedback"
                data-code=code.to_owned() hidden=message.is_empty()>{message}</p>
            {status.map(|status| view! {
                <div class=style::status data-testid="teletext-status">
                    {status}
                    <details class=style::check_details data-testid="teletext-broadcast-check">
                        <summary>{tr(TextKey::TeletextBroadcastCheck)}</summary>
                        {readiness}
                    </details>
                </div>
            })}
            {page_set}
            <div hidden=!no_sources class=style::empty data-testid="teletext-no-sources">
                <p><strong>{tr(TextKey::TeletextChannelEmpty)}</strong></p>
                <a class=style::manage href="/settings/addons">{tr(TextKey::TeletextSourceFixAddon)}</a>
            </div>
            {sources}
            {custom_pages}
            {publish}
        </div>
    }
    .into_any()
}

/// Whether an add-on gets an island: usable add-ons always, unusable ones only
/// while this channel still holds a selection for them.
fn source_visible(addon: &TeletextAddonData) -> bool {
    source_usable(addon) || addon.binding.is_some()
}

fn source_usable(addon: &TeletextAddonData) -> bool {
    addon.installed && addon.availability == "configured" && addon.installation.is_some()
}

fn source_on(addon: &TeletextAddonData) -> bool {
    source_usable(addon)
        && addon
            .binding
            .as_ref()
            .is_some_and(|binding| binding.enabled && binding.availability == "configured")
}

fn addon_name(addon: &TeletextAddonData) -> String {
    if addon.name_labels.is_empty() {
        addon.name.clone()
    } else {
        manifest_label(&addon.name_labels)
    }
}

fn island_anchor(addon: &TeletextAddonData) -> String {
    let id: String = addon
        .addon_id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    format!("teletext-source-{id}")
}

/// The effective value of the first listed setting key for the channel's
/// selected profile: profile override, then installation value, then default.
pub(super) fn effective_setting(addon: &TeletextAddonData, keys: &[&str]) -> Option<String> {
    let profile = addon.binding.as_ref().and_then(|binding| {
        addon
            .profiles
            .iter()
            .find(|profile| profile.id == binding.profile_id)
    });
    keys.iter().find_map(|key| {
        let setting = addon
            .settings
            .iter()
            .find(|setting| setting.definition.key == *key)?;
        profile
            .and_then(|profile| profile.settings.iter().find(|value| value.key == *key))
            .or(setting.inherited.as_ref())
            .filter(|value| value.mode == "literal" && !value.redacted)
            .map(|value| value.value.clone())
            .or_else(|| setting.definition.default.clone())
            .filter(|value| !value.trim().is_empty())
    })
}

/// The saved guide source channels, restricted to channels that still exist.
pub(super) fn effective_guide_selection(data: &ChannelTeletextData) -> Vec<i64> {
    let Some(channels) = data.guide_channels.as_deref() else {
        return Vec::new();
    };
    data.guide_selection
        .as_deref()
        .unwrap_or_default()
        .iter()
        .copied()
        .filter(|id| channels.iter().any(|channel| channel.id == *id))
        .collect()
}

fn page_range(addon: &TeletextAddonData) -> Option<String> {
    let start = effective_setting(addon, &["PAGE_START"])?
        .parse::<u16>()
        .ok()?;
    let end = effective_setting(addon, &["PAGE_END"])
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(start);
    Some(if end > start {
        format!("{start}–{end}")
    } else {
        start.to_string()
    })
}

fn status_line(
    readiness: &crate::admin::server_fns::teletext::TeletextReadinessData,
    addons: &[TeletextAddonData],
) -> AnyView {
    let on = addons.iter().any(source_on);
    let route_ready = matches!(readiness.route.as_str(), "legacy" | "hybrid-builtin");
    let (tone, label, detail, state) = if !on {
        (
            TeletextTone::Neutral,
            TextKey::TeletextStatusNotLive,
            TextKey::TeletextStatusNoSource,
            "no-source",
        )
    } else if !route_ready {
        (
            TeletextTone::Error,
            TextKey::TeletextStatusNotLive,
            TextKey::TeletextStatusRouteBlocked,
            "route",
        )
    } else if readiness.last_failure.is_some() {
        (
            TeletextTone::Error,
            TextKey::TeletextStatusNotLive,
            TextKey::TeletextStatusFailed,
            "failed",
        )
    } else if readiness.published_generation.is_none() {
        (
            TeletextTone::Warning,
            TextKey::TeletextStatusNotLive,
            TextKey::TeletextStatusNotPublished,
            "unpublished",
        )
    } else if readiness.publication_pending {
        (
            TeletextTone::Warning,
            TextKey::TeletextStatusUnpublished,
            TextKey::TeletextStatusPendingDetail,
            "pending",
        )
    } else if readiness.delivering_outputs > 0 {
        (
            TeletextTone::Success,
            TextKey::TeletextStatusLive,
            TextKey::TeletextStatusLiveDetail,
            "live",
        )
    } else {
        (
            TeletextTone::Success,
            TextKey::TeletextStatusPublished,
            TextKey::TeletextStatusPublishedIdle,
            "published",
        )
    };
    view! {
        <p class=style::status_text data-state=state>
            {status_badge(tone, tr(label))}" "{tr(detail)}
        </p>
    }
    .into_any()
}

fn page_set_island(data: &ChannelTeletextData) -> AnyView {
    let mut rows: Vec<AnyView> = data
        .addons
        .iter()
        .filter(|addon| {
            source_visible(addon)
                && addon
                    .binding
                    .as_ref()
                    .is_some_and(|binding| binding.enabled)
        })
        .map(|addon| {
            let on = source_on(addon);
            let pages = if on {
                page_range(addon)
                    .unwrap_or_else(|| tr(TextKey::TeletextPageSetRangeUnknown).to_owned())
            } else {
                "—".to_owned()
            };
            let badge = if on {
                status_badge(TeletextTone::Success, tr(TextKey::TeletextSourceOn))
            } else {
                status_badge(
                    TeletextTone::Error,
                    tr(TextKey::TeletextSourceNeedsAttention),
                )
            };
            view! {
                <tr data-testid="teletext-page-set-row" data-source=addon.addon_id.clone()>
                    <td>{pages}</td>
                    <td><a href=format!("#{}", island_anchor(addon))>{addon_name(addon)}</a>
                        " ("{tr(TextKey::TeletextPageSetAddon)}")"</td>
                    <td>{badge}</td>
                </tr>
            }
            .into_any()
        })
        .collect();
    rows.extend(super::teletext_custom_pages::page_set_rows(data));
    let empty = rows.is_empty();
    let start = data
        .addons
        .iter()
        .find(|addon| source_on(addon))
        .and_then(|addon| effective_setting(addon, &["INDEX_PAGE", "PAGE_START"]));
    let body = if empty {
        view! {
            <div class=style::empty>
                <p><strong>{tr(TextKey::TeletextPageSetEmpty)}</strong></p>
                <p>{tr(TextKey::TeletextPageSetEmptyHelp)}</p>
            </div>
        }
        .into_any()
    } else {
        view! {
            <div class=style::table_wrap>
                <table class=style::page_set>
                    <thead><tr>
                        <th scope="col">{tr(TextKey::TeletextPageSetPages)}</th>
                        <th scope="col">{tr(TextKey::TeletextPageSetSource)}</th>
                        <th scope="col">{tr(TextKey::TeletextPageSetStatus)}</th>
                    </tr></thead>
                    <tbody>{rows}</tbody>
                </table>
            </div>
            {start.map(|start| view! {
                <p class=style::help>{tr(TextKey::TeletextPageSetStart)}" "<strong>{start}</strong></p>
            })}
        }
        .into_any()
    };
    island(TeletextIsland {
        test_id: "teletext-page-set",
        title: tr(TextKey::TeletextPageSetTitle).to_owned(),
        badge: None,
        help: Some(tr(TextKey::TeletextPageSetHelp).to_owned()),
        data_id: None,
        body,
    })
}

fn publish_island(
    data: &ChannelTeletextData,
    preview: Option<Result<TeletextDraftPreviewResult, ServerFnError>>,
    requested: bool,
    redirected: Option<TeletextDraftErrorLocation>,
    current_guide: Option<AnyView>,
) -> AnyView {
    let checked = matches!(&preview, Some(Ok(result)) if result.preview.is_some());
    let selected = matches!(&preview, Some(Ok(result)) if result.preview.as_ref().and_then(|preview| preview.publication.as_ref()).is_some_and(|publication| matches!(publication.state, crate::admin::server_fns::teletext_drafts::TeletextDraftPublicationState::Selected)));
    let steps = if selected {
        vec![
            TeletextStepState::Done,
            TeletextStepState::Done,
            TeletextStepState::Done,
        ]
    } else if checked {
        vec![
            TeletextStepState::Done,
            TeletextStepState::Current,
            TeletextStepState::Todo,
        ]
    } else {
        vec![
            TeletextStepState::Current,
            TeletextStepState::Todo,
            TeletextStepState::Todo,
        ]
    };
    let labels = [
        TextKey::TeletextStepCheck,
        TextKey::TeletextStepPreview,
        TextKey::TeletextStepPublish,
    ];
    let steps = step_bar(
        tr(TextKey::TeletextPublishTitle),
        labels
            .iter()
            .zip(steps)
            .map(|(label, state)| (tr(*label).to_owned(), state))
            .collect(),
    );
    let scripted = data.effective_authoring.is_some().then(|| {
        view! {
            <div class=style::notice role="status" data-testid="teletext-script-notice">
                <p><strong>{tr(TextKey::TeletextScriptNotice)}</strong></p>
            </div>
        }
    });
    let body = view! {
        {steps}
        {super::teletext_drafts::authoring_controls(data)}
        {scripted}
        <super::teletext_drafts::TeletextDraftPreview preview=preview requested=requested redirected=redirected/>
        <details class=style::advanced data-testid="teletext-publish-advanced">
            <summary>{tr(TextKey::TeletextPublishAdvanced)}</summary>
            {super::teletext_drafts::effective_authoring_controls(data)}
            {current_guide}
        </details>
    }
    .into_any();
    island(TeletextIsland {
        test_id: "teletext-publish",
        title: tr(TextKey::TeletextPublishTitle).to_owned(),
        badge: None,
        help: Some(tr(TextKey::TeletextPublishHelp).to_owned()),
        data_id: None,
        body,
    })
}

#[allow(clippy::too_many_lines)]
fn addon_section(data: &ChannelTeletextData, addon: &TeletextAddonData) -> AnyView {
    let channel_id = data.channel_id;
    let language = use_i18n();
    let name = addon_name(addon);
    let installation = addon
        .installation
        .as_ref()
        .and_then(|value| serde_json::to_string(value).ok());
    let writable = addon.installed && addon.availability == "configured" && installation.is_some();
    let addon_id = addon.addon_id.clone();
    let on = source_on(addon);
    let selected = addon.binding.as_ref().map(|binding| binding.profile_id);
    let bound = addon
        .binding
        .as_ref()
        .filter(|binding| binding.enabled)
        .and_then(|binding| {
            addon
                .profiles
                .iter()
                .find(|profile| profile.id == binding.profile_id)
        });
    let usable = source_usable(addon);
    let badge = if !usable {
        let key = if addon.installed {
            TextKey::TeletextSourceDisabled
        } else {
            TextKey::TeletextSourceMissing
        };
        status_badge(TeletextTone::Error, tr(key))
    } else if on {
        status_badge(TeletextTone::Success, tr(TextKey::TeletextSourceOn))
    } else {
        status_badge(TeletextTone::Neutral, tr(TextKey::TeletextSourceOff))
    };
    let off = off_form(channel_id, addon);
    let body = if usable {
        usable_body(data, addon, bound, installation.as_deref(), writable, off)
    } else {
        view! {
            <p class=style::availability data-availability=addon.availability.clone()>
                {teletext_profile_availability_text(language, &addon.availability)}
            </p>
            <p class=style::help>{tr(TextKey::TeletextSourceUnavailableHelp)}</p>
            <div class=style::actions>
                <a class=style::manage href="/settings/addons">{tr(TextKey::TeletextSourceFixAddon)}</a>
                {off}
            </div>
            <details class=style::advanced data-testid="teletext-source-advanced">
                <summary>{tr(TextKey::TeletextSourceAdvanced)}</summary>
                {binding_controls(channel_id, addon, installation.as_deref(), writable)}
            </details>
        }
        .into_any()
    };
    let _ = selected;
    view! {
        <div id=island_anchor(addon) class=style::anchor data-testid="teletext-addon" data-addon-id=addon_id>
            {island(TeletextIsland {
                test_id: "teletext-source",
                title: name,
                badge: Some(badge),
                help: Some(tr(TextKey::TeletextSourceHelp).to_owned()),
                data_id: Some(addon.addon_id.clone()),
                body,
            })}
        </div>
    }
    .into_any()
}

fn usable_body(
    data: &ChannelTeletextData,
    addon: &TeletextAddonData,
    bound: Option<&TeletextProfileData>,
    installation: Option<&str>,
    writable: bool,
    off: Option<AnyView>,
) -> AnyView {
    let channel_id = data.channel_id;
    let primary = match bound {
        Some(profile) => profile_editor(
            channel_id,
            addon,
            Some(profile),
            installation,
            writable,
            true,
            Some(guide_choices(data)),
        ),
        None => enable_form(data, addon, installation.unwrap_or_default()),
    };
    let others = addon
        .profiles
        .iter()
        .filter(|profile| bound.is_none_or(|bound| bound.id != profile.id))
        .map(|profile| {
            profile_editor(
                channel_id,
                addon,
                Some(profile),
                installation,
                writable,
                false,
                None,
            )
        })
        .collect_view();
    let create = profile_editor(channel_id, addon, None, installation, writable, false, None);
    // Choosing between saved settings only makes sense once some exist;
    // a first-time channel sees the single enable action.
    let choice = binding_controls(channel_id, addon, installation, writable);
    let (visible_choice, advanced_choice) = if addon.profiles.is_empty() {
        (None, Some(choice))
    } else {
        (Some(choice), None)
    };
    view! {
        {primary}
        <div class=style::actions>{off}</div>
        {visible_choice}
        <details class=style::advanced data-testid="teletext-source-advanced">
            <summary>{tr(TextKey::TeletextSourceAdvanced)}</summary>
            <p class=style::help>{tr(TextKey::TeletextProfileSharedHelp)}</p>
            {advanced_choice}
            {others}
            {create}
        </details>
    }
    .into_any()
}

fn off_form(channel_id: i64, addon: &TeletextAddonData) -> Option<AnyView> {
    addon.binding.as_ref().filter(|binding| binding.enabled).map(|binding| view! {
        <form method="post" action="/admin-api/teletext/binding/off" data-testid="teletext-disable-form">
            {hidden("channel_id", channel_id.to_string())}
            {hidden("addon_id", addon.addon_id.clone())}
            {hidden("expected_binding_revision", binding.revision.to_string())}
            <button type="submit" class=btn_class(ButtonVariant::Secondary)>{tr(TextKey::TeletextBindingOff)}</button>
        </form>
    }.into_any())
}

fn enable_form(
    data: &ChannelTeletextData,
    addon: &TeletextAddonData,
    installation: &str,
) -> AnyView {
    let revision = addon
        .binding
        .as_ref()
        .map_or_else(String::new, |binding| binding.revision.to_string());
    view! {
        <form method="post" action="/admin-api/teletext/source/enable"
            class=style::enable data-testid="teletext-enable-form">
            {hidden("channel_id", data.channel_id.to_string())}
            {hidden("addon_id", addon.addon_id.clone())}
            {hidden("expected_binding_revision", revision)}
            {hidden("expected_installation", installation.to_owned())}
            {hidden("name", format!("{} · {}", data.channel_number, data.channel_name))}
            <p class=style::help>{tr(TextKey::TeletextSourceEnableHelp)}</p>
            <button type="submit" class=btn_class(ButtonVariant::Primary)>{tr(TextKey::TeletextSourceEnable)}</button>
        </form>
    }
    .into_any()
}

/// Guide source checkboxes saved with the selected settings for this channel.
fn guide_choices(data: &ChannelTeletextData) -> AnyView {
    let Some(channels) = data.guide_channels.as_deref() else {
        return view! {
            <fieldset class=style::setting data-testid="teletext-guide-choices" data-state="unavailable">
                <legend>{tr(TextKey::TeletextGuideSourceTitle)}</legend>
                <p class=style::help>{tr(TextKey::TeletextGuideChannelsUnavailable)}</p>
            </fieldset>
        }
        .into_any();
    };
    let selected = effective_guide_selection(data);
    let boxes = channels
        .iter()
        .map(|channel| {
            let id = format!("teletext-guide-choice-{}", channel.id);
            view! {
                <label class=style::check>
                    <input id=id type="checkbox" name="guide_selection_id" value=channel.id.to_string()
                        checked=selected.contains(&channel.id)/>
                    {format!("{} · {}", channel.number, channel.name)}
                </label>
            }
        })
        .collect_view();
    view! {
        <fieldset class=style::setting data-testid="teletext-guide-choices" data-state="available">
            <legend>{tr(TextKey::TeletextGuideSourceTitle)}</legend>
            {hidden("guide_selection_present", "1".to_owned())}
            <p class=style::help>{tr(TextKey::TeletextGuideSourceHelp)}</p>
            <div class=style::checks>{boxes}</div>
        </fieldset>
    }
    .into_any()
}

fn binding_controls(
    channel_id: i64,
    addon: &TeletextAddonData,
    installation: Option<&str>,
    writable: bool,
) -> AnyView {
    let selected = addon.binding.as_ref().map(|binding| binding.profile_id);
    let missing = selected.is_some_and(|id| !addon.profiles.iter().any(|profile| profile.id == id));
    let revision = addon
        .binding
        .as_ref()
        .map_or_else(String::new, |value| value.revision.to_string());
    let options = addon
        .profiles
        .iter()
        .map(|profile| {
            let valid = !profile.deleted && profile.availability == "configured";
            let label = format!(
                "{} · {}",
                profile.name,
                teletext_profile_availability_text(use_i18n(), &profile.availability)
            );
            view! {
                <option value=format!("{}:{}", profile.id, profile.revision)
                    selected=selected == Some(profile.id) disabled=!valid>{label}</option>
            }
        })
        .collect_view();
    let choice_id = format!("teletext-choice-{}", addon.addon_id);
    view! {
        <div class=style::binding>
            <form method="post" action="/admin-api/teletext/binding/save"
                class=style::binding_form data-testid="teletext-binding-form">
                {hidden("channel_id", channel_id.to_string())}
                {hidden("addon_id", addon.addon_id.clone())}
                {hidden("expected_binding_revision", revision)}
                {hidden("expected_installation", installation.unwrap_or_default().to_owned())}
                <label for=choice_id.clone()>{tr(TextKey::TeletextBindingProfile)}</label>
                <div class=style::inline>
                    <select id=choice_id name="profile_choice" required disabled=!writable>
                        <option value="" selected=selected.is_none()>{tr(TextKey::TeletextBindingNone)}</option>
                        {missing.then(|| view! { <option value="" selected disabled>{tr(TextKey::ProfileNotFound)}</option> })}
                        {options}
                    </select>
                    <button type="submit" class=btn_class(ButtonVariant::Secondary) disabled=!writable>
                        {tr(TextKey::TeletextBindingSave)}
                    </button>
                </div>
            </form>
        </div>
    }
    .into_any()
}

fn profile_editor(
    channel_id: i64,
    addon: &TeletextAddonData,
    profile: Option<&TeletextProfileData>,
    installation: Option<&str>,
    writable: bool,
    open: bool,
    extra: Option<AnyView>,
) -> AnyView {
    let id = profile.map_or_else(String::new, |profile| profile.id.to_string());
    let revision = profile.map_or_else(String::new, |profile| profile.revision.to_string());
    let name = profile.map_or_else(String::new, |profile| profile.name.clone());
    let title = match profile {
        None => tr(TextKey::TeletextProfileCreate).to_owned(),
        Some(profile) if extra.is_some() => {
            format!("{} · {}", tr(TextKey::TeletextSourceSettings), profile.name)
        }
        Some(profile) => profile.name.clone(),
    };
    let needs_repair = profile.is_some_and(|profile| unsupported_profile_settings(addon, profile));
    let available = writable && !needs_repair && !profile.is_some_and(|profile| profile.deleted);
    let name_id = format!("teletext-name-{}-{id}", addon.addon_id);
    let settings = addon
        .settings
        .iter()
        .enumerate()
        .map(|(index, setting)| {
            let stored = profile.and_then(|profile| {
                profile
                    .settings
                    .iter()
                    .find(|value| value.key == setting.definition.key)
            });
            setting_fields(&addon.addon_id, &id, index, setting, stored)
        })
        .collect_view();
    view! {
        <details class=style::profile open=open data-testid="teletext-profile" data-profile-id=id.clone()>
            <summary>{title}</summary>
            {profile.map(|profile| view! {
                <p class=style::availability data-availability=profile.availability.clone()>
                    {teletext_profile_availability_text(use_i18n(), &profile.availability)}
                </p>
            })}
            <p class=style::help role="status" hidden=!needs_repair>{tr(TextKey::TeletextProfileRepairHelp)}</p>
            <form method="post" action="/admin-api/teletext/profile/save"
                data-testid="teletext-profile-form" data-profile-id=id.clone()>
                {hidden("channel_id", channel_id.to_string())}
                {hidden("addon_id", addon.addon_id.clone())}
                {hidden("profile_id", id.clone())}
                {hidden("expected_profile_revision", revision)}
                {hidden("expected_installation", installation.unwrap_or_default().to_owned())}
                {hidden("setting_count", addon.settings.len().to_string())}
                <fieldset class=style::profile_fields disabled=!available>
                    <legend>{tr(TextKey::TeletextProfileName)}</legend>
                    <label for=name_id.clone()>{tr(TextKey::TeletextProfileName)}</label>
                    <input id=name_id name="name" value=name required maxlength="256"/>
                    <p class=style::help>{tr(TextKey::TeletextProfileInheritHelp)}</p>
                    <div class=style::settings_grid>{settings}</div>
                    {extra}
                    <button type="submit" class=btn_class(ButtonVariant::Primary)>
                        {if profile.is_some() { tr(TextKey::TeletextProfileSave) } else { tr(TextKey::TeletextProfileCreate) }}
                    </button>
                </fieldset>
            </form>
        </details>
    }
    .into_any()
}

fn setting_fields(
    addon_id: &str,
    profile_id: &str,
    index: usize,
    setting: &TeletextSettingData,
    stored: Option<&TeletextSettingValueData>,
) -> AnyView {
    use ersatzrs_addon_contract::AddonSettingKind;
    let definition = &setting.definition;
    let prefix = format!("setting.{index}");
    let id = format!("teletext-{addon_id}-{profile_id}-{index}");
    let mode_id = format!("{id}-mode");
    let secret = definition.kind == AddonSettingKind::SecretReference;
    let redacted = stored.is_some_and(|value| value.redacted);
    let override_mode = if secret { "reference" } else { "literal" };
    let controls = if secret {
        secret_fields(&id, &prefix, stored.or(setting.inherited.as_ref()))
    } else {
        literal_field(&id, &prefix, setting, stored)
    };
    view! {
        <fieldset class=style::setting data-testid="teletext-profile-setting" data-setting-key=definition.key.clone()>
            <legend>{manifest_label(&definition.label)}</legend>
            {hidden(&format!("{prefix}.key"), definition.key.clone())}
            <label for=mode_id.clone()>{tr(TextKey::TeletextSettingSource)}</label>
            <select id=mode_id name=format!("{prefix}.mode")>
                <option value="inherit" selected=stored.is_none()>{tr(TextKey::TeletextSettingDefault)}</option>
                <option value=override_mode selected=stored.is_some()>{tr(TextKey::TeletextSettingCustom)}</option>
            </select>
            <p class=style::help hidden=!redacted>{tr(TextKey::TeletextProfileRedacted)}</p>
            {controls}
        </fieldset>
    }
    .into_any()
}

fn literal_field(
    id: &str,
    prefix: &str,
    setting: &TeletextSettingData,
    stored: Option<&TeletextSettingValueData>,
) -> AnyView {
    use ersatzrs_addon_contract::AddonSettingKind;
    let value = stored
        .or(setting.inherited.as_ref())
        .filter(|value| !value.redacted)
        .map(|value| value.value.clone())
        .or_else(|| setting.definition.default.clone())
        .unwrap_or_default();
    let name = format!("{prefix}.value");
    let control = match setting.definition.kind {
        AddonSettingKind::Boolean | AddonSettingKind::Enum => {
            let choices = if setting.definition.kind == AddonSettingKind::Boolean {
                vec![
                    ("false".to_owned(), tr(TextKey::No).to_owned()),
                    ("true".to_owned(), tr(TextKey::Yes).to_owned()),
                ]
            } else {
                setting
                    .definition
                    .options
                    .iter()
                    .map(|value| (value.clone(), value.clone()))
                    .collect()
            };
            let unknown = !choices.iter().any(|(choice, _)| choice == &value);
            view! {
                <select id=id.to_owned() name=name>
                    {unknown.then(|| view! { <option value=value.clone() selected>{value.clone()}</option> })}
                    {choices.into_iter().map(|(choice, label)| view! {
                        <option selected=choice == value value=choice>{label}</option>
                    }).collect_view()}
                </select>
            }.into_any()
        }
        AddonSettingKind::Integer => view! {
            <input id=id.to_owned() name=name value=value inputmode="numeric"/>
        }
        .into_any(),
        AddonSettingKind::String => {
            // Tachys treats textarea children as raw SSR text. Match the local
            // library form's explicit encoding so content cannot close the tag.
            let content = textarea_content(&value);
            view! {
                <textarea id=id.to_owned() name=name rows="2" inner_html=content></textarea>
            }
            .into_any()
        }
        _ => view! { <input id=id.to_owned() name=name value=value/> }.into_any(),
    };
    view! {
        <label for=id.to_owned()>{tr(TextKey::TeletextProfileValue)}</label>
        {control}
        {hidden(&format!("{prefix}.reference_kind"), String::new())}
        {hidden(&format!("{prefix}.reference"), String::new())}
    }
    .into_any()
}

fn secret_fields(id: &str, prefix: &str, stored: Option<&TeletextSettingValueData>) -> AnyView {
    let reference = stored
        .filter(|value| !value.redacted)
        .map_or_else(String::new, |value| value.reference.clone());
    let kind = stored.map_or("environment", |value| value.reference_kind.as_str());
    let kind_id = format!("{id}-kind");
    view! {
        {hidden(&format!("{prefix}.value"), String::new())}
        <label for=kind_id.clone()>{tr(TextKey::Type)}</label>
        <select id=kind_id name=format!("{prefix}.reference_kind")>
            <option value="environment" selected=kind != "file">{tr(TextKey::EnvironmentVariable)}</option>
            <option value="file" selected=kind == "file">{tr(TextKey::SecretFile)}</option>
        </select>
        <label for=id.to_owned()>{tr(TextKey::TeletextProfileReference)}</label>
        <input id=id.to_owned() name=format!("{prefix}.reference") value=reference autocomplete="off"/>
    }.into_any()
}

fn hidden(name: &str, value: String) -> AnyView {
    view! { <input type="hidden" name=name.to_owned() value=value/> }.into_any()
}

fn textarea_content(value: &str) -> String {
    let encoded = value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    // HTML parsing normalizes CR/CRLF to LF, then ignores the first LF after
    // a textarea start tag. Preserve that first line for every newline form.
    if value.starts_with(['\n', '\r']) {
        format!("\n{encoded}")
    } else {
        encoded
    }
}

fn unsupported_profile_settings(addon: &TeletextAddonData, profile: &TeletextProfileData) -> bool {
    use ersatzrs_addon_contract::AddonSettingKind;
    profile.settings.iter().any(|value| {
        value.redacted
            || addon
                .settings
                .iter()
                .find(|setting| setting.definition.key == value.key)
                .is_none_or(|setting| {
                    let expected_mode =
                        if setting.definition.kind == AddonSettingKind::SecretReference {
                            "reference"
                        } else {
                            "literal"
                        };
                    value.mode != expected_mode
                })
    })
}

pub(super) fn manifest_label(labels: &std::collections::BTreeMap<String, String>) -> String {
    labels
        .iter()
        .find(|(code, _)| code.eq_ignore_ascii_case(use_i18n().code()))
        .or_else(|| {
            labels
                .iter()
                .find(|(code, _)| code.eq_ignore_ascii_case("en-US"))
        })
        .map_or_else(String::new, |(_, label)| label.clone())
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;
    use crate::admin::server_fns::teletext::{TeletextBindingData, TeletextInstallationData};
    use ersatzrs_addon_contract::{AddonSettingDefinition, AddonSettingKind};

    fn addon() -> TeletextAddonData {
        TeletextAddonData {
            addon_id: "synthetic.pages".to_owned(),
            name: "Synthetic <pages>".to_owned(),
            name_labels: std::collections::BTreeMap::new(),
            installed: true,
            availability: "configured".to_owned(),
            installation: Some(TeletextInstallationData {
                addon_id: "synthetic.pages".to_owned(),
                state_revision: "revision-1".to_owned(),
                version: "1.0.0".to_owned(),
                package_sha256: "a".repeat(64),
                content_sha256: "b".repeat(64),
                manifest_sha256: "c".repeat(64),
            }),
            settings: vec![TeletextSettingData {
                definition: AddonSettingDefinition {
                    key: "LABEL".to_owned(),
                    label: [("en-US".to_owned(), "Label".to_owned())].into(),
                    kind: AddonSettingKind::String,
                    required: false,
                    default: Some("default label".to_owned()),
                    options: Vec::new(),
                    suggested_reference: None,
                },
                inherited: None,
            }],
            profiles: Vec::new(),
            binding: Some(TeletextBindingData {
                profile_id: 12,
                enabled: true,
                revision: 7,
                availability: "configured".to_owned(),
            }),
        }
    }

    fn profile(key: &str, value: &str) -> TeletextProfileData {
        TeletextProfileData {
            id: 12,
            name: "Night".to_owned(),
            revision: 3,
            deleted: false,
            availability: "configured".to_owned(),
            settings: vec![TeletextSettingValueData {
                key: key.to_owned(),
                mode: "literal".to_owned(),
                value: value.to_owned(),
                reference_kind: String::new(),
                reference: String::new(),
                redacted: false,
            }],
        }
    }

    #[test]
    fn secondary_default_error_reaches_the_localized_channel_feedback() {
        let owner = Owner::new();
        let data = ChannelTeletextData {
            channel_id: 44,
            channel_number: "1".into(),
            channel_name: "Synthetic".into(),
            catalog_available: true,
            authoring_selection: None,
            authoring_error: None,
            effective_authoring: None,
            effective_authoring_error: None,
            guide_channels: Some(Vec::new()),
            guide_selection: None,
            current_guide_available: Some(false),
            addons: Vec::new(),
            readiness: None,
        };
        let html = owner.with(|| {
            channel_page(
                &data,
                "secondary-default-invalid",
                "",
                None,
                None,
                false,
                None,
            )
            .to_html()
        });
        assert!(html.contains("data-code=\"secondary-default-invalid\""));
        assert!(html.contains(tr(TextKey::TeletextSecondaryDefaultInvalid)));
    }

    fn channel(addons: Vec<TeletextAddonData>) -> ChannelTeletextData {
        ChannelTeletextData {
            channel_id: 44,
            channel_number: "1".into(),
            channel_name: "Synthetic".into(),
            catalog_available: true,
            authoring_selection: None,
            authoring_error: None,
            effective_authoring: None,
            effective_authoring_error: None,
            guide_channels: Some(vec![
                crate::admin::server_fns::teletext::TeletextGuideChannelData {
                    id: 44,
                    number: "1".into(),
                    name: "Synthetic".into(),
                },
                crate::admin::server_fns::teletext::TeletextGuideChannelData {
                    id: 45,
                    number: "2".into(),
                    name: "Other".into(),
                },
            ]),
            guide_selection: Some(vec![45, 99]),
            current_guide_available: Some(false),
            addons,
            readiness: None,
        }
    }

    #[test]
    fn native_selection_holds_profile_revision_and_broken_off_needs_no_installation() {
        let mut data = addon();
        data.profiles.push(profile("LABEL", "night"));
        let owner = Owner::new();
        let html = owner.with(|| addon_section(&channel(Vec::new()), &data).to_html());
        assert!(html.contains("value=\"12:3\" selected"));
        assert!(html.contains("name=\"expected_binding_revision\" value=\"7\""));
        assert!(html.contains("Synthetic &lt;pages&gt;"));
        data.installed = false;
        data.installation = None;
        data.profiles.clear();
        let html = owner.with(|| addon_section(&channel(Vec::new()), &data).to_html());
        assert!(html.contains(tr(TextKey::ProfileNotFound)));
        assert!(html.contains(tr(TextKey::TeletextSourceUnavailableHelp)));
        assert!(!html.contains("teletext-profile-form"));
        let off = html
            .split("action=\"/admin-api/teletext/binding/off\"")
            .nth(1)
            .unwrap()
            .split("</form>")
            .next()
            .unwrap();
        assert!(off.contains("name=\"expected_binding_revision\" value=\"7\""));
        assert!(!off.contains("expected_installation"));
        assert!(!off.contains("disabled"));
    }

    #[test]
    fn profile_form_preserves_empty_override_and_escapes_multiline_content() {
        let data = addon();
        let owner = Owner::new();
        for (value, expected) in [
            ("", ""),
            ("A & B &lt; C", "A &amp; B &amp;lt; C"),
            (
                "First\n<script>second</script>",
                "First\n&lt;script&gt;second&lt;/script&gt;",
            ),
            ("\nFirst\nSecond", "\n\nFirst\nSecond"),
            ("\r\nFirst\r\nSecond", "\n\r\nFirst\r\nSecond"),
            ("\rFirst\rSecond", "\n\rFirst\rSecond"),
            (
                "</textarea><script>synthetic()</script>&",
                "&lt;/textarea&gt;&lt;script&gt;synthetic()&lt;/script&gt;&amp;",
            ),
        ] {
            let profile = profile("LABEL", value);
            let html = owner.with(|| {
                profile_editor(44, &data, Some(&profile), Some("{}"), true, true, None).to_html()
            });
            assert!(html.contains("value=\"literal\" selected"));
            assert!(html.contains("name=\"setting.0.value\""));
            assert!(!html.contains("<script>"));
            let content = html
                .split("<textarea")
                .nth(1)
                .unwrap()
                .split_once('>')
                .unwrap()
                .1
                .split("</textarea>")
                .next()
                .unwrap();
            assert_eq!(content, expected);
            assert_eq!(html.matches("<textarea").count(), 1);
            assert_eq!(html.matches("</textarea>").count(), 1);
            if value.starts_with("</textarea>") {
                println!("synthetic textarea SSR content: {content}");
            }
            let fieldset = html
                .split("<fieldset")
                .nth(1)
                .unwrap()
                .split('>')
                .next()
                .unwrap();
            assert!(!fieldset.contains("disabled"));
        }
    }

    #[test]
    fn unsupported_override_blocks_edit_without_exposing_redacted_values() {
        let data = addon();
        let owner = Owner::new();
        for (key, redacted, mode) in [
            ("OLD_KEY", false, "literal"),
            ("LABEL", true, "literal"),
            ("LABEL", false, "reference"),
        ] {
            let mut profile = profile(key, "synthetic-private-secret");
            profile.settings[0].redacted = redacted;
            profile.settings[0].mode = mode.to_owned();
            let html = owner.with(|| {
                profile_editor(44, &data, Some(&profile), Some("{}"), true, true, None).to_html()
            });
            let fieldset = html
                .split("<fieldset")
                .nth(1)
                .unwrap()
                .split('>')
                .next()
                .unwrap();
            assert!(fieldset.contains("disabled"));
            assert!(html.contains(tr(TextKey::TeletextProfileRepairHelp)));
            if redacted || key == "OLD_KEY" {
                assert!(!html.contains("synthetic-private-secret"));
            }
        }
    }

    #[test]
    fn source_islands_follow_add_on_and_channel_state() {
        let owner = Owner::new();
        // Enabled selection: the bound settings open with guide choices, and
        // the page set lists the add-on's effective page range.
        let mut on = addon();
        on.profiles.push(profile("LABEL", "night"));
        on.settings.push(TeletextSettingData {
            definition: AddonSettingDefinition {
                key: "PAGE_START".to_owned(),
                label: [("en-US".to_owned(), "First".to_owned())].into(),
                kind: AddonSettingKind::Integer,
                required: false,
                default: Some("200".to_owned()),
                options: Vec::new(),
                suggested_reference: None,
            },
            inherited: None,
        });
        let data = channel(vec![on.clone()]);
        let html = owner.with(|| addon_section(&data, &on).to_html());
        assert!(html.contains(tr(TextKey::TeletextSourceOn)));
        assert!(html.contains("name=\"guide_selection_present\" value=\"1\""));
        // Saved choices that no longer exist are dropped; existing ones stay.
        assert!(html.contains("value=\"45\" checked"));
        assert!(!html.contains("value=\"99\""));
        assert!(html.contains("teletext-disable-form"));
        assert!(!html.contains("teletext-enable-form"));
        let set = owner.with(|| page_set_island(&data).to_html());
        assert!(set.contains("teletext-page-set-row"));
        assert!(set.contains(">200<"));
        // No selection yet: one enable action with the channel-named set.
        let mut off = addon();
        off.binding = None;
        let html = owner.with(|| addon_section(&channel(vec![off.clone()]), &off).to_html());
        assert!(html.contains(tr(TextKey::TeletextSourceOff)));
        assert!(html.contains("action=\"/admin-api/teletext/source/enable\""));
        assert!(html.contains("name=\"name\" value=\"1 · Synthetic\""));
        assert!(!html.contains("teletext-disable-form"));
        let set = owner.with(|| page_set_island(&channel(vec![off.clone()])).to_html());
        assert!(set.contains(tr(TextKey::TeletextPageSetEmpty)));
        // A disabled add-on without a selection gets no island at all.
        off.availability = "disabled-installation".to_owned();
        assert!(!source_visible(&off));
    }

    #[test]
    fn status_line_names_the_first_blocking_reason() {
        use crate::admin::server_fns::teletext::TeletextReadinessData;
        let owner = Owner::new();
        let mut readiness = TeletextReadinessData {
            authoring_policy: "profile".into(),
            output_policy: "profile".into(),
            capacity_profile: None,
            profiles: Vec::new(),
            route: "legacy".into(),
            script: None,
            published_generation: None,
            publication_pending: false,
            delivering_outputs: 0,
            active_outputs: 0,
            last_failure: None,
            last_failure_at: None,
        };
        let state = |readiness: &TeletextReadinessData, addons: &[TeletextAddonData]| {
            let html = owner.with(|| status_line(readiness, addons).to_html());
            html.split("data-state=\"")
                .nth(1)
                .unwrap()
                .split('"')
                .next()
                .unwrap()
                .to_owned()
        };
        assert_eq!(state(&readiness, &[]), "no-source");
        let on = [addon()];
        assert_eq!(state(&readiness, &on), "unpublished");
        readiness.route = "hls".into();
        assert_eq!(state(&readiness, &on), "route");
        readiness.route = "hybrid-builtin".into();
        readiness.published_generation = Some(3);
        assert_eq!(state(&readiness, &on), "published");
        readiness.publication_pending = true;
        assert_eq!(state(&readiness, &on), "pending");
        readiness.publication_pending = false;
        readiness.delivering_outputs = 1;
        assert_eq!(state(&readiness, &on), "live");
        readiness.last_failure = Some("deadline".into());
        assert_eq!(state(&readiness, &on), "failed");
    }
}
