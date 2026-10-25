use leptos::prelude::*;
use leptos_router::hooks::use_query_map;

use crate::admin::i18n::{tr, unable_to_load_playback_troubleshooting, TextKey};
use crate::admin::server_fns::troubleshooting::{
    list_block_playout_history, list_block_playout_options, list_blocks_for_playout,
    load_mpeg_troubleshooting_context, load_owned_preview_status,
    load_playback_troubleshooting_context, load_troubleshooting_report,
    validate_sequential_schedule_file, BlockPlayoutHistoryRow, BlockPlayoutOption,
    MpegTroubleshootingContext, MpegTroubleshootingReport, MpegTroubleshootingStatus,
    MpegTsduckAnalysis, MpegTsduckStabilityLevel, MpegTsduckStabilitySample, OwnedPreviewView,
    PlaybackTroubleshootingContext, PlaybackTroubleshootingMode, TroubleshootingBlockRow,
    TroubleshootingReport, YamlValidationResult, MPEG_DEFAULT_SAMPLE_DURATION_SECONDS,
    MPEG_MAX_SAMPLE_DURATION_SECONDS,
};
use crate::admin::ui::primitives::resource_state::{
    admin_loading_resource_view, admin_result_view,
};
use crate::admin::ui::primitives::system_navigation::SystemPageLayout;

stylance::import_crate_style!(style, "src/admin/pages/system/troubleshooting.module.css");

#[component]
pub fn TroubleshootingPage() -> impl IntoView {
    let report = Resource::new_blocking(|| (), |()| load_troubleshooting_report());

    view! {
        <SystemPageLayout>
            <div class=style::page>
                <header>
                    <h1 class=style::heading>
                        {tr(TextKey::Troubleshooting)}
                    </h1>
                    <div class=style::divider></div>
                </header>

                <Suspense fallback=admin_loading_resource_view>
                    {move || report.get().map(|result| admin_result_view(result, render_report))}
                </Suspense>
            </div>
        </SystemPageLayout>
    }
}

fn render_report(report: TroubleshootingReport) -> AnyView {
    view! {
        <div class=style::tabShell>
            <div role="tablist" aria-label=tr(TextKey::TroubleshootingReports) class=style::tabs>
                {tab_link(tr(TextKey::Tools), "tools-panel")}
                {tab_link(tr(TextKey::General), "general-panel")}
                {report.show_video_toolbox.then(|| tab_link("VideoToolbox", "videotoolbox-panel"))}
                {report.show_nvidia_qsv.then(|| tab_link("NVIDIA", "nvidia-panel"))}
                {report.show_nvidia_qsv.then(|| tab_link("QSV", "qsv-panel"))}
                {report.show_vaapi.then(|| tab_link("VAAPI", "vaapi-panel"))}
            </div>

            <section
                id="tools-panel"
                role="tabpanel"
                aria-label=tr(TextKey::Tools)
                class=style::panel
            >
                <div class=style::tools>
                    <a href="/system/troubleshooting/sequential-schedule" class=style::toolLink>
                        {tr(TextKey::SequentialScheduleValidation)}
                    </a>
                    <a href="/system/troubleshooting/block-playout" class=style::toolLink>
                        {tr(TextKey::BlockPlayoutTroubleshooting)}
                    </a>
                    <a href="/system/troubleshooting/mpeg" class=style::toolLink>
                        {tr(TextKey::MpegTsTroubleshooting)}
                    </a>
                </div>
            </section>

            <section
                id="general-panel"
                role="tabpanel"
                aria-label=tr(TextKey::General)
                class=style::panel
            >
                {report_block(report.general_json, true)}
            </section>

            {report.show_video_toolbox.then(|| view! {
                <section
                    id="videotoolbox-panel"
                    role="tabpanel"
                    aria-label="VideoToolbox"
                    class=style::panel
                >
                    {report_block(report.video_toolbox_capabilities.clone(), true)}
                </section>
            })}

            {report.show_nvidia_qsv.then(|| view! {
                <section
                    id="nvidia-panel"
                    role="tabpanel"
                    aria-label="NVIDIA"
                    class=style::panel
                >
                    {report_block(report.nvidia_capabilities.clone(), true)}
                </section>
            })}

            {report.show_nvidia_qsv.then(|| view! {
                <section
                    id="qsv-panel"
                    role="tabpanel"
                    aria-label="QSV"
                    class=style::panel
                >
                    {report_block(report.qsv_capabilities.clone(), true)}
                </section>
            })}

            {report.show_vaapi.then(|| view! {
                <section
                    id="vaapi-panel"
                    role="tabpanel"
                    aria-label="VAAPI"
                    class=style::panel
                >
                    {report_block(report.vaapi_capabilities, true)}
                </section>
            })}
        </div>
    }
    .into_any()
}

#[component]
pub fn MpegTroubleshootingPage() -> impl IntoView {
    let query = use_query_map();
    let selected_channel = move || {
        query.with(|params| {
            params
                .get("channel")
                .or_else(|| params.get("channelNumber"))
                .unwrap_or_default()
        })
    };
    let selected_mode = move || {
        query.with(|params| {
            params
                .get("mode")
                .unwrap_or_else(|| "internal-legacy".to_owned())
        })
    };
    let duration_seconds = move || {
        query.with(|params| {
            params
                .get("duration")
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(MPEG_DEFAULT_SAMPLE_DURATION_SECONDS)
        })
    };
    let retain_raw_sample = move || {
        query.with(|params| {
            params
                .get("retainRawSample")
                .is_some_and(|value| matches!(value.as_str(), "true" | "1" | "on"))
        })
    };

    let context: Resource<Result<MpegTroubleshootingContext, ServerFnError>> =
        Resource::new_blocking(
            move || {
                (
                    selected_channel(),
                    selected_mode(),
                    duration_seconds(),
                    retain_raw_sample(),
                )
            },
            |(channel, mode, duration, retain)| async move {
                load_mpeg_troubleshooting_context(channel, mode, duration, retain).await
            },
        );

    view! {
        <SystemPageLayout>
            <div class=style::page>
                <header>
                    <h1 class=style::heading>{tr(TextKey::MpegTsTroubleshooting)}</h1>
                    <div class=style::divider></div>
                </header>

                <Suspense fallback=move || view! { <p>{tr(TextKey::Loading)}</p> }>
                    {move || match context.get() {
                        Some(Ok(context)) => render_mpeg_troubleshooting(context),
                        Some(Err(error)) => view! {
                            <p class=style::empty>{error.to_string()}</p>
                        }.into_any(),
                        None => view! { <p>{tr(TextKey::Loading)}</p> }.into_any(),
                    }}
                </Suspense>
            </div>
        </SystemPageLayout>
    }
}

