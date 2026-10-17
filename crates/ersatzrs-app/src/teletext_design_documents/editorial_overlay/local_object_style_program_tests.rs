//! V13 keeps earlier storage roles closed and binds styles to immutable originals.
use super::all_local_object_glyph_program_tests::original;
use super::tests::{address, budget, bytes, provenance};
use super::{
    append_teletext_design_editorial_overlay, decode_teletext_design_documents,
    TeletextSavedEditorialOverlay,
};
use ersatzrs_domain::{
    TeletextColumnOperation, TeletextEnhancementCommand, TeletextEnhancementProgram,
    TeletextLocalObjectGlyphProgramOverlay, TeletextLocalObjectOperandOverlay, TeletextObjectId,
    TeletextObjectIdentity,
};
use serde_json::{json, Value};

fn identity() -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(address(16), TeletextObjectId::new(65520))
}
fn operation(value: Value) -> TeletextColumnOperation {
    serde_json::from_value(value).unwrap()
}
fn command(column: u8, value: Value) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column,
        operation: operation(value),
    }
}
fn program(commands: Vec<TeletextEnhancementCommand>) -> TeletextEnhancementProgram {
    TeletextEnhancementProgram::new(commands).unwrap()
}
fn styles() -> Vec<Value> {
    vec![
        json!({"kind":"foreground","colour":2}),
        json!({"kind":"background","colour":3}),
        json!({"kind":"attributes","double_height":true,"double_width":true,"boxed":true,
            "conceal":true,"invert":true,"underline_or_separated_mosaics":true}),
    ]
}
fn change(style: Value, delete: bool) -> TeletextLocalObjectGlyphProgramOverlay {
    let root = program(vec![
        command(0, json!({"kind":"foreground","colour":1})),
        command(1, json!({"kind":"g2","code":35})),
    ]);
    let mut inserted = root.commands().to_vec();
    inserted.insert(1, command(1, style));
    let inserted = program(inserted);
    let (before, after, map) = if delete {
        (inserted, root, vec![Some(0), Some(2)])
    } else {
        (root, inserted, vec![Some(0), None, Some(1)])
    };
    TeletextLocalObjectGlyphProgramOverlay::new_with_style_commands(identity(), before, after, map)
        .unwrap()
}
fn intent(
    change: TeletextLocalObjectGlyphProgramOverlay,
    scalars: Vec<TeletextLocalObjectOperandOverlay>,
) -> TeletextSavedEditorialOverlay {
    TeletextSavedEditorialOverlay::new_with_local_object_style_programs(
        provenance(),
        vec![],
        vec![],
        scalars,
        vec![],
        vec![change],
    )
    .unwrap()
}

#[test]
fn v13_style_insert_delete_roundtrip_preserves_original_context_and_checked_attributes() {
    for style in styles() {
        for delete in [false, true] {
            let change = change(style.clone(), delete);
            let original = original(change.expected());
            let root = decode_teletext_design_documents(&original, budget()).unwrap();
            let overlay = intent(change, vec![]);
            assert_eq!(overlay.schema_version(), 13);
            let encoded =
                append_teletext_design_editorial_overlay(&original, &overlay, budget()).unwrap();
            let restored = decode_teletext_design_documents(encoded.bytes(), budget()).unwrap();
            assert_eq!(restored.editorial_overlay(), Some(&overlay));
            assert_eq!(restored.sources()[0].bytes(), root.sources()[0].bytes());
            assert_eq!(restored.selection(), root.selection());
            assert_eq!(
                restored.resource_transmissions(),
                root.resource_transmissions()
            );
            for contract in [
                ersatzrs_domain::TeletextPageContract::V1,
                ersatzrs_domain::TeletextPageContract::V2,
                ersatzrs_domain::TeletextPageContract::V3,
            ] {
                assert_eq!(
                    restored
                        .guide_inputs()
                        .and_then(|value| value.request(contract)),
                    root.guide_inputs()
                        .and_then(|value| value.request(contract))
                );
            }
        }
    }
}

