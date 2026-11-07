//! `/teletext-elements/{id}`: preview a custom page, choose the data for its
//! fields, rename it, replace its file and see where it is used.

use leptos::prelude::*;
use leptos_router::hooks::{use_params_map, use_query_map};

use super::add::ACCEPT;
use super::data::{EditorData, FieldRow};
use super::list::{count_label, style};
use super::screen::teletext_screen;
use crate::admin::i18n::{tr, TextKey};
use crate::admin::ui::primitives::teletext::{
    island, preview_frame, status_badge, step_bar, TeletextIsland, TeletextStepState,
    TeletextSubpageLink, TeletextTone,
};

/// Translated text for a redirect `error=` code.
pub(super) fn error_text(code: &str) -> &'static str {
    tr(match code {
        "format" => TextKey::TeletextElementErrorFormat,
        "empty" => TextKey::TeletextElementErrorEmpty,
        "too-large" => TextKey::TeletextElementErrorTooLarge,
        "damaged" => TextKey::TeletextElementErrorDamaged,
        "duplicate-name" => TextKey::TeletextElementErrorDuplicateName,
        "stale" => TextKey::TeletextElementErrorStale,
        "name" => TextKey::TeletextElementErrorName,
        "page-number" => TextKey::TeletextElementErrorPageNumber,
        "cycle" => TextKey::TeletextElementErrorCycle,
        "fields" => TextKey::TeletextElementErrorFields,
        "confirm" => TextKey::TeletextElementErrorConfirmDelete,
        "not-found" => TextKey::TeletextElementNotFound,
        _ => TextKey::ErrorOccurred,
    })
}

fn saved_text(code: &str) -> Option<&'static str> {
    Some(tr(match code {
        "uploaded" => TextKey::TeletextElementSavedUploaded,
        "replaced" => TextKey::TeletextElementSavedReplaced,
        "details" => TextKey::TeletextElementSavedDetails,
        "fields" => TextKey::TeletextElementSavedFields,
        _ => return None,
    }))
}

fn warning_text(code: &str) -> Option<&'static str> {
    Some(tr(match code {
        "extra-pages" => TextKey::TeletextElementWarningExtraPages,
        "skipped-records" => TextKey::TeletextElementWarningSkippedRecords,
        "enhancements" => TextKey::TeletextElementWarningEnhancements,
        "subpages" => TextKey::TeletextElementWarningSubpages,
        "language" => TextKey::TeletextElementWarningLanguage,
        _ => return None,
    }))
}

/// Field sources in display order with their labels.
fn source_options() -> [(&'static str, TextKey); 10] {
    [
        ("unset", TextKey::TeletextElementSourceUnset),
        ("static", TextKey::TeletextElementSourceStatic),
        ("channel_name", TextKey::TeletextElementSourceChannelName),
        (
            "channel_number",
            TextKey::TeletextElementSourceChannelNumber,
        ),
        ("date", TextKey::TeletextElementSourceDate),
        ("time", TextKey::TeletextElementSourceTime),
        ("now_title", TextKey::TeletextElementSourceNowTitle),
        ("now_start", TextKey::TeletextElementSourceNowStart),
        ("next_title", TextKey::TeletextElementSourceNextTitle),
        ("next_start", TextKey::TeletextElementSourceNextStart),
    ]
}

fn load(id: i64, subpage: u16, channel: Option<i64>) -> Result<Option<EditorData>, ()> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        super::data::load_editor(id, subpage, channel)
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (id, subpage, channel);
        Err(())
    }
}

