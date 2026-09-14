//! ADR 0085 option editing: sparse inheritance, exact-revision bulk apply and undo.
//!
//! Preview is read-only. Explicit option edits or publication/preparation bind
//! provisional package defaults to a legacy draft; published options are never
//! written by these commands. Automatic refresh does not adopt legacy defaults.

use std::collections::{BTreeMap, BTreeSet};

use ersatzrs_addon_contract::{
    AddonCapability, AddonMediaListOptionApplicability, AddonMediaListOptionDefinition,
    AddonMediaListOptionValue as WireValue, AddonMediaListOptionValueScope,
    AddonMediaListOptionValues as WireValues,
};
use ersatzrs_core::repositories::{
    AddonMediaListDraftOptions, AddonMediaListItemOptionValues, AddonMediaListOptionItem,
    AddonMediaListOptionSchema, AddonMediaListOptionSchemaRef,
};
use ersatzrs_domain::{AddonMediaListOptionValue, EntityId, RemoteStreamLiveness};
use serde::{Deserialize, Serialize};

use crate::{ApplicationError, ApplicationServices, Result};

const MAX_OPTION_EDIT_ITEMS: usize = 10_000;

/// Read-only editor state. An unbound draft's displayed defaults are provisional.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonListOptionEditor {
    pub supports_access_check: bool,
    pub draft: AddonMediaListDraftOptions,
    pub schema: AddonMediaListOptionSchema,
    pub definition: AddonCapability,
}

/// Exact scope resolved by the UI before preview; never a mutable filter expression.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AddonListOptionTarget {
    ListDefault,
    Items(Vec<EntityId>),
}

/// One sparse option change. `None` restores inheritance by removing the key.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonListOptionEdit {
    pub list_id: EntityId,
    pub expected_revision: i32,
    pub schema: AddonMediaListOptionSchemaRef,
    pub key: String,
    pub target: AddonListOptionTarget,
    pub value: Option<AddonMediaListOptionValue>,
}

/// A single preview row, including unchanged or finite-only unavailable targets.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonListOptionPreviewRow {
    pub item_id: Option<EntityId>,
    pub before: Option<AddonMediaListOptionValue>,
    pub after: Option<AddonMediaListOptionValue>,
    pub effective_before: AddonMediaListOptionValue,
    pub effective_after: AddonMediaListOptionValue,
    pub applicable: bool,
}

/// Read-only preview retaining the exact command and revision to apply.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonListOptionPreview {
    pub edit: AddonListOptionEdit,
    pub rows: Vec<AddonListOptionPreviewRow>,
}

/// Sparse pre-change values retained by one browser session, not durable history.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonListOptionUndo {
    pub list_id: EntityId,
    pub expected_revision: i32,
    pub schema: AddonMediaListOptionSchemaRef,
    /// Undo first-time activation as well as the edited sparse value.
    pub previous_schema: Option<AddonMediaListOptionSchemaRef>,
    pub key: String,
    pub previous: Vec<AddonListOptionPreviousValue>,
}

/// Previous sparse value for either the list default or an explicitly selected item.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonListOptionPreviousValue {
    pub item_id: Option<EntityId>,
    pub value: Option<AddonMediaListOptionValue>,
}

impl ApplicationServices {
    /// Bind immutable package defaults only for an explicitly admitted preparation.
    pub(crate) async fn activate_addon_list_preparation_defaults(
        &self,
        command: &crate::commands::PublishAddonMediaList,
    ) -> Result<i32> {
        let lifecycle = self.ports().addon_media_list_lifecycle();
        let Some(draft) = lifecycle.draft_options(command.id, None).await? else {
            return Err(conflict("the saved draft is unavailable"));
        };
        if draft.schema.is_some() {
            return Ok(command.saved_revision);
        }
        if !crate::commands::validate_addon_media_list_publication(
            command,
            self.ports().addon_media_lists(),
            self.ports().managed_library_operations(),
        )
        .await?
        {
            return Ok(command.saved_revision);
        }
        let Some(editor) = self.addon_list_option_editor(command.id).await? else {
            return Ok(command.saved_revision);
        };
        let Some(draft) = prepare_default_activation(&editor, command.saved_revision)? else {
            return Ok(command.saved_revision);
        };
        lifecycle.register_option_schema(&editor.schema).await?;
        lifecycle
            .save_draft_options(&draft)
            .await
            .map_err(Into::into)
    }

