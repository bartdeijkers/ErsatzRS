//! Complete authored inputs use both existing physical forwarding algorithms.

use std::{
    sync::{atomic::AtomicU8, Arc},
    time::Duration,
};

use ersatzrs_app::teletext_pages_v2::parse_declared_service;
use ersatzrs_domain::*;
use ersatzrs_ffmpeg::teletext::source_pes::SuppliedTeletextPes;
use ersatzrs_ffmpeg::teletext::*;
use serde_json::json;
use tokio::{sync::Notify, time::Instant};

use super::super::{
    retained_stream_buffer::{ReservePolicy, TERMINAL_SUCCESS},
    teletext_binding::{
        TeletextOutputAdmission, TeletextOutputBinding, TeletextOutputContent, TeletextOutputRole,
    },
    teletext_output::tests::{fixture, flatten, policy},
    teletext_output::{forward_teletext_output_with_content, TeletextPumpError},
    teletext_retained_output::{forward_retained_ts_with_teletext, RetainedTeletextError},
};

fn normalization_budget() -> TeletextNormalizedMainProgramBudget {
    let catalog = TeletextObjectCatalogBudget {
        max_objects: 0,
        max_commands: 0,
    };
    let collection = TeletextEnhancementCollectionBudget {
        max_commands: 0,
        max_drcs_references: 0,
    };
    TeletextNormalizedMainProgramBudget {
        composition: TeletextMainCompositionBudget {
            max_input_commands: 0,
            max_output_commands: 0,
            collection,
        },
        realization: TeletextBaseCompatibilityRealizationBudget {
            max_commands: 0,
            max_candidates: 0,
            max_origins: 0,
            max_seeds: 0,
            max_edges: 0,
            max_updates: 0,
        },
        placement: TeletextOriginCarrierPlacementBudget {
            max_commands: 0,
            max_origins: 0,
            max_edges: 0,
            catalog,
            spatial: TeletextSpatialTraceBudget {
                max_events: 0,
                max_invocations: 0,
            },
            max_work: 0,
        },
        assembly: TeletextAugmentedOriginProgramBudget {
            max_input_commands: 0,
            max_output_commands: 0,
            max_carriers: 0,
            max_identity_checks: 0,
            max_work: 0,
            recipe: TeletextOriginCarrierBudget {
                max_candidates: 0,
                max_origins: 0,
                catalog,
                max_id_checks: 0,
                max_output_commands: 0,
            },
        },
        packing: catalog,
        replay: collection,
        ancestry: TeletextAugmentedInvocationBudget {
            max_original_invocations: 0,
            max_actual_invocations: 0,
            max_carriers: 0,
            max_work: 0,
        },
        glyph_comparison: TeletextAugmentedGlyphComparisonBudget {
            max_records: 0,
            max_cells: 0,
            max_diagnostics: 0,
            max_work: 0,
        },
        display_comparison: TeletextPhysicalComparisonBudget {
            max_records: 0,
            max_cells: 0,
            max_diagnostics: 0,
            max_updates: 0,
        },
        max_resource_objects: 0,
    }
}

fn authored_service() -> EncodedTeletextAuthoredService {
    authored_service_with_counter(3)
}

