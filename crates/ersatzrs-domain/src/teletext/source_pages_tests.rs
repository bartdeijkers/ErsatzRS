use super::*;
use crate::{
    MediaSourceKind, SourceTeletextBinding, SourceTeletextBindingIdentity,
    SourceTeletextDescriptors, SourceTeletextHeaderProgram, SourceTeletextObservation,
    SourceTeletextObservedPages, SourceTeletextPid, SourceTeletextPrograms,
    SourceTeletextProvenance, SourceTeletextRevision,
};

fn unit(packet: u8, ordinal: u32, magazine: u8) -> SourceTeletextCapturedUnit {
    let mut data = vec![0x80; 46];
    data[..4].copy_from_slice(&[2, 44, 0xc7, 0xe4]);
    SourceTeletextCapturedUnit {
        wire_magazine: magazine,
        packet_number: packet,
        data,
        completion_packet: 5,
        pes_ordinal: 0,
        unit_ordinal: ordinal,
        pts: Some(123),
    }
}

fn header(ordinal: u32, magazine: u8) -> SourceTeletextObservedHeader {
    SourceTeletextObservedHeader {
        wire_magazine: magazine,
        wire_page: 0x23,
        subcode: 0x0320,
        controls: 1,
        subtitle_data_unit: false,
        field_parity: false,
        line_offset: 7,
        corrected_address_bytes: 0,
        corrected_header_bytes: 0,
        completion_packet: 5,
        pes_ordinal: 0,
        unit_ordinal: ordinal,
        pts: Some(123),
    }
}

fn fixture() -> (SourceTeletextHeaderWindow, SourceTeletextPageWindow) {
    let headers = SourceTeletextHeaderWindow {
        transport_stream_id: 1,
        pat_version: 0,
        psi_fingerprint: [7; 32],
        packet_count: 10,
        complete_pes: 1,
        unobserved_prefix: false,
        ended_at_eof: true,
        programs: vec![SourceTeletextHeaderProgram {
            program_number: 1,
            pmt_pid: 0x20,
            pcr_pid: 0x100,
            pmt_version: 0,
            stream_type: 6,
            descriptors: vec![],
        }],
        headers: vec![header(1, 1), header(3, 1)],
    };
    let pages = SourceTeletextPageWindow {
        units: vec![unit(29, 0, 1), unit(0, 1, 1), unit(1, 2, 1), unit(0, 3, 1)],
        pages: vec![
            SourceTeletextCapturedPage {
                header_ordinal: 0,
                header_unit: 1,
                body_units: vec![2],
                closing_unit: Some(3),
                context: SourceTeletextPageContext {
                    page_function: Some(0),
                    page_coding: Some(0),
                    primary: Some(0),
                    secondary: Some(127),
                    charset_origin: Some(SourceTeletextCharsetOrigin::Magazine29_0),
                    context_unit: Some(0),
                    protection_error: false,
                },
            },
            SourceTeletextCapturedPage {
                header_ordinal: 1,
                header_unit: 3,
                body_units: vec![],
                closing_unit: None,
                context: SourceTeletextPageContext::default(),
            },
        ],
        unassociated_body_units: 0,
        capacity_exhausted: false,
        transmission_mode_conflict: false,
    };
    (headers, pages)
}

#[test]
fn source_page_capture_roundtrips_repeated_identity_without_ready_authority() {
    let (headers, pages) = fixture();
    pages.validate(&headers).unwrap();
    assert_eq!(headers.headers[0].wire_page, headers.headers[1].wire_page);
    assert_eq!(headers.headers[0].subcode, headers.headers[1].subcode);
    assert_eq!(
        pages.readiness(&headers).unwrap(),
        [
            SourceTeletextPageReadiness::ClosedBaseData,
            SourceTeletextPageReadiness::Open,
        ]
    );
    let restored: SourceTeletextPageWindow =
        serde_json::from_str(&serde_json::to_string(&pages).unwrap()).unwrap();
    assert_eq!(restored, pages);
    restored.validate(&headers).unwrap();
    assert!(!format!("{:?}", pages.units[2]).contains("128, 128"));
    assert!(format!("{:?}", pages.units[2]).contains("<redacted>"));
}

