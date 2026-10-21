//! Forward relative positions in independently reset object definitions.

use super::*;
use crate::teletext::authored_service_tests::{declared, transmissions};

mod nonuniform_decoder;
mod nonuniform_header;

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

fn assert_nested_header_geometry(
    geometry: &TeletextDisplayGeometry,
    outer: TeletextObjectIdentity,
    child: TeletextObjectIdentity,
) {
    assert_eq!(geometry.object_regions().len(), 2);
    for (target, start, finish) in [(outer, (0, 0), (0, 8)), (child, (0, 8), (0, 8))] {
        let region = geometry
            .object_regions()
            .iter()
            .find(|region| region.instance().target() == target)
            .unwrap();
        assert_eq!(
            region.interval(),
            TeletextObjectAddressInterval::Known { start, finish }
        );
    }
    assert_eq!(geometry.object_open_count().maximum_known_open(), 2);
}

fn assert_nested_header_invocations(
    trace: &TeletextSpatialTrace,
    outer: TeletextObjectIdentity,
    child: TeletextObjectIdentity,
    outer_type: TeletextObjectType,
    child_type: TeletextObjectType,
) {
    assert_eq!(trace.invocations().len(), 2);
    let parent = trace.invocations()[0];
    let nested = trace.invocations()[1];
    assert_eq!(parent.target(), outer);
    assert_eq!(parent.object_type(), outer_type);
    assert_eq!(parent.parent(), None);
    assert_eq!(parent.offset(), None);
    assert_eq!((parent.origin().row(), parent.origin().column()), (0, 0));
    assert_eq!(
        (
            parent.caller_position().row(),
            parent.caller_position().column()
        ),
        (0, 0)
    );
    assert_eq!(nested.target(), child);
    assert_eq!(nested.object_type(), child_type);
    assert_eq!(nested.parent(), Some(parent.id()));
    assert_eq!(nested.offset(), None);
    assert_eq!((nested.origin().row(), nested.origin().column()), (0, 8));
    assert_eq!(
        (
            nested.caller_position().row(),
            nested.caller_position().column()
        ),
        (0, 8)
    );
    assert_eq!(nested.source().object, Some(outer));
    let main_call = trace
        .events()
        .iter()
        .find(|event| event.called_invocation() == Some(parent.id()))
        .unwrap();
    assert_eq!(main_call.invocation(), None);
    assert_eq!(
        main_call.source(),
        TeletextEnhancementSource {
            object: None,
            command: 0
        }
    );
    assert_eq!(main_call.relative_before(), (0, 0));
    assert_eq!(main_call.relative_after(), (0, 0));
    assert_eq!(
        (
            main_call.absolute_after().row(),
            main_call.absolute_after().column()
        ),
        (0, 0)
    );
}

fn assert_nested_header_trace(
    original: &TeletextMainComposition,
    outer: TeletextObjectIdentity,
    child: TeletextObjectIdentity,
    outer_type: TeletextObjectType,
    child_type: TeletextObjectType,
) {
    assert_nested_header_invocations(
        original.spatial_trace(),
        outer,
        child,
        outer_type,
        child_type,
    );
    assert_eq!(
        original.spatial_trace().invocations()[1].source().command,
        1
    );
    assert_nested_header_geometry(original.display_geometry(), outer, child);
}

