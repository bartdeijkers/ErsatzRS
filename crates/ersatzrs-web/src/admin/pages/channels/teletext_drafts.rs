//! Explicit unpublished authoring input and exact immutable historical preview.

use ersatzrs_domain::{TeletextNavigationLink, TeletextNavigationRole, TeletextPageAddress};
use leptos::prelude::*;

use crate::admin::i18n::{tr, TextKey};
use crate::admin::server_fns::teletext::ChannelTeletextData;
use crate::admin::server_fns::teletext_drafts::{
    TeletextDraftDiagnosticData, TeletextDraftErrorLocation, TeletextDraftPreviewData,
    TeletextDraftPreviewResult,
};
use crate::admin::ui::primitives::button::{btn_class, ButtonVariant};
use crate::admin::ui::primitives::resource_state::admin_result_view;

stylance::import_crate_style!(style, "src/admin/pages/channels/teletext_drafts.module.css");

pub(super) fn draft_status_text(code: &str) -> &'static str {
    tr(match code {
        "draft" => TextKey::TeletextDraftSaved,
        "busy" => TextKey::TeletextWorkBusy,
        "expired" => TextKey::TeletextWorkExpired,
        "missing-design" | "foreign-channel" => TextKey::TeletextDraftMissing,
        "missing-policy" | "invalid-policy" | "off" | "unavailable" => {
            TextKey::TeletextAuthoringUnavailable
        }
        _ => TextKey::TeletextAuthoringRejected,
    })
}

pub(super) fn authoring_controls(data: &ChannelTeletextData) -> AnyView {
    let eligible = data.addons.iter().filter(|addon| {
        addon.installed
            && addon.availability == "configured"
            && addon
                .binding
                .as_ref()
                .is_some_and(|binding| binding.enabled && binding.availability == "configured")
    });
    let available = data.catalog_available
        && data.authoring_selection.is_some()
        && eligible.clone().next().is_some();
    let options = eligible
        .map(|addon| {
            let name = if addon.name_labels.is_empty() {
                addon.name.clone()
            } else {
                super::teletext::manifest_label(&addon.name_labels)
            };
            view! { <option value=addon.addon_id.clone()>{name}</option> }
        })
        .collect_view();
    view! {
        <section class=style::authoring data-testid="teletext-authoring">
            <h2>{tr(TextKey::TeletextAuthoringTitle)}</h2>
            <p>{tr(TextKey::TeletextAuthoringHelp)}</p>
            <p>{tr(TextKey::TeletextAuthoringStaticHelp)}</p>
            <p role="status" hidden=available>{draft_status_text(data.authoring_error.as_deref().unwrap_or("unavailable"))}</p>
            <form method="post" action="/admin-api/teletext/design/prepare"
                data-testid="teletext-authoring-form">
                {hidden("channel_id", data.channel_id.to_string())}
                {hidden("selection_digest", data.authoring_selection.clone().unwrap_or_default())}
                <fieldset disabled=!available class=style::fields>
                    <legend>{tr(TextKey::TeletextAuthoringTitle)}</legend>
                    <label for="teletext-draft-name">{tr(TextKey::TeletextDesignName)}</label>
                    <input id="teletext-draft-name" name="name" required
                        maxlength=ersatzrs_domain::MAX_TELETEXT_DESIGN_NAME_BYTES/>
                    <label for="teletext-entry-producer">{tr(TextKey::TeletextEntryProducer)}</label>
                    <select id="teletext-entry-producer" name="entry_addon_id" required>
                        <option value="" selected disabled>{tr(TextKey::TeletextEntryProducer)}</option>
                        {options}
                    </select>
                    {entry_fields()}
                    <label for="teletext-v1-region">{tr(TextKey::TeletextV1Region)}</label>
                    <select id="teletext-v1-region" name="v1_region" required>
                        <option value="" selected disabled>{tr(TextKey::TeletextV1Region)}</option>
                        <option value="western_latin">{tr(TextKey::TeletextV1WesternLatin)}</option>
                    </select>
                    <label for="teletext-resources">{tr(TextKey::TeletextResourceLifecycle)}</label>
                    <p id="teletext-resources-help">{tr(TextKey::TeletextResourceLifecycleHelp)}</p>
                    <textarea id="teletext-resources" name="resource_transmissions" required
                        rows="5" maxlength="65536" aria-describedby="teletext-resources-help"
                        inner_html=textarea_content("")></textarea>
                    <button type="submit" class=btn_class(ButtonVariant::Primary)>
                        {tr(TextKey::TeletextValidateSaveDraft)}
                    </button>
                </fieldset>
            </form>
        </section>
    }.into_any()
}

