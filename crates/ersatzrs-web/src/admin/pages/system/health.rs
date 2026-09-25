use leptos::prelude::*;

use crate::admin::i18n::{health_check_message, health_check_title, health_summary, tr, TextKey};
use crate::admin::release_notes::ReleaseNotesCard;
#[cfg(target_arch = "wasm32")]
use crate::admin::server_fns::layout::media_state_revision;
use crate::admin::server_fns::layout::{
    load_health_snapshot, AdminFfmpegRuntimeStatus, AdminHealthRow, AdminHealthSnapshot,
};
use crate::admin::ui::primitives::resource_state::{
    admin_loading_resource_view, admin_result_view,
};
use crate::admin::ui::primitives::system_navigation::SystemPageLayout;
#[cfg(target_arch = "wasm32")]
use crate::admin::ui::primitives::toast::ToastSeverity;
use crate::admin::ui::primitives::toast::{load_or_notify, use_toast};

#[component]
pub fn HealthPage() -> impl IntoView {
    let snapshot = Resource::new_blocking(|| (), |()| load_health_snapshot());

    view! {
        <SystemPageLayout>
            <Suspense fallback=admin_loading_resource_view>
                {move || snapshot.get().map(|result| admin_result_view(result, |initial_snapshot| {
                    view! { <HealthIsland initial_snapshot=initial_snapshot/> }.into_any()
                }))}
            </Suspense>
            <div style="max-width:64rem;margin:0 auto;padding:0 0 2rem">
                <ReleaseNotesCard/>
            </div>
        </SystemPageLayout>
    }
}

#[island]
fn HealthIsland(initial_snapshot: AdminHealthSnapshot) -> impl IntoView {
    let media_revision = RwSignal::new(String::new());
    let snapshot = RwSignal::new(initial_snapshot);
    let load_toast = use_toast();
    let refreshed_snapshot: LocalResource<AdminHealthSnapshot> = LocalResource::new(move || {
        let _ = media_revision.get();
        async move { load_or_notify(load_health_snapshot().await, load_toast) }
    });

    #[cfg(target_arch = "wasm32")]
    let revision_load_failed = RwSignal::new(false);
    Effect::new(move |_| {
        if let Some(next) = refreshed_snapshot.get() {
            snapshot.set(next);
        }
    });

    Effect::new(move |_| {
        #[cfg(target_arch = "wasm32")]
        {
            leptos::task::spawn_local(async move {
                loop {
                    match media_state_revision().await {
                        Ok(next) => {
                            if media_revision.get_untracked() != next {
                                media_revision.set(next);
                            }
                            revision_load_failed.set(false);
                        }
                        Err(error) if !revision_load_failed.get_untracked() => {
                            revision_load_failed.set(true);
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
        }
    });

    view! {
        <div style="max-width:64rem;margin:0 auto;padding:2rem 0;display:flex;flex-direction:column;gap:1.5rem">
            <h1 style="font-size:1.5rem;font-weight:700;color:var(--color-gray-100);margin:0">
                {tr(TextKey::HealthChecks)}
            </h1>
            <p style="font-size:0.875rem;color:var(--color-muted);margin:0">
                {tr(TextKey::HealthPersistedRunHelp)}
            </p>
            {move || render_snapshot(snapshot.get())}
        </div>
    }
}

fn render_snapshot(mut snapshot: AdminHealthSnapshot) -> AnyView {
    if snapshot.rows.is_empty() {
        return view! {
            <p style="font-size:0.875rem;color:var(--color-muted)">
                {tr(TextKey::HealthNoPersistedRun)}
            </p>
        }
        .into_any();
    }

    let summary = health_summary(snapshot.errors, snapshot.warnings);

    let runtime = snapshot.ffmpeg_runtime.take();
    let rows = std::mem::take(&mut snapshot.rows);
    view! {
        <p style="font-size:0.875rem;color:var(--color-gray-300);margin:0">{summary}</p>
        {runtime.map(render_ffmpeg_runtime)}
        <table style="width:100%;border-collapse:collapse;font-size:0.875rem">
            <thead>
                <tr style="border-bottom:1px solid var(--color-border);color:var(--color-gray-300);text-align:left">
                    <th style="padding:0.5rem 0.75rem;font-weight:600">{tr(TextKey::Check)}</th>
                    <th style="padding:0.5rem 0.75rem;font-weight:600">{tr(TextKey::Message)}</th>
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
        <section style="border:1px solid var(--color-border);border-radius:6px;padding:1rem">
            <h2 style="font-size:1rem;font-weight:700;color:var(--color-gray-100);margin:0 0 0.75rem 0">
                {tr(TextKey::FfmpegRuntime)}
            </h2>
            <dl style="display:grid;gap:0.5rem 1rem;grid-template-columns:max-content minmax(0,1fr);margin:0;font-size:0.875rem">
                <dt style="color:var(--color-gray-300)">{tr(TextKey::FfmpegRuntimeSource)}</dt>
                <dd style="color:var(--color-gray-100);margin:0">{source}</dd>
                <dt style="color:var(--color-gray-300)">{tr(TextKey::FfmpegRuntimeVersion)}</dt>
                <dd style="color:var(--color-gray-100);margin:0">
                    {format!("ffmpeg {}, ffprobe {}", runtime.ffmpeg_version, runtime.ffprobe_version)}
                </dd>
                <dt style="color:var(--color-gray-300)">{tr(TextKey::FfmpegRuntimeRelease)}</dt>
                <dd style="color:var(--color-gray-100);margin:0">{installed}</dd>
                <dt style="color:var(--color-gray-300)">{tr(TextKey::FfmpegRuntimeLatestRelease)}</dt>
                <dd style="color:var(--color-gray-100);margin:0">{latest}</dd>
                <dt style="color:var(--color-gray-300)">{tr(TextKey::Message)}</dt>
                <dd style="color:var(--color-gray-100);margin:0">{update}</dd>
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
    let color = status_color(persisted_status.as_str());
    let status = status_label(persisted_status.as_str());
    let title = health_check_title(&persisted_title);
    let message = health_check_message(&persisted_title, &persisted_message, &brief_message);

    view! {
        <tr style="border-bottom:1px solid var(--color-border-subtle)">
            <td style="padding:0.5rem 0.75rem;vertical-align:top;color:var(--color-gray-100)">
                <div style="align-items:center;display:flex;gap:0.5rem">
                    <span
                        aria-label=status
                        title=status
                        style=format!("background:{color};border-radius:999px;display:inline-block;flex:0 0 auto;height:0.65rem;width:0.65rem")
                    ></span>
                    <span>{title}</span>
                </div>
            </td>
            <td style="padding:0.5rem 0.75rem;vertical-align:top;color:var(--color-gray-300)">
                {match link {
                    Some(href) => view! {
                        <a href=href.clone() target=link_target(&href) rel="noopener" style="color:var(--color-primary)">
                            {message}
                        </a>
                    }.into_any(),
                    None => view! { <span>{message}</span> }.into_any(),
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

fn status_color(status: &str) -> &'static str {
    match status {
        "Fail" => "var(--color-error, #f44336)",
        "Warning" => "var(--color-warning, #ff9800)",
        "Info" => "var(--color-primary, #00c0c0)",
        "Pass" => "var(--color-success, #4caf50)",
        _ => "var(--color-muted)",
    }
}

fn link_target(href: &str) -> &'static str {
    if href.starts_with("http://") || href.starts_with("https://") {
        "_blank"
    } else {
        "_self"
    }
}
