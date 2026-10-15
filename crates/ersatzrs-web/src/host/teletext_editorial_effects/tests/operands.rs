//! Exact local semantic intent and non-spatial effect permissions.
use super::*;
use ersatzrs_ffmpeg::teletext::TeletextCompiledCommandPosition;

type WordMutation = (usize, fn(u32) -> u32);

fn foreground(value: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::Foreground {
        colour: TeletextColourReference::new(value).unwrap(),
    }
}
fn background(value: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::Background {
        colour: TeletextColourReference::new(value).unwrap(),
    }
}
fn attributes(invert: bool) -> TeletextColumnOperation {
    TeletextColumnOperation::Attributes {
        double_height: false,
        double_width: false,
        boxed: false,
        conceal: false,
        invert,
        underline_or_separated_mosaics: false,
    }
}
fn operand(page: u16, old: u8, new: u8) -> TeletextLocalProgramOperandOverlay {
    TeletextLocalProgramOperandOverlay::new(address(page), 1, 8, foreground(old), foreground(new))
        .unwrap()
}
fn local_service(first: u8, second: u8, background_colour: u8) -> EncodedTeletextAuthoredService {
    local_service_at(first, second, background_colour, 1)
}
fn local_service_at(
    first: u8,
    second: u8,
    background_colour: u8,
    row: u8,
) -> EncodedTeletextAuthoredService {
    let policy = policy();
    let lifted = ersatzrs_ffmpeg::teletext::lift_teletext_v1_page_set(
        draft("ASCII"),
        TeletextEncodingRegion::WesternLatin,
        policy.validation.service,
    )
    .unwrap();
    let mut pages: Vec<_> = lifted.page_enhancements().values().cloned().collect();
    for page in &mut pages {
        page.local_program = Some(
            TeletextEnhancementProgram::new(vec![
                TeletextEnhancementCommand::SetPosition {
                    position: TeletextEnhancementPosition::new(row, if row == 0 { 0 } else { 8 })
                        .unwrap(),
                },
                TeletextEnhancementCommand::AtColumn {
                    column: 8,
                    operation: foreground(if page.address == address(123) {
                        first
                    } else {
                        second
                    }),
                },
                TeletextEnhancementCommand::AtColumn {
                    column: 8,
                    operation: background(background_colour),
                },
                TeletextEnhancementCommand::AtColumn {
                    column: 8,
                    operation: attributes(false),
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
    }
    let semantic = TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base: lifted.base().clone(),
            pages,
            magazines: vec![],
            objects: vec![],
            drcs: vec![],
        },
        policy.validation.service,
    )
    .unwrap();
    let declared = TeletextDeclaredService::new(
        semantic,
        vec![],
        [123, 124]
            .map(|page| TeletextDisplayMotSelection {
                display: address(page),
                mot: None,
            })
            .into(),
        vec![],
        policy.validation.declarations,
    )
    .unwrap();
    ersatzrs_ffmpeg::teletext::encode_teletext_authored_service_before(
        declared,
        &[],
        &policy.encoding,
        deadline(),
    )
    .unwrap()
}
fn edits(operands: &[TeletextLocalProgramOperandOverlay]) -> TeletextEditorialEdits<'_> {
    TeletextEditorialEdits {
        rows: &[],
        operands,
    }
}
#[test]
fn exact_foreground_only_edit_is_allowed_for_its_complete_effect_range() {
    let original = local_service(1, 1, 0);
    let edited = local_service(4, 1, 0);
    validate_teletext_editorial_effects_with_operands(
        &original,
        &edited,
        &project("free", 1, 8, 32, 1),
        edits(&[operand(123, 1, 4)]),
        &policy().encoding,
        deadline(),
    )
    .unwrap();
    assert_ne!(
        original.pages()[0].data_units(),
        edited.pages()[0].data_units()
    );
    assert_eq!(
        original.pages()[1].data_units(),
        edited.pages()[1].data_units()
    );
    // The legacy rows-only API cannot authorize the exact same packet mutation.
    assert_eq!(
        validate_teletext_editorial_effects(
            &original,
            &edited,
            &project("free", 1, 8, 32, 1),
            &[],
            &policy().encoding,
            deadline(),
        )
        .unwrap_err()
        .reason,
        TeletextEditorialEffectsFailure::ProtectedStructure,
    );
}
#[test]
fn command_inside_zone_cannot_change_protected_cells_to_its_right() {
    let failure = validate_teletext_editorial_effects_with_operands(
        &local_service(1, 1, 0),
        &local_service(4, 1, 0),
        &project("zone", 1, 8, 1, 1),
        edits(&[operand(123, 1, 4)]),
        &policy().encoding,
        deadline(),
    )
    .unwrap_err();
    assert_eq!(
        failure.reason,
        TeletextEditorialEffectsFailure::ProtectedDisplay
    );
    assert_eq!(failure.page, Some(address(123)));
    assert_eq!(failure.position, Some((1, 9)));
}
#[test]
fn undeclared_second_operand_change_rejects_even_in_completely_free_page() {
    let failure = validate_teletext_editorial_effects_with_operands(
        &local_service(1, 1, 0),
        &local_service(4, 1, 2),
        &project("free", 1, 0, 40, 24),
        edits(&[operand(123, 1, 4)]),
        &policy().encoding,
        deadline(),
    )
    .unwrap_err();
    assert_eq!(
        failure.reason,
        TeletextEditorialEffectsFailure::ProtectedStructure
    );
}
#[test]
fn latest_permissions_do_not_reauthorize_unchanged_prior_owner() {
    let latest = local_service(1, 4, 0);
    let edited = local_service(2, 4, 0);
    let incoming = [operand(123, 1, 2)];
    let cumulative = [operand(123, 1, 2), operand(124, 1, 4)];
    validate_teletext_editorial_transition_with_operands(
        &latest,
        &edited,
        // Only page 123 occurs in this receipt's editable metadata.
        &project("free", 1, 8, 32, 1),
        edits(&incoming),
        edits(&cumulative),
        &policy().encoding,
        deadline(),
    )
    .unwrap();
}
#[test]
fn latest_operand_expectation_and_explicit_root_revert_are_distinct() {
    let latest = local_service(4, 1, 0);
    let reverted = local_service(1, 1, 0);
    validate_teletext_editorial_transition_with_operands(
        &latest,
        &reverted,
        &project("free", 1, 8, 32, 1),
        edits(&[operand(123, 4, 1)]),
        edits(&[]),
        &policy().encoding,
        deadline(),
    )
    .unwrap();
    let failure = validate_teletext_editorial_transition_with_operands(
        &latest,
        &local_service(2, 1, 0),
        &project("free", 1, 8, 32, 1),
        edits(&[operand(123, 1, 2)]),
        edits(&[operand(123, 1, 2)]),
        &policy().encoding,
        deadline(),
    )
    .unwrap_err();
    assert_eq!(failure.reason, TeletextEditorialEffectsFailure::Association);
}
#[test]
fn final_operand_association_and_raw_limits_reject_before_effect_permissions() {
    let original = local_service(1, 1, 0);
    let edited = local_service(4, 1, 0);
    let op = operand(123, 1, 4);
    for cumulative in [vec![operand(123, 1, 2)], vec![op, op]] {
        let failure = validate_teletext_editorial_transition_with_operands(
            &original,
            &edited,
            &project("free", 1, 8, 32, 1),
            edits(&[op]),
            edits(&cumulative),
            &policy().encoding,
            deadline(),
        )
        .unwrap_err();
        assert_eq!(failure.reason, TeletextEditorialEffectsFailure::Association);
    }
    let mut budget = policy().encoding;
    budget.max_commands = 0;
    let failure = validate_teletext_editorial_effects_with_operands(
        &original,
        &edited,
        &project("free", 1, 8, 32, 1),
        edits(&[op]),
        &budget,
        deadline(),
    )
    .unwrap_err();
    assert_eq!(failure.reason, TeletextEditorialEffectsFailure::Budget);
}

#[test]
fn background_and_nonspacing_attributes_use_the_entire_effect_range() {
    let original = local_service(1, 1, 0);
    for (command, old, new) in [
        (2, background(0), background(2)),
        (3, attributes(false), attributes(true)),
    ] {
        let operands =
            [
                TeletextLocalProgramOperandOverlay::new(address(123), command, 8, old, new)
                    .unwrap(),
            ];
        let declared = original
            .declared()
            .clone()
            .with_local_program_operand_overlays(&operands, policy().validation.service)
            .unwrap();
        let edited = ersatzrs_ffmpeg::teletext::encode_teletext_authored_service_before(
            declared,
            &[],
            &policy().encoding,
            deadline(),
        )
        .unwrap();
        validate_teletext_editorial_effects_with_operands(
            &original,
            &edited,
            &project("free", 1, 8, 32, 1),
            edits(&operands),
            &policy().encoding,
            deadline(),
        )
        .unwrap();
        let failure = validate_teletext_editorial_effects_with_operands(
            &original,
            &edited,
            &project("zone", 1, 8, 1, 1),
            edits(&operands),
            &policy().encoding,
            deadline(),
        )
        .unwrap_err();
        assert_eq!(
            failure.reason,
            TeletextEditorialEffectsFailure::ProtectedDisplay
        );
        assert_eq!(failure.position, Some((1, 9)));
    }
}

use crate::host::teletext_editorial_tti::{
    reconcile_teletext_editorial_tti, reconcile_teletext_editorial_tti_with_operands,
    reconcile_teletext_editorial_tti_with_status, validate_teletext_editorial_tti_encoding,
    NativeTeletextEditorialTtiFailure,
};
use ersatzrs_app::{
    teletext_export_project::TeletextExportPageMapping,
    teletext_tti::{
        write_teletext_tti, TeletextTtiBudget, TeletextTtiCycleUnit, TeletextTtiDocument,
        TeletextTtiRecord,
    },
};
use ersatzrs_ffmpeg::teletext::{EncodedTeletextAuthoredPage, TeletextEncodedDisplayRecord};

fn tti_budget() -> TeletextTtiBudget {
    TeletextTtiBudget {
        max_bytes: 16_384,
        max_records: 128,
    }
}
fn tti_mapping() -> TeletextExportPageMapping {
    project("free", 1, 8, 32, 1).pages.remove(0)
}
fn tti_records(service: &EncodedTeletextAuthoredService) -> Vec<TeletextTtiRecord> {
    let mut records = vec![
        TeletextTtiRecord::PageNumber {
            page: 0x123,
            editor_ordinal: 0,
        },
        TeletextTtiRecord::SubpageOrdinal(0),
        TeletextTtiRecord::PageStatus(0x8040),
        TeletextTtiRecord::Region(0),
        TeletextTtiRecord::Cycle {
            value: 20,
            unit: TeletextTtiCycleUnit::Time,
        },
    ];
    let page = service
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::CompiledDisplay(page)
                if page.page().address() == address(123) =>
            {
                Some(page.page())
            }
            _ => None,
        })
        .unwrap();
    records.extend(
        project_encoded_teletext_display(page, service.unit_count())
            .unwrap()
            .records
            .into_iter()
            .filter_map(|record| match record {
                TeletextEncodedDisplayRecord::Triplets {
                    row: 26,
                    designation,
                    words,
                } => Some(TeletextTtiRecord::Packet {
                    row: 26,
                    designation,
                    words,
                }),
                _ => None,
            }),
    );
    records
}
fn tti_bytes(records: Vec<TeletextTtiRecord>) -> Vec<u8> {
    write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, tti_budget()).unwrap(),
        tti_budget(),
    )
    .unwrap()
}
fn native_word(service: &EncodedTeletextAuthoredService, command: usize) -> usize {
    service
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::CompiledDisplay(page)
                if page.page().address() == address(123) =>
            {
                page.normalized()
                    .encoded()
                    .command_positions()
                    .iter()
                    .find(|position| position.original_local_command() == Some(command))
                    .map(TeletextCompiledCommandPosition::semantic_word)
            }
            _ => None,
        })
        .unwrap()
}
fn mutate_word(records: &mut [TeletextTtiRecord], word: usize, change: impl FnOnce(u32) -> u32) {
    let words = records
        .iter_mut()
        .find_map(|record| match record {
            TeletextTtiRecord::Packet {
                row: 26,
                designation,
                words,
            } if usize::from(*designation) == word / 13 => Some(words),
            _ => None,
        })
        .unwrap();
    words[word % 13] = change(words[word % 13]);
}
fn apply_operand(
    service: &EncodedTeletextAuthoredService,
    operands: &[TeletextLocalProgramOperandOverlay],
) -> EncodedTeletextAuthoredService {
    ersatzrs_ffmpeg::teletext::encode_teletext_authored_service_before(
        service
            .declared()
            .clone()
            .with_local_program_operand_overlays(operands, policy().validation.service)
            .unwrap(),
        &[],
        &policy().encoding,
        deadline(),
    )
    .unwrap()
}

