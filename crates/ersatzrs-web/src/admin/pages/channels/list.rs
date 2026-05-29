#![allow(clippy::too_many_lines)]
use leptos::prelude::*;

use crate::admin::i18n::{tr, TextKey};
use crate::admin::server_fns::channels::{
    count_channels, delete_channel, list_channel_rows, ChannelListRow,
};
use crate::admin::ui::primitives::{
    dialog::DeleteDialog,
    table::{Column, PageParams, PagedTable, RowActionsRenderer},
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

    let rows_resource: Resource<Vec<ChannelListRow>> = Resource::new(
        move || (show_disabled.get(), params.get()),
        |(disabled, p)| async move {
            list_channel_rows(disabled, p.page, p.page_size)
                .await
                .unwrap_or_default()
        },
    );

    let count_resource: Resource<u64> = Resource::new(
        move || show_disabled.get(),
        |disabled| async move { count_channels(disabled).await.unwrap_or(0) },
    );

    let total = Signal::derive(move || count_resource.get());

    let delete_action = Action::new(|id: &i64| delete_channel(*id));
    let delete_target: RwSignal<Option<ChannelListRow>> = RwSignal::new(None);
    let delete_open = RwSignal::new(false);

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
            let language = ch.preferred_audio_language.clone();
            view! { <span>{language}</span> }.into_any()
        }),
        Column::new(tr(TextKey::Mode), |ch: &ChannelListRow| {
            let m = ch.streaming_mode_label.clone();
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
        view! {
            <div class=style::row_actions>
                <a
                    href=format!("/iptv/channel/{number}.m3u8")
                    target="_blank"
                    class=style::row_btn_preview
                    title=tr(TextKey::PreviewChannel)
                    aria-label=tr(TextKey::PreviewChannel)
                ></a>
                <a
                    href=format!("/channels/{id}")
                    class=format!("{} {}", style::row_btn_icon, style::row_btn_edit)
                    title=tr(TextKey::EditChannel)
                    aria-label=tr(TextKey::EditChannel)
                ></a>
                <span
                    class=format!("{} {} {}", style::row_btn_icon, style::row_btn_search, style::row_btn_disabled)
                    title=tr(TextKey::TroubleshootChannel)
                    aria-label=tr(TextKey::TroubleshootChannel)
                    aria-disabled="true"
                ></span>
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
