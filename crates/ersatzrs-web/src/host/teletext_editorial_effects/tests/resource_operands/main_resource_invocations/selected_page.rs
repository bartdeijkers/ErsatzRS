//! Selected raw page and direct Main calls share one complete effects boundary.
use super::*;
fn fixture(
    global: bool,
) -> (
    EncodedTeletextAuthoredService,
    TeletextResourceIdentity,
    TeletextObjectIdentity,
) {
    let root = baseline(global);
    let resource = TeletextResourceIdentity::new(
        resource(global).kind(),
        TeletextRawDataPageNumber::new(2, 0xa4).unwrap(),
        TeletextResourceSubpageIndex::new(3).unwrap(),
    )
    .unwrap();
    let target = if global {
        TeletextObjectIdentity::global(resource, TeletextObjectId::new(10))
    } else {
        TeletextObjectIdentity::public(resource, TeletextObjectId::new(10))
    }
    .unwrap();
    let addition = TeletextObjectResourceAddition::new(
        resource,
        vec![TeletextEnhancementObjectDefinition {
            identity: target,
            object_type: TeletextObjectType::Passive,
            program: TeletextEnhancementProgram::new(vec![at(0, glyph(b'Y'))]).unwrap(),
        }],
        policy().validation.service,
    )
    .unwrap();
    let before = object_page_selections::encode(
        root.declared()
            .clone()
            .with_object_resource_additions(
                &[addition],
                policy().validation.service,
                policy().validation.declarations,
            )
            .unwrap(),
    );
    (before, resource, target)
}

#[test]
fn selected_raw_page_and_direct_main_consumers_are_checked_as_one_complete_effect() {
    use ersatzrs_app::teletext_design_documents::TeletextSavedObjectPageSelectionChange;
    for global in [false, true] {
        let (before, resource, target) = fixture(global);
        let role = if global {
            TeletextX27LinkRole::Gpop
        } else {
            TeletextX27LinkRole::Pop
        };
        let selections = [123, 124].map(|page| {
            let owner = address(page);
            TeletextSavedObjectPageSelectionChange::new(
                0,
                TeletextObjectPageSelectionChange::new(
                    owner,
                    role,
                    before.declared().semantic().page_enhancements()[&owner].compositional_links,
                    *before
                        .declared()
                        .resolutions()
                        .iter()
                        .find(|value| value.display() == owner)
                        .unwrap()
                        .role(role),
                    TeletextX27LinkDeclaration::new(
                        TeletextX27LinkTarget::Page {
                            page: resource.page(),
                        },
                        TeletextX27LinkValidity::Level25And35,
                        TeletextSubpageRequirementMask::new(1 << 3),
                    )
                    .unwrap(),
                )
                .unwrap(),
            )
            .unwrap()
        });
        let calls = [123, 124].map(|page| {
            TeletextMainResourceInvocationOverlay::new(
                address(page),
                1,
                call(global, 65520),
                TeletextEnhancementCommand::InvokeObject {
                    target,
                    object_type: TeletextObjectType::Passive,
                    offset: None,
                },
            )
            .unwrap()
        });
        let after = object_page_selections::encode(
            before
                .declared()
                .clone()
                .with_object_page_selections_and_consumers(
                    &selections
                        .iter()
                        .map(TeletextSavedObjectPageSelectionChange::change)
                        .collect::<Vec<_>>(),
                    TeletextObjectPageSelectionConsumers {
                        main: &calls,
                        ..Default::default()
                    },
                    policy().validation.service,
                    policy().validation.declarations,
                )
                .unwrap(),
        );
        let complete = TeletextEditorialEdits {
            object_page_selections: &selections,
            ..changed(&calls)
        };
        qualifies(&before, &after, complete, complete, &permissions(true)).unwrap();
        assert!(qualifies(
            &before,
            &after,
            changed(&calls),
            complete,
            &permissions(true)
        )
        .is_err());
        assert!(qualifies(
            &before,
            &after,
            TeletextEditorialEdits {
                object_page_selections: &selections,
                ..edits(&[])
            },
            complete,
            &permissions(true)
        )
        .is_err());
        qualifies(&after, &after, edits(&[]), complete, &permissions(true)).unwrap();
    }
}
