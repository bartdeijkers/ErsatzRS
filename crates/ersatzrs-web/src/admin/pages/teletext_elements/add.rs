//! `/teletext-elements/add`: upload a teletext page file.

use leptos::prelude::*;
use leptos_router::hooks::use_query_map;

use super::editor::error_text;
use super::list::style;
use crate::admin::i18n::{tr, TextKey};
use crate::admin::ui::primitives::help_disclosure::HelpDisclosure;
use crate::admin::ui::primitives::teletext::{island, TeletextIsland};

/// File types offered by the browser's file picker.
pub(super) const ACCEPT: &str = ".tti,.ttix,.t42,.ep1,.epx,.htt";

#[component]
pub fn TeletextElementAddPage() -> impl IntoView {
    let query = use_query_map();
    let error = query.with_untracked(|q| q.get("error"));
    let feedback = error.map(|code| {
        view! { <p class=style::alert role="alert" data-teletext-element-error=code.clone()>{error_text(&code)}</p> }
    });
    let body = view! {
        <form
            method="post"
            action="/admin-api/teletext-elements/upload"
            enctype="multipart/form-data"
            class=style::form
            data-teletext-element-upload
        >
            <label class=style::drop>
                <span class=style::label>{tr(TextKey::TeletextElementFile)}</span>
                <input type="file" name="file" accept=ACCEPT required/>
                <span class=style::muted>{tr(TextKey::TeletextElementFileFormatsHelp)}</span>
            </label>
            <HelpDisclosure title=tr(TextKey::TeletextElementFieldMarkerTipTitle)>
                <p>{tr(TextKey::TeletextElementFieldMarkerTip)}</p>
                <p><code>"%NOW%%%%%%%%%%%%%%%%%%%"</code></p>
            </HelpDisclosure>
            <div class=style::form_actions>
                <button type="submit" class=style::primary>{tr(TextKey::UploadTeletextPage)}</button>
            </div>
        </form>
    }
    .into_any();
    view! {
        <div class=style::page data-teletext-elements-page="add">
            <a href="/teletext-elements" class=style::back>{format!("‹ {}", tr(TextKey::TeletextElements))}</a>
            <header class=style::header>
                <div>
                    <h1 class=style::title>{tr(TextKey::UploadTeletextPage)}</h1>
                    <p class=style::subtitle>{tr(TextKey::UploadTeletextPageHelp)}</p>
                </div>
            </header>
            {feedback}
            {island(TeletextIsland {
                test_id: "teletext-element-upload",
                title: tr(TextKey::TeletextElementFile).to_owned(),
                badge: None,
                help: None,
                data_id: None,
                body,
            })}
        </div>
    }
}
