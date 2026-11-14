//! Refresh only published dynamic cells; mutable element/link state is absent.

use std::collections::BTreeSet;

use ersatzrs_domain::{
    TeletextCustomPagesDocument, TeletextCustomPagesError, TeletextCustomPagesWire,
    TeletextCustomRowWire, TeletextElementBindingSource,
};

use super::fields::{level1_safe_text, FieldValues};

/// Actual clock/projection boundaries observed by the native owner.
#[derive(Debug, Clone, Copy, Default)]
pub struct CustomPageRefreshTriggers {
    pub programme: bool,
    pub date: bool,
    pub time: bool,
}

impl CustomPageRefreshTriggers {
    fn includes(self, source: TeletextElementBindingSource) -> bool {
        match source {
            TeletextElementBindingSource::Date => self.date,
            TeletextElementBindingSource::Time => self.time,
            source if source.is_dynamic() => self.programme,
            _ => false,
        }
    }
}

/// Content and numeric diagnostics only; no publication or transport authority.
pub struct RefreshedCustomPages {
    pub document: TeletextCustomPagesWire,
    /// Physical page/subpage rows that actually changed.
    pub changed: Vec<(u16, u16)>,
    /// Logical pages whose dynamic values were blanked after a failed read.
    pub failed_pages: Vec<u16>,
}

