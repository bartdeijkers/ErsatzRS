//! Page data for the teletext element library, loaded during SSR.

use super::screen::ScreenView;

/// One library row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementRow {
    pub id: i64,
    pub name: String,
    pub format: &'static str,
    pub page: String,
    pub subpages: u16,
    pub fields: u16,
    pub unset_fields: u16,
    pub channels: u16,
    pub thumbnail: Option<ScreenView>,
}

/// A selectable data source for a field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldRow {
    pub index: usize,
    pub name: String,
    pub subpage: u16,
    pub row: u8,
    pub column: u8,
    pub length: u8,
    pub source: &'static str,
    pub static_text: String,
    /// The value the preview shows for this field, if a channel is chosen.
    pub example: Option<String>,
}

/// A channel option for the preview selector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelOption {
    pub id: i64,
    pub label: String,
}

/// A channel that shows this element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageRow {
    pub channel_id: i64,
    pub label: String,
    pub page: String,
    pub enabled: bool,
}

/// Everything the editor page shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorData {
    pub id: i64,
    pub name: String,
    pub revision: i64,
    pub format: &'static str,
    pub file_name: String,
    pub page: String,
    pub subpage_count: u16,
    pub subpage: u16,
    pub cycle_seconds: u16,
    pub updated_at: String,
    pub national_option: u8,
    pub enhancement_packets: usize,
    pub fields: Vec<FieldRow>,
    pub channels: Vec<ChannelOption>,
    pub preview_channel: Option<i64>,
    pub usage: Vec<UsageRow>,
    pub screen: ScreenView,
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) use native::*;

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::{ChannelOption, EditorData, ElementRow, FieldRow, UsageRow};
    use crate::admin::pages::teletext_elements::screen::screen_view;
    use crate::host::state::AppState;
    use ersatzrs_app::teletext_elements::{fields::FieldValues, teletext_element_page};
    use ersatzrs_domain::teletext_element_page_label;
    use leptos::prelude::use_context;

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(future))
    }

    /// Library rows with a thumbnail of each element's first subpage.
    pub(crate) fn load_rows() -> Result<Vec<ElementRow>, ()> {
        let state = use_context::<AppState>().ok_or(())?;
        block_on(async {
            let summaries = state
                .application
                .list_teletext_elements()
                .await
                .map_err(|_| ())?;
            let mut rows = Vec::with_capacity(summaries.len());
            for summary in summaries {
                let thumbnail = state
                    .application
                    .get_teletext_element(summary.id)
                    .await
                    .ok()
                    .flatten()
                    .and_then(|element| teletext_element_page(&element).ok())
                    .and_then(|page| {
                        page.subpages
                            .first()
                            .map(|subpage| screen_view(subpage, page.page, 0, &[]))
                    });
                rows.push(ElementRow {
                    id: summary.id,
                    name: summary.name,
                    format: summary.source_format.label(),
                    page: teletext_element_page_label(summary.page_number),
                    subpages: summary.subpage_count,
                    fields: summary.binding_count,
                    unset_fields: summary.unset_binding_count,
                    channels: summary.channel_count,
                    thumbnail,
                });
            }
            Ok(rows)
        })
    }

    fn example(
        values: Option<&FieldValues>,
        binding: &ersatzrs_domain::TeletextElementBinding,
    ) -> Option<String> {
        values
            .and_then(|values| values.text_for(binding))
            .map(|text| {
                let safe = ersatzrs_app::teletext_elements::fields::level1_safe_text(text);
                String::from_utf8_lossy(&safe[..safe.len().min(usize::from(binding.length))])
                    .into_owned()
            })
    }

    /// Editor data; `None` when the element does not exist.
    pub(crate) fn load_editor(
        id: i64,
        subpage: u16,
        preview_channel: Option<i64>,
    ) -> Result<Option<EditorData>, ()> {
        let state = use_context::<AppState>().ok_or(())?;
        block_on(async {
            let Some(element) = state
                .application
                .get_teletext_element(id)
                .await
                .map_err(|_| ())?
            else {
                return Ok(None);
            };
            let channels: Vec<ChannelOption> = state
                .application
                .ports()
                .channels()
                .list_number_rows()
                .await
                .map(|rows| {
                    rows.into_iter()
                        .map(|row| ChannelOption {
                            id: row.id,
                            label: format!("{} · {}", row.number, row.name),
                        })
                        .collect()
                })
                .unwrap_or_default();
            let preview_channel =
                preview_channel.filter(|channel| channels.iter().any(|c| c.id == *channel));
            let preview = state
                .application
                .preview_teletext_element(id, preview_channel, chrono::Utc::now())
                .await
                .map_err(|_| ())?;
            let page = teletext_element_page(&element).map_err(|_| ())?;
            let count = u16::try_from(preview.page.subpages.len())
                .unwrap_or(1)
                .max(1);
            let subpage = subpage.min(count - 1);
            let shown = preview.page.subpages.get(usize::from(subpage)).ok_or(())?;
            let screen = screen_view(shown, element.page_number, subpage, &element.bindings);
            let usage = state
                .application
                .teletext_element_usage(id)
                .await
                .unwrap_or_default()
                .into_iter()
                .map(|usage| UsageRow {
                    channel_id: usage.channel_id,
                    label: format!("{} · {}", usage.channel_number, usage.channel_name),
                    page: teletext_element_page_label(usage.page_number),
                    enabled: usage.enabled,
                })
                .collect();
            let fields = element
                .bindings
                .iter()
                .enumerate()
                .map(|(index, binding)| FieldRow {
                    index,
                    name: binding.field.clone(),
                    subpage: binding.subpage,
                    row: binding.row,
                    column: binding.column,
                    length: binding.length,
                    source: binding.source.key(),
                    static_text: binding.static_text.clone(),
                    example: example(preview.values.as_ref(), binding),
                })
                .collect();
            let first = page.subpages.first();
            Ok(Some(EditorData {
                id: element.id,
                name: element.name,
                revision: element.revision,
                format: element.source_format.label(),
                file_name: element.source_file_name,
                page: teletext_element_page_label(element.page_number),
                subpage_count: count,
                subpage,
                cycle_seconds: element.cycle_seconds,
                updated_at: element.updated_at,
                national_option: first.map_or(
                    0,
                    ersatzrs_app::teletext_elements::model::ElementSubpage::charset_designation,
                ),
                enhancement_packets: page.subpages.iter().map(|s| s.packets.len()).sum(),
                fields,
                channels,
                preview_channel,
                usage,
                screen,
            }))
        })
    }
}