fn nested_header_draft(
    outer_type: TeletextObjectType,
    child_type: TeletextObjectType,
) -> (
    TeletextResolvedServiceDraft,
    TeletextObjectIdentity,
    TeletextObjectIdentity,
) {
    assert!(outer_type.may_invoke(child_type));
    let mut draft = input();
    let owner = draft.base.pages[0].address;
    let outer = TeletextObjectIdentity::local(owner, TeletextObjectId::new(65534));
    let child = TeletextObjectIdentity::local(owner, TeletextObjectId::new(65535));
    let main = TeletextEnhancementProgram::new(vec![TeletextEnhancementCommand::InvokeObject {
        target: outer,
        object_type: outer_type,
        offset: None,
    }])
    .unwrap();
    draft.pages[0].local_program = Some(main.clone());
    let outer_program = TeletextEnhancementProgram::new(vec![
        position(0, 8),
        TeletextEnhancementCommand::InvokeObject {
            target: child,
            object_type: child_type,
            offset: None,
        },
    ])
    .unwrap();
    let child_program = TeletextEnhancementProgram::new(vec![foreground(0, 2), glyph(0)]).unwrap();
    draft.objects.extend([
        TeletextEnhancementObjectDefinition {
            identity: outer,
            object_type: outer_type,
            program: outer_program.clone(),
        },
        TeletextEnhancementObjectDefinition {
            identity: child,
            object_type: child_type,
            program: child_program.clone(),
        },
    ]);
    draft.base.pages[0].rows.push(TeletextRow {
        row: 0,
        runs: vec![TeletextRun {
            column: 8,
            style: TeletextStyle::default(),
            content: TeletextRunContent::Text { text: "Z".into() },
        }],
    });
    (draft, outer, child)
}

fn append_header_suffix(draft: &mut TeletextResolvedServiceDraft, outer: TeletextObjectIdentity) {
    let definition = draft
        .objects
        .iter_mut()
        .find(|d| d.identity == outer)
        .unwrap();
    let mut commands = definition.program.commands().to_vec();
    commands.push(TeletextEnhancementCommand::AtColumn {
        column: 9,
        operation: TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(b'B').unwrap(),
        },
    });
    definition.program = TeletextEnhancementProgram::new(commands).unwrap();
}

#[test]
fn nested_local_header_with_trailing_glyph_passes_complete_factory() {
    for (outer_type, child_type) in [
        (TeletextObjectType::Active, TeletextObjectType::Passive),
        (TeletextObjectType::Active, TeletextObjectType::Adaptive),
        (TeletextObjectType::Adaptive, TeletextObjectType::Passive),
    ] {
        let (mut draft, outer, child) = nested_header_draft(outer_type, child_type);
        append_header_suffix(&mut draft, outer);
        let saved = declared(draft);
        let owner = saved.semantic().base().pages[0].address;
        let original = compose_teletext_main_program(
            saved.semantic(),
            owner,
            limits().normalization.composition,
        )
        .unwrap();
        let actual =
            encode_teletext_authored_service(saved.clone(), &transmissions(&saved), &limits())
                .unwrap_or_else(|error| {
                    panic!("legal {outer_type:?}/{child_type:?} forward suffix: {error:?}")
                });
        assert_eq!(actual.declared(), &saved);
        let EncodedTeletextAuthoredPage::CompiledDisplay(display) = &actual.pages()[0] else {
            panic!("complete authored display required");
        };
        let replay = display.normalized().physical().replay();
        for (resolution, intent, trace) in [
            (
                original.display_resolution(),
                original.glyph_intent(),
                original.spatial_trace(),
            ),
            (
                replay.display_resolution(),
                replay.glyph_intent(),
                replay.spatial_trace(),
            ),
        ] {
            assert_selected_child_a(resolution, intent);
            let cell = resolution.cell(0, 9).unwrap();
            let candidate = resolution.origins()[cell.origin().unwrap()].candidate_index();
            assert!(matches!(intent.records()[candidate].glyph(),
                Some(TeletextGlyphIdentity::G0 { glyph }) if glyph.code() == b'B'));
            assert_nested_header_invocations(trace, outer, child, outer_type, child_type);
        }
        assert_untouched_header_tail(original.display_resolution(), replay.display_resolution());
        let suffix: Vec<_> = display
            .normalized()
            .encoded()
            .command_positions()
            .iter()
            .filter(|span| span.source().object == Some(outer) && span.source().command == 3)
            .collect();
        assert_eq!(suffix.len(), 1);
        assert_eq!(
            suffix[0].original_object_source(),
            Some(TeletextMainCompositionSource::Object {
                identity: outer,
                command: 2,
            })
        );
        let fallback = encode_teletext_page(
            &saved.semantic().base().pages[0],
            TeletextEncodingRegion::WesternLatin,
        )
        .unwrap();
        let physical = project_encoded_teletext_display(display.page(), 128).unwrap();
        let baseline = project_encoded_teletext_display(&fallback, 128).unwrap();
        assert_eq!(physical.header, baseline.header);
        let rows = |records: Vec<TeletextEncodedDisplayRecord>| {
            records
                .into_iter()
                .filter(|record| !matches!(record, TeletextEncodedDisplayRecord::Triplets { .. }))
                .collect::<Vec<_>>()
        };
        assert_eq!(rows(physical.records), rows(baseline.records));
    }
}

