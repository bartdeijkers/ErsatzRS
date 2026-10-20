//! Exact Main target changes retire callee authority, including equal-pixel calls.
use super::*;
#[path = "main_resource_invocations/selected_page.rs"]
mod selected_page;
use ersatzrs_domain::{
    TeletextMainResourceInvocationOverlay, TeletextObjectPageSelectionConsumers,
};

fn call(global: bool, id: u16) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::InvokeObject {
        target: target(global, id),
        object_type: TeletextObjectType::Passive,
        offset: None,
    }
}
fn baseline(global: bool) -> EncodedTeletextAuthoredService {
    let source = compile_with_program(
        Fixture {
            global,
            code: b'A',
            other_code: b'A',
            ..Fixture::default()
        },
        Some(TeletextEnhancementProgram::new(vec![at(0, glyph(b'A'))]).unwrap()),
        true,
    );
    let semantic = source.declared().semantic();
    let mut definitions = semantic.objects().definitions().to_vec();
    definitions.extend([123, 124].map(|page| TeletextEnhancementObjectDefinition {
        identity: TeletextObjectIdentity::local(address(page), TeletextObjectId::new(1)),
        object_type: TeletextObjectType::Passive,
        program: TeletextEnhancementProgram::new(vec![]).unwrap(),
    }));
    let semantic = TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base: semantic.base().clone(),
            pages: semantic.page_enhancements().values().cloned().collect(),
            magazines: vec![],
            objects: definitions,
            drcs: vec![],
        },
        policy().validation.service,
    )
    .unwrap();
    object_page_selections::encode(
        TeletextDeclaredService::new(
            semantic,
            vec![],
            source.declared().selections().to_vec(),
            vec![],
            policy().validation.declarations,
        )
        .unwrap(),
    )
}
fn changes(
    global: bool,
    ordinal: usize,
    reverse: bool,
) -> Vec<TeletextMainResourceInvocationOverlay> {
    let (old, new) = if reverse {
        (65521, 65520)
    } else {
        (65520, 65521)
    };
    [123, 124]
        .map(|page| {
            TeletextMainResourceInvocationOverlay::new(
                address(page),
                ordinal,
                call(global, old),
                call(global, new),
            )
            .unwrap()
        })
        .into()
}
fn apply(
    before: &EncodedTeletextAuthoredService,
    calls: &[TeletextMainResourceInvocationOverlay],
) -> EncodedTeletextAuthoredService {
    object_page_selections::encode(
        before
            .declared()
            .clone()
            .with_object_page_selections_and_consumers(
                &[],
                TeletextObjectPageSelectionConsumers {
                    main: calls,
                    ..Default::default()
                },
                policy().validation.service,
                policy().validation.declarations,
            )
            .unwrap(),
    )
}
fn changed(calls: &[TeletextMainResourceInvocationOverlay]) -> TeletextEditorialEdits<'_> {
    TeletextEditorialEdits {
        main_resource_invocations: calls,
        ..edits(&[])
    }
}
fn qualifies(
    before: &EncodedTeletextAuthoredService,
    after: &EncodedTeletextAuthoredService,
    incoming: TeletextEditorialEdits<'_>,
    cumulative: TeletextEditorialEdits<'_>,
    project: &TeletextExportProject,
) -> Result<()> {
    validate_teletext_editorial_transition_with_operands(
        before,
        after,
        project,
        incoming,
        cumulative,
        &policy().encoding,
        deadline(),
    )
}
fn assert_equal_pixels(old: &TeletextMainComposition, new: &TeletextMainComposition) {
    let raster = |composition| {
        render_teletext_composition(
            composition,
            TeletextColourPixelOptions {
                reveal_concealed: false,
                flash_phase: TeletextFlashPhase::Original,
            },
            TeletextRasterBudget {
                max_pixels: 168_000,
                max_diagnostics: 1400,
            },
        )
        .unwrap()
    };
    assert_eq!(raster(old), raster(new));
}
#[test]
fn main_resource_retarget_requires_exact_intent_and_never_reuses_equal_pixel_callee_authority() {
    for global in [false, true] {
        let before = baseline(global);
        let calls = changes(global, 1, false);
        let after = apply(&before, &calls);
        let incoming = changed(&calls);
        qualifies(&before, &after, incoming, incoming, &permissions(true)).unwrap();
        for invalid in [
            edits(&[]),
            changed(&calls[..1]),
            changed(&[calls[0], calls[0]]),
        ] {
            assert!(qualifies(&before, &after, invalid, incoming, &permissions(true)).is_err());
        }
        let old = display(&before.pages()[0]).unwrap().1;
        let new = display(&after.pages()[0]).unwrap().1;
        assert_equal_pixels(old, new);
        let lineage = super::super::super::lineage::ProtectedLineage::new(
            old,
            new,
            incoming,
            &policy().encoding,
            deadline(),
        )
        .unwrap();
        for invocation in new.spatial_trace().invocations() {
            assert!(matches!(
                lineage.source_at_invocation(invocation.source(), Some(invocation.id())),
                super::super::super::lineage::CanonicalSource::Inserted(_)
            ));
        }
        for project in [permissions(false), {
            let mut value = permissions(true);
            value.producers[0].authoring.as_mut().unwrap().pages[1].regions[0].kind =
                TeletextAuthoringRegionKindWire::Generated;
            value
        }] {
            assert_eq!(
                qualifies(&before, &after, incoming, incoming, &project)
                    .unwrap_err()
                    .reason,
                TeletextEditorialEffectsFailure::ProtectedDisplay
            );
        }
        for invalid_calls in [changes(global, 0, false), changes(global, 1, true)] {
            assert!(qualifies(
                &before,
                &after,
                changed(&invalid_calls),
                incoming,
                &permissions(true)
            )
            .is_err());
        }
        let incorrect_cumulative = changes(global, 1, true);
        assert!(qualifies(
            &after,
            &after,
            edits(&[]),
            changed(&incorrect_cumulative),
            &permissions(true)
        )
        .is_err());
        qualifies(&after, &after, edits(&[]), incoming, &permissions(true)).unwrap();
        assert!(qualifies(&after, &after, incoming, incoming, &permissions(true)).is_err());
        let reverse = changes(global, 1, true);
        let undone = apply(&after, &reverse);
        qualifies(
            &after,
            &undone,
            changed(&reverse),
            edits(&[]),
            &permissions(true),
        )
        .unwrap();
    }
}
fn structural(
    before: &EncodedTeletextAuthoredService,
) -> Vec<TeletextMainLocalInvocationProgramOverlay> {
    [123, 124]
        .map(|page| {
            let owner = address(page);
            let expected = before.declared().semantic().page_enhancements()[&owner]
                .local_program
                .as_ref()
                .unwrap()
                .clone();
            let mut commands = expected.commands().to_vec();
            commands.insert(
                1,
                TeletextEnhancementCommand::InvokeObject {
                    target: TeletextObjectIdentity::local(owner, TeletextObjectId::new(1)),
                    object_type: TeletextObjectType::Passive,
                    offset: None,
                },
            );
            TeletextMainLocalInvocationProgramOverlay::new(
                owner,
                expected,
                TeletextEnhancementProgram::new(commands).unwrap(),
                vec![Some(0), None, Some(1)],
            )
            .unwrap()
        })
        .into()
}
#[test]
fn main_resource_targets_compose_with_same_owner_local_structure_and_root_ordinal_history() {
    for global in [false, true] {
        let before = baseline(global);
        let structure = structural(&before);
        let structured = object_page_selections::encode(
            before
                .declared()
                .clone()
                .with_main_local_invocation_program_overlays(
                    &structure,
                    policy().validation.service,
                )
                .unwrap(),
        );
        let current_calls = changes(global, 2, false);
        let cumulative_calls = changes(global, 1, false);
        let after = apply(&structured, &current_calls);
        let cumulative = TeletextEditorialEdits {
            main_programs: &structure,
            ..changed(&cumulative_calls)
        };
        qualifies(&before, &after, cumulative, cumulative, &permissions(true)).unwrap();
        qualifies(
            &structured,
            &after,
            changed(&current_calls),
            cumulative,
            &permissions(true),
        )
        .unwrap();
        qualifies(&after, &after, edits(&[]), cumulative, &permissions(true)).unwrap();
        assert!(qualifies(
            &structured,
            &after,
            changed(&cumulative_calls),
            cumulative,
            &permissions(true)
        )
        .is_err());
        let reverse = changes(global, 2, true);
        let undone = apply(&after, &reverse);
        qualifies(
            &after,
            &undone,
            changed(&reverse),
            TeletextEditorialEdits {
                main_programs: &structure,
                ..edits(&[])
            },
            &permissions(true),
        )
        .unwrap();
    }
}