#[component]
pub fn TeletextElementEditorPage() -> impl IntoView {
    let params = use_params_map();
    let query = use_query_map();
    let id = params
        .with_untracked(|p| p.get("id").and_then(|s| s.parse::<i64>().ok()))
        .unwrap_or_default();
    let (subpage, channel, saved, error, warnings) = query.with_untracked(|q| {
        (
            q.get("subpage")
                .and_then(|value| value.parse::<u16>().ok())
                .map_or(0, |value| value.saturating_sub(1)),
            q.get("channel").and_then(|value| value.parse::<i64>().ok()),
            q.get("saved"),
            q.get("error"),
            q.get("warn").unwrap_or_default(),
        )
    });
    match load(id, subpage, channel) {
        Ok(Some(data)) => editor(&data, saved.as_deref(), error.as_deref(), &warnings).into_any(),
        Ok(None) => view! {
            <div class=style::page>
                <p role="alert" data-teletext-element-not-found>{tr(TextKey::TeletextElementNotFound)}</p>
                <a href="/teletext-elements">{tr(TextKey::TeletextElements)}</a>
            </div>
        }
        .into_any(),
        Err(()) => view! {
            <p role="alert" data-teletext-element-load-error>{tr(TextKey::ErrorOccurred)}</p>
        }
        .into_any(),
    }
}

fn editor(
    data: &EditorData,
    saved: Option<&str>,
    error: Option<&str>,
    warnings: &str,
) -> impl IntoView {
    let unset = data
        .fields
        .iter()
        .filter(|field| field.source == "unset")
        .count();
    let fields_state = if data.fields.is_empty() || unset == 0 {
        TeletextStepState::Done
    } else {
        TeletextStepState::Current
    };
    let usage_state = if !data.usage.is_empty() {
        TeletextStepState::Done
    } else if fields_state == TeletextStepState::Done {
        TeletextStepState::Current
    } else {
        TeletextStepState::Todo
    };
    let steps = step_bar(
        tr(TextKey::TeletextElementSteps),
        vec![
            (
                tr(TextKey::TeletextElementStepUploaded).to_owned(),
                TeletextStepState::Done,
            ),
            (
                tr(TextKey::TeletextElementFieldLinks).to_owned(),
                fields_state,
            ),
            (
                tr(TextKey::TeletextElementStepPutOnChannel).to_owned(),
                usage_state,
            ),
        ],
    );
    let usage_badge = if data.usage.is_empty() {
        status_badge(TeletextTone::Neutral, tr(TextKey::TeletextElementNotUsed))
    } else {
        status_badge(
            TeletextTone::Success,
            count_label(
                u16::try_from(data.usage.len()).unwrap_or(u16::MAX),
                TextKey::TeletextElementOneChannel,
                TextKey::TeletextElementCountChannels,
            ),
        )
    };
    let subtitle = format!(
        "{} · {} · {}",
        data.format,
        count_label(
            data.subpage_count,
            TextKey::TeletextElementOneSubpage,
            TextKey::TeletextElementCountSubpages
        ),
        data.file_name
    );
    let mut messages = Vec::new();
    if let Some(text) = saved.and_then(saved_text) {
        messages.push(
            view! { <p class=style::feedback role="status" data-teletext-element-saved>{text}</p> }
                .into_any(),
        );
    }
    if let Some(code) = error {
        messages.push(view! { <p class=style::alert role="alert" data-teletext-element-error=code.to_owned()>{error_text(code)}</p> }.into_any());
    }
    for text in warnings.split(',').filter_map(warning_text) {
        messages.push(view! { <p class=style::warning role="status" data-teletext-element-warning>{text}</p> }.into_any());
    }
    let preview = preview_column(data);
    view! {
        <div class=style::page data-teletext-elements-page="editor" data-teletext-element-id=data.id.to_string()>
            <a href="/teletext-elements" class=style::back>{format!("‹ {}", tr(TextKey::TeletextElements))}</a>
            <header class=style::header>
                <div>
                    <h1 class=style::title>{data.name.clone()}</h1>
                    <p class=style::subtitle>{subtitle}</p>
                </div>
                {usage_badge}
            </header>
            {steps}
            {messages}
            <div class=style::editor_grid>
                <div class=style::preview_column>{preview}</div>
                <div class=style::settings_column>
                    {fields_island(data)}
                    {details_island(data)}
                    {usage_island(data)}
                    {file_island(data)}
                </div>
            </div>
        </div>
    }
}

