//! One ordered readiness checklist for a channel's teletext.
//!
//! Every row is a separately observed fact. Enabled, validated, published and
//! broadcast stay distinct, and nothing here claims display in a client. The
//! only control is the explicit, server-validated capacity profile form; it is a
//! plain HTML form so it works identically with and without hydration.

use leptos::prelude::*;

use crate::admin::i18n::{tr, TextKey};
use crate::admin::server_fns::teletext::{TeletextAddonData, TeletextReadinessData};
use crate::admin::ui::primitives::button::{btn_class, ButtonVariant};

use super::style;

const INLINE_CHECK: &str = "width:auto;min-height:auto;margin-inline-end:0.5rem";

pub(super) fn view(
    channel_id: i64,
    readiness: &TeletextReadinessData,
    addons: &[TeletextAddonData],
    custom_pages_on: bool,
) -> AnyView {
    let addon_enabled = addons
        .iter()
        .filter_map(|addon| addon.binding.as_ref())
        .any(|binding| binding.enabled);
    // Switched-on custom pages are a complete source without add-on settings.
    let enabled = addon_enabled || custom_pages_on;
    let profile = addons
        .iter()
        .filter_map(|addon| addon.binding.as_ref())
        .any(|binding| binding.enabled && binding.availability == "configured")
        || (custom_pages_on && !addon_enabled);
    let capacity_ready = readiness.authoring_policy != "absent"
        && readiness.output_policy != "absent"
        && !matches!(
            readiness.authoring_policy.as_str(),
            "invalid" | "unreadable"
        )
        && !matches!(readiness.output_policy.as_str(), "invalid" | "unreadable");
    let route_ready = matches!(readiness.route.as_str(), "legacy" | "hybrid-builtin");
    let published = readiness.published_generation.is_some();
    let broadcast = readiness.delivering_outputs > 0;
    view! {
        <section class=style::addon data-testid="teletext-readiness">
            <h2>{tr(TextKey::TeletextReadinessTitle)}</h2>
            <p class=style::help>{tr(TextKey::TeletextReadinessHelp)}</p>
            <ol>
                {step("addon", enabled, TextKey::TeletextReadinessAddon,
                    (!enabled).then(|| help(TextKey::TeletextReadinessAddonHelp)))}
                {step("profile", profile, TextKey::TeletextChannelProfiles,
                    (!profile).then(|| help(TextKey::TeletextReadinessAddonHelp)))}
                {step("capacity", capacity_ready, TextKey::TeletextReadinessCapacity,
                    Some(capacity(channel_id, readiness)))}
                {step("route", route_ready, TextKey::TeletextReadinessRoute, Some(route(channel_id, readiness)))}
                {step("published", published, TextKey::TeletextReadinessPublished, Some(publication(readiness)))}
                {step("broadcast", broadcast, TextKey::TeletextReadinessBroadcast, Some(delivery(readiness)))}
            </ol>
        </section>
    }
    .into_any()
}

fn help(key: TextKey) -> AnyView {
    view! { <p class=style::help>{tr(key)}</p> }.into_any()
}

fn step(name: &'static str, done: bool, label: TextKey, detail: Option<AnyView>) -> AnyView {
    let state = if done { "done" } else { "missing" };
    let status = tr(if done {
        TextKey::TeletextReadinessDone
    } else {
        TextKey::TeletextReadinessMissing
    });
    view! {
        <li data-testid="teletext-readiness-step" data-step=name data-state=state>
            <strong>{tr(label)}</strong>" · "<span role="status">{status}</span>
            {detail}
        </li>
    }
    .into_any()
}

