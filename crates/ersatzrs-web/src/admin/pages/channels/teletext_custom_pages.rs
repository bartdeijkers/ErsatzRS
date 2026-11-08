//! Custom pages island on the channel teletext page.
//!
//! Custom pages come from the teletext element library (`/teletext-elements`).
//! This island lists the elements linked to the channel, lets the user change
//! their page number or switch them off, and links new ones. The page set
//! gets one row per enabled link.

use leptos::prelude::*;

use crate::admin::i18n::{tr, TextKey};
use crate::admin::server_fns::teletext::{
    ChannelTeletextData, TeletextCustomPageData, TeletextCustomPagesData,
};
use crate::admin::ui::primitives::button::{btn_class, ButtonVariant};
use crate::admin::ui::primitives::teletext::{island, status_badge, TeletextIsland, TeletextTone};

use super::teletext::{addon_name, style};

stylance::import_crate_style!(
    custom_style,
    "src/admin/pages/channels/teletext_custom_pages.module.css"
);

/// Anchor of the custom pages island; forms return here.
const ANCHOR: &str = "custom-pages";

/// Feedback for the custom page endpoints, or `None` for other codes.
pub(super) fn feedback(error: &str, saved: &str) -> Option<(&'static str, &'static str)> {
    if let Some(code) = error.strip_prefix("custom-page-") {
        let text = tr(match code {
            "element" | "not-found" => TextKey::TeletextElementChannelErrorElement,
            "page-number" => TextKey::TeletextElementErrorPageNumber,
            "page-in-use" => TextKey::TeletextElementChannelErrorPageInUse,
            _ => TextKey::ErrorOccurred,
        });
        return Some((text, "alert"));
    }
    match saved {
        "custom-page" => Some((tr(TextKey::TeletextElementChannelSaved), "status")),
        "custom-page-removed" => Some((tr(TextKey::TeletextElementChannelRemoved), "status")),
        _ => None,
    }
}

/// Why an enabled custom page cannot go out with the next Check, in words.
/// The server computes the reason with the same plan the host producer runs.
fn issue_text(data: &ChannelTeletextData, link: &TeletextCustomPageData) -> Option<String> {
    let code = link.issue.as_deref()?;
    Some(match code {
        "addon-range" => {
            let addon = link.issue_addon.as_deref().unwrap_or_default();
            let name = data
                .addons
                .iter()
                .find(|candidate| candidate.addon_id == addon)
                .map_or_else(|| addon.to_owned(), addon_name);
            tr(TextKey::TeletextElementChannelClash).replace("{addon}", &name)
        }
        "page-number" => tr(TextKey::TeletextCustomPageIssuePageNumber).to_owned(),
        "unset-field" => tr(TextKey::TeletextCustomPageIssueUnsetField).to_owned(),
        "duplicate-page" => tr(TextKey::TeletextElementChannelErrorPageInUse).to_owned(),
        "double-height" => tr(TextKey::TeletextCustomPageIssueDoubleHeight).to_owned(),
        _ => tr(TextKey::TeletextCustomPageIssueUnreadable).to_owned(),
    })
}

/// Whether any linked custom page is switched on, so the host source exists.
pub(super) fn custom_pages_on(data: &ChannelTeletextData) -> bool {
    data.custom_pages
        .as_ref()
        .is_some_and(|custom| custom.links.iter().any(|link| link.enabled))
}

/// Lowest switched-on custom page number, the entry page when custom pages
/// are the only source.
pub(super) fn first_custom_page(data: &ChannelTeletextData) -> Option<u16> {
    data.custom_pages
        .as_ref()?
        .links
        .iter()
        .filter(|link| link.enabled)
        .filter_map(|link| link.page.parse::<u16>().ok())
        .filter(|page| (100..=899).contains(page))
        .min()
}

