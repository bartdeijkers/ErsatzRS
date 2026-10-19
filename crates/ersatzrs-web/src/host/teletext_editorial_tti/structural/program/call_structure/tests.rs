//! Literal Local X26 atoms exercise parser correspondence, not editor actions.
use super::super::{parse, stream, Candidate, Location};
use ersatzrs_domain::{
    TeletextColumnOperation, TeletextEnhancementCommand as Command,
    TeletextEnhancementObjectCatalog as Catalog, TeletextEnhancementObjectDefinition as Definition,
    TeletextEnhancementPosition, TeletextEnhancementProgram as Program, TeletextGlyphCode,
    TeletextLocalObjectGlyphProgramOverlay, TeletextObjectCatalogBudget, TeletextObjectId,
    TeletextObjectIdentity, TeletextObjectOffset, TeletextObjectType, TeletextPageAddress,
    TeletextPageNumber, TeletextResolvedObjectPageLinks, TeletextSubcode,
};
use ersatzrs_ffmpeg::teletext::{
    allocate_teletext_resource_objects, compile_teletext_local_program_with_objects,
    EncodedTeletextLocalObjectProgram,
};
use std::time::{Duration, Instant};

const AP: u32 = 0x29 | (4 << 6) | (8 << 11);
const B: u32 = 8 | (9 << 6) | (0x42 << 11);
const C: u32 = 9 | (9 << 6) | (0x43 << 11);
const ORIGIN: u32 = 0x28 | (0x10 << 6);
const CALL: u32 = 0x28 | (0x13 << 6) | (1 << 11);
const FINAL: u32 = 0x3f | (0x1f << 6) | (5 << 11);

fn owner() -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(124).unwrap(),
        subcode: TeletextSubcode::new(32).unwrap(),
    }
}
fn identity(id: u16) -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(owner(), TeletextObjectId::new(id))
}
fn call(column: Option<u8>) -> Command {
    Command::InvokeObject {
        target: identity(65520),
        object_type: TeletextObjectType::Passive,
        offset: column.map(|column| TeletextObjectOffset::new(0, column).unwrap()),
    }
}
fn glyph(column: u8, value: u8) -> Command {
    Command::AtColumn {
        column,
        operation: TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(value).unwrap(),
        },
    }
}
fn position() -> Command {
    Command::SetPosition {
        position: TeletextEnhancementPosition::new(1, 8).unwrap(),
    }
}
fn image(body: &[u32]) -> stream::Image {
    let mut words = vec![
        0x3f | (0x1f << 6) | (6 << 11),
        0x28 | (0x17 << 6) | (1 << 11),
        8 | (9 << 6) | (0x41 << 11),
        0x28 | (0x16 << 6) | (3 << 11),
    ];
    words.extend_from_slice(body);
    words.push(FINAL);
    stream::Image {
        end: words.len(),
        words,
        markers: vec![1, 3],
    }
}
struct Fixture {
    definition: Definition,
    catalog: Catalog,
    program: EncodedTeletextLocalObjectProgram,
    old: stream::Image,
}
impl Fixture {
    fn new(commands: Vec<Command>, words: &[u32]) -> Self {
        let budget = TeletextObjectCatalogBudget {
            max_objects: 4,
            max_commands: 208,
        };
        let definition = Definition {
            identity: identity(65522),
            object_type: TeletextObjectType::Adaptive,
            program: Program::new(commands).unwrap(),
        };
        let catalog = Catalog::new(
            vec![
                Definition {
                    identity: identity(65520),
                    object_type: TeletextObjectType::Passive,
                    program: Program::new(vec![glyph(8, b'A')]).unwrap(),
                },
                definition.clone(),
            ],
            budget,
        )
        .unwrap();
        let allocations = allocate_teletext_resource_objects(&catalog, &[], 4).unwrap();
        let program = compile_teletext_local_program_with_objects(
            owner(),
            &Program::new(vec![]).unwrap(),
            &catalog,
            &allocations,
            TeletextResolvedObjectPageLinks {
                public: None,
                global: None,
            },
            budget,
        )
        .unwrap();
        assert_eq!(program.definition_positions()[0].absolute_index(), 1);
        assert_eq!(program.definition_positions()[1].absolute_index(), 3);
        Self {
            definition,
            catalog,
            program,
            old: image(words),
        }
    }
    fn parse(&self, words: &[u32]) -> super::Result<Candidate> {
        let new = image(words);
        let mut aligned = self.old.words.clone();
        let candidate = parse(
            (&self.definition, &self.catalog),
            &self.program,
            (&self.old, &new),
            1,
            &mut aligned,
            Location {
                page: 124,
                subcode: 32,
                resource: None,
            },
            Instant::now() + Duration::from_secs(5),
        )?;
        assert_eq!(
            aligned, self.old.words,
            "call atoms cannot alter scalar reconciliation input"
        );
        Ok(candidate)
    }
}
fn structure(candidate: &Candidate) -> TeletextLocalObjectGlyphProgramOverlay {
    TeletextLocalObjectGlyphProgramOverlay::new_with_invocation_structure(
        candidate.identity,
        candidate.expected.clone(),
        Program::new(candidate.replacement.clone()).unwrap(),
        candidate.retained.clone(),
    )
    .unwrap()
}

