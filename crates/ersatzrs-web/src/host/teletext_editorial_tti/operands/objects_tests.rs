//! Original source and marker authentication without constructing host authority.
use super::{authored_operation, retained_word, Location, PacketWords};
use ersatzrs_domain::{
    TeletextColumnOperation, TeletextEnhancementCommand, TeletextEnhancementObjectCatalog,
    TeletextEnhancementObjectDefinition, TeletextEnhancementProgram, TeletextEnhancementSource,
    TeletextGlyphCode, TeletextObjectCatalogBudget, TeletextObjectId, TeletextObjectIdentity,
    TeletextObjectType, TeletextPageAddress, TeletextPageNumber, TeletextRawDataPageNumber,
    TeletextResolvedObjectPageLinks, TeletextResourceIdentity, TeletextResourceKind,
    TeletextResourceSubpageIndex, TeletextSubcode,
};
use ersatzrs_ffmpeg::teletext::{
    allocate_teletext_resource_objects, compile_teletext_local_program_with_objects,
};
use std::time::{Duration, Instant};

fn owner(subcode: u16) -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(124).unwrap(),
        subcode: TeletextSubcode::new(subcode).unwrap(),
    }
}
fn identity() -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(owner(32), TeletextObjectId::new(65520))
}
fn source() -> TeletextEnhancementSource {
    TeletextEnhancementSource {
        object: Some(identity()),
        command: 0,
    }
}
fn location() -> Location {
    Location {
        page: 124,
        subcode: 32,
        resource: None,
    }
}
fn budget() -> TeletextObjectCatalogBudget {
    TeletextObjectCatalogBudget {
        max_objects: 1,
        max_commands: 1,
    }
}
fn catalog(operation: TeletextColumnOperation) -> TeletextEnhancementObjectCatalog {
    TeletextEnhancementObjectCatalog::new(
        vec![TeletextEnhancementObjectDefinition {
            identity: identity(),
            object_type: TeletextObjectType::Passive,
            program: TeletextEnhancementProgram::new(vec![TeletextEnhancementCommand::AtColumn {
                column: 0,
                operation,
            }])
            .unwrap(),
        }],
        budget(),
    )
    .unwrap()
}
fn g0() -> TeletextColumnOperation {
    TeletextColumnOperation::G0 {
        code: TeletextGlyphCode::new(b'A').unwrap(),
    }
}

#[test]
fn original_local_catalogue_membership_and_exact_source_are_required() {
    let catalog = catalog(g0());
    let (_, column, operation) =
        authored_operation(&catalog, owner(32), source(), location()).unwrap();
    assert_eq!((column, operation), (0, g0()));
    let global = TeletextObjectIdentity::global(
        TeletextResourceIdentity::new(
            TeletextResourceKind::Gpop,
            TeletextRawDataPageNumber::new(1, 0xa0).unwrap(),
            TeletextResourceSubpageIndex::new(0).unwrap(),
        )
        .unwrap(),
        TeletextObjectId::new(65520),
    )
    .unwrap();
    for bad in [
        TeletextEnhancementSource {
            object: None,
            command: 0,
        },
        TeletextEnhancementSource {
            object: Some(global),
            command: 0,
        },
        TeletextEnhancementSource {
            object: Some(TeletextObjectIdentity::local(
                owner(32),
                TeletextObjectId::new(65521),
            )),
            command: 0,
        },
        TeletextEnhancementSource {
            command: 1,
            ..source()
        },
    ] {
        assert!(authored_operation(&catalog, owner(32), bad, location()).is_err());
    }
    assert!(authored_operation(&catalog, owner(33), source(), location()).is_err());
}

#[test]
fn original_colour_and_attribute_commands_keep_exact_original_local_source() {
    for operation in [
        TeletextColumnOperation::Foreground {
            colour: ersatzrs_domain::TeletextColourReference::new(18).unwrap(),
        },
        TeletextColumnOperation::Background {
            colour: ersatzrs_domain::TeletextColourReference::new(0).unwrap(),
        },
        TeletextColumnOperation::Attributes {
            double_height: true,
            double_width: true,
            boxed: false,
            conceal: false,
            invert: false,
            underline_or_separated_mosaics: false,
        },
    ] {
        let catalog = catalog(operation);
        let (_, column, retained) =
            authored_operation(&catalog, owner(32), source(), location()).unwrap();
        assert_eq!((column, retained), (0, operation));
        assert!(authored_operation(&catalog, owner(33), source(), location()).is_err());
    }
}

#[test]
fn unsupported_original_object_family_cannot_gain_operand_edit_authority() {
    let catalog = catalog(TeletextColumnOperation::G1Alpha {
        code: ersatzrs_domain::TeletextG1AlphaCode::new(b'A').unwrap(),
    });
    assert!(authored_operation(&catalog, owner(32), source(), location()).is_err());
}

#[test]
fn final_definition_marker_type_presence_and_word_span_are_authenticated() {
    let catalog = catalog(g0());
    let main = TeletextEnhancementProgram::new(vec![]).unwrap();
    let allocations = allocate_teletext_resource_objects(&catalog, &[], 1).unwrap();
    let encoded = compile_teletext_local_program_with_objects(
        owner(32),
        &main,
        &catalog,
        &allocations,
        TeletextResolvedObjectPageLinks {
            public: None,
            global: None,
        },
        budget(),
    )
    .unwrap();
    // Empty main termination, local Passive marker number1, then G0 A at column0.
    assert_eq!(encoded.definition_positions()[0].absolute_index(), 1);
    let mut words: PacketWords = [None; 16];
    let mut first = [0; 13];
    first[1] = 0x0de8;
    first[2] = 0x20a40;
    words[0] = Some(first);
    let definition = catalog.definition(identity()).unwrap();
    let deadline = Instant::now().checked_add(Duration::from_secs(30)).unwrap();
    assert_eq!(
        retained_word(&encoded, definition, source(), &words, location(), deadline).unwrap(),
        2
    );
    let mut wrong_type = definition.clone();
    wrong_type.object_type = TeletextObjectType::Active;
    assert!(retained_word(
        &encoded,
        &wrong_type,
        source(),
        &words,
        location(),
        deadline
    )
    .is_err());
    words[0].as_mut().unwrap()[1] ^= 1 << 6;
    assert!(retained_word(&encoded, definition, source(), &words, location(), deadline).is_err());
    words[0] = None;
    assert!(retained_word(&encoded, definition, source(), &words, location(), deadline).is_err());
}