#[test]
fn nested_local_adaptive_header_position_preserves_original_parent_ap_in_complete_factory() {
    nested_header_factory(TeletextObjectType::Adaptive, TeletextObjectType::Passive);
}

#[test]
fn nested_local_active_header_position_preserves_original_parent_ap_in_complete_factory() {
    nested_header_factory(TeletextObjectType::Active, TeletextObjectType::Passive);
}

#[test]
fn nested_local_active_to_adaptive_header_position_preserves_original_parent_ap_in_complete_factory(
) {
    nested_header_factory(TeletextObjectType::Active, TeletextObjectType::Adaptive);
}

fn nested_header_factory(outer_type: TeletextObjectType, child_type: TeletextObjectType) {
    let (draft, outer, child) = nested_header_draft(outer_type, child_type);
    let owner = draft.base.pages[0].address;
    let main = draft.pages[0].local_program.clone().unwrap();
    let [outer_program, child_program] = [outer, child].map(|target| {
        draft
            .objects
            .iter()
            .find(|definition| definition.identity == target)
            .unwrap()
            .program
            .clone()
    });
    let saved = declared(draft);
    let original =
        compose_teletext_main_program(saved.semantic(), owner, limits().normalization.composition)
            .unwrap();
    assert_nested_header_trace(&original, outer, child, outer_type, child_type);
    assert_selected_child_a(original.display_resolution(), original.glyph_intent());
    let actual = encode_teletext_authored_service(saved.clone(), &transmissions(&saved), &limits())
        .unwrap_or_else(|error| {
            panic!("legal {outer_type:?} relative AP0/8 before {child_type:?}: {error:?}")
        });
    assert_eq!(actual.declared(), &saved);
    assert_eq!(
        saved
            .semantic()
            .page_enhancements()
            .get(&owner)
            .unwrap()
            .local_program
            .as_ref(),
        Some(&main)
    );
    for (target, program) in [(outer, outer_program), (child, child_program)] {
        assert_eq!(
            saved
                .semantic()
                .objects()
                .definitions()
                .iter()
                .find(|definition| definition.identity == target)
                .unwrap()
                .program,
            program
        );
    }
    let EncodedTeletextAuthoredPage::CompiledDisplay(display) = &actual.pages()[0] else {
        panic!("complete authored display required");
    };
    assert_nested_header_invocations(
        display.normalized().physical().replay().spatial_trace(),
        outer,
        child,
        outer_type,
        child_type,
    );
    assert_original_object_sources(display.normalized().encoded(), outer, child);
    let replay = display.normalized().physical().replay();
    assert_selected_child_a(replay.display_resolution(), replay.glyph_intent());
    assert_untouched_header_tail(original.display_resolution(), replay.display_resolution());
    for geometry in [
        display.normalized().original().display_geometry(),
        display.normalized().physical().replay().display_geometry(),
    ] {
        assert_nested_header_geometry(geometry, outer, child);
    }
    let fallback = encode_teletext_page(
        &saved.semantic().base().pages[0],
        TeletextEncodingRegion::WesternLatin,
    )
    .unwrap();
    let physical = project_encoded_teletext_display(display.page(), 128).unwrap();
    let baseline = project_encoded_teletext_display(&fallback, 128).unwrap();
    assert_eq!(physical.header, baseline.header);
    let rows = |records: Vec<TeletextEncodedDisplayRecord>| {
        records
            .into_iter()
            .filter(|record| !matches!(record, TeletextEncodedDisplayRecord::Triplets { .. }))
            .collect::<Vec<_>>()
    };
    assert_eq!(rows(physical.records), rows(baseline.records));
}

