//! Native option forms carry one sparse command, never a draft snapshot.

use crate::host::state::AppState;
use ersatzrs_addon_contract::{AddonMediaListOptionDefinition, AddonMediaListOptionScope};
use ersatzrs_app::addon_media_list_options::{
    AddonListOptionEdit, AddonListOptionPreviewRow, AddonListOptionTarget,
};

// Accommodate a URL-encoded one-key undo receipt for 10,000 bounded enum values.
// No whole schema, metadata or draft snapshot is accepted by these forms.
pub const MAX_OPTION_FORM_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_OPTION_ITEMS: usize = 10_000;

pub struct OptionField {
    pub definition: AddonMediaListOptionDefinition,
    pub edit: AddonListOptionEdit,
    pub rows: Vec<AddonListOptionPreviewRow>,
}

pub struct OptionEditor {
    pub access: Option<ItemAccessEditor>,
    pub list_id: i64,
    pub provisional: bool,
    pub bulk: bool,
    pub off_page_count: usize,
    pub titles: std::collections::BTreeMap<i64, String>,
    pub fields: Vec<OptionField>,
}

pub struct ItemAccessEditor {
    pub item_id: i64,
    pub saved_revision: i32,
    pub status: Result<Option<ersatzrs_app::ports::AddonItemAccessRead>, ()>,
}

/// Resolve only explicit identities. Empty means list defaults, not all items.
///
/// # Errors
/// Returns a static diagnostic for malformed, repeated, or out-of-range identities.
pub fn parse_option_items(value: &str) -> Result<Vec<i64>, &'static str> {
    if value.is_empty() {
        return Ok(Vec::new());
    }
    let ids = value
        .split(',')
        .map(str::parse::<i64>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "invalid item identity")?;
    if ids.is_empty()
        || ids.len() > MAX_OPTION_ITEMS
        || ids.iter().any(|id| *id <= 0)
        || ids.iter().collect::<std::collections::BTreeSet<_>>().len() != ids.len()
    {
        return Err("invalid item selection");
    }
    Ok(ids)
}

pub async fn load_option_editor(
    state: &AppState,
    list_id: i64,
    ids: Vec<i64>,
) -> Result<Option<OptionEditor>, ()> {
    let Some(editor) = state
        .application
        .addon_list_option_editor(list_id)
        .await
        .map_err(|_| ())?
    else {
        return Ok(None);
    };
    let access = if editor.supports_access_check && ids.len() == 1 {
        Some(ItemAccessEditor {
            item_id: ids[0],
            saved_revision: editor.draft.draft_revision,
            status: state
                .application
                .media_list_access_check(list_id, ids[0])
                .await
                .map_err(|_| ()),
        })
    } else {
        None
    };
    let bulk = ids.len() > 1;
    let selected_ids = ids
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let titles = if ids.is_empty() {
        std::collections::BTreeMap::new()
    } else {
        state
            .application
            .list_addon_media_list_items(list_id)
            .await
            .map_err(|_| ())?
            .into_iter()
            .filter(|item| selected_ids.contains(&item.id))
            .map(|item| (item.id, item.display_title))
            .collect()
    };
    let target = if ids.is_empty() {
        AddonListOptionTarget::ListDefault
    } else {
        AddonListOptionTarget::Items(ids)
    };
    let mut fields = Vec::new();
    for definition in editor.definition.options {
        if matches!(
            (&target, definition.scope),
            (
                AddonListOptionTarget::ListDefault,
                AddonMediaListOptionScope::ItemOverride
            ) | (
                AddonListOptionTarget::Items(_),
                AddonMediaListOptionScope::ListDefault
            )
        ) {
            continue;
        }
        let edit = AddonListOptionEdit {
            list_id,
            expected_revision: editor.draft.draft_revision,
            schema: editor.schema.schema.clone(),
            key: definition.key.clone(),
            target: target.clone(),
            value: None,
        };
        // The application owns effective-value resolution and applicability.
        let mut probe = edit.clone();
        probe.value = Some(
            serde_json::from_value(serde_json::to_value(&definition.default).map_err(|_| ())?)
                .map_err(|_| ())?,
        );
        let preview = state
            .application
            .preview_addon_list_option_edit(probe)
            .await
            .map_err(|_| ())?;
        fields.push(OptionField {
            definition,
            edit,
            rows: preview.rows,
        });
    }
    Ok(Some(OptionEditor {
        access,
        list_id,
        provisional: editor.draft.schema.is_none(),
        bulk,
        off_page_count: 0,
        titles,
        fields,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn option_scope_is_explicit_bounded_and_unique() {
        assert_eq!(parse_option_items(""), Ok(vec![]));
        assert_eq!(parse_option_items("8,3"), Ok(vec![8, 3]));
        for invalid in ["all", "current_page", "1,1", "0", "-1", "1,"] {
            assert!(parse_option_items(invalid).is_err(), "{invalid}");
        }
        assert!(parse_option_items(
            &(1..=10_001)
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(",")
        )
        .is_err());
    }
}
