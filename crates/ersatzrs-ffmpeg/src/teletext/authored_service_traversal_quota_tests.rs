//! Complete services retain separate collection, two-pass spatial and ID credit.

use super::planning_quota_tests::calling_displays;
use super::update_quota_tests::{assert_same, single};
use super::*;

#[derive(Clone, Copy)]
enum Bucket {
    Commands,
    DrcsReferences,
    SpatialEvents,
    SpatialInvocations,
    Identities,
}

const BUCKETS: [Bucket; 5] = [
    Bucket::Commands,
    Bucket::DrcsReferences,
    Bucket::SpatialEvents,
    Bucket::SpatialInvocations,
    Bucket::Identities,
];

impl Bucket {
    fn set(self, budget: &mut TeletextAuthoredServiceEncodingBudget, maximum: Option<usize>) {
        match self {
            Self::Commands => budget.max_collected_commands = maximum,
            Self::DrcsReferences => budget.max_collected_drcs_references = maximum,
            Self::SpatialEvents => budget.max_spatial_event_visits = maximum,
            Self::SpatialInvocations => budget.max_spatial_invocation_visits = maximum,
            Self::Identities => budget.max_identity_checks = maximum,
        }
    }
    fn used(self, usage: TeletextAuthoredServiceEncodingUsage) -> Option<usize> {
        match self {
            Self::Commands => usage.collected_commands,
            Self::DrcsReferences => usage.collected_drcs_references,
            Self::SpatialEvents => usage.spatial_event_visits,
            Self::SpatialInvocations => usage.spatial_invocation_visits,
            Self::Identities => usage.identity_checks,
        }
    }
}

fn link(page: Option<TeletextRawDataPageNumber>) -> TeletextX27LinkDeclaration {
    TeletextX27LinkDeclaration::new(
        page.map_or(
            TeletextX27LinkTarget::Unspecified {
                magazine: TeletextX27Magazine::new(2).unwrap(),
            },
            |page| TeletextX27LinkTarget::Page { page },
        ),
        TeletextX27LinkValidity::Level25Only,
        TeletextSubpageRequirementMask::new(if page.is_some() { 1 } else { u16::MAX }),
    )
    .unwrap()
}

fn resource_displays(count: u16) -> TeletextDeclaredService {
    let mut draft = input();
    draft
        .base
        .pages
        .retain(|page| page.address.subcode.get() < count);
    draft
        .pages
        .retain(|page| page.address.subcode.get() < count);
    for page in &mut draft.base.pages {
        page.rows = rows("£");
    }
    for object in &mut draft.objects {
        object.object_type = TeletextObjectType::Active;
    }
    let drcs = resource(TeletextResourceKind::Drcs, 0xc0, 0);
    draft.drcs = vec![TeletextDrcsSubtable::new(
        drcs,
        vec![TeletextDrcsGlyphDefinition {
            slot: TeletextDrcsGlyphSlot::new(0).unwrap(),
            bitmap: TeletextDrcsBitmap::new([0x801; 10]).unwrap(),
        }],
    )
    .unwrap()];
    let links = TeletextX27Level25Links::new(
        link(None),
        link(Some(target(20).resource().unwrap().page())),
        link(None),
        link(Some(drcs.page())),
    )
    .unwrap();
    for page in &mut draft.pages {
        page.resources.defaults[2].as_mut().unwrap().object_type = TeletextObjectType::Active;
        page.resources.drcs = TeletextDrcsPageLinks::new(Some(drcs.page()), None).unwrap();
        page.compositional_links = Some(links);
        page.local_program = Some(
            TeletextEnhancementProgram::new(vec![
                TeletextEnhancementCommand::InvokeObject {
                    target: target(20),
                    object_type: TeletextObjectType::Active,
                    offset: None,
                },
                TeletextEnhancementCommand::SetPosition {
                    position: TeletextEnhancementPosition::new(2, 8).unwrap(),
                },
                TeletextEnhancementCommand::AtColumn {
                    column: 8,
                    operation: TeletextColumnOperation::Drcs {
                        reference: TeletextDrcsGlyphReference {
                            table: TeletextDrcsTableScope::Normal,
                            subtable: TeletextDrcsSubtableIndex::new(0).unwrap(),
                            slot: TeletextDrcsGlyphSlot::new(0).unwrap(),
                        },
                    },
                },
            ])
            .unwrap(),
        );
    }
    let companions = vec![resource(TeletextResourceKind::Pop, 0xa0, 1)];
    let mut semantic_budget = semantic_limits();
    // One authored DRCS occurrence per exact display in this enriched fixture.
    semantic_budget.max_collected_drcs_references = usize::from(count);
    let semantic = TeletextSemanticService::new_with_object_companions(
        draft,
        companions.clone(),
        semantic_budget,
    )
    .unwrap();
    TeletextDeclaredService::new(
        semantic,
        vec![mot_with_type(TeletextObjectType::Active)],
        (0..count)
            .map(|subcode| TeletextDisplayMotSelection {
                display: owner(subcode),
                mot: Some(resource(TeletextResourceKind::Mot, 0xfe, 9)),
            })
            .collect(),
        companions,
        declaration_limits(),
    )
    .unwrap()
}

