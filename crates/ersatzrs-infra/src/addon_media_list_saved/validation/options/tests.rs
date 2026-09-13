use super::*;
use ersatzrs_core::repositories::AddonMediaListOptionSchemaRef;
use ersatzrs_domain::AddonMediaListOptionValue;
use serde_json::json;
use sha2::{Digest as _, Sha256};
use std::fmt::Write as _;

use crate::addon_media_list_saved::model::SavedOptionSchema;

const ADDON_ID: &str = "org.example.savedoptions";

fn fixture() -> (SavedOptions, AddonManifest) {
    let definition = json!({
        "id": "media-list.options.v1",
        "options": [{
            "key": "enabled", "label": {"en-US": "Enabled"},
            "description": {"en-US": "Enable processing"}, "scope": "list_default",
            "operation_targets": ["preparation"], "applicability": "finite_only",
            "kind": "boolean", "default": true
        }, {
            "key": "browser", "label": {"en-US": "Browser"},
            "description": {"en-US": "Browser family"}, "scope": "item_override",
            "operation_targets": ["playback"], "applicability": "all",
            "kind": "enum", "default": "none",
            "choices": [{"value": "none", "label": {"en-US": "None"}},
                {"value": "firefox", "label": {"en-US": "Firefox"}}]
        }]
    });
    let installed: AddonManifest = serde_json::from_value(json!({
        "manifest_version": 5, "id": ADDON_ID, "version": "1.0.0",
        "name": {"en-US": "Saved options fixture"},
        "summary": {"en-US": "Synthetic option declaration"},
        "license": "MIT", "source_url": "https://example.invalid/source",
        "host_version": "*", "entrypoints": {}, "capabilities": [definition]
    }))
    .unwrap();
    let snapshot = installed
        .media_list_option_schema_snapshot()
        .unwrap()
        .unwrap();
    let mut schema_digest = String::new();
    for byte in Sha256::digest(snapshot.as_bytes()) {
        write!(schema_digest, "{byte:02x}").unwrap();
    }
    let saved = SavedOptions {
        schema: Some(SavedOptionSchema {
            identity: AddonMediaListOptionSchemaRef {
                addon_id: ADDON_ID.into(),
                capability_id: "media-list.options.v1".into(),
                schema_digest,
            },
            definition: serde_json::from_str(&snapshot).unwrap(),
        }),
        list_values: [("enabled".into(), AddonMediaListOptionValue::Boolean(false))].into(),
        item_values: [(
            "retained_absent_item".into(),
            [(
                "browser".into(),
                AddonMediaListOptionValue::Enum("firefox".into()),
            )]
            .into(),
        )]
        .into(),
    };
    (saved, installed)
}

#[test]
fn validates_sparse_values_and_retained_item_owners_without_mutation() {
    let (saved, installed) = fixture();
    let snapshot = installed
        .media_list_option_schema_snapshot()
        .unwrap()
        .unwrap();
    // Value's object ordering differs from the typed declaration's field order.
    // A receiver must still accept the original canonical snapshot digest.
    assert_ne!(
        serde_json::to_string(&saved.schema.as_ref().unwrap().definition).unwrap(),
        snapshot
    );
    let before = saved.clone();
    assert!(validate_options(&saved, ADDON_ID, &installed).is_ok());
    assert_eq!(saved, before);
    let mut sparse = saved;
    sparse.list_values.clear();
    for values in sparse.item_values.values_mut() {
        values.clear();
    }
    let before = sparse.clone();
    assert!(validate_options(&sparse, ADDON_ID, &installed).is_ok());
    assert_eq!(sparse, before);
}