/// Fill a frozen published recipe without reading the latest element or link.
/// Missing programme data is represented by empty titles in `values`; `None`
/// means the refresh failed, and blanks every dynamic field on affected pages.
/// Fixed/channel cells, other pages, addresses and element revisions stay exact.
/// # Errors
/// Rejects malformed published rows or field ranges before changing any cell.
pub fn refresh_published_custom_pages(
    original: &TeletextCustomPagesWire,
    triggers: CustomPageRefreshTriggers,
    values: Option<&FieldValues>,
) -> Result<RefreshedCustomPages, TeletextCustomPagesError> {
    TeletextCustomPagesDocument::from_wire(original.clone(), &original.generation)?;
    let affected: BTreeSet<_> = original
        .pages
        .iter()
        .filter(|page| {
            page.dynamic_fields
                .iter()
                .any(|field| triggers.includes(field.source))
        })
        .map(|page| page.page)
        .collect();
    let mut document = original.clone();
    let mut changed = Vec::new();
    for page in &mut document.pages {
        if !affected.contains(&page.page) {
            continue;
        }
        let before = page.rows.clone();
        for field in &page.dynamic_fields {
            if values.is_some() && !triggers.includes(field.source) {
                continue;
            }
            let bad_row = TeletextCustomPagesError::Row {
                page: page.page,
                row: field.row,
            };
            let row = page
                .rows
                .iter_mut()
                .find(|row| row.row == field.row)
                .ok_or(bad_row)?;
            let mut cells = row.decode().ok_or(bad_row)?;
            let text = values
                .and_then(|values| values.text_for(field))
                .unwrap_or("");
            let mut encoded = level1_safe_text(text).into_iter();
            let start = usize::from(field.column);
            let end = start + usize::from(field.length);
            for cell in &mut cells[start..end] {
                *cell = encoded.next().unwrap_or(b' ');
            }
            *row = TeletextCustomRowWire::new(field.row, &cells);
        }
        if page.rows != before {
            changed.push((page.page, page.subcode));
        }
    }
    Ok(RefreshedCustomPages {
        document,
        changed,
        failed_pages: if values.is_none() {
            affected.into_iter().collect()
        } else {
            Vec::new()
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ersatzrs_domain::{
        TeletextCustomPageWire, TeletextElementBinding, TELETEXT_CUSTOM_PAGES_SCHEMA,
    };

    fn page(number: u16, source: TeletextElementBindingSource) -> TeletextCustomPageWire {
        let mut cells = [b' '; 40];
        cells[..6].copy_from_slice(b"\x01OLD!!");
        cells[20..25].copy_from_slice(b"FIXED");
        TeletextCustomPageWire {
            element_id: 7,
            element_revision: 3,
            page: number,
            subcode: 0,
            national_option: 0,
            rows: vec![TeletextCustomRowWire::new(1, &cells)],
            dynamic_fields: vec![TeletextElementBinding {
                field: "published".to_owned(),
                subpage: 0,
                row: 1,
                column: 1,
                length: 5,
                source,
                static_text: String::new(),
            }],
        }
    }

    fn document() -> TeletextCustomPagesWire {
        TeletextCustomPagesWire {
            schema: TELETEXT_CUSTOM_PAGES_SCHEMA.to_owned(),
            generation: "published".to_owned(),
            pages: vec![
                page(150, TeletextElementBindingSource::NowTitle),
                page(151, TeletextElementBindingSource::Time),
                page(152, TeletextElementBindingSource::Date),
            ],
        }
    }

    #[test]
    fn programme_refresh_changes_only_its_published_cells_and_keeps_fixed_rows() {
        let original = document();
        let values = FieldValues {
            now_title: "Één programme".to_owned(),
            time: "12:34".to_owned(),
            ..FieldValues::default()
        };
        let refreshed = refresh_published_custom_pages(
            &original,
            CustomPageRefreshTriggers {
                programme: true,
                ..Default::default()
            },
            Some(&values),
        )
        .unwrap();
        assert_eq!(refreshed.changed, [(150, 0)]);
        assert!(refreshed.failed_pages.is_empty());
        let cells = refreshed.document.pages[0].rows[0].decode().unwrap();
        assert_eq!(&cells[..6], b"\x01Een p");
        assert_eq!(&cells[20..25], b"FIXED");
        assert_eq!(&refreshed.document.pages[1..], &original.pages[1..]);
        assert_eq!(
            refreshed.document.pages[0].dynamic_fields,
            original.pages[0].dynamic_fields
        );
        assert_eq!(refreshed.document.pages[0].element_revision, 3);
        assert_eq!(original.pages[0].rows[0].decode().unwrap()[1], b'O');
    }

    #[test]
    fn no_due_boundary_and_old_frozen_documents_change_nothing() {
        let original = document();
        let unchanged =
            refresh_published_custom_pages(&original, CustomPageRefreshTriggers::default(), None)
                .unwrap();
        assert_eq!(unchanged.document, original);
        assert!(unchanged.changed.is_empty());
        assert!(unchanged.failed_pages.is_empty());
        let mut legacy = original;
        for page in &mut legacy.pages {
            page.dynamic_fields.clear();
        }
        let unchanged = refresh_published_custom_pages(
            &legacy,
            CustomPageRefreshTriggers {
                programme: true,
                date: true,
                time: true,
            },
            None,
        )
        .unwrap();
        assert_eq!(unchanged.document, legacy);
        assert!(unchanged.failed_pages.is_empty());
    }

    #[test]
    fn refresh_failure_blanks_all_dynamic_fields_on_affected_page_and_subpages_only() {
        let mut original = document();
        let mut subpage = page(150, TeletextElementBindingSource::Time);
        subpage.subcode = 1;
        original.pages.push(subpage);
        let refreshed = refresh_published_custom_pages(
            &original,
            CustomPageRefreshTriggers {
                programme: true,
                ..Default::default()
            },
            None,
        )
        .unwrap();
        assert_eq!(refreshed.failed_pages, [150]);
        assert_eq!(refreshed.changed, [(150, 0), (150, 1)]);
        assert_eq!(&refreshed.document.pages[1..3], &original.pages[1..3]);
        for index in [0, 3] {
            let cells = refreshed.document.pages[index].rows[0].decode().unwrap();
            assert_eq!(&cells[..6], b"\x01     ");
            assert_eq!(&cells[20..25], b"FIXED");
        }
        let recovered = refresh_published_custom_pages(
            &refreshed.document,
            CustomPageRefreshTriggers {
                programme: true,
                ..Default::default()
            },
            Some(&FieldValues {
                now_title: "New".to_owned(),
                ..Default::default()
            }),
        )
        .unwrap();
        assert!(recovered.failed_pages.is_empty());
        assert_eq!(
            &recovered.document.pages[0].rows[0].decode().unwrap()[1..6],
            b"New  "
        );
    }

    #[test]
    fn midnight_and_minute_triggers_are_separate_from_programme_refresh() {
        for (triggers, expected) in [
            (
                CustomPageRefreshTriggers {
                    date: true,
                    ..Default::default()
                },
                152,
            ),
            (
                CustomPageRefreshTriggers {
                    time: true,
                    ..Default::default()
                },
                151,
            ),
        ] {
            let result = refresh_published_custom_pages(
                &document(),
                triggers,
                Some(&FieldValues {
                    date: "14-11-2026".to_owned(),
                    time: "00:01".to_owned(),
                    ..Default::default()
                }),
            )
            .unwrap();
            assert_eq!(result.changed, [(expected, 0)]);
        }
    }
}
