//! Resource call topology grants no inherited authority to new callee writers.
use super::*;

mod definition_additions;
mod main_roots;

fn native(commands: Vec<TeletextEnhancementCommand>) -> EncodedTeletextAuthoredService {
    let mut definitions = object_definitions(None);
    definitions[0].program = program(commands);
    definitions.push(TeletextEnhancementObjectDefinition {
        identity: target(24),
        object_type: TeletextObjectType::Adaptive,
        program: program(vec![call(22, TeletextObjectType::Passive, None)]),
    });
    native_with_definitions(definitions)
}
fn original_calls() -> Vec<TeletextEnhancementCommand> {
    vec![
        call(22, TeletextObjectType::Passive, None),
        call(23, TeletextObjectType::Passive, None),
    ]
}
fn inserted_calls() -> Vec<TeletextEnhancementCommand> {
    let mut commands = original_calls();
    commands.insert(
        0,
        call(
            24,
            TeletextObjectType::Adaptive,
            Some(TeletextObjectOffset::new(0, 4).unwrap()),
        ),
    );
    commands
}
fn change(
    before: &EncodedTeletextAuthoredService,
    after: &EncodedTeletextAuthoredService,
    retained: Vec<Option<usize>>,
) -> TeletextResourceObjectProgramOverlay {
    let root = |service: &EncodedTeletextAuthoredService| {
        service
            .declared()
            .semantic()
            .objects()
            .definition(target(21))
            .unwrap()
            .program
            .clone()
    };
    TeletextResourceObjectProgramOverlay::new_with_invocation_structure(
        target(21),
        root(before),
        root(after),
        retained,
        policy().validation.service,
    )
    .unwrap()
}
fn edits(changes: &[TeletextResourceObjectProgramOverlay]) -> TeletextEditorialEdits<'_> {
    TeletextEditorialEdits {
        base_text_edits: &[],
        rows: &[],
        operands: &[],
        object_operands: &[],
        resource_operands: &[],
        resource_programs: changes,
        resource_additions: &[],
        object_resource_additions: &[],
        object_subpage_announcements: &[],
        object_page_selections: &[],
        object_programs: &[],
        main_programs: &[],
        main_resource_invocations: &[],
        main_presence: &[],
        bitmaps: &[],
    }
}
fn assert_retained_charset(
    before: &TeletextMainComposition,
    after: &TeletextMainComposition,
    lineage: &ProtectedLineage<'_>,
    id: u16,
) {
    let (a, b) = (before.charset_execution(), after.charset_execution());
    let (a_index, a_declaration) = declaration(a, id);
    let (b_index, b_declaration) = b
        .declarations()
        .iter()
        .enumerate()
        .find(|(_, value)| {
            value.source().object == Some(target(id))
                && lineage.source_at_invocation(value.source(), value.invocation())
                    == CanonicalSource::Existing(a_declaration.source())
                && lineage.canonical_invocation(value.invocation()) == a_declaration.invocation()
        })
        .unwrap();
    assert_ne!(a_declaration.invocation(), b_declaration.invocation());
    assert!(same_selection(
        a,
        b,
        Some(selection(a, a_index)),
        Some(selection(b, b_index)),
        lineage,
        deadline(),
    )
    .unwrap());
}

#[test]
fn inserted_resource_call_and_descendants_are_new_writers_while_siblings_keep_original_ids() {
    let (before, after) = (native(original_calls()), native(inserted_calls()));
    let changes = [change(&before, &after, vec![None, Some(0), Some(1)])];
    let (a, b) = (composition(&before), composition(&after));
    let lineage =
        ProtectedLineage::new(a, b, edits(&changes), &policy().encoding, deadline()).unwrap();
    let inserted = b
        .spatial_trace()
        .invocations()
        .iter()
        .find(|value| value.target() == target(24))
        .unwrap();
    let child = b
        .spatial_trace()
        .invocations()
        .iter()
        .find(|value| value.parent() == Some(inserted.id()))
        .unwrap();
    assert_eq!(child.target(), target(22));
    for call in [inserted, child] {
        assert!(matches!(
            lineage.source_at_invocation(call.source(), Some(call.id())),
            CanonicalSource::Inserted(_)
        ));
    }
    let (a_index, _) = declaration(a.charset_execution(), 22);
    let (b_index, b_declaration) = declaration(b.charset_execution(), 22);
    assert_eq!(b_declaration.invocation(), Some(child.id()));
    assert!(!same_selection(
        a.charset_execution(),
        b.charset_execution(),
        Some(selection(a.charset_execution(), a_index)),
        Some(selection(b.charset_execution(), b_index)),
        &lineage,
        deadline(),
    )
    .unwrap());
    assert_retained_charset(a, b, &lineage, 22);
    assert_retained_charset(a, b, &lineage, 23);
}

