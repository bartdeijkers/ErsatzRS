//! Host-owned producer `ersatzrs.host.custom-pages` (ADR 0120 amendment of
//! 2026-11-07).
//!
//! Each enabled channel link contributes one page with all its subpages. Rows
//! 1–24 come from the element with bound fields filled; row 0 stays the
//! channel's header. Enhancement packets are not carried. A page that cannot
//! be produced blocks the whole Check with a per-page reason; nothing is
//! silently dropped, truncated or given precedence.

use std::collections::BTreeMap;
use std::time::Instant;

use chrono::{DateTime, Utc};
use ersatzrs_addon_contract as wire;
use ersatzrs_domain::{
    is_teletext_custom_pages_producer, teletext_element_page_label, EntityId,
    TeletextCustomPageWire, TeletextCustomPagesWire, TeletextCustomRowWire, TeletextElementBinding,
    TeletextElementBindingSource, TeletextProfileValue, TeletextSelectionSnapshot,
    TELETEXT_CUSTOM_PAGES_SCHEMA,
};

use super::fields::{apply_field_values, find_field_markers};
use super::model::ElementPage;
use super::{teletext_element_page, TeletextElement, TeletextElementError};
use crate::teletext_profiles::{TeletextProfileRuntimeError, TeletextProfileRuntimeReceipt};
use crate::ApplicationServices;

/// Why one linked page cannot be broadcast. Shown per page in the admin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CustomPageIssueKind {
    /// The element is gone or its stored page can no longer be read.
    Unreadable,
    /// The page number uses hexadecimal digits that receivers cannot select.
    PageNumber,
    /// A field marker has no data source, so marker text would be broadcast.
    UnsetField,
    /// Another enabled custom page on this channel uses the same number.
    DuplicatePage,
    /// A double-height line hides text on the row below it, or sits on row
    /// 23 or 24 where it has no room. Receivers would not show those bytes,
    /// so the page is refused instead of silently dropping them.
    DoubleHeight,
    /// The page lies inside the declared range of an enabled add-on.
    AddonRange { addon_id: String },
}

/// One blocked page, named by its link and display page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomPageIssue {
    pub link_id: EntityId,
    pub element_id: EntityId,
    pub element_name: String,
    /// Display page, e.g. "150".
    pub page: String,
    pub kind: CustomPageIssueKind,
}

/// Outcome of preparing a channel's custom pages at one moment.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CustomPagesPlan {
    pub pages: Vec<TeletextCustomPageWire>,
    pub issues: Vec<CustomPageIssue>,
}

impl CustomPagesPlan {
    /// True when every enabled link can be broadcast.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.issues.is_empty()
    }
}

/// Subcode for subpage `index` of `count`: a single page uses 0000, a
/// rotating page numbers its subpages 0001, 0002, … in the usual BCD form.
fn subcode(index: usize, count: usize) -> u16 {
    if count <= 1 {
        return 0;
    }
    let number = u16::try_from(index + 1).unwrap_or(u16::MAX).min(79);
    ((number / 10) << 4) | (number % 10)
}

/// Decimal display page of a stored page number, or `None` for hexadecimal
/// pages (for example 1A0) that receivers cannot select by number.
fn display_page(stored: u16) -> Option<u16> {
    let label = teletext_element_page_label(stored);
    label
        .parse::<u16>()
        .ok()
        .filter(|page| (100..=899).contains(page))
}

/// True when a double-height control (or a double-size control on row 23
/// or 24) would hide or cut off coded text, matching the encoder's
/// no-hidden-row rule for rows 1–24.
fn hides_text_under_double_height(rows: &[[u8; 40]]) -> bool {
    (1..=24_usize).any(|row| {
        let cells = &rows[row];
        let double_height = cells.contains(&0x0d);
        if !double_height && !cells.contains(&0x0f) {
            return false;
        }
        row >= 23 || (double_height && rows[row + 1].iter().any(|&code| code != b' '))
    })
}

/// True when a field marker has no data source, or a binding is left unset,
/// so marker text would be broadcast.
fn has_unset_field(page: &ElementPage, element: &TeletextElement) -> bool {
    find_field_markers(page).iter().any(|marker| {
        !element.bindings.iter().any(|binding| {
            binding.source != TeletextElementBindingSource::Unset
                && binding.subpage == marker.subpage
                && binding.row == marker.row
                && binding.column <= marker.column
                && binding.column + binding.length >= marker.column + marker.length
        })
    }) || element
        .bindings
        .iter()
        .any(|binding| binding.source == TeletextElementBindingSource::Unset)
}