fn render_mpeg_troubleshooting(context: MpegTroubleshootingContext) -> AnyView {
    let availability = if context.availability.available {
        context
            .availability
            .version
            .clone()
            .unwrap_or_else(|| "available".to_owned())
    } else {
        context
            .availability
            .error
            .clone()
            .unwrap_or_else(|| "not available".to_owned())
    };

    view! {
        <form method="get" action="/system/troubleshooting/mpeg" class=style::form>
            <label class=style::field>
                <span>{tr(TextKey::Channel)}</span>
                <select class=style::select name="channel">
                    <option value="">{tr(TextKey::DefaultOption)}</option>
                    {context.channels.into_iter().map(|channel| {
                        let selected = channel.number == context.selected_channel_number;
                        let label = format!("{} - {}", channel.number, channel.name);
                        view! {
                            <option value=channel.number selected=selected>{label}</option>
                        }
                    }).collect_view()}
                </select>
            </label>
            <label class=style::field>
                <span>{tr(TextKey::Mode)}</span>
                <select class=style::select name="mode">
                    <option value="internal-legacy" selected=context.selected_mode == "internal-legacy">
                        {tr(TextKey::InternalMpegTs)}
                    </option>
                    <option value="public-ts" selected=context.selected_mode == "public-ts">
                        {tr(TextKey::PublicMpegTs)}
                    </option>
                </select>
            </label>
            <label class=style::field>
                <span>{tr(TextKey::DurationSeconds)}</span>
                <input
                    class=style::input
                    type="number"
                    min="1"
                    max=MPEG_MAX_SAMPLE_DURATION_SECONDS.to_string()
                    name="duration"
                    value=context.duration_seconds.to_string()
                />
            </label>
            <label class=style::checkField>
                <input
                    type="checkbox"
                    name="retainRawSample"
                    value="true"
                    checked=context.retain_raw_sample
                />
                <span>{tr(TextKey::RetainRawSample)}</span>
            </label>
            <button type="submit" class=style::primaryButton>{tr(TextKey::Check)}</button>
        </form>

        <dl class=style::detailsGrid>
            <dt>{tr(TextKey::TsdDuckVersion)}</dt>
            <dd>{availability}</dd>
        </dl>

        {context.latest_report.map(render_mpeg_report)}
    }
    .into_any()
}

fn render_mpeg_report(report: MpegTroubleshootingReport) -> AnyView {
    let status = mpeg_status_label(report.status);
    let sample_path = report
        .sample_path
        .clone()
        .unwrap_or_else(|| tr(TextKey::No).to_owned());
    let errors = report.errors.clone();
    let analysis = report.analysis.clone();

    view! {
        <section>
            <h2 class=style::subheading>{tr(TextKey::Report)}</h2>
            <div class=style::divider></div>
            <dl class=style::detailsGrid>
                <dt>{tr(TextKey::RunId)}</dt>
                <dd><code>{report.run_id}</code></dd>
                <dt>{tr(TextKey::Status)}</dt>
                <dd>{status}</dd>
                <dt>{tr(TextKey::Summary)}</dt>
                <dd>{report.summary}</dd>
                <dt>{tr(TextKey::SampleBytes)}</dt>
                <dd>{report.sample_bytes.to_string()}</dd>
                <dt>{tr(TextKey::RawSampleArchived)}</dt>
                <dd>{sample_path}</dd>
                <dt>{tr(TextKey::ArchivePath)}</dt>
                <dd><code>{report.archive_path}</code></dd>
                <dt>{tr(TextKey::SampleUrl)}</dt>
                <dd><code>{report.sample_url}</code></dd>
            </dl>

            {(!errors.is_empty()).then(|| view! {
                <section>
                    <h3 class=style::subheading>{tr(TextKey::Messages)}</h3>
                    {report_block(errors.join("\n"), false)}
                </section>
            })}

            {analysis.map(render_mpeg_analysis)}
        </section>
    }
    .into_any()
}

fn render_mpeg_analysis(analysis: MpegTsduckAnalysis) -> AnyView {
    let MpegTsduckAnalysis {
        stability_summary,
        stability_samples,
        analysis_json_excerpt,
        continuity_findings,
        pcr_findings,
        services,
        pids,
        bitrate_bps,
        stdout_excerpt,
        stderr_excerpt,
        ..
    } = analysis;

    view! {
        <div class=style::historyStack>
            {render_stability_meter(stability_summary, &stability_samples)}
            {render_string_list(tr(TextKey::Services), services)}
            {render_string_list(tr(TextKey::Pids), pids)}
            {render_string_list(tr(TextKey::ContinuityFindings), continuity_findings)}
            {render_string_list(tr(TextKey::PcrFindings), pcr_findings)}
            <dl class=style::detailsGrid>
                <dt>{tr(TextKey::Bitrate)}</dt>
                <dd>{bitrate_bps.map_or_else(String::new, |value| value.to_string())}</dd>
            </dl>
            <section>
                <h3 class=style::subheading>{tr(TextKey::AnalysisJson)}</h3>
                {report_block(analysis_json_excerpt, true)}
            </section>
            <section>
                <h3 class=style::subheading>{tr(TextKey::StandardOutput)}</h3>
                {report_block(stdout_excerpt, true)}
            </section>
            <section>
                <h3 class=style::subheading>{tr(TextKey::StandardError)}</h3>
                {report_block(stderr_excerpt, true)}
            </section>
        </div>
    }
    .into_any()
}

