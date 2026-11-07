//! Custom pages island on the channel teletext page.
//!
//! Custom pages come from the teletext element library (`/teletext-elements`).
//! This island lists the elements linked to the channel, lets the user change
//! their page number or switch them off, and links new ones. The page set
//! gets one row per enabled link.

use leptos::prelude::*;

use crate::admin::i18n::{tr, TextKey};
use crate::admin::server_fns::teletext::{
    ChannelTeletextData, TeletextAddonData, TeletextCustomPageData, TeletextCustomPagesData,
};
use crate::admin::ui::primitives::button::{btn_class, ButtonVariant};
use crate::admin::ui::primitives::teletext::{island, status_badge, TeletextIsland, TeletextTone};

use super::teletext::{addon_name, effective_setting, source_on, style};

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

/// Decimal page range `(start, end)` an enabled add-on uses on this channel.
fn addon_range(addon: &TeletextAddonData) -> Option<(u16, u16)> {
    if !source_on(addon) {
        return None;
    }
    let start = effective_setting(addon, &["PAGE_START"])?
        .parse::<u16>()
        .ok()?;
    let end = effective_setting(addon, &["PAGE_END"])
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(start);
    Some((start, end.max(start)))
}

/// Name of the add-on whose page range contains `page`, if any.
fn clashing_addon(data: &ChannelTeletextData, page: &str) -> Option<String> {
    let page = page.parse::<u16>().ok()?;
    data.addons.iter().find_map(|addon| {
        let (start, end) = addon_range(addon)?;
        (start..=end).contains(&page).then(|| addon_name(addon))
    })
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
            let badge = match clashing_addon(data, &link.page) {
                Some(addon) => status_badge(
                    TeletextTone::Error,
                    tr(TextKey::TeletextElementChannelClash).replace("{addon}", &addon),
                ),
                None => status_badge(
                    TeletextTone::Neutral,
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
    let channel_id = data.channel_id;
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
            (Some(badge), body(channel_id, custom))
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

fn body(channel_id: i64, custom: &TeletextCustomPagesData) -> AnyView {
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
                {custom.links.iter().map(|link| link_row(channel_id, link)).collect_view()}
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
        <p class=style::notice>{tr(TextKey::TeletextElementChannelNotBroadcast)}</p>
        {links}
        {add}
        <p><a class=style::manage href="/teletext-elements">{tr(TextKey::TeletextElementChannelLibrary)}</a></p>
    }
    .into_any()
}

fn link_row(channel_id: i64, link: &TeletextCustomPageData) -> impl IntoView {
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
