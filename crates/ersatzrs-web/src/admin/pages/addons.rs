#![allow(clippy::too_many_lines)]

use std::collections::{BTreeMap, BTreeSet};

use ersatzrs_addon_contract::{
    AddonDependency, AddonPanel, AddonPanelBlock, AddonPermissions, AddonSettingDefinition,
    AddonSettingKind, CAPABILITY_MEDIA_LIST_IMPORT_V1, CAPABILITY_MEDIA_LIST_LIST_V1,
    CAPABILITY_MEDIA_LIST_LIST_V2, CAPABILITY_MEDIA_LIST_LIST_V3, CAPABILITY_MEDIA_LIST_LIST_V4,
    CAPABILITY_MEDIA_LIST_LIST_V5,
};
use leptos::{html, prelude::*};
use leptos_router::hooks::{use_params_map, use_query_map};

use crate::admin::i18n::{provide_i18n_context, tr, use_i18n, TextKey};
#[cfg(feature = "ssr")]
use crate::admin::server_fns::addons::load_addon_detail;
use crate::admin::server_fns::addons::{
    check_addon_package, disable_addon_package, enable_addon_package, install_addon_package,
    load_addon_overview, remove_addon_repository, save_addon_configuration,
    save_addon_detail_wait_settings, scan_addon_repositories, set_default_remote_stream_addon,
    set_unverified_addon_sources, uninstall_addon_package, AddonDetail,
    AddonDetailWaitSettingsForm, AddonOverview, AddonRepositoryRow, AddonSettingEditor,
    AddonSettingRow, AvailableAddonRow, InstalledAddonRow,
};
use crate::admin::ui::primitives::dialog::{DeleteDialog, Modal};
use crate::admin::ui::primitives::resource_state::AdminErrorState;
use crate::admin::ui::primitives::settings_navigation::{
    SettingsPageLayout, SettingsPageLayoutForPath,
};
use crate::admin::ui::primitives::toast::{load_or_notify, use_toast, ToastSeverity, ToastStore};

stylance::import_crate_style!(style, "src/admin/pages/addons.module.css");
/// The media-source pages own the house style for a list of external sources
/// with row actions; the repository list is the same thing and reuses it.
mod source_style {
    #![allow(dead_code)]
    stylance::import_crate_style!(s, "src/admin/pages/media_sources/shared.module.css");
    pub use s::*;
}

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
    match load_addon_overview_initial() {
        Ok(initial_overview) => view! {
            <SettingsPageLayout>
                <AddonsSettingsIsland
                    initial_view
                    initial_search
                    initial_filter
                    initial_overview
                />
            </SettingsPageLayout>
        }
        .into_any(),
        Err(error) => view! {
            <SettingsPageLayout><AdminErrorState error/></SettingsPageLayout>
        }
        .into_any(),
    }
}