    /// Queue item access using explicitly saved options, never provisional defaults.
    pub async fn queue_media_list_access_check(
        &self,
        list_id: i64,
        item_id: i64,
        expected_revision: i32,
    ) -> Result<i64> {
        self.validate_access_item(list_id, item_id).await?;
        let draft = self
            .ports()
            .addon_media_list_lifecycle()
            .draft_options(list_id, None)
            .await?
            .ok_or_else(|| conflict("the saved draft is unavailable"))?;
        if expected_revision < 0
            || draft.draft_revision != expected_revision
            || draft.schema.is_none()
        {
            return Err(conflict(
                "save the current draft options before checking access",
            ));
        }
        self.ports()
            .addons()
            .queue_media_list_access_check(list_id, item_id, expected_revision)
            .await
            .map_err(|_| access_unavailable())
    }

    /// Read bounded, redacted access evidence for an item owned by this list.
    pub async fn media_list_access_check(
        &self,
        list_id: i64,
        item_id: i64,
    ) -> Result<Option<crate::ports::AddonItemAccessRead>> {
        self.validate_access_item(list_id, item_id).await?;
        self.ports()
            .addons()
            .media_list_access_check(list_id, item_id)
            .await
            .map_err(|_| access_unavailable())
    }

    /// Cancel one owned durable access job without changing draft options.
    pub async fn cancel_media_list_access_check(
        &self,
        list_id: i64,
        item_id: i64,
        job_id: i64,
    ) -> Result<()> {
        self.validate_access_item(list_id, item_id).await?;
        if job_id <= 0 {
            return Err(conflict("invalid item access job"));
        }
        self.ports()
            .addons()
            .cancel_media_list_access_check(list_id, item_id, job_id)
            .await
            .map_err(|_| access_unavailable())
    }

    async fn validate_access_item(&self, list_id: i64, item_id: i64) -> Result<()> {
        if list_id <= 0 || item_id <= 0 {
            return Err(conflict("invalid item access owner"));
        }
        let items = self
            .ports()
            .addon_media_list_lifecycle()
            .option_items(list_id, &[item_id])
            .await?;
        if items.len() != 1 || items[0].item_id != item_id {
            return Err(conflict("invalid item access owner"));
        }
        Ok(())
    }

    /// Load schema-rendered draft options without saving or activating defaults.
    pub async fn addon_list_option_editor(
        &self,
        list_id: EntityId,
    ) -> Result<Option<AddonListOptionEditor>> {
        let form = self
            .ports()
            .addon_media_lists()
            .get_admin_form(list_id)
            .await?
            .ok_or(ApplicationError::NotFound {
                kind: "add-on media list",
                id: list_id,
            })?;
        let lifecycle = self.ports().addon_media_list_lifecycle();
        let draft =
            lifecycle
                .draft_options(list_id, None)
                .await?
                .ok_or(ApplicationError::NotFound {
                    kind: "add-on media list",
                    id: list_id,
                })?;
        let installed = self
            .ports()
            .addons()
            .media_list_option_schema(&form.addon_id)
            .await
            .map_err(|_| ApplicationError::Unavailable {
                operation: "read installed option schema",
                message: "the installed package option schema is unavailable".into(),
            })?;
        let Some(installed) = installed else {
            if draft.schema.is_some() {
                return Err(conflict(
                    "the saved option schema is unavailable in the installed package",
                ));
            }
            return Ok(None);
        };
        let schema = if let Some(saved) = &draft.schema {
            if saved != &installed.schema {
                return Err(conflict(
                    "the installed option schema differs from the saved draft",
                ));
            }
            lifecycle
                .option_schema(saved)
                .await?
                .ok_or_else(|| conflict("the saved option schema is missing"))?
        } else {
            installed
        };
        let definition = serde_json::from_str(&schema.definition_json)
            .map_err(|_| conflict("the saved option schema is invalid"))?;
        Ok(Some(AddonListOptionEditor {
            supports_access_check: self
                .ports()
                .addons()
                .supports_media_list_access_check(&form.addon_id)
                .await
                .map_err(|_| access_unavailable())?,
            draft,
            schema,
            definition,
        }))
    }

