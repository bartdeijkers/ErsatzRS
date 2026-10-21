//! One exact Local page's real Main/body word order, not resource storage order.

use super::*;
use crate::teletext::{
    compile_teletext_normalized_main_program, EncodedTeletextLocalObjectProgram,
    EncodedTeletextNormalizedMainProgram, TeletextMainCompositionSource,
};

fn fg(column: u8, value: u8) -> TeletextEnhancementCommand {
    at(
        column,
        TeletextColumnOperation::Foreground {
            colour: TeletextColourReference::new(value).unwrap(),
        },
    )
}

fn target(id: u16) -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(
        super::super::transport_tests::draft(200, 0, false).address,
        TeletextObjectId::new(id),
    )
}

fn invoke(id: u16, object_type: TeletextObjectType) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::InvokeObject {
        target: target(id),
        object_type,
        offset: None,
    }
}

fn main() -> Vec<TeletextEnhancementCommand> {
    vec![
        fg(8, 1),
        invoke(65534, TeletextObjectType::Active),
        position(0, 9),
        invoke(65535, TeletextObjectType::Passive),
    ]
}

fn definitions() -> Vec<TeletextEnhancementObjectDefinition> {
    // Keep the original Active body exact. A display glyph must not be added
    // merely to expose its two colour writers to an external decoder.
    vec![
        TeletextEnhancementObjectDefinition {
            identity: target(65535),
            object_type: TeletextObjectType::Passive,
            program: TeletextEnhancementProgram::new(vec![
                fg(0, 2),
                at(
                    0,
                    TeletextColumnOperation::G0 {
                        code: TeletextGlyphCode::new(b'A').unwrap(),
                    },
                ),
            ])
            .unwrap(),
        },
        TeletextEnhancementObjectDefinition {
            identity: target(65534),
            object_type: TeletextObjectType::Active,
            program: TeletextEnhancementProgram::new(vec![fg(0, 3), fg(0, 4)]).unwrap(),
        },
    ]
}

fn rows() -> Vec<TeletextRow> {
    vec![text(0, 9, "ZZ"), text(1, 8, "B")]
}

fn source() -> TeletextSemanticService {
    let mut page = super::super::transport_tests::draft(200, 0, false);
    page.national_option = TeletextNationalOption::new(1).unwrap();
    page.rows = rows();
    service(&page, main(), definitions())
}

fn normalized(source: &TeletextSemanticService) -> EncodedTeletextNormalizedMainProgram {
    let composition = TeletextMainCompositionBudget {
        max_input_commands: 64,
        max_output_commands: 64,
        collection: TeletextEnhancementCollectionBudget {
            max_commands: 128,
            max_drcs_references: 0,
        },
    };
    compile_teletext_normalized_main_program(
        source,
        target(65534).owner().unwrap(),
        normalization_budget(
            composition,
            TeletextObjectCatalogBudget {
                max_objects: 4,
                max_commands: 128,
            },
        ),
    )
    .expect("exact same-page Main/Active writers must complete before decoder qualification")
}

fn word(column: u32, mode: u32, data: u32) -> u32 {
    column | (mode << 6) | (data << 11)
}

fn assert_native_word(encoded: &EncodedTeletextLocalObjectProgram, ordinal: usize, value: u32) {
    let offset = 1 + ordinal % 13 * 3;
    assert_eq!(
        &encoded.packet_bodies()[ordinal / 13][offset..offset + 3],
        &crate::teletext::packets::hamming_24_18(value),
        "claimed C.2 word must be the final native packet bytes"
    );
}

pub(super) fn assert_same_page_source_order(
    source: &TeletextSemanticService,
    normalized: &EncodedTeletextNormalizedMainProgram,
) {
    assert_eq!(source.objects().definitions(), definitions().as_slice());
    assert_eq!(
        source.page_enhancements()[&target(65534).owner().unwrap()]
            .local_program
            .as_ref()
            .unwrap()
            .commands(),
        main().as_slice()
    );
    let encoded = normalized.encoded();
    let main: Vec<_> = encoded
        .command_positions()
        .iter()
        .filter(|position| {
            position.source().object.is_none()
                && position.original_main_source()
                    == Some(TeletextMainCompositionSource::OriginalPage { command: 0 })
        })
        .collect();
    assert_eq!(main.len(), 1);
    assert_native_word(encoded, main[0].semantic_word(), word(8, 0, 1));
    let active: Vec<_> = encoded
        .command_positions()
        .iter()
        .filter(|position| position.source().object == Some(target(65534)))
        .collect();
    assert_eq!(
        active.len(),
        2,
        "the original Active remains exactly two colour writers"
    );
    for ((command, colour), position) in [(0, 3), (1, 4)].into_iter().zip(&active) {
        assert_eq!(
            position.original_object_source(),
            Some(TeletextMainCompositionSource::Object {
                identity: target(65534),
                command,
            })
        );
        assert_native_word(encoded, position.semantic_word(), word(0, 0, colour));
    }
    assert!(main[0].semantic_word() < active[0].semantic_word());
    assert!(active[0].semantic_word() < active[1].semantic_word());
    // EN 300 706 C.2 and 13.1/B.2/B.3 apply to this authenticated one-page
    // sequence. This says nothing about arbitrary resources or repeated calls.
    let execution = normalized.physical().replay().attribute_execution();
    assert_eq!(
        execution
            .cell(0, 10)
            .unwrap()
            .foreground()
            .value()
            .unwrap()
            .get(),
        4
    );
}

#[tokio::test]
#[ignore = "Requires Python 3 and the explicitly pinned external teletextimager checkout"]
async fn same_page_main_active_c2_order_decodes_with_pinned_independent_teletextimager() {
    let source = source();
    let normalized = normalized(&source);
    assert_same_page_source_order(&source, &normalized);
    let case = encoded_case_with_encoding(
        "same_page_main_active_c2_order",
        rows(),
        main(),
        definitions(),
        &[
            cell(0, 9, [b'Z', 15, 0, 7, 0], [b'A', 0, 0, 2, 0]),
            cell(0, 10, [b'Z', 15, 0, 7, 0], [b'Z', 15, 0, 4, 0]),
            cell(1, 8, [b'B', 15, 0, 7, 0], [b'B', 15, 0, 7, 0]),
        ],
        MainEncoding::Normalized,
    );
    assert_contextual_decoding(vec![case], "ERSATZRS_TELETEXT_TEST_SAME_PAGE_C2_FIXTURE").await;
}
