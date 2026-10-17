//! Changed Local glyph topology through the semantic service and final X26 packing.
//! Uses the existing independent Hamming/selector oracle; not TTI/editor/client proof.
use super::*;
use crate::teletext::authored_service_tests::{input, semantic_limits};

fn glyph(column: u8, code: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column,
        operation: TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(code).unwrap(),
        },
    }
}

fn fixture(commands: Vec<TeletextEnhancementCommand>) -> TeletextSemanticService {
    let mut draft = input();
    let display = owner(0);
    let edited = local(display, 65534);
    let protected = local(display, 65535);
    draft.pages[0].local_program = Some(program(vec![
        position(1, 0),
        invoke(edited, TeletextObjectType::Passive, None),
        position(3, 0),
        invoke(edited, TeletextObjectType::Passive, None),
        position(5, 0),
        invoke(protected, TeletextObjectType::Passive, None),
    ]));
    draft.objects.extend([
        definition(edited, TeletextObjectType::Passive, commands),
        definition(protected, TeletextObjectType::Passive, vec![glyph(0, b'Z')]),
    ]);
    TeletextSemanticService::new(draft, semantic_limits()).unwrap()
}

fn encoded(service: &TeletextSemanticService) -> EncodedTeletextLocalObjectProgram {
    let display = owner(0);
    compile(
        display,
        service
            .page_enhancements()
            .get(&display)
            .unwrap()
            .local_program
            .as_ref()
            .unwrap(),
        service.objects(),
    )
}

fn assert_target(page: &EncodedTeletextLocalObjectProgram, word: usize, marker: usize) {
    let decoded = words(page.packet_bodies());
    // EN 300 706 Table 28: Local Passive call/definition modes 19/23.
    // The selector encodes designation in data bits 4..6 plus address bit 0.
    let designation = marker / 13;
    let slot = marker % 13;
    let address = 40 + u32::try_from(designation >> 3).unwrap();
    let data = u32::try_from(((designation & 7) << 4) | slot).unwrap();
    assert_eq!(decoded[word], (address, 19, data));
    assert_eq!(decoded[marker], (address, 23, data));
    let actual = &page.definition_positions()[1];
    assert_eq!(actual.target(), local(owner(0), 65535));
    assert_eq!(usize::from(actual.absolute_index()), marker);
    assert_eq!(
        usize::from(actual.object_number()),
        (designation << 4) | slot
    );
    let protected = page
        .command_positions()
        .iter()
        .find(|position| {
            position.source()
                == TeletextEnhancementSource {
                    object: Some(local(owner(0), 65535)),
                    command: 0,
                }
        })
        .unwrap();
    assert_eq!(protected.first_word(), marker + 1);
    assert_eq!(protected.semantic_word(), marker + 1);
    assert_eq!(decoded[marker + 1], (0, 9, u32::from(b'Z')));
}

#[test]
fn local_glyph_structure_grow_shrink_and_undo_repack_packets_and_protected_forward_calls() {
    let original_commands = vec![
        glyph(0, b'A'),
        glyph(1, b'A'),
        glyph(2, b'B'),
        TeletextEnhancementCommand::AtColumn {
            column: 3,
            operation: TeletextColumnOperation::G1 {
                mosaic: TeletextMosaic::new(63).unwrap(),
            },
        },
    ];
    let original = fixture(original_commands.clone());
    let before = encoded(&original);
    assert_eq!(before.main_triplet_count(), 6);
    assert_eq!(before.packet_bodies().len(), 2);
    assert_target(&before, 5, 12);
    for inserted in [1, 12] {
        let mut replacement = original_commands.clone();
        replacement.extend(vec![glyph(3, b'C'); inserted]);
        let mut correspondence: Vec<_> = (0..4).map(Some).collect();
        correspondence.extend(vec![None; inserted]);
        let overlay = TeletextLocalObjectGlyphProgramOverlay::new(
            local(owner(0), 65534),
            program(original_commands.clone()),
            program(replacement.clone()),
            correspondence,
        )
        .unwrap();
        let edited = original
            .clone()
            .with_local_object_glyph_program_overlays(&[overlay], semantic_limits())
            .unwrap();
        let after = encoded(&edited);
        assert_eq!(after.main_triplet_count(), 6);
        assert_eq!(
            after.packet_bodies().len(),
            if inserted == 1 { 2 } else { 3 }
        );
        assert_target(&after, 5, 12 + inserted);
        // Both calls to the edited definition still target its original marker.
        let decoded = words(after.packet_bodies());
        assert_eq!(decoded[1], (40, 19, 7));
        assert_eq!(decoded[3], (40, 19, 7));
        assert!(decoded[15 + inserted..]
            .iter()
            .all(|word| *word == (63, 31, 5)));
        let undo = TeletextLocalObjectGlyphProgramOverlay::new(
            local(owner(0), 65534),
            program(replacement),
            program(original_commands.clone()),
            (0..4).map(Some).collect(),
        )
        .unwrap();
        let restored = edited
            .with_local_object_glyph_program_overlays(&[undo], semantic_limits())
            .unwrap();
        assert_eq!(restored, original);
        assert_eq!(encoded(&restored), before);
    }
    let delete = TeletextLocalObjectGlyphProgramOverlay::new(
        local(owner(0), 65534),
        program(original_commands),
        program(vec![]),
        vec![],
    )
    .unwrap();
    let empty = original
        .clone()
        .with_local_object_glyph_program_overlays(&[delete], semantic_limits())
        .unwrap();
    let packed = encoded(&empty);
    assert_eq!(packed.packet_bodies().len(), 1);
    assert_target(&packed, 5, 8);
    assert_eq!(encoded(&original), before);
}

#[test]
fn local_glyph_structure_does_not_bypass_the_complete_x26_capacity_gate() {
    let commands = vec![glyph(0, b'A'); 197];
    let original = fixture(commands.clone());
    let before = encoded(&original);
    assert_eq!(before.packet_bodies().len(), 16);
    assert_target(&before, 5, 205);
    let mut replacement = commands.clone();
    replacement.push(glyph(1, b'B'));
    let mut mapping: Vec<_> = (0..197).map(Some).collect();
    mapping.push(None);
    let overlay = TeletextLocalObjectGlyphProgramOverlay::new(
        local(owner(0), 65534),
        program(commands),
        program(replacement),
        mapping,
    )
    .unwrap();
    let edited = original
        .clone()
        .with_local_object_glyph_program_overlays(&[overlay], semantic_limits())
        .unwrap();
    let allocations = allocate_teletext_resource_objects(edited.objects(), &[], 256).unwrap();
    let error = compile_teletext_local_program_with_objects(
        owner(0),
        edited
            .page_enhancements()
            .get(&owner(0))
            .unwrap()
            .local_program
            .as_ref()
            .unwrap(),
        edited.objects(),
        &allocations,
        links(),
        budget(),
    )
    .unwrap_err();
    assert!(matches!(
        error.reason,
        TeletextLocalObjectProgramEncodingFailure::Capacity { .. }
    ));
    assert_eq!(encoded(&original), before);
}