    /// Preview one list-default or explicit item-selection change without writes.
    pub async fn preview_addon_list_option_edit(
        &self,
        edit: AddonListOptionEdit,
    ) -> Result<AddonListOptionPreview> {
        let (editor, items) = self.load_option_edit(&edit).await?;
        let (_, rows) = prepare_edit(&editor, &items, &edit)?;
        Ok(AddonListOptionPreview { edit, rows })
    }

    /// Validate several visible controls without persisting intermediate values.
    pub async fn prepare_addon_list_option_edits(
        &self,
        edits: Vec<AddonListOptionEdit>,
    ) -> Result<(Option<AddonMediaListDraftOptions>, Vec<AddonListOptionUndo>)> {
        let mut undo = Vec::new();
        let mut prepared: Option<AddonMediaListDraftOptions> = None;
        for edit in edits {
            let (mut editor, items) = self.load_option_edit(&edit).await?;
            let previous_schema = editor.draft.schema.clone();
            if let Some(draft) = &prepared {
                if draft.list_id != edit.list_id || draft.draft_revision != edit.expected_revision {
                    return Err(conflict("option controls must share one list revision"));
                }
                editor.draft = draft.clone();
            }
            let (draft, rows) = prepare_edit(&editor, &items, &edit)?;
            if !rows.iter().any(|row| row.applicable) {
                return Err(conflict("the option does not apply to any selected item"));
            }
            self.ports()
                .addon_media_list_lifecycle()
                .register_option_schema(&editor.schema)
                .await?;
            undo.push(AddonListOptionUndo {
                list_id: edit.list_id,
                expected_revision: edit.expected_revision.saturating_add(1),
                schema: edit.schema,
                previous_schema,
                key: edit.key,
                previous: rows
                    .into_iter()
                    .filter(|r| r.applicable)
                    .map(|r| AddonListOptionPreviousValue {
                        item_id: r.item_id,
                        value: r.before,
                    })
                    .collect(),
            });
            prepared = Some(draft);
        }
        Ok((prepared, undo))
    }

    /// Undo a coherent set of option controls once, including initial activation.
    pub async fn undo_addon_list_option_task(
        &self,
        receipts: Vec<AddonListOptionUndo>,
    ) -> Result<i32> {
        let first = receipts
            .first()
            .ok_or_else(|| conflict("missing option undo receipt"))?;
        let previous_schema = first.previous_schema.clone();
        let mut prepared: Option<AddonMediaListDraftOptions> = None;
        for receipt in &receipts {
            if receipt.list_id != first.list_id
                || receipt.expected_revision != first.expected_revision
                || receipt.previous_schema != previous_schema
                || receipt.schema != first.schema
            {
                return Err(conflict(
                    "option undo receipts must share one task revision",
                ));
            }
            let edit = undo_edit(receipt)?;
            let (mut editor, items) = self.load_option_edit(&edit).await?;
            if let Some(draft) = prepared {
                editor.draft = draft;
            }
            let mut staged = receipt.clone();
            staged.previous_schema = Some(receipt.schema.clone());
            prepared = Some(prepare_undo(&editor, &items, &staged)?);
        }
        let mut draft = prepared.ok_or_else(|| conflict("missing option undo values"))?;
        draft.schema = previous_schema;
        if draft.schema.is_none()
            && (!draft.list_values.is_empty() || !draft.item_values.is_empty())
        {
            return Err(conflict("option undo cannot leave values without a schema"));
        }
        self.ports()
            .addon_media_list_lifecycle()
            .save_draft_options(&draft)
            .await
            .map_err(Into::into)
    }

