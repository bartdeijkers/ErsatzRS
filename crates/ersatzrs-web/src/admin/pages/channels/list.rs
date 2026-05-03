#![allow(clippy::too_many_lines)]
use leptos::prelude::*;

use crate::admin::server_fns::channels::{
    count_channels, delete_channel, list_channel_rows, ChannelListRow,
};
use crate::admin::ui::primitives::{
    dialog::DeleteDialog,
    icon::{Icon, IconName},
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
        Column::new("Number", |ch: &ChannelListRow| {
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
                "Channel disabled"
            } else if !show_in_epg {
                "Channel hidden from EPG"
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
        Column::new("Logo", |ch: &ChannelListRow| {
            let name = ch.name.clone();
            let encoded = urlencoding::encode(&name).into_owned();
            view! {
                <img
                    src=format!("/iptv/logos/gen?text={encoded}")
                    class=style::logo_img
                    alt=format!("{name} logo")
                />
            }
            .into_any()
        }),
        Column::new("Name", |ch: &ChannelListRow| {
            let n = ch.name.clone();
            view! { <span>{n}</span> }.into_any()
        }),
        Column::new("Language", |ch: &ChannelListRow| {
            let language = ch.preferred_audio_language.clone();
            view! { <span>{language}</span> }.into_any()
        }),
        Column::new("Mode", |ch: &ChannelListRow| {
            let m = ch.streaming_mode_label.clone();
            view! { <span>{m}</span> }.into_any()
        }),
        Column::new("FFmpeg Profile", |ch: &ChannelListRow| {
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
                    title="Preview channel"
                    aria-label="Preview channel"
                >
                    <Icon name=IconName::Play size="1.25rem"/>
                </a>
                <a
                    href=format!("/channels/{id}")
                    class=style::row_btn_icon
                    title="Edit channel"
                    aria-label="Edit channel"
                >
                    <Icon name=IconName::Edit size="1.25rem"/>
                </a>
                <span
                    class=format!("{} {}", style::row_btn_icon, style::row_btn_disabled)
                    title="Troubleshoot channel"
                    aria-label="Troubleshoot channel"
                    aria-disabled="true"
                >
                    <Icon name=IconName::Search size="1.25rem"/>
                </span>
                <button
                    class=style::row_btn_icon
                    title="Delete channel"
                    aria-label="Delete channel"
                    on:click=move |_| {
                        delete_target.set(Some(ch_clone.clone()));
                        delete_open.set(true);
                    }
                >
                    <Icon name=IconName::Delete size="1.25rem"/>
                </button>
            </div>
        }
        .into_any()
    });

    view! {
        <div class=style::page>
            <div class=style::action_bar>
                <a href="/channels/add" class=style::btn_add>
                    <Icon name=IconName::Add size="1.125rem"/>
                    "Add Channel"
                </a>
                <a href="/channels/numbers" class=style::btn_numbers>
                    <Icon name=IconName::Edit size="1.125rem"/>
                    "Edit Numbers"
                </a>
            </div>

            <section class=style::content>
                <h1 class=style::page_title>"Channels"</h1>
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
                            <span>"Show Disabled"</span>
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

            {move || {
                delete_target.get().map(|ch| {
                    let name = ch.name.clone();
                    let id = ch.id;
                    view! {
                        <DeleteDialog
                            open=delete_open
                            entity_name=name
                            on_confirm=Callback::new(move |confirmed: bool| {
                                if confirmed {
                                    delete_action.dispatch(id);
                                } else {
                                    delete_open.set(false);
                                }
                            })
                        />
                    }
                })
            }}
        </div>
    }
}