#[test]
fn native_tti_recognizes_exact_existing_operands_and_matches_complete_reencoding() {
    let original = local_service(1, 1, 0);
    let baseline = tti_bytes(tti_records(&original));
    for (command, expected, replacement, data) in [
        (1, foreground(1), foreground(4), 4),
        (2, background(0), background(2), 2),
        (3, attributes(false), attributes(true), 16),
    ] {
        let mut records = tti_records(&original);
        mutate_word(&mut records, native_word(&original, command), |word| {
            (word & 0x7ff) | (data << 11)
        });
        let edited = tti_bytes(records);
        let changes = reconcile_teletext_editorial_tti_with_operands(
            &tti_mapping(),
            &baseline,
            &edited,
            tti_budget(),
            deadline(),
            0x8040,
            &original,
        )
        .unwrap();
        assert!(changes.rows.is_empty());
        assert_eq!(
            changes.operands,
            [TeletextLocalProgramOperandOverlay::new(
                address(123),
                command,
                8,
                expected,
                replacement
            )
            .unwrap()]
        );
        let candidate = apply_operand(&original, &changes.operands);
        validate_teletext_editorial_tti_encoding(
            &tti_mapping(),
            &edited,
            tti_budget(),
            deadline(),
            &candidate,
        )
        .unwrap();
        // Recognized intent does not by itself grant the caller packet-edit permission.
        assert_eq!(
            reconcile_teletext_editorial_tti(
                &tti_mapping(),
                &baseline,
                &edited,
                tti_budget(),
                deadline()
            )
            .unwrap_err()
            .failure,
            NativeTeletextEditorialTtiFailure::Packet
        );
        assert_eq!(
            reconcile_teletext_editorial_tti_with_status(
                &tti_mapping(),
                &baseline,
                &edited,
                tti_budget(),
                deadline(),
                0x8040
            )
            .unwrap_err()
            .failure,
            NativeTeletextEditorialTtiFailure::Packet
        );
        let mismatch = validate_teletext_editorial_tti_encoding(
            &tti_mapping(),
            &edited,
            tti_budget(),
            deadline(),
            &original,
        )
        .unwrap_err();
        assert_eq!(mismatch.failure, NativeTeletextEditorialTtiFailure::Packet);
        assert_eq!(
            (mismatch.page, mismatch.subcode, mismatch.row),
            (123, 16, Some(26))
        );
    }
}