fn render_stability_meter(summary: String, samples: &[MpegTsduckStabilitySample]) -> AnyView {
    if samples.is_empty() {
        return view! {
            <section class=style::stabilityMeter>
                <h3 class=style::subheading>{tr(TextKey::StabilityMeter)}</h3>
                <p class=style::empty>{tr(TextKey::NoMessages)}</p>
            </section>
        }
        .into_any();
    }

    let max_bitrate = samples
        .iter()
        .map(|sample| sample.bitrate_bps)
        .max()
        .unwrap_or_default();
    let aria_label = summary.clone();

    view! {
        <section class=style::stabilityMeter>
            <h3 class=style::subheading>{tr(TextKey::StabilityMeter)}</h3>
            <p class=style::meterSummary>{summary}</p>
            <div class=style::stabilityBars role="img" aria-label=aria_label>
                {samples.iter().map(|sample| {
                    render_stability_sample(sample, max_bitrate)
                }).collect_view()}
            </div>
            <div class=style::stabilityLegend aria-hidden="true">
                {render_stability_legend_item(MpegTsduckStabilityLevel::Stable)}
                {render_stability_legend_item(MpegTsduckStabilityLevel::Warning)}
                {render_stability_legend_item(MpegTsduckStabilityLevel::Error)}
            </div>
        </section>
    }
    .into_any()
}

fn render_stability_sample(sample: &MpegTsduckStabilitySample, max_bitrate: u64) -> AnyView {
    let label = stability_level_label(sample.level);
    let tooltip = format!(
        "{}-{}s: {}; {} {}; {} bit/s. {}",
        sample.second_start,
        sample.second_end,
        label,
        sample.packet_count,
        tr(TextKey::Packets),
        sample.bitrate_bps,
        sample.note
    );
    let class = format!(
        "{} {}",
        style::stabilitySegment,
        stability_level_class(sample.level)
    );
    let height = format!(
        "height: {}%;",
        stability_bar_height(sample.bitrate_bps, max_bitrate)
    );

    view! {
        <div class=style::stabilitySecond>
            <span class=class style=height title=tooltip.clone() aria-label=tooltip></span>
            <span class=style::stabilityTick>{sample.second_end.to_string()}</span>
        </div>
    }
    .into_any()
}

fn render_stability_legend_item(level: MpegTsduckStabilityLevel) -> AnyView {
    let class = format!(
        "{} {}",
        style::stabilityLegendSwatch,
        stability_level_class(level)
    );

    view! {
        <span class=style::stabilityLegendItem>
            <span class=class></span>
            <span>{stability_level_label(level)}</span>
        </span>
    }
    .into_any()
}

fn stability_level_class(level: MpegTsduckStabilityLevel) -> &'static str {
    match level {
        MpegTsduckStabilityLevel::Stable => style::stabilityStable,
        MpegTsduckStabilityLevel::Warning => style::stabilityWarning,
        MpegTsduckStabilityLevel::Error => style::stabilityError,
    }
}

fn stability_level_label(level: MpegTsduckStabilityLevel) -> &'static str {
    match level {
        MpegTsduckStabilityLevel::Stable => tr(TextKey::Stable),
        MpegTsduckStabilityLevel::Warning => tr(TextKey::Warning),
        MpegTsduckStabilityLevel::Error => tr(TextKey::Error),
    }
}

fn stability_bar_height(bitrate_bps: u64, max_bitrate_bps: u64) -> u64 {
    if max_bitrate_bps == 0 {
        return 20;
    }

    20 + bitrate_bps.saturating_mul(80) / max_bitrate_bps
}

fn render_string_list(label: &'static str, values: Vec<String>) -> AnyView {
    view! {
        <section>
            <h3 class=style::subheading>{label}</h3>
            {if values.is_empty() {
                view! { <p class=style::empty>{tr(TextKey::NoMessages)}</p> }.into_any()
            } else {
                view! {
                    <div class=style::tableWrap>
                        <table class=style::table>
                            <tbody>
                                {values.into_iter().map(|value| view! {
                                    <tr><td><code>{value}</code></td></tr>
                                }).collect_view()}
                            </tbody>
                        </table>
                    </div>
                }.into_any()
            }}
        </section>
    }
    .into_any()
}

fn mpeg_status_label(status: MpegTroubleshootingStatus) -> &'static str {
    match status {
        MpegTroubleshootingStatus::NotAvailable => tr(TextKey::NotAvailable),
        MpegTroubleshootingStatus::NotApplicable => tr(TextKey::NotApplicable),
        MpegTroubleshootingStatus::Completed => tr(TextKey::Completed),
        MpegTroubleshootingStatus::Failed => tr(TextKey::Failed),
    }
}

