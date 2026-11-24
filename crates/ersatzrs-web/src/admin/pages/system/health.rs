use leptos::prelude::*;

use crate::admin::i18n::{
    health_check_message, health_check_title, health_summary, tr, use_i18n, TextKey, UiLanguage,
};
use crate::admin::release_notes::ReleaseNotesCard;
#[cfg(target_arch = "wasm32")]
use crate::admin::server_fns::layout::media_state_revision;
use crate::admin::server_fns::layout::{
    load_health_snapshot, AdminFfmpegRuntimeStatus, AdminHealthRow, AdminHealthSnapshot,
    RefreshHealthSnapshot,
};
use crate::admin::ui::primitives::button::{btn_class, ButtonVariant};
use crate::admin::ui::primitives::resource_state::admin_result_view;
use crate::admin::ui::primitives::system_navigation::SystemPageLayout;
use crate::admin::ui::primitives::toast::use_toast;
use crate::admin::ui::primitives::toast::ToastSeverity;

stylance::import_crate_style!(style, "src/admin/pages/system/health.module.css");

#[component]
pub fn HealthPage() -> impl IntoView {
    // Resolve on the server before emitting the island. A streamed Suspense
    // fallback inside the island can otherwise race its hydration markers.
    let content = Suspend::new(async move {
        admin_result_view(load_health_snapshot().await, |initial_snapshot| {
            view! { <HealthIsland initial_snapshot/> }.into_any()
        })
    });
    view! {
        <SystemPageLayout>
            <div class=style::page>
                {content}
                <ReleaseNotesCard/>
            </div>
        </SystemPageLayout>
    }
}

#[island]
fn HealthIsland(initial_snapshot: AdminHealthSnapshot) -> impl IntoView {
    // Hydrated islands must restore the page language before rendering updates.
    crate::admin::i18n::provide_i18n_context();
    let snapshot = RwSignal::new(initial_snapshot);
    let auto_refresh = RwSignal::new(true);
    let interactive = RwSignal::new(false);
    Effect::new(move |_| interactive.set(true));
    let refresh = ServerAction::<RefreshHealthSnapshot>::new();
    let load_toast = use_toast();
    Effect::new(move |_| {
        if let Some(result) = refresh.value().get() {
            match result {
                Ok(next) => snapshot.set(next),
                Err(error) => load_toast.push(
                    format!("{}: {error}", tr(TextKey::ErrorOccurred)),
                    ToastSeverity::Error,
                ),
            }
        }
    });

    #[cfg(target_arch = "wasm32")]
    {
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        };
        let running = Arc::new(AtomicBool::new(true));
        let cleanup_running = running.clone();
        on_cleanup(move || cleanup_running.store(false, Ordering::Relaxed));
        Effect::new(move |_| {
            let running = running.clone();
            leptos::task::spawn_local(async move {
                let mut previous_revision = None;
                let mut revision_load_failed = false;
                while running.load(Ordering::Relaxed) {
                    if !auto_refresh.get_untracked() {
                        gloo_timers::future::TimeoutFuture::new(1_500).await;
                        continue;
                    }
                    let result = media_state_revision().await;
                    if !running.load(Ordering::Relaxed) {
                        break;
                    }
                    if !auto_refresh.get_untracked() {
                        continue;
                    }
                    match result {
                        Ok(next) => {
                            // The first poll establishes a baseline; hydration
                            // must not immediately bypass the SSR result cache.
                            if previous_revision
                                .as_ref()
                                .is_some_and(|previous| previous != &next)
                            {
                                refresh.dispatch(RefreshHealthSnapshot {});
                            }
                            previous_revision = Some(next);
                            revision_load_failed = false;
                        }
                        Err(error) if !revision_load_failed => {
                            revision_load_failed = true;
                            load_toast.push(
                                format!("{}: {error}", tr(TextKey::ErrorOccurred)),
                                ToastSeverity::Error,
                            );
                        }
                        Err(_) => {}
                    }
                    gloo_timers::future::TimeoutFuture::new(1_500).await;
                }
            });
        });
    }

    view! {
        <div class=style::report>
            <header>
                <h1>
                    {tr(TextKey::HealthChecks)}
                </h1>
                <form method="post" action="/admin-api/system/health/refresh"
                    on:submit=move |event| {
                        event.prevent_default();
                        refresh.dispatch(RefreshHealthSnapshot {});
                    }
                >
                    <button type="submit" class=btn_class(ButtonVariant::Secondary)
                        disabled=move || refresh.pending().get()
                        aria-busy=move || refresh.pending().get().to_string()
                    >{tr(TextKey::Refresh)}</button>
                </form>
            </header>
            <p>
                {tr(TextKey::HealthPersistedRunHelp)}
            </p>
            <label class=style::auto_refresh hidden=move || !interactive.get()>
                <input type="checkbox" checked on:change=move |event| auto_refresh.set(event_target_checked(&event))/>
                {tr(TextKey::HealthAutoRefresh)}
            </label>
            <p role="status">{move || snapshot.with(|report| {
                if report.rows.is_empty() {
                    tr(TextKey::HealthNoPersistedRun).to_owned()
                } else {
                    health_summary(report.errors, report.warnings)
                }
            })}</p>
            {move || render_snapshot(snapshot.get())}
        </div>
    }
}

