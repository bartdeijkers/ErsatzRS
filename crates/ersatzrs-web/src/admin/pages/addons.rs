#![allow(dead_code, clippy::too_many_lines)]

use std::collections::{BTreeMap, BTreeSet};

use ersatzrs_addon_contract::{
    AddonDependency, AddonPanel, AddonPanelBlock, AddonPermissions, AddonSettingDefinition,
    AddonSettingKind,
};
use leptos::prelude::*;

use crate::admin::i18n::{provide_i18n_context, tr, use_i18n, TextKey};
use crate::admin::server_fns::addons::{
    check_addon_package, disable_addon_package, enable_addon_package, install_addon_package,
    load_addon_overview, refresh_addon_repository, remove_addon_repository,
    save_addon_configuration, save_addon_repository, uninstall_addon_package, AddonOverview,
    AddonRepositoryEditor, AddonSettingEditor, AddonSettingRow,
};
use crate::admin::ui::primitives::toast::{load_or_notify, use_toast, ToastSeverity, ToastStore};

stylance::import_crate_style!(style, "src/admin/pages/media_sources/shared.module.css");

#[component]
pub fn AddonsSettingsPage() -> impl IntoView {
    view! { <AddonsSettingsIsland/> }
}

#[island]
fn AddonsSettingsIsland() -> impl IntoView {
    provide_i18n_context();
    let toast = use_toast();
    let overview: LocalResource<AddonOverview> =
        LocalResource::new(
            move || async move { load_or_notify(load_addon_overview().await, toast) },
        );
    let current = RwSignal::new(None::<AddonOverview>);
    Effect::new(move |_| {
        if let Some(value) = overview.get() {
            current.set(Some(value));
        }
    });

    let repository_editor_open = RwSignal::new(false);
    let repository_id = RwSignal::new(String::new());
    let repository_name = RwSignal::new(String::new());
    let repository_url = RwSignal::new(String::new());
    let repository_key = RwSignal::new(String::new());

    let save_repository_action =
        Action::new(move |editor: &AddonRepositoryEditor| save_addon_repository(editor.clone()));
    let refresh_action = Action::new(move |id: &String| refresh_addon_repository(id.clone()));
    let remove_repository_action =
        Action::new(move |id: &String| remove_addon_repository(id.clone()));
    let install_action = Action::new(move |target: &(String, String)| {
        install_addon_package(target.0.clone(), target.1.clone())
    });
    let check_action = Action::new(move |id: &String| check_addon_package(id.clone()));
    let enable_action = Action::new(move |id: &String| enable_addon_package(id.clone()));
    let disable_action = Action::new(move |id: &String| disable_addon_package(id.clone()));
    let uninstall_action =
        Action::new(move |id: &String| uninstall_addon_package(id.clone(), false));
    let setting_action =
        Action::new(move |setting: &AddonSettingEditor| save_addon_configuration(setting.clone()));

    Effect::new(move |_| match save_repository_action.value().get() {
        Some(Ok(())) => {
            repository_editor_open.set(false);
            overview.refetch();
        }
        Some(Err(error)) => toast.push(error.to_string(), ToastSeverity::Error),
        None => {}
    });
    Effect::new(move |_| action_result(refresh_action.value().get(), overview, toast));
    Effect::new(move |_| action_result(remove_repository_action.value().get(), overview, toast));
    Effect::new(move |_| action_result(install_action.value().get(), overview, toast));
    Effect::new(move |_| action_result(disable_action.value().get(), overview, toast));
    Effect::new(move |_| action_result(uninstall_action.value().get(), overview, toast));
    Effect::new(move |_| action_result(setting_action.value().get(), overview, toast));
    Effect::new(move |_| match check_action.value().get() {
        Some(Ok(message)) => {
            toast.push(message, ToastSeverity::Info);
            overview.refetch();
        }
        Some(Err(error)) => toast.push(error.to_string(), ToastSeverity::Error),
        None => {}
    });
    Effect::new(move |_| match enable_action.value().get() {
        Some(Ok(message)) => {
            toast.push(message, ToastSeverity::Success);
            overview.refetch();
        }
        Some(Err(error)) => toast.push(error.to_string(), ToastSeverity::Error),
        None => {}
    });

    view! {
        <div class=style::page data-testid="addons-settings-page">
            <div class=style::toolbar>
                <h1 class=style::page_title>{tr(TextKey::Addons)}</h1>
            </div>

            {move || current.get().map(|data| {
                let managed = data.repositories_are_deployment_managed;
                let installed_versions = data
                    .installed
                    .iter()
                    .map(|item| (item.addon_id.clone(), item.version.clone()))
                    .collect::<BTreeMap<_, _>>();
                let official_repository_ids = data
                    .repositories
                    .iter()
                    .filter(|repository| repository.is_official)
                    .map(|repository| repository.id.clone())
                    .collect::<BTreeSet<_>>();
                view! {
                    <section class=style::form_section>
                        <div class=style::toolbar>
                            <h2 class=style::page_title>{tr(TextKey::AddonRepositories)}</h2>
                            <button
                                type="button"
                                class=style::btn_primary
                                disabled=managed
                                on:click=move |_| {
                                    repository_id.set(String::new());
                                    repository_name.set(String::new());
                                    repository_url.set(String::new());
                                    repository_key.set(String::new());
                                    repository_editor_open.set(true);
                                }
                            >{tr(TextKey::AddRepository)}</button>
                        </div>
                        <p class=style::cell_muted style:display=if managed { "" } else { "none" }>
                            {tr(TextKey::DeploymentManagedRepositories)}
                        </p>
                        <div
                            class=style::editor_form
                            data-testid="addon-repository-editor"
                            style:display=move || if repository_editor_open.get() { "" } else { "none" }
                        >
                            <label class=style::form_label for="addon-repository-name">{tr(TextKey::Name)}</label>
                            <input id="addon-repository-name" class=style::form_input prop:value=move || repository_name.get() on:input=move |event| repository_name.set(event_target_value(&event))/>
                            <label class=style::form_label for="addon-repository-url">{tr(TextKey::RepositoryUrl)}</label>
                            <input id="addon-repository-url" class=style::form_input prop:value=move || repository_url.get() on:input=move |event| repository_url.set(event_target_value(&event))/>
                            <label class=style::form_label for="addon-repository-id">{tr(TextKey::Id)}</label>
                            <input id="addon-repository-id" class=style::form_input prop:value=move || repository_id.get() on:input=move |event| repository_id.set(event_target_value(&event))/>
                            <label class=style::form_label for="addon-repository-key">{tr(TextKey::PublicSigningKey)}</label>
                            <textarea id="addon-repository-key" class=style::form_input prop:value=move || repository_key.get() on:input=move |event| repository_key.set(event_target_value(&event))></textarea>
                            <p class=style::cell_muted>{tr(TextKey::ThirdPartyRepositoryWarning)}</p>
                            <div class=style::toolbar_actions>
                                <button type="button" class=style::btn_secondary on:click=move |_| repository_editor_open.set(false)>{tr(TextKey::Cancel)}</button>
                                <button type="button" class=style::btn_primary on:click=move |_| {
                                    save_repository_action.dispatch(AddonRepositoryEditor {
                                        id: repository_id.get_untracked(),
                                        name: repository_name.get_untracked(),
                                        index_url: repository_url.get_untracked(),
                                        public_key: repository_key.get_untracked(),
                                        is_enabled: true,
                                    });
                                }>{tr(TextKey::Save)}</button>
                            </div>
                        </div>
                        {data.repositories.into_iter().map(|repository| {
                            let refresh_id = repository.id.clone();
                            let remove_id = repository.id.clone();
                            view! {
                                <article class=style::editor_form data-testid="addon-repository-row">
                                    <div class=style::toolbar>
                                        <div>
                                            <h3 class=style::page_title>{repository.name}</h3>
                                            <p class=style::cell_muted>{repository.id}</p>
                                            <p class=style::cell_muted>{repository.index_url}</p>
                                            <p class=style::cell_muted style:display=if repository.is_official { "" } else { "none" }>{tr(TextKey::Official)}</p>
                                            {repository.last_error.map(|error| view! { <p class=style::cell_muted>{error}</p> })}
                                        </div>
                                        <div class=style::toolbar_actions>
                                            <button type="button" class=style::btn_secondary on:click=move |_| { refresh_action.dispatch(refresh_id.clone()); }>{tr(TextKey::Refresh)}</button>
                                            <button type="button" class=style::btn_danger disabled=managed on:click=move |_| { remove_repository_action.dispatch(remove_id.clone()); }>{tr(TextKey::Delete)}</button>
                                        </div>
                                    </div>
                                </article>
                            }
                        }).collect_view()}
                    </section>

                    <section class=style::form_section>
                        <h2 class=style::page_title>{tr(TextKey::AvailableAddons)}</h2>
                        {data.available.into_iter().map(|available| {
                            let addon_id = available.addon.id.clone();
                            let repository_id = available.repository_id.clone();
                            let installed_version = installed_versions.get(&addon_id);
                            let is_installed = installed_version.is_some();
                            let can_install = available.host_compatible
                                && available.package_available
                                && installed_version.is_none_or(|version| version != &available.addon.version.to_string());
                            let is_official = official_repository_ids.contains(&repository_id);
                            let name = localized(&available.addon.name);
                            let summary = localized(&available.addon.summary);
                            let permissions = permissions_view(
                                &available.addon.capabilities,
                                &available.addon.permissions,
                            );
                            let dependencies = dependencies_view(&available.addon.dependencies);
                            view! {
                                <article class=style::editor_form data-testid="available-addon-row">
                                    <div class=style::toolbar>
                                        <div>
                                            <h3 class=style::page_title>{name}</h3>
                                            <p>{summary}</p>
                                            <p class=style::cell_muted>{format!("{} · {}", addon_id, available.addon.version)}</p>
                                            <p class=style::cell_muted>{if is_official { tr(TextKey::Official) } else { tr(TextKey::ThirdParty) }}</p>
                                            <p class=style::cell_muted style:display=if is_official { "none" } else { "" }>{tr(TextKey::ThirdPartyRepositoryWarning)}</p>
                                            {permissions}
                                            {dependencies}
                                            <p class=style::cell_muted style:display=if available.host_compatible { "none" } else { "" }>{tr(TextKey::Incompatible)}</p>
                                            <p class=style::cell_muted style:display=if available.package_available { "none" } else { "" }>{tr(TextKey::PackageUnavailable)}</p>
                                        </div>
                                        <button type="button" class=style::btn_primary disabled=!can_install on:click=move |_| {
                                            install_action.dispatch((repository_id.clone(), addon_id.clone()));
                                        }>{if is_installed { tr(TextKey::UpdateAddon) } else { tr(TextKey::Install) }}</button>
                                    </div>
                                </article>
                            }
                        }).collect_view()}
                    </section>

                    <section class=style::form_section>
                        <h2 class=style::page_title>{tr(TextKey::InstalledAddons)}</h2>
                        {data.installed.into_iter().map(|installed| {
                            let check_id = installed.addon_id.clone();
                            let toggle_id = installed.addon_id.clone();
                            let uninstall_id = installed.addon_id.clone();
                            let name = localized(&installed.manifest.name);
                            let summary = localized(&installed.manifest.summary);
                            let capabilities = installed
                                .manifest
                                .capabilities
                                .iter()
                                .map(|item| item.id.clone())
                                .collect::<Vec<_>>();
                            let permissions = permissions_view(
                                &capabilities,
                                &installed.manifest.permissions,
                            );
                            let dependencies = dependencies_view(&installed.manifest.dependencies);
                            let panels = installed
                                .manifest
                                .panels
                                .into_iter()
                                .map(panel_view)
                                .collect_view();
                            let stored = installed.settings.into_iter()
                                .map(|setting| (setting.key.clone(), setting))
                                .collect::<BTreeMap<_, _>>();
                            view! {
                                <article class=style::editor_form data-testid="installed-addon-row">
                                    <div class=style::toolbar>
                                        <div>
                                            <h3 class=style::page_title>{name}</h3>
                                            <p>{summary}</p>
                                            <p class=style::cell_muted>{format!("{} · {}", installed.addon_id, installed.version)}</p>
                                            {permissions}
                                            {dependencies}
                                            {installed.last_check_message.map(|message| view! { <p class=style::cell_muted>{message}</p> })}
                                        </div>
                                        <div class=style::toolbar_actions>
                                            <button type="button" class=style::btn_secondary on:click=move |_| { check_action.dispatch(check_id.clone()); }>{tr(TextKey::Check)}</button>
                                            <button type="button" class=style::btn_primary on:click=move |_| {
                                                if installed.enabled {
                                                    disable_action.dispatch(toggle_id.clone());
                                                } else {
                                                    enable_action.dispatch(toggle_id.clone());
                                                }
                                            }>{if installed.enabled { tr(TextKey::Disable) } else { tr(TextKey::Enable) }}</button>
                                            <button type="button" class=style::btn_danger on:click=move |_| { uninstall_action.dispatch(uninstall_id.clone()); }>{tr(TextKey::Uninstall)}</button>
                                        </div>
                                    </div>
                                    {panels}
                                    <h4>{tr(TextKey::Configuration)}</h4>
                                    {installed.manifest.settings.into_iter().map(|definition| {
                                        setting_editor(
                                            installed.addon_id.clone(),
                                            definition.clone(),
                                            stored.get(&definition.key).cloned(),
                                            setting_action,
                                        )
                                    }).collect_view()}
                                </article>
                            }
                        }).collect_view()}
                    </section>
                }
            })}
        </div>
    }
}