#[component]
pub fn PlaybackTroubleshootingPage() -> impl IntoView {
    let query = use_query_map();
    let media_item_id = query.with_untracked(|params| {
        params
            .get("mediaItem")
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or_default()
    });
    let channel_id = query.with_untracked(|params| {
        params
            .get("channel")
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or_default()
    });
    let preferred_audio_title =
        query.with_untracked(|params| params.get("preferredAudioTitle").unwrap_or_default());
    let selected_subtitle_id = query.with_untracked(|params| {
        params
            .get("subtitleId")
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or_default()
    });

    let preview_id = query.with_untracked(|params| params.get("previewId").unwrap_or_default());
    let context: Resource<
        Result<(PlaybackTroubleshootingContext, Option<OwnedPreviewView>), ServerFnError>,
    > = Resource::new_blocking(
        move || {
            (
                media_item_id,
                channel_id,
                preferred_audio_title.clone(),
                selected_subtitle_id,
                preview_id.clone(),
            )
        },
        |(media_item, channel, preferred_audio, subtitle_id, preview)| async move {
            let context = load_playback_troubleshooting_context(
                media_item,
                channel,
                preferred_audio,
                subtitle_id,
            )
            .await?;
            let preview = if preview.is_empty() {
                None
            } else {
                load_owned_preview_status(preview).await?
            };
            Ok((context, preview))
        },
    );

    view! {
        <SystemPageLayout>
            <Suspense fallback=move || view! { <p>{tr(TextKey::LoadingMediaInfo)}</p> }>
                {move || match context.get() {
                    Some(Ok((context, preview))) => render_playback_troubleshooting(context, preview),
                    Some(Err(error)) => view! {
                        <div class=style::page>
                            <p style="margin:0;color:var(--color-error,#f87171)">
                                {unable_to_load_playback_troubleshooting(error)}
                            </p>
                        </div>
                    }.into_any(),
                    None => view! { <p>{tr(TextKey::LoadingMediaInfo)}</p> }.into_any(),
                }}
            </Suspense>
        </SystemPageLayout>
    }
}

fn render_playback_troubleshooting(
    context: PlaybackTroubleshootingContext,
    preview: Option<OwnedPreviewView>,
) -> AnyView {
    let is_media_item = context.mode == PlaybackTroubleshootingMode::MediaItem;
    let is_channel = context.mode == PlaybackTroubleshootingMode::Channel;
    let title = if context.title.trim().is_empty() {
        if is_channel {
            "Channel Settings".to_owned()
        } else {
            format!("{} Settings", tr(TextKey::Playback))
        }
    } else if is_channel {
        format!("Channel Settings - {}", context.title)
    } else {
        format!("{} Settings - {}", context.kind, context.title)
    };
    let default_seek = (context.duration_seconds / 2).max(0).to_string();
    let sample_href = format!(
        "/api/troubleshoot/playback/sample/{}",
        context.media_item_id
    );

    view! {
        <div class=style::playbackShell>
            <div class=style::playbackToolbar>
                <a class=style::secondaryButton href="/api/troubleshoot/playback/archive">
                    {tr(TextKey::DownloadResults)}
                </a>
                {is_media_item.then(|| view! {
                    <a class=style::secondaryButton href=sample_href>
                        {tr(TextKey::DownloadMediaSample)}
                    </a>
                })}
            </div>

            <div class=style::page>
                <header>
                    <h1 class=style::heading>{title}</h1>
                    <div class=style::divider></div>
                </header>

                {if context.mode == PlaybackTroubleshootingMode::Missing {
                    view! {
                        <section class=style::panel style="display:block">
                            <p class=style::empty>{tr(TextKey::MissingMediaItemQueryParameter)}</p>
                        </section>
                    }.into_any()
                } else {
                    render_playback_form(context, default_seek, preview)
                }}
            </div>
        </div>
    }
    .into_any()
}

fn render_playback_form(
    context: PlaybackTroubleshootingContext,
    default_seek: String,
    preview: Option<OwnedPreviewView>,
) -> AnyView {
    let is_media_item = context.mode == PlaybackTroubleshootingMode::MediaItem;
    let use_next = context.streaming_engine == ersatzrs_domain::StreamingEngine::Next;
    let stream_selectors_disabled = context.stream_selectors.is_empty();
    let preferred_audio_title = context.preferred_audio_title.clone();
    let selected_subtitle_id = context.selected_subtitle_id;
    let is_music_video = context.kind == "Music Video";

    let preview = preview.unwrap_or_default();
    let log = if preview.log.is_empty() && !preview.error_code.is_empty() {
        preview_failure_label(&preview.error_code).to_owned()
    } else {
        preview.log.clone()
    };
    let preview_id = preview.id.clone();
    view! {
        <form method="post" action="/admin-api/troubleshoot/playback/start" class=style::playbackForm data-playback-troubleshooting data-preview-id=preview_id>
            <input type="hidden" name="mediaItem" value=context.media_item_id.to_string()/>
            <input type="hidden" name="channel" value=context.channel_id.to_string()/>
            <input type="hidden" name="streamingMode" value="1"/>
            <input type="hidden" name="preferredAudioTitle" value=preferred_audio_title/>

            <label class=style::playbackField>
                <span>{tr(TextKey::StreamingEngine)}</span>
                <select class=style::select name="streamingEngine">
                    <option value="Legacy" selected=!use_next>"Legacy"</option>
                    <option value="Next" selected=use_next>"Next"</option>
                </select>
            </label>
            {playback_select_field(
                tr(TextKey::FfmpegProfile),
                "ffmpegProfile",
                context.selected_ffmpeg_profile_id,
                context.ffmpeg_profiles,
                false,
            )}
            <label class=style::playbackField>
                <span>{tr(TextKey::StreamSelector)}</span>
                <select class=style::select name="streamSelector" disabled=stream_selectors_disabled>
                    <option value="">{tr(TextKey::DefaultOption)}</option>
                    {context.stream_selectors.into_iter().map(|selector| {
                        view! { <option value=selector.clone()>{selector.clone()}</option> }
                    }).collect_view()}
                </select>
            </label>

            {if is_media_item {
                view! {
                    <label class=style::playbackField>
                        <span>{tr(TextKey::Subtitle)}</span>
                        <select class=style::select name="subtitleId">
                            <option value="" selected=selected_subtitle_id == 0>{tr(TextKey::DefaultOption)}</option>
                            {context.subtitles.into_iter().map(|subtitle| {
                                let label = format!(
                                    "{}: {} - {} ({})",
                                    subtitle.id, subtitle.language, subtitle.title, subtitle.codec
                                );
                                view! {
                                    <option
                                        value=subtitle.id.to_string()
                                        selected=subtitle.id == selected_subtitle_id
                                    >
                                        {label}
                                    </option>
                                }
                            }).collect_view()}
                        </select>
                    </label>
                    {is_music_video.then(|| playback_credits_field(context.music_video_credits_templates))}
                    {playback_watermark_field(context.watermarks)}
                    <div class=style::wideField>
                        {playback_multi_select_field(tr(TextKey::GraphicsElements), "graphicsElement", context.graphics_elements)}
                    </div>
                    <label class=style::playbackCheck>
                        <span>{tr(TextKey::StartFromBeginning)}</span>
                        <input type="checkbox" name="startFromBeginning" value="true"/>
                    </label>
                    <label class=style::playbackField>
                        <span>{tr(TextKey::SeekSeconds)}</span>
                        <input class=style::input type="number" min="0" name="seekSeconds" value=default_seek/>
                    </label>
                }.into_any()
            } else {
                playback_channel_seek_fields().into_any()
            }}

            {playback_preview_player(preview)}

            <section class=style::wideSection>
                <h2 class=style::subheading>{tr(TextKey::Logs)}</h2>
                <p data-preview-speed hidden aria-live="polite">
                    {tr(TextKey::PlaybackSpeed)}": "<output></output>"x"
                </p>
                <div class=style::divider></div>
                <textarea class=style::logs readonly rows="20">{log}</textarea>
            </section>

            {is_media_item.then(|| view! {
                <section class=style::wideSection>
                    <h2 class=style::subheading>{tr(TextKey::MediaItem)}</h2>
                    {report_block(context.media_info_json, false)}
                </section>
            })}
        </form>
    }
    .into_any()
}

