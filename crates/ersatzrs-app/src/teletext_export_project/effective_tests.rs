//! Effective-role interchange syntax does not authenticate an uploaded sidecar.

use super::*;

const START: &str = "2026-10-14T10:00:00.123456789Z";
const FINISH: &str = "2026-10-14T11:00:00.987654321+00:00";
const UNTIL: &str = "2026-10-14T10:30:00.456789123Z";

fn effective_project() -> TeletextExportProject {
    let mut project = valid_project();
    project.schema_version = 2;
    project.editor_profile = "qteletextmaker-1.0-carrier-v4".into();
    project.original.document_version = 2;
    project.effective_scope = Some(TeletextExportEffectiveScope {
        binding_count: 4,
        valid_until: Some(UNTIL.into()),
    });
    // Binding positions zero and two are original Off members. They create no
    // invented producer or binding revision in the descriptive editor project.
    project.producers[0].effective_origin =
        Some(TeletextExportEffectiveOrigin::Base { binding_index: 1 });
    let mut scripted = project.producers[0].clone();
    scripted.ordinal = 1;
    scripted.addon_id = "org.ersatzrs.synthetic-scripted".into();
    scripted.profile_id = 5;
    scripted.binding_revision = None;
    scripted.effective_origin = Some(TeletextExportEffectiveOrigin::Scripted {
        binding_index: 3,
        playout_id: 8,
        start: START.into(),
        finish: FINISH.into(),
    });
    project.producers.push(scripted);
    let mut page = project.pages[0].clone();
    page.name = "201-0123.tti".into();
    page.page.page = 201;
    page.editor_page = 0x201;
    page.producer_ordinal = 1;
    project.pages.push(page);
    project
}

#[test]
fn legacy_schema_one_bytes_and_required_positive_binding_revision_are_preserved() {
    let project = valid_project();
    let golden = include_bytes!("fixtures/schema1-project.json");
    assert_eq!(
        write_teletext_export_project(&project, budget()).unwrap(),
        golden.as_slice()
    );
    assert_eq!(
        parse_teletext_export_project(golden, budget()).unwrap(),
        project
    );
    let value: Value = serde_json::from_slice(golden).unwrap();
    assert!(value.get("effective_scope").is_none());
    assert!(value["producers"][0].get("effective_origin").is_none());
    assert_eq!(value["producers"][0]["binding_revision"], json!(2));
    for replacement in [Value::Null, json!(0), json!(-1)] {
        let mut invalid = value.clone();
        invalid["producers"][0]["binding_revision"] = replacement;
        assert!(parse_teletext_export_project(&as_bytes(&invalid), budget()).is_err());
    }
    let mut missing = value;
    missing["producers"][0]
        .as_object_mut()
        .unwrap()
        .remove("binding_revision");
    assert!(parse_teletext_export_project(&as_bytes(&missing), budget()).is_err());
}

#[test]
fn mixed_effective_origins_roundtrip_exact_fractional_utc_strings_and_outer_schema_one() {
    let project = effective_project();
    let encoded = write_teletext_export_project(&project, budget()).unwrap();
    assert_eq!(
        parse_teletext_export_project(&encoded, budget()).unwrap(),
        project
    );
    let value: Value = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(value["effective_scope"]["valid_until"], json!(UNTIL));
    assert_eq!(
        value["producers"][1]["effective_origin"]["start"],
        json!(START)
    );
    assert_eq!(
        value["producers"][1]["effective_origin"]["finish"],
        json!(FINISH)
    );
    assert_eq!(value["producers"][1]["binding_revision"], Value::Null);
    let mut body = receipt();
    body.project = project.clone();
    let mut second = body.files[0].clone();
    second.name = "201-0123.tti".into();
    second.bytes[5] = b'1';
    body.files.push(second);
    let bytes = write_teletext_export_receipt_body(&body, budget()).unwrap();
    assert_eq!(
        parse_teletext_export_receipt_body(&bytes, budget()).unwrap(),
        body
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&bytes).unwrap()["schema_version"],
        json!(1)
    );
    let sidecar = TeletextExportSidecar {
        schema_version: 1,
        receipt_id: 9,
        receipt_digest: "a".repeat(64),
        project,
    };
    let bytes = write_teletext_export_sidecar(&sidecar, budget()).unwrap();
    assert_eq!(
        parse_teletext_export_sidecar(&bytes, budget()).unwrap(),
        sidecar
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&bytes).unwrap()["schema_version"],
        json!(1)
    );
}