fn fixture_limits() -> TeletextAuthoredServiceEncodingBudget {
    let mut budget = limits();
    // Original collection contains one DRCS reference. Physical replay may
    // retain it alongside one derived carrier for that same authored glyph.
    budget
        .normalization
        .composition
        .collection
        .max_drcs_references = 1;
    budget.normalization.replay.max_drcs_references = 2;
    budget
}

fn metered_limits() -> TeletextAuthoredServiceEncodingBudget {
    let mut budget = fixture_limits();
    for bucket in BUCKETS {
        bucket.set(&mut budget, Some(usize::MAX));
    }
    budget
}

fn encode(
    declared: TeletextDeclaredService,
    budget: &TeletextAuthoredServiceEncodingBudget,
) -> (
    Result<EncodedTeletextAuthoredService, TeletextAuthoredServiceEncodingError>,
    TeletextAuthoredServiceEncodingUsage,
) {
    let tx = transmissions(&declared);
    encode_teletext_authored_service_with_work_usage(declared, &tx, budget, None)
}

#[test]
fn exact_service_traversal_and_identity_totals_preserve_models_wire_and_both_spatial_passes() {
    let declared = resource_displays(2);
    let tx = transmissions(&declared);
    let expected =
        encode_teletext_authored_service(declared.clone(), &tx, &fixture_limits()).unwrap();
    let (full, usage) = encode(declared.clone(), &metered_limits());
    let full = full.unwrap();
    assert_same(&expected, &full);
    let mut exact = fixture_limits();
    for bucket in BUCKETS {
        assert!(bucket.used(usage).unwrap() > 0);
        bucket.set(&mut exact, bucket.used(usage));
    }
    let (result, exact_usage) = encode(declared, &exact);
    assert_eq!(exact_usage, usage);
    assert_same(&expected, &result.unwrap());
    let mut events = 0;
    let mut invocations = 0;
    for page in full.pages() {
        if let EncodedTeletextAuthoredPage::CompiledDisplay(page) = page {
            let original = page.normalized().original();
            let placement = resolve_teletext_spatial_program(
                original.owner(),
                original.program(),
                full.declared().semantic().objects(),
                fixture_limits().normalization.placement.spatial,
            )
            .unwrap();
            for trace in [
                original.spatial_trace(),
                &placement,
                page.normalized().physical().replay().spatial_trace(),
            ] {
                events += trace.events().len();
                invocations += trace.invocations().len();
            }
        }
    }
    assert_eq!(usage.spatial_event_visits, Some(events * 2));
    assert_eq!(usage.spatial_invocation_visits, Some(invocations * 2));
}

