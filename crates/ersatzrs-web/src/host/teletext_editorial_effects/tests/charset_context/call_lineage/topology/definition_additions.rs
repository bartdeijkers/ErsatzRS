//! Incoming new definitions are distinct from cumulative root-added identities.
use super::*;

fn addition() -> TeletextResourceObjectDefinitionAddition {
    TeletextResourceObjectDefinitionAddition::new(
        TeletextEnhancementObjectDefinition {
            identity: target(25),
            object_type: TeletextObjectType::Adaptive,
            program: program(vec![
                at(0, designation(0)),
                call(22, TeletextObjectType::Passive, None),
            ]),
        },
        policy().validation.service,
    )
    .unwrap()
}
fn permissions(generated: bool) -> TeletextExportProject {
    let mut value = project("zone", 1, 8, 8, 1);
    let mut mapping = value.pages[0].clone();
    mapping.name = "p124.tti".into();
    mapping.page.page = 124;
    mapping.editor_page = 0x124;
    value.pages.push(mapping);
    let pages = &mut value.producers[0].authoring.as_mut().unwrap().pages;
    let mut page = pages[0].clone();
    page.page.page = 124;
    if generated {
        page.regions[0].kind = TeletextAuthoringRegionKindWire::Generated;
    }
    pages.push(page);
    value
}
fn with_additions<'a>(
    programs: &'a [TeletextResourceObjectProgramOverlay],
    additions: &'a [TeletextResourceObjectDefinitionAddition],
) -> TeletextEditorialEdits<'a> {
    let mut value = edits(programs);
    value.resource_additions = additions;
    value
}
fn latent(addition: &TeletextResourceObjectDefinitionAddition) -> EncodedTeletextAuthoredService {
    let mut definitions = object_definitions(None);
    definitions.push(addition.definition().clone());
    native_with_definitions(definitions)
}
fn invoked(addition: &TeletextResourceObjectDefinitionAddition) -> EncodedTeletextAuthoredService {
    let mut definitions = object_definitions(None);
    definitions[0].program = program(vec![
        call(
            25,
            TeletextObjectType::Adaptive,
            Some(TeletextObjectOffset::new(0, 4).unwrap()),
        ),
        call(22, TeletextObjectType::Passive, None),
        call(23, TeletextObjectType::Passive, None),
    ]);
    definitions.push(addition.definition().clone());
    native_with_definitions(definitions)
}
fn validate(
    before: &EncodedTeletextAuthoredService,
    after: &EncodedTeletextAuthoredService,
    incoming: TeletextEditorialEdits<'_>,
    cumulative: TeletextEditorialEdits<'_>,
    generated: bool,
) -> Result<()> {
    validate_teletext_editorial_transition_with_operands(
        before,
        after,
        &permissions(generated),
        incoming,
        cumulative,
        &policy().encoding,
        deadline(),
    )
}

fn assert_new_definition_lineage(
    root: &EncodedTeletextAuthoredService,
    invoked: &EncodedTeletextAuthoredService,
    changes: &[TeletextResourceObjectProgramOverlay],
    additions: &[TeletextResourceObjectDefinitionAddition],
) {
    let a = composition(root);
    let b = composition(invoked);
    let lineage = ProtectedLineage::new(
        a,
        b,
        with_additions(changes, additions),
        &policy().encoding,
        deadline(),
    )
    .unwrap();
    for event in b
        .spatial_trace()
        .events()
        .iter()
        .filter(|event| event.source().object == Some(target(25)))
    {
        assert_eq!(
            lineage.source(event.source()),
            CanonicalSource::Inserted(event.source())
        );
    }
    let new_call = b
        .spatial_trace()
        .invocations()
        .iter()
        .find(|call| call.target() == target(25))
        .unwrap();
    let child = b
        .spatial_trace()
        .invocations()
        .iter()
        .find(|call| call.parent() == Some(new_call.id()))
        .unwrap();
    assert_eq!(child.target(), target(22));
    assert!(matches!(
        lineage.source_at_invocation(child.source(), Some(child.id())),
        CanonicalSource::Inserted(_)
    ));
    assert_retained_charset(a, b, &lineage, 22);
    assert_retained_charset(a, b, &lineage, 23);
}