fn playback_channel_seek_fields() -> impl IntoView {
    view! {
        <label class=style::playbackField>
            <span>{tr(TextKey::DateAndTime)}" (UTC)"</span>
            <input class=style::input type="datetime-local" name="start" required
                aria-describedby="preview-start-help" aria-errormessage="preview-start-error"
                data-preview-start data-message-required=tr(TextKey::PreviewStartRequired)
                data-message-invalid=tr(TextKey::PreviewStartInvalid)
                value=chrono::Utc::now().format("%Y-%m-%dT%H:%M").to_string()/>
            <small id="preview-start-help" data-preview-time-help>{tr(TextKey::PreviewStartHelp)}</small>
            <small id="preview-start-error" data-preview-time-error role="alert" hidden></small>
        </label>
        <label class=style::playbackField>
            <span>{tr(TextKey::PreviewProgrammeSeek)}</span>
            <input class=style::input type="number" min="0" name="seekSeconds" value=""/>
            <small>{tr(TextKey::PreviewProgrammeSeekHelp)}</small>
        </label>
    }
}

fn playback_credits_field(templates: Vec<String>) -> impl IntoView {
    view! {
        <label class=style::playbackField>
            <span>{tr(TextKey::MusicVideoCreditsTemplate)}</span>
            <select class=style::select name="musicVideoCreditsTemplate">
                <option value="" selected>{tr(TextKey::NoneOption)}</option>
                {templates.into_iter().map(|template| view! {
                    <option value=template.clone()>{template.clone()}</option>
                }).collect_view()}
            </select>
        </label>
    }
}

fn playback_watermark_field(
    options: Vec<crate::admin::server_fns::troubleshooting::PlaybackTroubleshootingOption>,
) -> impl IntoView {
    view! {
        <label class=style::playbackField>
            <span data-watermark-label>
                {tr(TextKey::Watermarks)}
            </span>
            <select class=style::select name="watermark" multiple>
                <option value="">{tr(TextKey::NoneOption)}</option>
                {options.into_iter().map(|option| view! {
                    <option value=option.id.to_string()>{option.name}</option>
                }).collect_view()}
            </select>
        </label>
    }
}

fn preview_operation_label(state: &str) -> &'static str {
    tr(match state {
        "queued" => TextKey::PreviewQueued,
        "running" => TextKey::PreviewRunning,
        "completed" => TextKey::PreviewCompleted,
        "failed" => TextKey::Failed,
        "cancelling" => TextKey::PreviewCancelling,
        "cancelled" => TextKey::PreviewStopped,
        _ => TextKey::PreviewIdle,
    })
}

fn preview_failure_label(code: &str) -> &'static str {
    tr(match code {
        "configuration" => TextKey::PreviewConfigurationInvalid,
        "capacity" => TextKey::PreviewCapacityReached,
        "cleanup" => TextKey::PreviewCleanupUnconfirmed,
        _ => TextKey::PreviewStartupFailed,
    })
}

