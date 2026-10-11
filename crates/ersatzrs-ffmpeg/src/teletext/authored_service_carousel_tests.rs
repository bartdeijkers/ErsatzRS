//! Complete-cycle acquisition order, immutable repeats and bounded timing.

use super::*;
use crate::teletext::authored_service_tests as fixtures;
use crate::teletext::*;
use ersatzrs_domain::*;

const HAM: [u8; 16] = [
    0x15, 0x02, 0x49, 0x5e, 0x64, 0x73, 0x38, 0x2f, 0xd0, 0xc7, 0x8c, 0x9b, 0xa1, 0xb6, 0xfd, 0xea,
];

fn resource(kind: TeletextResourceKind, byte: u8) -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        kind,
        TeletextRawDataPageNumber::new(2, byte).unwrap(),
        TeletextResourceSubpageIndex::new(0).unwrap(),
    )
    .unwrap()
}

fn rich_candidate(update: TeletextResourceFullUpdate) -> EncodedTeletextAuthoredService {
    let mut input = fixtures::input();
    let public = resource(TeletextResourceKind::Pop, 0xa2);
    let global = resource(TeletextResourceKind::Gpop, 0xa3);
    for (owner, count) in [(public, 505), (global, 0)] {
        let identity = if owner.kind() == TeletextResourceKind::Gpop {
            TeletextObjectIdentity::global(owner, TeletextObjectId::new(30)).unwrap()
        } else {
            TeletextObjectIdentity::public(owner, TeletextObjectId::new(30)).unwrap()
        };
        input.objects.push(TeletextEnhancementObjectDefinition {
            identity,
            object_type: TeletextObjectType::Active,
            program: TeletextEnhancementProgram::new(vec![
                TeletextEnhancementCommand::AtColumn {
                    column: 0,
                    operation: TeletextColumnOperation::Foreground {
                        colour: TeletextColourReference::new(1).unwrap(),
                    },
                };
                count
            ])
            .unwrap(),
        });
    }
    input.drcs.push(
        TeletextDrcsSubtable::new(resource(TeletextResourceKind::Gdrcs, 0xc1), vec![]).unwrap(),
    );
    let declared = fixtures::declared(input);
    let mut tx = fixtures::transmissions(&declared);
    for record in &mut tx {
        record.update = update;
        record.counter = TeletextResourceUpdateCounter::new(7).unwrap();
    }
    encode_teletext_authored_service(declared, &tx, &fixtures::limits()).unwrap()
}

pub(super) fn cycle(carousel: &mut TeletextCarousel) -> Vec<TeletextCarouselBatch> {
    let mut batches = vec![];
    loop {
        let batch = carousel.next_batch().unwrap();
        let done = batch.kind() == TeletextCarouselBatchKind::Terminator;
        batches.push(batch);
        if done {
            return batches;
        }
    }
}

pub(in crate::teletext) fn many_displays(
    count: usize,
    resources: bool,
) -> EncodedTeletextAuthoredService {
    service_with_resources(count, usize::from(resources), resources)
}

pub(super) fn service_with_resources(
    count: usize,
    resource_count: usize,
    magazines: bool,
) -> EncodedTeletextAuthoredService {
    let mut input = fixtures::input();
    let template = input.base.pages[0].clone();
    let enhancement = input.pages[1].clone();
    input.base.pages = (0..count)
        .map(|index| {
            let mut page = template.clone();
            page.address.subcode =
                TeletextSubcode::new(u16::try_from((index / 128) * 256 + index % 128).unwrap())
                    .unwrap();
            page
        })
        .collect();
    input.pages = input
        .base
        .pages
        .iter()
        .map(|page| {
            let mut entry = enhancement.clone();
            entry.address = page.address;
            entry.local_program = None;
            entry.resources.objects = TeletextResolvedObjectPageLinks {
                public: None,
                global: None,
            };
            entry.resources.defaults = [None; 4];
            entry
        })
        .collect();
    input.objects.clear();
    if resource_count != 1 {
        input.drcs = (0..resource_count)
            .map(|index| {
                let page =
                    TeletextRawDataPageNumber::new(2, 0xc0 + u8::try_from(index / 16).unwrap())
                        .unwrap();
                let identity = TeletextResourceIdentity::new(
                    TeletextResourceKind::Drcs,
                    page,
                    TeletextResourceSubpageIndex::new(u8::try_from(index % 16).unwrap()).unwrap(),
                )
                .unwrap();
                TeletextDrcsSubtable::new(identity, vec![]).unwrap()
            })
            .collect();
    }
    if !magazines {
        input.magazines.clear();
    }
    let mut semantic_budget = fixtures::semantic_limits();
    semantic_budget.max_records = 2_000;
    semantic_budget.max_structure_items = 20_000;
    semantic_budget.max_base_content_bytes = 100_000;
    let semantic = TeletextSemanticService::new(input, semantic_budget).unwrap();
    let selections = semantic
        .base()
        .pages
        .iter()
        .map(|page| TeletextDisplayMotSelection {
            display: page.address,
            mot: None,
        })
        .collect();
    let mut declaration_budget = fixtures::declaration_limits();
    declaration_budget.max_displays = 600;
    declaration_budget.max_resources = 600;
    let declared =
        TeletextDeclaredService::new(semantic, vec![], selections, vec![], declaration_budget)
            .unwrap();
    let tx = fixtures::transmissions(&declared);
    let mut budget = fixtures::limits();
    budget.max_pages = 600;
    budget.max_units = 20_000;
    encode_teletext_authored_service(declared, &tx, &budget).unwrap()
}