fn render_snapshot(mut snapshot: AdminHealthSnapshot) -> AnyView {
    if snapshot.rows.is_empty() {
        return ().into_any();
    }

    let runtime = snapshot.ffmpeg_runtime.take();
    let rows = std::mem::take(&mut snapshot.rows);
    view! {
        {runtime.map(render_ffmpeg_runtime)}
        <table class=style::checks>
            <caption>{tr(TextKey::HealthCheckResults)}</caption>
            <thead>
                <tr>
                    <th scope="col">{tr(TextKey::Check)}</th>
                    <th scope="col">{tr(TextKey::Status)}</th>
                    <th scope="col">{tr(TextKey::Message)}</th>
                </tr>
            </thead>
            <tbody>
                {rows.into_iter().map(render_row).collect_view()}
            </tbody>
        </table>
    }
    .into_any()
}

fn render_ffmpeg_runtime(runtime: AdminFfmpegRuntimeStatus) -> AnyView {
    let source = runtime_source_label(&runtime.source);
    let installed = runtime.installed_tag.unwrap_or_else(|| "-".to_owned());
    let latest = runtime.latest_tag.unwrap_or_else(|| "-".to_owned());
    let update = if let Some(error) = runtime.update_check_error {
        format!("{}: {error}", tr(TextKey::FfmpegRuntimeUpdateFailed))
    } else if runtime.downloaded_update {
        match runtime.previous_tag.as_deref() {
            Some(previous) => format!(
                "{}: {previous} -> {installed}",
                tr(TextKey::FfmpegRuntimeUpdated)
            ),
            None => tr(TextKey::FfmpegRuntimeUpdated).to_owned(),
        }
    } else if runtime.source == "downloaded" {
        format!("{}: {installed}", tr(TextKey::FfmpegRuntimeDownloaded))
    } else if runtime.update_available {
        tr(TextKey::FfmpegRuntimeUpdateAvailable).to_owned()
    } else {
        tr(TextKey::FfmpegRuntimeUpToDate).to_owned()
    };
    view! {
        <section class=style::runtime>
            <h2>
                {tr(TextKey::FfmpegRuntime)}
            </h2>
            <dl>
                <dt>{tr(TextKey::FfmpegRuntimeSource)}</dt>
                <dd>{source}</dd>
                <dt>{tr(TextKey::FfmpegRuntimeVersion)}</dt>
                <dd>
                    {format!("ffmpeg {}, ffprobe {}", runtime.ffmpeg_version, runtime.ffprobe_version)}
                </dd>
                <dt>{tr(TextKey::FfmpegRuntimeRelease)}</dt>
                <dd>{installed}</dd>
                <dt>{tr(TextKey::FfmpegRuntimeLatestRelease)}</dt>
                <dd>{latest}</dd>
                <dt>{tr(TextKey::Message)}</dt>
                <dd>{update}</dd>
            </dl>
        </section>
    }
    .into_any()
}

fn runtime_source_label(source: &str) -> &'static str {
    match source {
        "override" => tr(TextKey::FfmpegRuntimeSourceOverride),
        "managed" => tr(TextKey::FfmpegRuntimeSourceManaged),
        "PATH" => tr(TextKey::FfmpegRuntimeSourcePath),
        "downloaded" => tr(TextKey::FfmpegRuntimeSourceDownloaded),
        _ => tr(TextKey::FfmpegRuntimeSourceUnknown),
    }
}

fn render_row(row: AdminHealthRow) -> AnyView {
    let AdminHealthRow {
        title: persisted_title,
        status: persisted_status,
        message: persisted_message,
        brief_message,
        link,
    } = row;
    let status = status_label(persisted_status.as_str());
    let title = health_check_title(&persisted_title);
    let message = health_check_message(&persisted_title, &persisted_message, &brief_message);

    // Persisted check names and diagnostic fallbacks originate in English.
    let localized = use_i18n() != UiLanguage::English;
    let title_lang = (localized && title == persisted_title).then_some("en");
    let message_lang = (localized
        && !message.is_empty()
        && (message == persisted_message || message == brief_message))
        .then_some("en");
    view! {
        <tr data-status=persisted_status>
            <th scope="row" lang=title_lang>{title}</th>
            <td>{status}</td>
            <td lang=message_lang>
                {match link {
                    Some(href) => view! {
                        <a href=href.clone() target=link_target(&href) rel="noopener">
                            {message}
                        </a>
                    }.into_any(),
                    None => message.into_any(),
                }}
            </td>
        </tr>
    }
    .into_any()
}

fn status_label(status: &str) -> &'static str {
    match status {
        "Fail" => tr(TextKey::Fail),
        "Warning" => tr(TextKey::Warning),
        "Info" => tr(TextKey::Info),
        "Pass" => tr(TextKey::Pass),
        _ => tr(TextKey::Unavailable),
    }
}

fn link_target(href: &str) -> &'static str {
    if href.starts_with("http://") || href.starts_with("https://") {
        "_blank"
    } else {
        "_self"
    }
}