fn action_result(
    result: Option<Result<(), ServerFnError>>,
    overview: LocalResource<AddonOverview>,
    toast: ToastStore,
) {
    match result {
        Some(Ok(())) => overview.refetch(),
        Some(Err(error)) => toast.push(error.to_string(), ToastSeverity::Error),
        None => {}
    }
}

fn setting_editor(
    addon_id: String,
    definition: AddonSettingDefinition,
    stored: Option<AddonSettingRow>,
    action: Action<AddonSettingEditor, Result<(), ServerFnError>>,
) -> AnyView {
    let value = RwSignal::new(
        stored
            .as_ref()
            .and_then(|item| item.value.clone())
            .or_else(|| definition.default.clone())
            .unwrap_or_default(),
    );
    let reference_kind = RwSignal::new(
        stored
            .as_ref()
            .and_then(|item| item.reference_kind.clone())
            .unwrap_or_else(|| "environment".to_owned()),
    );
    let reference = RwSignal::new(stored.and_then(|item| item.reference).unwrap_or_default());
    let label = localized(&definition.label);
    let key = definition.key.clone();
    let input = match definition.kind {
        AddonSettingKind::Boolean => view! {
            <select class=style::form_select prop:value=move || value.get() on:change=move |event| value.set(event_target_value(&event))>
                <option value="false">{tr(TextKey::No)}</option>
                <option value="true">{tr(TextKey::Yes)}</option>
            </select>
        }.into_any(),
        AddonSettingKind::Enum => view! {
            <select class=style::form_select prop:value=move || value.get() on:change=move |event| value.set(event_target_value(&event))>
                {definition.options.into_iter().map(|option| view! { <option value=option.clone()>{option.clone()}</option> }).collect_view()}
            </select>
        }.into_any(),
        AddonSettingKind::SecretReference => view! {
            <div class=style::form_row>
                <select class=style::form_select prop:value=move || reference_kind.get() on:change=move |event| reference_kind.set(event_target_value(&event))>
                    <option value="environment">{tr(TextKey::EnvironmentVariable)}</option>
                    <option value="file">{tr(TextKey::SecretFile)}</option>
                </select>
                <input class=style::form_input prop:value=move || reference.get() on:input=move |event| reference.set(event_target_value(&event))/>
            </div>
        }.into_any(),
        _ => view! {
            <input class=style::form_input prop:value=move || value.get() on:input=move |event| value.set(event_target_value(&event))/>
        }.into_any(),
    };
    let is_secret = definition.kind == AddonSettingKind::SecretReference;
    view! {
        <div class=style::form_row data-testid="addon-setting-row">
            <label class=style::form_label>{format!("{label} ({key})")}</label>
            {input}
            <button type="button" class=style::btn_secondary on:click=move |_| {
                action.dispatch(AddonSettingEditor {
                    addon_id: addon_id.clone(),
                    key: key.clone(),
                    value: (!is_secret).then(|| value.get_untracked()),
                    reference_kind: is_secret.then(|| reference_kind.get_untracked()),
                    reference: is_secret.then(|| reference.get_untracked()),
                });
            }>{tr(TextKey::Save)}</button>
        </div>
    }
    .into_any()
}

