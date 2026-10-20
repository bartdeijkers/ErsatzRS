use super::*;

#[test]
fn main_program_presence_revalidates_original_mot_defaults_and_exact_root_undo() {
    let input = draft();
    let original = bind(input.clone(), declaration(0, 1), vec![]).unwrap();
    let limits = TeletextServiceBudget {
        max_records: 64,
        max_objects: 64,
        max_structure_items: 1024,
        max_base_content_bytes: 1024,
        max_commands: 1024,
        max_collected_commands: 1024,
        max_collected_drcs_references: 64,
    };
    let empty = TeletextEnhancementProgram::new(vec![]).unwrap();
    let created = original
        .clone()
        .with_main_program_presence_overlays(
            &[
                TeletextMainProgramPresenceOverlay::new(owner(0), None, Some(empty.clone()))
                    .unwrap(),
            ],
            limits,
            budget(),
        )
        .unwrap();
    let mut expected = input;
    expected.pages[0].local_program = Some(empty.clone());
    assert_eq!(created, bind(expected, declaration(0, 1), vec![]).unwrap());
    assert_eq!(created.mots(), original.mots());
    assert_eq!(created.selections(), original.selections());
    assert_eq!(created.defaults(), original.defaults());
    assert_eq!(created.resource_inventory(), original.resource_inventory());
    let restored = created
        .with_main_program_presence_overlays(
            &[TeletextMainProgramPresenceOverlay::new(owner(0), Some(empty), None).unwrap()],
            limits,
            budget(),
        )
        .unwrap();
    assert_eq!(restored, original);
    let mut small = budget();
    small.max_defaults = 0;
    assert!(original
        .with_main_program_presence_overlays(
            &[TeletextMainProgramPresenceOverlay::new(
                owner(0),
                None,
                Some(TeletextEnhancementProgram::new(vec![]).unwrap())
            )
            .unwrap()],
            limits,
            small,
        )
        .is_err());
}