/// Freeze only this subpage's refreshable fields, without obsolete fixed text.
fn dynamic_fields(
    bindings: &[TeletextElementBinding],
    subpage: usize,
) -> Vec<TeletextElementBinding> {
    bindings
        .iter()
        .filter(|field| usize::from(field.subpage) == subpage && field.source.is_dynamic())
        .cloned()
        .map(|mut field| {
            field.subpage = 0;
            field.static_text.clear();
            field
        })
        .collect()
}

/// Declared page ranges of the enabled add-ons in a selection.
fn addon_ranges(bindings: &[TeletextSelectionSnapshot]) -> Vec<(String, u16, u16)> {
    let literal = |binding: &TeletextSelectionSnapshot, key: &str| {
        binding.effective_settings.iter().find_map(|setting| {
            match (&setting.value, setting.key == key) {
                (TeletextProfileValue::Literal(value), true) => value.trim().parse::<u16>().ok(),
                _ => None,
            }
        })
    };
    bindings
        .iter()
        .filter(|binding| !is_teletext_custom_pages_producer(&binding.addon_id))
        .filter_map(|binding| {
            let start = literal(binding, "PAGE_START")?;
            let end = literal(binding, "PAGE_END").unwrap_or(start).max(start);
            Some((binding.addon_id.clone(), start, end))
        })
        .collect()
}

impl ApplicationServices {
    /// Prepare the enabled custom pages of one channel with fields filled for
    /// `now`, and list every page that blocks Check.
    /// # Errors
    /// Storage failures and a missing library or channel.
    pub async fn plan_teletext_custom_pages(
        &self,
        channel_id: EntityId,
        now: DateTime<Utc>,
    ) -> Result<CustomPagesPlan, TeletextElementError> {
        let links: Vec<_> = self
            .channel_teletext_elements(channel_id)
            .await?
            .into_iter()
            .filter(|link| link.enabled)
            .collect();
        if links.is_empty() {
            return Ok(CustomPagesPlan::default());
        }
        let values = self
            .teletext_element_field_values(channel_id, now)
            .await?
            .ok_or(TeletextElementError::NotFound)?;
        // Without a readable selection the add-on side fails Check by itself.
        let ranges = self
            .observe_channel_teletext_document_selection(channel_id)
            .await
            .map(|selection| addon_ranges(&selection.bindings))
            .unwrap_or_default();
        let mut uses: BTreeMap<u16, usize> = BTreeMap::new();
        for link in &links {
            *uses.entry(link.effective_page_number).or_default() += 1;
        }
        let mut plan = CustomPagesPlan::default();
        for link in &links {
            let label = teletext_element_page_label(link.effective_page_number);
            let issue = |kind| CustomPageIssue {
                link_id: link.id,
                element_id: link.element_id,
                element_name: link.element_name.clone(),
                page: label.clone(),
                kind,
            };
            let Some(page_number) = display_page(link.effective_page_number) else {
                plan.issues.push(issue(CustomPageIssueKind::PageNumber));
                continue;
            };
            if uses.get(&link.effective_page_number).copied().unwrap_or(0) > 1 {
                plan.issues.push(issue(CustomPageIssueKind::DuplicatePage));
                continue;
            }
            if let Some((addon_id, _, _)) = ranges
                .iter()
                .find(|(_, start, end)| (*start..=*end).contains(&page_number))
            {
                plan.issues.push(issue(CustomPageIssueKind::AddonRange {
                    addon_id: addon_id.clone(),
                }));
                continue;
            }
            let element = match self.get_teletext_element(link.element_id).await {
                Ok(Some(element)) => element,
                Ok(None) | Err(TeletextElementError::Storage) => {
                    plan.issues.push(issue(CustomPageIssueKind::Unreadable));
                    continue;
                }
                Err(error) => return Err(error),
            };
            let Ok(page) = teletext_element_page(&element) else {
                plan.issues.push(issue(CustomPageIssueKind::Unreadable));
                continue;
            };
            if has_unset_field(&page, &element) {
                plan.issues.push(issue(CustomPageIssueKind::UnsetField));
                continue;
            }
            let filled = apply_field_values(&page, &element.bindings, &values);
            if filled
                .subpages
                .iter()
                .any(|subpage| hides_text_under_double_height(&subpage.rows))
            {
                plan.issues.push(issue(CustomPageIssueKind::DoubleHeight));
                continue;
            }
            let count = filled.subpages.len();
            for (index, subpage) in filled.subpages.iter().enumerate() {
                plan.pages.push(TeletextCustomPageWire {
                    element_id: element.id,
                    element_revision: element.revision,
                    page: page_number,
                    subcode: subcode(index, count),
                    national_option: subpage.national_option(),
                    rows: (1..=24_u8)
                        .map(|row| TeletextCustomRowWire::new(row, &subpage.rows[usize::from(row)]))
                        .collect(),
                    dynamic_fields: dynamic_fields(&element.bindings, index),
                });
            }
        }
        Ok(plan)
    }

