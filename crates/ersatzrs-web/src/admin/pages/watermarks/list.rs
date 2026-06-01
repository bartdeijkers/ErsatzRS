#![allow(clippy::too_many_lines)]
use leptos::prelude::*;

use crate::admin::i18n::{tr, watermark_error_message, TextKey};
use crate::admin::server_fns::watermarks::{
    copy_watermark, count_watermarks, delete_watermark, list_watermark_rows, WatermarkListRow,
};
use crate::admin::ui::primitives::{
    dialog::{DeleteDialog, Modal},
    form::TextField,
    table::{Column, PageParams, PagedTable, RowActionsRenderer},
    toast::{use_toast, ToastSeverity},
};

stylance::import_crate_style!(style, "src/admin/pages/watermarks/list.module.css");

const PAGE_SIZE: u32 = 25;

#[cfg(target_arch = "wasm32")]
fn set_browser_location(path: &str) {
    if let Some(window) = web_sys::window() {
        let _ = window.location().set_href(path);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn set_browser_location(_path: &str) {}

#[component]
pub fn WatermarksListPage() -> impl IntoView {
    view! { <WatermarksListIsland/> }
}

#[island]
fn WatermarksListIsland() -> impl IntoView {
    let params = RwSignal::new(PageParams::new(PAGE_SIZE));

    let rows_resource: Resource<Vec<WatermarkListRow>> = Resource::new(
        move || params.get(),
        |p| async move {
            list_watermark_rows(p.page, p.page_size)
                .await
                .unwrap_or_default()
        },
    );

    let count_resource: Resource<u64> = Resource::new(
        || (),
        |()| async move { count_watermarks().await.unwrap_or(0) },
    );

    let total = Signal::derive(move || count_resource.get());

    let delete_action = Action::new(|id: &i64| delete_watermark(*id));
    let delete_target: RwSignal<Option<WatermarkListRow>> = RwSignal::new(None);
    let delete_open = RwSignal::new(false);

    Effect::new(move |_| {
        if matches!(delete_action.value().get(), Some(Ok(()))) {
            rows_resource.refetch();
            count_resource.refetch();
            delete_open.set(false);
        }
    });

    // Copy watermark state
    let copy_action = Action::new(|(id, name): &(i64, String)| {
        let id = *id;
        let name = name.clone();
        async move { copy_watermark(id, name).await }
    });
    let copy_source: RwSignal<Option<WatermarkListRow>> = RwSignal::new(None);
    let copy_open = RwSignal::new(false);
    let copy_name = RwSignal::new(String::new());
    let toast = use_toast();

    Effect::new(move |_| {
        if let Some(res) = copy_action.value().get() {
            match res {
                Ok(new_id) => {
                    toast.push(
                        tr(TextKey::WatermarkCopied).to_string(),
                        ToastSeverity::Success,
                    );
                    copy_open.set(false);
                    set_browser_location(&format!("/watermarks/{new_id}"));
                }
                Err(e) => toast.push(
                    format!(
                        "{}: {}",
                        tr(TextKey::CopyFailed),
                        watermark_error_message(e)
                    ),
                    ToastSeverity::Error,
                ),
            }
        }
    });

    let columns: Vec<Column<WatermarkListRow>> = vec![
        Column::new(tr(TextKey::Name), |r: &WatermarkListRow| {
            let n = r.name.clone();
            view! { <span>{n}</span> }.into_any()
        }),
        Column::new(tr(TextKey::Image), |r: &WatermarkListRow| {
            if let Some(path) = r.image_path.clone() {
                view! {
                    <img
                        src=format!("/artwork/watermarks/{path}")
                        class=style::thumb
                        alt=tr(TextKey::WatermarkPreview)
                    />
                }
                .into_any()
            } else if r.image_source == 1 {
                view! { <span class=style::cell_muted>{tr(TextKey::ChannelLogo)}</span> }.into_any()
            } else {
                view! { <span/> }.into_any()
            }
        }),
        Column::new(tr(TextKey::Mode), |r: &WatermarkListRow| {
            let m = watermark_mode_label(r.mode_id);
            view! { <span class=style::cell_muted>{m}</span> }.into_any()
        }),
        Column::new(tr(TextKey::Location), |r: &WatermarkListRow| {
            let l = watermark_location_label(r.location_id);
            view! { <span class=style::cell_muted>{l}</span> }.into_any()
        }),
    ];

    let row_actions: RowActionsRenderer<WatermarkListRow> = Box::new(move |r| {
        let id = r.id;
        let r_for_delete = r.clone();
        let r_for_copy = r.clone();
        view! {
            <div class=style::row_actions>
                <a
                    href=format!("/watermarks/{id}")
                    class=format!("{} {}", style::row_btn_icon, style::row_btn_edit)
                    title=tr(TextKey::EditWatermark).to_string()
                    aria-label=tr(TextKey::EditWatermark).to_string()
                ></a>
                <button
                    class=format!("{} {}", style::row_btn_icon, style::row_btn_copy)
                    title=tr(TextKey::CopyWatermark).to_string()
                    aria-label=tr(TextKey::CopyWatermark).to_string()
                    on:click=move |_| {
                        copy_source.set(Some(r_for_copy.clone()));
                        copy_name.set(format!("{} {}", tr(TextKey::CopyOf), r_for_copy.name));
                        copy_open.set(true);
                    }
                ></button>
                <button
                    class=format!("{} {}", style::row_btn_icon, style::row_btn_delete)
                    title=tr(TextKey::DeleteWatermark).to_string()
                    aria-label=tr(TextKey::DeleteWatermark).to_string()
                    on:click=move |_| {
                        delete_target.set(Some(r_for_delete.clone()));
                        delete_open.set(true);
                    }
                ></button>
            </div>
        }
        .into_any()
    });

    let delete_name = Signal::derive(move || {
        delete_target
            .get()
            .map(|r| r.name.clone())
            .unwrap_or_default()
    });

    view! {
        <div class=style::page>
            <div class=style::action_bar>
                <a href="/watermarks/add" class=style::btn_add>
                    {tr(TextKey::AddWatermark)}
                </a>
            </div>

            <section class=style::content>
                <h1 class=style::page_title>{tr(TextKey::Watermarks)}</h1>
                <div class=style::divider/>

                <PagedTable
                    rows=rows_resource
                    columns=columns
                    row_actions=Some(row_actions)
                    params=params
                    total=total
                />
            </section>

            <DeleteDialog
                open=delete_open
                entity_name=delete_name
                on_confirm=Callback::new(move |confirmed: bool| {
                    if confirmed {
                        if let Some(r) = delete_target.get_untracked() {
                            delete_action.dispatch(r.id);
                        }
                    } else {
                        delete_open.set(false);
                    }
                })
            />

            <Modal
                open=copy_open
                title=tr(TextKey::CopyWatermark).to_string()
                on_close=Callback::new(move |confirmed: bool| {
                    if confirmed {
                        if let Some(src) = copy_source.get_untracked() {
                            copy_action.dispatch((src.id, copy_name.get_untracked()));
                        }
                    } else {
                        copy_open.set(false);
                    }
                })
            >
                <TextField label=tr(TextKey::NewName) value=copy_name/>
            </Modal>
        </div>
    }
}

fn watermark_mode_label(mode: i32) -> &'static str {
    match mode {
        0 => tr(TextKey::NoneOption),
        1 => tr(TextKey::Permanent),
        2 => tr(TextKey::Intermittent),
        3 => tr(TextKey::OpacityExpression),
        _ => tr(TextKey::Unavailable),
    }
}

fn watermark_location_label(location: i32) -> &'static str {
    match location {
        0 => tr(TextKey::BottomRight),
        1 => tr(TextKey::BottomLeft),
        2 => tr(TextKey::TopRight),
        3 => tr(TextKey::TopLeft),
        4 => tr(TextKey::TopMiddle),
        5 => tr(TextKey::RightMiddle),
        6 => tr(TextKey::BottomMiddle),
        7 => tr(TextKey::LeftMiddle),
        8 => tr(TextKey::MiddleCenter),
        _ => tr(TextKey::Unavailable),
    }
}