fn placement_spatial_failure(
    error: &TeletextAuthoredServiceEncodingError,
) -> (
    &TeletextOriginCarrierPlacementError,
    &TeletextSpatialTraceError,
) {
    let Failure::Compiled(error) = &error.reason else {
        panic!("expected compiled-page error: {error:?}");
    };
    let TeletextCompiledDisplayPageEncodingFailure::Normalization(error) = &error.reason else {
        panic!("expected normalization error: {error:?}");
    };
    let TeletextNormalizedMainProgramFailure::Placement(error) = &error.reason else {
        panic!("expected placement error: {error:?}");
    };
    let TeletextOriginCarrierPlacementFailure::Spatial(spatial) = &error.reason else {
        panic!("expected placement spatial error: {error:?}");
    };
    (error, spatial)
}

#[test]
fn exhausted_original_spatial_credit_rejects_placement_before_physical_compilation() {
    let declared = resource_displays(2);
    let (full, _) = encode(declared.clone(), &metered_limits());
    let full = full.unwrap();
    let original = full
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::CompiledDisplay(page) => {
                Some(page.normalized().original())
            }
            _ => None,
        })
        .unwrap();
    let placement = resolve_teletext_spatial_program(
        original.owner(),
        original.program(),
        full.declared().semantic().objects(),
        fixture_limits().normalization.placement.spatial,
    )
    .unwrap();
    let original_events = original.spatial_trace().events().len() * 2;
    let original_calls = original.spatial_trace().invocations().len() * 2;
    for invocations in [false, true] {
        let mut budget = metered_limits();
        budget.max_glyph_comparison_work = Some(usize::MAX);
        budget.max_display_comparison_updates = Some(usize::MAX);
        budget.max_assembly_work = Some(usize::MAX);
        budget.max_ancestry_work = Some(usize::MAX);
        if invocations {
            budget.max_spatial_invocation_visits = Some(original_calls);
        } else {
            budget.max_spatial_event_visits = Some(original_events);
        }
        let (result, admitted) = encode(declared.clone(), &budget);
        let outer = result.unwrap_err();
        assert_eq!(outer.display, Some(owner(0)));
        let (located, spatial) = placement_spatial_failure(&outer);
        let (event, ordinal) = if invocations {
            placement
                .events()
                .iter()
                .enumerate()
                .find_map(|(ordinal, event)| {
                    event
                        .called_invocation()
                        .is_some()
                        .then_some((event, ordinal))
                })
                .unwrap()
        } else {
            (&placement.events()[0], 0)
        };
        assert_eq!(spatial.owner, owner(0));
        assert_eq!(spatial.location, Some(event.source()));
        assert_eq!(spatial.root_source, Some(event.root_source()));
        assert_eq!(spatial.invocation, event.invocation());
        assert_eq!(located.location, spatial.location);
        assert_eq!(located.root_source, spatial.root_source);
        assert_eq!(located.invocation, spatial.invocation);
        assert_eq!(admitted.spatial_invocation_visits, Some(original_calls));
        if invocations {
            assert_eq!(
                spatial.reason,
                TeletextSpatialTraceFailure::InvocationBudget { maximum: 0 }
            );
            assert_eq!(
                admitted.spatial_event_visits,
                Some(original_events + ordinal + 1)
            );
        } else {
            assert_eq!(
                spatial.reason,
                TeletextSpatialTraceFailure::EventBudget { maximum: 0 }
            );
            assert_eq!(admitted.spatial_event_visits, Some(original_events));
        }
        assert_eq!(admitted.assembly_work, Some(0));
        assert_eq!(admitted.ancestry_work, Some(0));
        assert_eq!(admitted.glyph_comparison_work, Some(0));
        assert_eq!(admitted.display_comparison_updates, Some(0));
    }
}