#[test]
fn local_call_deletion_preserves_context_and_original_ordinals() {
    let fixture = Fixture::new(
        vec![position(), glyph(8, b'B'), call(Some(0)), glyph(9, b'C')],
        &[AP, B, ORIGIN, CALL, C],
    );
    let candidate = fixture.parse(&[AP, B, C]).unwrap();
    assert_eq!(
        candidate.replacement,
        [position(), glyph(8, b'B'), glyph(9, b'C')]
    );
    assert_eq!(candidate.retained, [Some(0), Some(1), Some(3)]);
    assert!(structure(&candidate).has_invocation_structure_changes());
}

#[test]
fn local_call_insertion_is_one_authenticated_origin_call_atom() {
    let fixture = Fixture::new(
        vec![position(), glyph(8, b'B'), glyph(9, b'C')],
        &[AP, B, C],
    );
    let candidate = fixture.parse(&[AP, B, ORIGIN, CALL, C]).unwrap();
    assert_eq!(
        candidate.replacement,
        [position(), glyph(8, b'B'), call(Some(0)), glyph(9, b'C')]
    );
    assert_eq!(candidate.retained, [Some(0), Some(1), None, Some(2)]);
    assert!(structure(&candidate).has_invocation_structure_changes());
}

#[test]
fn local_call_insertion_retains_mixed_existing_call_atoms_exact_first() {
    let fixture = Fixture::new(
        vec![
            position(),
            glyph(8, b'B'),
            call(None),
            glyph(9, b'C'),
            call(Some(0)),
        ],
        &[AP, B, CALL, C, ORIGIN, CALL],
    );
    let candidate = fixture
        .parse(&[AP, B, ORIGIN | (1 << 11), CALL, CALL, C, ORIGIN, CALL])
        .unwrap();
    assert_eq!(
        candidate.replacement,
        [
            position(),
            glyph(8, b'B'),
            call(Some(1)),
            call(None),
            glyph(9, b'C'),
            call(Some(0)),
        ]
    );
    assert_eq!(
        candidate.retained,
        [Some(0), Some(1), None, Some(2), Some(3), Some(4)]
    );
    assert!(structure(&candidate).has_invocation_structure_changes());
}

#[test]
fn local_leading_ap_before_only_calls_remains_original_and_carrier_stays_prospective() {
    let ap = Command::SetPosition {
        position: TeletextEnhancementPosition::new(0, 8).unwrap(),
    };
    let fixture = Fixture::new(vec![ap], &[]);
    let candidate = fixture.parse(&[8 | (3 << 6), ORIGIN, CALL]).unwrap();
    assert_eq!(candidate.replacement, [ap, call(Some(0))]);
    assert_eq!(candidate.retained, [Some(0), None]);
    assert!(
        candidate.carrier.is_some(),
        "a candidate producer must discharge this hypothesis"
    );
    assert!(structure(&candidate).has_invocation_structure_changes());
    // A duplicate carrier is not promoted to authored styling or swallowed.
    assert!(fixture
        .parse(&[8 | (3 << 6), 8 | (3 << 6), ORIGIN, CALL])
        .is_err());
    assert!(fixture.parse(&[9 | (3 << 6), ORIGIN, CALL]).is_err());
}

#[test]
fn local_only_leading_anchor_keeps_two_calls_without_changing_resource_policy() {
    let ap = Command::SetPosition {
        position: TeletextEnhancementPosition::new(0, 8).unwrap(),
    };
    let row_call = Command::InvokeObject {
        target: identity(65520),
        object_type: TeletextObjectType::Passive,
        offset: Some(TeletextObjectOffset::new(1, 0).unwrap()),
    };
    let old = [(0, ap), (1, call(Some(0)))];
    let new = [call(Some(0)), row_call];
    let location = Location {
        page: 124,
        subcode: 32,
        resource: None,
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    let aligned = super::super::alignment::local_calls(&old, &new, location, deadline).unwrap();
    assert_eq!(aligned.commands, [ap, call(Some(0)), row_call]);
    assert_eq!(aligned.retained, [Some(0), Some(1), None]);
    assert!(super::super::alignment::resource_calls(&old, &new, 208, location, deadline).is_err());
}

#[test]
fn local_call_structure_rejects_split_atoms_noncanonical_targets_and_context_changes() {
    let fixture = Fixture::new(
        vec![position(), glyph(8, b'B'), call(Some(0)), glyph(9, b'C')],
        &[AP, B, ORIGIN, CALL, C],
    );
    for words in [
        vec![AP, B, ORIGIN],
        vec![AP, B, ORIGIN, C, CALL],
        vec![AP, B, ORIGIN, ORIGIN, CALL, C],
        vec![AP, B, ORIGIN | (0x48 << 11), CALL, C],
        vec![AP, B, CALL | 2, C],
        vec![AP, B, 0x28 | (0x13 << 6) | (2 << 11), C],
        vec![AP, B, 0x28 | (0x11 << 6) | (1 << 11), C],
        vec![0x2a | (4 << 6) | (8 << 11), B, C],
    ] {
        assert!(
            fixture.parse(&words).is_err(),
            "refused literal words {words:x?}"
        );
    }
}