/// Page-set rows contributed by custom pages linked to this channel.
pub(super) fn page_set_rows(data: &ChannelTeletextData) -> Vec<AnyView> {
    let Some(custom) = data.custom_pages.as_ref() else {
        return Vec::new();
    };
    custom
        .links
        .iter()
        .filter(|link| link.enabled)
        .map(|link| {
            let badge = match issue_text(data, link) {
                Some(reason) => status_badge(TeletextTone::Error, reason),
                None => status_badge(
                    TeletextTone::Success,
                    tr(TextKey::TeletextElementChannelLinked),
                ),
            };
            view! {
                <tr data-testid="teletext-page-set-row" data-source="custom-page"
                    data-element-id=link.element_id.to_string()>
                    <td>{link.page.clone()}</td>
                    <td><a href=format!("#{ANCHOR}")>{link.name.clone()}</a>
                        " ("{tr(TextKey::TeletextElementChannelKind)}")"</td>
                    <td>{badge}</td>
                </tr>
            }
            .into_any()
        })
        .collect()
}

pub(super) fn custom_pages_island(data: &ChannelTeletextData) -> AnyView {
    let (badge, body) = match data.custom_pages.as_ref() {
        None => (
            None,
            view! { <p role="alert">{tr(TextKey::TeletextElementChannelUnavailable)}</p> }
                .into_any(),
        ),
        Some(custom) => {
            let enabled = custom.links.iter().filter(|link| link.enabled).count();
            let badge = if custom.links.is_empty() {
                status_badge(TeletextTone::Neutral, tr(TextKey::TeletextCustomPagesNone))
            } else {
                status_badge(
                    TeletextTone::Neutral,
                    if enabled == 1 {
                        tr(TextKey::TeletextElementChannelOneOn).to_owned()
                    } else {
                        tr(TextKey::TeletextElementChannelCountOn)
                            .replace("{count}", &enabled.to_string())
                    },
                )
            };
            (Some(badge), body(data, custom))
        }
    };
    view! {
        <div id=ANCHOR>
            {island(TeletextIsland {
                test_id: "teletext-custom-pages",
                title: tr(TextKey::TeletextCustomPagesTitle).to_owned(),
                badge,
                help: Some(tr(TextKey::TeletextCustomPagesHelp).to_owned()),
                data_id: None,
                body,
            })}
        </div>
    }
    .into_any()
}

fn return_path(channel_id: i64) -> String {
    format!("/channels/{channel_id}/teletext#{ANCHOR}")
}

fn body(data: &ChannelTeletextData, custom: &TeletextCustomPagesData) -> AnyView {
    let channel_id = data.channel_id;
    let links = if custom.links.is_empty() {
        view! {
            <div class=style::empty>
                <p><strong>{tr(TextKey::TeletextCustomPagesEmpty)}</strong></p>
                <p>{tr(TextKey::TeletextElementChannelEmptyHelp)}</p>
            </div>
        }
        .into_any()
    } else {
        view! {
            <ul class=custom_style::links data-testid="teletext-custom-page-links">
                {custom.links.iter().map(|link| link_row(channel_id, link, issue_text(data, link))).collect_view()}
            </ul>
        }
        .into_any()
    };
    let available: Vec<_> = custom
        .library
        .iter()
        .filter(|element| {
            !custom
                .links
                .iter()
                .any(|link| link.element_id == element.id)
        })
        .collect();
    let add = if custom.library.is_empty() {
        view! {
            <p class=style::help>
                {tr(TextKey::TeletextElementChannelNoLibrary)}" "
                <a href="/teletext-elements/add">{tr(TextKey::UploadTeletextPage)}</a>
            </p>
        }
        .into_any()
    } else if available.is_empty() {
        view! { <p class=style::help>{tr(TextKey::TeletextElementChannelAllLinked)}</p> }.into_any()
    } else {
        view! {
            <form method="post" action=format!("/admin-api/channels/{channel_id}/teletext-elements/save")
                class=custom_style::add data-testid="teletext-custom-page-add">
                <input type="hidden" name="return" value=return_path(channel_id)/>
                <input type="hidden" name="enabled" value="1"/>
                <label>
                    <span>{tr(TextKey::TeletextElementChannelElement)}</span>
                    <select name="element_id" required>
                        {available.iter().map(|element| view! {
                            <option value=element.id.to_string()>
                                {format!("{} · {} {}", element.name, tr(TextKey::TeletextElementPageNumber), element.page)}
                            </option>
                        }).collect_view()}
                    </select>
                </label>
                <label>
                    <span>{tr(TextKey::TeletextElementChannelPage)}</span>
                    <input type="text" name="page_number" inputmode="numeric" pattern="[1-8][0-9]{2}"
                        maxlength="3" size="4"/>
                </label>
                <button type="submit" class=btn_class(ButtonVariant::Primary)>{tr(TextKey::TeletextElementChannelAdd)}</button>
                <p class=style::help>{tr(TextKey::TeletextElementChannelPageHelp)}</p>
            </form>
        }
        .into_any()
    };
    view! {
        <p class=style::notice>{tr(TextKey::TeletextElementChannelBroadcastHelp)}</p>
        {links}
        {add}
        <p><a class=style::manage href="/teletext-elements">{tr(TextKey::TeletextElementChannelLibrary)}</a></p>
    }
    .into_any()
}

