//! Physical correspondence fixtures; production source binding is native-tested.
use super::{correspondence, program, stream, units, Command, Location, Unit};
use ersatzrs_domain::{
    TeletextColumnOperation, TeletextEnhancementObjectCatalog as Catalog,
    TeletextEnhancementObjectDefinition as Definition, TeletextEnhancementPosition,
    TeletextEnhancementProgram as Program, TeletextGlyphCode,
    TeletextMainLocalInvocationProgramOverlay, TeletextObjectCatalogBudget, TeletextObjectId,
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
const ORIGIN: u32 = 0x28 | (0x10 << 6);
const END: u32 = 0x3f | (0x1f << 6) | (6 << 11);
fn owner() -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(124).unwrap(),
        subcode: TeletextSubcode::new(32).unwrap(),
    }
}
fn call(column: u8) -> Command {
    Command::InvokeObject {
        target: TeletextObjectIdentity::local(owner(), TeletextObjectId::new(65520)),
        object_type: TeletextObjectType::Passive,
        offset: Some(TeletextObjectOffset::new(0, column).unwrap()),
    }
}
fn base() -> [Command; 2] {
    [
        Command::SetPosition {
            position: TeletextEnhancementPosition::new(1, 8).unwrap(),
        },
        Command::AtColumn {
            column: 8,
            operation: TeletextColumnOperation::G0 {
                code: TeletextGlyphCode::new(b'B').unwrap(),
            },
        },
    ]
}
fn pointer(index: u8) -> u32 {
    0x28 | (0x13 << 6) | (u32::from(index) << 11)
}
fn image(prefix: &[u32]) -> stream::Image {
    assert!(prefix.len() < 13);
    let index = u32::try_from(prefix.len()).unwrap();
    let mut words = prefix.to_vec();
    words.extend([
        0x28 | (0x17 << 6) | (index << 11),
        9 << 6 | (0x41 << 11),
        0x3f | (0x1f << 6) | (5 << 11),
    ]);
    stream::Image {
        end: words.len(),
        words,
        markers: vec![prefix.len()],
    }
}
struct Fixture {
    original: Program,
    catalog: Catalog,
    native: EncodedTeletextLocalObjectProgram,
    old: stream::Image,
}
impl Fixture {
    fn new(commands: Vec<Command>, prefix: &[u32]) -> Self {
        let original = Program::new(commands).unwrap();
        let budget = TeletextObjectCatalogBudget {
            max_objects: 4,
            max_commands: 208,
        };
        let catalog = Catalog::new(
            vec![Definition {
                identity: TeletextObjectIdentity::local(owner(), TeletextObjectId::new(65520)),
                object_type: TeletextObjectType::Passive,
                program: Program::new(vec![Command::AtColumn {
                    column: 0,
                    operation: TeletextColumnOperation::G0 {
                        code: TeletextGlyphCode::new(b'A').unwrap(),
                    },
                }])
                .unwrap(),
            }],
            budget,
        )
        .unwrap();
        let allocations = allocate_teletext_resource_objects(&catalog, &[], 4).unwrap();
        let native = compile_teletext_local_program_with_objects(
            owner(),
            &original,
            &catalog,
            &allocations,
            TeletextResolvedObjectPageLinks {
                public: None,
                global: None,
            },
            budget,
        )
        .unwrap();
        assert_eq!(
            usize::from(native.definition_positions()[0].absolute_index()),
            prefix.len()
        );
        assert_eq!(native.main_triplet_count() + 1, prefix.len());
        Self {
            original,
            catalog,
            native,
            old: image(prefix),
        }
    }
    fn physical(&self, prefix: &[u32]) -> super::Result<TeletextMainLocalInvocationProgramOverlay> {
        self.physical_with_scalars(prefix).map(|(intent, _)| intent)
    }
    fn physical_with_scalars(
        &self,
        prefix: &[u32],
    ) -> super::Result<(TeletextMainLocalInvocationProgramOverlay, Vec<(usize, u32)>)> {
        let location = Location {
            page: 124,
            subcode: 32,
            resource: None,
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        let new = image(prefix);
        let mut old = units(&self.catalog, &self.native, &self.old, location, deadline)?;
        // Standalone packing exposes actual command ordinals, while the complete
        // factory independently binds original_local_command in production.
        for position in self
            .native
            .command_positions()
            .iter()
            .filter(|position| position.source().object.is_none())
        {
            let unit = old
                .iter_mut()
                .find(|unit| unit.last == position.semantic_word())
                .unwrap();
            let ordinal = position.source().command;
            unit.original = Some(ordinal);
            unit.editable = matches!(
                self.original.commands()[ordinal],
                Command::InvokeObject { .. }
            );
            if position.first_word() == position.semantic_word() {
                if let Command::AtColumn { column, operation } = self.original.commands()[ordinal] {
                    unit.scalar = Some((column, operation));
                }
            }
        }
        let new_units = units(&self.catalog, &self.native, &new, location, deadline)?;
        let steps =
            correspondence::align((&old, &new_units), (&self.old, &new), location, deadline)?;
        let scalars = correspondence::scalars(
            &self.original,
            (&old, &new_units),
            &steps,
            (&self.old, &new),
            owner(),
            location,
            deadline,
        )?;
        let (replacement, retained) = correspondence::assemble(
            &scalars.expected,
            (&old, &new_units),
            &steps,
            location,
            deadline,
        )?;
        let intent = TeletextMainLocalInvocationProgramOverlay::new(
            owner(),
            scalars.expected,
            Program::new(replacement).unwrap(),
            retained,
        )
        .map_err(|_| super::fail(location))?;
        Ok((intent, scalars.words))
    }
}

#[test]
fn main_call_deletion_relocates_marker_and_keeps_noncall_ordinals() {
    let [ap, glyph] = base();
    let fixture = Fixture::new(
        vec![ap, call(0), glyph, call(1)],
        &[
            AP,
            ORIGIN,
            pointer(7),
            B,
            ORIGIN | (1 << 11),
            pointer(7),
            END,
        ],
    );
    let overlay = fixture
        .physical(&[AP, B, ORIGIN | (1 << 11), pointer(5), END])
        .unwrap();
    assert_eq!(overlay.replacement().commands(), &[ap, glyph, call(1)]);
    assert_eq!(overlay.retained_commands(), &[Some(0), Some(2), Some(3)]);
}

#[test]
fn main_call_insertion_and_balanced_replacement_never_retarget_retained_calls() {
    let [ap, glyph] = base();
    let fixture = Fixture::new(vec![ap, glyph, call(0)], &[AP, B, ORIGIN, pointer(5), END]);
    let overlay = fixture
        .physical(&[
            AP,
            ORIGIN | (1 << 11),
            pointer(7),
            B,
            ORIGIN,
            pointer(7),
            END,
        ])
        .unwrap();
    assert_eq!(
        overlay.replacement().commands(),
        &[ap, call(1), glyph, call(0)]
    );
    assert_eq!(
        overlay.retained_commands(),
        &[Some(0), None, Some(1), Some(2)]
    );
    let overlay = fixture
        .physical(&[AP, B, ORIGIN | (1 << 11), pointer(5), END])
        .unwrap();
    assert_eq!(overlay.retained_commands(), &[Some(0), Some(1), None]);
}

#[test]
fn main_existing_glyph_change_and_call_deletion_patch_original_word_before_structure() {
    let [ap, glyph] = base();
    let fixture = Fixture::new(
        vec![ap, call(0), glyph, call(1)],
        &[
            AP,
            ORIGIN,
            pointer(7),
            B,
            ORIGIN | (1 << 11),
            pointer(7),
            END,
        ],
    );
    let c = 8 | (9 << 6) | (0x43 << 11);
    let (intent, words) = fixture
        .physical_with_scalars(&[AP, c, ORIGIN | (1 << 11), pointer(5), END])
        .unwrap();
    let replacement = Command::AtColumn {
        column: 8,
        operation: TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(b'C').unwrap(),
        },
    };
    assert_eq!(
        words,
        [(3, c)],
        "the shifted upload index1 must patch original packed index3"
    );
    assert_eq!(
        intent.expected().commands(),
        &[ap, call(0), replacement, call(1)]
    );
    assert_eq!(intent.replacement().commands(), &[ap, replacement, call(1)]);
    assert_eq!(intent.retained_commands(), &[Some(0), Some(2), Some(3)]);
}

