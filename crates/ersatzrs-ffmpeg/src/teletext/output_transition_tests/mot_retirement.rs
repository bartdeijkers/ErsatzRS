//! Retiring a fully acknowledged neutral MOT must not retain active bank state.

use super::*;
use crate::teletext::TeletextServiceResourceTransmission;

fn neutral_mot() -> TeletextMotLevel25Declaration {
    let target = TeletextX27LinkTarget::Unspecified {
        magazine: TeletextX27Magazine::new(2).unwrap(),
    };
    let extent = TeletextMotSubpageExtent::new(0).unwrap();
    let object = TeletextMotObjectLink::new(
        target,
        extent,
        false,
        TeletextMotFallback::new(0).unwrap(),
        [TeletextMotDefaultObject::NoObject { raw_number: 0 }; 2],
    );
    let drcs = TeletextMotDrcsLink::new(target, extent, false);
    TeletextMotLevel25Declaration::new(
        TeletextResourceIdentity::new(
            TeletextResourceKind::Mot,
            TeletextRawDataPageNumber::new(2, 0xfe).unwrap(),
            TeletextResourceSubpageIndex::new(0).unwrap(),
        )
        .unwrap(),
        vec![],
        TeletextMotLevel25Links::new(
            object,
            [object; 7],
            drcs,
            [drcs; 7],
            TeletextMotEnhancementCounts::new(0, 0, 0),
        )
        .unwrap(),
    )
    .unwrap()
}

fn with_mot(mot: TeletextMotLevel25Declaration) -> Arc<EncodedTeletextAuthoredService> {
    let base = simple("nld", None);
    let semantic = base.declared().semantic().clone();
    let display = semantic.base().pages[0].address;
    let resource = mot.identity();
    let declared = TeletextDeclaredService::new(
        semantic,
        vec![mot],
        vec![TeletextDisplayMotSelection {
            display,
            mot: Some(resource),
        }],
        vec![],
        authored::declaration_limits(),
    )
    .unwrap();
    let transmission = TeletextServiceResourceTransmission {
        resource,
        counter: TeletextResourceUpdateCounter::new(1).unwrap(),
        update: TeletextResourceFullUpdate::Changed,
        last_subpage: true,
    };
    Arc::new(
        super::super::super::encode_teletext_authored_service(
            declared,
            &[transmission],
            &authored::limits(),
        )
        .unwrap()
        .with_resource_lifecycle_variants(std::time::Instant::now() + Duration::from_secs(30))
        .unwrap(),
    )
}

#[test]
fn neutral_other_s1_cannot_erase_an_earlier_nonempty_mot_owner() {
    let mut input = authored::input();
    input.magazines.clear();
    let declared = authored::declared(input);
    let transmissions = authored::transmissions(&declared);
    let old = Arc::new(
        super::super::super::encode_teletext_authored_service(
            declared,
            &transmissions,
            &authored::limits(),
        )
        .unwrap(),
    );
    let cleared_other = with_mot(neutral_mot());
    let bankless = simple("nld", None);
    let (mut gate, source) = gate(old, 400);
    let (receipt, _, now) = cycle(&mut gate);
    gate.replace_authored_at_acknowledged_boundary(&receipt, &cleared_other, now)
        .unwrap();
    let (receipt, _, now) = cycle(&mut gate);
    let first = usize::try_from(gate.source_start).unwrap();
    assert_eq!(
        gate.replace_authored_at_acknowledged_boundary(&receipt, &bankless, now),
        Err(TeletextOutputError::CacheReconciliationRequired),
        "a completed empty S1=0 does not erase the old populated S1=9"
    );
    let (receipt, output, now) = cycle(&mut gate);
    assert_source_positions(&output, &source, first);
    assert!(Arc::ptr_eq(
        gate.warm_service.as_ref().unwrap(),
        &cleared_other
    ));
    gate.withdraw_at_acknowledged_boundary(&receipt, now)
        .unwrap();
    let (receipt, _, now) = deliver(&mut gate, 1);
    assert_eq!(gate.acknowledged_nonempty_mots[2], 1 << 9);
    assert_eq!(
        gate.replace_authored_at_acknowledged_boundary(&receipt, &bankless, now),
        Err(TeletextOutputError::CacheReconciliationRequired),
        "withdrawing the descriptor does not erase receiver state"
    );
    let empty = neutral_mot();
    let exact_owner = TeletextMotLevel25Declaration::new(
        TeletextResourceIdentity::new(
            TeletextResourceKind::Mot,
            empty.identity().page(),
            TeletextResourceSubpageIndex::new(9).unwrap(),
        )
        .unwrap(),
        empty.associations().to_vec(),
        empty.links(),
    )
    .unwrap();
    gate.replace_authored_at_acknowledged_boundary(&receipt, &with_mot(exact_owner), now)
        .unwrap();
    assert_eq!(gate.acknowledged_nonempty_mots[2], 1 << 9);
    assert_eq!(
        gate.replace_authored_at_acknowledged_boundary(&receipt, &bankless, now),
        Err(TeletextOutputError::Acknowledgement),
        "a proposed neutral bank has not been delivered yet"
    );
    let (receipt, _, now) = cycle(&mut gate);
    assert_eq!(gate.acknowledged_nonempty_mots, [0; 8]);
    let first = usize::try_from(gate.source_start).unwrap();
    gate.replace_authored_at_acknowledged_boundary(&receipt, &bankless, now)
        .unwrap();
    let (_, output, _) = cycle(&mut gate);
    assert_source_positions(&output, &source, first);
    assert!(Arc::ptr_eq(gate.warm_service.as_ref().unwrap(), &bankless));
    assert!(gate.failure.is_none());
}