    /// Apply an already-previewed command atomically; intervening edits fail closed.
    pub async fn apply_addon_list_option_edit(
        &self,
        edit: AddonListOptionEdit,
    ) -> Result<AddonListOptionUndo> {
        let (editor, items) = self.load_option_edit(&edit).await?;
        let (draft, rows) = prepare_edit(&editor, &items, &edit)?;
        if !rows.iter().any(|row| row.applicable) {
            return Err(conflict("the option does not apply to any selected item"));
        }
        let lifecycle = self.ports().addon_media_list_lifecycle();
        lifecycle.register_option_schema(&editor.schema).await?;
        let revision = lifecycle.save_draft_options(&draft).await?;
        Ok(AddonListOptionUndo {
            list_id: edit.list_id,
            expected_revision: revision,
            schema: edit.schema,
            previous_schema: editor.draft.schema,
            key: edit.key,
            previous: rows
                .into_iter()
                .filter(|row| row.applicable)
                .map(|row| AddonListOptionPreviousValue {
                    item_id: row.item_id,
                    value: row.before,
                })
                .collect(),
        })
    }

    /// Restore sparse values once, provided nothing has changed since their apply.
    pub async fn undo_addon_list_option_edit(&self, undo: AddonListOptionUndo) -> Result<i32> {
        let edit = undo_edit(&undo)?;
        let (editor, items) = self.load_option_edit(&edit).await?;
        let draft = prepare_undo(&editor, &items, &undo)?;
        // Never substitute a freshly read revision for the apply receipt's revision.
        self.ports()
            .addon_media_list_lifecycle()
            .save_draft_options(&draft)
            .await
            .map_err(Into::into)
    }

    async fn load_option_edit(
        &self,
        edit: &AddonListOptionEdit,
    ) -> Result<(AddonListOptionEditor, Vec<AddonMediaListOptionItem>)> {
        validate_target(&edit.target)?;
        let editor = self
            .addon_list_option_editor(edit.list_id)
            .await?
            .ok_or_else(|| conflict("the package does not declare list options"))?;
        check_fence(&editor, edit)?;
        let items = match &edit.target {
            AddonListOptionTarget::ListDefault => Vec::new(),
            AddonListOptionTarget::Items(ids) => {
                self.ports()
                    .addon_media_list_lifecycle()
                    .option_items(edit.list_id, ids)
                    .await?
            }
        };
        Ok((editor, items))
    }
}

fn undo_edit(undo: &AddonListOptionUndo) -> Result<AddonListOptionEdit> {
    validate_undo_targets(&undo.previous)?;
    Ok(AddonListOptionEdit {
        list_id: undo.list_id,
        expected_revision: undo.expected_revision,
        schema: undo.schema.clone(),
        key: undo.key.clone(),
        target: if undo.previous[0].item_id.is_none() {
            AddonListOptionTarget::ListDefault
        } else {
            AddonListOptionTarget::Items(
                undo.previous.iter().filter_map(|row| row.item_id).collect(),
            )
        },
        value: None,
    })
}

fn prepare_undo(
    editor: &AddonListOptionEditor,
    items: &[AddonMediaListOptionItem],
    undo: &AddonListOptionUndo,
) -> Result<AddonMediaListDraftOptions> {
    let edit = undo_edit(undo)?;
    check_fence(editor, &edit)?;
    if undo
        .previous_schema
        .as_ref()
        .is_some_and(|schema| schema != &undo.schema)
    {
        return Err(conflict("option undo cannot replace the saved schema"));
    }
    let definition = find_definition(editor, &edit)?;
    let mut draft = editor.draft.clone();
    for previous in &undo.previous {
        validate_value(
            &editor.definition,
            &definition,
            previous.item_id.is_some(),
            previous.value.as_ref(),
        )?;
        patch_value(
            &mut draft,
            items,
            &edit.key,
            previous.item_id,
            previous.value.clone(),
        )?;
    }
    draft.schema.clone_from(&undo.previous_schema);
    if draft.schema.is_none() && (!draft.list_values.is_empty() || !draft.item_values.is_empty()) {
        return Err(conflict("option undo cannot leave values without a schema"));
    }
    Ok(draft)
}