fn playback_preview_player(preview: OwnedPreviewView) -> impl IntoView {
    let active = matches!(preview.state.as_str(), "queued" | "running" | "cancelling");
    let blocked = preview.state == "failed" && !preview.cleanup_confirmed;
    let stop = format!("/admin-api/troubleshoot/playback/{}/stop", preview.id);
    let timestamp = preview.selected_at.clone();
    let label = preview_operation_label(&preview.state);
    let player_label = if preview.error_code.is_empty() {
        tr(TextKey::PreviewIdle)
    } else {
        preview_failure_label(&preview.error_code)
    };
    view! {
        <section class=style::wideSection data-channel-preview data-preview-state="idle"
            data-operation-state=preview.state
            data-message-queued=tr(TextKey::PreviewQueued)
            data-message-running=tr(TextKey::PreviewRunning)
            data-message-completed=tr(TextKey::PreviewCompleted)
            data-message-failed=tr(TextKey::Failed)
            data-message-cancelling=tr(TextKey::PreviewCancelling)
            data-message-cancelled=tr(TextKey::PreviewStopped)
            data-message-idle=tr(TextKey::PreviewIdle)
            data-message-connecting=tr(TextKey::PreviewConnecting)
            data-message-live=tr(TextKey::PreviewLive)
            data-message-paused=tr(TextKey::PreviewPaused)
            data-message-recovering=tr(TextKey::PreviewRecovering)
            data-message-stopped=tr(TextKey::PreviewStopped)
            data-message-unsupported=tr(TextKey::PreviewUnsupported)
            data-message-authorization=tr(TextKey::PreviewAuthorizationFailed)
            data-message-busy=tr(TextKey::PreviewAlreadyRunning)
            data-message-cleanup=tr(TextKey::PreviewCleanupUnconfirmed)
            data-message-configuration=tr(TextKey::PreviewConfigurationInvalid)
            data-message-startup=tr(TextKey::PreviewStartupFailed)
            data-message-unavailable=tr(TextKey::PreviewUnavailable)
            data-message-capacity=tr(TextKey::PreviewCapacityReached)
            data-message-network=tr(TextKey::PreviewNetworkError)
            data-message-media=tr(TextKey::PreviewMediaError)
        >
            <h2 class=style::subheading>{tr(TextKey::Preview)}</h2>
            <div class=style::divider></div>
            <button class=style::primaryButton type="submit" data-preview-play disabled=active || blocked
                aria-describedby=(active || blocked).then_some("preview-play-reason")>{tr(TextKey::Play)}</button>
            <p id="preview-play-reason" data-preview-play-reason hidden=!active && !blocked>
                {if blocked { tr(TextKey::PreviewCleanupUnconfirmed) } else { tr(TextKey::PreviewAlreadyRunning) }}
            </p>
            <button class=style::secondaryButton type="submit" data-preview-stop formaction=stop formmethod="post" formnovalidate disabled=!active || preview.state == "cancelling">{tr(TextKey::StopPreview)}</button>
            <a class=style::secondaryButton data-preview-refresh href=preview.return_url hidden=preview.id.is_empty()>{tr(TextKey::Refresh)}</a>
            <p data-preview-operation-status aria-live="polite">{label}</p>
            <p data-preview-timestamp hidden=timestamp.is_empty()>
                {tr(TextKey::PreviewTimestamp)}" "<time datetime=timestamp.clone()>{timestamp.clone()}</time>
            </p>
            <div class=style::previewFrame>
                <video controls playsinline preload="none" src=if preview.ready { preview.playlist_url } else { String::new() }></video>
                <div class=style::previewSpacer></div>
            </div>
            <p data-preview-status aria-live="polite">{player_label}</p>
        </section>
    }
}

fn playback_select_field(
    label: &'static str,
    name: &'static str,
    selected: i64,
    options: Vec<crate::admin::server_fns::troubleshooting::PlaybackTroubleshootingOption>,
    multiple: bool,
) -> impl IntoView {
    view! {
        <label class=style::playbackField>
            <span>{label}</span>
            <select class=style::select name=name multiple=multiple>
                {options.into_iter().map(|option| {
                    view! {
                        <option value=option.id.to_string() selected=selected == option.id>
                            {option.name}
                        </option>
                    }
                }).collect_view()}
            </select>
        </label>
    }
}

fn playback_multi_select_field(
    label: &'static str,
    name: &'static str,
    options: Vec<crate::admin::server_fns::troubleshooting::PlaybackTroubleshootingOption>,
) -> impl IntoView {
    playback_select_field(label, name, 0, options, true)
}

#[component]
pub fn SequentialScheduleValidationPage() -> impl IntoView {
    let query = use_query_map();
    let yaml_file = move || {
        query.with(|params| {
            params
                .get("yamlFile")
                .or_else(|| params.get("YAMLFile"))
                .unwrap_or_default()
        })
    };
    let is_import = move || {
        query.with(|params| {
            params
                .get("isImport")
                .is_some_and(|value| matches!(value.as_str(), "true" | "1" | "import"))
        })
    };

    let result = Resource::new_blocking(
        move || (yaml_file(), is_import()),
        |(path, import)| async move {
            if path.trim().is_empty() {
                Ok(YamlValidationResult::default())
            } else {
                validate_sequential_schedule_file(path, import).await
            }
        },
    );

    view! {
        <SystemPageLayout>
            <div class=style::page>
                <header>
                    <h1 class=style::heading>{tr(TextKey::SequentialScheduleValidation)}</h1>
                    <div class=style::divider></div>
                </header>

                <form method="get" action="/system/troubleshooting/sequential-schedule" class=style::form>
                    <label class=style::field>
                        <span>{tr(TextKey::YamlFile)}</span>
                        <input
                            class=style::input
                            type="text"
                            name="yamlFile"
                            value=yaml_file()
                        />
                    </label>
                    <label class=style::field>
                        <span>{tr(TextKey::Schema)}</span>
                        <select class=style::select name="isImport">
                            <option value="false" selected=move || !is_import()>{tr(TextKey::Full)}</option>
                            <option value="true" selected=is_import>{tr(TextKey::Import)}</option>
                        </select>
                    </label>
                    <button type="submit" class=style::primaryButton>
                        {tr(TextKey::Validate)}
                    </button>
                </form>

                <Suspense fallback=admin_loading_resource_view>
                    {move || result.get().map(|result| admin_result_view(result, render_yaml_validation))}
                </Suspense>
            </div>
        </SystemPageLayout>
    }
}

