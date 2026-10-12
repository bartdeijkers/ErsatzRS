//! Absolute deadline checkpoints preserve complete service identity and atomicity.

use super::*;
use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};

fn assert_same_service(
    expected: &EncodedTeletextAuthoredService,
    actual: &EncodedTeletextAuthoredService,
) {
    assert_eq!(actual.declared(), expected.declared());
    assert_eq!(actual.inventory(), expected.inventory());
    assert_eq!(actual.unit_count(), expected.unit_count());
    assert_eq!(actual.magazine_units(), expected.magazine_units());
    assert_eq!(actual.pages().len(), expected.pages().len());
    for (actual, expected) in actual.pages().iter().zip(expected.pages()) {
        assert_eq!(actual.data_units(), expected.data_units());
        assert_eq!(actual.resource_header(), expected.resource_header());
        assert_eq!(actual.inventory_record(), expected.inventory_record());
    }
}

#[test]
fn expired_original_deadline_precedes_lifecycle_or_resource_work() {
    let service = declared(input());
    let original = service.clone();
    // Intentionally omit required lifecycle records; expiry must stop earlier.
    let error =
        encode_teletext_authored_service_before(service.clone(), &[], &limits(), Instant::now())
            .unwrap_err();
    assert!(matches!(error.reason, Failure::TimedOut));
    assert!(error.display.is_none());
    assert!(error.resource.is_none());
    assert_eq!(service, original);
}

#[test]
fn future_deadline_preserves_native_compiled_and_resource_units_and_metadata() {
    let service = declared(input());
    let tx = transmissions(&service);
    let expected = encode_teletext_authored_service(service.clone(), &tx, &limits()).unwrap();
    let actual = encode_teletext_authored_service_before(
        service,
        &tx,
        &limits(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    assert_same_service(&expected, &actual);
    assert!(actual
        .pages()
        .iter()
        .any(|page| matches!(page, EncodedTeletextAuthoredPage::NativeDisplay(_))));
    assert!(actual
        .pages()
        .iter()
        .any(|page| matches!(page, EncodedTeletextAuthoredPage::CompiledDisplay(_))));
    assert!(actual
        .pages()
        .iter()
        .any(|page| page.resource_header().is_some()));
}

#[test]
fn every_service_checkpoint_honours_inclusive_absolute_expiry_atomically() {
    let service = declared(input());
    let original = service.clone();
    let tx = transmissions(&service);
    let before = Instant::now();
    // Real inner stages remain before this future deadline; the injected
    // outer clock independently moves to that exact deadline at each boundary.
    let deadline = before + Duration::from_hours(1);
    let mut observed = 0;
    encode_teletext_authored_service_with_clock(
        service.clone(),
        &tx,
        &limits(),
        Some(deadline),
        &mut || {
            observed += 1;
            before
        },
    )
    .unwrap();
    assert!(observed > service.resource_inventory().len() + service.semantic().base().pages.len());
    let mut resources = BTreeSet::new();
    let mut displays = BTreeSet::new();
    for expiry_at in 0..observed {
        let mut reads = 0;
        let error = encode_teletext_authored_service_with_clock(
            service.clone(),
            &tx,
            &limits(),
            Some(deadline),
            &mut || {
                let instant = if reads < expiry_at { before } else { deadline };
                reads += 1;
                instant
            },
        )
        .unwrap_err();
        assert!(matches!(error.reason, Failure::TimedOut));
        // No renewed deadline or later checkpoint may resume this compilation.
        assert_eq!(reads, expiry_at + 1);
        if let Some(resource) = error.resource {
            assert!(service.resource_inventory().contains(&resource));
            resources.insert(resource);
        }
        if let Some(display) = error.display {
            assert!(service
                .semantic()
                .base()
                .pages
                .iter()
                .any(|page| page.address == display));
            displays.insert(display);
        }
        assert_eq!(service, original);
    }
    assert_eq!(resources.len(), service.resource_inventory().len());
    assert_eq!(displays.len(), service.semantic().base().pages.len());
}

#[test]
fn untimed_compatibility_api_never_reads_the_new_clock() {
    let service = declared(input());
    let tx = transmissions(&service);
    let expected = encode_teletext_authored_service(service.clone(), &tx, &limits()).unwrap();
    let actual =
        encode_teletext_authored_service_with_clock(service, &tx, &limits(), None, &mut || {
            panic!("untimed compatibility must not acquire a deadline")
        })
        .unwrap();
    assert_same_service(&expected, &actual);
}