fn check_fence(editor: &AddonListOptionEditor, edit: &AddonListOptionEdit) -> Result<()> {
    if editor.draft.list_id != edit.list_id
        || editor.draft.draft_revision != edit.expected_revision
        || editor.schema.schema != edit.schema
    {
        return Err(conflict(
            "the option preview is stale; preview the change again",
        ));
    }
    Ok(())
}

fn validate_target(target: &AddonListOptionTarget) -> Result<()> {
    if let AddonListOptionTarget::Items(ids) = target {
        if ids.is_empty()
            || ids.len() > MAX_OPTION_EDIT_ITEMS
            || ids.iter().collect::<BTreeSet<_>>().len() != ids.len()
        {
            return Err(conflict("select between one and 10000 distinct items"));
        }
    }
    Ok(())
}

fn validate_undo_targets(previous: &[AddonListOptionPreviousValue]) -> Result<()> {
    if previous.is_empty()
        || previous.len() > MAX_OPTION_EDIT_ITEMS
        || previous
            .iter()
            .map(|row| row.item_id)
            .collect::<BTreeSet<_>>()
            .len()
            != previous.len()
        || (previous.len() != 1 && previous.iter().any(|row| row.item_id.is_none()))
    {
        return Err(conflict("the option undo has invalid targets"));
    }
    Ok(())
}

fn find_definition(
    editor: &AddonListOptionEditor,
    edit: &AddonListOptionEdit,
) -> Result<AddonMediaListOptionDefinition> {
    editor
        .definition
        .options
        .iter()
        .find(|definition| definition.key == edit.key)
        .cloned()
        .ok_or_else(|| conflict("the option is not declared by the saved schema"))
}

fn validate_value(
    schema: &AddonCapability,
    definition: &AddonMediaListOptionDefinition,
    item: bool,
    value: Option<&AddonMediaListOptionValue>,
) -> Result<()> {
    // Validate scope on reset too, using the declaration's own valid default.
    let value = value.map_or_else(|| definition.default.clone(), to_wire);
    schema
        .validate_stored_option_values(
            &WireValues {
                schema: schema.id.clone(),
                values: BTreeMap::from([(definition.key.clone(), value)]),
            },
            if item {
                AddonMediaListOptionValueScope::ItemOverride
            } else {
                AddonMediaListOptionValueScope::ListDefault
            },
        )
        .map_err(|_| ApplicationError::Validation {
            field: "options",
            message: "the option value or scope is invalid".into(),
        })
}

fn prepare_edit(
    editor: &AddonListOptionEditor,
    items: &[AddonMediaListOptionItem],
    edit: &AddonListOptionEdit,
) -> Result<(AddonMediaListDraftOptions, Vec<AddonListOptionPreviewRow>)> {
    check_fence(editor, edit)?;
    validate_target(&edit.target)?;
    let definition = find_definition(editor, edit)?;
    validate_value(
        &editor.definition,
        &definition,
        matches!(edit.target, AddonListOptionTarget::Items(_)),
        edit.value.as_ref(),
    )?;
    let default = from_wire(&definition.default);
    let inherited = editor
        .draft
        .list_values
        .get(&edit.key)
        .cloned()
        .unwrap_or_else(|| default.clone());
    let mut draft = editor.draft.clone();
    draft.schema = Some(editor.schema.schema.clone());
    let targets = match &edit.target {
        AddonListOptionTarget::ListDefault => vec![None],
        AddonListOptionTarget::Items(ids) => ids.iter().copied().map(Some).collect(),
    };
    let mut rows = Vec::with_capacity(targets.len());
    for item_id in targets {
        let (before, fallback, applicable) = if let Some(id) = item_id {
            let item = items
                .iter()
                .find(|item| item.item_id == id)
                .ok_or_else(|| conflict("the selected option item is no longer available"))?;
            let before = draft
                .item_values
                .iter()
                .find(|item| item.item_id == Some(id))
                .and_then(|item| item.values.get(&edit.key))
                .cloned();
            (
                before,
                inherited.clone(),
                edit.value.is_none()
                    || definition.applicability != AddonMediaListOptionApplicability::FiniteOnly
                    || item.liveness == RemoteStreamLiveness::Finite,
            )
        } else {
            (
                draft.list_values.get(&edit.key).cloned(),
                default.clone(),
                true,
            )
        };
        let after = if applicable {
            edit.value.clone()
        } else {
            before.clone()
        };
        rows.push(AddonListOptionPreviewRow {
            item_id,
            effective_before: before.clone().unwrap_or_else(|| fallback.clone()),
            effective_after: after.clone().unwrap_or(fallback),
            before,
            after: after.clone(),
            applicable,
        });
        if applicable {
            patch_value(&mut draft, items, &edit.key, item_id, after)?;
        }
    }
    Ok((draft, rows))
}

