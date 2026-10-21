//! Pure observation-boundary tests; these fixtures do not certify source bytes.

use super::*;

fn observation(
    provider: MediaSourceKind,
    stream: i64,
    index: i32,
) -> (SourceTeletextParents, SourceTeletextObservation) {
    let mut value = report(provider);
    let SourceTeletextBinding::Bound(binding) = &mut value.binding else {
        unreachable!()
    };
    binding.media_stream_id = stream;
    value.stream_index = index;
    // Descriptor requests remain discoverable without claiming page readiness.
    let headers = window(&mut value);
    headers.headers.clear();
    headers.complete_pes = 0;
    (
        SourceTeletextParents {
            media_version_id: 301,
            media_file_id: 501,
            media_stream_id: stream,
        },
        value,
    )
}

#[tokio::test]
async fn acquired_provider_descriptors_preserve_intent_without_stored_reports_or_readiness() {
    for provider in [
        MediaSourceKind::Local,
        MediaSourceKind::Jellyfin,
        MediaSourceKind::Emby,
        MediaSourceKind::Plex,
    ] {
        let fixture = Fixture::new(provider).await;
        let input = observation(provider, 401, 4);
        let unchanged = input.clone();
        let candidates = descriptor_candidates_from_acquired(std::slice::from_ref(&input)).unwrap();
        assert_eq!(input, unchanged);
        assert_eq!(candidates.len(), 3);
        assert_eq!(
            candidates
                .iter()
                .map(|candidate| (
                    candidate.request.page.get(),
                    candidate.language.as_str(),
                    candidate.descriptor_type,
                ))
                .collect::<Vec<_>>(),
            [(888, "eng", 2), (889, "nld", 5), (899, "eng", 2)]
        );
        let SourceTeletextBinding::Bound(binding) = &input.1.binding else {
            unreachable!()
        };
        for candidate in &candidates {
            assert_eq!(&candidate.request.binding, binding);
            assert_eq!(candidate.request.stream_index, 4);
            assert_eq!(candidate.request.pid, 258);
            assert_eq!(candidate.request.program_number, 1);
            assert_eq!(candidate.psi_fingerprint, [7; 32]);
        }
        assert_eq!(
            candidates[0]
                .descriptor_occurrences
                .iter()
                .map(|entry| entry.descriptor_tag)
                .collect::<Vec<_>>(),
            [0x56, 0x46]
        );
        assert!(fixture.requests().await.unwrap().is_empty());
        let stored: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "SourceTeletextMetadata""#)
            .fetch_one(&fixture.pool)
            .await
            .unwrap();
        assert_eq!(stored, 0);
    }
}

#[test]
fn acquired_descriptors_refuse_mismatched_parents_sources_and_malformed_batches() {
    let first = observation(MediaSourceKind::Jellyfin, 401, 4);
    let second = observation(MediaSourceKind::Jellyfin, 402, 5);
    assert_eq!(
        descriptor_candidates_from_acquired(&[first.clone(), second.clone()])
            .unwrap()
            .len(),
        6
    );
    for field in 0..12 {
        let mut invalid = second.clone();
        let SourceTeletextBinding::Bound(binding) = &mut invalid.1.binding else {
            unreachable!()
        };
        match field {
            0 => invalid.0.media_stream_id += 1,
            1 => invalid.0.media_file_id += 1,
            2 => invalid.0.media_version_id += 1,
            3 => binding.media_source_id += 1,
            4 => binding.provider = MediaSourceKind::Emby,
            5 => {
                binding.source_revision =
                    SourceTeletextRevision::new("other-revision".into()).unwrap();
            }
            6 => {
                invalid.0.media_stream_id = first.0.media_stream_id;
                binding.media_stream_id = invalid.0.media_stream_id;
            }
            7 => invalid.1.stream_index = first.1.stream_index,
            8 => invalid.1.stream_index = -1,
            9 => invalid.1.provenance = SourceTeletextProvenance::Unavailable,
            10 => {
                invalid.0.media_file_id += 1;
                binding.media_file_id = invalid.0.media_file_id;
            }
            11 => {
                invalid.0.media_version_id += 1;
                binding.media_version_id = invalid.0.media_version_id;
            }
            _ => unreachable!(),
        }
        assert!(
            descriptor_candidates_from_acquired(&[first.clone(), invalid]).is_err(),
            "field {field}"
        );
    }
    let mut malformed = second.clone();
    window(&mut malformed.1).programs[0].descriptors = vec![0x56, 5, b'e'];
    assert!(descriptor_candidates_from_acquired(&[first, malformed]).is_err());

    let mut unbound = second.clone();
    unbound.1.binding = SourceTeletextBinding::Unavailable;
    unbound.1.observed_pages = SourceTeletextObservedPages::NotAcquired;
    assert!(descriptor_candidates_from_acquired(&[unbound])
        .unwrap()
        .is_empty());
    let mut no_psi = second.clone();
    no_psi.1.observed_pages = SourceTeletextObservedPages::NotAcquired;
    assert!(descriptor_candidates_from_acquired(&[no_psi])
        .unwrap()
        .is_empty());
    let mut no_caption = second.clone();
    window(&mut no_caption.1).programs[0].descriptors = descriptors(0x56, &[("eng", 1, 2, 0)]);
    assert!(descriptor_candidates_from_acquired(&[no_caption])
        .unwrap()
        .is_empty());
    let mut wrong_membership = second;
    window(&mut wrong_membership.1).programs[0].stream_type = 5;
    assert!(descriptor_candidates_from_acquired(&[wrong_membership])
        .unwrap()
        .is_empty());
}

#[test]
fn acquired_descriptors_keep_aggregate_stream_and_wire_limits_across_observations() {
    let mut streams: Vec<_> = (0..MAX_TELETEXT_PROBE_STREAMS)
        .map(|ordinal| {
            let mut value = observation(
                MediaSourceKind::Plex,
                401 + i64::try_from(ordinal).unwrap(),
                i32::try_from(ordinal).unwrap(),
            );
            window(&mut value.1).programs[0].descriptors =
                descriptors(0x56, &[("eng", 2, 0, 0x88); 4]);
            value
        })
        .collect();
    let candidates = descriptor_candidates_from_acquired(&streams).unwrap();
    assert_eq!(candidates.len(), MAX_TELETEXT_PROBE_STREAMS);
    assert_eq!(
        candidates
            .iter()
            .map(|candidate| candidate.descriptor_occurrences.len())
            .sum::<usize>(),
        MAX_CAPTION_CANDIDATES
    );
    streams.push(observation(MediaSourceKind::Plex, 1000, 1000));
    assert!(descriptor_candidates_from_acquired(&streams).is_err());
    streams.pop();
    window(&mut streams[0].1).programs[0].descriptors =
        descriptors(0x56, &[("eng", 2, 0, 0x88); 5]);
    assert!(descriptor_candidates_from_acquired(&streams).is_err());
}
