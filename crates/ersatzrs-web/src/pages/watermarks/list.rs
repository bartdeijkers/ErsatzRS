use leptos::prelude::*;

use crate::server::watermarks::{
    count_watermarks, delete_watermark, list_watermark_rows, WatermarkListRow,
};
use crate::ui::primitives::{
    dialog::DeleteDialog,
    table::{Column, PageParams, PagedTable, RowActionsRenderer},
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

    let total = Signal::derive(move || count_resource.get().unwrap_or(0));

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

    let columns: Vec<Column<WatermarkListRow>> = vec![
        Column::new("Name", |r: &WatermarkListRow| {
            let n = r.name.clone();
            view! { <span>{n}</span> }.into_any()
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
        let r_clone = r.clone();
        view! {
            <div class=style::row_actions>
                <a href=format!("/watermarks/{id}") class=style::row_btn_edit>"Edit"</a>
                <button
                    class=style::row_btn_delete
                    on:click=move |_| {
                        delete_target.set(Some(r_clone.clone()));
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
        </div>
    }
}