fn editor_href(data: &EditorData, subpage: u16) -> String {
    let mut href = format!("/teletext-elements/{}?subpage={}", data.id, subpage + 1);
    if let Some(channel) = data.preview_channel {
        href = format!("{href}&channel={channel}");
    }
    href
}

fn preview_column(data: &EditorData) -> AnyView {
    let caption = format!(
        "{} {} · {} {}/{}",
        tr(TextKey::TeletextElementPageNumber),
        data.page,
        tr(TextKey::TeletextElementSubpage),
        data.subpage + 1,
        data.subpage_count
    );
    let subpages = if data.subpage_count > 1 {
        (0..data.subpage_count)
            .map(|index| TeletextSubpageLink {
                href: editor_href(data, index),
                label: format!("{}/{}", index + 1, data.subpage_count),
                current: index == data.subpage,
            })
            .collect()
    } else {
        Vec::new()
    };
    let options = data
        .channels
        .iter()
        .map(|channel| {
            let selected = data.preview_channel == Some(channel.id);
            view! { <option value=channel.id.to_string() selected=selected>{channel.label.clone()}</option> }
        })
        .collect_view();
    let footer = view! {
        <form method="get" action=format!("/teletext-elements/{}", data.id) class=style::preview_form data-teletext-preview-form>
            <input type="hidden" name="subpage" value=(data.subpage + 1).to_string()/>
            <label>
                <span class=style::label>{tr(TextKey::TeletextElementPreviewWith)}</span>
                <select name="channel">
                    <option value="" selected=data.preview_channel.is_none()>{tr(TextKey::TeletextElementPreviewAsDesigned)}</option>
                    {options}
                </select>
            </label>
            <button type="submit" class=style::secondary>{tr(TextKey::TeletextElementShowPreview)}</button>
            <p class=style::muted>{tr(TextKey::TeletextElementHeaderRowHelp)}</p>
        </form>
    }
    .into_any();
    let label = format!("{} {}", tr(TextKey::Preview), data.name);
    preview_frame(
        &caption,
        teletext_screen(&data.screen, label, false),
        subpages,
        footer,
    )
}

fn field_row(field: &FieldRow) -> impl IntoView {
    let index = field.index;
    let options = source_options()
        .into_iter()
        .map(|(key, label)| {
            view! { <option value=key selected=field.source == key>{tr(label)}</option> }
        })
        .collect_view();
    let position = tr(TextKey::TeletextElementFieldPosition)
        .replace("{row}", &field.row.to_string())
        .replace("{column}", &(field.column + 1).to_string())
        .replace("{length}", &field.length.to_string())
        .replace("{subpage}", &(field.subpage + 1).to_string());
    let state = if field.source == "unset" {
        Some(status_badge(
            TeletextTone::Warning,
            tr(TextKey::TeletextElementFieldChooseData),
        ))
    } else {
        None
    };
    view! {
        <tr data-teletext-field=index.to_string()>
            <th scope="row">
                <span class=style::field_name>{field.name.clone()}</span>
                <span class=style::muted>{position}</span>
            </th>
            <td>
                <select name=format!("field_{index}_source") aria-label=format!("{} {}", tr(TextKey::TeletextElementFieldData), field.name)>
                    {options}
                </select>
                <input
                    type="text"
                    name=format!("field_{index}_text")
                    value=field.static_text.clone()
                    maxlength=field.length.to_string()
                    placeholder=tr(TextKey::TeletextElementStaticTextPlaceholder)
                    aria-label=format!("{} {}", tr(TextKey::TeletextElementSourceStatic), field.name)
                    class=style::static_text
                />
                {state}
            </td>
            <td class=style::example>{field.example.clone().unwrap_or_default()}</td>
        </tr>
    }
}