pub(in crate::transport::iptv) fn authored_service_with_counter(
    counter: u8,
) -> EncodedTeletextAuthoredService {
    let owner = json!({"page":200,"subcode":16});
    let unused = json!({"target":{"kind":"unspecified","magazine":2},"validity":"level25_only","required_subpages":65535});
    let resource =
        |kind: &str, page: u8| json!({"kind":kind,"page":{"magazine":2,"page":page},"subpage":0});
    let input = json!({"schema_version":2,"generation":"synthetic-authored-output","language":"nld","initial_page":200,
        "pages":[{"address":owner,"kind":"full_page","language":"nld","national_option":0,"rows":[],"navigation":[]}],
        "page_enhancements":[{"address":owner,"presentation":null,"local_program":null,
            "compositional_links":{"gpop":unused,"pop":{"target":{"kind":"page","page":{"magazine":2,"page":160}},"validity":"level25_only","required_subpages":1},
                "gdrcs":unused,"drcs":{"target":{"kind":"page","page":{"magazine":2,"page":192}},"validity":"level25_only","required_subpages":1}},
            "resources":{"objects":{"public":{"magazine":2,"page":160},"global":null},
                "drcs":{"normal":{"magazine":2,"page":192},"global":null},"defaults":[null,null,null,null]}}],
        "magazines":[{"magazine":2,"presentation":{"primary":0,"secondary":null,
            "palette":{"clut2":vec![[1,2,3];8],"clut3":vec![[4,5,6];8]},"screen_colour":0,"row_colour":0,
            "black_background_substitution":false,"remapping":"fg_clut0_bg_clut0",
            "panels":{"left":false,"right":false,"status":"level25_and35","width":0}}}],
        "objects":[],"drcs":[{"identity":resource("drcs",192),"glyphs":[{"slot":0,"bitmap":[2048,1024,512,256,128,64,32,16,8,4]}]}],
        "mots":[],"mot_selections":[{"display":owner,"mot":null}],"empty_object_pages":[resource("pop",160)]});
    let declared = parse_declared_service(
        &serde_json::to_vec(&input).unwrap(),
        "synthetic-authored-output",
        TeletextServiceBudget {
            max_records: 8,
            max_objects: 0,
            max_structure_items: 256,
            max_base_content_bytes: 256,
            max_commands: 0,
            max_collected_commands: 0,
            max_collected_drcs_references: 0,
        },
        TeletextDeclaredServiceBudget {
            max_mots: 0,
            max_displays: 1,
            max_resources: 2,
            max_defaults: 0,
            max_objects: 0,
        },
    )
    .unwrap();
    let transmissions = declared
        .resource_inventory()
        .iter()
        .map(|&resource| TeletextServiceResourceTransmission {
            resource,
            counter: TeletextResourceUpdateCounter::new(counter).unwrap(),
            update: TeletextResourceFullUpdate::Changed,
            last_subpage: true,
        })
        .collect::<Vec<_>>();
    encode_teletext_authored_service(
        declared,
        &transmissions,
        &TeletextAuthoredServiceEncodingBudget {
            max_pages: 3,
            max_units: 256,
            max_commands: 0,
            max_display_updates: None,
            max_glyph_comparison_work: None,
            max_display_comparison_updates: None,
            normalization: normalization_budget(),
        },
    )
    .unwrap()
}

fn binding(config: TeletextOutputConfig) -> TeletextOutputBinding {
    TeletextOutputBinding::new_authored(config, authored_service(), Duration::from_millis(40))
        .unwrap()
}

fn expected_units(binding: &TeletextOutputBinding) -> Vec<[u8; 46]> {
    let service = binding.authored_service().unwrap();
    assert!(binding.pages().is_none());
    assert_eq!(service.pages().len(), 3);
    assert_eq!(service.magazine_units().len(), 1);
    assert_eq!(
        service.declared().semantic().drcs().subtables()[0].glyphs()[0]
            .bitmap
            .get(),
        [2048, 1024, 512, 256, 128, 64, 32, 16, 8, 4]
    );
    assert_eq!(service.inventory().len(), 3);
    service
        .magazine_units()
        .iter()
        .chain(
            service
                .pages()
                .iter()
                .flat_map(EncodedTeletextAuthoredPage::data_units),
        )
        .copied()
        .collect()
}

pub(in crate::transport::iptv) fn actual_units(bytes: &[u8]) -> Vec<[u8; 46]> {
    let mut pes = Vec::new();
    let mut units = Vec::new();
    for packet in bytes.chunks_exact(188) {
        let pid = u16::from_be_bytes([packet[1] & 0x1f, packet[2]]);
        if pid != 512 {
            continue;
        }
        let start = if packet[3] & 0x20 != 0 {
            5 + usize::from(packet[4])
        } else {
            4
        };
        if packet[1] & 0x40 != 0 {
            assert!(pes.is_empty());
        }
        pes.extend_from_slice(&packet[start..]);
        if pes.len() >= 6 {
            let end = 6 + usize::from(u16::from_be_bytes([pes[4], pes[5]]));
            if pes.len() >= end {
                let parsed = SuppliedTeletextPes::parse(&pes[..end]).unwrap();
                units.extend(parsed.units().filter_map(|u| u.teletext_unit().copied()));
                pes.clear();
            }
        }
    }
    assert!(pes.is_empty());
    units
}