#[test]
fn v13_prebuilt_styles_cannot_enter_v11_v12_or_downgraded_storage() {
    let change = change(styles()[0].clone(), false);
    for constructor in [
        TeletextSavedEditorialOverlay::new_with_local_object_glyph_programs,
        TeletextSavedEditorialOverlay::new_with_all_local_object_glyph_programs,
    ] {
        assert!(constructor(
            provenance(),
            vec![],
            vec![],
            vec![],
            vec![],
            vec![change.clone()]
        )
        .is_err());
    }
    let original = original(change.expected());
    let encoded =
        append_teletext_design_editorial_overlay(&original, &intent(change, vec![]), budget())
            .unwrap();
    for version in 1..=12 {
        let mut value: Value = serde_json::from_slice(encoded.bytes()).unwrap();
        value["editorial_overlay"]["schema_version"] = json!(version);
        assert!(decode_teletext_design_documents(&bytes(&value), budget()).is_err());
    }
    let glyph_only = TeletextLocalObjectGlyphProgramOverlay::new_with_all_glyphs(
        identity(),
        program(vec![command(1, json!({"kind":"g2","code":35}))]),
        program(vec![command(2, json!({"kind":"g2","code":36}))]),
        vec![Some(0)],
    )
    .unwrap();
    assert!(
        TeletextSavedEditorialOverlay::new_with_local_object_style_programs(
            provenance(),
            vec![],
            vec![],
            vec![],
            vec![],
            vec![glyph_only],
        )
        .is_err()
    );
}

#[test]
fn v13_style_scalar_uses_exact_original_ordinal_but_glyph_overlap_is_rejected() {
    let root = change(styles()[0].clone(), false);
    let original = original(root.expected());
    let scalar = TeletextLocalObjectOperandOverlay::new(
        identity(),
        0,
        0,
        operation(json!({"kind":"foreground","colour":1})),
        operation(json!({"kind":"foreground","colour":4})),
    )
    .unwrap();
    let patch = |commands: &TeletextEnhancementProgram| {
        let mut value = commands.commands().to_vec();
        value[0] = command(0, json!({"kind":"foreground","colour":4}));
        program(value)
    };
    let patched = TeletextLocalObjectGlyphProgramOverlay::new_with_style_commands(
        identity(),
        patch(root.expected()),
        patch(root.replacement()),
        root.retained_commands().to_vec(),
    )
    .unwrap();
    let overlay = intent(patched, vec![scalar]);
    let saved = append_teletext_design_editorial_overlay(&original, &overlay, budget()).unwrap();
    assert_eq!(
        decode_teletext_design_documents(saved.bytes(), budget())
            .unwrap()
            .editorial_overlay(),
        Some(&overlay)
    );
    let glyph = TeletextLocalObjectOperandOverlay::new(
        identity(),
        1,
        1,
        operation(json!({"kind":"g2","code":35})),
        operation(json!({"kind":"g2","code":36})),
    )
    .unwrap();
    assert!(
        TeletextSavedEditorialOverlay::new_with_local_object_style_programs(
            provenance(),
            vec![],
            vec![],
            vec![glyph],
            vec![],
            vec![root.clone()],
        )
        .is_err()
    );
    assert!(
        append_teletext_design_editorial_overlay(&original, &intent(root, vec![scalar]), budget())
            .is_err(),
        "a captured expected skeleton must include its authenticated scalar patch"
    );
}

#[test]
fn v13_strict_maps_original_association_and_existing_budgets_remain_enforced() {
    let change = change(styles()[2].clone(), false);
    let original = original(change.expected());
    let encoded =
        append_teletext_design_editorial_overlay(&original, &intent(change, vec![]), budget())
            .unwrap();
    let base: Value = serde_json::from_slice(encoded.bytes()).unwrap();
    for (path, replacement) in [
        ("/editorial_overlay/local_object_glyph_program_overlays", Value::Null),
        ("/editorial_overlay/local_object_glyph_program_overlays/0/retained_commands", json!([0,0,1])),
        ("/editorial_overlay/local_object_glyph_program_overlays/0/replacement/commands/1/operation/double_height", json!(1)),
        ("/editorial_overlay/local_object_glyph_program_overlays/0/expected/commands/0/operation/colour", json!(5)),
    ] {
        let mut value = base.clone();
        *value.pointer_mut(path).unwrap() = replacement;
        assert!(decode_teletext_design_documents(&bytes(&value), budget()).is_err());
    }
    let mut limited = budget();
    limited.max_serialized_bytes = encoded.bytes().len() - 1;
    assert!(decode_teletext_design_documents(encoded.bytes(), limited).is_err());
    limited = budget();
    limited.validation.service.max_commands = 1;
    assert!(decode_teletext_design_documents(encoded.bytes(), limited).is_err());
}
