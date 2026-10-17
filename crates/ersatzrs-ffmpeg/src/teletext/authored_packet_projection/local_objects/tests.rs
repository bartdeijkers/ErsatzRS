use super::*;
use crate::teletext::{
    allocate_teletext_resource_objects, compile_teletext_local_program_with_objects,
};
use ersatzrs_domain::*;

fn encoded() -> EncodedTeletextLocalObjectProgram {
    let owner = TeletextPageAddress {
        page: TeletextPageNumber::new(100).unwrap(),
        subcode: TeletextSubcode::new(0).unwrap(),
    };
    let target = TeletextObjectIdentity::local(owner, TeletextObjectId::new(65535));
    let budget = TeletextObjectCatalogBudget {
        max_objects: 1,
        max_commands: 1,
    };
    let catalog = TeletextEnhancementObjectCatalog::new(
        vec![TeletextEnhancementObjectDefinition {
            identity: target,
            object_type: TeletextObjectType::Passive,
            program: TeletextEnhancementProgram::new(vec![]).unwrap(),
        }],
        budget,
    )
    .unwrap();
    let allocations = allocate_teletext_resource_objects(&catalog, &[], 1).unwrap();
    let main = TeletextEnhancementProgram::new(vec![TeletextEnhancementCommand::InvokeObject {
        target,
        object_type: TeletextObjectType::Passive,
        offset: None,
    }])
    .unwrap();
    compile_teletext_local_program_with_objects(
        owner,
        &main,
        &catalog,
        &allocations,
        TeletextResolvedObjectPageLinks {
            public: None,
            global: None,
        },
        budget,
    )
    .unwrap()
}

#[test]
fn generated_local_wrapper_projects_literal_invocation_marker_and_terminators() {
    let program = encoded();
    let records = project_encoded_teletext_local_objects(&program, 1).unwrap();
    let TeletextEncodedDisplayRecord::Triplets {
        row,
        designation,
        words,
    } = records[0]
    else {
        panic!("expected X26")
    };
    assert_eq!((row, designation), (26, 0));
    assert_eq!(words[0], 0x28 | (19 << 6) | (2 << 11));
    assert_eq!(words[1], 63 | (31 << 6) | (6 << 11));
    assert_eq!(words[2], 0x28 | (23 << 6) | (2 << 11));
    assert_eq!(words[3..], [0x2fff; 10]);
    assert_eq!(program.definition_positions()[0].absolute_index(), 2);
}

#[test]
fn local_projection_rejects_correctable_damage_wrong_designations_and_bounds() {
    let program = encoded();
    for byte in [0, 1, 2, 3] {
        let mut bodies = program.packet_bodies().to_vec();
        bodies[0][byte] ^= 1;
        let error = project_bodies(&bodies, 1).unwrap_err();
        assert_eq!(error.failure, Failure::Protection);
        assert_eq!((error.unit, error.row), (Some(0), Some(26)));
    }
    let mut bodies = program.packet_bodies().to_vec();
    bodies[0][0] = crate::teletext::packets::hamming_8_4(1);
    assert_eq!(
        project_bodies(&bodies, 1).unwrap_err().failure,
        Failure::UnsupportedPacket
    );
    for (bodies, maximum) in [(program.packet_bodies(), 0), (&[][..], 1)] {
        assert_eq!(
            project_bodies(bodies, maximum).unwrap_err().failure,
            Failure::Budget
        );
    }
    assert_eq!(
        project_bodies(&[program.packet_bodies()[0]; 17], 17)
            .unwrap_err()
            .failure,
        Failure::Budget
    );
}