#[test]
fn omitted_resource_call_retires_only_its_descendants_and_preserves_shifted_siblings() {
    let (before, after) = (native(inserted_calls()), native(original_calls()));
    let changes = [change(&before, &after, vec![Some(1), Some(2)])];
    let (a, b) = (composition(&before), composition(&after));
    let lineage =
        ProtectedLineage::new(a, b, edits(&changes), &policy().encoding, deadline()).unwrap();
    assert_eq!(a.spatial_trace().invocations().len(), 5);
    assert_eq!(b.spatial_trace().invocations().len(), 3);
    assert_retained_charset(a, b, &lineage, 23);
    let remaining = b
        .spatial_trace()
        .invocations()
        .iter()
        .find(|value| value.target() == target(22))
        .unwrap();
    let original = a
        .spatial_trace()
        .invocations()
        .iter()
        .find(|value| {
            value.target() == target(22)
                && value.parent()
                    == a.spatial_trace()
                        .invocations()
                        .iter()
                        .find(|parent| parent.target() == target(21))
                        .copied()
                        .map(ersatzrs_domain::TeletextSpatialInvocation::id)
        })
        .unwrap();
    assert_eq!(
        lineage.canonical_invocation(Some(remaining.id())),
        Some(original.id())
    );
}

#[test]
fn unchanged_passive_default_writers_survive_id_shifts_without_granting_inserted_authority() {
    let (original, inserted) = (native(original_calls()), native(inserted_calls()));
    for (before, after, retained) in [
        (&original, &inserted, vec![None, Some(0), Some(1)]),
        (&inserted, &original, vec![Some(1), Some(2)]),
    ] {
        let changes = [change(before, after, retained)];
        let (a, b) = (composition(before), composition(after));
        let lineage =
            ProtectedLineage::new(a, b, edits(&changes), &policy().encoding, deadline()).unwrap();
        // Retained Passive23 paints B at Main(1,10). No explicit flash command
        // exists, so this field is sourced from that exact invocation's default.
        let (old_cell, new_cell) = (
            a.display_resolution().cell(1, 10).unwrap(),
            b.display_resolution().cell(1, 10).unwrap(),
        );
        let old_writer = old_cell.attributes().flash().writer().unwrap();
        let new_writer = new_cell.attributes().flash().writer().unwrap();
        assert!(matches!(
            old_writer.source(),
            TeletextAttributeSource::PassiveDefault { .. }
        ));
        assert_ne!(old_writer.source(), new_writer.source());
        assert!(old_writer.same_payload(new_writer));
        assert!(lineage.same_attribute_source(old_writer.source(), new_writer.source()));
        super::super::super::super::compare_cell(a, b, 1, 10, &lineage, deadline()).unwrap();
        assert!(!ProtectedLineage::default()
            .same_attribute_source(old_writer.source(), new_writer.source()));
    }
    let changes = [change(&original, &inserted, vec![None, Some(0), Some(1)])];
    let (a, b) = (composition(&original), composition(&inserted));
    let lineage =
        ProtectedLineage::new(a, b, edits(&changes), &policy().encoding, deadline()).unwrap();
    let old_writer = a
        .display_resolution()
        .cell(1, 10)
        .unwrap()
        .attributes()
        .flash()
        .writer()
        .unwrap();
    let new_writer = b
        .display_resolution()
        .cell(1, 12)
        .unwrap()
        .attributes()
        .flash()
        .writer()
        .unwrap();
    assert!(old_writer.same_payload(new_writer));
    assert_eq!(old_writer.source(), new_writer.source());
    assert!(!lineage.same_attribute_source(old_writer.source(), new_writer.source()));
}

