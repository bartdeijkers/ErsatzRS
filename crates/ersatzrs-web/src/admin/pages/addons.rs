#![allow(clippy::too_many_lines)]

use std::collections::{BTreeMap, BTreeSet};

use ersatzrs_addon_contract::{
    AddonDependency, AddonPanel, AddonPanelBlock, AddonPermissions, AddonSettingDefinition,
    AddonSettingKind,
};
use leptos::{html, prelude::*};
use leptos_router::hooks::{use_params_map, use_query_map};

use crate::admin::i18n::{provide_i18n_context, tr, use_i18n, TextKey};
#[cfg(feature = "ssr")]
use crate::admin::server_fns::addons::load_addon_detail;
use crate::admin::server_fns::addons::{
    check_addon_package, disable_addon_package, enable_addon_package, install_addon_package,
    load_addon_overview, refresh_addon_repository, remove_addon_repository,
    save_addon_configuration, save_addon_repository, set_unverified_addon_sources,
    uninstall_addon_package, AddonDetail, AddonOverview, AddonRepositoryEditor, AddonSettingEditor,
    AddonSettingRow, AvailableAddonRow, InstalledAddonRow,
};
use crate::admin::ui::primitives::dialog::Modal;
use crate::admin::ui::primitives::resource_state::AdminErrorState;
use crate::admin::ui::primitives::toast::{load_or_notify, use_toast, ToastSeverity, ToastStore};

stylance::import_crate_style!(style, "src/admin/pages/addons.module.css");

#[component]
pub fn AddonsSettingsPage() -> impl IntoView {
    let query = use_query_map();
    let (initial_view, initial_search, initial_filter) = query.with_untracked(|params| {
        (
            params
                .get("view")
                .filter(|value| {
                    matches!(
                        value.as_str(),
                        "installed" | "browse" | "updates" | "sources"
                    )
                })
                .unwrap_or_else(|| "installed".to_owned()),
            params.get("search").unwrap_or_default(),
            params.get("state").unwrap_or_else(|| "all".to_owned()),
        )
    });
    view! { <AddonsSettingsIsland initial_view initial_search initial_filter/> }
}

#[component]
pub fn AddonDetailPage() -> impl IntoView {
    let params = use_params_map();
    let query = use_query_map();
    let addon_id = params.with_untracked(|params| params.get("addon_id").unwrap_or_default());
    let source = query.with_untracked(|params| params.get("source"));
    match load_addon_detail_initial(addon_id) {
        Ok(Some(detail)) => view! { <AddonDetailIsland detail source/> }.into_any(),
        Ok(None) => view! {
            <div class=style::not_found>
                <h1>{tr(TextKey::AddonNotFound)}</h1>
                <a href="/settings/addons">{tr(TextKey::BackToAddons)}</a>
            </div>
        }
        .into_any(),
        Err(error) => view! { <AdminErrorState error/> }.into_any(),
    }
}

fn load_addon_detail_initial(addon_id: String) -> Result<Option<AddonDetail>, String> {
    #[cfg(feature = "ssr")]
    {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(load_addon_detail(addon_id))
                .map_err(|error| error.to_string())
        })
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = addon_id;
        Ok(None)
    }
}