fn foreground(value: u8) -> Command {
    Command::AtColumn {
        column: 8,
        operation: TeletextColumnOperation::Foreground {
            colour: ersatzrs_domain::TeletextColourReference::new(value).unwrap(),
        },
    }
}

#[test]
fn main_existing_foreground_change_and_call_insertion_keep_scalar_family_and_column() {
    let [ap, _] = base();
    let fg1 = 8 | (1 << 11);
    let fg2 = 8 | (2 << 11);
    let fixture = Fixture::new(
        vec![ap, foreground(1), call(0)],
        &[AP, fg1, ORIGIN, pointer(5), END],
    );
    let (intent, words) = fixture
        .physical_with_scalars(&[
            AP,
            ORIGIN | (1 << 11),
            pointer(7),
            fg2,
            ORIGIN,
            pointer(7),
            END,
        ])
        .unwrap();
    assert_eq!(
        words,
        [(1, fg2)],
        "upload index3 remains original scalar index1"
    );
    assert_eq!(intent.expected().commands(), &[ap, foreground(2), call(0)]);
    assert_eq!(
        intent.replacement().commands(),
        &[ap, call(1), foreground(2), call(0)]
    );
    assert_eq!(
        intent.retained_commands(),
        &[Some(0), None, Some(1), Some(2)]
    );
    // Scalar-only remains the established fastpath, not a structural intent.
    assert!(fixture
        .physical(&[AP, fg2, ORIGIN, pointer(5), END])
        .is_err());
}

