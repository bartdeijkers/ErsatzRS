//! Complete authored output keeps the original shared model and Level1.5 fallback.
use super::*;
use crate::teletext::{
    authored_service_tests::{declared, transmissions},
    encode_teletext_authored_service, encode_teletext_page, project_encoded_teletext_display,
    EncodedTeletextAuthoredPage, TeletextEncodedDisplayRecord, TeletextEncodingRegion,
};

#[test]
fn repeated_same_page_active_on_separate_rows_preserves_original_order() {
    let mut draft = fixture_draft_with_active_body(
        |passive, active| {
            vec![
                fg(8, 1),
                active,
                position(9),
                passive,
                TeletextEnhancementCommand::SetPosition {
                    position: TeletextEnhancementPosition::new(1, 8).unwrap(),
                },
                active,
            ]
        },
        vec![fg(0, 3), fg(0, 4)],
    );
    draft.base.pages[0].rows.push(TeletextRow {
        row: 0,
        runs: vec![TeletextRun {
            column: 9,
            style: TeletextStyle::default(),
            content: TeletextRunContent::Text { text: "ZZ".into() },
        }],
    });
    let source = declared(draft);
    let unchanged = source.clone();
    let service =
        encode_teletext_authored_service(source, &transmissions(&unchanged), &limits()).unwrap();
    assert_eq!(service.declared(), &unchanged);
    let EncodedTeletextAuthoredPage::CompiledDisplay(page) = &service.pages()[0] else {
        panic!("the complete factory must retain a compiled shared display");
    };
    let target = TeletextObjectIdentity::local(
        unchanged.semantic().base().pages[0].address,
        TeletextObjectId::new(65534),
    );
    let roots = page
        .normalized()
        .original()
        .spatial_trace()
        .invocations()
        .iter()
        .filter(|call| call.target() == target)
        .map(|call| call.root_source())
        .collect::<Vec<_>>();
    assert_eq!(
        roots,
        [
            TeletextSpatialRootSource::Local { command: 1 },
            TeletextSpatialRootSource::Local { command: 5 },
        ]
    );
    assert_shared_definition(page.normalized().encoded(), target);
    for attributes in [
        page.normalized().original().attribute_execution(),
        page.normalized().physical().replay().attribute_execution(),
    ] {
        for (row, column) in [(0, 10), (1, 8)] {
            let foreground = attributes.cell(row, column).unwrap().foreground();
            assert_eq!(
                foreground.value(),
                Some(TeletextColourReference::new(4).unwrap())
            );
            assert_eq!(foreground.pending(), None);
        }
    }
    assert_base_fallback(&unchanged.semantic().base().pages[0], page.page());
}

fn assert_shared_definition(
    encoded: &crate::teletext::EncodedTeletextLocalObjectProgram,
    target: TeletextObjectIdentity,
) {
    assert_eq!(
        encoded
            .definition_positions()
            .iter()
            .filter(|definition| definition.target() == target)
            .count(),
        1,
        "both instances must retain one original physical definition"
    );
    let commands = encoded
        .command_positions()
        .iter()
        .filter(|position| position.source().object == Some(target))
        .collect::<Vec<_>>();
    assert_eq!(commands.len(), 2, "the unchanged body has only FG3 and FG4");
    for ((command, colour), position) in [(0, 3), (1, 4)].into_iter().zip(commands) {
        assert_eq!(
            position.original_object_source(),
            Some(TeletextMainCompositionSource::Object {
                identity: target,
                command
            })
        );
        let word = position.semantic_word();
        let offset = 1 + word % 13 * 3;
        assert_eq!(
            &encoded.packet_bodies()[word / 13][offset..offset + 3],
            &crate::teletext::packets::hamming_24_18(colour << 11)
        );
    }
}

#[test]
fn same_page_main_active_order_reaches_complete_authored_shared_display_and_fallback() {
    let mut draft = fixture_draft_with_active_body(
        |passive, active| vec![fg(8, 1), active, position(9), passive],
        vec![fg(0, 3), fg(0, 4)],
    );
    draft.base.pages[0].rows.push(TeletextRow {
        row: 0,
        runs: vec![TeletextRun {
            column: 9,
            style: TeletextStyle::default(),
            content: TeletextRunContent::Text { text: "ZZ".into() },
        }],
    });
    let source = declared(draft);
    let unchanged = source.clone();
    let service =
        encode_teletext_authored_service(source, &transmissions(&unchanged), &limits()).unwrap();
    assert_eq!(service.declared(), &unchanged);
    let EncodedTeletextAuthoredPage::CompiledDisplay(page) = &service.pages()[0] else {
        panic!("the complete factory must retain a compiled shared display");
    };
    let original = page.normalized().original();
    let physical = page.normalized().physical().replay();
    for attributes in [
        original.attribute_execution(),
        physical.attribute_execution(),
    ] {
        assert_eq!(
            attributes.cell(0, 10).unwrap().foreground().value(),
            Some(TeletextColourReference::new(4).unwrap())
        );
        assert_eq!(attributes.cell(0, 10).unwrap().foreground().pending(), None);
    }
    for display in [original.display_resolution(), physical.display_resolution()] {
        assert_eq!(
            display
                .cell(0, 10)
                .unwrap()
                .attributes()
                .foreground()
                .value(),
            Some(TeletextColourReference::new(4).unwrap())
        );
    }
    assert_base_fallback(&unchanged.semantic().base().pages[0], page.page());
    assert_eq!(unchanged, *service.declared());
}

fn assert_base_fallback(base: &TeletextPageDraft, encoded: &crate::teletext::EncodedTeletextPage) {
    let fallback = encode_teletext_page(base, TeletextEncodingRegion::WesternLatin).unwrap();
    let actual = project_encoded_teletext_display(encoded, 128).unwrap();
    let baseline = project_encoded_teletext_display(&fallback, 128).unwrap();
    assert_eq!(actual.header, baseline.header);
    let rows = |records: Vec<TeletextEncodedDisplayRecord>| {
        records
            .into_iter()
            .filter(|record| !matches!(record, TeletextEncodedDisplayRecord::Triplets { .. }))
            .collect::<Vec<_>>()
    };
    assert_eq!(rows(actual.records), rows(baseline.records));
}