#[island]
fn AddonsSettingsIsland(
    initial_view: String,
    initial_search: String,
    initial_filter: String,
) -> impl IntoView {
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
    let unknown_sources_confirm_open = RwSignal::new(false);
    let remove_confirm_open = RwSignal::new(false);
    let remove_target = RwSignal::new(String::new());
    let addon_file = NodeRef::<html::Input>::new();
    let repository_file = NodeRef::<html::Input>::new();

    let save_repository_action =
        Action::new(move |editor: &AddonRepositoryEditor| save_addon_repository(editor.clone()));
    let refresh_action = Action::new(move |id: &String| refresh_addon_repository(id.clone()));
    let remove_action = Action::new(move |id: &String| remove_addon_repository(id.clone()));
    let unknown_sources_action =
        Action::new(move |enabled: &bool| set_unverified_addon_sources(*enabled));
    let refresh_all_action = Action::new(move |ids: &Vec<String>| {
        let ids = ids.clone();
        async move {
            let mut failures = Vec::new();
            for id in ids {
                if let Err(error) = refresh_addon_repository(id.clone()).await {
                    failures.push(format!("{id}: {error}"));
                }
            }
            if failures.is_empty() {
                Ok(())
            } else {
                Err(ServerFnError::new(failures.join("; ")))
            }
        }
    });

    Effect::new(move |_| match save_repository_action.value().get() {
        Some(Ok(())) => {
            repository_editor_open.set(false);
            overview.refetch();
        }
        Some(Err(error)) => toast.push(error.to_string(), ToastSeverity::Error),
        None => {}
    });
    Effect::new(move |_| action_result(refresh_action.value().get(), overview, toast));
    Effect::new(move |_| action_result(remove_action.value().get(), overview, toast));
    Effect::new(move |_| action_result(unknown_sources_action.value().get(), overview, toast));
    Effect::new(move |_| action_result(refresh_all_action.value().get(), overview, toast));

    let view_name = initial_view.clone();
    let search_value = initial_search.clone();
    let filter_value = initial_filter.clone();
    let upload_addon = move |_| {
        #[cfg(target_arch = "wasm32")]
        if let Some(input) = addon_file.get() {
            leptos::task::spawn_local(async move {
                match upload_selected_file(input, "/api/addons/package").await {
                    Ok(()) => {
                        toast.push(tr(TextKey::Install).to_owned(), ToastSeverity::Success);
                        overview.refetch();
                    }
                    Err(error) => toast.push(error, ToastSeverity::Error),
                }
            });
        }
    };
    let upload_repository = move |_| {
        #[cfg(target_arch = "wasm32")]
        if let Some(input) = repository_file.get() {
            leptos::task::spawn_local(async move {
                match upload_selected_file(input, "/api/addons/repository-package").await {
                    Ok(()) => {
                        toast.push(
                            tr(TextKey::AddRepository).to_owned(),
                            ToastSeverity::Success,
                        );
                        overview.refetch();
                    }
                    Err(error) => toast.push(error, ToastSeverity::Error),
                }
            });
        }
    };

    view! {
        <main class=style::page data-testid="addons-settings-page">
            <header class=style::page_header>
                <div>
                    <h1>{tr(TextKey::Addons)}</h1>
                    <p>{tr(TextKey::ThirdPartyRepositoryWarning)}</p>
                </div>
                <button
                    type="button"
                    class=style::btn_secondary
                    on:click=move |_| {
                        let ids = current
                            .get_untracked()
                            .map(|data| {
                                data.repositories
                                    .into_iter()
                                    .filter(|repository| repository.is_enabled)
                                    .map(|repository| repository.id)
                                    .collect()
                            })
                            .unwrap_or_default();
                        refresh_all_action.dispatch(ids);
                    }
                >{tr(TextKey::CheckForUpdates)}</button>
            </header>

            {move || current.get().map(|data| manager_body(
                data,
                &view_name,
                &search_value,
                &filter_value,
                repository_editor_open,
                repository_id,
                repository_name,
                repository_url,
                repository_key,
                save_repository_action,
                refresh_action,
                remove_target,
                remove_confirm_open,
                unknown_sources_confirm_open,
                unknown_sources_action,
                addon_file,
                repository_file,
                upload_addon,
                upload_repository,
            ))}

            <Modal
                open=unknown_sources_confirm_open
                title=tr(TextKey::UnknownSources).to_owned()
                on_close=Callback::new(move |confirmed| {
                    if confirmed {
                        unknown_sources_action.dispatch(true);
                    }
                })
            >
                <p>{tr(TextKey::UnknownSourcesHelp)}</p>
            </Modal>
            <Modal
                open=remove_confirm_open
                title=tr(TextKey::ConfirmDelete).to_owned()
                destructive=true
                on_close=Callback::new(move |confirmed| {
                    if confirmed {
                        remove_action.dispatch(remove_target.get_untracked());
                    }
                })
            >
                <p>{tr(TextKey::AreYouSureDelete)}" "<strong>{move || remove_target.get()}</strong>"?"</p>
            </Modal>
        </main>
    }
}

