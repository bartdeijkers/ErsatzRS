use leptos::prelude::*;

use crate::admin::i18n::{
    ffmpeg_profile_audio_format_text, ffmpeg_profile_error_text, ffmpeg_profile_video_format_text,
    provide_i18n_context, text, tr, use_i18n, TextKey,
};
use crate::admin::server_fns::ffmpeg_profiles::{
    count_ffmpeg_profiles, delete_ffmpeg_profile, list_ffmpeg_profile_rows, FfmpegProfileListRow,
};
use crate::admin::ui::primitives::{
    dialog::DeleteDialog,
    page_header::{PageAction, PageHeader},
    table::{Column, PageParams, PagedTable, RowActionsRenderer},
    toast::{load_or_notify, use_toast, ToastSeverity},
};

stylance::import_crate_style!(style, "src/admin/pages/ffmpeg_profiles/list.module.css");

const PAGE_SIZE: u32 = 25;

#[component]
pub fn FfmpegProfilesListPage() -> impl IntoView {
    view! { <FfmpegProfilesListIsland/> }
}

#[island]
fn FfmpegProfilesListIsland() -> impl IntoView {
    provide_i18n_context();
    let language = use_i18n();
    let params = RwSignal::new(PageParams::new(PAGE_SIZE));
    let load_toast = use_toast();

    let rows_resource: LocalResource<Vec<FfmpegProfileListRow>> = LocalResource::new(move || {
        let p = params.get();
        async move {
            load_or_notify(
                list_ffmpeg_profile_rows(p.page, p.page_size).await,
                load_toast,
            )
        }
    });

    let count_resource: LocalResource<u64> = LocalResource::new(move || async move {
        load_or_notify(count_ffmpeg_profiles().await, load_toast)
    });

    let total = Signal::derive(move || count_resource.get());

    let delete_action = Action::new(|id: &i64| delete_ffmpeg_profile(*id));
    let delete_target: RwSignal<Option<FfmpegProfileListRow>> = RwSignal::new(None);
    let delete_open = RwSignal::new(false);

    Effect::new(move |_| match delete_action.value().get() {
        Some(Ok(())) => {
            rows_resource.refetch();
            count_resource.refetch();
            delete_open.set(false);
        }
        Some(Err(error)) => {
            load_toast.push(
                ffmpeg_profile_error_text(language, &error.to_string()),
                ToastSeverity::Error,
            );
            delete_open.set(false);
        }
        None => {}
    });

    let columns: Vec<Column<FfmpegProfileListRow>> = vec![
        Column::new(tr(TextKey::Name), |p: &FfmpegProfileListRow| {
            let n = p.name.clone();
            view! { <span>{n}</span> }.into_any()
        }),
        Column::new(tr(TextKey::Resolution), |p: &FfmpegProfileListRow| {
            let r = p.resolution.clone();
            view! { <span class=style::cell_muted>{r}</span> }.into_any()
        }),
        Column::new(tr(TextKey::VideoFormat), move |p: &FfmpegProfileListRow| {
            let v =
                ffmpeg_profile_video_format_text(language, p.video_format, p.hardware_acceleration);
            view! { <span class=style::cell_muted>{v}</span> }.into_any()
        }),
        Column::new(tr(TextKey::AudioFormat), move |p: &FfmpegProfileListRow| {
            let a = ffmpeg_profile_audio_format_text(language, p.audio_format);
            view! { <span class=style::cell_muted>{a}</span> }.into_any()
        }),
    ];

    let row_actions: RowActionsRenderer<FfmpegProfileListRow> = Box::new(move |p| {
        let id = p.id;
        let p_clone = p.clone();
        view! {
            <div class=style::row_actions>
                <a href=format!("/ffmpeg/{id}") class=style::row_btn_edit>{text(language, TextKey::Edit)}</a>
                <button
                    class=style::row_btn_delete
                    on:click=move |_| {
                        delete_target.set(Some(p_clone.clone()));
                        delete_open.set(true);
                    }
                >
                    {text(language, TextKey::Delete)}
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
            <PageHeader title=tr(TextKey::FfmpegProfiles)>
                <PageAction href="/ffmpeg/add" label=tr(TextKey::AddProfile)/>
            </PageHeader>

            <PagedTable
                rows=rows_resource
                columns=columns
                row_actions=Some(row_actions)
                params=params
                total=total
            />

            <DeleteDialog
                dialog_id="ffmpeg-profile-delete-dialog"
                open=delete_open
                entity_name=delete_name
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
