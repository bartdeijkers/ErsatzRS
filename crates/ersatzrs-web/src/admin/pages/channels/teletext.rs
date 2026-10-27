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
    load_teletext_draft_preview, TeletextDraftErrorLocation, TeletextDraftPreviewResult,
};
use crate::admin::ui::primitives::button::{btn_class, ButtonVariant};
use crate::admin::ui::primitives::resource_state::{
    admin_loading_resource_view, admin_result_view,
};

stylance::import_crate_style!(style, "src/admin/pages/channels/teletext.module.css");

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
                        Some((data, preview)) => channel_page(
                            data, &error, &saved, preview,
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
);

async fn load_channel_page(
    (channel_id, preview): (i64, Option<super::teletext_drafts::PreviewQuery>),
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
    Ok(Some((channel, preview)))
}

fn channel_page(
    data: ChannelTeletextData,
    error: &str,
    saved: &str,
    preview: Option<Result<TeletextDraftPreviewResult, ServerFnError>>,
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
                | "deleted-profile" | "limit" | "storage" => error,
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
    } else if matches!(saved, "profile" | "binding" | "off") {
        (saved, tr(TextKey::TeletextAdminSaved), "status")
    } else {
        ("", "", "status")
    };
    let channel_id = data.channel_id;
    let empty = data.addons.is_empty();
    let authoring = super::teletext_drafts::authoring_controls(&data);
    let effective_authoring = super::teletext_drafts::effective_authoring_controls(&data);
    view! {
        <div class=style::page data-testid="channel-teletext-page" data-channel-id=channel_id>
            <header class=style::header>
                <a href=format!("/channels/{channel_id}")>{tr(TextKey::EditChannel)}</a>
                <h1>{tr(TextKey::TeletextChannelTitle)}</h1>
                <p>{data.channel_number}" · "{data.channel_name}</p>
            </header>
            <p class=style::help>{tr(TextKey::TeletextChannelBaseHelp)}</p>
            <p role="alert" hidden=data.catalog_available>{tr(TextKey::TeletextAdminUnavailable)}</p>
            <p class=style::feedback role=role data-testid="teletext-feedback"
                data-code=code.to_owned() hidden=message.is_empty()>{message}</p>
            <p class=style::help hidden=!empty>{tr(TextKey::TeletextChannelEmpty)}</p>
            <a class=style::manage href="/settings/addons">{tr(TextKey::Addons)}</a>
            {data.addons.into_iter().map(|addon| addon_section(channel_id, &addon)).collect_view()}
            {authoring}
            {effective_authoring}
            <super::teletext_drafts::TeletextDraftPreview preview=preview requested=requested redirected=redirected/>
        </div>
    }
    .into_any()
}

fn addon_section(channel_id: i64, addon: &TeletextAddonData) -> AnyView {
    let language = use_i18n();
    let name = if addon.name_labels.is_empty() {
        addon.name.clone()
    } else {
        manifest_label(&addon.name_labels)
    };
    let installation = addon
        .installation
        .as_ref()
        .and_then(|value| serde_json::to_string(value).ok());
    let writable = addon.installed && addon.availability == "configured" && installation.is_some();
    let addon_id = addon.addon_id.clone();
    let binding = binding_controls(channel_id, addon, installation.as_deref(), writable);
    let selected = addon.binding.as_ref().map(|binding| binding.profile_id);
    let editors = addon
        .profiles
        .iter()
        .map(|profile| {
            profile_editor(
                channel_id,
                addon,
                Some(profile),
                installation.as_deref(),
                writable,
                selected == Some(profile.id),
            )
        })
        .collect_view();
    let create = profile_editor(
        channel_id,
        addon,
        None,
        installation.as_deref(),
        writable,
        addon.profiles.is_empty(),
    );
    view! {
        <section class=style::addon data-testid="teletext-addon" data-addon-id=addon_id>
            <h2>{name}</h2>
            <p class=style::availability data-availability=addon.availability.clone()>
                {teletext_profile_availability_text(language, &addon.availability)}
            </p>
            {binding}
            <h3>{tr(TextKey::TeletextChannelProfiles)}</h3>
            <p class=style::help>{tr(TextKey::TeletextProfileSharedHelp)}</p>
            {editors}
            {create}
        </section>
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
    let status = addon.binding.as_ref().map_or(TextKey::Off, |binding| {
        if binding.enabled {
            TextKey::Enabled
        } else {
            TextKey::Off
        }
    });
    let off = addon.binding.as_ref().map(|binding| view! {
        <form method="post" action="/admin-api/teletext/binding/off" data-testid="teletext-disable-form">
            {hidden("channel_id", channel_id.to_string())}
            {hidden("addon_id", addon.addon_id.clone())}
            {hidden("expected_binding_revision", binding.revision.to_string())}
            <button type="submit" class=btn_class(ButtonVariant::Text)>{tr(TextKey::TeletextBindingOff)}</button>
        </form>
    });
    view! {
        <div class=style::binding>
            <p data-testid="teletext-base-state">{tr(status)}</p>
            <form method="post" action="/admin-api/teletext/binding/save"
                class=style::binding_form data-testid="teletext-binding-form">
                {hidden("channel_id", channel_id.to_string())}
                {hidden("addon_id", addon.addon_id.clone())}
                {hidden("expected_binding_revision", revision)}
                {hidden("expected_installation", installation.unwrap_or_default().to_owned())}
                <label for=choice_id.clone()>{tr(TextKey::TeletextBindingProfile)}</label>
                <select id=choice_id name="profile_choice" required disabled=!writable>
                    <option value="" selected=selected.is_none()>{tr(TextKey::TeletextBindingNone)}</option>
                    {missing.then(|| view! { <option value="" selected disabled>{tr(TextKey::ProfileNotFound)}</option> })}
                    {options}
                </select>
                <button type="submit" class=btn_class(ButtonVariant::Primary) disabled=!writable>
                    {tr(TextKey::TeletextBindingSave)}
                </button>
            </form>
            {off}
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
) -> AnyView {
    let id = profile.map_or_else(String::new, |profile| profile.id.to_string());
    let revision = profile.map_or_else(String::new, |profile| profile.revision.to_string());
    let name = profile.map_or_else(String::new, |profile| profile.name.clone());
    let title = profile.map_or_else(
        || tr(TextKey::TeletextProfileCreate).to_owned(),
        |profile| profile.name.clone(),
    );
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
                    {settings}
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
            <label for=mode_id.clone()>{tr(TextKey::Type)}</label>
            <select id=mode_id name=format!("{prefix}.mode")>
                <option value="inherit" selected=stored.is_none()>{tr(TextKey::Inherit)}</option>
                <option value=override_mode selected=stored.is_some()>{tr(TextKey::TeletextProfileOverride)}</option>
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
    fn native_selection_holds_profile_revision_and_broken_off_needs_no_installation() {
        let mut data = addon();
        data.profiles.push(profile("LABEL", "night"));
        let owner = Owner::new();
        let html = owner.with(|| addon_section(44, &data).to_html());
        assert!(html.contains("value=\"12:3\" selected"));
        assert!(html.contains("name=\"expected_binding_revision\" value=\"7\""));
        assert!(html.contains("Synthetic &lt;pages&gt;"));
        data.installed = false;
        data.installation = None;
        data.profiles.clear();
        let html = owner.with(|| addon_section(44, &data).to_html());
        assert!(html.contains(tr(TextKey::ProfileNotFound)));
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
                profile_editor(44, &data, Some(&profile), Some("{}"), true, true).to_html()
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
                profile_editor(44, &data, Some(&profile), Some("{}"), true, true).to_html()
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
}
