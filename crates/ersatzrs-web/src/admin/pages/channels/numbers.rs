use leptos::prelude::*;
use leptos_router::hooks::use_query_map;

use crate::admin::i18n::{tr, TextKey};
#[cfg(feature = "ssr")]
use crate::admin::server_fns::channels::list_channel_number_rows;
use crate::admin::server_fns::channels::ChannelNumberEditorRow;

stylance::import_crate_style!(style, "src/admin/pages/channels/numbers.module.css");

#[component]
pub fn ChannelNumbersPage() -> impl IntoView {
    let query = use_query_map();
    let form_error = query.with_untracked(|params| params.get("error").unwrap_or_default());

    view! {
        <ChannelNumbersForm rows=channel_number_rows_for_page() form_error=form_error/>
    }
}

#[cfg(feature = "ssr")]
fn channel_number_rows_for_page() -> Vec<ChannelNumberEditorRow> {
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current()
            .block_on(list_channel_number_rows())
            .unwrap_or_default()
    })
}

#[cfg(not(feature = "ssr"))]
fn channel_number_rows_for_page() -> Vec<ChannelNumberEditorRow> {
    Vec::new()
}

#[component]
fn ChannelNumbersForm(rows: Vec<ChannelNumberEditorRow>, form_error: String) -> impl IntoView {
    let has_error = !form_error.is_empty();

    view! {
        <form
            class=style::page
            method="post"
            action="/api/channels/numbers/save"
            data-channel-number-form="true"
        >
            <div class=style::action_bar>
                <div class=style::desktop_actions>
                    <button type="submit" class=style::btn_save>
                        {tr(TextKey::SaveChannels)}
                    </button>
                    <button type="reset" class=style::btn_reset>
                        {tr(TextKey::ResetChannels)}
                    </button>
                </div>
                <details class=style::mobile_actions data-channel-number-mobile-actions="true">
                    <summary
                        class=style::mobile_menu_button
                        aria-label=tr(TextKey::Actions)
                    ></summary>
                    <div class=style::mobile_menu>
                        <button type="submit" class=style::mobile_menu_item>
                            {tr(TextKey::SaveChannels)}
                        </button>
                        <button type="reset" class=style::mobile_menu_item>
                            {tr(TextKey::ResetChannels)}
                        </button>
                    </div>
                </details>
            </div>

            <section class=style::content>
                <a href="/channels" class=style::back_link>{tr(TextKey::Channels)}</a>
                <h1 class=style::page_title>{tr(TextKey::EditChannelNumbers)}</h1>
                <div class=style::divider/>

                <p class=style::form_error role="alert" hidden=!has_error>
                    {form_error}
                </p>

                <ol class=style::number_list>
                    {rows
                        .into_iter()
                        .map(channel_number_row)
                        .collect_view()}
                </ol>
            </section>
            <script inner_html=channel_number_change_script()></script>
        </form>
    }
}

fn channel_number_row(row: ChannelNumberEditorRow) -> impl IntoView {
    let input_id = format!("channel-number-{}", row.id);
    let input_name = format!("number_{}", row.id);
    let name = row.name;
    let label = format!("{} {}", tr(TextKey::ChannelNumber), name);
    let original_number = row.original_number;
    let number = row.number;

    view! {
        <li class=style::number_card data-channel-number-row="true" data-changed="false">
            <input
                id=input_id
                class=style::number_input
                name=input_name
                type="text"
                inputmode="decimal"
                value=number
                aria-label=label
                data-channel-number-input="true"
                data-original=original_number.clone()
            />
            <span class=style::channel_name>{name}</span>
            <span class=style::original_number>
                {format!("({original_number})")}
            </span>
        </li>
    }
}

fn channel_number_change_script() -> &'static str {
    r"
(() => {
  const inputs = document.querySelectorAll('[data-channel-number-input]');
  inputs.forEach((input) => {
    const row = input.closest('[data-channel-number-row]');
    const original = input.dataset.original || '';
    const sync = () => {
      if (row) {
        row.dataset.changed = input.value.trim() === original ? 'false' : 'true';
      }
    };
    sync();
    input.addEventListener('input', sync);
    if (input.form) {
      input.form.addEventListener('reset', () => setTimeout(sync, 0));
    }
  });
})();
"
}
