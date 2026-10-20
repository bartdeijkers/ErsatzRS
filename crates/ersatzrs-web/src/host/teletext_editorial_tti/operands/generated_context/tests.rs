//! Pure correspondence proof; the native import fixture proves source/factory use.
use super::*;
use ersatzrs_domain::{TeletextEnhancementCharsetDesignation, TeletextPageNumber, TeletextSubcode};
use std::time::Duration;

fn location() -> Location {
    Location {
        page: 200,
        subcode: 0,
        resource: None,
    }
}

fn deadline() -> Instant {
    Instant::now().checked_add(Duration::from_secs(30)).unwrap()
}

fn owner() -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(200).unwrap(),
        subcode: TeletextSubcode::new(0).unwrap(),
    }
}

fn charset(column: u8, value: u8) -> u32 {
    u32::from(column) | (8 << 6) | (u32::from(value) << 11)
}

fn scalar(command: usize, column: u8, value: u8) -> Atom<'static> {
    Atom::Scalar {
        command,
        column,
        expected: TeletextColumnOperation::DesignateCharset {
            designation: TeletextEnhancementCharsetDesignation::new(value).unwrap(),
        },
        before: charset(column, value),
    }
}

fn parsed(atoms: &[Atom<'_>], upload: &[u32]) -> Result<TeletextEditorialChanges> {
    let assigned = alignment::assign(atoms, upload, location(), deadline())?;
    changes(owner(), atoms, &assigned, location(), deadline())
}

#[test]
fn generated_context_reconciles_same_packing_removal_and_insertion_without_owning_base() {
    let position = [0x2a | (4 << 11)];
    let glyph = [8 | (17 << 6) | (101 << 11)];
    let original = [charset(8, 0), glyph[0], charset(8, 0x57)];
    for (old_charset, new_charset, old_group, new_group) in [
        (
            0x57,
            0x37,
            original.as_slice(),
            vec![charset(8, 0), glyph[0], charset(8, 0x37)],
        ),
        (0x57, 0, original.as_slice(), vec![glyph[0]]),
        (0, 0x57, glyph.as_slice(), original.to_vec()),
    ] {
        let atoms = [
            Atom::Fixed(&position),
            scalar(1, 0, old_charset),
            Atom::Glyph {
                words: &glyph,
                original: old_group,
                column: 8,
            },
        ];
        let uploaded = [position.as_slice(), &[charset(0, new_charset)], &new_group].concat();
        let result = parsed(&atoms, &uploaded).unwrap();
        assert_eq!(result.operands.len(), 1);
        let edit = &result.operands[0];
        assert_eq!(edit.owner(), owner());
        assert_eq!(edit.command(), 1);
        assert_eq!(edit.column(), 0);
        assert_eq!(
            edit.expected(),
            TeletextColumnOperation::DesignateCharset {
                designation: TeletextEnhancementCharsetDesignation::new(old_charset).unwrap()
            }
        );
        assert_eq!(
            edit.replacement(),
            TeletextColumnOperation::DesignateCharset {
                designation: TeletextEnhancementCharsetDesignation::new(new_charset).unwrap()
            }
        );
        assert!(result.base_text_edits.is_empty());
        assert!(result.main_programs.is_empty());
        assert!(result.object_operands.is_empty());
    }
}

#[test]
fn generated_context_refuses_generated_only_edits_invalid_spelling_and_protected_words() {
    let fixed = [0x2a | (4 << 11)];
    let glyph = [8 | (17 << 6) | (101 << 11)];
    let atoms = [
        Atom::Fixed(&fixed),
        scalar(1, 0, 0x57),
        Atom::Glyph {
            words: &glyph,
            original: &glyph,
            column: 8,
        },
    ];
    for uploaded in [
        vec![fixed[0], charset(0, 0x57), charset(8, 0), glyph[0]], // generated only
        vec![fixed[0], charset(0, 0x37), glyph[0]],                // context unchanged
        vec![fixed[0] ^ 1, charset(0, 0x37), charset(8, 0), glyph[0]],
        vec![fixed[0], charset(1, 0x37), charset(8, 0), glyph[0]],
        vec![fixed[0], charset(0, 0x37), charset(9, 0), glyph[0]],
        vec![fixed[0], charset(0, 0x37), charset(8, 0), glyph[0] ^ 1],
        vec![
            fixed[0],
            charset(0, 0x37),
            charset(8, 0) | (1 << 18),
            glyph[0],
        ],
        vec![
            fixed[0],
            charset(0, 0x37),
            charset(8, 0),
            charset(8, 0),
            glyph[0],
        ],
    ] {
        assert!(parsed(&atoms, &uploaded).is_err());
    }
}

#[test]
fn generated_context_rejects_distinct_scalar_assignments_but_not_equal_context_paths() {
    let first = [8 | (9 << 6) | (65 << 11)];
    let second = [8 | (9 << 6) | (66 << 11)];
    let original = [charset(8, 0), first[0]];
    let atoms = [
        Atom::Glyph {
            words: &first,
            original: &original,
            column: 8,
        },
        scalar(1, 8, 0),
        Atom::Glyph {
            words: &second,
            original: &second,
            column: 8,
        },
    ];
    assert!(parsed(
        &atoms,
        &[first[0], charset(8, 0x37), charset(8, 0x57), second[0]]
    )
    .is_err());
    let unambiguous = parsed(
        &atoms,
        &[first[0], charset(8, 0x37), charset(8, 0x37), second[0]],
    )
    .unwrap();
    assert_eq!(unambiguous.operands.len(), 1);
}

fn indexed(packets: &[[u32; 13]]) -> Page<'_> {
    let mut page = Page::default();
    for (index, packet) in packets.iter().enumerate() {
        page.packets[0][index] = Some(packet);
    }
    page
}

#[test]
fn generated_context_requires_complete_contiguous_final_padded_packets() {
    assert!(words(&indexed(&[[16383; 13]]), location(), deadline())
        .unwrap()
        .is_empty());
    assert!(words(&indexed(&[[12287; 13]]), location(), deadline()).is_err());
    let mut packet = [FINAL; 13];
    packet[0] = charset(0, 0x37);
    assert_eq!(
        words(&indexed(&[packet]), location(), deadline()).unwrap(),
        [packet[0]]
    );
    assert!(words(&Page::default(), location(), deadline()).is_err());
    assert!(words(&indexed(&[[0; 13]]), location(), deadline()).is_err());
    let mut trailing = packet;
    trailing[2] = 0;
    assert!(words(&indexed(&[trailing]), location(), deadline()).is_err());
    assert!(words(&indexed(&[packet, [FINAL; 13]]), location(), deadline()).is_err());
    let mut gap = Page::default();
    gap.packets[0][0] = Some(&packet);
    gap.packets[0][2] = Some(&packet);
    assert!(words(&gap, location(), deadline()).is_err());
}

#[test]
fn generated_context_checks_deadline_and_raw_bounds_before_alignment_storage() {
    let atoms = std::iter::repeat_with(|| Atom::Fixed(&[0]))
        .take(CAPACITY + 1)
        .collect::<Vec<_>>();
    assert!(alignment::assign(&atoms, &[], location(), deadline()).is_err());
    assert!(alignment::assign(&[], &vec![0; CAPACITY + 1], location(), deadline()).is_err());
    let expired = Instant::now().checked_sub(Duration::from_secs(1)).unwrap();
    let failure = alignment::assign(&[], &[], location(), expired).unwrap_err();
    assert_eq!(
        failure.failure,
        super::super::super::NativeTeletextEditorialTtiFailure::TimedOut
    );
    assert!(words(&Page::default(), location(), expired).is_err());
}
