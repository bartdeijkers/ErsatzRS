#![allow(clippy::too_many_lines)]
use leptos::prelude::*;

use crate::admin::i18n::{tr, TextKey};
use crate::admin::ui::primitives::{
    badge::Badge,
    button::{Button, ButtonVariant, IconButton, LinkButton},
    dialog::{DeleteDialog, Modal},
    form::{CheckBox, NumberField, SelectField, TextArea, TextField},
    icon::{icon_style, IconName},
    table::{Column, PageParams, PagedTable},
    toast::{use_toast, ToastSeverity},
};

/// Component gallery for Phase 6.0 verification. Accessible at `/dev/components`.
#[allow(clippy::too_many_lines)]
#[component]
pub fn ComponentsPage() -> impl IntoView {
    if !development_routes_enabled() {
        return view! {
            <div data-not-found="true">
                <h1>"404"</h1>
                <a href="/">{tr(TextKey::GoHome)}</a>
            </div>
        }
        .into_any();
    }

    let text_val = RwSignal::new(tr(TextKey::EnterName).to_string());
    let num_val = RwSignal::new(42i64);
    let check_val = RwSignal::new(false);
    let textarea_val = RwSignal::new("schedule:\n  items: []".to_string());
    let select_val = RwSignal::new("b".to_string());
    let modal_open = RwSignal::new(false);
    let delete_open = RwSignal::new(false);
    let badge_count = RwSignal::new(3u32);
    let channel_label = tr(TextKey::Channel);

    let params = RwSignal::new(PageParams::new(5));
    let rows_data: Vec<(String, String)> = vec![
        ("101".to_owned(), format!("{channel_label} 101")),
        ("102".to_owned(), format!("{channel_label} 102")),
        ("103".to_owned(), format!("{channel_label} 103")),
        ("104".to_owned(), format!("{channel_label} 104")),
        ("105".to_owned(), format!("{channel_label} 105")),
        ("106".to_owned(), format!("{channel_label} 106")),
        ("107".to_owned(), format!("{channel_label} 107")),
    ];
    let rows_data_for_total = rows_data.clone();
    let total = Signal::derive(move || Some(rows_data_for_total.len() as u64));
    let rows = Resource::new(
        move || params.get(),
        move |p| {
            let data = rows_data.clone();
            async move {
                let start = (p.page * p.page_size) as usize;
                let end = (start + p.page_size as usize).min(data.len());
                data.get(start..end).unwrap_or(&[]).to_vec()
            }
        },
    );

    let columns: Vec<Column<(String, String)>> = vec![
        Column::new("#", |row: &(String, String)| {
            let v = row.0.clone();
            view! { <span>{v}</span> }.into_any()
        }),
        Column::new(tr(TextKey::Name), |row: &(String, String)| {
            let v = row.1.clone();
            view! { <span style="font-weight:500">{v}</span> }.into_any()
        }),
    ];

    let toast = use_toast();

    view! {
        <div style="max-width:48rem;margin:0 auto;padding:2rem 0;display:flex;flex-direction:column;gap:2.5rem">
            <h1 style="font-size:1.5rem;font-weight:700;color:var(--color-gray-100);margin:0">
                {tr(TextKey::ComponentGallery)}
            </h1>

            // ── Icons ─────────────────────────────────────────────────────────────
            <section style="display:flex;flex-direction:column;gap:0.75rem">
                <h2 style="font-size:0.875rem;font-weight:600;color:var(--color-gray-300);border-bottom:1px solid var(--color-border);padding-bottom:0.25rem;margin:0">
                    {tr(TextKey::Icons)}
                </h2>
                <div style="display:flex;flex-wrap:wrap;gap:1rem;color:var(--color-gray-300)">
                    <span style=icon_style(IconName::Add, "1.5rem") aria-hidden="true"></span>
                    <span style=icon_style(IconName::Edit, "1.5rem") aria-hidden="true"></span>
                    <span style=icon_style(IconName::Delete, "1.5rem") aria-hidden="true"></span>
                    <span style=icon_style(IconName::Save, "1.5rem") aria-hidden="true"></span>
                    <span style=icon_style(IconName::Menu, "1.5rem") aria-hidden="true"></span>
                    <span style=icon_style(IconName::Close, "1.5rem") aria-hidden="true"></span>
                    <span style="color:var(--color-warning)"><span style=icon_style(IconName::Warning, "1.5rem") aria-hidden="true"></span></span>
                    <span style="color:var(--color-error)"><span style=icon_style(IconName::Error, "1.5rem") aria-hidden="true"></span></span>
                    <span style="color:var(--color-success)"><span style=icon_style(IconName::Check, "1.5rem") aria-hidden="true"></span></span>
                    <span style=icon_style(IconName::Search, "1.5rem") aria-hidden="true"></span>
                    <span style=icon_style(IconName::Settings, "1.5rem") aria-hidden="true"></span>
                    <span style="color:var(--color-primary)"><span style=icon_style(IconName::Play, "1.5rem") aria-hidden="true"></span></span>
                    <span style=icon_style(IconName::Stop, "1.5rem") aria-hidden="true"></span>
                    <span style=icon_style(IconName::Refresh, "1.5rem") aria-hidden="true"></span>
                </div>
            </section>

            // ── Buttons ───────────────────────────────────────────────────────────
            <section style="display:flex;flex-direction:column;gap:0.75rem">
                <h2 style="font-size:0.875rem;font-weight:600;color:var(--color-gray-300);border-bottom:1px solid var(--color-border);padding-bottom:0.25rem;margin:0">
                    {tr(TextKey::Buttons)}
                </h2>
                <div style="display:flex;flex-wrap:wrap;align-items:center;gap:0.75rem">
                    <Button variant=ButtonVariant::Primary>{tr(TextKey::PrimaryButton)}</Button>
                    <Button variant=ButtonVariant::Secondary>{tr(TextKey::SecondaryButton)}</Button>
                    <Button variant=ButtonVariant::Text>{tr(TextKey::TextButton)}</Button>
                    <Button variant=ButtonVariant::Danger>{tr(TextKey::DangerButton)}</Button>
                    <Button variant=ButtonVariant::Primary disabled=true>{tr(TextKey::DisabledButton)}</Button>
                    <LinkButton href="/channels".to_string() variant=ButtonVariant::Secondary>
                        {tr(TextKey::LinkButton)}
                    </LinkButton>
                </div>
                <div style="display:flex;align-items:center;gap:0.5rem">
                    <IconButton icon=IconName::Edit title=tr(TextKey::Edit)/>
                    <IconButton icon=IconName::Delete title=tr(TextKey::Delete) variant=ButtonVariant::Danger/>
                    <IconButton icon=IconName::Play title=tr(TextKey::Play) variant=ButtonVariant::Primary/>
                </div>
            </section>

            // ── Badge ─────────────────────────────────────────────────────────────
            <section style="display:flex;flex-direction:column;gap:0.75rem">
                <h2 style="font-size:0.875rem;font-weight:600;color:var(--color-gray-300);border-bottom:1px solid var(--color-border);padding-bottom:0.25rem;margin:0">
                    {tr(TextKey::Badge)}
                </h2>
                <div style="display:flex;align-items:center;gap:1.5rem">
                    <div style="position:relative;display:inline-flex">
                        <span style="color:var(--color-warning)"><span style=icon_style(IconName::Warning, "1.5rem") aria-hidden="true"></span></span>
                        <Badge count=badge_count.into()/>
                    </div>
                    <div style="position:relative;display:inline-flex">
                        <span style="color:var(--color-error)"><span style=icon_style(IconName::Error, "1.5rem") aria-hidden="true"></span></span>
                        <Badge count=Signal::derive(|| 99u32)/>
                    </div>
                    <Button
                        variant=ButtonVariant::Text
                        on_click=Some(Callback::new(move |()| badge_count.update(|n| *n += 1)))
                    >{tr(TextKey::IncrementBadge)}</Button>
                    <Button
                        variant=ButtonVariant::Text
                        on_click=Some(Callback::new(move |()| badge_count.update(|n| *n = n.saturating_sub(1))))
                    >{tr(TextKey::Decrement)}</Button>
                </div>
            </section>

            // ── Forms ─────────────────────────────────────────────────────────────
            <section style="display:flex;flex-direction:column;gap:0.75rem">
                <h2 style="font-size:0.875rem;font-weight:600;color:var(--color-gray-300);border-bottom:1px solid var(--color-border);padding-bottom:0.25rem;margin:0">
                    {tr(TextKey::FormInputs)}
                </h2>
                <div style="display:grid;grid-template-columns:1fr 1fr;gap:1rem">
                    <TextField label=tr(TextKey::ChannelName) value=text_val placeholder=tr(TextKey::EnterName)/>
                    <NumberField label=tr(TextKey::ChannelNumber) value=num_val min=Some(1) max=Some(9999)/>
                    <SelectField
                        label=tr(TextKey::StreamingMode)
                        value=select_val
                        items=vec![
                            ("a".to_string(), "HLS".to_string()),
                            ("b".to_string(), "MPEG-TS".to_string()),
                            ("c".to_string(), tr(TextKey::HlsDirect).to_string()),
                        ]
                    />
                    <CheckBox label=tr(TextKey::ShowDisabled) value=check_val/>
                </div>
                <TextArea label=tr(TextKey::YamlScript) value=textarea_val rows=4 placeholder="# yaml..."/>
                <p style="font-size:0.75rem;color:var(--color-muted)">
                    {tr(TextKey::TextValue)}": "{move || text_val.get()}
                    " | "{tr(TextKey::NumValue)}": "{move || num_val.get().to_string()}
                    " | "{tr(TextKey::SelectValue)}": "{move || select_val.get()}
                    " | "{tr(TextKey::CheckValue)}": "{move || check_val.get().to_string()}
                </p>
            </section>

            // ── Table ─────────────────────────────────────────────────────────────
            <section style="display:flex;flex-direction:column;gap:0.75rem">
                <h2 style="font-size:0.875rem;font-weight:600;color:var(--color-gray-300);border-bottom:1px solid var(--color-border);padding-bottom:0.25rem;margin:0">
                    {tr(TextKey::PagedTableDemo)}
                </h2>
                <PagedTable
                    rows=rows
                    total=total
                    params=params
                    columns=columns
                    row_actions=Some(Box::new(|_row: &(String, String)| view! {
                        <div style="display:flex;gap:0.25rem">
                            <IconButton icon=IconName::Edit title=tr(TextKey::Edit)/>
                            <IconButton icon=IconName::Delete title=tr(TextKey::Delete) variant=ButtonVariant::Danger/>
                        </div>
                    }.into_any()))
                />
            </section>

            // ── Dialogs ───────────────────────────────────────────────────────────
            <section style="display:flex;flex-direction:column;gap:0.75rem">
                <h2 style="font-size:0.875rem;font-weight:600;color:var(--color-gray-300);border-bottom:1px solid var(--color-border);padding-bottom:0.25rem;margin:0">
                    {tr(TextKey::Dialogs)}
                </h2>
                <div style="display:flex;gap:0.75rem">
                    <Button on_click=Some(Callback::new(move |()| modal_open.set(true)))>
                        {tr(TextKey::OpenModal)}
                    </Button>
                    <Button
                        variant=ButtonVariant::Danger
                        on_click=Some(Callback::new(move |()| delete_open.set(true)))
                    >{tr(TextKey::DeleteDialog)}</Button>
                </div>
                <Modal
                    open=modal_open
                    title=tr(TextKey::ExampleModal).to_string()
                    on_close=Callback::new(move |confirmed| {
                        let msg = if confirmed { tr(TextKey::ModalConfirmed) } else { tr(TextKey::ModalCancelled) };
                        toast.push(msg, ToastSeverity::Info);
                    })
                >
                    <p>{tr(TextKey::ExampleModalBody)}</p>
                </Modal>
                <DeleteDialog
                    open=delete_open
                    entity_name=Signal::derive(move || format!("{channel_label} 101"))
                    on_confirm=Callback::new(move |confirmed| {
                        if confirmed {
                            toast.push(tr(TextKey::OperationSucceeded), ToastSeverity::Success);
                        }
                    })
                />
            </section>

            // ── Toasts ────────────────────────────────────────────────────────────
            <section style="display:flex;flex-direction:column;gap:0.75rem">
                <h2 style="font-size:0.875rem;font-weight:600;color:var(--color-gray-300);border-bottom:1px solid var(--color-border);padding-bottom:0.25rem;margin:0">
                    {tr(TextKey::Toasts)}
                </h2>
                <div style="display:flex;flex-wrap:wrap;gap:0.5rem">
                    <Button
                        variant=ButtonVariant::Primary
                        on_click=Some(Callback::new(move |()| {
                            toast.push(tr(TextKey::OperationSucceeded), ToastSeverity::Success);
                        }))
                    >{tr(TextKey::SuccessToast)}</Button>
                    <Button
                        variant=ButtonVariant::Text
                        on_click=Some(Callback::new(move |()| {
                            toast.push(tr(TextKey::JustFyi), ToastSeverity::Info);
                        }))
                    >{tr(TextKey::InfoToast)}</Button>
                    <Button
                        variant=ButtonVariant::Secondary
                        on_click=Some(Callback::new(move |()| {
                            toast.push(tr(TextKey::SomethingMightBeWrong), ToastSeverity::Warning);
                        }))
                    >{tr(TextKey::WarningToast)}</Button>
                    <Button
                        variant=ButtonVariant::Danger
                        on_click=Some(Callback::new(move |()| {
                            toast.push(tr(TextKey::ErrorOccurred), ToastSeverity::Error);
                        }))
                    >{tr(TextKey::ErrorToast)}</Button>
                </div>
            </section>
        </div>
    }
    .into_any()
}

#[cfg(not(target_arch = "wasm32"))]
fn development_routes_enabled() -> bool {
    crate::host::config::development_routes_enabled()
}

#[cfg(target_arch = "wasm32")]
fn development_routes_enabled() -> bool {
    cfg!(feature = "development-routes")
}