fn load_addon_overview_initial() -> Result<Option<AddonOverview>, String> {
    #[cfg(feature = "ssr")]
    {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(load_addon_overview())
                .map(Some)
                .map_err(|error| error.to_string())
        })
    }
    #[cfg(not(feature = "ssr"))]
    {
        Ok(None)
    }
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
            <SettingsPageLayout>
                <div class=style::not_found>
                    <h1>{tr(TextKey::AddonNotFound)}</h1>
                    <a href="/settings/addons">{tr(TextKey::BackToAddons)}</a>
                </div>
            </SettingsPageLayout>
        }
        .into_any(),
        Err(error) => view! {
            <SettingsPageLayout><AdminErrorState error/></SettingsPageLayout>
        }
        .into_any(),
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
    initial_overview: Option<AddonOverview>,
) -> impl IntoView {
    provide_i18n_context();
    let toast = use_toast();
    let overview: LocalResource<AddonOverview> =
        LocalResource::new(
            move || async move { load_or_notify(load_addon_overview().await, toast) },
        );
    let current = RwSignal::new(initial_overview);
    let detail_wait = DetailWaitEditor {
        minimum: RwSignal::new("2".to_owned()),
        maximum: RwSignal::new("16".to_owned()),
        save: Action::new(move |form: &AddonDetailWaitSettingsForm| {
            save_addon_detail_wait_settings(*form)
        }),
    };
    Effect::new(move |_| {
        if let Some(value) = overview.get() {
            detail_wait
                .minimum
                .set(value.detail_wait.minimum_seconds.to_string());
            detail_wait
                .maximum
                .set(value.detail_wait.maximum_seconds.to_string());
            current.set(Some(value));
        }
    });

    let pickers = FilePickers {
        addon: NodeRef::<html::Input>::new(),
        addon_name: RwSignal::new(String::new()),
        repository: NodeRef::<html::Input>::new(),
        repository_name: RwSignal::new(String::new()),
    };
    let unknown_sources_confirm_open = RwSignal::new(false);
    let repository_delete_open = RwSignal::new(false);
    let repository_delete_target = RwSignal::new(None::<(String, String)>);

    let scan_action = Action::new(move |(): &()| scan_addon_repositories());
    let remove_repository_action =
        Action::new(move |repository_id: &String| remove_addon_repository(repository_id.clone()));
    let unknown_sources_action =
        Action::new(move |enabled: &bool| set_unverified_addon_sources(*enabled));
    let default_remote_stream_action =
        Action::new(move |addon_id: &String| set_default_remote_stream_addon(addon_id.clone()));
    Effect::new(move |_| action_result(scan_action.value().get(), overview, toast));
    Effect::new(move |_| action_result(unknown_sources_action.value().get(), overview, toast));
    Effect::new(move |_| {
        action_result(remove_repository_action.value().get(), overview, toast);
    });
    Effect::new(move |_| {
        action_result(default_remote_stream_action.value().get(), overview, toast);
    });
    Effect::new(move |_| match detail_wait.save.value().get() {
        Some(Ok(())) => {
            toast.push(tr(TextKey::Saved).to_owned(), ToastSeverity::Success);
            overview.refetch();
        }
        Some(Err(error)) => toast.push(error.to_string(), ToastSeverity::Error),
        None => {}
    });

    let view_name = initial_view.clone();
    let search_value = initial_search.clone();
    let filter_value = initial_filter.clone();
    let upload_addon = move |_| {
        #[cfg(target_arch = "wasm32")]
        if let Some(input) = pickers.addon.get() {
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
        if let Some(input) = pickers.repository.get() {
            leptos::task::spawn_local(async move {
                match upload_selected_file(input, "/api/addons/repository-bundle").await {
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
                    on:click=move |_| { scan_action.dispatch(()); }
                >{tr(TextKey::Refresh)}</button>
            </header>

            {move || current.get().map(|data| manager_body(
                data,
                &view_name,
                &search_value,
                &filter_value,
                pickers,
                detail_wait,
                toast,
                unknown_sources_confirm_open,
                unknown_sources_action,
                default_remote_stream_action,
                repository_delete_open,
                repository_delete_target,
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

            <DeleteDialog
                open=repository_delete_open
                entity_name=Signal::derive(move || {
                    repository_delete_target
                        .get()
                        .map_or_else(String::new, |(_, name)| name)
                })
                on_confirm=Callback::new(move |confirmed| {
                    if confirmed {
                        if let Some((repository_id, _)) = repository_delete_target.get_untracked() {
                            remove_repository_action.dispatch(repository_id);
                        }
                    }
                    repository_delete_target.set(None);
                })
            />
        </main>
    }
}

#[derive(Clone, Copy)]
struct DetailWaitEditor {
    minimum: RwSignal<String>,
    maximum: RwSignal<String>,
    save: Action<AddonDetailWaitSettingsForm, Result<(), ServerFnError>>,
}

/// The two local-package pickers in the Sources view. The names are kept
/// beside the inputs so the page can render the chosen file itself instead of
/// leaving the browser's own, unlocalized file-input chrome on screen.
#[derive(Clone, Copy)]
struct FilePickers {
    addon: NodeRef<html::Input>,
    addon_name: RwSignal<String>,
    repository: NodeRef<html::Input>,
    repository_name: RwSignal<String>,
}

#[allow(clippy::too_many_arguments)]
fn manager_body(
    data: AddonOverview,
    view_name: &str,
    search_value: &str,
    filter_value: &str,
    pickers: FilePickers,
    detail_wait: DetailWaitEditor,
    toast: ToastStore,
    unknown_sources_confirm_open: RwSignal<bool>,
    unknown_sources_action: Action<bool, Result<(), ServerFnError>>,
    default_remote_stream_action: Action<String, Result<(), ServerFnError>>,
    repository_delete_open: RwSignal<bool>,
    repository_delete_target: RwSignal<Option<(String, String)>>,
    upload_addon: impl Fn(leptos::ev::MouseEvent) + Copy + Send + 'static,
    upload_repository: impl Fn(leptos::ev::MouseEvent) + Copy + Send + 'static,
) -> AnyView {
    let installed_count = data.installed.len();
    let enabled_count = data.installed.iter().filter(|item| item.enabled).count();
    let installed_versions = data
        .installed
        .iter()
        .map(|item| {
            (
                item.addon_id.clone(),
                (item.version.clone(), item.trust_tier.clone()),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let update_ids = data
        .available
        .iter()
        .filter(|offer| {
            installed_versions
                .get(&offer.addon.id)
                .and_then(|(version, trust)| {
                    semver::Version::parse(version)
                        .ok()
                        .map(|version| (version, trust))
                })
                .is_some_and(|(version, trust)| {
                    offer.addon.version > version
                        || (trust == "legacy" && offer.addon.version == version)
                })
        })
        .map(|offer| offer.addon.id.clone())
        .collect::<BTreeSet<_>>();
    let source_count = data.repositories.len();
    let checkout_path = data.checkout_path.clone();
    let has_repositories = !data.repositories.is_empty();
    let offers_per_repository =
        data.available
            .iter()
            .fold(BTreeMap::<String, usize>::new(), |mut counts, offer| {
                *counts.entry(offer.repository_id.clone()).or_default() += 1;
                counts
            });
    let query = search_value.trim().to_ascii_lowercase();
    let repository_tiers = data
        .repositories
        .iter()
        .map(|repository| (repository.id.clone(), repository.kind.clone()))
        .collect::<BTreeMap<_, _>>();
    let installed_cards = data
        .installed
        .iter()
        .filter(|item| addon_matches(&localized(&item.manifest.name), &item.addon_id, &query))
        .filter(|item| match filter_value {
            "enabled" => item.enabled,
            "disabled" => !item.enabled,
            "local" | "sideloaded" | "legacy" => item.trust_tier == filter_value,
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
    let unknown_enabled = data.unverified_sources_enabled;
    // Only an enabled package that can actually play a remote stream may be the
    // fallback, mirroring the check the save path enforces server-side.
    let remote_stream_addons = data
        .installed
        .iter()
        .filter(|item| item.enabled && item.manifest.has_remote_stream_play_capability())
        .map(|item| (item.addon_id.clone(), localized(&item.manifest.name)))
        .collect::<Vec<_>>();
    let default_remote_stream_addon = data.default_remote_stream_addon.clone().unwrap_or_default();

    view! {
        <nav class=style::tabs aria-label=tr(TextKey::Addons)>
            {manager_tab("installed", tr(TextKey::InstalledAddons), Some(format!("{enabled_count}/{installed_count}")), view_name)}
            {manager_tab("browse", tr(TextKey::BrowseAddons), None, view_name)}
            {manager_tab("updates", tr(TextKey::AddonUpdates), Some(update_ids.len().to_string()), view_name)}
            {manager_tab("sources", tr(TextKey::AddonSources), Some(source_count.to_string()), view_name)}
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
            <form
                class=style::section
                data-testid="addon-detail-wait-settings"
                on:submit=move |event| {
                    event.prevent_default();
                    let minimum = detail_wait.minimum.get_untracked().trim().parse::<i32>();
                    let maximum = detail_wait.maximum.get_untracked().trim().parse::<i32>();
                    match (minimum, maximum) {
                        (Ok(minimum_seconds), Ok(maximum_seconds))
                            if minimum_seconds > 0
                                && maximum_seconds > 0
                                && minimum_seconds <= maximum_seconds =>
                        {
                            detail_wait.save.dispatch(AddonDetailWaitSettingsForm {
                                minimum_seconds,
                                maximum_seconds,
                            });
                        }
                        _ => toast.push(
                            tr(TextKey::AddonDetailWaitInvalid).to_owned(),
                            ToastSeverity::Error,
                        ),
                    }
                }
            >
                <div class=style::section_header>
                    <div>
                        <h2>{tr(TextKey::AddonDetailWait)}</h2>
                        <p id="addon-detail-wait-help" class=style::section_help>
                            {tr(TextKey::AddonDetailWaitHelp)}
                        </p>
                    </div>
                    <button
                        type="submit"
                        class=style::btn_primary
                        disabled=move || detail_wait.save.pending().get()
                    >{tr(TextKey::Save)}</button>
                </div>
                <div class=style::section_divider></div>
                <div class=style::detail_wait_fields>
                    <label for="addon-detail-wait-minimum">
                        {format!("{} ({})", tr(TextKey::MinimumDetailWait), tr(TextKey::Seconds))}
                        <input
                            id="addon-detail-wait-minimum"
                            data-testid="addon-detail-wait-minimum"
                            type="number"
                            min="1"
                            max="2147483647"
                            step="1"
                            required
                            aria-describedby="addon-detail-wait-help"
                            prop:value=move || detail_wait.minimum.get()
                            on:input=move |event| detail_wait.minimum.set(event_target_value(&event))
                        />
                    </label>
                    <label for="addon-detail-wait-maximum">
                        {format!("{} ({})", tr(TextKey::MaximumDetailWait), tr(TextKey::Seconds))}
                        <input
                            id="addon-detail-wait-maximum"
                            data-testid="addon-detail-wait-maximum"
                            type="number"
                            min="1"
                            max="2147483647"
                            step="1"
                            required
                            aria-describedby="addon-detail-wait-help"
                            prop:value=move || detail_wait.maximum.get()
                            on:input=move |event| detail_wait.maximum.set(event_target_value(&event))
                        />
                    </label>
                </div>
            </form>
            // A routing preference over installed add-ons, so it lives with
            // the add-ons it chooses between rather than with source trust.
            <div class=style::section data-testid="default-remote-stream-addon">
                <div class=style::section_header>
                    <h2>{tr(TextKey::DefaultRemoteStreamAddon)}</h2>
                    <select
                        class=style::select
                        aria-label=tr(TextKey::DefaultRemoteStreamAddon)
                        on:change=move |event| {
                            default_remote_stream_action.dispatch(event_target_value(&event));
                        }
                    >
                        <option value="" selected=default_remote_stream_addon.is_empty()>{tr(TextKey::NoneOption)}</option>
                        {remote_stream_addons.into_iter().map(|(addon_id, name)| {
                            let selected = addon_id == default_remote_stream_addon;
                            view! { <option value=addon_id selected=selected>{name}</option> }
                        }).collect_view()}
                    </select>
                </div>
                <div class=style::section_divider></div>
                <p class=style::section_help>{tr(TextKey::DefaultRemoteStreamAddonHelp)}</p>
            </div>
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
            }).collect(), updates_empty_message(search_value, filter_value))}
        </section>

        <section class=style::sources style:display=if view_name == "sources" { "" } else { "none" }>
            <div class=style::section>
                <div class=style::section_header>
                    <h2>{tr(TextKey::UnknownSources)}</h2>
                    <div class=style::section_actions>
                        <span class=format!(
                            "{} {}",
                            style::badge,
                            if unknown_enabled { style::badge_warn } else { style::badge_neutral },
                        )>
                            {if unknown_enabled { tr(TextKey::Enabled) } else { tr(TextKey::Disabled) }}
                        </span>
                        // Danger marks the risky click, not the safe one:
                        // turning unknown sources on is what deserves a second
                        // thought, and turning them off never does.
                        <button
                            type="button"
                            class=if unknown_enabled { style::btn_secondary } else { style::btn_danger }
                            on:click=move |_| {
                                if unknown_enabled {
                                    unknown_sources_action.dispatch(false);
                                } else {
                                    unknown_sources_confirm_open.set(true);
                                }
                            }
                        >{if unknown_enabled { tr(TextKey::Disable) } else { tr(TextKey::Enable) }}</button>
                    </div>
                </div>
                <div class=style::section_divider></div>
                <p class=style::section_help>{tr(TextKey::UnknownSourcesHelp)}</p>
            </div>

            <div class=style::section>
                <div class=style::section_header>
                    <h2 class=style::section_heading>{tr(TextKey::AddonRepositories)}</h2>
                    <code class=style::checkout_path>{checkout_path}</code>
                </div>
                <div class=style::section_divider></div>
                <p class=style::empty style:display=if has_repositories { "none" } else { "" }>{tr(TextKey::NoRepositories)}</p>
                <div class=source_style::path_table_wrap style:display=if has_repositories { "" } else { "none" }>
                    <table class=style::repo_table>
                        <thead>
                            <tr>
                                <th>{tr(TextKey::Name)}</th>
                                <th>{tr(TextKey::Addons)}</th>
                                <th>{tr(TextKey::LastRefresh)}</th>
                                <th>{tr(TextKey::Status)}</th>
                                <th/>
                            </tr>
                        </thead>
                        <tbody>
                            {data.repositories.into_iter().map(|repository| {
                                repository_row(
                                    repository,
                                    &offers_per_repository,
                                    repository_delete_open,
                                    repository_delete_target,
                                )
                            }).collect_view()}
                        </tbody>
                    </table>
                </div>
            </div>

            <div class=style::section>
                <div class=style::section_header>
                    <h2 class=style::section_heading>{tr(TextKey::InstallFromFile)}</h2>
                </div>
                <div class=style::section_divider></div>
                <p class=style::section_help style:display=if unknown_enabled { "none" } else { "" }>{tr(TextKey::UnknownSourcesRequired)}</p>
                <div class=style::file_row>
                    <span class=style::file_row_label>{tr(TextKey::AddonPackage)}</span>
                    <label class=style::file_button aria-disabled=!unknown_enabled title=(!unknown_enabled).then(|| tr(TextKey::UnknownSourcesRequired))>
                        {tr(TextKey::ChooseFile)}
                        <input
                            node_ref=pickers.addon
                            class=style::file_input
                            type="file"
                            accept=".zip,application/zip"
                            disabled=!unknown_enabled
                            on:change=move |event| pickers.addon_name.set(chosen_file_name(&event_target_value(&event)))
                        />
                    </label>
                    <span class=style::file_name>
                        {move || {
                            let name = pickers.addon_name.get();
                            if name.is_empty() { tr(TextKey::NoFileSelected).to_owned() } else { name }
                        }}
                    </span>
                    <button type="button" class=style::btn_secondary disabled=!unknown_enabled on:click=upload_addon>{tr(TextKey::Install)}</button>
                </div>
                <div class=style::file_row>
                    <span class=style::file_row_label>{tr(TextKey::RepositoryPackage)}</span>
                    <label
                        class=style::file_button
                        aria-disabled=!unknown_enabled
                        title=(!unknown_enabled).then(|| tr(TextKey::UnknownSourcesRequired))
                    >
                        {tr(TextKey::ChooseFile)}
                        <input
                            node_ref=pickers.repository
                            class=style::file_input
                            type="file"
                            accept=".zip,application/zip"
                            disabled=!unknown_enabled
                            on:change=move |event| pickers.repository_name.set(chosen_file_name(&event_target_value(&event)))
                        />
                    </label>
                    <span class=style::file_name>
                        {move || {
                            let name = pickers.repository_name.get();
                            if name.is_empty() { tr(TextKey::NoFileSelected).to_owned() } else { name }
                        }}
                    </span>
                    <button type="button" class=style::btn_secondary disabled=!unknown_enabled on:click=upload_repository>{tr(TextKey::Install)}</button>
                </div>
            </div>
        </section>
    }
    .into_any()
}

/// One local source row: its identity, on-disk location, last scan, and error.
#[allow(clippy::needless_pass_by_value)]
fn repository_row(
    repository: AddonRepositoryRow,
    offers: &BTreeMap<String, usize>,
    delete_open: RwSignal<bool>,
    delete_target: RwSignal<Option<(String, String)>>,
) -> AnyView {
    let offered = offers.get(&repository.id).copied().unwrap_or_default();
    let last_error = repository.last_error.clone();
    let removable = repository.kind == "bundle";
    let repository_id = repository.id.clone();
    let repository_name = repository.name.clone();
    view! {
        <tr data-testid="addon-repository-row">
            <td>
                <div class=style::repo_name>
                    <div class=style::card_title_row>
                        <strong>{repository.name}</strong>
                        {status_badge(&repository.kind)}
                    </div>
                    <span class=style::muted>{repository.id}</span>
                    <span class=style::muted>{repository.root_path}</span>
                </div>
            </td>
            <td>{offered}</td>
            <td class=source_style::cell_muted>{refresh_timestamp(Some(&repository.last_scan))}</td>
            <td>
                <div class=style::repo_status>
                    {status_badge(if repository.last_error.is_some() { "warning" } else { "ready" })}
                    {last_error.map(|error| view! { <span class=style::error>{error}</span> })}
                </div>
            </td>
            <td>
                {removable.then(|| view! {
                    <form method="post" action="/api/addons/repository-bundle/remove">
                        <input type="hidden" name="repository_id" value=repository_id.clone()/>
                        <button
                            type="submit"
                            class=source_style::row_btn_del
                            data-testid="remove-addon-repository"
                            on:click=move |event| {
                                event.prevent_default();
                                delete_target.set(Some((repository_id.clone(), repository_name.clone())));
                                delete_open.set(true);
                            }
                        >{tr(TextKey::Delete)}</button>
                    </form>
                })}
            </td>
        </tr>
    }
    .into_any()
}

/// Render a stored RFC 3339 scan time as a plain local-looking stamp.
fn refresh_timestamp(value: Option<&str>) -> String {
    value
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .map_or_else(
            || tr(TextKey::NeverRefreshed).to_owned(),
            |stamp| {
                stamp
                    .with_timezone(&chrono::Local)
                    .format("%Y-%m-%d %H:%M")
                    .to_string()
            },
        )
}

/// The file name out of an `<input type="file">` value, which browsers report
/// as a fake path (`C:\fakepath\package.zip`).
fn chosen_file_name(value: &str) -> String {
    value
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .to_owned()
}

/// One manager tab, optionally carrying the count of what it holds.
///
/// The counts used to sit in a four-card summary band above the tabs, which
/// spent a quarter of the viewport restating what the tab strip already names.
/// `Installed add-ons` carries both of its numbers as `enabled/installed`;
/// `Browse add-ons` carries none, because an offer total says nothing about
/// this server.
fn manager_tab(
    view: &'static str,
    label: &'static str,
    count: Option<String>,
    selected: &str,
) -> AnyView {
    let href = format!("/settings/addons?view={view}");
    let label = match count {
        Some(count) => format!("{label} ({count})"),
        None => label.to_owned(),
    };
    view! { <a href=href class=if selected == view { style::tab_active } else { style::tab }>{label}</a> }.into_any()
}

fn filter_options(view_name: &str, selected: &str) -> AnyView {
    if view_name == "installed" {
        view! {
            <option value="enabled" selected=selected == "enabled">{tr(TextKey::Enabled)}</option>
            <option value="disabled" selected=selected == "disabled">{tr(TextKey::Disabled)}</option>
            <option value="local" selected=selected == "local">{tr(TextKey::Local)}</option>
            <option value="sideloaded" selected=selected == "sideloaded">{tr(TextKey::Sideloaded)}</option>
            <option value="legacy" selected=selected == "legacy">{tr(TextKey::LegacyUnmanaged)}</option>
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

/// Empty-state text for the Updates view.
///
/// An empty Updates list means two different things. With a search or state
/// filter applied it means the filter excluded everything, which is what
/// `NoAddonsMatch` says. Unfiltered it means there is genuinely nothing to
/// update — reporting that as a filter miss reads like the view is broken, so
/// say so plainly instead. The other views have no such second meaning: an
/// unfiltered empty Installed or Browse list is not good news to report.
fn updates_empty_message(search_value: &str, filter_value: &str) -> &'static str {
    if search_value.trim().is_empty() && filter_value == "all" {
        tr(TextKey::AddonsUpToDate)
    } else {
        tr(TextKey::NoAddonsMatch)
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
        "local" => tr(TextKey::Local).to_owned(),
        "checkout" => tr(TextKey::GitCheckout).to_owned(),
        "bundle" => tr(TextKey::OfflineBundle).to_owned(),
        "legacy" => tr(TextKey::LegacyUnmanaged).to_owned(),
        "ready" => tr(TextKey::Available).to_owned(),
        "warning" => tr(TextKey::Warning).to_owned(),
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
    let adopt_legacy = installed
        .as_ref()
        .is_some_and(|item| item.trust_tier == "legacy");
    let install_target = selected_offer.as_ref().map(|offer| {
        (
            offer.repository_id.clone(),
            offer.addon.id.clone(),
            adopt_legacy,
        )
    });

    let install_action = Action::new(move |target: &(String, String, bool)| {
        install_addon_package(target.0.clone(), target.1.clone(), target.2)
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
        _compatible,
        permissions,
        dependencies,
        license,
        source_url,
    ) = detail_identity(&detail, selected_offer.as_ref());
    let is_installed = installed.is_some();
    let is_enabled = installed.as_ref().is_some_and(|item| item.enabled);
    let update_available = match (&installed, &selected_offer) {
        (Some(installed), Some(offer)) => {
            semver::Version::parse(&installed.version).is_ok_and(|version| {
                offer.addon.version > version || (adopt_legacy && offer.addon.version == version)
            })
        }
        (None, Some(_)) => true,
        _ => false,
    };
    let offer_installable = selected_offer
        .as_ref()
        .is_some_and(|offer| offer.host_compatible && offer.package_available);
    let can_install = offer_installable && install_target.is_some() && update_available;
    let can_import_list = is_enabled && supports_media_list_import(&permissions.0);
    let import_list_href = format!("/media/addon-lists/add?addon_id={addon_id}");
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
    let modal_name = selected_offer
        .as_ref()
        .map_or_else(|| name.clone(), |offer| localized(&offer.addon.name));
    let modal_version = selected_offer
        .as_ref()
        .map_or_else(|| version.clone(), |offer| offer.addon.version.to_string());
    let modal_source = selected_offer
        .as_ref()
        .map_or_else(|| source_id.clone(), |offer| offer.repository_id.clone());
    let modal_trust = detail
        .repositories
        .iter()
        .find(|repository| repository.id == modal_source)
        .map_or_else(|| trust.clone(), |repository| repository.kind.clone());
    let install_permissions = permissions.clone();
    let toggle_permissions = permissions.clone();

    view! {
        <SettingsPageLayoutForPath current_path="/settings/addons">
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
                    <a
                        class=style::btn_primary
                        href=import_list_href
                        style:display=if can_import_list { "inline-flex" } else { "none" }
                    >{tr(TextKey::ImportAList)}</a>
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

            <Modal open=install_confirm title=format!("{} · {} {}", tr(TextKey::Install), modal_name, modal_version) on_close=Callback::new(move |confirmed| {
                if confirmed {
                    if let Some(target) = install_target.clone() { install_action.dispatch(target); }
                }
            })>
                <p>{format!("{}: {} · {}: {}", tr(TextKey::Source), modal_source, tr(TextKey::Status), modal_trust)}</p>
                <p style:display=if adopt_legacy { "block" } else { "none" }>{tr(TextKey::LegacyAdoptionHelp)}</p>
                <p>{tr(TextKey::InstallLeavesDisabled)}</p>
                <p>{tr(TextKey::ThirdPartyRepositoryWarning)}</p>
                {permissions_view(&install_permissions.0, &install_permissions.1)}
            </Modal>
            <Modal open=toggle_confirm title=if is_enabled { tr(TextKey::Disable).to_owned() } else { tr(TextKey::Enable).to_owned() } on_close=Callback::new(move |confirmed| {
                if confirmed {
                    if is_enabled { disable_action.dispatch(addon_id_for_toggle.clone()); } else { enable_action.dispatch(addon_id_for_toggle.clone()); }
                }
            })>
                <p>{summary_for_modal.clone()}</p>
                <p style:display=if is_enabled { "none" } else { "block" }>{tr(TextKey::EnableActivatesPermissions)}</p>
                {permissions_view(&toggle_permissions.0, &toggle_permissions.1)}
            </Modal>
            <Modal open=uninstall_confirm title=tr(TextKey::Uninstall).to_owned() destructive=true on_close=Callback::new(move |confirmed| {
                if confirmed { uninstall_action.dispatch(addon_id_for_uninstall.clone()); }
            })><p>{tr(TextKey::ThisActionCannotBeUndone)}</p></Modal>
            </main>
        </SettingsPageLayoutForPath>
    }
}

fn supports_media_list_import(capabilities: &[String]) -> bool {
    capabilities.iter().any(|capability| {
        matches!(
            capability.as_str(),
            CAPABILITY_MEDIA_LIST_IMPORT_V1
                | CAPABILITY_MEDIA_LIST_LIST_V1
                | CAPABILITY_MEDIA_LIST_LIST_V2
                | CAPABILITY_MEDIA_LIST_LIST_V3
                | CAPABILITY_MEDIA_LIST_LIST_V4
                | CAPABILITY_MEDIA_LIST_LIST_V5
        )
    })
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
        .map_or_else(|| "local".to_owned(), |repository| repository.kind.clone());
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
    let suggested = definition.suggested_reference.as_ref();
    let reference_kind = RwSignal::new(
        stored
            .as_ref()
            .and_then(|item| item.reference_kind.clone())
            .or_else(|| {
                suggested.map(|item| match item.kind {
                    ersatzrs_addon_contract::AddonSuggestedSecretReferenceKind::Environment => {
                        "environment".to_owned()
                    }
                    ersatzrs_addon_contract::AddonSuggestedSecretReferenceKind::File => {
                        "file".to_owned()
                    }
                })
            })
            .unwrap_or_else(|| "environment".to_owned()),
    );
    let reference = RwSignal::new(
        stored
            .and_then(|item| item.reference)
            .or_else(|| suggested.map(|item| item.reference.clone()))
            .unwrap_or_default(),
    );
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
    use super::{localized_for_code, supports_media_list_import, updates_empty_message};
    use crate::admin::i18n::{tr, TextKey};
    use ersatzrs_addon_contract::{
        CAPABILITY_MEDIA_LIST_IMPORT_V1, CAPABILITY_MEDIA_LIST_LIST_V1,
        CAPABILITY_MEDIA_LIST_LIST_V5,
    };
    use std::collections::BTreeMap;

    #[test]
    fn updates_empty_state_only_claims_up_to_date_when_nothing_is_filtered() {
        assert_eq!(
            updates_empty_message("", "all"),
            tr(TextKey::AddonsUpToDate),
            "an unfiltered empty Updates list means there is nothing to update"
        );
        assert_eq!(
            updates_empty_message("   ", "all"),
            tr(TextKey::AddonsUpToDate)
        );

        // A search or a state filter can hide a pending update, so the list
        // being empty no longer proves anything is up to date.
        assert_eq!(
            updates_empty_message("yt-dlp", "all"),
            tr(TextKey::NoAddonsMatch)
        );
        assert_eq!(
            updates_empty_message("", "verified"),
            tr(TextKey::NoAddonsMatch)
        );
        assert_eq!(
            updates_empty_message("yt-dlp", "verified"),
            tr(TextKey::NoAddonsMatch)
        );
    }

    #[test]
    fn localized_manifest_text_matches_bcp47_codes_case_insensitively() {
        let values = BTreeMap::from([
            ("de-DE".to_owned(), "Deutsch".to_owned()),
            ("en-US".to_owned(), "English".to_owned()),
        ]);
        assert_eq!(localized_for_code(&values, "de-de"), "Deutsch");
        assert_eq!(localized_for_code(&values, "unknown"), "English");
    }

    #[test]
    fn list_import_entry_point_accepts_every_supported_contract_generation() {
        for capability in [
            CAPABILITY_MEDIA_LIST_LIST_V1,
            CAPABILITY_MEDIA_LIST_LIST_V5,
            CAPABILITY_MEDIA_LIST_IMPORT_V1,
        ] {
            assert!(supports_media_list_import(&[capability.to_owned()]));
        }
        assert!(!supports_media_list_import(&[
            "remote-stream.play.v1".to_owned()
        ]));
    }
}
