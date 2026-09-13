use super::*;

pub(in crate::addon_media_list_saved) fn fixture() -> SavedListDocument {
    let draft_item = SavedDraftItem {
        content: SavedItemContent {
            provider_id: "unpublished".to_owned(),
            display_title: "Draft item".to_owned(),
            title: "Draft item".to_owned(),
            included: false,
            ..SavedItemContent::default()
        },
        metadata: SavedMetadata {
            provider: serde_json::from_value(serde_json::json!({"title": "Provider title"}))
                .unwrap(),
            overrides: serde_json::from_value(serde_json::json!({"title": "Draft item"})).unwrap(),
            conflicts: BTreeMap::new(),
        },
        liveness_conflict: false,
        source_observation: None,
        replacement: None,
    };
    let published_file = SavedFile {
        relative_path: "streams/published.mkv".to_owned(),
        sha256: "a".repeat(64),
        byte_length: 1024,
    };
    SavedListDocument {
        version: SAVED_LIST_VERSION,
        addon_id: "org.example.media".to_owned(),
        provider_id: "shared-list".to_owned(),
        source_url: "https://example.invalid/list".to_owned(),
        draft: SavedListDraft {
            revision: 8,
            presentation: SavedListPresentation {
                name: "New draft name".to_owned(),
                ..SavedListPresentation::default()
            },
            metadata: SavedMetadata::default(),
            detail_import_policy: AddonMediaListDetailPolicy::ManualOnly,
            chapter_input: String::new(),
            refresh_cadence: AddonMediaListRefreshCadence::Daily,
            refresh_publication_policy: AddonMediaListRefreshPublicationPolicy::Review,
            disk_warning_gb: 25,
            delivery: PortableDeliveryPolicy {
                mode: ersatzrs_domain::AddonMediaListDeliveryMode::DownloadAhead,
                cutoff_at: None,
                before_cutoff: vec!["published".to_owned(), "unpublished".to_owned()],
                after_cutoff: Vec::new(),
            },
            options: SavedOptions::default(),
            items: vec![draft_item],
            files: Vec::new(),
        },
        published: Some(SavedListPublication {
            revision: 3,
            source_draft_revision: 6,
            completed_at: DateTime::parse_from_rfc3339("2026-09-12T12:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            presentation: SavedListPresentation {
                name: "Published name".to_owned(),
                ..SavedListPresentation::default()
            },
            metadata: Map::new(),
            options: SavedOptions::default(),
            items: vec![SavedPublishedItem {
                content: SavedItemContent {
                    provider_id: "published".to_owned(),
                    included: true,
                    ..SavedItemContent::default()
                },
                metadata: Map::new(),
                playback: SavedPlayback::DownloadAhead {
                    file: published_file.clone(),
                    duration_milliseconds: 12_500,
                },
                subtitles: vec![SavedSubtitle {
                    file: SavedFile {
                        relative_path: "streams/published.nl.srt".to_owned(),
                        sha256: "b".repeat(64),
                        byte_length: 20,
                    },
                    language: "nl".to_owned(),
                    timeline: MediaListSubtitleTimeline::Prepared,
                }],
            }],
            files: vec![published_file],
        }),
    }
}

#[test]
fn saved_list_model_round_trip_keeps_draft_and_publication_independent() {
    let original = fixture();
    let bytes = serde_json::to_vec(&original).unwrap();
    let restored: SavedListDocument = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(restored, original);
    assert_ne!(
        restored.draft.presentation.name,
        restored.published.as_ref().unwrap().presentation.name
    );
    assert!(!restored.draft.items[0].content.included);
    assert_eq!(
        restored.draft.items[0].metadata.provider["title"],
        "Provider title"
    );
    assert_eq!(
        restored.draft.items[0].metadata.overrides["title"],
        "Draft item"
    );
    assert!(matches!(
        restored.published.unwrap().items[0].playback,
        SavedPlayback::DownloadAhead { .. }
    ));
    let text = String::from_utf8(bytes).unwrap();
    for forbidden in [
        "media_item_id",
        "list_id",
        "artifact_id",
        "blob_id",
        "lease_owner",
        "context_fingerprint",
        "admin_hidden",
        "publication_journal",
    ] {
        assert!(
            !text.contains(forbidden),
            "unexpected local field {forbidden}"
        );
    }
}

#[test]
fn saved_list_model_rejects_injected_local_execution_fields() {
    let document = serde_json::to_value(fixture()).unwrap();
    let mut top_level = document.clone();
    top_level["publication_journal"] = serde_json::json!({"operation_id": 123});
    assert!(serde_json::from_value::<SavedListDocument>(top_level).is_err());
    let mut item = document.clone();
    item["draft"]["items"][0]["content"]["media_item_id"] = serde_json::json!(123);
    assert!(serde_json::from_value::<SavedListDocument>(item).is_err());
    let mut playback = document;
    playback["published"]["items"][0]["playback"]["artifact_id"] = serde_json::json!(123);
    assert!(serde_json::from_value::<SavedListDocument>(playback).is_err());
}

#[test]
fn saved_list_model_rebinds_timing_without_sender_context() {
    let sender = AddonMediaListPlaybackTimelinePlan::new(
        120_000,
        10_000..110_000,
        &[30_000..40_000, 80_000..85_000],
        "a".repeat(64),
    )
    .unwrap();
    let portable = SavedPlaybackTimeline::from(&sender);
    let json = serde_json::to_string(&portable).unwrap();
    assert!(!json.contains("context_fingerprint"));
    let decoded: SavedPlaybackTimeline = serde_json::from_str(&json).unwrap();
    let receiving = decoded.bind("b".repeat(64)).unwrap();
    assert_eq!(
        receiving.duration_milliseconds(),
        sender.duration_milliseconds()
    );
    assert_eq!(
        receiving.source_window_milliseconds(),
        sender.source_window_milliseconds()
    );
    assert_eq!(
        receiving.excluded_intervals_milliseconds(),
        sender.excluded_intervals_milliseconds()
    );
    assert_ne!(
        receiving.context_fingerprint(),
        sender.context_fingerprint()
    );
    assert!(decoded.bind("invalid".to_owned()).is_err());
    let invalid = SavedPlaybackTimeline {
        source_window_milliseconds: 110_000..130_000,
        ..decoded
    };
    assert!(invalid.bind("b".repeat(64)).is_err());
}
