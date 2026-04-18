#![allow(clippy::too_many_lines)]
use leptos::prelude::*;

use crate::server::channels::{count_channels, delete_channel, list_channel_rows, ChannelListRow};
use crate::ui::primitives::{
    dialog::DeleteDialog,
    table::{Column, PageParams, PagedTable, RowActionsRenderer},
};

stylance::import_crate_style!(style, "src/pages/channels/list.module.css");

const PAGE_SIZE: u32 = 25;

#[component]
pub fn ChannelsListPage() -> impl IntoView {
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

    let total = Signal::derive(move || count_resource.get().unwrap_or(0));

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
            view! {
                <span class=if enabled { "" } else { style::cell_disabled }>{n}</span>
            }
            .into_any()
        }),
        Column::new("Name", |ch: &ChannelListRow| {
            let n = ch.name.clone();
            view! { <span>{n}</span> }.into_any()
        }),
        Column::new("Streaming Mode", |ch: &ChannelListRow| {
            let m = ch.streaming_mode_label.clone();
            view! { <span class=style::cell_muted>{m}</span> }.into_any()
        }),
        Column::new("FFmpeg Profile", |ch: &ChannelListRow| {
            let p = ch.ffmpeg_profile_name.clone();
            view! { <span class=style::cell_muted>{p}</span> }.into_any()
        }),
    ];

    let row_actions: RowActionsRenderer<ChannelListRow> = Box::new(move |ch| {
        let id = ch.id;
        let ch_clone = ch.clone();
        view! {
            <div class=style::row_actions>
                <a href=format!("/channels/{id}") class=style::row_btn_edit>"Edit"</a>
                <button
                    class=style::row_btn_delete
                    on:click=move |_| {
                        delete_target.set(Some(ch_clone.clone()));
                        delete_open.set(true);
                    }
                >
                    "Delete"
                </button>
            </div>
        }
        .into_any()
    });

    view! {
        <div class=style::page>
            <div class=style::toolbar>
                <h1 class=style::page_title>"Channels"</h1>
                <div class=style::toolbar_actions>
                    <label class=style::show_disabled_label>
                        <input
                            type="checkbox"
                            class=style::show_disabled_check
                            prop:checked=move || show_disabled.get()
                            on:change=move |e| show_disabled.set(event_target_checked(&e))
                        />
                        "Show disabled"
                    </label>
                    <a href="/channels/add" class=style::btn_add>"+ Add Channel"</a>
                    <a href="/channels/numbers" class=style::btn_numbers>"Edit Numbers"</a>
                </div>
            </div>

            <PagedTable
                rows=rows_resource
                total=total
                params=params
                columns=columns
                row_actions=Some(row_actions)
            />

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