fn fields_island(data: &EditorData) -> AnyView {
    let body = if data.fields.is_empty() {
        view! {
            <div class=style::empty_inline data-teletext-fields-empty>
                <p><strong>{tr(TextKey::TeletextElementNoFields)}</strong></p>
                <p class=style::muted>{tr(TextKey::TeletextElementFieldMarkerTip)}</p>
            </div>
        }
        .into_any()
    } else {
        let rows = data.fields.iter().map(field_row).collect_view();
        view! {
            <form method="post" action=format!("/admin-api/teletext-elements/{}/fields", data.id) data-teletext-fields-form>
                <input type="hidden" name="revision" value=data.revision.to_string()/>
                <input type="hidden" name="preview_channel" value=data.preview_channel.map(|c| c.to_string()).unwrap_or_default()/>
                <div class=style::table_wrap>
                    <table class=style::fields_table>
                        <thead><tr>
                            <th>{tr(TextKey::TeletextElementField)}</th>
                            <th>{tr(TextKey::TeletextElementFieldData)}</th>
                            <th>{tr(TextKey::Preview)}</th>
                        </tr></thead>
                        <tbody>{rows}</tbody>
                    </table>
                </div>
                <p class=style::muted>{tr(TextKey::TeletextElementFieldsCutHelp)}</p>
                <div class=style::form_actions>
                    <button type="submit" class=style::primary>{tr(TextKey::TeletextElementSaveFieldLinks)}</button>
                </div>
            </form>
        }
        .into_any()
    };
    island(TeletextIsland {
        test_id: "teletext-element-fields",
        title: tr(TextKey::TeletextElementFieldLinks).to_owned(),
        badge: None,
        help: Some(tr(TextKey::TeletextElementFieldLinksHelp).to_owned()),
        data_id: None,
        body,
    })
}

fn details_island(data: &EditorData) -> AnyView {
    let body = view! {
        <form method="post" action=format!("/admin-api/teletext-elements/{}/details", data.id) class=style::form data-teletext-details-form>
            <input type="hidden" name="revision" value=data.revision.to_string()/>
            <label>
                <span class=style::label>{tr(TextKey::Name)}</span>
                <input type="text" name="name" value=data.name.clone() required maxlength="100"/>
            </label>
            <label>
                <span class=style::label>{tr(TextKey::TeletextElementSuggestedPageNumber)}</span>
                <input type="text" name="page_number" value=data.page.clone() inputmode="numeric" pattern="[1-8][0-9][0-9]" size="4" required/>
                <span class=style::muted>{tr(TextKey::TeletextElementSuggestedPageNumberHelp)}</span>
            </label>
            <label>
                <span class=style::label>{tr(TextKey::TeletextElementSubpageCycle)}</span>
                <input type="number" name="cycle_seconds" value=data.cycle_seconds.to_string() min="1" max="600" required/>
                <span class=style::muted>{tr(TextKey::TeletextElementSubpageCycleHelp)}</span>
            </label>
            <div class=style::form_actions>
                <button type="submit" class=style::primary>{tr(TextKey::TeletextElementSaveDetails)}</button>
            </div>
        </form>
    }
    .into_any();
    island(TeletextIsland {
        test_id: "teletext-element-details",
        title: tr(TextKey::TeletextElementDetails).to_owned(),
        badge: None,
        help: None,
        data_id: None,
        body,
    })
}

