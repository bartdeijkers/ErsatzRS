use super::*;

fn descriptor(ordinal: u32) -> SourceTeletextDescriptor {
    SourceTeletextDescriptor::new(
        ordinal,
        TeletextLanguage::new("eng".into()).unwrap(),
        1,
        2,
        0,
    )
    .unwrap()
}

fn observation(
    descriptors: SourceTeletextDescriptors,
) -> Result<SourceTeletextObservation, SourceTeletextError> {
    SourceTeletextObservation::new(
        SourceTeletextBinding::Unavailable,
        2,
        SourceTeletextProvenance::Ffprobe9PackedDescriptorProjection,
        SourceTeletextPid::Unavailable,
        SourceTeletextPrograms::Unavailable,
        descriptors,
    )
}

fn binding(provider: MediaSourceKind) -> SourceTeletextBindingIdentity {
    SourceTeletextBindingIdentity::new(
        1,
        provider,
        2,
        3,
        4,
        SourceTeletextRevision::new("provider-revision".into()).unwrap(),
    )
    .unwrap()
}

#[test]
fn revision_checks_utf8_bytes_and_retains_exact_opaque_text() {
    assert_eq!(
        SourceTeletextRevision::new(String::new()),
        Err(SourceTeletextError::Revision)
    );
    let exact = "é".repeat(MAX_SOURCE_TELETEXT_REVISION_BYTES / 2);
    let revision = SourceTeletextRevision::new(exact.clone()).unwrap();
    assert_eq!(revision.as_str(), exact);
    assert_eq!(
        SourceTeletextRevision::new(format!("{exact}a")),
        Err(SourceTeletextError::Revision)
    );
    assert!(!format!("{revision:?}").contains(&exact));
    assert!(serde_json::from_str::<SourceTeletextRevision>("\"\"").is_err());
}

#[test]
fn binding_requires_durable_ids_for_each_provider_without_inferring_revision() {
    for provider in MediaSourceKind::ALL {
        let identity = binding(provider);
        assert_eq!(identity.provider, provider);
        assert_eq!(identity.source_revision.as_str(), "provider-revision");
        for position in 0..4 {
            let mut ids = [1, 2, 3, 4];
            ids[position] = 0;
            assert_eq!(
                SourceTeletextBindingIdentity::new(
                    ids[0],
                    provider,
                    ids[1],
                    ids[2],
                    ids[3],
                    identity.source_revision.clone()
                ),
                Err(SourceTeletextError::ParentIdentity)
            );
            ids[position] = -1;
            assert_eq!(
                SourceTeletextBindingIdentity::new(
                    ids[0],
                    provider,
                    ids[1],
                    ids[2],
                    ids[3],
                    identity.source_revision.clone()
                ),
                Err(SourceTeletextError::ParentIdentity)
            );
        }
    }
}

#[test]
fn descriptor_preserves_reserved_types_and_nondecimal_wire_pages() {
    let raw =
        SourceTeletextDescriptor::new(9, TeletextLanguage::new("nld".into()).unwrap(), 31, 0, 0xfa)
            .unwrap();
    assert_eq!(raw.descriptor_type, 31);
    assert_eq!(raw.wire_magazine, 0);
    assert_eq!(raw.wire_page, 0xfa);
    assert!(raw.display_page().is_none());
    assert_eq!(
        SourceTeletextDescriptor::new(0, raw.language.clone(), 32, 0, 0),
        Err(SourceTeletextError::DescriptorType)
    );
    assert_eq!(
        SourceTeletextDescriptor::new(0, raw.language, 1, 8, 0),
        Err(SourceTeletextError::Magazine)
    );
    let mut eighth = descriptor(0);
    eighth.wire_magazine = 0;
    eighth.wire_page = 0x88;
    assert_eq!(eighth.display_page().unwrap().get(), 888);
}

#[test]
fn repeated_reported_pages_keep_ordinals_but_duplicate_ordinals_are_rejected() {
    let entries = vec![descriptor(4), descriptor(9)];
    let projected = observation(SourceTeletextDescriptors::Reported {
        entries: entries.clone(),
        trailing_unpaired_pairs: Some(3),
    })
    .unwrap();
    assert_eq!(
        projected.descriptors,
        SourceTeletextDescriptors::Reported {
            entries,
            trailing_unpaired_pairs: Some(3),
        }
    );
    assert_eq!(
        observation(SourceTeletextDescriptors::Reported {
            entries: vec![descriptor(4), descriptor(4)],
            trailing_unpaired_pairs: None
        }),
        Err(SourceTeletextError::DuplicateOrdinal)
    );
    assert_eq!(
        observation(SourceTeletextDescriptors::Reported {
            entries: vec![],
            trailing_unpaired_pairs: Some(0)
        }),
        Err(SourceTeletextError::EmptyShrinkWarning)
    );
}

#[test]
fn independent_states_and_all_reported_memberships_survive_roundtrip() {
    let mut projected = observation(SourceTeletextDescriptors::Unusable).unwrap();
    projected.provenance = SourceTeletextProvenance::Unavailable;
    projected.pid = SourceTeletextPid::Unusable;
    projected.programs = SourceTeletextPrograms::Reported(vec![
        SourceTeletextProgram {
            program_id: 1,
            program_number: 1,
            pmt_pid: 4096,
            pcr_pid: 256,
        },
        SourceTeletextProgram {
            program_id: 2,
            program_number: 2,
            pmt_pid: 4096,
            pcr_pid: 0x1fff,
        },
    ]);
    projected.validate().unwrap();
    let decoded: SourceTeletextObservation =
        serde_json::from_slice(&serde_json::to_vec(&projected).unwrap()).unwrap();
    assert_eq!(decoded, projected);
    assert_eq!(
        decoded.observed_pages,
        SourceTeletextObservedPages::NotAcquired
    );
}

#[test]
fn bound_decimal_descriptor_still_has_no_actually_observed_pages() {
    let mut projected = observation(SourceTeletextDescriptors::Reported {
        entries: vec![descriptor(0)],
        trailing_unpaired_pairs: None,
    })
    .unwrap();
    projected.binding = SourceTeletextBinding::Bound(binding(MediaSourceKind::Local));
    projected.validate().unwrap();
    assert_eq!(descriptor(0).display_page().unwrap().get(), 200);
    assert_eq!(
        projected.observed_pages,
        SourceTeletextObservedPages::NotAcquired
    );
}

#[test]
fn validation_catches_mutated_or_deserialized_identity_and_bit_widths() {
    let mut projected = observation(SourceTeletextDescriptors::Unavailable).unwrap();
    projected.stream_index = -1;
    assert_eq!(projected.validate(), Err(SourceTeletextError::StreamIndex));
    projected.stream_index = 0;
    projected.pid = SourceTeletextPid::Reported(0x2000);
    assert_eq!(projected.validate(), Err(SourceTeletextError::Pid));
    projected.pid = SourceTeletextPid::Reported(0x102);
    projected.programs = SourceTeletextPrograms::Reported(vec![SourceTeletextProgram {
        program_id: 1,
        program_number: 1,
        pmt_pid: 0x2000,
        pcr_pid: 256,
    }]);
    assert_eq!(projected.validate(), Err(SourceTeletextError::Pid));
    projected.programs = SourceTeletextPrograms::Unavailable;
    let mut identity = binding(MediaSourceKind::Local);
    identity.media_file_id = 0;
    projected.binding = SourceTeletextBinding::Bound(identity);
    assert_eq!(
        projected.validate(),
        Err(SourceTeletextError::ParentIdentity)
    );
}