fn patch_value(
    draft: &mut AddonMediaListDraftOptions,
    items: &[AddonMediaListOptionItem],
    key: &str,
    item_id: Option<EntityId>,
    value: Option<AddonMediaListOptionValue>,
) -> Result<()> {
    let values = if let Some(id) = item_id {
        let index = if let Some(index) = draft
            .item_values
            .iter()
            .position(|item| item.item_id == Some(id))
        {
            index
        } else {
            let identity = items
                .iter()
                .find(|item| item.item_id == id)
                .ok_or_else(|| conflict("the selected option item is no longer available"))?;
            draft.item_values.push(AddonMediaListItemOptionValues {
                item_id: Some(id),
                provider_id: identity.provider_id.clone(),
                values: BTreeMap::new(),
            });
            draft.item_values.len() - 1
        };
        &mut draft.item_values[index].values
    } else {
        &mut draft.list_values
    };
    if let Some(value) = value {
        values.insert(key.into(), value);
    } else {
        values.remove(key);
    }
    draft.item_values.retain(|item| !item.values.is_empty());
    Ok(())
}

fn to_wire(value: &AddonMediaListOptionValue) -> WireValue {
    match value {
        AddonMediaListOptionValue::Boolean(value) => WireValue::Boolean(*value),
        AddonMediaListOptionValue::Enum(value) => WireValue::Enum(value.clone()),
    }
}

fn from_wire(value: &WireValue) -> AddonMediaListOptionValue {
    match value {
        WireValue::Boolean(value) => AddonMediaListOptionValue::Boolean(*value),
        WireValue::Enum(value) => AddonMediaListOptionValue::Enum(value.clone()),
    }
}

fn access_unavailable() -> ApplicationError {
    ApplicationError::Unavailable {
        operation: "item access check",
        message: "the item access operation is unavailable".into(),
    }
}

fn conflict(message: &str) -> ApplicationError {
    ApplicationError::Conflict(message.into())
}