#[component]
pub fn BlockPlayoutTroubleshootingPage() -> impl IntoView {
    let query = use_query_map();
    let selected_playout = move || {
        query.with(|params| {
            params
                .get("playoutId")
                .or_else(|| params.get("playout"))
                .and_then(|value| value.parse::<i64>().ok())
                .unwrap_or_default()
        })
    };
    let search = move || query.with(|params| params.get("q").unwrap_or_default());

    let options = Resource::new_blocking(|| (), |()| list_block_playout_options());
    let blocks = Resource::new_blocking(selected_playout, list_blocks_for_playout);

    view! {
        <SystemPageLayout>
            <div class=style::page>
                <header>
                    <h1 class=style::heading>{tr(TextKey::BlockPlayoutTroubleshooting)}</h1>
                    <div class=style::divider></div>
                </header>

                <Suspense fallback=admin_loading_resource_view>
                    {move || options.get().map(|result| admin_result_view(result, |items| {
                        render_block_playout_selector(items, selected_playout(), &search())
                    }))}
                </Suspense>

                <section>
                    <h2 class=style::subheading>{tr(TextKey::Blocks)}</h2>
                    <Suspense fallback=admin_loading_resource_view>
                        {move || blocks.get().map(|result| admin_result_view(result, |items| {
                            render_blocks_table(items, selected_playout(), &search())
                        }))}
                    </Suspense>
                </section>
            </div>
        </SystemPageLayout>
    }
}

#[component]
pub fn BlockPlayoutHistoryPage() -> impl IntoView {
    let query = use_query_map();
    let playout_id = move || {
        query.with(|params| {
            params
                .get("playoutId")
                .and_then(|value| value.parse::<i64>().ok())
                .unwrap_or_default()
        })
    };
    let block_id = move || {
        query.with(|params| {
            params
                .get("blockId")
                .and_then(|value| value.parse::<i64>().ok())
                .unwrap_or_default()
        })
    };
    let history_id = move || {
        query.with(|params| {
            params
                .get("historyId")
                .and_then(|value| value.parse::<i64>().ok())
                .unwrap_or_default()
        })
    };

    let history = Resource::new_blocking(
        move || (playout_id(), block_id()),
        |(playout, block)| list_block_playout_history(playout, block),
    );

    view! {
        <SystemPageLayout>
            <div class=style::page>
                <header>
                    <h1 class=style::heading>{tr(TextKey::BlockPlayoutHistory)}</h1>
                    <div class=style::divider></div>
                </header>

                <Suspense fallback=admin_loading_resource_view>
                    {move || history.get().map(|result| admin_result_view(result, |items| {
                        render_history(items, playout_id(), block_id(), history_id())
                    }))}
                </Suspense>
            </div>
        </SystemPageLayout>
    }
}

fn tab_link(label: &'static str, panel_id: &'static str) -> impl IntoView {
    let href = format!("#{panel_id}");

    view! {
        <a href=href role="tab" class=style::tab>
            {label}
        </a>
    }
}

fn render_yaml_validation(result: YamlValidationResult) -> AnyView {
    if !result.exists && result.yaml_text.is_empty() {
        return view! { <section class=style::panel style="display:block"></section> }.into_any();
    }

    let messages = if result.messages.is_empty() {
        tr(TextKey::NoMessages).to_owned()
    } else {
        result.messages.join("\n")
    };

    view! {
        <div class=style::tabShell>
            <div role="tablist" aria-label=tr(TextKey::SequentialScheduleValidation) class=style::tabs>
                {tab_link(tr(TextKey::Yaml), "yaml-panel")}
                {tab_link(tr(TextKey::Json), "json-panel")}
                {tab_link(tr(TextKey::Messages), "messages-panel")}
            </div>
            <section id="yaml-panel" role="tabpanel" aria-label=tr(TextKey::Yaml) class=style::panel>
                {report_block(result.yaml_text, false)}
            </section>
            <section id="json-panel" role="tabpanel" aria-label=tr(TextKey::Json) class=style::panel>
                {report_block(result.json_text, false)}
            </section>
            <section id="messages-panel" role="tabpanel" aria-label=tr(TextKey::Messages) class=style::panel>
                {report_block(messages, false)}
            </section>
        </div>
    }
    .into_any()
}

fn render_block_playout_selector(
    options: Vec<BlockPlayoutOption>,
    selected: i64,
    search: &str,
) -> AnyView {
    view! {
        <form method="get" action="/system/troubleshooting/block-playout" class=style::form>
            <label class=style::field>
                <span>{tr(TextKey::BlockPlayout)}</span>
                <select class=style::select name="playoutId">
                    <option value="">{tr(TextKey::DefaultOption)}</option>
                    {options.into_iter().map(|option| {
                        let id_text = option.playout_id.to_string();
                        let label = format!(
                            "{} - {}",
                            option.channel_number,
                            option.channel_name
                        );
                        view! {
                            <option value=id_text.clone() selected=selected == option.playout_id>
                                {label}
                            </option>
                        }
                    }).collect_view()}
                </select>
            </label>
            <label class=style::field>
                <span>{tr(TextKey::SearchForBlocks)}</span>
                <input class=style::input type="search" name="q" value=search.to_owned()/>
            </label>
            <button type="submit" class=style::primaryButton>{tr(TextKey::Check)}</button>
        </form>
    }
    .into_any()
}