#[allow(clippy::too_many_arguments)]
fn manager_body(
    data: AddonOverview,
    view_name: &str,
    search_value: &str,
    filter_value: &str,
    repository_editor_open: RwSignal<bool>,
    repository_id: RwSignal<String>,
    repository_name: RwSignal<String>,
    repository_url: RwSignal<String>,
    repository_key: RwSignal<String>,
    save_repository_action: Action<AddonRepositoryEditor, Result<(), ServerFnError>>,
    refresh_action: Action<String, Result<(), ServerFnError>>,
    remove_target: RwSignal<String>,
    remove_confirm_open: RwSignal<bool>,
    unknown_sources_confirm_open: RwSignal<bool>,
    unknown_sources_action: Action<bool, Result<(), ServerFnError>>,
    addon_file: NodeRef<html::Input>,
    repository_file: NodeRef<html::Input>,
    upload_addon: impl Fn(leptos::ev::MouseEvent) + Copy + Send + 'static,
    upload_repository: impl Fn(leptos::ev::MouseEvent) + Copy + Send + 'static,
) -> AnyView {
    let installed_count = data.installed.len();
    let enabled_count = data.installed.iter().filter(|item| item.enabled).count();
    let installed_versions = data
        .installed
        .iter()
        .map(|item| (item.addon_id.clone(), item.version.clone()))
        .collect::<BTreeMap<_, _>>();
    let update_ids = data
        .available
        .iter()
        .filter(|offer| {
            installed_versions
                .get(&offer.addon.id)
                .and_then(|version| semver::Version::parse(version).ok())
                .is_some_and(|version| offer.addon.version > version)
        })
        .map(|offer| offer.addon.id.clone())
        .collect::<BTreeSet<_>>();
    let problem_count = data
        .installed
        .iter()
        .filter(|item| {
            item.last_check_code
                .as_deref()
                .is_some_and(|code| code != "ready")
        })
        .count()
        + data
            .repositories
            .iter()
            .filter(|item| item.last_error.is_some())
            .count();
    let query = search_value.trim().to_ascii_lowercase();
    let repository_tiers = data
        .repositories
        .iter()
        .map(|repository| (repository.id.clone(), repository.trust_tier.clone()))
        .collect::<BTreeMap<_, _>>();
    let installed_cards = data
        .installed
        .iter()
        .filter(|item| addon_matches(&localized(&item.manifest.name), &item.addon_id, &query))
        .filter(|item| match filter_value {
            "enabled" => item.enabled,
            "disabled" => !item.enabled,
            "verified" | "unverified" | "sideloaded" => item.trust_tier == filter_value,
            _ => true,
        })
        .cloned()
        .collect::<Vec<_>>();
    let browse_cards = data
        .available
        .iter()
        .filter(|item| addon_matches(&localized(&item.addon.name), &item.addon.id, &query))
        .filter(|item| match filter_value {
            "compatible" => item.host_compatible && item.package_available,
            "incompatible" => !item.host_compatible || !item.package_available,
            _ => true,
        })
        .cloned()
        .collect::<Vec<_>>();
    let update_cards = browse_cards
        .iter()
        .filter(|item| update_ids.contains(&item.addon.id))
        .cloned()
        .collect::<Vec<_>>();
    let managed = data.repositories_are_deployment_managed;
    let unknown_enabled = data.unverified_sources_enabled;

    view! {
        <section class=style::stats aria-label=tr(TextKey::Status)>
            <div><strong>{installed_count}</strong><span>{tr(TextKey::InstalledAddons)}</span></div>
            <div><strong>{enabled_count}</strong><span>{tr(TextKey::Enabled)}</span></div>
            <div><strong>{update_ids.len()}</strong><span>{tr(TextKey::AddonUpdates)}</span></div>
            <div><strong>{problem_count}</strong><span>{tr(TextKey::Status)}</span></div>
        </section>

        <nav class=style::tabs aria-label=tr(TextKey::Addons)>
            {manager_tab("installed", tr(TextKey::InstalledAddons), view_name)}
            {manager_tab("browse", tr(TextKey::BrowseAddons), view_name)}
            {manager_tab("updates", tr(TextKey::AddonUpdates), view_name)}
            {manager_tab("sources", tr(TextKey::AddonSources), view_name)}
        </nav>

        <form class=style::filters method="get" style:display=if view_name == "sources" { "none" } else { "" }>
            <input type="hidden" name="view" value=view_name.to_owned()/>
            <input class=style::search type="search" name="search" value=search_value.to_owned() placeholder=tr(TextKey::Search)/>
            <select class=style::select name="state">
                <option value="all" selected=filter_value == "all">{tr(TextKey::All)}</option>
                {filter_options(view_name, filter_value)}
            </select>
            <button class=style::btn_primary type="submit">{tr(TextKey::Search)}</button>
        </form>

        <section style:display=if view_name == "installed" { "" } else { "none" }>
            {addon_grid(installed_cards.into_iter().map(installed_card).collect(), tr(TextKey::NoAddonsMatch))}
        </section>
        <section style:display=if view_name == "browse" { "" } else { "none" }>
            {addon_grid(browse_cards.into_iter().map(|item| {
                let trust = repository_tiers.get(&item.repository_id).cloned();
                available_card(item, trust)
            }).collect(), tr(TextKey::NoAddonsMatch))}
        </section>
        <section style:display=if view_name == "updates" { "" } else { "none" }>
            {addon_grid(update_cards.into_iter().map(|item| {
                let trust = repository_tiers.get(&item.repository_id).cloned();
                available_card(item, trust)
            }).collect(), tr(TextKey::NoAddonsMatch))}
        </section>

        <section class=style::sources style:display=if view_name == "sources" { "" } else { "none" }>
            <article class=style::security_card>
                <div>
                    <h2>{tr(TextKey::UnknownSources)}</h2>
                    <p>{tr(TextKey::UnknownSourcesHelp)}</p>
                </div>
                <button
                    type="button"
                    class=if unknown_enabled { style::btn_danger } else { style::btn_secondary }
                    on:click=move |_| {
                        if unknown_enabled {
                            unknown_sources_action.dispatch(false);
                        } else {
                            unknown_sources_confirm_open.set(true);
                        }
                    }
                >{if unknown_enabled { tr(TextKey::Disable) } else { tr(TextKey::Enable) }}</button>
            </article>

            <div class=style::source_actions>
                <label class=style::file_action aria-disabled=!unknown_enabled>
                    {tr(TextKey::InstallFromFile)}
                    <input node_ref=addon_file type="file" accept=".zip,application/zip" disabled=!unknown_enabled/>
                </label>
                <button type="button" class=style::btn_secondary disabled=!unknown_enabled on:click=upload_addon>{tr(TextKey::Install)}</button>
                <label class=style::file_action aria-disabled=!unknown_enabled>
                    {tr(TextKey::InstallRepositoryFromFile)}
                    <input node_ref=repository_file type="file" accept=".zip,application/zip" disabled=!unknown_enabled || managed/>
                </label>
                <button type="button" class=style::btn_secondary disabled=!unknown_enabled || managed on:click=upload_repository>{tr(TextKey::Install)}</button>
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
            <p class=style::muted style:display=if managed { "" } else { "none" }>{tr(TextKey::DeploymentManagedRepositories)}</p>

            <div class=style::repository_editor data-testid="addon-repository-editor" style:display=move || if repository_editor_open.get() { "" } else { "none" }>
                <label for="addon-repository-name">{tr(TextKey::Name)}<input id="addon-repository-name" prop:value=move || repository_name.get() on:input=move |event| repository_name.set(event_target_value(&event))/></label>
                <label for="addon-repository-url">{tr(TextKey::RepositoryUrl)}<input id="addon-repository-url" prop:value=move || repository_url.get() on:input=move |event| repository_url.set(event_target_value(&event))/></label>
                <label for="addon-repository-id">{tr(TextKey::Id)}<input id="addon-repository-id" prop:value=move || repository_id.get() on:input=move |event| repository_id.set(event_target_value(&event))/></label>
                <label for="addon-repository-key">{tr(TextKey::PublicSigningKey)}<textarea id="addon-repository-key" prop:value=move || repository_key.get() on:input=move |event| repository_key.set(event_target_value(&event))></textarea></label>
                <div class=style::actions>
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

            <h2 class=style::section_heading>{tr(TextKey::AddonRepositories)}</h2>
            <div class=style::repository_list>
                {data.repositories.into_iter().map(|repository| {
                    let refresh_id = repository.id.clone();
                    let remove_id = repository.id.clone();
                    let edit_id = repository.id.clone();
                    let edit_name = repository.name.clone();
                    let edit_url = repository.index_url.clone();
                    let edit_key = repository.public_key.clone();
                    let toggle_editor = AddonRepositoryEditor {
                        id: repository.id.clone(),
                        name: repository.name.clone(),
                        index_url: repository.index_url.clone(),
                        public_key: repository.public_key.clone(),
                        is_enabled: !repository.is_enabled,
                    };
                    view! {
                        <article class=style::repository data-testid="addon-repository-row">
                            <div>
                                <div class=style::card_title_row>
                                    <h3>{repository.name}</h3>
                                    {status_badge(&repository.trust_tier)}
                                    {repository.is_official.then(|| status_badge("official"))}
                                </div>
                                <p class=style::muted>{repository.id}</p>
                                <p class=style::muted>{repository.index_url}</p>
                                <p class=style::error>{repository.last_error.unwrap_or_default()}</p>
                            </div>
                            <div class=style::actions>
                                <button type="button" class=style::btn_secondary disabled=managed on:click=move |_| {
                                    repository_id.set(edit_id.clone());
                                    repository_name.set(edit_name.clone());
                                    repository_url.set(edit_url.clone());
                                    repository_key.set(edit_key.clone());
                                    repository_editor_open.set(true);
                                }>{tr(TextKey::Edit)}</button>
                                <button type="button" class=style::btn_secondary disabled=managed on:click=move |_| {
                                    save_repository_action.dispatch(toggle_editor.clone());
                                }>{if repository.is_enabled { tr(TextKey::Disable) } else { tr(TextKey::Enable) }}</button>
                                <button type="button" class=style::btn_secondary disabled=(repository.is_official && repository.public_key.trim().is_empty()) || (repository.trust_tier == "unverified" && !unknown_enabled) on:click=move |_| { refresh_action.dispatch(refresh_id.clone()); }>{tr(TextKey::Refresh)}</button>
                                <button type="button" class=style::btn_danger disabled=managed || repository.is_official on:click=move |_| {
                                    remove_target.set(remove_id.clone());
                                    remove_confirm_open.set(true);
                                }>{tr(TextKey::Delete)}</button>
                            </div>
                        </article>
                    }
                }).collect_view()}
            </div>
        </section>
    }
    .into_any()
}