#[test]
fn intrinsic_acquisition_capacity_failure_is_atomic_at_replacement() {
    let mut carousel = TeletextCarousel::new_authored(many_displays(497, true)).unwrap();
    let batches = cycle(&mut carousel);
    assert_eq!(batches.len(), 998);
    assert_eq!(carousel.ticks, 998 * 1_800);
    let state = (
        carousel.page,
        carousel.unit,
        carousel.ticks,
        carousel.boundary,
        carousel.first_cycle,
        carousel.magazine_pending,
        carousel.resource_start_ticks,
        carousel.resource_resume,
    );
    let original = carousel.content.page(0).0.to_vec();
    assert_eq!(
        carousel.replace_authored_at_boundary(service_with_resources(1, 500, true)),
        Err(TeletextTransportError::CycleTiming {
            batches: 1_005,
            maximum: 1_000
        })
    );
    assert_eq!(
        (
            carousel.page,
            carousel.unit,
            carousel.ticks,
            carousel.boundary,
            carousel.first_cycle,
            carousel.magazine_pending,
            carousel.resource_start_ticks,
            carousel.resource_resume
        ),
        state
    );
    assert_eq!(carousel.content.page(0).0, original);
    assert_eq!(
        carousel.next_batch().unwrap().kind(),
        TeletextCarouselBatchKind::Magazine
    );
    assert_eq!(
        carousel.replace_authored_at_boundary(many_displays(499, true)),
        Err(TeletextTransportError::MidCycleReplacement)
    );
}

#[test]
fn display_only_cycles_do_not_invent_a_resource_repetition_limit() {
    let mut carousel = TeletextCarousel::new_authored(many_displays(500, false)).unwrap();
    let batches = cycle(&mut carousel);
    assert_eq!(batches.len(), 1_001);
    assert!(!batches
        .iter()
        .any(|batch| batch.kind() == TeletextCarouselBatchKind::Magazine));
}

#[test]
fn authored_cycle_acquires_magazine_then_mot_drcs_objects_and_displays() {
    let candidate = rich_candidate(TeletextResourceFullUpdate::Changed);
    let expected_kinds = [
        TeletextResourceKind::Mot,
        TeletextResourceKind::Drcs,
        TeletextResourceKind::Gdrcs,
        TeletextResourceKind::Pop,
        TeletextResourceKind::Pop,
        TeletextResourceKind::Pop,
        TeletextResourceKind::Gpop,
    ];
    let mut carousel = TeletextCarousel::new_authored(candidate).unwrap();
    let CarouselContent::Authored {
        service,
        resource_order,
    } = &carousel.content
    else {
        panic!("authored content");
    };
    assert_eq!(resource_order.len(), expected_kinds.len());
    for (&physical, kind) in resource_order.iter().zip(expected_kinds) {
        assert_eq!(
            service.pages()[physical]
                .resource_header()
                .unwrap()
                .identity()
                .kind(),
            kind
        );
    }
    let expected: Vec<_> = (0..carousel.content.page_count())
        .map(|logical| carousel.content.page(logical).0.to_vec())
        .collect();
    let magazines = carousel.content.magazines().to_vec();
    let batches = cycle(&mut carousel);
    assert_eq!(batches[0].kind(), TeletextCarouselBatchKind::Magazine);
    assert_eq!(batches[0].data_units(), magazines);
    assert_eq!(
        batches
            .iter()
            .filter(|batch| batch.kind() == TeletextCarouselBatchKind::Magazine)
            .count(),
        1
    );
    let mut cursor = 1;
    for units in expected {
        assert_eq!(batches[cursor].kind(), TeletextCarouselBatchKind::Header);
        assert_eq!(batches[cursor].data_units(), &units[..1]);
        cursor += 1;
        let mut body = vec![];
        while batches[cursor].kind() == TeletextCarouselBatchKind::Page {
            assert!(batches[cursor].data_units().len() <= 31);
            body.extend_from_slice(batches[cursor].data_units());
            cursor += 1;
        }
        assert_eq!(body, units[1..]);
    }
    assert_eq!(cursor, batches.len() - 1);
    for (index, batch) in batches.iter().enumerate() {
        assert_eq!(
            batch.presentation_ticks(),
            u64::try_from(index).unwrap() * 1_800
        );
    }
    assert!(carousel.is_cycle_boundary());
}