fn usage_island(data: &EditorData) -> AnyView {
    let body = if data.usage.is_empty() {
        view! { <p class=style::muted data-teletext-usage-empty>{tr(TextKey::TeletextElementUsageEmpty)}</p> }.into_any()
    } else {
        let rows = data
            .usage
            .iter()
            .map(|usage| {
                let state = if usage.enabled {
                    status_badge(TeletextTone::Success, tr(TextKey::TeletextElementLinked))
                } else {
                    status_badge(TeletextTone::Neutral, tr(TextKey::TeletextElementOff))
                };
                view! {
                    <li>
                        <a href=format!("/channels/{}/teletext#custom-pages", usage.channel_id)>{usage.label.clone()}</a>
                        " → "
                        {format!("{} {}", tr(TextKey::TeletextElementPageNumber), usage.page)}
                        " "
                        {state}
                    </li>
                }
            })
            .collect_view();
        view! { <ul class=style::usage_list data-teletext-usage>{rows}</ul> }.into_any()
    };
    island(TeletextIsland {
        test_id: "teletext-element-usage",
        title: tr(TextKey::TeletextElementUsedOn).to_owned(),
        badge: None,
        help: Some(tr(TextKey::TeletextElementUsedOnHelp).to_owned()),
        data_id: None,
        body,
    })
}

fn file_island(data: &EditorData) -> AnyView {
    let id = data.id;
    let delete_warning = if data.usage.is_empty() {
        tr(TextKey::TeletextElementDeleteConfirm).to_owned()
    } else {
        tr(TextKey::TeletextElementDeleteConfirmUsed)
            .replace("{count}", &data.usage.len().to_string())
    };
    let body = view! {
        <div class=style::form_actions>
            <a href=format!("/admin-api/teletext-elements/{id}/download") class=style::secondary data-teletext-download download>
                {tr(TextKey::TeletextElementDownloadTti)}
            </a>
        </div>
        <form method="post" action=format!("/admin-api/teletext-elements/{id}/replace") enctype="multipart/form-data" class=style::form data-teletext-replace-form>
            <input type="hidden" name="revision" value=data.revision.to_string()/>
            <label>
                <span class=style::label>{tr(TextKey::TeletextElementNewVersion)}</span>
                <input type="file" name="file" accept=ACCEPT required/>
                <span class=style::muted>{tr(TextKey::TeletextElementNewVersionHelp)}</span>
            </label>
            <div class=style::form_actions>
                <button type="submit" class=style::secondary>{tr(TextKey::TeletextElementUploadNewVersion)}</button>
            </div>
        </form>
        <details class=style::details data-teletext-advanced>
            <summary>{tr(TextKey::TeletextElementAdvanced)}</summary>
            <dl class=style::facts>
                <dt>{tr(TextKey::TeletextElementSourceFormat)}</dt><dd>{data.format}</dd>
                <dt>{tr(TextKey::TeletextElementFileName)}</dt><dd>{data.file_name.clone()}</dd>
                <dt>{tr(TextKey::TeletextElementCharacterSet)}</dt><dd>{format!("0x{:02X}", data.national_option)}</dd>
                <dt>{tr(TextKey::TeletextElementEnhancementPackets)}</dt><dd>{data.enhancement_packets.to_string()}</dd>
                <dt>{tr(TextKey::TeletextElementRevision)}</dt><dd>{data.revision.to_string()}</dd>
                <dt>{tr(TextKey::TeletextElementUpdatedAt)}</dt><dd>{data.updated_at.clone()}</dd>
            </dl>
            <p class=style::muted>{tr(TextKey::TeletextElementLevel1PreviewNote)}</p>
        </details>
        <form method="post" action=format!("/admin-api/teletext-elements/{id}/delete") class=style::danger_form data-teletext-delete-form>
            <label class=style::check>
                <input type="checkbox" name="confirm" value="yes" required/>
                {delete_warning}
            </label>
            <button type="submit" class=style::danger>{tr(TextKey::TeletextElementDeleteElement)}</button>
        </form>
    }
    .into_any();
    island(TeletextIsland {
        test_id: "teletext-element-file",
        title: tr(TextKey::TeletextElementFile).to_owned(),
        badge: None,
        help: Some(tr(TextKey::TeletextElementFileHelp).to_owned()),
        data_id: None,
        body,
    })
}