#[test]
fn resource_definition_effects_add_invoke_and_edit_with_delta_and_cumulative_lineage() {
    let root = native_with_definitions(object_definitions(None));
    let additions = [addition()];
    let unused = latent(&additions[0]);
    validate(
        &root,
        &unused,
        with_additions(&[], &additions),
        with_additions(&[], &additions),
        true,
    )
    .unwrap();
    let invoked = invoked(&additions[0]);
    let changes = [change(&root, &invoked, vec![None, Some(0), Some(1)])];
    validate(
        &unused,
        &invoked,
        edits(&changes),
        with_additions(&changes, &additions),
        false,
    )
    .unwrap();
    validate(
        &root,
        &invoked,
        with_additions(&changes, &additions),
        with_additions(&changes, &additions),
        false,
    )
    .unwrap();
    assert_new_definition_lineage(&root, &invoked, &changes, &additions);

    let mut replacement = additions[0].definition().clone();
    let mut commands = replacement.program.commands().to_vec();
    commands.push(at(
        1,
        TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(b'C').unwrap(),
        },
    ));
    replacement.program = program(commands);
    let final_additions =
        [
            TeletextResourceObjectDefinitionAddition::new(replacement, policy().validation.service)
                .unwrap(),
        ];
    let final_service = self::invoked(&final_additions[0]);
    let incoming = [TeletextResourceObjectProgramOverlay::new(
        target(25),
        additions[0].definition().program.clone(),
        final_additions[0].definition().program.clone(),
        vec![Some(0), Some(1), None],
        policy().validation.service,
    )
    .unwrap()];
    validate(
        &invoked,
        &final_service,
        edits(&incoming),
        with_additions(&changes, &final_additions),
        false,
    )
    .unwrap();
    assert!(
        validate(
            &invoked,
            &final_service,
            edits(&incoming),
            with_additions(&incoming, &final_additions),
            false,
        )
        .is_err(),
        "root-added definition bodies must fold into their addition, not overlap it"
    );
    validate(
        &root,
        &final_service,
        with_additions(&changes, &final_additions),
        with_additions(&changes, &final_additions),
        false,
    )
    .unwrap();
}

#[test]
fn resource_definition_effects_refuse_unclaimed_catalogues_shadowing_and_generated_consumers() {
    let root = native_with_definitions(object_definitions(None));
    let additions = [addition()];
    let unused = latent(&additions[0]);
    assert!(validate(&root, &unused, edits(&[]), edits(&[]), false).is_err());
    assert!(validate(
        &root,
        &unused,
        with_additions(&[], &additions),
        edits(&[]),
        false
    )
    .is_err());
    assert!(validate(
        &unused,
        &unused,
        with_additions(&[], &additions),
        with_additions(&[], &additions),
        false
    )
    .is_err());
    let invoked = invoked(&additions[0]);
    let changes = [change(&root, &invoked, vec![None, Some(0), Some(1)])];
    let failure = validate(
        &root,
        &invoked,
        with_additions(&changes, &additions),
        with_additions(&changes, &additions),
        true,
    )
    .unwrap_err();
    assert_eq!(
        failure.reason,
        TeletextEditorialEffectsFailure::ProtectedDisplay
    );
    assert_eq!(failure.page, Some(address(124)));
    assert_eq!(failure.position, Some((1, 12)));
    let duplicate = [additions[0].clone(), additions[0].clone()];
    assert!(validate(
        &root,
        &unused,
        with_additions(&[], &duplicate),
        with_additions(&[], &duplicate),
        false
    )
    .is_err());
    let mut wrong = additions[0].definition().clone();
    wrong.program = program(vec![call(23, TeletextObjectType::Passive, None)]);
    let wrong = [
        TeletextResourceObjectDefinitionAddition::new(wrong, policy().validation.service).unwrap(),
    ];
    assert!(validate(
        &root,
        &unused,
        with_additions(&[], &wrong),
        with_additions(&[], &wrong),
        false
    )
    .is_err());
    let mut reordered = object_definitions(None);
    reordered.swap(1, 2);
    reordered.push(additions[0].definition().clone());
    let reordered = native_with_definitions(reordered);
    assert!(
        validate(
            &root,
            &reordered,
            with_additions(&[], &additions),
            with_additions(&[], &additions),
            false
        )
        .is_err(),
        "adding a definition does not authorize reordering old native identities"
    );
}