#[test]
fn main_combined_scalar_call_edits_refuse_new_styles_columns_families_and_malformed_atoms() {
    let [ap, _] = base();
    let fg1 = 8 | (1 << 11);
    let fg2 = 8 | (2 << 11);
    let fixture = Fixture::new(
        vec![ap, foreground(1), call(0)],
        &[AP, fg1, ORIGIN, pointer(5), END],
    );
    for prefix in [
        vec![AP, 9 | (2 << 11), END],
        vec![AP, 8 | (3 << 6) | (2 << 11), END],
        vec![AP, 8 | (0x20 << 11), END],
        vec![AP, fg1, fg2, END],
        vec![AP, END],
        vec![AP ^ 1, fg2, END],
        vec![AP, fg2, ORIGIN, ORIGIN, pointer(6), END],
        vec![AP, fg2, ORIGIN | (0x48 << 11), pointer(5), END],
        vec![AP, fg2, ORIGIN, pointer(4), END],
    ] {
        assert!(
            fixture.physical(&prefix).is_err(),
            "invalid scalar/call literal {prefix:x?}"
        );
    }
}

#[test]
fn main_call_parser_refuses_malformed_unknown_wrong_type_and_changed_noncalls() {
    let [ap, glyph] = base();
    let fixture = Fixture::new(vec![ap, glyph, call(0)], &[AP, B, ORIGIN, pointer(5), END]);
    for prefix in [
        vec![AP, B, ORIGIN, END],
        vec![AP, B, ORIGIN, ORIGIN, pointer(6), END],
        vec![AP, B, ORIGIN | (0x48 << 11), pointer(5), END],
        vec![AP, B, ORIGIN, pointer(4), END],
        vec![AP, B, ORIGIN, pointer(5) ^ (1 << 6), END],
        vec![AP, B ^ (1 << 6), END],
    ] {
        assert!(fixture.physical(&prefix).is_err());
    }
    let wrong_owner = TeletextPageAddress {
        page: TeletextPageNumber::new(125).unwrap(),
        ..owner()
    };
    assert!(program::decode_main(
        &[ORIGIN, pointer(5)],
        wrong_owner,
        &fixture.catalog,
        &fixture.native,
        &fixture.old,
        Location {
            page: 124,
            subcode: 32,
            resource: None
        }
    )
    .is_err());
}

#[test]
fn main_generated_and_resource_units_stay_mandatory() {
    let location = Location {
        page: 124,
        subcode: 32,
        resource: None,
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    let resource = 0x30 | (0x13 << 6);
    let old = image(&[7 << 11, ORIGIN, resource, END]);
    let protected = [
        Unit {
            first: 0,
            last: 0,
            original: None,
            call: None,
            editable: false,
            scalar: None,
        },
        Unit {
            first: 1,
            last: 2,
            original: Some(0),
            call: None,
            editable: false,
            scalar: None,
        },
        Unit {
            first: 3,
            last: 3,
            original: None,
            call: None,
            editable: false,
            scalar: None,
        },
    ];
    let same = image(&[7 << 11, ORIGIN, resource, END]);
    assert!(
        correspondence::align((&protected, &protected), (&old, &same), location, deadline).is_ok()
    );
    for changed in [
        image(&[6 << 11, ORIGIN, resource, END]),
        image(&[7 << 11, ORIGIN, resource ^ (1 << 11), END]),
    ] {
        assert!(correspondence::align(
            (&protected, &protected),
            (&old, &changed),
            location,
            deadline
        )
        .is_err());
    }
}