fn assert_complete_delivery(bytes: &[u8], original: &[[u8; 188]], expected: &[[u8; 46]]) {
    assert_eq!(bytes.len(), original.len() * 188);
    for (before, after) in original.iter().zip(bytes.chunks_exact(188)) {
        if before[1] & 0x1f == 1 && before[2] == 44 {
            assert_eq!(before.as_slice(), after);
        }
    }
    let actual = actual_units(bytes);
    for unit in expected {
        assert!(
            actual.contains(unit),
            "lost complete authored unit {unit:?}"
        );
    }
}

#[test]
fn complete_services_remain_shared_and_only_final_mpeg_ts_roles_accept_them() {
    let binding = Arc::new(binding(policy()));
    let TeletextOutputContent::Authored(original) = binding.content() else {
        panic!("complete typed service required")
    };
    let TeletextOutputContent::Authored(cloned) = binding.content().clone() else {
        panic!("same complete service required")
    };
    assert!(Arc::ptr_eq(original, &cloned));
    for role in [
        TeletextOutputRole::FinalDirectMpegTs,
        TeletextOutputRole::FinalWrapperMpegTs,
        TeletextOutputRole::InternalItem,
        TeletextOutputRole::HlsWorker,
        TeletextOutputRole::OtherContainer,
        TeletextOutputRole::CapacitySlate,
    ] {
        let result = TeletextOutputAdmission::with_binding(role, Arc::clone(&binding));
        if matches!(
            role,
            TeletextOutputRole::FinalDirectMpegTs | TeletextOutputRole::FinalWrapperMpegTs
        ) {
            let admitted = result.unwrap();
            assert!(admitted
                .validate_format(ersatzrs_ffmpeg::pipeline::OutputFormat::MpegTs)
                .is_ok());
            assert!(admitted
                .validate_format(ersatzrs_ffmpeg::pipeline::OutputFormat::Hls)
                .is_err());
        } else {
            assert!(matches!(result, Err(TeletextPumpError::Configuration)));
        }
    }
}

#[tokio::test]
async fn paced_complete_service_keeps_actual_m29_drcs_pop_headers_and_source_clock() {
    let binding = binding(policy());
    let expected = expected_units(&binding);
    let original = fixture(15);
    let mut source = std::io::Cursor::new(flatten(&original));
    let mut sink = Vec::new();
    let count = forward_teletext_output_with_content(
        &mut source,
        &mut sink,
        binding.config(),
        binding.content().clone(),
        binding.first_byte_timeout(),
    )
    .await
    .unwrap();
    assert_eq!(count, u64::try_from(original.len() * 188).unwrap());
    assert_complete_delivery(&sink, &original, &expected);
    assert_eq!(expected_units(&binding), expected);
}

#[tokio::test]
async fn retained_complete_service_keeps_all_units_and_successful_eof_acknowledgement() {
    let reserve = ReservePolicy::new(5_000_000, Some(Duration::from_millis(300))).unwrap();
    let mut config = policy();
    config.max_buffer_packets = reserve.capacity_bytes().div_ceil(188);
    config.max_hold = Duration::from_secs(9);
    let binding = binding(config);
    let expected = expected_units(&binding);
    let original = fixture(15);
    let mut source = std::io::Cursor::new(flatten(&original));
    let mut sink = Vec::new();
    forward_retained_ts_with_teletext(
        &mut source,
        &mut sink,
        reserve,
        &AtomicU8::new(TERMINAL_SUCCESS),
        Instant::now() + Duration::from_secs(1),
        &Notify::new(),
        &binding,
    )
    .await
    .unwrap();
    assert_complete_delivery(&sink, &original, &expected);
}

#[tokio::test]
async fn authored_content_preserves_false_eof_and_capacity_rejections() {
    let reserve = ReservePolicy::new(5_000_000, Some(Duration::from_millis(300))).unwrap();
    let mut config = policy();
    config.max_buffer_packets = reserve.capacity_bytes().div_ceil(188);
    config.max_hold = Duration::from_secs(9);
    let binding = binding(config);
    let mut source = std::io::Cursor::new(flatten(&fixture(15)));
    let mut sink = Vec::new();
    assert!(matches!(
        forward_retained_ts_with_teletext(
            &mut source,
            &mut sink,
            reserve,
            &AtomicU8::new(0),
            Instant::now() + Duration::from_secs(1),
            &Notify::new(),
            &binding
        )
        .await,
        Err(RetainedTeletextError::Reserve(_))
    ));
    assert!(sink.is_empty());
    let mut invalid = policy();
    invalid.max_buffer_packets = 0;
    assert!(matches!(
        TeletextOutputBinding::new_authored(invalid, authored_service(), Duration::from_millis(40)),
        Err(TeletextPumpError::Gate(_))
    ));
    let mut source = tokio::io::empty();
    let mut sink = Vec::new();
    assert!(matches!(
        forward_teletext_output_with_content(
            &mut source,
            &mut sink,
            policy(),
            TeletextOutputContent::Authored(Arc::new(authored_service())),
            Duration::from_millis(40)
        )
        .await,
        Err(TeletextPumpError::EmptyOutput)
    ));
    assert!(sink.is_empty());
}

