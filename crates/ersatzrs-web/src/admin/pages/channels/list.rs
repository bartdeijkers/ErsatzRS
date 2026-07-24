#![allow(clippy::too_many_lines)]
use leptos::prelude::*;

use crate::admin::i18n::{channel_language_label, channel_streaming_mode_label, tr, TextKey};
#[cfg(target_arch = "wasm32")]
use crate::admin::server_fns::channels::list_active_channel_numbers;
use crate::admin::server_fns::channels::{
    count_channels, delete_channel, list_channel_rows, stop_channel_session, ChannelListRow,
};
use crate::admin::ui::primitives::{
    dialog::DeleteDialog,
    table::{Column, PageParams, PagedTable, RowActionsRenderer},
    toast::{load_or_notify, use_toast},
};

stylance::import_crate_style!(style, "src/admin/pages/channels/list.module.css");

const PAGE_SIZE: u32 = 10;

#[component]
pub fn ChannelsListPage() -> impl IntoView {
    view! { <ChannelsListIsland/> }
}

#[island]
fn ChannelsListIsland() -> impl IntoView {
    let show_disabled = RwSignal::new(false);
    let params = RwSignal::new(PageParams::new(PAGE_SIZE));
    let load_toast = use_toast();

    let rows_resource: LocalResource<Vec<ChannelListRow>> = LocalResource::new(move || {
        let disabled = show_disabled.get();
        let p = params.get();
        async move {
            load_or_notify(
                list_channel_rows(disabled, p.page, p.page_size).await,
                load_toast,
            )
        }
    });

    let count_resource: LocalResource<u64> = LocalResource::new(move || {
        let disabled = show_disabled.get();
        async move { load_or_notify(count_channels(disabled).await, load_toast) }
    });

    let total = Signal::derive(move || count_resource.get());

    let delete_action = Action::new(|id: &i64| delete_channel(*id));
    let delete_target: RwSignal<Option<ChannelListRow>> = RwSignal::new(None);
    let delete_open = RwSignal::new(false);

    // Channel numbers with a live transcode session. Mirrors ErsatzTV's
    // `IFFmpegSegmenterService.IsActive` check in `Channels.razor`; ErsatzTV
    // re-renders on the `OnWorkersChanged` event, so we poll the registry the
    // same way the libraries page polls the entity locker (client-side, no
    // SignalR). The set drives the per-row stop-button visibility.
    let active_numbers: RwSignal<Vec<String>> = RwSignal::new(Vec::new());
    #[cfg(target_arch = "wasm32")]
    let active_load_failed = RwSignal::new(false);
    Effect::new(move |_| {
        #[cfg(target_arch = "wasm32")]
        {
            leptos::task::spawn_local(async move {
                loop {
                    match list_active_channel_numbers().await {
                        Ok(numbers) => {
                            active_numbers.set(numbers);
                            active_load_failed.set(false);
                        }
                        Err(error) if !active_load_failed.get_untracked() => {
                            active_load_failed.set(true);
                            load_toast.push(
                                format!("{}: {error}", tr(TextKey::ErrorOccurred)),
                                crate::admin::ui::primitives::toast::ToastSeverity::Error,
                            );
                        }
                        Err(_) => {}
                    }
                    gloo_timers::future::TimeoutFuture::new(1_500).await;
                }
            });
        }
    });

    // Stop the active transcode session for a channel number. The worker tears
    // itself down asynchronously and removes itself from the registry, so the
    // stop button disappears on the next poll tick — matching ErsatzTV, where
    // the icon clears on the `OnWorkersChanged` event after teardown.
    let stop_action = Action::new(|number: &String| {
        let number = number.clone();
        async move { stop_channel_session(number).await }
    });

    Effect::new(move |_| {
        if matches!(delete_action.value().get(), Some(Ok(()))) {
            rows_resource.refetch();
            count_resource.refetch();
            delete_open.set(false);
        }
    });

    let columns: Vec<Column<ChannelListRow>> = vec![
        Column::new(tr(TextKey::Number), |ch: &ChannelListRow| {
            let n = ch.number.clone();
            let enabled = ch.is_enabled;
            let show_in_epg = ch.show_in_epg;
            let label = if !enabled {
                format!("(D) {n}")
            } else if !show_in_epg {
                format!("(H) {n}")
            } else {
                n
            };
            let title = if !enabled {
                tr(TextKey::ChannelDisabled)
            } else if !show_in_epg {
                tr(TextKey::ChannelHiddenFromEpg)
            } else {
                ""
            };
            view! {
                <span
                    class=if enabled { "" } else { style::cell_disabled }
                    title=title
                >
                    {label}
                </span>
            }
            .into_any()
        }),
        Column::new(tr(TextKey::Logo), |ch: &ChannelListRow| {
            let url = ch.logo_url.clone();
            let name = ch.name.clone();
            view! {
                <img
                    src=url
                    class=style::logo_img
                    alt=format!("{name} {}", tr(TextKey::Logo))
                />
            }
            .into_any()
        }),
        Column::new(tr(TextKey::Name), |ch: &ChannelListRow| {
            let n = ch.name.clone();
            view! { <span>{n}</span> }.into_any()
        }),
        Column::new(tr(TextKey::Language), |ch: &ChannelListRow| {
            let language = channel_language_label(&ch.preferred_audio_language_code);
            view! { <span>{language}</span> }.into_any()
        }),
        Column::new(tr(TextKey::Mode), |ch: &ChannelListRow| {
            let m = channel_streaming_mode_label(ch.streaming_mode);
            view! { <span>{m}</span> }.into_any()
        }),
        Column::new(tr(TextKey::FfmpegProfile), |ch: &ChannelListRow| {
            let p = ch.ffmpeg_profile_name.clone();
            view! { <span>{p}</span> }.into_any()
        }),
    ];

    let row_actions: RowActionsRenderer<ChannelListRow> = Box::new(move |ch| {
        let id = ch.id;
        let number = ch.number.clone();
        let ch_clone = ch.clone();
        let stop_number = number.clone();
        let active_number = number.clone();
        let is_active =
            move || active_numbers.with(|nums| nums.iter().any(|n| n == &active_number));
        let can_troubleshoot = can_troubleshoot_channel(ch);
        let troubleshoot_href = if can_troubleshoot {
            format!("/system/troubleshooting/playback?channel={id}")
        } else {
            String::new()
        };
        view! {
            <div class=style::row_actions>
                <a
                    href=format!("/iptv/channel/{number}.m3u8")
                    target="_blank"
                    class=style::row_btn_preview
                    title=tr(TextKey::PreviewChannel)
                    aria-label=tr(TextKey::PreviewChannel)
                ></a>
                <button
                    class=format!("{} {}", style::row_btn_icon, style::row_btn_stop)
                    style:visibility=move || if is_active() { "visible" } else { "hidden" }
                    title=tr(TextKey::StopTranscodeSession)
                    aria-label=tr(TextKey::StopTranscodeSession)
                    on:click=move |_| {
                        stop_action.dispatch(stop_number.clone());
                    }
                ></button>
                <a
                    href=format!("/channels/{id}")
                    class=format!("{} {}", style::row_btn_icon, style::row_btn_edit)
                    title=tr(TextKey::EditChannel)
                    aria-label=tr(TextKey::EditChannel)
                ></a>
                <a
                    href=troubleshoot_href
                    class=format!("{} {}", style::row_btn_icon, style::row_btn_search)
                    style:visibility=if can_troubleshoot { "visible" } else { "hidden" }
                    title=tr(TextKey::TroubleshootChannel)
                    aria-label=tr(TextKey::TroubleshootChannel)
                    aria-hidden=if can_troubleshoot { "false" } else { "true" }
                    tabindex=if can_troubleshoot { "0" } else { "-1" }
                ></a>
                <button
                    class=format!("{} {}", style::row_btn_icon, style::row_btn_delete)
                    title=tr(TextKey::DeleteChannel)
                    aria-label=tr(TextKey::DeleteChannel)
                    on:click=move |_| {
                        delete_target.set(Some(ch_clone.clone()));
                        delete_open.set(true);
                    }
                ></button>
            </div>
        }
        .into_any()
    });

    view! {
        <div class=style::page>
            <div class=style::action_bar>
                <a href="/channels/add" class=style::btn_add>
                    {tr(TextKey::AddChannel)}
                </a>
                <a href="/channels/numbers" class=style::btn_numbers>
                    {tr(TextKey::EditNumbers)}
                </a>
            </div>

            <section class=style::content>
                <h1 class=style::page_title>{tr(TextKey::Channels)}</h1>
                <div class=style::divider/>

                <div class=style::table_panel>
                    <div class=style::table_toolbar>
                        <label class=style::switch_label>
                            <input
                                type="checkbox"
                                class=style::switch_input
                                prop:checked=move || show_disabled.get()
                                on:change=move |e| show_disabled.set(event_target_checked(&e))
                            />
                            <span class=style::switch_track>
                                <span class=style::switch_thumb/>
                            </span>
                            <span>{tr(TextKey::ShowDisabled)}</span>
                        </label>
                    </div>

                    <PagedTable
                        rows=rows_resource
                        total=total
                        params=params
                        columns=columns
                        row_actions=Some(row_actions)
                    />
                </div>
            </section>

            // DeleteDialog is always rendered; visibility is controlled via `open`.
            // A stable DOM shape is required to avoid failed_to_cast_marker_node on hydration.
            <DeleteDialog
                open=delete_open
                entity_name=Signal::derive(move || {
                    delete_target.get().map(|ch| ch.name.clone()).unwrap_or_default()
                })
                on_confirm=Callback::new(move |confirmed: bool| {
                    if let Some(ch) = delete_target.get_untracked() {
                        if confirmed {
                            delete_action.dispatch(ch.id);
                        } else {
                            delete_open.set(false);
                        }
                    } else {
                        delete_open.set(false);
                    }
                })
            />
        </div>
    }
}

fn can_troubleshoot_channel(channel: &ChannelListRow) -> bool {
    channel.is_enabled && channel.playout_count > 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(is_enabled: bool, playout_count: i64) -> ChannelListRow {
        ChannelListRow {
            is_enabled,
            playout_count,
            ..ChannelListRow::default()
        }
    }

    #[test]
    fn troubleshoot_action_matches_ersatztv_visibility_rule() {
        assert!(can_troubleshoot_channel(&row(true, 1)));
        assert!(!can_troubleshoot_channel(&row(true, 0)));
        assert!(!can_troubleshoot_channel(&row(false, 1)));
    }
}