fn assert_original_object_sources(
    packed: &EncodedTeletextLocalObjectProgram,
    outer: TeletextObjectIdentity,
    child: TeletextObjectIdentity,
) {
    let mut outer_spans = 0;
    for span in packed.command_positions() {
        match span.source().object {
            Some(identity) if identity == outer => {
                outer_spans += 1;
                let expected = match span.source().command {
                    1 => TeletextMainCompositionSource::HeaderPosition { command: 0 },
                    2 => TeletextMainCompositionSource::Object {
                        identity,
                        command: 1,
                    },
                    other => panic!("unexpected actual wrapper command {other}"),
                };
                assert_eq!(span.original_object_source(), Some(expected));
            }
            Some(identity) if identity == child => assert_eq!(
                span.original_object_source(),
                Some(TeletextMainCompositionSource::Object {
                    identity,
                    command: span.source().command,
                })
            ),
            _ => assert_eq!(span.original_object_source(), None),
        }
    }
    assert_eq!(outer_spans, 2);
    // Final rebinding cannot fall back to an actual ordinal when a producer
    // provides no original correspondence, including previously retained spans.
    let mut unmapped = packed.clone();
    unmapped.bind_original_object_sources(|_| None);
    assert!(unmapped
        .command_positions()
        .iter()
        .all(|span| span.original_object_source().is_none()));
}

fn qualified_outer_background(
    original: &TeletextMainComposition,
    outer: TeletextObjectIdentity,
) -> TeletextColourReference {
    let colour = TeletextColourReference::new(0).unwrap();
    let calls: Vec<_> = original
        .spatial_trace()
        .invocations()
        .iter()
        .filter(|invocation| invocation.target() == outer)
        .collect();
    assert_eq!(calls.len(), 1);
    assert_eq!(original.display_geometry().total_width(), 40);
    // A final Adaptive call may leave its own extent unresolved. Check the
    // entire possible same-row tail for every actual caller, not the child's
    // private attributes or a supposed fixed Adaptive default.
    for invocation in calls {
        let row = u8::try_from(invocation.origin().row()).unwrap();
        let first = u8::try_from(invocation.origin().column() + 8).unwrap();
        for column in first..40 {
            let background = original
                .attribute_execution()
                .before_adaptive_cell(row, column)
                .unwrap()
                .background();
            assert_eq!(background.value(), Some(colour));
            assert_eq!(background.pending(), None);
            assert!(matches!(
                background.source(),
                Some(TeletextAttributeSource::Base { .. })
            ));
        }
    }
    colour
}

fn assert_selected_child_a(display: &TeletextDisplayResolution, intent: &TeletextGlyphIntent) {
    let cell = display.cell(0, 8).unwrap();
    assert_eq!(
        cell.attributes().foreground().value(),
        Some(TeletextColourReference::new(2).unwrap())
    );
    assert_eq!(
        cell.attributes().background().value(),
        Some(TeletextColourReference::new(0).unwrap())
    );
    let candidate = display.origins()[cell.origin().unwrap()].candidate_index();
    assert!(matches!(intent.records()[candidate].glyph(),
        Some(TeletextGlyphIdentity::G0 { glyph }) if glyph.code() == b'A'));
}

fn assert_untouched_header_tail(
    original: &TeletextDisplayResolution,
    physical: &TeletextDisplayResolution,
) {
    for column in 9..40 {
        for model in [original, physical] {
            let attributes = model.cell(0, column).unwrap().attributes();
            assert_eq!(
                attributes.foreground().value(),
                Some(TeletextColourReference::new(7).unwrap())
            );
            assert_eq!(
                attributes.background().value(),
                Some(TeletextColourReference::new(0).unwrap())
            );
            assert_eq!(attributes.foreground().pending(), None);
            assert_eq!(attributes.background().pending(), None);
        }
    }
}