#[test]
fn base_only_effective_project_preserves_required_nullable_original_expiry() {
    let mut project = effective_project();
    project.producers.truncate(1);
    project.pages.truncate(1);
    project.effective_scope.as_mut().unwrap().valid_until = None;
    let bytes = write_teletext_export_project(&project, budget()).unwrap();
    assert_eq!(
        parse_teletext_export_project(&bytes, budget()).unwrap(),
        project
    );
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["effective_scope"]["valid_until"], Value::Null);
    let mut missing = value;
    missing["effective_scope"]
        .as_object_mut()
        .unwrap()
        .remove("valid_until");
    assert!(parse_teletext_export_project(&as_bytes(&missing), budget()).is_err());
    let mut scripted = effective_project();
    scripted.effective_scope.as_mut().unwrap().valid_until = None;
    assert!(scripted.validate(budget()).is_err());
}

#[test]
fn effective_projects_require_matching_schema_document_carrier_and_real_origins() {
    let original = serde_json::to_value(effective_project()).unwrap();
    for (pointer, replacement) in [
        ("/schema_version", json!(1)),
        ("/schema_version", json!(3)),
        ("/original/document_version", json!(1)),
        ("/editor_profile", json!("qteletextmaker-1.0")),
        ("/editor_profile", json!("qteletextmaker-1.0-carrier-v3")),
        ("/effective_scope", Value::Null),
        ("/producers/0/effective_origin", Value::Null),
        ("/producers/1/effective_origin/origin", json!("off")),
        ("/producers/0/binding_revision", Value::Null),
        ("/producers/1/binding_revision", json!(1)),
        ("/producers/1/effective_origin/playout_id", json!(0)),
    ] {
        let mut invalid = original.clone();
        *invalid.pointer_mut(pointer).unwrap() = replacement;
        assert!(
            parse_teletext_export_project(&as_bytes(&invalid), budget()).is_err(),
            "{pointer}"
        );
    }
    for (parent, key) in [
        ("", "effective_scope"),
        ("/producers/0", "effective_origin"),
    ] {
        let mut invalid = original.clone();
        invalid
            .pointer_mut(parent)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove(key);
        assert!(parse_teletext_export_project(&as_bytes(&invalid), budget()).is_err());
    }
    let mut legacy = valid_project();
    legacy.producers[0].effective_origin =
        Some(TeletextExportEffectiveOrigin::Base { binding_index: 0 });
    assert!(legacy.validate(budget()).is_err());
}

#[test]
fn scope_expiry_is_inside_every_exact_scripted_interval_without_timezone_or_precision_loss() {
    let original = serde_json::to_value(effective_project()).unwrap();
    for pointer in [
        "/effective_scope/valid_until",
        "/producers/1/effective_origin/start",
        "/producers/1/effective_origin/finish",
    ] {
        for invalid_time in [
            "2026-10-14T10:30:00+01:00",
            "2026-10-14T10:30:00-00:00",
            "2026-10-14T10:30:00.1234567891Z",
            "2026-10-14 10:30:00Z",
            "2026-10-14T10:30:00",
            "2026-13-14T10:30:00Z",
        ] {
            let mut invalid = original.clone();
            *invalid.pointer_mut(pointer).unwrap() = json!(invalid_time);
            assert!(
                parse_teletext_export_project(&as_bytes(&invalid), budget()).is_err(),
                "{pointer}"
            );
        }
    }
    for (pointer, time) in [
        ("/effective_scope/valid_until", START),
        (
            "/effective_scope/valid_until",
            "2026-10-14T11:00:00.987654322Z",
        ),
        ("/producers/1/effective_origin/start", UNTIL),
        ("/producers/1/effective_origin/finish", START),
    ] {
        let mut invalid = original.clone();
        *invalid.pointer_mut(pointer).unwrap() = json!(time);
        assert!(parse_teletext_export_project(&as_bytes(&invalid), budget()).is_err());
    }
    let mut exact_finish = original;
    exact_finish["effective_scope"]["valid_until"] = json!(FINISH);
    assert!(parse_teletext_export_project(&as_bytes(&exact_finish), budget()).is_ok());
    // Syntax retains historical scope; the fresh native operation must establish
    // expiry eligibility, rather than treating a parsed upload as current authority.
}

