//! Typed local edits through the complete authored encoder, without publication.

use super::authored_service_tests::{declared, input, limits, semantic_limits, transmissions};
use super::*;
use ersatzrs_domain::*;

fn colour(foreground: bool, value: u8) -> TeletextColumnOperation {
    let colour = TeletextColourReference::new(value).unwrap();
    if foreground {
        TeletextColumnOperation::Foreground { colour }
    } else {
        TeletextColumnOperation::Background { colour }
    }
}

fn attributes(decorated: bool) -> TeletextColumnOperation {
    TeletextColumnOperation::Attributes {
        double_height: false,
        double_width: false,
        boxed: false,
        conceal: false,
        invert: decorated,
        underline_or_separated_mosaics: decorated,
    }
}

fn original(operation: TeletextColumnOperation) -> TeletextDeclaredService {
    let mut draft = input();
    let owner = draft.base.pages[0].address;
    let sibling = draft.base.pages[1].address;
    draft.base.pages[0].navigation = vec![TeletextNavigationLink {
        role: TeletextNavigationRole::Red,
        target: TeletextNavigationTarget {
            page: sibling.page,
            subcode: Some(sibling.subcode),
        },
    }];
    draft.pages[0].local_program = Some(
        TeletextEnhancementProgram::new(vec![
            TeletextEnhancementCommand::SetPosition {
                position: TeletextEnhancementPosition::new(1, 8).unwrap(),
            },
            TeletextEnhancementCommand::AtColumn {
                column: 8,
                operation,
            },
            TeletextEnhancementCommand::AtColumn {
                column: 8,
                operation: TeletextColumnOperation::G0 {
                    code: TeletextGlyphCode::new(b'Z').unwrap(),
                },
            },
        ])
        .unwrap(),
    );
    // An already saved physical row must survive semantic reconstruction.
    let mut cells = [b' '; 40];
    cells[12] = b'Q';
    declared(draft)
        .with_physical_base_overlays(
            vec![TeletextPhysicalBaseOverlay::new(
                owner,
                vec![TeletextPhysicalBaseRow::new(3, cells).unwrap()],
            )
            .unwrap()],
            semantic_limits(),
        )
        .unwrap()
}

fn compiled(declared: TeletextDeclaredService) -> EncodedTeletextAuthoredService {
    let tx = transmissions(&declared);
    encode_teletext_authored_service(declared, &tx, &limits()).unwrap()
}

fn display(service: &EncodedTeletextAuthoredService) -> &EncodedTeletextCompiledDisplayPage {
    let EncodedTeletextAuthoredPage::CompiledDisplay(page) = &service.pages()[0] else {
        panic!("an explicit local program must use the full compiled display path");
    };
    page
}

fn packet_number(unit: &[u8; 46]) -> u8 {
    use super::source_header::{observe_dvb_teletext_header, TeletextHeaderObservation};
    match observe_dvb_teletext_header(unit).unwrap() {
        TeletextHeaderObservation::Header(header) => header.packet_address.packet_number,
        TeletextHeaderObservation::NotHeader(address) => address.packet_number,
        _ => panic!("authored encoder emitted a non-page unit"),
    }
}

fn x26_words(service: &EncodedTeletextAuthoredService) -> Vec<u32> {
    project_encoded_teletext_display(display(service).page(), 128)
        .unwrap()
        .records
        .into_iter()
        .filter_map(|record| match record {
            TeletextEncodedDisplayRecord::Triplets { row: 26, words, .. } => Some(words),
            _ => None,
        })
        .flatten()
        .collect()
}

