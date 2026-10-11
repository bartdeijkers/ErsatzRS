//! Empty resource owners participate in semantic inventory without fake objects.
use super::*;

#[test]
fn explicit_empty_selected_pop_is_valid_without_a_phantom_definition() {
    let owner = address(200, 0x1234);
    let mut input = draft(&[owner]);
    input.pages[0].resources.objects.public = Some(raw(0xa0));
    let empty = resource(TeletextResourceKind::Pop, 0xa0, 0);
    assert!(
        matches!(TeletextSemanticService::new(input.clone(), budget()),
        Err(TeletextServiceError::ObjectLink { display, .. }) if display == owner)
    );
    let service =
        TeletextSemanticService::new_with_object_companions(input, vec![empty], budget()).unwrap();
    assert_eq!(service.object_companions(), [empty]);
    assert!(service.objects().definitions().is_empty());
    let (source, program) = service.effective_program(owner).unwrap();
    assert_eq!(source, TeletextEffectiveProgramSource::BaseOnly);
    assert!(program.commands().is_empty());
}

#[test]
fn empty_subpage_closes_actual_x27_mask_without_invented_object() {
    let owner = address(200, 0);
    let mut input = draft(&[owner]);
    input.pages[0].resources.objects.public = Some(raw(0xa0));
    let absent = TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Unspecified {
            magazine: TeletextX27Magazine::new(8).unwrap(),
        },
        TeletextX27LinkValidity::Level25Only,
        TeletextSubpageRequirementMask::new(0xffff),
    )
    .unwrap();
    let mut links = [absent; 4];
    links[TeletextX27LinkRole::Pop.wire_code() as usize] = TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Page { page: raw(0xa0) },
        TeletextX27LinkValidity::Level25Only,
        TeletextSubpageRequirementMask::new(3),
    )
    .unwrap();
    input.pages[0].compositional_links =
        Some(TeletextX27Level25Links::new(links[0], links[1], links[2], links[3]).unwrap());
    input.objects.push(definition(
        public(0xa0, 1, 65535),
        TeletextObjectType::Passive,
        vec![],
    ));
    let empty = resource(TeletextResourceKind::Pop, 0xa0, 0);
    let service =
        TeletextSemanticService::new_with_object_companions(input, vec![empty], budget()).unwrap();
    assert_eq!(service.objects().definitions().len(), 1);
    assert_eq!(service.object_companions(), [empty]);
}

#[test]
fn empty_companions_reject_repeated_and_populated_owners() {
    let owner = address(200, 0);
    let empty = resource(TeletextResourceKind::Pop, 0xa0, 0);
    assert_eq!(
        TeletextSemanticService::new_with_object_companions(
            draft(&[owner]),
            vec![empty, empty],
            budget()
        )
        .unwrap_err(),
        TeletextServiceError::ResourceCollision {
            page: raw(0xa0),
            subpage: empty.logical_subpage()
        }
    );
    let mut input = draft(&[owner]);
    input.objects.push(definition(
        public(0xa0, 0, 65535),
        TeletextObjectType::Passive,
        vec![],
    ));
    assert_eq!(
        TeletextSemanticService::new_with_object_companions(input, vec![empty], budget())
            .unwrap_err(),
        TeletextServiceError::ResourceCollision {
            page: raw(0xa0),
            subpage: empty.logical_subpage()
        }
    );
}

#[test]
fn wrong_purpose_is_located_after_raw_budget_fences() {
    let owner = address(200, 0);
    let invalid = resource(TeletextResourceKind::Drcs, 0xa0, 0);
    let input = draft(&[owner]);
    let mut limits = budget();
    limits.max_records = 1;
    assert_eq!(
        TeletextSemanticService::new_with_object_companions(input.clone(), vec![invalid], limits)
            .unwrap_err(),
        TeletextServiceError::Budget {
            dimension: "record",
            maximum: 1
        }
    );
    assert_eq!(
        TeletextSemanticService::new_with_object_companions(input, vec![invalid], budget())
            .unwrap_err(),
        TeletextServiceError::ObjectCompanionKind { identity: invalid }
    );
}

#[test]
fn exact_companion_record_limit_and_structure_work_are_charged() {
    let input = draft(&[address(200, 0)]);
    let empty = resource(TeletextResourceKind::Pop, 0xa0, 0);
    let mut limits = budget();
    limits.max_records = 2;
    assert!(TeletextSemanticService::new_with_object_companions(
        input.clone(),
        vec![empty],
        limits
    )
    .is_ok());
    limits.max_structure_items = 2; // Base owner + page metadata fit; companion does not.
    assert_eq!(
        TeletextSemanticService::new_with_object_companions(input, vec![empty], limits)
            .unwrap_err(),
        TeletextServiceError::Budget {
            dimension: "structure item",
            maximum: 2
        }
    );
}

#[test]
fn separate_s1_companions_preserve_purpose_and_cannot_satisfy_a_missing_default() {
    let owner = address(200, 0);
    let public_empty = resource(TeletextResourceKind::Pop, 0xa0, 0);
    let global_empty = resource(TeletextResourceKind::Gpop, 0xa0, 1);
    let service = TeletextSemanticService::new_with_object_companions(
        draft(&[owner]),
        vec![global_empty, public_empty],
        budget(),
    )
    .unwrap();
    assert_eq!(service.object_companions(), [global_empty, public_empty]);
    let mut input = draft(&[owner]);
    input.pages[0].resources.defaults[2] = Some(selection(
        public(0xa0, 0, 1234),
        TeletextObjectType::Passive,
    ));
    assert!(TeletextSemanticService::new_with_object_companions(
        input,
        vec![public_empty],
        budget()
    )
    .is_err());
}