#[test]
fn each_traversal_and_identity_credit_is_shared_between_resource_backed_displays() {
    let (first, first_usage) = encode(resource_displays(1), &metered_limits());
    first.unwrap();
    let declared = resource_displays(2);
    let (full, full_usage) = encode(declared.clone(), &metered_limits());
    full.unwrap();
    for bucket in BUCKETS {
        let first = bucket.used(first_usage).unwrap();
        assert!(first > 0);
        assert_eq!(bucket.used(full_usage), Some(first * 2));
        let mut budget = metered_limits();
        bucket.set(&mut budget, Some(first));
        let (result, admitted) = encode(declared.clone(), &budget);
        assert_eq!(result.unwrap_err().display, Some(owner(1)));
        assert_eq!(bucket.used(admitted), Some(first));
    }
}

#[test]
fn one_less_traversal_credit_denies_second_page_and_retains_other_admitted_buckets() {
    let declared = resource_displays(2);
    let (full, usage) = encode(declared.clone(), &metered_limits());
    full.unwrap();
    for bucket in BUCKETS {
        let maximum = bucket.used(usage).unwrap() - 1;
        let mut budget = metered_limits();
        bucket.set(&mut budget, Some(maximum));
        let (result, admitted) = encode(declared.clone(), &budget);
        assert_eq!(result.unwrap_err().display, Some(owner(1)));
        assert!(bucket.used(admitted).unwrap() <= maximum);
        for other in BUCKETS {
            assert!(
                other.used(admitted).unwrap() > 0,
                "the completed first page retains every bucket's charges"
            );
        }
    }
}

#[test]
fn native_zero_preflight_zero_and_late_failure_preserve_traversal_usage_without_partial_service() {
    let budget = metered_limits();
    let (native, usage) = encode(single(1), &budget);
    native.unwrap();
    assert_eq!(usage.collected_drcs_references, Some(0));
    assert_eq!(usage.identity_checks, Some(0));
    assert!(usage.collected_commands.unwrap() > 0);
    assert!(usage.spatial_event_visits.unwrap() > 0);
    assert!(usage.spatial_invocation_visits.unwrap() > 0);
    let declared = resource_displays(2);
    let (full, full_usage) = encode(declared.clone(), &budget);
    let mut late = budget;
    late.max_units = full.unwrap().unit_count() - 1;
    let (result, admitted) = encode(declared.clone(), &late);
    assert!(matches!(
        result.unwrap_err().reason,
        Failure::Budget {
            category: "units",
            ..
        }
    ));
    assert_eq!(admitted, full_usage);
    let (early, admitted) =
        encode_teletext_authored_service_with_work_usage(declared, &[], &budget, None);
    assert!(matches!(
        early.unwrap_err().reason,
        Failure::MissingTransmission
    ));
    for bucket in BUCKETS {
        assert_eq!(bucket.used(admitted), Some(0));
    }
}

#[test]
fn legacy_apis_honor_new_traversal_caps_and_disabled_usage_stays_unknown() {
    let (full, usage) = encode(calling_displays(2), &fixture_limits());
    full.unwrap();
    for bucket in BUCKETS {
        assert_eq!(bucket.used(usage), None);
    }
    let declared = resource_displays(2);
    let tx = transmissions(&declared);
    for bucket in BUCKETS {
        let mut budget = fixture_limits();
        bucket.set(&mut budget, Some(0));
        let error = encode_teletext_authored_service(declared.clone(), &tx, &budget).unwrap_err();
        assert_eq!(error.display, Some(owner(0)));
        let (result, usage) = encode(declared.clone(), &budget);
        assert_eq!(result.unwrap_err().to_string(), error.to_string());
        assert_eq!(bucket.used(usage), Some(0));
        let (result, compatible) =
            encode_teletext_authored_service_with_usage(declared.clone(), &tx, &budget, None);
        assert!(result.is_err());
        assert_eq!(compatible, None);
    }
}