#[test]
fn diagnostic_nested_adaptive_background_candidate_preserves_full_original_model_and_calls() {
    let (draft, outer, child) =
        nested_header_draft(TeletextObjectType::Adaptive, TeletextObjectType::Passive);
    let saved = declared(draft);
    let service = saved.semantic();
    let budget = limits().normalization;
    let original =
        compose_teletext_main_program(service, service.base().pages[0].address, budget.composition)
            .unwrap();
    let unchanged = original.clone();
    assert_nested_header_trace(
        &original,
        outer,
        child,
        TeletextObjectType::Adaptive,
        TeletextObjectType::Passive,
    );
    assert_selected_child_a(original.display_resolution(), original.glyph_intent());
    let colour = qualified_outer_background(&original, outer);
    let (derived, work) = super::super::object_header_positions::derive(
        &original,
        service.objects(),
        budget.packing,
        usize::MAX,
    );
    let derived = derived.unwrap();
    assert!(work > 0);
    let definition = derived.catalog().definition(outer).unwrap();
    assert_eq!(definition.object_type, TeletextObjectType::Adaptive);
    assert_eq!(
        definition.program.commands(),
        &[
            position(0, 8),
            TeletextEnhancementCommand::AtColumn {
                column: 8,
                operation: TeletextColumnOperation::Background { colour },
            },
            TeletextEnhancementCommand::InvokeObject {
                target: child,
                object_type: TeletextObjectType::Passive,
                offset: None
            }
        ]
    );
    let candidate = derived.catalog();
    let allocations =
        allocate_teletext_resource_objects(candidate, &[], budget.max_resource_objects).unwrap();
    let physical = crate::teletext::composed_main::compile_physical_with_context(
        service,
        &original,
        (original.program(), candidate),
        &allocations,
        (budget.replay, budget.packing),
        None,
        &mut crate::teletext::compile_context::TeletextCompileContext::new(None, None, None),
    )
    .unwrap();
    let replay = physical.replay();
    assert_nested_header_invocations(
        replay.spatial_trace(),
        outer,
        child,
        TeletextObjectType::Adaptive,
        TeletextObjectType::Passive,
    );
    assert_nested_header_geometry(replay.display_geometry(), outer, child);
    assert_selected_child_a(replay.display_resolution(), replay.glyph_intent());
    let comparison =
        compare_teletext_physical_display(&original, replay, budget.display_comparison).unwrap();
    assert!(
        replay
            .attribute_execution()
            .pending()
            .iter()
            .any(|pending| pending.reason() == TeletextAttributePendingReason::AdaptiveExtent),
        "candidate feasibility diagnostic: {:#?}",
        comparison.diagnostics()
    );
    // The append-only AdaptiveExtent history above does not make any retained
    // origin or selected model uncertain for this qualified uniform background.
    // Zero raw-model findings are not a factory or ancestry/provenance capability.
    assert!(
        comparison.diagnostics().is_empty(),
        "actual candidate comparison: {:#?}",
        comparison.diagnostics()
    );
    assert_eq!(original, unchanged);
}