fn link_row(
    channel_id: i64,
    link: &TeletextCustomPageData,
    issue: Option<String>,
) -> impl IntoView {
    let save = format!("/admin-api/channels/{channel_id}/teletext-elements/save");
    let remove = format!(
        "/admin-api/channels/{channel_id}/teletext-elements/{}/remove",
        link.link_id
    );
    let state = if link.enabled { "on" } else { "off" };
    view! {
        <li class=custom_style::link data-testid="teletext-custom-page" data-element-id=link.element_id.to_string()
            data-state=state>
            <div class=custom_style::link_title>
                <a href=format!("/teletext-elements/{}", link.element_id)><strong>{link.name.clone()}</strong></a>
                " · "{tr(TextKey::TeletextElementPageNumber)}" "{link.page.clone()}
            </div>
            {issue.filter(|_| link.enabled).map(|reason| view! {
                <p role="alert" data-testid="teletext-custom-page-issue">{reason}</p>
            })}
            <form method="post" action=save class=custom_style::link_form>
                <input type="hidden" name="return" value=return_path(channel_id)/>
                <input type="hidden" name="element_id" value=link.element_id.to_string()/>
                <label>
                    <span>{tr(TextKey::TeletextElementChannelPage)}</span>
                    <input type="text" name="page_number" inputmode="numeric" pattern="[1-8][0-9]{2}"
                        maxlength="3" size="4" value=link.page_override.clone() placeholder=link.default_page.clone()/>
                </label>
                <label>
                    <span>{tr(TextKey::TeletextElementChannelState)}</span>
                    <select name="enabled">
                        <option value="1" selected=link.enabled>{tr(TextKey::TeletextElementChannelOn)}</option>
                        <option value="0" selected=!link.enabled>{tr(TextKey::Off)}</option>
                    </select>
                </label>
                <button type="submit" class=btn_class(ButtonVariant::Secondary)>{tr(TextKey::Save)}</button>
            </form>
            <form method="post" action=remove class=custom_style::remove>
                <input type="hidden" name="return" value=return_path(channel_id)/>
                <button type="submit" class=btn_class(ButtonVariant::Text)>{tr(TextKey::Remove)}</button>
            </form>
        </li>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feedback_maps_custom_page_codes_only() {
        assert_eq!(
            feedback("custom-page-page-in-use", "").map(|(_, role)| role),
            Some("alert")
        );
        assert_eq!(
            feedback("", "custom-page").map(|(_, role)| role),
            Some("status")
        );
        assert_eq!(
            feedback("", "custom-page-removed").map(|(_, role)| role),
            Some("status")
        );
        assert!(feedback("invalid", "draft").is_none());
    }
}