#[test]
fn source_page_capture_rejects_corrupt_envelopes_order_bounds_and_references() {
    let invalid: &[fn(&mut SourceTeletextPageWindow)] = &[
        |v| {
            v.units[0].data.pop();
        },
        |v| v.units[0].data[0] = 1,
        |v| v.units[0].data[1] = 43,
        |v| v.units[0].data[2] = 7,
        |v| v.units[0].data[2] = 0xc1,
        |v| v.units[0].data[3] = 0,
        |v| v.units[0].wire_magazine = 8,
        |v| v.units[0].packet_number = 32,
        |v| v.units[0].completion_packet = 10,
        |v| v.units[0].pes_ordinal = 1,
        |v| v.units[0].unit_ordinal = 32_727,
        |v| v.units[0].pts = Some(1_u64 << 33),
        |v| v.units[1].unit_ordinal = 0,
        |v| v.units[1].completion_packet = 6,
        |v| v.units[1].pts = None,
        |v| v.units[1].data[0] = 3,
        |v| v.units[1].data[2] = 0xe7,
        |v| v.pages[0].header_unit = 0,
        |v| v.pages[1].header_ordinal = 0,
        |v| v.pages[0].body_units = vec![1],
        |v| v.pages[0].body_units = vec![2, 2],
        |v| v.pages[0].body_units.clear(),
        |v| v.pages[0].body_units = vec![4],
        |v| v.pages[0].closing_unit = None,
        |v| v.pages[0].closing_unit = Some(2),
        |v| v.pages[0].context.context_unit = Some(3),
        |v| v.pages[0].context.charset_origin = Some(SourceTeletextCharsetOrigin::Page28_0),
        |v| v.pages[0].context.primary = Some(128),
        |v| v.pages[0].context.secondary = Some(128),
        |v| v.pages[0].context.page_function = Some(16),
        |v| v.pages[0].context.page_coding = Some(8),
        |v| v.unassociated_body_units = 1,
        |v| {
            let _ = v.pages.pop();
        },
        |v| v.units.resize(4_097, v.units[0].clone()),
        |v| v.pages.resize(1_025, v.pages[0].clone()),
    ];
    for mutate in invalid {
        let (headers, mut pages) = fixture();
        mutate(&mut pages);
        assert_eq!(
            pages.validate(&headers),
            Err(SourceTeletextError::PageWindow)
        );
    }
}

#[test]
fn source_page_capture_retains_unknown_reserved_and_damaged_context() {
    let (headers, mut pages) = fixture();
    pages.pages[0].context.primary = Some(127);
    pages.validate(&headers).unwrap();
    assert_eq!(
        pages.readiness(&headers).unwrap()[0],
        SourceTeletextPageReadiness::ContextUnknown
    );
    pages.pages[0].context = SourceTeletextPageContext::default();
    pages.validate(&headers).unwrap();
    assert_eq!(
        pages.readiness(&headers).unwrap()[0],
        SourceTeletextPageReadiness::ContextUnknown
    );
    pages.pages[0].context.protection_error = true;
    pages.pages[0].context.context_unit = Some(0);
    pages.validate(&headers).unwrap();
    assert_eq!(
        pages.readiness(&headers).unwrap()[0],
        SourceTeletextPageReadiness::ProtectionError
    );
    pages.pages[0].context.primary = Some(0);
    assert_eq!(
        pages.validate(&headers),
        Err(SourceTeletextError::PageWindow)
    );
}

#[test]
fn source_page_capture_keeps_parity_prior_state_and_non_lop_facts_separate() {
    let (mut headers, mut pages) = fixture();
    headers.headers[0].controls = 0;
    assert_eq!(
        pages.readiness(&headers).unwrap()[0],
        SourceTeletextPageReadiness::PriorState
    );
    headers.headers[0].controls = 1;
    pages.units[2].data[6] = 0;
    assert_eq!(
        pages.readiness(&headers).unwrap()[0],
        SourceTeletextPageReadiness::ProtectionError
    );
    pages.units[2].data[6] = 0x80;
    pages.units[1].data[14] = 0;
    assert_eq!(
        pages.readiness(&headers).unwrap()[0],
        SourceTeletextPageReadiness::ProtectionError
    );
    pages.units[1].data[14] = 0x80;
    pages.pages[0].context.page_function = Some(15);
    pages.pages[0].context.page_coding = Some(7);
    pages.units[1].data[14] = 0;
    pages.units[2].data[6] = 0;
    assert_eq!(
        pages.readiness(&headers).unwrap()[0],
        SourceTeletextPageReadiness::NeedsEnhancementContext
    );
    pages.pages[0].context = SourceTeletextPageContext::default();
    assert_eq!(
        pages.readiness(&headers).unwrap()[0],
        SourceTeletextPageReadiness::ContextUnknown
    );
}

#[test]
fn source_page_capture_accepts_both_exact_resource_bounds() {
    let (mut headers, mut pages) = fixture();
    headers.headers.clear();
    pages.units.clear();
    pages.pages.clear();
    for ordinal in 0..MAX_SOURCE_TELETEXT_CAPTURED_PAGES {
        let opening = u32::try_from(pages.units.len()).unwrap();
        headers.headers.push(header(opening, 1));
        pages.units.push(unit(0, opening, 1));
        pages.units.push(unit(1, opening + 1, 1));
        pages.pages.push(SourceTeletextCapturedPage {
            header_ordinal: u32::try_from(ordinal).unwrap(),
            header_unit: opening,
            body_units: vec![opening + 1],
            closing_unit: (ordinal + 1 < MAX_SOURCE_TELETEXT_CAPTURED_PAGES).then_some(opening + 2),
            context: SourceTeletextPageContext::default(),
        });
    }
    while pages.units.len() < MAX_SOURCE_TELETEXT_CAPTURED_UNITS {
        pages
            .units
            .push(unit(30, u32::try_from(pages.units.len()).unwrap(), 1));
    }
    pages.validate(&headers).unwrap();
    assert_eq!(pages.pages.len(), 1_024);
    assert_eq!(pages.units.len(), 4_096);
}

