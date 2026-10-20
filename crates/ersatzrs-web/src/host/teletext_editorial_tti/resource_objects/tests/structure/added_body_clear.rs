//! Clearing a saved Add body cannot become authority over an original anchor.
use super::*;
use crate::host::teletext_editorial_tti::resource_objects::project::{native_body, Body};
use crate::host::teletext_editorial_tti::resource_objects::PositionScope;

fn addition(
    service: &EncodedTeletextAuthoredService,
    identity: TeletextObjectIdentity,
) -> TeletextResourceObjectDefinitionAddition {
    TeletextResourceObjectDefinitionAddition::new(
        service
            .declared()
            .semantic()
            .objects()
            .definition(identity)
            .unwrap()
            .clone(),
        bounds().2,
    )
    .unwrap()
}

#[test]
fn added_body_clear_requires_exact_saved_identity_root_absence_and_current_native_body() {
    for global in [false, true] {
        for kind in [
            TeletextObjectType::Passive,
            TeletextObjectType::Active,
            TeletextObjectType::Adaptive,
        ] {
            let current = compile(global, 2, b'A', kind);
            let root = compile(!global, 2, b'A', kind);
            let project = project(&current, global);
            let mapped = mapping(&project.assets[1], &project, &current).unwrap();
            let additions = [addition(&current, mapped.identity)];
            let scope = PositionScope {
                additions: &additions,
                programs: &[],
                root: Some(&root),
            };
            let body = Body {
                before: native_body(&mapped, bounds(), deadline()).unwrap(),
                after: vec![],
                spans: vec![],
                context_calls: None,
            };
            let cleared = scope
                .restore(&mapped, &body, bounds(), deadline())
                .unwrap()
                .unwrap();
            assert_eq!(cleared.identity(), mapped.identity);
            assert_eq!(cleared.expected(), &mapped.definition.program);
            assert!(cleared.replacement().commands().is_empty());
            assert!(cleared.retained_commands().is_empty());
            assert!(cleared.is_position_restoration());
            assert!(!cleared.is_original_position_intent());
            assert!(
                authorize_programs(&project, &current, &[], &[cleared], bounds(), deadline())
                    .is_err()
            );
            assert!(PositionScope {
                root: Some(&current),
                ..scope
            }
            .restore(&mapped, &body, bounds(), deadline())
            .is_err());
            assert!(PositionScope {
                root: None,
                ..scope
            }
            .restore(&mapped, &body, bounds(), deadline())
            .is_err());
            assert!(PositionScope {
                additions: &[],
                ..scope
            }
            .restore(&mapped, &body, bounds(), deadline())
            .unwrap()
            .is_none());
            let duplicate = [additions[0].clone(), additions[0].clone()];
            assert!(PositionScope {
                additions: &duplicate,
                ..scope
            }
            .restore(&mapped, &body, bounds(), deadline())
            .is_err());
            let different = compile(global, 2, b'B', kind);
            let changed = [addition(&different, mapped.identity)];
            assert!(PositionScope {
                additions: &changed,
                ..scope
            }
            .restore(&mapped, &body, bounds(), deadline())
            .is_err());
            let mut forged = Body {
                before: body.before.clone(),
                after: vec![],
                spans: vec![],
                context_calls: None,
            };
            forged.before[0] ^= 1;
            assert!(scope
                .restore(&mapped, &forged, bounds(), deadline())
                .is_err());
            assert!(scope
                .restore(&mapped, &body, bounds(), Instant::now())
                .is_err());
        }
    }
}