#[test]
fn rejects_invalid_types_unknown_keys_enums_and_owner_scopes() {
    let (saved, installed) = fixture();
    for (key, value) in [
        ("enabled", AddonMediaListOptionValue::Enum("false".into())),
        ("unknown", AddonMediaListOptionValue::Boolean(true)),
        ("browser", AddonMediaListOptionValue::Enum("firefox".into())),
    ] {
        let mut invalid = saved.clone();
        invalid.list_values = [(key.into(), value)].into();
        assert!(validate_options(&invalid, ADDON_ID, &installed).is_err());
    }
    for (key, value) in [
        ("browser", AddonMediaListOptionValue::Boolean(true)),
        (
            "browser",
            AddonMediaListOptionValue::Enum("undeclared".into()),
        ),
        ("enabled", AddonMediaListOptionValue::Boolean(true)),
    ] {
        let mut invalid = saved.clone();
        invalid.item_values = [("item".into(), [(key.into(), value)].into())].into();
        assert!(validate_options(&invalid, ADDON_ID, &installed).is_err());
    }
}

#[test]
fn rejects_wrong_addon_schema_and_digest() {
    let (saved, installed) = fixture();
    assert!(validate_options(&saved, "org.example.other", &installed).is_err());
    let mut other_installed = installed.clone();
    other_installed.id = "org.example.other".into();
    assert!(validate_options(&saved, ADDON_ID, &other_installed).is_err());
    for field in ["addon", "capability", "digest"] {
        let mut invalid = saved.clone();
        let identity = &mut invalid.schema.as_mut().unwrap().identity;
        match field {
            "addon" => identity.addon_id = "org.example.other".into(),
            "capability" => identity.capability_id = "media-list.options.v2".into(),
            _ => identity.schema_digest = "0".repeat(64),
        }
        assert!(validate_options(&invalid, ADDON_ID, &installed).is_err());
    }
}

#[test]
fn preserves_unbound_empty_options_and_rejects_unbound_values() {
    let (saved, installed) = fixture();
    let empty = SavedOptions::default();
    assert!(validate_options(&empty, ADDON_ID, &installed).is_ok());
    assert_eq!(empty, SavedOptions::default());
    let mut invalid = saved;
    invalid.schema = None;
    assert!(validate_options(&invalid, ADDON_ID, &installed).is_err());
    invalid.list_values.clear();
    assert!(validate_options(&invalid, ADDON_ID, &installed).is_err());
    for values in invalid.item_values.values_mut() {
        values.clear();
    }
    let before = invalid.clone();
    assert!(validate_options(&invalid, ADDON_ID, &installed).is_ok());
    assert_eq!(invalid, before);
}

#[test]
fn rejects_changed_missing_or_invalid_declarations() {
    let (saved, installed) = fixture();
    let mut missing = installed.clone();
    missing.capabilities.clear();
    assert!(validate_options(&saved, ADDON_ID, &missing).is_err());
    let mut changed = saved.clone();
    changed.schema.as_mut().unwrap().definition["options"][0]["default"] = json!(false);
    assert!(validate_options(&changed, ADDON_ID, &installed).is_err());
    let mut changed_installed = installed.clone();
    changed_installed.capabilities[0].options[0].default =
        ersatzrs_addon_contract::AddonMediaListOptionValue::Boolean(false);
    assert!(validate_options(&saved, ADDON_ID, &changed_installed).is_err());
    let mut invalid = saved;
    invalid.schema.as_mut().unwrap().definition["options"][0]["operation_targets"] = json!([]);
    let mut invalid_installed = installed;
    invalid_installed.capabilities[0] =
        serde_json::from_value(invalid.schema.as_ref().unwrap().definition.clone()).unwrap();
    assert!(validate_options(&invalid, ADDON_ID, &invalid_installed).is_err());
}

#[test]
fn rejects_oversized_value_envelopes() {
    let (mut saved, installed) = fixture();
    saved.list_values.insert(
        "enabled".into(),
        AddonMediaListOptionValue::Enum("x".repeat(32 * 1024)),
    );
    assert!(validate_options(&saved, ADDON_ID, &installed).is_err());
}