#[test]
fn object_header_derivation_maps_only_retained_original_commands() {
    for (outer_type, suffix) in [TeletextObjectType::Adaptive, TeletextObjectType::Active]
        .into_iter()
        .flat_map(|kind| [(kind, false), (kind, true)])
    {
        let (mut draft, outer, child) =
            nested_header_draft(outer_type, TeletextObjectType::Passive);
        if suffix {
            append_header_suffix(&mut draft, outer);
        }
        let unused = TeletextObjectIdentity::local(
            draft.base.pages[0].address,
            TeletextObjectId::new(65533),
        );
        let mut definition = draft
            .objects
            .iter()
            .find(|definition| definition.identity == outer)
            .unwrap()
            .clone();
        definition.identity = unused;
        draft.objects.push(definition);
        let saved = declared(draft);
        let service = saved.semantic();
        let budget = limits().normalization;
        let original = compose_teletext_main_program(
            service,
            service.base().pages[0].address,
            budget.composition,
        )
        .unwrap();
        let saved_catalog = service.objects().clone();
        let (derived, _) = super::super::object_header_positions::derive(
            &original,
            service.objects(),
            budget.packing,
            usize::MAX,
        );
        let derived = derived.unwrap();
        assert_eq!(service.objects(), &saved_catalog);
        assert_eq!(
            derived.catalog().definition(unused),
            saved_catalog.definition(unused)
        );
        let source = |target, command| TeletextEnhancementSource {
            object: Some(target),
            command,
        };
        for (physical, retained) in [
            (0, Some(0)),
            (1, None),
            (2, Some(1)),
            (3, suffix.then_some(2)),
            (4, None),
        ] {
            assert_eq!(
                derived.retained_source(source(outer, physical)),
                retained.map(|command| source(outer, command))
            );
        }
        let (catalog, sources) = derived.into_parts();
        assert_eq!(
            catalog.definition(outer).unwrap().program.commands().len(),
            if suffix { 4 } else { 3 }
        );
        for target in [child, unused] {
            assert_eq!(
                sources.retained_source(source(target, 1)),
                Some(source(target, 1))
            );
            let length = saved_catalog
                .definition(target)
                .unwrap()
                .program
                .commands()
                .len();
            if suffix && target == unused {
                assert_eq!(
                    sources.retained_source(source(target, 2)),
                    Some(source(target, 2))
                );
            }
            assert_eq!(sources.retained_source(source(target, length)), None);
            assert_eq!(sources.materialized_position(source(target, 1)), None);
        }
        assert_eq!(sources.materialized_position(source(outer, 1)), Some(0));
        assert_eq!(sources.materialized_position(source(outer, 3)), None);
        assert_eq!(
            sources.materialized_position(source(outer, usize::MAX)),
            None
        );
        let unknown = TeletextObjectIdentity::local(original.owner(), TeletextObjectId::new(65532));
        assert_eq!(sources.retained_source(source(unknown, 0)), None);
    }
}

#[test]
fn object_header_derivation_uses_shared_foreground_when_invocation_backgrounds_differ() {
    for outer_type in [TeletextObjectType::Adaptive, TeletextObjectType::Active] {
        for suffix in [false, true] {
            conflicting_actual_backgrounds(outer_type, suffix);
        }
    }
}

fn conflicting_actual_backgrounds(outer_type: TeletextObjectType, suffix: bool) {
    let (mut draft, outer, _) = nested_header_draft(outer_type, TeletextObjectType::Passive);
    if suffix {
        append_header_suffix(&mut draft, outer);
    }
    let call = TeletextEnhancementCommand::InvokeObject {
        target: outer,
        object_type: outer_type,
        offset: None,
    };
    draft.pages[0].local_program =
        Some(TeletextEnhancementProgram::new(vec![call, position(1, 0), call]).unwrap());
    draft.base.pages[0].rows.retain(|row| row.row != 1);
    draft.base.pages[0].rows.push(TeletextRow {
        row: 1,
        runs: vec![TeletextRun {
            column: 8,
            style: TeletextStyle {
                background: TeletextColour::Blue,
                ..TeletextStyle::default()
            },
            content: TeletextRunContent::Text {
                text: "Z".repeat(32),
            },
        }],
    });
    let saved = declared(draft);
    let service = saved.semantic();
    let budget = limits().normalization;
    let original =
        compose_teletext_main_program(service, service.base().pages[0].address, budget.composition)
            .unwrap();
    let calls: Vec<_> = original
        .spatial_trace()
        .invocations()
        .iter()
        .filter(|invocation| invocation.target() == outer)
        .collect();
    assert_eq!(calls.len(), 2);
    for (row, colour) in [(0, 0), (1, 4)] {
        assert_eq!(
            original
                .attribute_execution()
                .before_adaptive_cell(row, 8)
                .unwrap()
                .background()
                .value(),
            Some(TeletextColourReference::new(colour).unwrap())
        );
    }
    let (derived, admitted) = super::super::object_header_positions::derive(
        &original,
        service.objects(),
        budget.packing,
        usize::MAX,
    );
    assert!(admitted > 0);
    nonuniform_header::assert_foreground_derivation(
        &original,
        service.objects(),
        &derived.unwrap(),
        outer,
    );
}

