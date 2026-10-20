//! Definition growth is charged as objects inside one retained bank record.
use super::*;
use ersatzrs_app::teletext_design_documents::{
    TeletextSavedResourceTransmission, TeletextSavedResourceUpdate,
};
use ersatzrs_domain::{
    TeletextEnhancementObjectDefinition, TeletextEnhancementProgram, TeletextObjectId,
    TeletextObjectIdentity, TeletextObjectType, TeletextRawDataPageNumber,
    TeletextResourceIdentity, TeletextResourceKind, TeletextResourceSubpageIndex,
    TeletextResourceUpdateCounter,
};

fn bank(budget: TeletextServiceBudget) -> TeletextSavedObjectResourceAddition {
    let resource = TeletextResourceIdentity::new(
        TeletextResourceKind::Pop,
        TeletextRawDataPageNumber::new(2, 0xe0).unwrap(),
        TeletextResourceSubpageIndex::new(0).unwrap(),
    )
    .unwrap();
    TeletextSavedObjectResourceAddition::new(
        0,
        TeletextObjectResourceAddition::new(resource, vec![], budget).unwrap(),
        TeletextSavedResourceTransmission {
            resource,
            counter: TeletextResourceUpdateCounter::new(7).unwrap(),
            update: TeletextSavedResourceUpdate::Unchanged,
            last_subpage: false,
        },
    )
    .unwrap()
}
fn definition(
    bank: &TeletextSavedObjectResourceAddition,
    id: u16,
    budget: TeletextServiceBudget,
) -> TeletextResourceObjectDefinitionAddition {
    TeletextResourceObjectDefinitionAddition::new(
        TeletextEnhancementObjectDefinition {
            identity: TeletextObjectIdentity::public(
                bank.addition().resource(),
                TeletextObjectId::new(id),
            )
            .unwrap(),
            object_type: TeletextObjectType::Active,
            program: TeletextEnhancementProgram::new(vec![]).unwrap(),
        },
        budget,
    )
    .unwrap()
}
#[test]
fn retained_bank_growth_charges_two_definitions_as_one_record_and_refuses_combined_limits() {
    let budget = TeletextServiceBudget {
        max_records: 1,
        max_objects: 2,
        max_structure_items: 2,
        max_base_content_bytes: 1024,
        max_commands: 16,
        max_collected_commands: 16,
        max_collected_drcs_references: 16,
    };
    let original = bank(budget);
    let incoming = TeletextEditorialChanges {
        resource_additions: vec![
            definition(&original, 32, budget),
            definition(&original, 34, budget),
        ],
        ..Default::default()
    };
    let mut cumulative = TeletextEditorialChanges {
        object_resource_additions: vec![original.clone()],
        object_subpage_announcements: Vec::new(),
        object_page_selections: Vec::new(),
        main_resource_invocations: Vec::new(),
        ..Default::default()
    };
    let deadline = Instant::now() + std::time::Duration::from_secs(5);
    let mut narrow = budget;
    narrow.max_objects = 1;
    assert!(matches!(
        append(&mut cumulative, &incoming, narrow, deadline),
        Err(NativeTeletextEditorialImportError::Budget)
    ));
    assert_eq!(cumulative.object_resource_additions, vec![original.clone()]);
    assert!(cumulative.resource_additions.is_empty());
    narrow = budget;
    narrow.max_structure_items = 1;
    assert!(matches!(
        append(&mut cumulative, &incoming, narrow, deadline),
        Err(NativeTeletextEditorialImportError::Budget)
    ));
    assert_eq!(cumulative.object_resource_additions, vec![original.clone()]);
    append(&mut cumulative, &incoming, budget, deadline).unwrap();
    assert!(cumulative.resource_additions.is_empty());
    assert_eq!(cumulative.object_resource_additions.len(), 1);
    let saved = &cumulative.object_resource_additions[0];
    assert_eq!(saved.document_index(), original.document_index());
    assert_eq!(saved.transmission(), original.transmission());
    assert_eq!(
        saved.addition().definitions(),
        incoming
            .resource_additions
            .iter()
            .map(|addition| addition.definition().clone())
            .collect::<Vec<_>>()
    );
}