#[test]
fn source_page_capture_accepts_page_context_without_treating_enhancement_as_base() {
    let (headers, mut pages) = fixture();
    pages.units[2].packet_number = 28;
    pages.pages[0].context.charset_origin = Some(SourceTeletextCharsetOrigin::Page28_4);
    pages.pages[0].context.context_unit = Some(2);
    pages.validate(&headers).unwrap();
    assert_eq!(
        pages.readiness(&headers).unwrap()[0],
        SourceTeletextPageReadiness::NeedsEnhancementContext
    );
    pages.pages[0].context = SourceTeletextPageContext {
        context_unit: Some(2),
        protection_error: true,
        ..Default::default()
    };
    pages.validate(&headers).unwrap();
    assert_eq!(
        pages.readiness(&headers).unwrap()[0],
        SourceTeletextPageReadiness::ProtectionError
    );
}

#[test]
fn source_page_capture_closes_serial_on_other_magazine_and_parallel_on_own() {
    let (mut headers, mut pages) = fixture();
    headers.headers[1].wire_magazine = 2;
    pages.units[3].wire_magazine = 2;
    pages.pages[0].closing_unit = None;
    pages.validate(&headers).unwrap();
    headers.headers[0].controls |= 0x80;
    pages.transmission_mode_conflict = true;
    pages.pages[0].closing_unit = Some(3);
    pages.validate(&headers).unwrap();
    assert_eq!(
        pages.readiness(&headers).unwrap()[0],
        SourceTeletextPageReadiness::PriorState
    );
    pages.transmission_mode_conflict = false;
    assert_eq!(
        pages.validate(&headers),
        Err(SourceTeletextError::PageWindow)
    );
}

#[test]
fn source_page_capture_keeps_orphan_body_and_capacity_exhaustion_explicit() {
    let (headers, mut pages) = fixture();
    pages.units[0].packet_number = 2;
    pages.pages[0].context = SourceTeletextPageContext::default();
    pages.unassociated_body_units = 1;
    pages.validate(&headers).unwrap();
    pages.units.clear();
    pages.pages.clear();
    pages.unassociated_body_units = 0;
    pages.capacity_exhausted = true;
    pages.validate(&headers).unwrap();
    assert!(pages.readiness(&headers).unwrap().is_empty());
}

#[test]
fn source_page_capture_allows_terminator_closure_without_inventing_header_occurrence() {
    let (mut headers, mut pages) = fixture();
    let _ = headers.headers.pop();
    let _ = pages.pages.pop();
    pages.validate(&headers).unwrap();
    assert_eq!(pages.pages[0].closing_unit, Some(3));
}

#[test]
fn source_page_observation_requires_bound_parent_and_preserves_old_variants() {
    let (headers, pages) = fixture();
    let mut observation = SourceTeletextObservation::new(
        SourceTeletextBinding::Unavailable,
        2,
        SourceTeletextProvenance::Ffprobe9PackedDescriptorProjection,
        SourceTeletextPid::Reported(0x100),
        SourceTeletextPrograms::Unavailable,
        SourceTeletextDescriptors::Unavailable,
    )
    .unwrap();
    assert_eq!(
        serde_json::to_string(&observation.observed_pages).unwrap(),
        "\"NotAcquired\""
    );
    observation.observed_pages = SourceTeletextObservedPages::PagesObserved {
        headers: Box::new(headers.clone()),
        pages: Box::new(pages),
    };
    assert_eq!(observation.observed_pages.headers(), Some(&headers));
    assert!(observation.observed_pages.page_window().is_some());
    assert_eq!(
        observation.validate(),
        Err(SourceTeletextError::HeaderBinding)
    );
    observation.binding = SourceTeletextBinding::Bound(
        SourceTeletextBindingIdentity::new(
            1,
            MediaSourceKind::Local,
            2,
            3,
            4,
            SourceTeletextRevision::new("source-pages-test".into()).unwrap(),
        )
        .unwrap(),
    );
    observation.validate().unwrap();
    let restored: SourceTeletextObservation =
        serde_json::from_str(&serde_json::to_string(&observation).unwrap()).unwrap();
    assert_eq!(restored, observation);
    restored.validate().unwrap();
    observation.observed_pages = SourceTeletextObservedPages::HeadersObserved(Box::new(headers));
    assert!(serde_json::to_string(&observation.observed_pages)
        .unwrap()
        .starts_with("{\"HeadersObserved\":"));
    observation.validate().unwrap();
}
