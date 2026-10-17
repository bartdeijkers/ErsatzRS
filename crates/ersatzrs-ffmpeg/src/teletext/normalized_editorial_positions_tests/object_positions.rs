//! Forward relative positions in independently reset object definitions.

use super::*;
use crate::teletext::authored_service_tests::{declared, transmissions};

fn fixture(
    object_type: TeletextObjectType,
    scope: TeletextObjectScope,
    explicit: bool,
) -> (TeletextDeclaredService, TeletextObjectIdentity) {
    let mut draft = input();
    let owner = draft.base.pages[0].address;
    let target = match scope {
        TeletextObjectScope::Public => TeletextObjectIdentity::public(
            draft.objects[0].identity.resource().unwrap(),
            TeletextObjectId::new(65535),
        )
        .unwrap(),
        TeletextObjectScope::Global => {
            let resource = TeletextResourceIdentity::new(
                TeletextResourceKind::Gpop,
                TeletextRawDataPageNumber::new(2, 0xb0).unwrap(),
                TeletextResourceSubpageIndex::new(0).unwrap(),
            )
            .unwrap();
            draft.pages[0].resources.objects.global = Some(resource.page());
            let link = |page: Option<TeletextRawDataPageNumber>| {
                TeletextX27LinkDeclaration::new(
                    page.map_or(
                        TeletextX27LinkTarget::Unspecified {
                            magazine: TeletextX27Magazine::new(2).unwrap(),
                        },
                        |page| TeletextX27LinkTarget::Page { page },
                    ),
                    TeletextX27LinkValidity::Level25Only,
                    TeletextSubpageRequirementMask::new(if page.is_some() { 1 } else { 0xffff }),
                )
                .unwrap()
            };
            draft.pages[0].compositional_links = Some(
                TeletextX27Level25Links::new(
                    link(Some(resource.page())),
                    link(draft.pages[0].resources.objects.public),
                    link(None),
                    link(None),
                )
                .unwrap(),
            );
            TeletextObjectIdentity::global(resource, TeletextObjectId::new(65535)).unwrap()
        }
        TeletextObjectScope::Local => {
            TeletextObjectIdentity::local(owner, TeletextObjectId::new(65535))
        }
    };
    draft.pages[0].local_program = Some(
        TeletextEnhancementProgram::new(vec![
            position(1, 8),
            TeletextEnhancementCommand::InvokeObject {
                target,
                object_type,
                offset: None,
            },
        ])
        .unwrap(),
    );
    let mut commands = Vec::new();
    if explicit {
        commands.push(position(0, 8));
    }
    commands.extend([foreground(8, 2), glyph(8)]);
    draft.objects.push(TeletextEnhancementObjectDefinition {
        identity: target,
        object_type,
        program: TeletextEnhancementProgram::new(commands).unwrap(),
    });
    (declared(draft), target)
}

#[test]
fn forward_relative_object_positions_pass_the_authored_factory() {
    for object_type in [
        TeletextObjectType::Active,
        TeletextObjectType::Adaptive,
        TeletextObjectType::Passive,
    ] {
        for scope in [
            TeletextObjectScope::Local,
            TeletextObjectScope::Public,
            TeletextObjectScope::Global,
        ] {
            let (original, target) = fixture(object_type, scope, true);
            let saved = original.clone();
            let actual =
                encode_teletext_authored_service(original, &transmissions(&saved), &limits())
                    .unwrap_or_else(|error| panic!("{object_type:?}/scope={scope:?}: {error:?}"));
            assert_eq!(actual.declared(), &saved);
            let (implicit, _) = fixture(object_type, scope, false);
            let expected = encode_teletext_authored_service(
                implicit.clone(),
                &transmissions(&implicit),
                &limits(),
            )
            .unwrap();
            assert_eq!(actual.pages().len(), expected.pages().len());
            assert_eq!(actual.inventory(), expected.inventory());
            for (a, b) in actual.pages().iter().zip(expected.pages()) {
                assert_eq!(a.data_units(), b.data_units());
            }
            let EncodedTeletextAuthoredPage::CompiledDisplay(display) = &actual.pages()[0] else {
                panic!("object position must retain compiled-display ownership")
            };
            for trace in [
                display.normalized().original().spatial_trace(),
                display.normalized().physical().replay().spatial_trace(),
            ] {
                let invocation = trace
                    .invocations()
                    .iter()
                    .find(|invocation| invocation.target() == target)
                    .unwrap();
                assert_eq!(
                    (
                        invocation.caller_position().row(),
                        invocation.caller_position().column()
                    ),
                    (1, 8)
                );
                assert_eq!(
                    (invocation.origin().row(), invocation.origin().column()),
                    (1, 8)
                );
                assert_eq!(invocation.offset(), None);
            }
            for geometry in [
                display.normalized().original().display_geometry(),
                display.normalized().physical().replay().display_geometry(),
            ] {
                let region = geometry
                    .object_regions()
                    .iter()
                    .find(|region| region.instance().target() == target)
                    .unwrap();
                assert_eq!(
                    region.interval(),
                    TeletextObjectAddressInterval::Known {
                        start: (1, 8),
                        finish: (1, 16),
                    }
                );
            }
        }
    }
}
