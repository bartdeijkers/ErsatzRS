//! Retained Main roots keep charset authority only through exact correspondence.
use super::*;
use ersatzrs_domain::TeletextMainLocalInvocationProgramOverlay;

fn local() -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(address(123), TeletextObjectId::new(65520))
}
fn local_call() -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::InvokeObject {
        target: local(),
        object_type: TeletextObjectType::Passive,
        offset: Some(TeletextObjectOffset::new(0, 0).unwrap()),
    }
}
fn main(service: &EncodedTeletextAuthoredService) -> &TeletextEnhancementProgram {
    service.declared().semantic().page_enhancements()[&address(123)]
        .local_program
        .as_ref()
        .unwrap()
}
fn native_main(calls: usize) -> EncodedTeletextAuthoredService {
    let mut objects = object_definitions(None);
    objects.push(TeletextEnhancementObjectDefinition {
        identity: local(),
        object_type: TeletextObjectType::Passive,
        program: program(vec![
            at(0, designation(0)),
            at(
                0,
                TeletextColumnOperation::G0 {
                    code: TeletextGlyphCode::new(b'A').unwrap(),
                },
            ),
        ]),
    });
    let original = native_with_definitions(objects);
    let mut commands = main(&original).commands().to_vec();
    let mut retained = vec![Some(0), Some(1)];
    for _ in 0..calls {
        commands.insert(1, local_call());
        retained.insert(1, None);
    }
    let intent = TeletextMainLocalInvocationProgramOverlay::new(
        address(123),
        main(&original).clone(),
        program(commands),
        retained,
    )
    .unwrap();
    let declared = original
        .declared()
        .clone()
        .with_main_local_invocation_program_overlays(&[intent], policy().validation.service)
        .unwrap();
    encode_teletext_authored_service_before(
        declared,
        &[TeletextServiceResourceTransmission {
            resource: resource(),
            counter: TeletextResourceUpdateCounter::new(1).unwrap(),
            update: ersatzrs_ffmpeg::teletext::TeletextResourceFullUpdate::Changed,
            last_subpage: true,
        }],
        &policy().encoding,
        deadline(),
    )
    .unwrap()
}

#[test]
fn main_call_insert_delete_maps_retained_charset_roots_without_granting_new_writer_authority() {
    let (single, duplicate) = (native_main(1), native_main(2));
    for (before, after, retained) in [
        (&single, &duplicate, vec![Some(0), None, Some(1), Some(2)]),
        (&duplicate, &single, vec![Some(0), Some(2), Some(3)]),
    ] {
        let changes = [TeletextMainLocalInvocationProgramOverlay::new(
            address(123),
            main(before).clone(),
            main(after).clone(),
            retained,
        )
        .unwrap()];
        let (a, b) = (composition(before), composition(after));
        let lineage = ProtectedLineage::new(
            a,
            b,
            TeletextEditorialEdits {
                base_text_edits: &[],
                main_programs: &changes,
                ..edits(&[])
            },
            &policy().encoding,
            deadline(),
        )
        .unwrap();
        let old = a.charset_execution();
        let new = b.charset_execution();
        // Deletion keeps the second old call. Insertion keeps the original as
        // the second new call; both otherwise execute the exact same body/AP.
        let (old_index, old_declaration) = old
            .declarations()
            .iter()
            .enumerate()
            .rfind(|(_, value)| value.source().object == Some(local()))
            .unwrap();
        let (new_index, new_declaration) = new
            .declarations()
            .iter()
            .enumerate()
            .find(|(_, value)| {
                value.source().object == Some(local())
                    && lineage.source_at_invocation(value.source(), value.invocation())
                        == CanonicalSource::Existing(old_declaration.source())
                    && lineage.canonical_invocation(value.invocation())
                        == old_declaration.invocation()
            })
            .unwrap();
        assert_eq!(old_declaration.source(), new_declaration.source());
        assert_eq!(old_declaration.designation(), new_declaration.designation());
        assert_eq!(old_declaration.position(), new_declaration.position());
        assert_ne!(old_declaration.root_source(), new_declaration.root_source());
        assert_ne!(old_declaration.invocation(), new_declaration.invocation());
        let selections = (
            Some(selection(old, old_index)),
            Some(selection(new, new_index)),
        );
        assert!(
            same_selection(old, new, selections.0, selections.1, &lineage, deadline()).unwrap()
        );
        assert!(!same_selection(
            old,
            new,
            selections.0,
            selections.1,
            &ProtectedLineage::default(),
            deadline()
        )
        .unwrap());
        assert_new_writers(
            old,
            new,
            old_index,
            &lineage,
            usize::from(main(after).commands().len() > main(before).commands().len()),
        );
    }
}

fn assert_new_writers(
    old: &TeletextCharsetExecution,
    new: &TeletextCharsetExecution,
    old_index: usize,
    lineage: &ProtectedLineage<'_>,
    expected: usize,
) {
    let old_declaration = &old.declarations()[old_index];
    let mut inserted = 0;
    for (index, declaration) in new.declarations().iter().enumerate().filter(|(_, value)| {
        value.source().object == Some(local())
            && matches!(
                lineage.source_at_invocation(value.source(), value.invocation()),
                CanonicalSource::Inserted(_)
            )
    }) {
        inserted += 1;
        assert_eq!(declaration.designation(), old_declaration.designation());
        assert_eq!(declaration.position(), old_declaration.position());
        assert_eq!(declaration.source(), old_declaration.source());
        assert!(!same_selection(
            old,
            new,
            Some(selection(old, old_index)),
            Some(selection(new, index)),
            lineage,
            deadline()
        )
        .unwrap());
    }
    assert_eq!(inserted, expected);
}
