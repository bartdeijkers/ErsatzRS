use leptos::prelude::*;
use leptos_router::hooks::use_query_map;

use crate::admin::i18n::{tr, unable_to_load_playback_troubleshooting, TextKey};
use crate::admin::server_fns::troubleshooting::{
    list_block_playout_history, list_block_playout_options, list_blocks_for_playout,
    load_playback_troubleshooting_context, load_troubleshooting_report,
    validate_sequential_schedule_file, BlockPlayoutHistoryRow, BlockPlayoutOption,
    PlaybackTroubleshootingContext, PlaybackTroubleshootingMode, TroubleshootingBlockRow,
    TroubleshootingReport, YamlValidationResult,
};

stylance::import_crate_style!(style, "src/admin/pages/system/troubleshooting.module.css");

#[component]
pub fn TroubleshootingPage() -> impl IntoView {
    let report = Resource::new_blocking(
        || (),
        |()| async move { load_troubleshooting_report().await.unwrap_or_default() },
    );

    view! {
        <div class=style::page>
            <header>
                <h1 class=style::heading>
                    {tr(TextKey::Troubleshooting)}
                </h1>
                <div class=style::divider></div>
            </header>

            <Suspense fallback=move || view! { <p>{tr(TextKey::Loading)}</p> }>
                {move || report.get().map(render_report)}
            </Suspense>
        </div>
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

    let context: Resource<Result<PlaybackTroubleshootingContext, ServerFnError>> =
        Resource::new_blocking(
            move || (media_item_id, channel_id),
            |(media_item, channel)| async move {
                load_playback_troubleshooting_context(media_item, channel).await
            },
        );

    view! {
        <Suspense fallback=move || view! { <p>{tr(TextKey::LoadingMediaInfo)}</p> }>
            {move || match context.get() {
                Some(Ok(context)) => render_playback_troubleshooting(context),
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
    }
}

fn render_playback_troubleshooting(context: PlaybackTroubleshootingContext) -> AnyView {
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
                    render_playback_form(context, default_seek)
                }}
            </div>
        </div>
    }
    .into_any()
}

fn render_playback_form(context: PlaybackTroubleshootingContext, default_seek: String) -> AnyView {
    let is_media_item = context.mode == PlaybackTroubleshootingMode::MediaItem;
    let stream_selectors_disabled = context.stream_selectors.is_empty();

    view! {
        <form method="get" action="/api/troubleshoot/playback.m3u8" class=style::playbackForm>
            <input type="hidden" name="mediaItem" value=context.media_item_id.to_string()/>
            <input type="hidden" name="channel" value=context.channel_id.to_string()/>
            <input type="hidden" name="streamingMode" value="1"/>

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
                            <option value="">{tr(TextKey::DefaultOption)}</option>
                            {context.subtitles.into_iter().map(|subtitle| {
                                let label = format!(
                                    "{}: {} - {} ({})",
                                    subtitle.id, subtitle.language, subtitle.title, subtitle.codec
                                );
                                view! { <option value=subtitle.id.to_string()>{label}</option> }
                            }).collect_view()}
                        </select>
                    </label>
                    {playback_multi_select_field(tr(TextKey::Watermarks), "watermark", context.watermarks)}
                    {playback_multi_select_field(tr(TextKey::GraphicsElements), "graphicsElement", context.graphics_elements)}
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
                view! {
                    <label class=style::playbackField>
                        <span>{tr(TextKey::DateAndTime)}</span>
                        <input class=style::input type="datetime-local" name="start"/>
                    </label>
                }.into_any()
            }}

            <section class=style::wideSection>
                <h2 class=style::subheading>{tr(TextKey::Preview)}</h2>
                <div class=style::divider></div>
                <button class=style::primaryButton type="submit">
                    {tr(TextKey::Play)}
                </button>
                <div class=style::previewFrame>
                    <video controls></video>
                    <div class=style::previewSpacer></div>
                </div>
            </section>

            <section class=style::wideSection>
                <h2 class=style::subheading>{tr(TextKey::Logs)}</h2>
                <div class=style::divider></div>
                <textarea class=style::logs readonly rows="20"></textarea>
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
                YamlValidationResult::default()
            } else {
                validate_sequential_schedule_file(path, import)
                    .await
                    .unwrap_or_else(|error| YamlValidationResult {
                        exists: true,
                        messages: vec![error.to_string()],
                        ..YamlValidationResult::default()
                    })
            }
        },
    );

    view! {
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

            <Suspense fallback=move || view! { <p>{tr(TextKey::Loading)}</p> }>
                {move || result.get().map(render_yaml_validation)}
            </Suspense>
        </div>
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

    let options = Resource::new_blocking(
        || (),
        |()| async move { list_block_playout_options().await.unwrap_or_default() },
    );
    let blocks = Resource::new_blocking(selected_playout, |playout_id| async move {
        list_blocks_for_playout(playout_id)
            .await
            .unwrap_or_default()
    });

    view! {
        <div class=style::page>
            <header>
                <h1 class=style::heading>{tr(TextKey::BlockPlayoutTroubleshooting)}</h1>
                <div class=style::divider></div>
            </header>

            <Suspense fallback=move || view! { <p>{tr(TextKey::Loading)}</p> }>
                {move || options.get().map(|items| {
                    let search = search();
                    render_block_playout_selector(items, selected_playout(), &search)
                })}
            </Suspense>

            <section>
                <h2 class=style::subheading>{tr(TextKey::Blocks)}</h2>
                <Suspense fallback=move || view! { <p>{tr(TextKey::Loading)}</p> }>
                    {move || blocks.get().map(|items| {
                        let search = search();
                        render_blocks_table(items, selected_playout(), &search)
                    })}
                </Suspense>
            </section>
        </div>
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
        |(playout, block)| async move {
            list_block_playout_history(playout, block)
                .await
                .unwrap_or_default()
        },
    );

    view! {
        <div class=style::page>
            <header>
                <h1 class=style::heading>{tr(TextKey::BlockPlayoutHistory)}</h1>
                <div class=style::divider></div>
            </header>

            <Suspense fallback=move || view! { <p>{tr(TextKey::Loading)}</p> }>
                {move || history.get().map(|items| render_history(items, playout_id(), block_id(), history_id()))}
            </Suspense>
        </div>
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