fn capacity(channel_id: i64, readiness: &TeletextReadinessData) -> AnyView {
    let states = [
        readiness.authoring_policy.as_str(),
        readiness.output_policy.as_str(),
    ];
    let summary = if let Some(id) = &readiness.capacity_profile {
        view! {
            <p class=style::help data-capacity-profile=id.clone()>
                {tr(TextKey::TeletextCapacityActive)}" "{profile_label(id)}
            </p>
        }
        .into_any()
    } else if states
        .iter()
        .any(|state| matches!(*state, "invalid" | "unreadable"))
    {
        help(TextKey::TeletextCapacityInvalid)
    } else if states.contains(&"absent") {
        help(TextKey::TeletextCapacityAbsent)
    } else {
        help(TextKey::TeletextCapacityCustom)
    };
    let custom = states.contains(&"custom");
    let options = readiness
        .profiles
        .iter()
        .map(|profile| {
            let selected = readiness.capacity_profile.as_deref() == Some(profile.id.as_str());
            view! {
                <option value=profile.id.clone() selected=selected data-max-pages=profile.max_pages>
                    {profile_label(&profile.id)}
                </option>
            }
        })
        .collect_view();
    let replace = custom.then(|| {
        view! {
            <label>
                <input type="checkbox" name="replace" value="1" style=INLINE_CHECK/>
                {tr(TextKey::TeletextCapacityReplace)}
            </label>
        }
    });
    view! {
        <div data-testid="teletext-capacity" data-authoring=states[0] data-output=states[1]>
            {summary}
            <form method="post" action="/admin-api/teletext/capacity/apply"
                class=style::binding_form data-testid="teletext-capacity-form">
                <input type="hidden" name="channel_id" value=channel_id.to_string()/>
                <label for="teletext-capacity-profile">{tr(TextKey::TeletextReadinessCapacity)}</label>
                <select id="teletext-capacity-profile" name="profile">{options}</select>
                <p class=style::help>{tr(TextKey::TeletextCapacityApplyHelp)}</p>
                <label>
                    <input type="checkbox" name="confirm_scope" value="1" required style=INLINE_CHECK/>
                    {tr(TextKey::TeletextCapacityConfirmScope)}
                </label>
                {replace}
                <button type="submit" class=btn_class(ButtonVariant::Primary)>
                    {tr(TextKey::TeletextCapacityApply)}
                </button>
            </form>
        </div>
    }
    .into_any()
}

fn profile_label(id: &str) -> String {
    match id {
        "tvguide-16" => tr(TextKey::TeletextCapacityProfileTvguide16).to_owned(),
        other => other.to_owned(),
    }
}

fn route(channel_id: i64, readiness: &TeletextReadinessData) -> AnyView {
    let explanation = match readiness.route.as_str() {
        "legacy" => TextKey::TeletextRouteLegacy,
        "hybrid-builtin" => TextKey::TeletextRouteHybridBuiltin,
        "hybrid-script" => TextKey::TeletextRouteHybridScript,
        _ => TextKey::TeletextRouteHls,
    };
    let script = readiness.script.clone().map(|script| {
        view! {
            <p class=style::help>
                {tr(TextKey::TeletextRouteScript)}": "<code data-testid="teletext-route-script">{script}</code>
            </p>
        }
    });
    let fixes = (!matches!(readiness.route.as_str(), "legacy" | "hybrid-builtin")).then(|| {
        view! {
            <p class=style::help>
                <a href=format!("/channels/{channel_id}")>{tr(TextKey::TeletextRouteFixChannel)}</a>
            </p>
            <p class=style::help hidden=readiness.route == "hls">
                <a href="/settings/ffmpeg">{tr(TextKey::TeletextRouteFixGlobal)}</a>
            </p>
        }
    });
    view! {
        <div data-testid="teletext-route" data-route=readiness.route.clone()>
            {help(explanation)}
            {script}
            {fixes}
        </div>
    }
    .into_any()
}

fn publication(readiness: &TeletextReadinessData) -> AnyView {
    match readiness.published_generation {
        Some(generation) => view! {
            <p class=style::help data-published-generation=generation>
                {tr(TextKey::TeletextDeliveryGeneration)}" "{generation}
            </p>
        }
        .into_any(),
        None if readiness.publication_pending => help(TextKey::TeletextReadinessPending),
        None => help(TextKey::TeletextReadinessUnpublished),
    }
}

fn delivery(readiness: &TeletextReadinessData) -> AnyView {
    let outputs = if readiness.active_outputs == 0 {
        help(TextKey::TeletextReadinessNoOutputs)
    } else {
        view! {
            <p class=style::help data-delivering=readiness.delivering_outputs>
                {tr(TextKey::TeletextReadinessDelivering)}": "
                {format!("{} / {}", readiness.delivering_outputs, readiness.active_outputs)}
            </p>
        }
        .into_any()
    };
    let failure = readiness.last_failure.clone().map(|kind| {
        let advice = match kind.as_str() {
            "transport_capacity" => TextKey::TeletextFailureCapacity,
            "deadline" => TextKey::TeletextFailureDeadline,
            "source" => TextKey::TeletextFailureSource,
            _ => TextKey::TeletextFailureTransport,
        };
        let at = readiness.last_failure_at.clone().unwrap_or_default();
        view! {
            <div role="alert" data-testid="teletext-transport-failure" data-failure=kind>
                <p class=style::help>
                    {tr(TextKey::TeletextReadinessFailed)}" "<time datetime=at.clone()>{at.clone()}</time>"."
                </p>
                {help(advice)}
            </div>
        }
    });
    view! { {outputs}{failure} }.into_any()
}