fn localized(values: &BTreeMap<String, String>) -> String {
    localized_for_code(values, use_i18n().code())
}

fn localized_for_code(values: &BTreeMap<String, String>, locale: &str) -> String {
    values
        .iter()
        .find(|(code, _)| code.eq_ignore_ascii_case(locale))
        .or_else(|| {
            values
                .iter()
                .find(|(code, _)| code.eq_ignore_ascii_case("en-US"))
        })
        .map(|(_, value)| value)
        .or_else(|| values.values().next())
        .cloned()
        .unwrap_or_default()
}

fn permissions_view(capabilities: &[String], permissions: &AddonPermissions) -> AnyView {
    let capabilities = capabilities.join(", ");
    let networks = permissions.network_hosts.join(", ");
    let commands = permissions.external_commands.join(", ");
    view! {
        <div class=style::cell_muted data-testid="addon-permissions">
            <strong>{tr(TextKey::Permissions)}</strong>
            <p>{format!("{}: {}", tr(TextKey::Capabilities), capabilities)}</p>
            <p>{format!("{}: {}", tr(TextKey::Networks), if networks.is_empty() { tr(TextKey::No).to_owned() } else { networks })}</p>
            <p>{format!("{}: {}", tr(TextKey::MediaReadAccess), if permissions.media_read { tr(TextKey::Yes) } else { tr(TextKey::No) })}</p>
            <p>{format!("{}: {}", tr(TextKey::Ffmpeg), if permissions.managed_ffmpeg { tr(TextKey::Yes) } else { tr(TextKey::No) })}</p>
            <p>{format!("{}: {}", tr(TextKey::ExternalCommands), if commands.is_empty() { tr(TextKey::No).to_owned() } else { commands })}</p>
        </div>
    }
    .into_any()
}