    /// Run the host producer for one invocation of the channel selection.
    /// The request supplies the correlation and the moment fields describe.
    /// # Errors
    /// Rejects an invalid request, an expired deadline and any blocked page.
    pub(crate) async fn produce_teletext_custom_pages(
        &self,
        selection: TeletextSelectionSnapshot,
        input: &[u8],
        deadline: Instant,
    ) -> Result<TeletextProfileRuntimeReceipt, TeletextProfileRuntimeError> {
        if !is_teletext_custom_pages_producer(&selection.addon_id) {
            return Err(TeletextProfileRuntimeError::Unsupported);
        }
        let request = wire::TeletextPagesRequest::from_json_for_schema(
            input,
            selection.contract.schema_version(),
        )
        .map_err(|_| TeletextProfileRuntimeError::InvalidRequest)?;
        if Instant::now() >= deadline {
            return Err(TeletextProfileRuntimeError::TimedOut);
        }
        let plan = self
            .plan_teletext_custom_pages(selection.channel_id, request.captured_at)
            .await
            .map_err(|_| TeletextProfileRuntimeError::IoFailed)?;
        if Instant::now() >= deadline {
            return Err(TeletextProfileRuntimeError::TimedOut);
        }
        if !plan.is_ready() || plan.pages.is_empty() {
            return Err(TeletextProfileRuntimeError::Failed);
        }
        let stdout = serde_json::to_vec(&TeletextCustomPagesWire {
            schema: TELETEXT_CUSTOM_PAGES_SCHEMA.to_owned(),
            generation: request.generation.clone(),
            pages: plan.pages,
        })
        .map_err(|_| TeletextProfileRuntimeError::Failed)?;
        Ok(TeletextProfileRuntimeReceipt {
            stdout,
            selection,
            generation: request.generation,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_published_subpage_carries_only_its_own_normalized_dynamic_bindings() {
        let field = |subpage, source| TeletextElementBinding {
            field: "field".to_owned(),
            subpage,
            row: 1,
            column: 2,
            length: 8,
            source,
            static_text: "older static value".to_owned(),
        };
        let bindings = vec![
            field(0, TeletextElementBindingSource::NowTitle),
            field(1, TeletextElementBindingSource::Time),
            field(1, TeletextElementBindingSource::ChannelName),
            field(1, TeletextElementBindingSource::Static),
        ];
        let saved = dynamic_fields(&bindings, 1);
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].source, TeletextElementBindingSource::Time);
        assert_eq!(saved[0].subpage, 0);
        assert_eq!((saved[0].row, saved[0].column, saved[0].length), (1, 2, 8));
        assert!(saved[0].static_text.is_empty());
        assert_eq!(bindings[1].subpage, 1);
        assert_eq!(bindings[1].static_text, "older static value");
        assert!(dynamic_fields(&bindings, 2).is_empty());
    }

    #[test]
    fn subcodes_follow_rotating_page_convention() {
        assert_eq!(subcode(0, 1), 0x0000);
        assert_eq!(subcode(0, 3), 0x0001);
        assert_eq!(subcode(9, 12), 0x0010);
        assert_eq!(subcode(11, 12), 0x0012);
    }

    #[test]
    fn text_under_a_double_height_line_is_detected() {
        let mut rows = [[b' '; 40]; 26];
        rows[1][0] = 0x0d;
        assert!(!hides_text_under_double_height(&rows));
        rows[2][5] = b'X';
        assert!(hides_text_under_double_height(&rows));
        let mut rows = [[b' '; 40]; 26];
        rows[23][0] = 0x0f;
        assert!(hides_text_under_double_height(&rows));
    }

    #[test]
    fn hexadecimal_pages_are_not_display_pages() {
        assert_eq!(display_page(0x150), Some(150));
        assert_eq!(display_page(0x1a0), None);
        assert_eq!(display_page(0x0ff), None);
    }
}