#[test]
fn unused_slots_and_unselected_associations_still_prevent_empty_mot_classification() {
    let empty = neutral_mot();
    let is_empty = super::super::transitions::empty_mot;
    assert!(is_empty(&empty));
    for selection in [
        TeletextMotAssociationSelection::new(true, None),
        TeletextMotAssociationSelection::new(
            false,
            Some(TeletextMotPublicLinkIndex::new(7).unwrap()),
        ),
    ] {
        for (objects, drcs) in [
            (selection, TeletextMotAssociationSelection::new(false, None)),
            (TeletextMotAssociationSelection::new(false, None), selection),
        ] {
            let mot = TeletextMotLevel25Declaration::new(
                empty.identity(),
                vec![TeletextMotPageAssociation::new(0x80, objects, drcs)],
                empty.links(),
            )
            .unwrap();
            assert!(!is_empty(&mot), "all raw association owners matter");
        }
    }
    let links = empty.links();
    let object = links.object_link(None);
    let drcs = links.drcs_link(None);
    for counts in [
        TeletextMotEnhancementCounts::new(4, 0, 0),
        TeletextMotEnhancementCounts::new(0, 1, 0),
        TeletextMotEnhancementCounts::new(0, 0, 1),
    ] {
        let bank =
            TeletextMotLevel25Links::new(object, [object; 7], drcs, [drcs; 7], counts).unwrap();
        let mot = TeletextMotLevel25Declaration::new(empty.identity(), vec![], bank).unwrap();
        assert!(!is_empty(&mot), "an advertised inventory is not empty");
    }
    let mut public = [object; 7];
    public[6] = TeletextMotObjectLink::new(
        object.target(),
        object.extent(),
        object.supersedable(),
        TeletextMotFallback::new(8).unwrap(),
        [
            TeletextMotDefaultObject::Dormant {
                object_type: TeletextObjectType::Passive,
                raw_number: 1,
            },
            TeletextMotDefaultObject::NoObject { raw_number: 0 },
        ],
    );
    let bank =
        TeletextMotLevel25Links::new(object, public, drcs, [drcs; 7], links.counts()).unwrap();
    let mot = TeletextMotLevel25Declaration::new(empty.identity(), vec![], bank).unwrap();
    assert!(
        !is_empty(&mot),
        "dormant slot metadata is not erased by omission"
    );
}

#[test]
fn completed_neutral_mot_can_retire_without_restarting_source_or_losing_display() {
    let old = with_mot(neutral_mot());
    let next = simple("nld", None);
    let (mut gate, source) = gate(Arc::clone(&old), 100);
    let (receipt, _, now) = cycle(&mut gate);
    let first = usize::try_from(gate.source_start).unwrap();
    gate.replace_authored_at_acknowledged_boundary(&receipt, &next, now)
        .expect("a completed all-zero MOT has no remaining association to reconcile");
    let (_, output, _) = cycle(&mut gate);
    assert_source_positions(&output, &source, first);
    assert!(Arc::ptr_eq(gate.warm_service.as_ref().unwrap(), &next));
    assert!(gate
        .warm_service
        .as_ref()
        .unwrap()
        .declared()
        .mots()
        .is_empty());
    assert!(gate.failure.is_none());
}