fn assert_unchanged_surroundings(
    before: &EncodedTeletextAuthoredService,
    after: &EncodedTeletextAuthoredService,
) {
    assert_eq!(after.inventory(), before.inventory());
    assert_eq!(after.magazine_units(), before.magazine_units());
    assert_eq!(after.declared().mots(), before.declared().mots());
    assert_eq!(
        after.declared().selections(),
        before.declared().selections()
    );
    assert_eq!(
        after.declared().resolutions(),
        before.declared().resolutions()
    );
    assert_eq!(after.declared().defaults(), before.declared().defaults());
    assert_eq!(
        after.declared().empty_object_pages(),
        before.declared().empty_object_pages()
    );
    assert_eq!(
        after.declared().semantic().base(),
        before.declared().semantic().base()
    );
    assert_eq!(after.pages().len(), before.pages().len());
    assert!(
        before.pages().len() > 2,
        "fixture must retain actual resources"
    );
    for (before, after) in before.pages().iter().zip(after.pages()).skip(1) {
        assert_eq!(after.data_units(), before.data_units());
    }
    let preserved = |page: &EncodedTeletextCompiledDisplayPage| {
        page.page()
            .data_units()
            .iter()
            .copied()
            .filter(|unit| packet_number(unit) != 26)
            .collect::<Vec<_>>()
    };
    // Includes every fallback/header byte, actual X27 navigation/CRC and
    // presentation metadata; only X26 may change.
    assert_eq!(preserved(display(after)), preserved(display(before)));
    assert!(display(before)
        .page()
        .data_units()
        .iter()
        .any(|unit| packet_number(unit) == 27));
    assert_eq!(
        display(after)
            .normalized()
            .original()
            .base_display()
            .cell(3, 12)
            .unwrap()
            .code(),
        b'Q'
    );
}

fn assert_edit(
    expected: TeletextColumnOperation,
    replacement: TeletextColumnOperation,
    mode: u32,
    old_data: u32,
    new_data: u32,
) {
    let original = original(expected);
    let unchanged = original.clone();
    let before = compiled(original.clone());
    let owner = original.semantic().base().pages[0].address;
    let overlay =
        TeletextLocalProgramOperandOverlay::new(owner, 1, 8, expected, replacement).unwrap();
    let changed = original
        .clone()
        .with_local_program_operand_overlays(&[overlay], semantic_limits())
        .unwrap();
    let after = compiled(changed.clone());
    assert_eq!(original, unchanged);
    assert_eq!(after.declared(), &changed);
    assert_unchanged_surroundings(&before, &after);
    let old_words = x26_words(&before);
    let new_words = x26_words(&after);
    // EN 300 706 Table 28: address bits 0..5, mode 6..10, data 11..17.
    assert!(old_words.contains(&(8 | (mode << 6) | (old_data << 11))));
    assert!(new_words.contains(&(8 | (mode << 6) | (new_data << 11))));
    assert_ne!(new_words, old_words);
    let actual = display(&after)
        .normalized()
        .physical()
        .replay()
        .attribute_execution()
        .cell(1, 8)
        .unwrap();
    match replacement {
        TeletextColumnOperation::Foreground { colour } => {
            assert_eq!(actual.foreground().value(), Some(colour));
            assert_eq!(actual.foreground().pending(), None);
        }
        TeletextColumnOperation::Background { colour } => {
            assert_eq!(actual.background().value(), Some(colour));
            assert_eq!(actual.background().pending(), None);
        }
        TeletextColumnOperation::Attributes { invert, .. } => {
            assert_eq!(actual.invert().value(), Some(invert));
            assert_eq!(actual.invert().pending(), None);
            assert_eq!(actual.underline_or_separated_mosaics().value(), Some(true));
        }
        _ => unreachable!("fixture uses checked editable families"),
    }
    let recompiled = compiled(original.clone());
    for (before, recompiled) in before.pages().iter().zip(recompiled.pages()) {
        assert_eq!(recompiled.data_units(), before.data_units());
    }
    assert!(matches!(
        changed.with_local_program_operand_overlays(&[overlay], semantic_limits()),
        Err(TeletextServiceError::LocalProgramOperandOverlay {
            reason: TeletextLocalProgramOperandFailure::CommandMismatch,
            ..
        })
    ));
    assert_eq!(original, unchanged);
}

#[test]
fn foreground_edit_recompiles_only_local_x26_and_retains_fallback_and_resources() {
    assert_edit(colour(true, 1), colour(true, 4), 0, 1, 4);
}

#[test]
fn background_edit_recompiles_only_local_x26_and_retains_fallback_and_resources() {
    assert_edit(colour(false, 0), colour(false, 2), 3, 0, 2);
}

#[test]
fn attribute_edit_recompiles_only_local_x26_and_retains_fallback_and_resources() {
    assert_edit(attributes(false), attributes(true), 12, 0, 0x30);
}