#[test]
fn object_header_derivation_preserves_changing_row_colours_with_steady_context() {
    let (mut draft, outer, child) =
        nested_header_draft(TeletextObjectType::Active, TeletextObjectType::Passive);
    draft.base.pages[0]
        .rows
        .iter_mut()
        .find(|row| row.row == 0)
        .unwrap()
        .runs
        .push(TeletextRun {
            column: 20,
            style: TeletextStyle {
                background: TeletextColour::Blue,
                ..TeletextStyle::default()
            },
            content: TeletextRunContent::Text {
                text: "Z".repeat(20),
            },
        });
    let saved = declared(draft);
    let service = saved.semantic();
    let budget = limits().normalization;
    let original =
        compose_teletext_main_program(service, service.base().pages[0].address, budget.composition)
            .unwrap();
    for (column, colour) in [(8, 0), (20, 4), (39, 4)] {
        let background = original
            .attribute_execution()
            .before_adaptive_cell(0, column)
            .unwrap()
            .background();
        assert_eq!(
            background.value(),
            Some(TeletextColourReference::new(colour).unwrap())
        );
        assert_eq!(background.pending(), None);
    }
    let (derived, admitted) = super::super::object_header_positions::derive(
        &original,
        service.objects(),
        budget.packing,
        usize::MAX,
    );
    assert!(admitted > 0);
    nonuniform_header::assert_flash_derivation(
        &original,
        service.objects(),
        &derived.unwrap(),
        outer,
    );
    nonuniform_header::assert_complete(&saved, &original, (outer, child));
}

#[test]
fn object_header_derivation_enforces_work_and_catalogue_bounds_before_cloning() {
    use super::super::object_header_positions::{derive, DerivationError};

    let (draft, _, _) =
        nested_header_draft(TeletextObjectType::Adaptive, TeletextObjectType::Passive);
    let saved = declared(draft);
    let service = saved.semantic();
    let budget = limits().normalization;
    let original =
        compose_teletext_main_program(service, service.base().pages[0].address, budget.composition)
            .unwrap();
    let before = service.objects().clone();
    let (zero, admitted) = derive(&original, &before, budget.packing, 0);
    assert!(matches!(
        zero,
        Err(DerivationError::WorkBudget { maximum: 0 })
    ));
    assert_eq!(admitted, 0);
    let (complete, required) = derive(&original, &before, budget.packing, usize::MAX);
    complete.unwrap();
    let (short, admitted) = derive(&original, &before, budget.packing, required - 1);
    assert!(matches!(short, Err(DerivationError::WorkBudget { .. })));
    assert!(admitted > 0 && admitted < required);
    let total: usize = before
        .definitions()
        .iter()
        .map(|definition| definition.program.commands().len())
        .sum();
    let (commands, _) = derive(
        &original,
        &before,
        TeletextObjectCatalogBudget {
            max_commands: total,
            ..budget.packing
        },
        usize::MAX,
    );
    assert!(matches!(
        commands,
        Err(DerivationError::Catalog(
            TeletextObjectCatalogError::CommandBudget { .. }
        ))
    ));
    let (objects, admitted) = derive(
        &original,
        &before,
        TeletextObjectCatalogBudget {
            max_objects: before.definitions().len() - 1,
            ..budget.packing
        },
        usize::MAX,
    );
    assert!(matches!(
        objects,
        Err(DerivationError::Catalog(
            TeletextObjectCatalogError::ObjectBudget { .. }
        ))
    ));
    assert_eq!(admitted, 0);
    assert_eq!(service.objects(), &before);
}