#[test]
fn maximum_object_page_body_is_split_into_31_and_11_units() {
    let mut carousel =
        TeletextCarousel::new_authored(rich_candidate(TeletextResourceFullUpdate::Changed))
            .unwrap();
    let logical = (0..carousel.content.page_count())
        .find(|&index| carousel.content.page(index).0.len() == 43)
        .unwrap();
    let units = carousel.content.page(logical).0.to_vec();
    while carousel.magazine_pending || carousel.page < logical {
        carousel.next_batch().unwrap();
    }
    assert_eq!(carousel.next_batch().unwrap().data_units(), &units[..1]);
    assert_eq!(carousel.next_batch().unwrap().data_units(), &units[1..32]);
    assert_eq!(carousel.next_batch().unwrap().data_units(), &units[32..]);
    assert_eq!(carousel.page, logical + 1);
}

#[test]
fn resource_repeats_clear_only_c4_c8_and_preserve_source_for_both_updates() {
    for update in [
        TeletextResourceFullUpdate::Changed,
        TeletextResourceFullUpdate::Unchanged,
    ] {
        let mut carousel = TeletextCarousel::new_authored(rich_candidate(update)).unwrap();
        let original: Vec<_> = (0..carousel.content.page_count())
            .map(|logical| {
                let (units, subcode, resource) = carousel.content.page(logical);
                (units.to_vec(), subcode, resource)
            })
            .collect();
        cycle(&mut carousel);
        for _ in 0..3 {
            let batches = cycle(&mut carousel);
            let headers: Vec<_> = batches
                .iter()
                .filter(|batch| batch.kind() == TeletextCarouselBatchKind::Header)
                .collect();
            for (logical, ((units, subcode, resource), batch)) in
                original.iter().zip(headers).enumerate()
            {
                let mut expected = units[0];
                expected[9] = HAM[usize::from(subcode.to_le_bytes()[0] >> 4)].reverse_bits();
                if resource.is_some() {
                    // Resource C7/C9/C10 are one; C8 is zero on repeat.
                    expected[12] = HAM[13].reverse_bits();
                    assert_eq!(batch.data_units()[0][13], units[0][13]);
                }
                assert_eq!(batch.data_units(), &[expected]);
                assert_eq!(carousel.content.page(logical).0, units);
            }
            assert_eq!(
                batches
                    .iter()
                    .filter(|batch| batch.kind() == TeletextCarouselBatchKind::Magazine)
                    .count(),
                1
            );
        }
    }
}

#[test]
fn magazine_prefix_and_partial_cycles_forbid_replacement_but_boundary_keeps_clock() {
    use crate::teletext::transport_tests::{draft, set};
    let mut carousel = TeletextCarousel::new_authored(fixtures::candidate()).unwrap();
    assert_eq!(
        carousel.next_batch().unwrap().kind(),
        TeletextCarouselBatchKind::Magazine
    );
    assert!(!carousel.is_cycle_boundary());
    assert_eq!(
        carousel.replace_authored_at_boundary(fixtures::candidate()),
        Err(TeletextTransportError::MidCycleReplacement)
    );
    assert_eq!(
        carousel.replace_at_boundary(set(vec![draft(201, 0, false)])),
        Err(TeletextTransportError::MidCycleReplacement)
    );
    cycle(&mut carousel);
    let ticks = carousel.ticks;
    carousel
        .replace_at_boundary(set(vec![draft(201, 0, false)]))
        .unwrap();
    assert_eq!(carousel.next_batch().unwrap().presentation_ticks(), ticks);
    cycle(&mut carousel);
    let ticks = carousel.ticks;
    carousel
        .replace_authored_at_boundary(fixtures::candidate())
        .unwrap();
    let magazine = carousel.next_batch().unwrap();
    assert_eq!(magazine.kind(), TeletextCarouselBatchKind::Magazine);
    assert_eq!(magazine.presentation_ticks(), ticks);
    cycle(&mut carousel);
    carousel
        .replace_at_boundary(set(vec![draft(202, 0, false)]))
        .unwrap();
    assert_eq!(
        carousel.next_batch().unwrap().kind(),
        TeletextCarouselBatchKind::Header
    );
}

#[test]
fn clock_overflow_leaves_prefix_header_body_and_terminator_cursors_unchanged() {
    let mut carousel = TeletextCarousel::new_authored(fixtures::candidate()).unwrap();
    loop {
        let state = (
            carousel.page,
            carousel.unit,
            carousel.boundary,
            carousel.first_cycle,
            carousel.magazine_pending,
        );
        let ticks = carousel.ticks;
        carousel.ticks = u64::MAX - 1_799;
        assert_eq!(
            carousel.next_batch(),
            Err(TeletextTransportError::ClockOverflow)
        );
        assert_eq!(
            (
                carousel.page,
                carousel.unit,
                carousel.boundary,
                carousel.first_cycle,
                carousel.magazine_pending
            ),
            state
        );
        assert_eq!(carousel.ticks, u64::MAX - 1_799);
        carousel.ticks = ticks;
        if carousel.next_batch().unwrap().kind() == TeletextCarouselBatchKind::Terminator {
            break;
        }
    }
}