#[test]
fn complete_original_binding_count_bounds_sorted_unique_real_producer_indices() {
    let original = serde_json::to_value(effective_project()).unwrap();
    for (pointer, replacement) in [
        ("/effective_scope/binding_count", json!(0)),
        ("/effective_scope/binding_count", json!(MAX_BINDINGS + 1)),
        ("/producers/1/effective_origin/binding_index", json!(1)),
        ("/producers/1/effective_origin/binding_index", json!(0)),
        ("/producers/1/effective_origin/binding_index", json!(4)),
    ] {
        let mut invalid = original.clone();
        *invalid.pointer_mut(pointer).unwrap() = replacement;
        assert!(parse_teletext_export_project(&as_bytes(&invalid), budget()).is_err());
    }
    let mut upper = effective_project();
    upper.effective_scope.as_mut().unwrap().binding_count = MAX_BINDINGS;
    if let Some(TeletextExportEffectiveOrigin::Scripted { binding_index, .. }) =
        &mut upper.producers[1].effective_origin
    {
        *binding_index = MAX_BINDINGS - 1;
    }
    assert!(upper.validate(budget()).is_ok());
    let mut unbounded = original;
    let producer = unbounded["producers"][0].clone();
    unbounded["producers"] = json!(vec![producer; MAX_BINDINGS + 1]);
    let mut wider = budget();
    wider.max_bytes = 2 * 1024 * 1024;
    wider.max_producers = MAX_BINDINGS + 1;
    assert_eq!(
        parse_teletext_export_project(&as_bytes(&unbounded), wider)
            .unwrap_err()
            .kind,
        TeletextExportProjectFailure::Limit
    );
}

#[test]
fn effective_objects_retain_closed_map_duplicate_depth_and_redacted_diagnostic_rules() {
    let original = serde_json::to_value(effective_project()).unwrap();
    for pointer in [
        "/effective_scope",
        "/producers/0/effective_origin",
        "/producers/1/effective_origin",
    ] {
        for replacement in [Value::Null, json!([])] {
            let mut invalid = original.clone();
            *invalid.pointer_mut(pointer).unwrap() = replacement;
            assert_eq!(
                parse_teletext_export_project(&as_bytes(&invalid), budget())
                    .unwrap_err()
                    .kind,
                TeletextExportProjectFailure::Shape
            );
        }
        let mut unknown = original.clone();
        unknown
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("settings".into(), json!("private-value"));
        assert_eq!(
            parse_teletext_export_project(&as_bytes(&unknown), budget())
                .unwrap_err()
                .kind,
            TeletextExportProjectFailure::Shape
        );
    }
    let text = serde_json::to_string(&original).unwrap();
    for (from, to) in [
        (
            "\"binding_count\":4",
            "\"binding_count\":4,\"binding_count\":4",
        ),
        (
            "\"origin\":\"scripted\"",
            "\"origin\":\"scripted\",\"origin\":\"scripted\"",
        ),
    ] {
        let duplicate = text.replace(from, to);
        assert_ne!(duplicate, text);
        assert_eq!(
            parse_teletext_export_project(duplicate.as_bytes(), budget())
                .unwrap_err()
                .kind,
            TeletextExportProjectFailure::Duplicate
        );
    }
    let nested = format!(
        "{{\"effective_scope\":{}0{}}}",
        "[".repeat(40),
        "]".repeat(40)
    );
    assert_eq!(
        parse_teletext_export_project(nested.as_bytes(), budget())
            .unwrap_err()
            .kind,
        TeletextExportProjectFailure::Depth
    );
    let project = effective_project();
    let debug = format!(
        "{:?} {:?}",
        project.effective_scope, project.producers[1].effective_origin
    );
    for retained in [START, FINISH, UNTIL] {
        assert!(!debug.contains(retained));
    }
    // A structurally valid alteration remains an untrusted claim. It must differ
    // from the native host's regenerated immutable baseline, never grant authority.
    let mut tampered = project.clone();
    tampered.producers[0].effective_origin =
        Some(TeletextExportEffectiveOrigin::Base { binding_index: 0 });
    assert!(tampered.validate(budget()).is_ok());
    assert_ne!(
        write_teletext_export_project(&tampered, budget()).unwrap(),
        write_teletext_export_project(&project, budget()).unwrap()
    );
}