fn manager_tab(view: &'static str, label: &'static str, selected: &str) -> AnyView {
    let href = format!("/settings/addons?view={view}");
    view! { <a href=href class=if selected == view { style::tab_active } else { style::tab }>{label}</a> }.into_any()
}

fn filter_options(view_name: &str, selected: &str) -> AnyView {
    if view_name == "installed" {
        view! {
            <option value="enabled" selected=selected == "enabled">{tr(TextKey::Enabled)}</option>
            <option value="disabled" selected=selected == "disabled">{tr(TextKey::Disabled)}</option>
            <option value="verified" selected=selected == "verified">{tr(TextKey::Verified)}</option>
            <option value="unverified" selected=selected == "unverified">{tr(TextKey::Unverified)}</option>
            <option value="sideloaded" selected=selected == "sideloaded">{tr(TextKey::Sideloaded)}</option>
        }
        .into_any()
    } else {
        view! {
            <option value="compatible" selected=selected == "compatible">{tr(TextKey::AvailableAddons)}</option>
            <option value="incompatible" selected=selected == "incompatible">{tr(TextKey::Incompatible)}</option>
        }
        .into_any()
    }
}

fn addon_grid(cards: Vec<AnyView>, empty: &'static str) -> AnyView {
    let is_empty = cards.is_empty();
    view! {
        <div class=style::grid>{cards}</div>
        <div class=style::empty style:display=if is_empty { "" } else { "none" }>{empty}</div>
    }
    .into_any()
}