#[test]
fn native_tti_rejects_address_mode_reserved_geometry_glyph_position_and_trailer_edits() {
    let original = local_service(1, 1, 0);
    let baseline = tti_bytes(tti_records(&original));
    let packet_words: Vec<_> = tti_records(&original)
        .into_iter()
        .filter_map(|record| match record {
            TeletextTtiRecord::Packet {
                row: 26,
                designation,
                words,
            } => Some((designation, words)),
            _ => None,
        })
        .collect();
    let trailer = packet_words
        .iter()
        .flat_map(|(designation, words)| {
            words
                .iter()
                .enumerate()
                .map(move |(slot, word)| (usize::from(*designation) * 13 + slot, *word))
        })
        .find(|(_, word)| (word >> 6) & 31 == 31)
        .unwrap()
        .0;
    let cases: [WordMutation; 8] = [
        (native_word(&original, 1), |word| word ^ 1),
        (native_word(&original, 1), |word| word ^ (1 << 6)),
        (native_word(&original, 1), |word| {
            (word & 0x7ff) | (32 << 11)
        }),
        (native_word(&original, 3), |word| word | (8 << 11)),
        (native_word(&original, 3), |word| word | (1 << 11)),
        (native_word(&original, 4), |word| word ^ (1 << 11)),
        (native_word(&original, 0), |word| word ^ (1 << 11)),
        (trailer, |word| word ^ (1 << 11)),
    ];
    for (word, change) in cases {
        let mut records = tti_records(&original);
        mutate_word(&mut records, word, change);
        let failure = reconcile_teletext_editorial_tti_with_operands(
            &tti_mapping(),
            &baseline,
            &tti_bytes(records),
            tti_budget(),
            deadline(),
            0x8040,
            &original,
        )
        .err()
        .unwrap();
        assert_eq!(failure.failure, NativeTeletextEditorialTtiFailure::Packet);
        assert_eq!(
            (
                failure.page,
                failure.subcode,
                failure.row,
                failure.column,
                failure.designation
            ),
            (
                123,
                16,
                Some(26),
                Some(word % 13),
                Some(u8::try_from(word / 13).unwrap())
            )
        );
    }
}