#[test]
fn unrelated_resource_intent_cannot_authorize_added_or_missing_calls() {
    let (before, after) = (native(original_calls()), native(inserted_calls()));
    let unrelated = TeletextResourceObjectProgramOverlay::new_with_invocation_structure(
        target(24),
        program(vec![call(22, TeletextObjectType::Passive, None)]),
        program(vec![
            call(22, TeletextObjectType::Passive, None),
            call(22, TeletextObjectType::Passive, None),
        ]),
        vec![Some(0), None],
        policy().validation.service,
    )
    .unwrap();
    for (a, b) in [(&before, &after), (&after, &before)] {
        let result = ProtectedLineage::new(
            composition(a),
            composition(b),
            edits(std::slice::from_ref(&unrelated)),
            &policy().encoding,
            deadline(),
        );
        assert!(
            matches!(result, Err(error) if error.reason == TeletextEditorialEffectsFailure::Association)
        );
    }
}

#[test]
fn omitted_call_requires_its_exact_original_command_not_only_an_omitted_ordinal() {
    let before = native(original_calls());
    let after = native(vec![call(23, TeletextObjectType::Passive, None)]);
    let forged = [
        TeletextResourceObjectProgramOverlay::new_with_invocation_structure(
            target(21),
            program(vec![
                call(24, TeletextObjectType::Adaptive, None),
                call(23, TeletextObjectType::Passive, None),
            ]),
            program(vec![call(23, TeletextObjectType::Passive, None)]),
            vec![Some(1)],
            policy().validation.service,
        )
        .unwrap(),
    ];
    let result = ProtectedLineage::new(
        composition(&before),
        composition(&after),
        edits(&forged),
        &policy().encoding,
        deadline(),
    );
    assert!(
        matches!(result, Err(error) if error.reason == TeletextEditorialEffectsFailure::Association)
    );
}

fn all_consumers(kind: &str) -> TeletextExportProject {
    let mut value = serde_json::to_value(project(kind, 1, 0, 40, 24)).unwrap();
    let mut authoring = value["producers"][0]["authoring"]["pages"][0].clone();
    authoring["page"]["page"] = serde_json::json!(124);
    value["producers"][0]["authoring"]["pages"]
        .as_array_mut()
        .unwrap()
        .push(authoring);
    let mut page = value["pages"][0].clone();
    page["name"] = serde_json::json!("p124.tti");
    page["page"]["page"] = serde_json::json!(124);
    page["editor_page"] = serde_json::json!(292);
    value["pages"].as_array_mut().unwrap().push(page);
    serde_json::from_value(value).unwrap()
}

#[test]
fn equal_pixel_call_topology_still_requires_editable_regions_for_every_consumer() {
    let before = native(original_calls());
    let mut commands = original_calls();
    commands.insert(1, commands[0]);
    let after = native(commands);
    let options = TeletextColourPixelOptions {
        reveal_concealed: false,
        flash_phase: TeletextFlashPhase::Original,
    };
    let raster = |service: &EncodedTeletextAuthoredService| {
        render_teletext_composition(
            composition(service),
            options,
            TeletextRasterBudget {
                max_pixels: 168_000,
                max_diagnostics: 1400,
            },
        )
        .unwrap()
    };
    assert_eq!(raster(&before), raster(&after));
    for (a, b, map) in [
        (&before, &after, vec![Some(0), None, Some(1)]),
        (&after, &before, vec![Some(0), Some(2)]),
    ] {
        let changes = [change(a, b, map)];
        let validate = |project: &TeletextExportProject, edits| {
            validate_teletext_editorial_effects_with_operands(
                a,
                b,
                project,
                edits,
                &policy().encoding,
                deadline(),
            )
        };
        validate(&all_consumers("free"), edits(&changes)).unwrap();
        assert!(validate(&all_consumers("generated"), edits(&changes)).is_err());
        assert!(validate(&project("free", 1, 0, 40, 24), edits(&changes)).is_err());
        assert!(validate(&all_consumers("generated"), edits(&[])).is_err());
    }
}
