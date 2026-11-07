//! `/teletext-elements`: the library of custom teletext pages.

use leptos::prelude::*;
use leptos_router::hooks::use_query_map;

use super::data::ElementRow;
use super::screen::teletext_screen;
use crate::admin::i18n::{tr, TextKey};
use crate::admin::ui::primitives::teletext::{status_badge, TeletextTone};

stylance::import_crate_style!(pub(super) style, "src/admin/pages/teletext_elements/pages.module.css");

fn load() -> Result<Vec<ElementRow>, ()> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        super::data::load_rows()
    }
    #[cfg(target_arch = "wasm32")]
    {
        Err(())
    }
}

/// Count text such as "2 channels", localized with a singular form.
pub(super) fn count_label(count: u16, one: TextKey, many: TextKey) -> String {
    if count == 1 {
        tr(one).to_owned()
    } else {
        tr(many).replace("{count}", &count.to_string())
    }
}

#[component]
pub fn TeletextElementsListPage() -> impl IntoView {
    let query = use_query_map();
    let saved = query.with_untracked(|q| q.get("saved"));
    let feedback = (saved.as_deref() == Some("deleted")).then(|| {
        view! {
            <p class=style::feedback role="status" data-teletext-elements-feedback>
                {tr(TextKey::TeletextElementDeleted)}
            </p>
        }
    });
    let body = match load() {
        Ok(rows) if rows.is_empty() => empty_state(),
        Ok(rows) => table(rows),
        Err(()) => view! {
            <p role="alert" data-teletext-elements-error>{tr(TextKey::ErrorOccurred)}</p>
        }
        .into_any(),
    };
    view! {
        <div class=style::page data-teletext-elements-page="list">
            <header class=style::header>
                <div>
                    <h1 class=style::title>{tr(TextKey::TeletextElements)}</h1>
                    <p class=style::subtitle>{tr(TextKey::TeletextElementsHelp)}</p>
                </div>
                <a href="/teletext-elements/add" class=style::primary data-add-teletext-element>
                    {tr(TextKey::UploadTeletextPage)}
                </a>
            </header>
            {feedback}
            {body}
        </div>
    }
}

fn empty_state() -> AnyView {
    view! {
        <section class=style::empty data-teletext-elements-empty>
            <h2>{tr(TextKey::NoTeletextElements)}</h2>
            <p>{tr(TextKey::NoTeletextElementsHelp)}</p>
            <a href="/teletext-elements/add" class=style::primary>{tr(TextKey::UploadTeletextPage)}</a>
        </section>
    }
    .into_any()
}

fn table(rows: Vec<ElementRow>) -> AnyView {
    view! {
        <div class=style::table_wrap>
            <table class=style::table data-teletext-elements-table>
                <thead>
                    <tr>
                        <th><span class=style::visually_hidden>{tr(TextKey::Preview)}</span></th>
                        <th>{tr(TextKey::Name)}</th>
                        <th>{tr(TextKey::TeletextElementPageNumber)}</th>
                        <th>{tr(TextKey::TeletextElementFieldLinks)}</th>
                        <th>{tr(TextKey::TeletextElementUsedOn)}</th>
                        <th><span class=style::visually_hidden>{tr(TextKey::Actions)}</span></th>
                    </tr>
                </thead>
                <tbody>{rows.into_iter().map(row).collect_view()}</tbody>
            </table>
        </div>
    }
    .into_any()
}

fn row(row: ElementRow) -> impl IntoView {
    let href = format!("/teletext-elements/{}", row.id);
    let detail = format!(
        "{} · {}",
        row.format,
        count_label(
            row.subpages,
            TextKey::TeletextElementOneSubpage,
            TextKey::TeletextElementCountSubpages
        )
    );
    let thumbnail = row
        .thumbnail
        .as_ref()
        .map(|view| teletext_screen(view, format!("{} {}", tr(TextKey::Preview), row.name), true));
    let fields = if row.fields == 0 {
        view! { <span class=style::muted>{tr(TextKey::TeletextElementNoFields)}</span> }.into_any()
    } else if row.unset_fields > 0 {
        status_badge(
            TeletextTone::Warning,
            count_label(
                row.unset_fields,
                TextKey::TeletextElementOneFieldToChoose,
                TextKey::TeletextElementCountFieldsToChoose,
            ),
        )
    } else {
        status_badge(
            TeletextTone::Success,
            count_label(
                row.fields,
                TextKey::TeletextElementOneFieldLinked,
                TextKey::TeletextElementCountFieldsLinked,
            ),
        )
    };
    let usage = if row.channels == 0 {
        status_badge(TeletextTone::Neutral, tr(TextKey::TeletextElementNotUsed))
    } else {
        status_badge(
            TeletextTone::Success,
            count_label(
                row.channels,
                TextKey::TeletextElementOneChannel,
                TextKey::TeletextElementCountChannels,
            ),
        )
    };
    view! {
        <tr data-teletext-element-row=row.id.to_string()>
            <td class=style::thumb_cell><a href=href.clone() tabindex="-1" class=style::thumb>{thumbnail}</a></td>
            <td>
                <a href=href.clone() class=style::name_link>{row.name.clone()}</a>
                <div class=style::muted>{detail}</div>
            </td>
            <td>{row.page}</td>
            <td>{fields}</td>
            <td>{usage}</td>
            <td class=style::actions>
                <a href=href class=style::edit_link data-teletext-element-edit>{tr(TextKey::Edit)}</a>
            </td>
        </tr>
    }
}