fn installed_card(item: InstalledAddonRow) -> AnyView {
    let name = localized(&item.manifest.name);
    let summary = localized(&item.manifest.summary);
    let href = format!("/settings/addons/{}", item.addon_id);
    view! {
        <article class=style::card data-testid="installed-addon-row">
            {addon_icon(item.icon_url, &name)}
            <div class=style::card_body>
                <div class=style::card_title_row><h2>{name}</h2>{status_badge(if item.enabled { "enabled" } else { "disabled" })}</div>
                <p>{summary}</p>
                <div class=style::badges>{status_badge(&item.trust_tier)}{item.last_check_code.map(|code| status_badge(&code))}</div>
                <p class=style::muted>{format!("{} · {}", item.addon_id, item.version)}</p>
            </div>
            <a class=style::card_link href=href aria-label=tr(TextKey::Settings)>{tr(TextKey::Settings)}</a>
        </article>
    }
    .into_any()
}

fn available_card(item: AvailableAddonRow, trust: Option<String>) -> AnyView {
    let name = localized(&item.addon.name);
    let summary = localized(&item.addon.summary);
    let href = format!(
        "/settings/addons/{}?source={}",
        item.addon.id, item.repository_id
    );
    view! {
        <article class=style::card data-testid="available-addon-row">
            {addon_icon(item.icon_url, &name)}
            <div class=style::card_body>
                <div class=style::card_title_row><h2>{name}</h2>{status_badge(&item.addon.version.to_string())}</div>
                <p>{summary}</p>
                <div class=style::badges>
                    {trust.map(|trust| status_badge(&trust))}
                    {(!item.host_compatible).then(|| status_badge("incompatible"))}
                    {(!item.package_available).then(|| status_badge("unavailable"))}
                </div>
                <p class=style::muted>{format!("{} · {}", item.addon.id, item.repository_id)}</p>
            </div>
            <a class=style::card_link href=href aria-label=tr(TextKey::AddonOverview)>{tr(TextKey::AddonOverview)}</a>
        </article>
    }
    .into_any()
}

fn addon_icon(url: Option<String>, name: &str) -> AnyView {
    let placeholder = name
        .chars()
        .next()
        .unwrap_or('#')
        .to_uppercase()
        .to_string();
    match url {
        Some(url) => view! { <img class=style::icon src=url alt="" loading="lazy"/> }.into_any(),
        None => view! { <div class=style::icon_placeholder aria-hidden="true">{placeholder}</div> }
            .into_any(),
    }
}

fn status_badge(code: &str) -> AnyView {
    let normalized = code.to_ascii_lowercase();
    let class = if matches!(
        normalized.as_str(),
        "ready" | "enabled" | "verified" | "official"
    ) {
        style::badge_good
    } else if matches!(
        normalized.as_str(),
        "unverified" | "sideloaded" | "warning" | "disabled"
    ) {
        style::badge_warn
    } else {
        style::badge_neutral
    };
    let label = match normalized.as_str() {
        "verified" => tr(TextKey::Verified).to_owned(),
        "unverified" => tr(TextKey::Unverified).to_owned(),
        "sideloaded" => tr(TextKey::Sideloaded).to_owned(),
        "enabled" => tr(TextKey::Enabled).to_owned(),
        "disabled" => tr(TextKey::Disabled).to_owned(),
        "official" => tr(TextKey::Official).to_owned(),
        "incompatible" => tr(TextKey::Incompatible).to_owned(),
        "unavailable" => tr(TextKey::PackageUnavailable).to_owned(),
        _ => code.to_owned(),
    };
    view! { <span class=format!("{} {}", style::badge, class)>{label}</span> }.into_any()
}

fn addon_matches(name: &str, id: &str, query: &str) -> bool {
    query.is_empty()
        || name.to_ascii_lowercase().contains(query)
        || id.to_ascii_lowercase().contains(query)
}