fn render_blocks_table(
    rows: Vec<TroubleshootingBlockRow>,
    playout_id: i64,
    search: &str,
) -> AnyView {
    let needle = search.trim().to_ascii_lowercase();
    let rows = rows
        .into_iter()
        .filter(|row| {
            needle.is_empty()
                || row.name.to_ascii_lowercase().contains(&needle)
                || row.group_name.to_ascii_lowercase().contains(&needle)
        })
        .collect::<Vec<_>>();

    if playout_id <= 0 {
        return view! { <p class=style::empty>{tr(TextKey::NoBlockPlayoutSelected)}</p> }
            .into_any();
    }
    if rows.is_empty() {
        return view! { <p class=style::empty>{tr(TextKey::NoBlocks)}</p> }.into_any();
    }

    let mut current_group = String::new();
    view! {
        <div class=style::tableWrap>
            <table class=style::table>
                <thead>
                    <tr>
                        <th>{tr(TextKey::Name)}</th>
                        <th>{tr(TextKey::Minutes)}</th>
                        <th>{tr(TextKey::StopScheduling)}</th>
                        <th></th>
                    </tr>
                </thead>
                <tbody>
                    {rows.into_iter().map(move |row| {
                        let show_group = row.group_name != current_group;
                        if show_group {
                            current_group.clone_from(&row.group_name);
                        }
                        let href = format!(
                            "/system/troubleshooting/block-playout/history?playoutId={playout_id}&blockId={}",
                            row.id
                        );
                        view! {
                            {show_group.then(|| view! {
                                <tr class=style::groupRow>
                                    <td colspan="4">{row.group_name.clone()}</td>
                                </tr>
                            })}
                            <tr>
                                <td>{row.name}</td>
                                <td>{row.minutes.to_string()}</td>
                                <td>{row.stop_scheduling.to_string()}</td>
                                <td><a class=style::iconLink href=href>{tr(TextKey::Troubleshoot)}</a></td>
                            </tr>
                        }
                    }).collect_view()}
                </tbody>
            </table>
        </div>
    }
    .into_any()
}

fn render_history(
    rows: Vec<BlockPlayoutHistoryRow>,
    playout_id: i64,
    block_id: i64,
    history_id: i64,
) -> AnyView {
    if playout_id <= 0 || block_id <= 0 {
        return view! { <p class=style::empty>{tr(TextKey::NoHistory)}</p> }.into_any();
    }
    let selected = rows
        .iter()
        .find(|row| row.id == history_id)
        .or_else(|| rows.first())
        .cloned();
    let selected_details = selected.clone();

    view! {
        <div class=style::historyStack>
            <div class=style::tableWrap>
                <table class=style::table>
                    <thead>
                        <tr>
                            <th>{tr(TextKey::Start)}</th>
                            <th>{tr(TextKey::Finish)}</th>
                            <th>{tr(TextKey::Key)}</th>
                            <th>{tr(TextKey::Details)}</th>
                        </tr>
                    </thead>
                    <tbody>
                        {rows.into_iter().map(|row| {
                            let href = format!(
                                "/system/troubleshooting/block-playout/history?playoutId={playout_id}&blockId={block_id}&historyId={}",
                                row.id
                            );
                            view! {
                                <tr>
                                    <td><a href=href>{row.start}</a></td>
                                    <td>{row.finish}</td>
                                    <td><code>{row.key}</code></td>
                                    <td><code>{row.details}</code></td>
                                </tr>
                            }
                        }).collect_view()}
                    </tbody>
                </table>
            </div>

            <section>
                <h2 class=style::subheading>{tr(TextKey::HistoryDetails)}</h2>
                <div class=style::divider></div>
                {selected_details.map_or_else(|| {
                    view! { <p class=style::empty>{tr(TextKey::NoHistory)}</p> }.into_any()
                }, render_history_details)}
            </section>
        </div>
    }
    .into_any()
}

fn render_history_details(row: BlockPlayoutHistoryRow) -> AnyView {
    view! {
        <dl class=style::detailsGrid>
            <dt>{tr(TextKey::PlaybackOrder)}</dt>
            <dd>{row.playback_order}</dd>
            <dt>{tr(TextKey::CollectionType)}</dt>
            <dd>{row.collection_type}</dd>
            <dt>{tr(TextKey::CollectionName)}</dt>
            <dd>{row.collection_name}</dd>
            <dt>{tr(TextKey::MediaItemType)}</dt>
            <dd>{row.media_item_type}</dd>
            <dt>{tr(TextKey::MediaItemTitle)}</dt>
            <dd>{row.media_item_title}</dd>
        </dl>
    }
    .into_any()
}

fn report_block(content: String, include_copy: bool) -> impl IntoView {
    let copy_content = content.clone();

    view! {
        <div class=style::report>
            <pre class=style::pre>
                <code>{content}</code>
            </pre>
            {include_copy.then(|| view! {
                <button
                    type="button"
                    class=style::copyButton
                    on:click=move |_| copy_to_clipboard(copy_content.clone())
                >
                    {tr(TextKey::Copy)}
                </button>
            })}
        </div>
    }
}

#[cfg(target_arch = "wasm32")]
fn copy_to_clipboard(content: String) {
    if let Some(window) = web_sys::window() {
        let _ = window.navigator().clipboard().write_text(&content);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn copy_to_clipboard(_content: String) {}

#[cfg(all(test, feature = "ssr"))]
mod default_engine_tests {
    use super::*;

    #[test]
    fn playback_ssr_selects_resolved_engine_without_hiding_the_native_option() {
        for engine in [
            ersatzrs_domain::StreamingEngine::Legacy,
            ersatzrs_domain::StreamingEngine::Next,
        ] {
            let owner = Owner::new();
            let html = owner.with(|| {
                render_playback_form(
                    PlaybackTroubleshootingContext {
                        mode: PlaybackTroubleshootingMode::MediaItem,
                        streaming_engine: engine,
                        ..PlaybackTroubleshootingContext::default()
                    },
                    "0".to_owned(),
                    None,
                )
                .to_html()
            });
            let native = html
                .split("value=\"Legacy\"")
                .nth(1)
                .unwrap()
                .split('>')
                .next()
                .unwrap();
            let next = html
                .split("value=\"Next\"")
                .nth(1)
                .unwrap()
                .split('>')
                .next()
                .unwrap();
            assert_eq!(
                native.contains("selected"),
                engine == ersatzrs_domain::StreamingEngine::Legacy
            );
            assert_eq!(
                next.contains("selected"),
                engine == ersatzrs_domain::StreamingEngine::Next
            );
        }
    }
}