fn prepare_default_activation(
    editor: &AddonListOptionEditor,
    expected_revision: i32,
) -> Result<Option<AddonMediaListDraftOptions>> {
    // The editor read may race another save. Never adopt its newer fence.
    if editor.draft.draft_revision != expected_revision {
        return Err(conflict("the saved draft changed before preparation"));
    }
    if editor.draft.schema.is_some() {
        return Ok(None);
    }
    if !editor.draft.list_values.is_empty() || !editor.draft.item_values.is_empty() {
        return Err(conflict(
            "unbound draft options cannot be adopted as defaults",
        ));
    }
    let mut draft = editor.draft.clone();
    draft.schema = Some(editor.schema.schema.clone());
    Ok(Some(draft))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preparation_defaults_bind_only_an_exact_unbound_empty_draft() {
        let (mut editor, _, _) = fixture();
        let revision = editor.draft.draft_revision;
        assert!(prepare_default_activation(&editor, revision)
            .unwrap()
            .is_none());
        editor.draft.schema = None;
        assert!(prepare_default_activation(&editor, revision).is_err());
        editor.draft.list_values.clear();
        editor.draft.item_values.clear();
        assert!(prepare_default_activation(&editor, revision - 1).is_err());
        let activated = prepare_default_activation(&editor, revision)
            .unwrap()
            .unwrap();
        assert_eq!(activated.schema, Some(editor.schema.schema.clone()));
        assert_eq!(activated.draft_revision, revision);
        assert!(activated.list_values.is_empty());
        assert!(activated.item_values.is_empty());
        assert!(editor.draft.schema.is_none());
        editor.draft = activated;
        assert!(prepare_default_activation(&editor, revision)
            .unwrap()
            .is_none());
    }

    fn fixture() -> (
        AddonListOptionEditor,
        Vec<AddonMediaListOptionItem>,
        AddonListOptionEdit,
    ) {
        let definition: AddonCapability = serde_json::from_value(serde_json::json!({
            "id": "media-list.options.v1",
            "options": [{
                "key": "remove_segments", "kind": "boolean", "scope": "both",
                "label": {"en-US": "Remove segments"},
                "description": {"en-US": "Prepare finite media."},
                "default": true, "operation_targets": ["preparation"],
                "applicability": "finite_only", "requires_preparation": true
            }, {
                "key": "browser", "kind": "enum", "scope": "both",
                "label": {"en-US": "Browser"}, "description": {"en-US": "Selected browser."},
                "default": "none", "operation_targets": ["test_access", "playback"],
                "applicability": "all", "choices": [
                    {"value": "none", "label": {"en-US": "None"}},
                    {"value": "firefox", "label": {"en-US": "Firefox"}}
                ]
            }]
        }))
        .unwrap();
        let schema = AddonMediaListOptionSchema {
            schema: AddonMediaListOptionSchemaRef {
                addon_id: "org.example.options".into(),
                capability_id: definition.id.clone(),
                schema_digest: "fixture-digest".into(),
            },
            definition_json: serde_json::to_string(&definition).unwrap(),
            created_at: chrono::Utc::now(),
        };
        let edit = AddonListOptionEdit {
            list_id: 1,
            expected_revision: 7,
            schema: schema.schema.clone(),
            key: "remove_segments".into(),
            target: AddonListOptionTarget::Items(vec![10, 20]),
            value: Some(AddonMediaListOptionValue::Boolean(false)),
        };
        let editor = AddonListOptionEditor {
            supports_access_check: false,
            definition,
            schema: schema.clone(),
            draft: AddonMediaListDraftOptions {
                list_id: 1,
                draft_revision: 7,
                schema: Some(schema.schema),
                list_values: BTreeMap::new(),
                item_values: vec![AddonMediaListItemOptionValues {
                    item_id: Some(30),
                    provider_id: "item-30".into(),
                    values: BTreeMap::from([(
                        "browser".into(),
                        AddonMediaListOptionValue::Enum("firefox".into()),
                    )]),
                }],
            },
        };
        let items = vec![
            AddonMediaListOptionItem {
                item_id: 10,
                provider_id: "item-10".into(),
                liveness: RemoteStreamLiveness::Finite,
            },
            AddonMediaListOptionItem {
                item_id: 20,
                provider_id: "item-20".into(),
                liveness: RemoteStreamLiveness::Live,
            },
        ];
        (editor, items, edit)
    }

    #[test]
    fn option_preview_preserves_unrelated_overrides_and_marks_live_unavailable() {
        let (editor, items, edit) = fixture();
        let original = editor.clone();
        let (draft, rows) = prepare_edit(&editor, &items, &edit).unwrap();
        assert_eq!(editor, original, "preview must not mutate the loaded draft");
        assert_eq!(draft.draft_revision, edit.expected_revision);
        assert!(rows[0].applicable);
        assert_eq!(rows[0].before, None);
        assert_eq!(
            rows[0].effective_before,
            AddonMediaListOptionValue::Boolean(true)
        );
        assert!(!rows[1].applicable);
        assert_eq!(rows[1].before, rows[1].after);
        assert_eq!(draft.item_values[0], editor.draft.item_values[0]);
        assert!(!draft.item_values.iter().any(|row| row.item_id == Some(20)));
    }

    #[test]
    fn option_preview_can_explain_an_entirely_inapplicable_selection() {
        let (editor, items, mut edit) = fixture();
        edit.target = AddonListOptionTarget::Items(vec![20]);
        let (draft, rows) = prepare_edit(&editor, &items, &edit).unwrap();
        assert_eq!(rows.len(), 1);
        assert!(!rows[0].applicable);
        assert_eq!(draft, editor.draft);
    }

    #[test]
    fn option_reset_and_undo_restore_sparse_inheritance_exactly() {
        let (editor, items, mut edit) = fixture();
        edit.target = AddonListOptionTarget::Items(vec![10]);
        let (mut draft, rows) = prepare_edit(&editor, &items, &edit).unwrap();
        assert_eq!(draft.item_values.len(), 2);
        patch_value(
            &mut draft,
            &items,
            &edit.key,
            Some(10),
            rows[0].before.clone(),
        )
        .unwrap();
        assert_eq!(draft, editor.draft);
        edit.value = None;
        let (_, reset_rows) = prepare_edit(&editor, &items, &edit).unwrap();
        assert_eq!(reset_rows[0].after, None);
        assert_eq!(
            reset_rows[0].effective_after,
            AddonMediaListOptionValue::Boolean(true)
        );
    }

    #[test]
    fn option_edits_reject_stale_fences_duplicates_and_unknown_values() {
        let (editor, items, edit) = fixture();
        let mut stale = edit.clone();
        stale.expected_revision -= 1;
        assert!(prepare_edit(&editor, &items, &stale).is_err());
        stale = edit.clone();
        stale.schema.schema_digest = "different".into();
        assert!(prepare_edit(&editor, &items, &stale).is_err());
        let mut duplicate = edit.clone();
        duplicate.target = AddonListOptionTarget::Items(vec![10, 10]);
        assert!(prepare_edit(&editor, &items, &duplicate).is_err());
        let mut invalid = edit;
        invalid.key = "browser".into();
        invalid.value = Some(AddonMediaListOptionValue::Enum("undeclared".into()));
        assert!(prepare_edit(&editor, &items, &invalid).is_err());
        assert!(validate_undo_targets(&[]).is_err());
        assert!(validate_undo_targets(&[
            AddonListOptionPreviousValue {
                item_id: None,
                value: None
            },
            AddonListOptionPreviousValue {
                item_id: Some(10),
                value: None
            },
        ])
        .is_err());
    }

    #[test]
    fn option_legacy_defaults_are_only_bound_in_the_prepared_save() {
        let (mut editor, items, mut edit) = fixture();
        editor.draft.schema = None;
        editor.draft.item_values.clear();
        edit.target = AddonListOptionTarget::ListDefault;
        edit.value = None;
        let (draft, rows) = prepare_edit(&editor, &items, &edit).unwrap();
        assert_eq!(editor.draft.schema, None);
        assert_eq!(draft.schema, Some(edit.schema));
        assert_eq!(
            rows[0].effective_after,
            AddonMediaListOptionValue::Boolean(true)
        );
        assert!(draft.list_values.is_empty());
    }

    #[test]
    fn option_undo_reverses_first_schema_activation_and_rejects_intervening_edit() {
        let (mut editor, items, mut edit) = fixture();
        editor.draft.schema = None;
        editor.draft.item_values.clear();
        edit.key = "browser".into();
        edit.target = AddonListOptionTarget::Items(vec![10]);
        edit.value = Some(AddonMediaListOptionValue::Enum("firefox".into()));
        let original = editor.draft.clone();
        let (mut saved, rows) = prepare_edit(&editor, &items, &edit).unwrap();
        saved.draft_revision += 1;
        let undo = AddonListOptionUndo {
            list_id: edit.list_id,
            expected_revision: saved.draft_revision,
            schema: edit.schema,
            previous_schema: editor.draft.schema.clone(),
            key: edit.key,
            previous: rows
                .into_iter()
                .map(|row| AddonListOptionPreviousValue {
                    item_id: row.item_id,
                    value: row.before,
                })
                .collect(),
        };
        editor.draft = saved;
        let restored = prepare_undo(&editor, &items, &undo).unwrap();
        assert_eq!(restored.schema, original.schema);
        assert_eq!(restored.list_values, original.list_values);
        assert_eq!(restored.item_values, original.item_values);
        assert_eq!(restored.draft_revision, undo.expected_revision);
        editor.draft.draft_revision += 1;
        assert!(prepare_undo(&editor, &items, &undo).is_err());
    }
}
