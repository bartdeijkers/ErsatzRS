//! Error fallback retains actual packet coordinates without inventing authority.
use super::*;

fn scalar() -> super::super::NativeTeletextEditorialTtiError {
    super::super::word_error(
        Location {
            page: 0x2c0,
            subcode: 0,
            resource: Some(
                ersatzrs_domain::TeletextResourceIdentity::new(
                    ersatzrs_domain::TeletextResourceKind::Pop,
                    ersatzrs_domain::TeletextRawDataPageNumber::new(2, 0xc0).unwrap(),
                    ersatzrs_domain::TeletextResourceSubpageIndex::ZERO,
                )
                .unwrap(),
            ),
        },
        20,
    )
}

fn failure(kind: Failure) -> super::super::NativeTeletextEditorialTtiError {
    let source = scalar();
    Location {
        page: source.page,
        subcode: source.subcode,
        resource: source.resource,
    }
    .error(kind)
}

#[test]
fn generic_structure_refusal_retains_the_original_scalar_word_without_reclassification() {
    let original = scalar();
    let located = retain_scalar_position(failure(Failure::Structure), &original);
    assert_eq!(located.failure, Failure::Structure);
    assert_eq!(
        (located.resource, located.page, located.subcode),
        (original.resource, original.page, original.subcode)
    );
    assert_eq!(
        (located.row, located.designation, located.column),
        (Some(26), Some(1), Some(7))
    );
    assert!(located.cause.is_none());
}

#[test]
fn scalar_location_never_replaces_deadlines_other_errors_or_existing_locations() {
    for kind in [
        Failure::TimedOut,
        Failure::Mapping,
        Failure::Identity,
        Failure::Syntax,
        Failure::HeaderIdentity,
        Failure::Status,
        Failure::Region,
        Failure::Navigation,
        Failure::Description,
        Failure::Cycle,
        Failure::Packet,
        Failure::ProtectedRow,
    ] {
        let original = failure(kind);
        assert_eq!(
            retain_scalar_position(original.clone(), &scalar()),
            original
        );
    }
    let mut located = failure(Failure::Structure);
    located.row = Some(28);
    located.column = Some(5);
    assert_eq!(retain_scalar_position(located.clone(), &scalar()), located);
    let mut other = failure(Failure::Structure);
    other.resource = None;
    assert_eq!(retain_scalar_position(other.clone(), &scalar()), other);
    other = failure(Failure::Structure);
    other.subcode = 1;
    assert_eq!(retain_scalar_position(other.clone(), &scalar()), other);
    let no_packet = failure(Failure::Structure);
    assert_eq!(
        retain_scalar_position(no_packet.clone(), &failure(Failure::TimedOut)),
        no_packet
    );
}
