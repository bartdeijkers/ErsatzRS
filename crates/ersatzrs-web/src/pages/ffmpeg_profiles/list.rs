use leptos::prelude::*;

use crate::server::ffmpeg_profiles::{
    count_ffmpeg_profiles, delete_ffmpeg_profile, list_ffmpeg_profile_rows, FfmpegProfileListRow,
};
use crate::ui::primitives::{
    dialog::DeleteDialog,
    table::{Column, PageParams, PagedTable, RowActionsRenderer},
};

stylance::import_crate_style!(style, "src/pages/ffmpeg_profiles/list.module.css");

const PAGE_SIZE: u32 = 25;

#[component]
pub fn FfmpegProfilesListPage() -> impl IntoView {
    let params = RwSignal::new(PageParams::new(PAGE_SIZE));

    let rows_resource: Resource<Vec<FfmpegProfileListRow>> = Resource::new(
        move || params.get(),
        |p| async move {
            list_ffmpeg_profile_rows(p.page, p.page_size)
                .await
                .unwrap_or_default()
        },
    );

    let count_resource: Resource<u64> = Resource::new(
        || (),
        |()| async move { count_ffmpeg_profiles().await.unwrap_or(0) },
    );

    let total = Signal::derive(move || count_resource.get().unwrap_or(0));

    let delete_action = Action::new(|id: &i64| delete_ffmpeg_profile(*id));
    let delete_target: RwSignal<Option<FfmpegProfileListRow>> = RwSignal::new(None);
    let delete_open = RwSignal::new(false);

    Effect::new(move |_| {
        if matches!(delete_action.value().get(), Some(Ok(()))) {
            rows_resource.refetch();
            count_resource.refetch();
            delete_open.set(false);
        }
    });

    let columns: Vec<Column<FfmpegProfileListRow>> = vec![
        Column::new("Name", |p: &FfmpegProfileListRow| {
            let n = p.name.clone();
            view! { <span>{n}</span> }.into_any()
        }),
        Column::new("Resolution", |p: &FfmpegProfileListRow| {
            let r = p.resolution.clone();
            view! { <span class=style::cell_muted>{r}</span> }.into_any()
        }),
        Column::new("Video Format", |p: &FfmpegProfileListRow| {
            let v = p.video_format.clone();
            view! { <span class=style::cell_muted>{v}</span> }.into_any()
        }),
        Column::new("Audio Format", |p: &FfmpegProfileListRow| {
            let a = p.audio_format.clone();
            view! { <span class=style::cell_muted>{a}</span> }.into_any()
        }),
    ];

    let row_actions: RowActionsRenderer<FfmpegProfileListRow> = Box::new(move |p| {
        let id = p.id;
        let p_clone = p.clone();
        view! {
            <div class=style::row_actions>
                <a href=format!("/ffmpeg/profiles/{id}") class=style::row_btn_edit>"Edit"</a>
                <button
                    class=style::row_btn_delete
                    on:click=move |_| {
                        delete_target.set(Some(p_clone.clone()));
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
            .map(|p| p.name.clone())
            .unwrap_or_default()
    });

    view! {
        <div class=style::page>
            <div class=style::toolbar>
                <h1 class=style::page_title>"FFmpeg Profiles"</h1>
                <div class=style::toolbar_actions>
                    <a href="/ffmpeg/profiles/add" class=style::btn_add>"Add Profile"</a>
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
                        if let Some(p) = delete_target.get_untracked() {
                            delete_action.dispatch(p.id);
                        }
                    } else {
                        delete_open.set(false);
                    }
                })
            />
        </div>
    }
}