#[test]
fn caller_shared_complete_service_is_identical_in_multiple_bindings_and_content_clones() {
    let service = Arc::new(authored_service());
    let descriptor = TeletextServiceDescriptor::from_authored_service(&service);
    let first = TeletextOutputBinding::new_authored_shared(
        policy(),
        Arc::clone(&service),
        Duration::from_millis(40),
    )
    .unwrap();
    let second = TeletextOutputBinding::new_authored_shared(
        policy(),
        Arc::clone(&service),
        Duration::from_millis(50),
    )
    .unwrap();
    assert_eq!(Arc::strong_count(&service), 3);
    let expected = expected_units(&first);
    for binding in [&first, &second] {
        let TeletextOutputContent::Authored(shared) = binding.content() else {
            panic!("shared complete service required");
        };
        assert!(Arc::ptr_eq(&service, shared));
        assert!(std::ptr::eq(service.pages(), shared.pages()));
        assert!(std::ptr::eq(
            service.magazine_units(),
            shared.magazine_units()
        ));
        assert!(std::ptr::eq(service.inventory(), shared.inventory()));
        assert_eq!(shared.unit_count(), service.unit_count());
        assert_eq!(
            TeletextServiceDescriptor::from_authored_service(shared),
            descriptor
        );
        assert_eq!(expected_units(binding), expected);
        assert_eq!(binding.config().elementary_pid, policy().elementary_pid);
        assert!(binding.pages().is_none());
    }
    assert_eq!(first.first_byte_timeout(), Duration::from_millis(40));
    assert_eq!(second.first_byte_timeout(), Duration::from_millis(50));
    let TeletextOutputContent::Authored(cloned) = first.content().clone() else {
        panic!("cloned content must retain authored identity");
    };
    assert!(Arc::ptr_eq(&service, &cloned));
    assert_eq!(Arc::strong_count(&service), 4);
    drop((first, second, cloned));
    assert_eq!(Arc::strong_count(&service), 1);
}

#[test]
fn shared_authored_constructor_rejects_the_same_timing_and_gate_policies_without_retention() {
    let service = Arc::new(authored_service());
    for timeout in [Duration::ZERO, Duration::MAX] {
        assert!(matches!(
            TeletextOutputBinding::new_authored_shared(policy(), Arc::clone(&service), timeout),
            Err(TeletextPumpError::Configuration)
        ));
        assert_eq!(Arc::strong_count(&service), 1);
    }
    let mut overflow = policy();
    overflow.max_hold = Duration::MAX;
    assert!(matches!(
        TeletextOutputBinding::new_authored_shared(
            overflow,
            Arc::clone(&service),
            Duration::from_millis(40)
        ),
        Err(TeletextPumpError::Configuration)
    ));
    assert_eq!(Arc::strong_count(&service), 1);
    for kind in 0..3 {
        let mut invalid = policy();
        match kind {
            0 => invalid.max_buffer_packets = 0,
            1 => invalid.playout_delay = Duration::ZERO,
            _ => invalid.clock.muxrate_bps = 0,
        }
        assert!(matches!(
            TeletextOutputBinding::new_authored_shared(
                invalid,
                Arc::clone(&service),
                Duration::from_millis(40)
            ),
            Err(TeletextPumpError::Gate(_))
        ));
        assert_eq!(Arc::strong_count(&service), 1);
    }
    let valid = TeletextOutputBinding::new_authored_shared(
        policy(),
        Arc::clone(&service),
        Duration::from_millis(40),
    )
    .unwrap();
    assert_eq!(Arc::strong_count(&service), 2);
    assert!(!expected_units(&valid).is_empty());
}