#[island]
fn AddonDetailIsland(detail: AddonDetail, source: Option<String>) -> impl IntoView {
    provide_i18n_context();
    let toast = use_toast();
    let install_confirm = RwSignal::new(false);
    let toggle_confirm = RwSignal::new(false);
    let uninstall_confirm = RwSignal::new(false);
    let selected_offer = select_offer(&detail, source.as_deref());
    let installed = detail.installed.clone();
    let addon_id = detail.addon_id.clone();
    let install_target = selected_offer
        .as_ref()
        .map(|offer| (offer.repository_id.clone(), offer.addon.id.clone()));

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
    action_reload_effect(install_action.value().into(), toast);
    action_reload_effect(disable_action.value().into(), toast);
    action_reload_effect(uninstall_action.value().into(), toast);
    action_reload_effect(setting_action.value().into(), toast);
    Effect::new(move |_| match check_action.value().get() {
        Some(Ok(message)) => toast.push(message, ToastSeverity::Info),
        Some(Err(error)) => toast.push(error.to_string(), ToastSeverity::Error),
        None => {}
    });
    Effect::new(move |_| match enable_action.value().get() {
        Some(Ok(message)) => {
            toast.push(message, ToastSeverity::Success);
            reload_page();
        }
        Some(Err(error)) => toast.push(error.to_string(), ToastSeverity::Error),
        None => {}
    });

    let (
        name,
        summary,
        icon_url,
        version,
        trust,
        source_id,
        compatible,
        permissions,
        dependencies,
        license,
        source_url,
    ) = detail_identity(&detail, selected_offer.as_ref());
    let is_installed = installed.is_some();
    let is_enabled = installed.as_ref().is_some_and(|item| item.enabled);
    let update_available = match (&installed, &selected_offer) {
        (Some(installed), Some(offer)) => semver::Version::parse(&installed.version)
            .is_ok_and(|version| offer.addon.version > version),
        (None, Some(_)) => true,
        _ => false,
    };
    let can_install = compatible && install_target.is_some() && update_available;
    let selected_offer_note = is_installed
        .then(|| {
            selected_offer.as_ref().map(|offer| {
                format!(
                    "{} {} · {} {}",
                    tr(TextKey::AvailableVersion),
                    offer.addon.version,
                    tr(TextKey::Source),
                    offer.repository_id
                )
            })
        })
        .flatten();
    let stored = installed
        .as_ref()
        .map(|item| {
            item.settings
                .iter()
                .cloned()
                .map(|setting| (setting.key.clone(), setting))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let setting_definitions = installed
        .as_ref()
        .map(|item| item.manifest.settings.clone())
        .unwrap_or_default();
    let panels = installed
        .as_ref()
        .map(|item| item.manifest.panels.clone())
        .unwrap_or_default();
    let addon_id_for_check = addon_id.clone();
    let addon_id_for_settings = addon_id.clone();
    let addon_id_for_toggle = addon_id.clone();
    let addon_id_for_uninstall = addon_id.clone();
    let summary_for_modal = summary.clone();

    view! {
        <main class=style::detail data-testid="addon-detail-page">
            <a class=style::back href="/settings/addons">{"← "}{tr(TextKey::BackToAddons)}</a>
            <header class=style::detail_hero>
                {addon_icon(icon_url, &name)}
                <div class=style::detail_heading>
                    <h1>{name}</h1>
                    <p>{summary.clone()}</p>
                    <div class=style::badges>{status_badge(&trust)}{status_badge(if is_enabled { "enabled" } else if is_installed { "disabled" } else { "available" })}</div>
                    <p class=style::muted>{format!("{} · {} {} · {} {}", addon_id, tr(TextKey::Version), version, tr(TextKey::Source), source_id)}</p>
                    {selected_offer_note.map(|note| view! { <p class=style::muted>{note}</p> })}
                </div>
                <div class=style::hero_actions>
                    <button type="button" class=style::btn_primary disabled=!can_install on:click=move |_| install_confirm.set(true)>
                        {if is_installed { tr(TextKey::UpdateAddon) } else { tr(TextKey::Install) }}
                    </button>
                    <button type="button" class=style::btn_secondary disabled=!is_installed on:click=move |_| toggle_confirm.set(true)>
                        {if is_enabled { tr(TextKey::Disable) } else { tr(TextKey::Enable) }}
                    </button>
                    <button type="button" class=style::btn_secondary disabled=!is_installed on:click=move |_| { check_action.dispatch(addon_id_for_check.clone()); }>{tr(TextKey::Check)}</button>
                    <button type="button" class=style::btn_danger disabled=!is_installed on:click=move |_| uninstall_confirm.set(true)>{tr(TextKey::Uninstall)}</button>
                </div>
            </header>

            <nav class=style::detail_tabs aria-label=tr(TextKey::Addon)>
                <a href="#overview">{tr(TextKey::AddonOverview)}</a>
                <a href="#configuration">{tr(TextKey::Configuration)}</a>
                <a href="#status">{tr(TextKey::Status)}</a>
            </nav>

            <section id="overview" class=style::detail_section>
                <h2>{tr(TextKey::AddonOverview)}</h2>
                <dl class=style::facts>
                    <dt>{tr(TextKey::Version)}</dt><dd>{version}</dd>
                    <dt>{tr(TextKey::Source)}</dt><dd>{source_id}</dd>
                    <dt>{tr(TextKey::License)}</dt><dd>{license}</dd>
                    <dt>{tr(TextKey::Capabilities)}</dt><dd>{permissions.0.join(", ")}</dd>
                </dl>
                // Distinct from the `Source` fact above, which names the
                // repository this add-on came from; this points at the
                // manifest's public source-code URL.
                <a
                    class=style::source_link
                    href=source_url
                    target="_blank"
                    rel="noopener noreferrer"
                >{tr(TextKey::ViewSourceCode)}</a>
                {permissions_view(&permissions.0, &permissions.1)}
                {dependencies_view(&dependencies)}
                <div class=style::offer_list>
                    {detail.offers.into_iter().map(|offer| {
                        let href = format!("/settings/addons/{}?source={}", offer.addon.id, offer.repository_id);
                        view! { <a href=href>{format!("{} · {}", offer.repository_id, offer.addon.version)}</a> }
                    }).collect_view()}
                </div>
            </section>

            <section id="configuration" class=style::detail_section>
                <h2>{tr(TextKey::Configuration)}</h2>
                <div class=style::settings_grid>
                    {setting_definitions.into_iter().map(|definition| setting_editor(
                        addon_id_for_settings.clone(),
                        definition.clone(),
                        stored.get(&definition.key).cloned(),
                        setting_action,
                    )).collect_view()}
                </div>
            </section>

            <section id="status" class=style::detail_section>
                <h2>{tr(TextKey::Status)}</h2>
                <p>{installed.as_ref().and_then(|item| item.last_check_message.clone()).unwrap_or_else(|| "—".to_owned())}</p>
                <p class=style::muted>{installed.as_ref().and_then(|item| item.last_check_at.clone()).unwrap_or_default()}</p>
            </section>

            {panels.into_iter().map(panel_view).collect_view()}

            <Modal open=install_confirm title=tr(TextKey::Install).to_owned() on_close=Callback::new(move |confirmed| {
                if confirmed {
                    if let Some(target) = install_target.clone() { install_action.dispatch(target); }
                }
            })>
                <p>{tr(TextKey::ThirdPartyRepositoryWarning)}</p>
                {permissions_view(&permissions.0, &permissions.1)}
            </Modal>
            <Modal open=toggle_confirm title=if is_enabled { tr(TextKey::Disable).to_owned() } else { tr(TextKey::Enable).to_owned() } on_close=Callback::new(move |confirmed| {
                if confirmed {
                    if is_enabled { disable_action.dispatch(addon_id_for_toggle.clone()); } else { enable_action.dispatch(addon_id_for_toggle.clone()); }
                }
            })><p>{summary_for_modal.clone()}</p></Modal>
            <Modal open=uninstall_confirm title=tr(TextKey::Uninstall).to_owned() destructive=true on_close=Callback::new(move |confirmed| {
                if confirmed { uninstall_action.dispatch(addon_id_for_uninstall.clone()); }
            })><p>{tr(TextKey::ThisActionCannotBeUndone)}</p></Modal>
        </main>
    }
}

fn select_offer(detail: &AddonDetail, source: Option<&str>) -> Option<AvailableAddonRow> {
    source
        .and_then(|source| {
            detail
                .offers
                .iter()
                .find(|offer| offer.repository_id == source)
        })
        .or_else(|| {
            detail.installed.as_ref().and_then(|installed| {
                installed.repository_id.as_deref().and_then(|source| {
                    detail
                        .offers
                        .iter()
                        .find(|offer| offer.repository_id == source)
                })
            })
        })
        .or_else(|| {
            detail.offers.iter().find(|offer| {
                detail.repositories.iter().any(|repository| {
                    repository.id == offer.repository_id && repository.is_official
                })
            })
        })
        .or_else(|| {
            detail
                .offers
                .iter()
                .find(|offer| offer.host_compatible && offer.package_available)
        })
        .or_else(|| detail.offers.first())
        .cloned()
}

#[allow(clippy::type_complexity)]
fn detail_identity(
    detail: &AddonDetail,
    offer: Option<&AvailableAddonRow>,
) -> (
    String,
    String,
    Option<String>,
    String,
    String,
    String,
    bool,
    (Vec<String>, AddonPermissions),
    Vec<AddonDependency>,
    String,
    String,
) {
    if let Some(installed) = &detail.installed {
        return (
            localized(&installed.manifest.name),
            localized(&installed.manifest.summary),
            installed.icon_url.clone(),
            installed.version.clone(),
            installed.trust_tier.clone(),
            installed
                .repository_id
                .clone()
                .unwrap_or_else(|| tr(TextKey::Sideloaded).to_owned()),
            true,
            (
                installed
                    .manifest
                    .capabilities
                    .iter()
                    .map(|item| item.id.clone())
                    .collect(),
                installed.manifest.permissions.clone(),
            ),
            installed.manifest.dependencies.clone(),
            installed.manifest.license.clone(),
            installed.manifest.source_url.clone(),
        );
    }
    let Some(offer) = offer else {
        return (
            detail.addon_id.clone(),
            String::new(),
            None,
            String::new(),
            "unverified".to_owned(),
            String::new(),
            false,
            (Vec::new(), AddonPermissions::default()),
            Vec::new(),
            String::new(),
            String::new(),
        );
    };
    let trust = detail
        .repositories
        .iter()
        .find(|repository| repository.id == offer.repository_id)
        .map_or_else(
            || "unverified".to_owned(),
            |repository| repository.trust_tier.clone(),
        );
    (
        localized(&offer.addon.name),
        localized(&offer.addon.summary),
        offer.icon_url.clone(),
        offer.addon.version.to_string(),
        trust,
        offer.repository_id.clone(),
        offer.host_compatible && offer.package_available,
        (
            offer.addon.capabilities.clone(),
            offer.addon.permissions.clone(),
        ),
        offer.addon.dependencies.clone(),
        offer.addon.license.clone(),
        offer.addon.source_url.clone(),
    )
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

fn action_reload_effect(value: Signal<Option<Result<(), ServerFnError>>>, toast: ToastStore) {
    Effect::new(move |_| match value.get() {
        Some(Ok(())) => reload_page(),
        Some(Err(error)) => toast.push(error.to_string(), ToastSeverity::Error),
        None => {}
    });
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
            <select prop:value=move || value.get() on:change=move |event| value.set(event_target_value(&event))>
                <option value="false">{tr(TextKey::No)}</option><option value="true">{tr(TextKey::Yes)}</option>
            </select>
        }.into_any(),
        AddonSettingKind::Enum => view! {
            <select prop:value=move || value.get() on:change=move |event| value.set(event_target_value(&event))>
                {definition.options.into_iter().map(|option| {
                    let label = option.clone();
                    view! { <option value=option>{label}</option> }
                }).collect_view()}
            </select>
        }.into_any(),
        AddonSettingKind::SecretReference => view! {
            <div class=style::setting_secret>
                <select prop:value=move || reference_kind.get() on:change=move |event| reference_kind.set(event_target_value(&event))>
                    <option value="environment">{tr(TextKey::EnvironmentVariable)}</option><option value="file">{tr(TextKey::SecretFile)}</option>
                </select>
                <input prop:value=move || reference.get() on:input=move |event| reference.set(event_target_value(&event))/>
            </div>
        }.into_any(),
        _ => view! { <input prop:value=move || value.get() on:input=move |event| value.set(event_target_value(&event))/> }.into_any(),
    };
    let is_secret = definition.kind == AddonSettingKind::SecretReference;
    view! {
        <div class=style::setting data-testid="addon-setting-row">
            <label>{label}<span class=style::muted>{key.clone()}</span></label>
            {input}
            <button type="button" class=style::btn_secondary on:click=move |_| {
                action.dispatch(AddonSettingEditor {
                    addon_id: addon_id.clone(), key: key.clone(),
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
    view! {
        <div class=style::permissions data-testid="addon-permissions">
            <h3>{tr(TextKey::Permissions)}</h3>
            <p>{format!("{}: {}", tr(TextKey::Capabilities), capabilities.join(", "))}</p>
            <p>{format!("{}: {}", tr(TextKey::Networks), value_or_no(&permissions.network_hosts))}</p>
            <p>{format!("{}: {}", tr(TextKey::MediaReadAccess), yes_no(permissions.media_read))}</p>
            <p>{format!("{}: {}", tr(TextKey::Ffmpeg), yes_no(permissions.managed_ffmpeg))}</p>
            <p>{format!("{}: {}", tr(TextKey::ExternalCommands), value_or_no(&permissions.external_commands))}</p>
        </div>
    }.into_any()
}

fn value_or_no(values: &[String]) -> String {
    if values.is_empty() {
        tr(TextKey::No).to_owned()
    } else {
        values.join(", ")
    }
}

fn yes_no(value: bool) -> &'static str {
    if value {
        tr(TextKey::Yes)
    } else {
        tr(TextKey::No)
    }
}

fn dependencies_view(dependencies: &[AddonDependency]) -> AnyView {
    if dependencies.is_empty() {
        return ().into_any();
    }
    view! {
        <div class=style::permissions>
            <h3>{tr(TextKey::Dependencies)}</h3>
            {dependencies.iter().map(|dependency| view! {
                <p>{format!("{} {}{}", dependency.id, dependency.version, if dependency.optional { format!(" ({})", tr(TextKey::Optional)) } else { String::new() })}</p>
            }).collect_view()}
        </div>
    }.into_any()
}

fn panel_view(panel: AddonPanel) -> AnyView {
    view! {
        <section class=style::detail_section data-testid="addon-declarative-panel">
            <h2>{localized(&panel.title)}</h2>
            {panel.blocks.into_iter().map(|block| match block {
                AddonPanelBlock::Text { text } => view! { <p>{localized(&text)}</p> }.into_any(),
                AddonPanelBlock::Status { label, code } => view! { <p><strong>{localized(&label)}</strong>{format!(": {code}")}</p> }.into_any(),
                AddonPanelBlock::KeyValue { label, value } => view! { <p><strong>{localized(&label)}</strong>{format!(": {value}")}</p> }.into_any(),
                AddonPanelBlock::Link { label, url } => view! { <p><a href=url target="_blank" rel="noopener noreferrer">{localized(&label)}</a></p> }.into_any(),
            }).collect_view()}
        </section>
    }.into_any()
}

#[cfg(target_arch = "wasm32")]
async fn upload_selected_file(
    input: web_sys::HtmlInputElement,
    endpoint: &str,
) -> Result<(), String> {
    use gloo_net::http::Request;
    use web_sys::FormData;
    let file = input
        .files()
        .and_then(|files| files.get(0))
        .ok_or_else(|| crate::admin::i18n::upload_error_text(use_i18n(), "No file field"))?;
    let form = FormData::new().map_err(|error| format!("{error:?}"))?;
    let blob: &web_sys::Blob = file.as_ref();
    form.append_with_blob_and_filename("file", blob, &file.name())
        .map_err(|error| format!("{error:?}"))?;
    let response = Request::post(endpoint)
        .body(form)
        .map_err(|error| error.to_string())?
        .send()
        .await
        .map_err(|error| error.to_string())?;
    let status = response.status();
    let json: serde_json::Value = response.json().await.map_err(|error| error.to_string())?;
    if !(200..300).contains(&status) {
        return Err(json
            .get("error")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(tr(TextKey::RequestFailed))
            .to_owned());
    }
    Ok(())
}

fn reload_page() {
    #[cfg(target_arch = "wasm32")]
    if let Some(window) = web_sys::window() {
        let _ = window.location().reload();
    }
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