#[test]
fn native_tti_rejects_packet_presence_changes_and_uploaded_baseline_forgery() {
    let original = local_service(1, 1, 0);
    let records = tti_records(&original);
    let baseline = tti_bytes(records.clone());
    let mut dropped = records.clone();
    dropped.retain(|record| {
        !matches!(
            record,
            TeletextTtiRecord::Packet {
                row: 26,
                designation: 0,
                ..
            }
        )
    });
    let mut added = records.clone();
    added.push(TeletextTtiRecord::Packet {
        row: 26,
        designation: 15,
        words: [0; 13],
    });
    for records in [dropped, added] {
        let failure = reconcile_teletext_editorial_tti_with_operands(
            &tti_mapping(),
            &baseline,
            &tti_bytes(records),
            tti_budget(),
            deadline(),
            0x8040,
            &original,
        )
        .err()
        .unwrap();
        assert_eq!(failure.failure, NativeTeletextEditorialTtiFailure::Packet);
        assert_eq!(
            (failure.page, failure.subcode, failure.row),
            (123, 16, Some(26))
        );
        assert!(failure.designation.is_some());
    }
    let mut forged = records;
    let word = native_word(&original, 1);
    mutate_word(&mut forged, word, |word| (word & 0x7ff) | (7 << 11));
    let forged = tti_bytes(forged);
    let failure = reconcile_teletext_editorial_tti_with_operands(
        &tti_mapping(),
        &forged,
        &forged,
        tti_budget(),
        deadline(),
        0x8040,
        &original,
    )
    .err()
    .unwrap();
    assert_eq!(failure.failure, NativeTeletextEditorialTtiFailure::Packet);
    assert_eq!((failure.row, failure.column), (Some(26), Some(word % 13)));
}

#[test]
fn native_tti_rejects_actual_generated_row_zero_compatibility_selector() {
    let original = local_service_at(1, 1, 0, 0);
    let selector = original
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::CompiledDisplay(page)
                if page.page().address() == address(123) =>
            {
                page.normalized()
                    .encoded()
                    .command_positions()
                    .iter()
                    .find(|position| position.first_word() != position.semantic_word())
                    .map(TeletextCompiledCommandPosition::first_word)
            }
            _ => None,
        })
        .expect("the native row-zero compiler must emit its compatibility selector");
    let records = tti_records(&original);
    let baseline = tti_bytes(records.clone());
    let mut edited = records;
    mutate_word(&mut edited, selector, |word| word ^ (1 << 11));
    let failure = reconcile_teletext_editorial_tti_with_operands(
        &tti_mapping(),
        &baseline,
        &tti_bytes(edited),
        tti_budget(),
        deadline(),
        0x8040,
        &original,
    )
    .err()
    .unwrap();
    assert_eq!(failure.failure, NativeTeletextEditorialTtiFailure::Packet);
    assert_eq!(
        (failure.row, failure.column, failure.designation),
        (
            Some(26),
            Some(selector % 13),
            Some(u8::try_from(selector / 13).unwrap())
        )
    );
}
