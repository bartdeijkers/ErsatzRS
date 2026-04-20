#![allow(clippy::too_many_lines)]
use leptos::prelude::*;

use crate::server::watermarks::{
    copy_watermark, count_watermarks, delete_watermark, list_watermark_rows, WatermarkListRow,
};
use crate::ui::primitives::{
    dialog::{DeleteDialog, Modal},
    form::TextField,
    table::{Column, PageParams, PagedTable, RowActionsRenderer},
    toast::{use_toast, ToastSeverity},
};

stylance::import_crate_style!(style, "src/pages/watermarks/list.module.css");

const PAGE_SIZE: u32 = 25;

#[component]
pub fn WatermarksListPage() -> impl IntoView {
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
    let navigate = leptos_router::hooks::use_navigate();

    Effect::new(move |_| {
        if let Some(res) = copy_action.value().get() {
            match res {
                Ok(new_id) => {
                    toast.push("Watermark copied.".to_string(), ToastSeverity::Success);
                    copy_open.set(false);
                    navigate(
                        &format!("/watermarks/{new_id}"),
                        leptos_router::NavigateOptions::default(),
                    );
                }
                Err(e) => toast.push(format!("Copy failed: {e}"), ToastSeverity::Error),
            }
        }
    });

    let columns: Vec<Column<WatermarkListRow>> = vec![
        Column::new("Name", |r: &WatermarkListRow| {
            let n = r.name.clone();
            view! { <span>{n}</span> }.into_any()
        }),
        Column::new("Image", |r: &WatermarkListRow| {
            if let Some(path) = r.image_path.clone() {
                view! {
                    <img
                        src=format!("/artwork/watermarks/{path}")
                        class=style::thumb
                        alt="watermark"
                    />
                }
                .into_any()
            } else if r.image_source == 1 {
                view! { <span class=style::cell_muted>"[channel logo]"</span> }.into_any()
            } else {
                view! { <span/> }.into_any()
            }
        }),
        Column::new("Mode", |r: &WatermarkListRow| {
            let m = r.mode.clone();
            view! { <span class=style::cell_muted>{m}</span> }.into_any()
        }),
        Column::new("Location", |r: &WatermarkListRow| {
            let l = r.location.clone();
            view! { <span class=style::cell_muted>{l}</span> }.into_any()
        }),
    ];

    let row_actions: RowActionsRenderer<WatermarkListRow> = Box::new(move |r| {
        let id = r.id;
        let r_for_delete = r.clone();
        let r_for_copy = r.clone();
        view! {
            <div class=style::row_actions>
                <a href=format!("/watermarks/{id}") class=style::row_btn_edit>"Edit"</a>
                <button
                    class=style::row_btn_copy
                    on:click=move |_| {
                        copy_source.set(Some(r_for_copy.clone()));
                        copy_name.set(format!("Copy of {}", r_for_copy.name));
                        copy_open.set(true);
                    }
                >
                    "Copy"
                </button>
                <button
                    class=style::row_btn_delete
                    on:click=move |_| {
                        delete_target.set(Some(r_for_delete.clone()));
                        delete_open.set(true);
                    }
                >
                    "Delete"
                </button>
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
            <div class=style::toolbar>
                <h1 class=style::page_title>"Watermarks"</h1>
                <div class=style::toolbar_actions>
                    <a href="/watermarks/add" class=style::btn_add>"Add Watermark"</a>
                </div>
            </div>

            <PagedTable
                rows=rows_resource
                columns=columns
                row_actions=Some(row_actions)
                params=params
                total=total
            />

            <DeleteDialog
                open=delete_open
                entity_name=Signal::derive(move || delete_name.get()).get()
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
                title="Copy Watermark".to_string()
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
                <TextField label="New Name" value=copy_name/>
            </Modal>
        </div>
    }
}
