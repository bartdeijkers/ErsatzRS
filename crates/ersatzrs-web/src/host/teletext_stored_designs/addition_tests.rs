//! New catalogue intent cannot borrow an older resource-replay authorization.
use super::*;
use ersatzrs_app::teletext_design_documents::TeletextSavedEditorialProvenance;
use ersatzrs_domain::{
    TeletextEnhancementObjectDefinition, TeletextEnhancementProgram, TeletextObjectId,
    TeletextObjectIdentity, TeletextObjectType, TeletextRawDataPageNumber,
    TeletextResourceIdentity, TeletextResourceKind, TeletextResourceObjectDefinitionAddition,
    TeletextResourceSubpageIndex,
};

#[test]
fn unqualified_definition_additions_cannot_use_existing_resource_replay_authority() {
    let budget = super::super::teletext_runtime_policy::parse_native_teletext_runtime_policy(
        include_str!("../../../tests/fixtures/teletext-output-policy-v1.json"),
    )
    .unwrap()
    .design
    .validation
    .service;
    let resource = TeletextResourceIdentity::new(
        TeletextResourceKind::Pop,
        TeletextRawDataPageNumber::new(2, 0xc0).unwrap(),
        TeletextResourceSubpageIndex::new(0).unwrap(),
    )
    .unwrap();
    let addition = TeletextResourceObjectDefinitionAddition::new(
        TeletextEnhancementObjectDefinition {
            identity: TeletextObjectIdentity::public(resource, TeletextObjectId::new(25)).unwrap(),
            object_type: TeletextObjectType::Passive,
            program: TeletextEnhancementProgram::new(Vec::new()).unwrap(),
        },
        budget,
    )
    .unwrap();
    let overlay = TeletextSavedEditorialOverlay::new_with_resource_object_definition_additions(
        TeletextSavedEditorialProvenance {
            export_receipt_id: 1,
            export_receipt_digest: "a".repeat(64),
            original_design_id: 1,
            original_revision: 1,
            original_digest: "b".repeat(64),
        },
        vec![addition],
    )
    .unwrap();
    assert_eq!(overlay.schema_version(), 18);
    assert!(overlay.resource_object_operands().is_empty());
    assert!(overlay.resource_object_program_overlays().is_empty());
    for has_physical_rows in [false, true] {
        assert!(matches!(
            check_authenticated_editorial_resource_replay(
                Some(&overlay),
                has_physical_rows,
                &[],
                &[],
            ),
            Err(NativeTeletextDesignError::EditorialValidationUnavailable)
        ));
    }
    check_authenticated_editorial_resource_replay(None, false, &[], &[]).unwrap();
}
