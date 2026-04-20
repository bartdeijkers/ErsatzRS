#![allow(clippy::too_many_lines)]
use leptos::prelude::*;

use crate::ui::primitives::{
    badge::Badge,
    button::{Button, ButtonVariant, IconButton, LinkButton},
    dialog::{DeleteDialog, Modal},
    form::{CheckBox, NumberField, SelectField, TextArea, TextField},
    icon::{Icon, IconName},
    table::{Column, PageParams, PagedTable},
    toast::{use_toast, ToastContainer, ToastSeverity},
};

/// Component gallery for Phase 6.0 verification. Accessible at `/dev/components`.
#[allow(clippy::too_many_lines)]
#[component]
pub fn ComponentsPage() -> impl IntoView {
    let text_val = RwSignal::new("hello".to_string());
    let num_val = RwSignal::new(42i64);
    let check_val = RwSignal::new(false);
    let textarea_val = RwSignal::new("line one\nline two".to_string());
    let select_val = RwSignal::new("b".to_string());
    let modal_open = RwSignal::new(false);
    let delete_open = RwSignal::new(false);
    let badge_count = RwSignal::new(3u32);

    let params = RwSignal::new(PageParams::new(5));
    let rows_data: Vec<(String, String)> = vec![
        ("101", "News Channel"),
        ("102", "Sports HD"),
        ("103", "Movies 24/7"),
        ("104", "Kids Zone"),
        ("105", "Documentary+"),
        ("106", "Music TV"),
        ("107", "Local News"),
    ]
    .into_iter()
    .map(|(n, name)| (n.to_string(), name.to_string()))
    .collect();
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
        Column::new("Name", |row: &(String, String)| {
            let v = row.1.clone();
            view! { <span style="font-weight:500">{v}</span> }.into_any()
        }),
    ];

    let toast = use_toast();

    view! {
        <div style="max-width:48rem;margin:0 auto;padding:2rem 0;display:flex;flex-direction:column;gap:2.5rem">
            <h1 style="font-size:1.5rem;font-weight:700;color:var(--color-gray-100);margin:0">
                "ErsatzRS Component Gallery"
            </h1>

            // ── Icons ─────────────────────────────────────────────────────────────
            <section style="display:flex;flex-direction:column;gap:0.75rem">
                <h2 style="font-size:0.875rem;font-weight:600;color:var(--color-gray-300);border-bottom:1px solid var(--color-border);padding-bottom:0.25rem;margin:0">
                    "Icons"
                </h2>
                <div style="display:flex;flex-wrap:wrap;gap:1rem;color:var(--color-gray-300)">
                    <Icon name=IconName::Add size="1.5rem"/>
                    <Icon name=IconName::Edit size="1.5rem"/>
                    <Icon name=IconName::Delete size="1.5rem"/>
                    <Icon name=IconName::Save size="1.5rem"/>
                    <Icon name=IconName::Menu size="1.5rem"/>
                    <Icon name=IconName::Close size="1.5rem"/>
                    <span style="color:var(--color-warning)"><Icon name=IconName::Warning size="1.5rem"/></span>
                    <span style="color:var(--color-error)"><Icon name=IconName::Error size="1.5rem"/></span>
                    <span style="color:var(--color-success)"><Icon name=IconName::Check size="1.5rem"/></span>
                    <Icon name=IconName::Search size="1.5rem"/>
                    <Icon name=IconName::Settings size="1.5rem"/>
                    <span style="color:var(--color-primary)"><Icon name=IconName::Play size="1.5rem"/></span>
                    <Icon name=IconName::Stop size="1.5rem"/>
                    <Icon name=IconName::Refresh size="1.5rem"/>
                </div>
            </section>

            // ── Buttons ───────────────────────────────────────────────────────────
            <section style="display:flex;flex-direction:column;gap:0.75rem">
                <h2 style="font-size:0.875rem;font-weight:600;color:var(--color-gray-300);border-bottom:1px solid var(--color-border);padding-bottom:0.25rem;margin:0">
                    "Buttons"
                </h2>
                <div style="display:flex;flex-wrap:wrap;align-items:center;gap:0.75rem">
                    <Button variant=ButtonVariant::Primary>"Primary"</Button>
                    <Button variant=ButtonVariant::Secondary>"Secondary"</Button>
                    <Button variant=ButtonVariant::Text>"Text"</Button>
                    <Button variant=ButtonVariant::Danger>"Danger"</Button>
                    <Button variant=ButtonVariant::Primary disabled=true>"Disabled"</Button>
                    <LinkButton href="/channels".to_string() variant=ButtonVariant::Secondary>
                        "Link Button"
                    </LinkButton>
                </div>
                <div style="display:flex;align-items:center;gap:0.5rem">
                    <IconButton icon=IconName::Edit title="Edit"/>
                    <IconButton icon=IconName::Delete title="Delete" variant=ButtonVariant::Danger/>
                    <IconButton icon=IconName::Play title="Play" variant=ButtonVariant::Primary/>
                </div>
            </section>

            // ── Badge ─────────────────────────────────────────────────────────────
            <section style="display:flex;flex-direction:column;gap:0.75rem">
                <h2 style="font-size:0.875rem;font-weight:600;color:var(--color-gray-300);border-bottom:1px solid var(--color-border);padding-bottom:0.25rem;margin:0">
                    "Badge"
                </h2>
                <div style="display:flex;align-items:center;gap:1.5rem">
                    <div style="position:relative;display:inline-flex">
                        <span style="color:var(--color-warning)"><Icon name=IconName::Warning size="1.5rem"/></span>
                        <Badge count=badge_count.into()/>
                    </div>
                    <div style="position:relative;display:inline-flex">
                        <span style="color:var(--color-error)"><Icon name=IconName::Error size="1.5rem"/></span>
                        <Badge count=Signal::derive(|| 99u32)/>
                    </div>
                    <Button
                        variant=ButtonVariant::Text
                        on_click=Some(Callback::new(move |()| badge_count.update(|n| *n += 1)))
                    >"Increment badge"</Button>
                    <Button
                        variant=ButtonVariant::Text
                        on_click=Some(Callback::new(move |()| badge_count.update(|n| *n = n.saturating_sub(1))))
                    >"Decrement"</Button>
                </div>
            </section>

            // ── Forms ─────────────────────────────────────────────────────────────
            <section style="display:flex;flex-direction:column;gap:0.75rem">
                <h2 style="font-size:0.875rem;font-weight:600;color:var(--color-gray-300);border-bottom:1px solid var(--color-border);padding-bottom:0.25rem;margin:0">
                    "Form Inputs"
                </h2>
                <div style="display:grid;grid-template-columns:1fr 1fr;gap:1rem">
                    <TextField label="Channel Name" value=text_val placeholder="Enter name"/>
                    <NumberField label="Channel Number" value=num_val min=Some(1) max=Some(9999)/>
                    <SelectField
                        label="Streaming Mode"
                        value=select_val
                        items=vec![
                            ("a".to_string(), "HLS".to_string()),
                            ("b".to_string(), "MPEG-TS".to_string()),
                            ("c".to_string(), "Direct".to_string()),
                        ]
                    />
                    <CheckBox label="Show Disabled Channels" value=check_val/>
                </div>
                <TextArea label="YAML Script" value=textarea_val rows=4 placeholder="# yaml..."/>
                <p style="font-size:0.75rem;color:var(--color-muted)">
                    "Text: "{move || text_val.get()}
                    " | Num: "{move || num_val.get().to_string()}
                    " | Select: "{move || select_val.get()}
                    " | Check: "{move || check_val.get().to_string()}
                </p>
            </section>

            // ── Table ─────────────────────────────────────────────────────────────
            <section style="display:flex;flex-direction:column;gap:0.75rem">
                <h2 style="font-size:0.875rem;font-weight:600;color:var(--color-gray-300);border-bottom:1px solid var(--color-border);padding-bottom:0.25rem;margin:0">
                    "PagedTable (5 rows per page, 7 total)"
                </h2>
                <PagedTable
                    rows=rows
                    total=total
                    params=params
                    columns=columns
                    row_actions=Some(Box::new(|_row: &(String, String)| view! {
                        <div style="display:flex;gap:0.25rem">
                            <IconButton icon=IconName::Edit title="Edit"/>
                            <IconButton icon=IconName::Delete title="Delete" variant=ButtonVariant::Danger/>
                        </div>
                    }.into_any()))
                />
            </section>

            // ── Dialogs ───────────────────────────────────────────────────────────
            <section style="display:flex;flex-direction:column;gap:0.75rem">
                <h2 style="font-size:0.875rem;font-weight:600;color:var(--color-gray-300);border-bottom:1px solid var(--color-border);padding-bottom:0.25rem;margin:0">
                    "Dialogs"
                </h2>
                <div style="display:flex;gap:0.75rem">
                    <Button on_click=Some(Callback::new(move |()| modal_open.set(true)))>
                        "Open Modal"
                    </Button>
                    <Button
                        variant=ButtonVariant::Danger
                        on_click=Some(Callback::new(move |()| delete_open.set(true)))
                    >"Delete Dialog"</Button>
                </div>
                <Modal
                    open=modal_open
                    title="Example Modal".to_string()
                    on_close=Callback::new(move |confirmed| {
                        let msg = if confirmed { "Confirmed!" } else { "Cancelled." };
                        toast.push(msg, ToastSeverity::Info);
                    })
                >
                    <p>"This is an example modal dialog body."</p>
                </Modal>
                <DeleteDialog
                    open=delete_open
                    entity_name="News Channel".to_string()
                    on_confirm=Callback::new(move |confirmed| {
                        if confirmed {
                            toast.push("Channel deleted.", ToastSeverity::Success);
                        }
                    })
                />
            </section>

            // ── Toasts ────────────────────────────────────────────────────────────
            <section style="display:flex;flex-direction:column;gap:0.75rem">
                <h2 style="font-size:0.875rem;font-weight:600;color:var(--color-gray-300);border-bottom:1px solid var(--color-border);padding-bottom:0.25rem;margin:0">
                    "Toasts"
                </h2>
                <div style="display:flex;flex-wrap:wrap;gap:0.5rem">
                    <Button
                        variant=ButtonVariant::Primary
                        on_click=Some(Callback::new(move |()| {
                            toast.push("Operation succeeded.", ToastSeverity::Success);
                        }))
                    >"Success toast"</Button>
                    <Button
                        variant=ButtonVariant::Text
                        on_click=Some(Callback::new(move |()| {
                            toast.push("Just FYI.", ToastSeverity::Info);
                        }))
                    >"Info toast"</Button>
                    <Button
                        variant=ButtonVariant::Secondary
                        on_click=Some(Callback::new(move |()| {
                            toast.push("Something might be wrong.", ToastSeverity::Warning);
                        }))
                    >"Warning toast"</Button>
                    <Button
                        variant=ButtonVariant::Danger
                        on_click=Some(Callback::new(move |()| {
                            toast.push("An error occurred.", ToastSeverity::Error);
                        }))
                    >"Error toast"</Button>
                </div>
            </section>
        </div>

        <ToastContainer/>
    }
}