fn dependencies_view(dependencies: &[AddonDependency]) -> AnyView {
    if dependencies.is_empty() {
        return ().into_any();
    }
    let dependencies = dependencies
        .iter()
        .map(|dependency| {
            let optional = if dependency.optional {
                format!(" ({})", tr(TextKey::Optional))
            } else {
                String::new()
            };
            format!("{} {}{optional}", dependency.id, dependency.version)
        })
        .collect::<Vec<_>>()
        .join(", ");
    view! {
        <p class=style::cell_muted>
            <strong>{format!("{}: ", tr(TextKey::Dependencies))}</strong>
            {dependencies}
        </p>
    }
    .into_any()
}

fn panel_view(panel: AddonPanel) -> AnyView {
    view! {
        <section class=style::form_section data-testid="addon-declarative-panel">
            <h4>{localized(&panel.title)}</h4>
            {panel.blocks.into_iter().map(|block| match block {
                AddonPanelBlock::Text { text } => view! { <p>{localized(&text)}</p> }.into_any(),
                AddonPanelBlock::Status { label, code } => view! {
                    <p><strong>{localized(&label)}</strong>{format!(": {code}")}</p>
                }.into_any(),
                AddonPanelBlock::KeyValue { label, value } => view! {
                    <p><strong>{localized(&label)}</strong>{format!(": {value}")}</p>
                }.into_any(),
                AddonPanelBlock::Link { label, url } => view! {
                    <p><a href=url target="_blank" rel="noopener noreferrer">{localized(&label)}</a></p>
                }.into_any(),
            }).collect_view()}
        </section>
    }
    .into_any()
}

#[cfg(test)]
mod tests {
    use super::localized_for_code;
    use std::collections::BTreeMap;

    #[test]
    fn localized_manifest_text_matches_bcp47_codes_case_insensitively() {
        let values = BTreeMap::from([
            ("de-DE".to_owned(), "Deutsch".to_owned()),
            ("en-US".to_owned(), "English".to_owned()),
        ]);

        assert_eq!(localized_for_code(&values, "de-de"), "Deutsch");
        assert_eq!(localized_for_code(&values, "unknown"), "English");
    }
}