fn entry_fields() -> AnyView {
    view! {
        <div class=style::address>
            <div><label for="teletext-entry-page">{tr(TextKey::TeletextEntryPage)}</label>
                <input id="teletext-entry-page" name="entry_page" type="number"
                    min="100" max="899" step="1" required/></div>
            <div><label for="teletext-entry-subcode">{tr(TextKey::TeletextEntrySubpage)}</label>
                <input id="teletext-entry-subcode" name="entry_subcode" pattern="[0-9a-fA-F]{4}"
                    minlength="4" maxlength="4" required/></div>
        </div>
    }
    .into_any()
}

fn hidden(name: &str, value: String) -> AnyView {
    view! { <input type="hidden" name=name.to_owned() value=value/> }.into_any()
}

fn textarea_content(value: &str) -> String {
    // Tachys emits raw textarea children. Escape explicitly and preserve the
    // initial newline ignored by HTML parsing, including normalized CR/CRLF.
    let escaped = value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    if value.starts_with(['\n', '\r']) {
        format!("\n{escaped}")
    } else {
        escaped
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct PreviewQuery {
    pub(super) design_id: i64,
    pub(super) revision: i64,
    pub(super) page: u16,
    pub(super) subcode: u16,
}

pub(super) fn preview_query(values: &leptos_router::params::ParamsMap) -> Option<PreviewQuery> {
    let subcode = values.get("subcode")?;
    if subcode.len() != 4 || !subcode.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let query = PreviewQuery {
        design_id: values.get("design_id")?.parse().ok()?,
        revision: values.get("revision")?.parse().ok()?,
        page: values.get("page")?.parse().ok()?,
        subcode: u16::from_str_radix(&subcode, 16).ok()?,
    };
    (query.design_id > 0 && query.revision > 0).then_some(query)
}

#[component]
pub fn TeletextDraftPreview(
    preview: Option<Result<TeletextDraftPreviewResult, ServerFnError>>,
    requested: bool,
    redirected: Option<TeletextDraftErrorLocation>,
) -> impl IntoView {
    let content = match preview {
        Some(result) => admin_result_view(
            result.map_err(|_| draft_status_text("unavailable").to_owned()),
            preview_result,
        ),
        None if requested => {
            view! { <p role="alert">{draft_status_text("invalid-input")}</p> }.into_any()
        }
        None => ().into_any(),
    };
    view! {
        <section class=style::preview data-testid="teletext-draft-preview">
            <h2>{tr(TextKey::TeletextPreviewTitle)}</h2>
            <p>{tr(TextKey::TeletextPreviewHelp)}</p>
            {redirected.map(|location| error_location(&location))}
            {content}
        </section>
    }
}

fn preview_result(result: TeletextDraftPreviewResult) -> AnyView {
    match result.preview {
        Some(preview) => preview_view(&preview),
        None => view! {
            <div role="alert" data-testid="teletext-preview-error">
                <p>{draft_status_text(result.error.as_deref().unwrap_or("unavailable"))}</p>
                {result.error_location.as_ref().map(error_location)}
            </div>
        }
        .into_any(),
    }
}

fn address_text(owner: TeletextPageAddress) -> String {
    format!("{} / {:04X}", owner.page.get(), owner.subcode.get())
}

fn preview_url(data: &TeletextDraftPreviewData, owner: TeletextPageAddress) -> String {
    format!(
        "/channels/{}/teletext?design_id={}&revision={}&page={}&subcode={:04X}",
        data.channel_id,
        data.design_id,
        data.revision,
        owner.page.get(),
        owner.subcode.get()
    )
}

fn page_link(data: &TeletextDraftPreviewData, owner: TeletextPageAddress) -> AnyView {
    view! {
        <a href=preview_url(data, owner) aria-current=(owner == data.owner).then_some("page")>
            {address_text(owner)}
        </a>
    }
    .into_any()
}

fn preview_view(data: &TeletextDraftPreviewData) -> AnyView {
    let description = format!(
        "{} · {}",
        tr(TextKey::TeletextPreviewTitle),
        address_text(data.owner)
    );
    let inventory = data
        .pages
        .iter()
        .map(|owner| page_link(data, *owner))
        .collect_view();
    let navigation = data
        .navigation
        .iter()
        .map(|link| navigation_link(data, link))
        .collect_view();
    let diagnostics = diagnostic_list(&data.diagnostics);
    view! {
        <div data-testid="teletext-preview-model" data-page=data.owner.page.get()
            data-subcode=format!("{:04X}", data.owner.subcode.get())>
            {preview_controls(data)}
            <p>{address_text(data.owner)}</p>
            <img class=style::raster src=data.image_url.clone() alt=description/>
            {preview_legend()}
            <nav class=style::inventory aria-label=tr(TextKey::TeletextPreviewPage)>{inventory}</nav>
            <ul class=style::navigation>{navigation}</ul>
            {diagnostics}
        </div>
    }.into_any()
}

fn preview_controls(data: &TeletextDraftPreviewData) -> AnyView {
    view! {
        <form method="get" action=format!("/channels/{}/teletext", data.channel_id)
            class=style::preview_form data-testid="teletext-preview-form">
            {hidden("design_id", data.design_id.to_string())}
            {hidden("revision", data.revision.to_string())}
            <div><label for="teletext-preview-page">{tr(TextKey::TeletextPreviewPage)}</label>
                <input id="teletext-preview-page" name="page" type="number" min="100" max="899"
                    step="1" required value=data.owner.page.get()/></div>
            <div><label for="teletext-preview-subcode">{tr(TextKey::TeletextPreviewSubpage)}</label>
                <input id="teletext-preview-subcode" name="subcode" pattern="[0-9a-fA-F]{4}"
                    minlength="4" maxlength="4" required value=format!("{:04X}", data.owner.subcode.get())/></div>
            <button type="submit" class=btn_class(ButtonVariant::Primary)>{tr(TextKey::TeletextPreviewLoad)}</button>
            <a href=preview_url(data, data.owner)>{tr(TextKey::TeletextPreviewReload)}</a>
        </form>
    }.into_any()
}

fn navigation_role(role: TeletextNavigationRole) -> &'static str {
    tr(match role {
        TeletextNavigationRole::Index => TextKey::TeletextNavIndex,
        TeletextNavigationRole::Red => TextKey::TeletextNavRed,
        TeletextNavigationRole::Green => TextKey::TeletextNavGreen,
        TeletextNavigationRole::Yellow => TextKey::TeletextNavYellow,
        TeletextNavigationRole::Cyan => TextKey::TeletextNavCyan,
    })
}

fn navigation_targets(
    data: &TeletextDraftPreviewData,
    link: &TeletextNavigationLink,
) -> Vec<TeletextPageAddress> {
    data.pages
        .iter()
        .filter(|owner| {
            owner.page == link.target.page
                && link
                    .target
                    .subcode
                    .is_none_or(|subcode| owner.subcode == subcode)
        })
        .copied()
        .collect()
}

fn navigation_link(data: &TeletextDraftPreviewData, link: &TeletextNavigationLink) -> AnyView {
    let targets = navigation_targets(data, link)
        .into_iter()
        .map(|owner| page_link(data, owner))
        .collect_view();
    let target = link.target.subcode.map_or_else(
        || link.target.page.get().to_string(),
        |subcode| format!("{} / {:04X}", link.target.page.get(), subcode.get()),
    );
    view! { <li><span>{navigation_role(link.role)}" · "{target}</span><div>{targets}</div></li> }
        .into_any()
}

fn preview_legend() -> AnyView {
    view! {
        <ul class=style::legend aria-label=tr(TextKey::TeletextPreviewDiagnostics)>
            <li><span class=style::header_pattern aria-hidden="true"></span>{tr(TextKey::TeletextPreviewReceiverHeader)}</li>
            <li><span class=style::video_pattern aria-hidden="true"></span>{tr(TextKey::TeletextPreviewVideo)}</li>
            <li><span class=style::unresolved_pattern aria-hidden="true"></span>{tr(TextKey::TeletextPreviewUnresolved)}</li>
        </ul>
    }.into_any()
}

fn diagnostic_list(diagnostics: &[TeletextDraftDiagnosticData]) -> AnyView {
    let entries = diagnostics.iter().map(|diagnostic| {
        let original = diagnostic.original_position.map(|(row, column)| format!("{row}, {column}"));
        view! {
            <li>{address_text(diagnostic.owner)}" · "
                {tr(TextKey::TeletextPreviewPosition)}" "{diagnostic.visual_row}", "{diagnostic.visual_column}
                <span>{tr(TextKey::TeletextPreviewSource)}" "{diagnostic.category.clone()}</span>
                {original.map(|position| view! { <span>{tr(TextKey::TeletextPreviewSource)}" · "{tr(TextKey::TeletextPreviewPosition)}" "{position}</span> })}
                {diagnostic_provenance(diagnostic)}
            </li>
        }
    }).collect_view();
    view! {
        <section class=style::diagnostics data-testid="teletext-preview-diagnostics">
            <h3>{tr(TextKey::TeletextPreviewDiagnostics)}</h3>
            <p hidden=!diagnostics.is_empty()>{tr(TextKey::TeletextPreviewNoDiagnostics)}</p>
            <ul>{entries}</ul>
        </section>
    }
    .into_any()
}

fn diagnostic_provenance(diagnostic: &TeletextDraftDiagnosticData) -> AnyView {
    let indices = diagnostic
        .candidate
        .map(|index| format!("candidate {index}"))
        .into_iter()
        .chain(
            diagnostic
                .invocation
                .map(|index| format!("invocation {index}")),
        )
        .collect::<Vec<_>>()
        .join(" / ");
    let indices_empty = indices.is_empty();
    view! {
        <span hidden=diagnostic.source.is_none()>{tr(TextKey::TeletextPreviewSource)}" "{diagnostic.source.clone()}</span>
        <span hidden=indices_empty>{tr(TextKey::TeletextPreviewSource)}" "{indices}</span>
    }.into_any()
}

pub(super) fn redirected_error_location(
    values: &leptos_router::params::ParamsMap,
) -> Option<TeletextDraftErrorLocation> {
    use ersatzrs_domain::{TeletextPageNumber, TeletextSubcode};
    let owner = values
        .get("error_page")
        .and_then(|page| page.parse().ok())
        .and_then(|page| TeletextPageNumber::new(page).ok())
        .zip(
            values
                .get("error_subcode")
                .and_then(|subcode| u16::from_str_radix(&subcode, 16).ok())
                .and_then(|subcode| TeletextSubcode::new(subcode).ok()),
        )
        .map(|(page, subcode)| TeletextPageAddress { page, subcode });
    let position = values
        .get("error_row")
        .and_then(|row| row.parse::<u16>().ok())
        .zip(
            values
                .get("error_column")
                .and_then(|column| column.parse::<u16>().ok()),
        );
    let resource = values
        .get("error_resource")
        .filter(|raw| raw.len() <= 2048)
        .and_then(|raw| {
            serde_json::from_str::<ersatzrs_domain::TeletextResourceIdentity>(&raw).ok()
        });
    (owner.is_some() || position.is_some() || resource.is_some()).then_some(
        TeletextDraftErrorLocation {
            owner,
            resource,
            position,
            source: None,
        },
    )
}

fn error_location(location: &TeletextDraftErrorLocation) -> AnyView {
    let owner = location.owner.map(address_text);
    let position = location
        .position
        .map(|(row, column)| format!("{row}, {column}"));
    let resource = location.resource.map(|identity| {
        use ersatzrs_domain::TeletextResourceKind;
        let kind = match identity.kind() {
            TeletextResourceKind::Pop => "pop",
            TeletextResourceKind::Gpop => "gpop",
            TeletextResourceKind::Drcs => "drcs",
            TeletextResourceKind::Gdrcs => "gdrcs",
            TeletextResourceKind::Mot => "mot",
            TeletextResourceKind::Mip => "mip",
        };
        format!(
            "{kind} {}{:02X} / {:X}",
            identity.page().magazine(),
            identity.page().page(),
            identity.logical_subpage().get()
        )
    });
    view! {
        <dl class=style::location data-testid="teletext-error-location">
            {owner.map(|owner| view! { <dt>{tr(TextKey::TeletextPreviewPage)}</dt><dd>{owner}</dd> })}
            {resource.map(|resource| view! { <dt>{tr(TextKey::TeletextResourceLifecycle)}</dt><dd>{resource}</dd> })}
            {position.map(|position| view! { <dt>{tr(TextKey::TeletextPreviewPosition)}</dt><dd>{position}</dd> })}
            {location.source.clone().map(|source| view! { <dt>{tr(TextKey::TeletextPreviewSource)}</dt><dd>{source}</dd> })}
        </dl>
    }.into_any()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::server_fns::teletext::{TeletextAddonData, TeletextBindingData};
    use ersatzrs_domain::{TeletextNavigationTarget, TeletextPageNumber, TeletextSubcode};

    fn owner(page: u16, subcode: u16) -> TeletextPageAddress {
        TeletextPageAddress {
            page: TeletextPageNumber::new(page).unwrap(),
            subcode: TeletextSubcode::new(subcode).unwrap(),
        }
    }

    fn channel() -> ChannelTeletextData {
        let addon = |addon_id: &str, enabled: bool| TeletextAddonData {
            addon_id: addon_id.to_owned(),
            name: addon_id.to_owned(),
            name_labels: std::collections::BTreeMap::default(),
            installed: true,
            availability: "configured".to_owned(),
            installation: None,
            settings: Vec::new(),
            profiles: Vec::new(),
            binding: Some(TeletextBindingData {
                profile_id: 1,
                enabled,
                revision: 2,
                availability: "configured".to_owned(),
            }),
        };
        ChannelTeletextData {
            channel_id: 44,
            channel_number: "1".to_owned(),
            channel_name: "Synthetic".to_owned(),
            catalog_available: true,
            authoring_selection: Some("observed-membership".to_owned()),
            authoring_error: None,
            addons: vec![
                addon("org.example.on", true),
                addon("org.example.off", false),
            ],
        }
    }

    fn preview() -> TeletextDraftPreviewData {
        TeletextDraftPreviewData {
            channel_id: 44,
            design_id: 8,
            revision: 3,
            owner: owner(200, 0x10),
            image_url: "data:image/svg+xml;base64,PHN2Zy8+".to_owned(),
            pages: vec![owner(200, 0x10), owner(201, 0x20), owner(201, 0x30)],
            navigation: vec![TeletextNavigationLink {
                role: TeletextNavigationRole::Red,
                target: TeletextNavigationTarget {
                    page: TeletextPageNumber::new(201).unwrap(),
                    subcode: None,
                },
            }],
            diagnostics: Vec::new(),
        }
    }

    #[test]
    fn authoring_form_requires_explicit_input_and_preserves_membership() {
        let owner = Owner::new();
        let mut data = channel();
        let html = owner.with(|| authoring_controls(&data).to_html());
        assert!(html.contains("name=\"selection_digest\" value=\"observed-membership\""));
        assert!(html.contains("value=\"org.example.on\""));
        assert!(!html.contains("value=\"org.example.off\""));
        for name in ["name", "entry_page", "entry_subcode"] {
            let input = html
                .split(&format!("name=\"{name}\""))
                .nth(1)
                .unwrap()
                .split('>')
                .next()
                .unwrap();
            assert!(!input.contains("value="));
            assert!(input.contains("required"));
        }
        let textarea = html.split("<textarea").nth(1).unwrap();
        let content = textarea
            .split_once('>')
            .unwrap()
            .1
            .split_once("</textarea>")
            .unwrap()
            .0;
        assert_eq!(content, "");
        assert!(!html.contains("<fieldset disabled"));
        data.catalog_available = false;
        let html = owner.with(|| authoring_controls(&data).to_html());
        assert!(html.contains("<fieldset disabled"));
        assert!(html.contains("name=\"selection_digest\" value=\"observed-membership\""));
        data.catalog_available = true;
        let addons = std::mem::take(&mut data.addons);
        let html = owner.with(|| authoring_controls(&data).to_html());
        assert!(html.contains("<fieldset disabled"));
        assert!(html.contains("name=\"selection_digest\" value=\"observed-membership\""));
        data.addons = addons;
        data.authoring_selection = None;
        assert!(owner
            .with(|| authoring_controls(&data).to_html())
            .contains("<fieldset disabled"));
        assert_eq!(textarea_content("</textarea>&"), "&lt;/textarea&gt;&amp;");
        assert_eq!(textarea_content("\r\n[]"), "\n\r\n[]");
    }

    #[test]
    fn preview_navigation_retains_all_actual_subpages_without_inventing_zero() {
        let data = preview();
        assert_eq!(
            navigation_targets(&data, &data.navigation[0]),
            vec![owner(201, 0x20), owner(201, 0x30)]
        );
        let mut exact = data.navigation[0].clone();
        exact.target.subcode = Some(TeletextSubcode::new(0x30).unwrap());
        assert_eq!(navigation_targets(&data, &exact), vec![owner(201, 0x30)]);
        exact.target.subcode = Some(TeletextSubcode::new(0x40).unwrap());
        assert!(navigation_targets(&data, &exact).is_empty());
        let owner = Owner::new();
        let html = owner.with(|| preview_view(&data).to_html());
        for subcode in ["0010", "0020", "0030"] {
            assert!(html.contains(&format!("subcode={subcode}")));
        }
        assert!(!html.contains("subcode=0000"));
        assert!(html.contains("design_id=8"));
        assert!(html.contains("revision=3"));
        assert!(html.contains(tr(TextKey::TeletextNavRed)));
        assert!(html.contains("data:image/svg+xml;base64,PHN2Zy8+"));
        for key in [
            TextKey::TeletextPreviewReceiverHeader,
            TextKey::TeletextPreviewVideo,
            TextKey::TeletextPreviewUnresolved,
        ] {
            assert!(html.contains(tr(key)));
        }
    }

    #[test]
    fn preview_locations_and_diagnostics_are_checked_and_escaped() {
        let owner_context = Owner::new();
        let diagnostic = TeletextDraftDiagnosticData {
            owner: owner(200, 0x10),
            visual_row: 7,
            visual_column: 8,
            original_position: Some((3, 4)),
            category: "glyph".to_owned(),
            candidate: Some(5),
            source: Some("X26".to_owned()),
            invocation: Some(6),
        };
        let html = owner_context.with(|| diagnostic_list(&[diagnostic]).to_html());
        for value in ["200 / 0010", "3, 4", "X26", "candidate 5", "invocation 6"] {
            assert!(html.contains(value));
        }
        let html = owner_context.with(|| {
            preview_result(TeletextDraftPreviewResult {
                preview: None,
                error: Some("<script>private()</script>".to_owned()),
                error_location: Some(TeletextDraftErrorLocation {
                    owner: Some(owner(200, 0x10)),
                    resource: None,
                    position: Some((3, 4)),
                    source: Some("<synthetic>".to_owned()),
                }),
            })
            .to_html()
        });
        assert!(html.contains(tr(TextKey::TeletextAuthoringRejected)));
        assert!(html.contains("200 / 0010"));
        assert!(html.contains("&lt;synthetic&gt;"));
        assert!(!html.contains("private()"));
    }

    #[test]
    fn preloaded_preview_renders_without_router_or_async_resource_context() {
        let owner_context = Owner::new();
        let html = owner_context.with(|| {
            view! {
                <TeletextDraftPreview preview=Some(Ok(TeletextDraftPreviewResult {
                    preview: Some(preview()), error: None, error_location: None,
                })) requested=true redirected=None/>
            }
            .to_html()
        });
        assert!(html.contains("design_id=8"));
        assert!(html.contains("revision=3"));
        assert!(html.contains("data:image/svg+xml;base64,PHN2Zy8+"));
        for requested in [false, true] {
            let html = owner_context.with(|| {
                view! {
                    <TeletextDraftPreview preview=None requested=requested redirected=None/>
                }
                .to_html()
            });
            assert_eq!(html.contains(draft_status_text("invalid-input")), requested);
        }
        let html = owner_context.with(|| view! {
            <TeletextDraftPreview preview=Some(Err(ServerFnError::new("synthetic-private-transport")))
                requested=true redirected=Some(TeletextDraftErrorLocation {
                    owner: Some(owner(200, 0x10)), resource: None, position: Some((7, 8)), source: None,
                })/>
        }.to_html());
        assert!(html.contains(draft_status_text("unavailable")));
        assert!(html.contains("200 / 0010"));
        assert!(html.contains("7, 8"));
        assert!(!html.contains("synthetic-private-transport"));
    }

    #[test]
    fn preview_query_never_supplies_missing_design_or_address_defaults() {
        let mut values = leptos_router::params::ParamsMap::new();
        assert!(preview_query(&values).is_none());
        for (key, value) in [("design_id", "8"), ("revision", "3"), ("page", "200")] {
            values.insert(key, value.to_owned());
        }
        assert!(preview_query(&values).is_none());
        values.insert("subcode", "0010".to_owned());
        assert_eq!(preview_query(&values).unwrap().subcode, 0x10);
        values.insert("subcode", "10".to_owned());
        assert!(preview_query(&values).is_none());
        values.insert("error_row", "7".to_owned());
        values.insert("error_column", "8".to_owned());
        let location = redirected_error_location(&values).unwrap();
        assert_eq!(location.position, Some((7, 8)));
        assert_eq!(location.owner, None);
        let mut resource_values = leptos_router::params::ParamsMap::new();
        resource_values.insert(
            "error_resource",
            r#"{"kind":"pop","page":{"magazine":2,"page":160},"logical_subpage":3}"#.to_owned(),
        );
        let resource = redirected_error_location(&resource_values).unwrap();
        assert_eq!(resource.resource.unwrap().logical_subpage().get(), 3);
        assert_eq!(resource.owner, None);
        assert_eq!(resource.position, None);
        resource_values.insert("error_resource", r#"{"kind":"pop","page":{"magazine":2,"page":160},"logical_subpage":3,"unknown":"<script>"}"#.to_owned());
        assert!(redirected_error_location(&resource_values).is_none());
    }
}
