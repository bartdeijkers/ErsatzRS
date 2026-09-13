use super::*;
use ersatzrs_core::repositories::{AddonMediaListPublishedOptions, AddonMediaListRevisionItem};
use ersatzrs_domain::{AddonMediaListItemRole, AddonMediaListPresentationKind};

fn snapshot() -> AddonMediaListRevisionSnapshot {
    let item = AddonMediaListRevisionItem {
        provider_id: "published-only".into(),
        guids: vec!["fixture:old".into()],
        rank: 0,
        kind: ersatzrs_domain::AddonMediaListItemKind::RemoteStream,
        content_kind: AddonMediaListContentKind::TelevisionEpisode,
        included: false,
        published: true,
        display_title: "Old display".into(),
        title: "Old title".into(),
        year: Some(2020),
        season: Some(1),
        episode: Some(2),
        source_url: Some("https://example.invalid/old".into()),
        additional_image_urls: Some("[]".into()),
        duration_seconds: Some(10),
        parent_provider_id: Some("excluded-parent".into()),
        fragment_start_seconds: Some(10),
        fragment_end_seconds: Some(20),
        chapter_input: String::new(),
        provider_chapter_input: String::new(),
        collection_role: AddonMediaListItemRole::default(),
        availability: AddonMediaListItemAvailability::Available,
        availability_reason: None,
        availability_reason_code: None,
        provider_liveness: RemoteStreamLiveness::Finite,
        liveness_override: None,
        effective_liveness: RemoteStreamLiveness::Finite,
        metadata_json:
            r#"{"title":"Old reviewed title","year":2020,"guids":["fixture:old"],"tags":["old"]}"#
                .into(),
        metadata_digest: String::new(),
        replacement_media_item_id: None,
        replacement_media_version_id: None,
        published_media_item_id: Some(90),
        prepared_artifact_id: None,
        subtitle_artifact_ids: vec![70],
        playback_timeline: None,
    };
    AddonMediaListRevisionSnapshot {
        list_id: 1,
        revision: 3,
        source_draft_revision: 4,
        completed_at: Utc::now(),
        name: "Old list".into(),
        description: None,
        generate_playlist: true,
        content_kind_override: AddonMediaListContentKind::Movie,
        presentation_kind: AddonMediaListPresentationKind::default(),
        requested_presentation_kind: AddonMediaListPresentationKind::default(),
        metadata_json: "{}".into(),
        options: AddonMediaListPublishedOptions {
            list_id: 1,
            revision: 3,
            source_draft_revision: Some(4),
            schema: None,
            list_values: BTreeMap::default(),
            item_values: Vec::new(),
            completed_at: None,
        },
        items: vec![item],
        files: Vec::new(),
        artifact_ids: vec![70],
        snapshot_digest: String::new(),
    }
}

#[tokio::test]
async fn retained_source_ignores_newer_draft_fields_and_keeps_published_only_owner() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    sqlx::raw_sql(
        r#"INSERT INTO "AddonInstallation" (
               "AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256",
               "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt"
           ) VALUES ('org.example.snapshot', '1.0.0', 1, 'fixture', 'fixture',
                     '{}', 'addons/fixture', '2026-09-13', '2026-09-13');
           INSERT INTO "AddonMediaList" (
               "Id", "AddonId", "ProviderId", "SourceUrl", "Name"
           ) VALUES (1, 'org.example.snapshot', 'fixture-list',
                     'https://example.invalid/list', 'Fixture list');"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(r#"INSERT INTO "AddonMediaListItem" (
        "Id","AddonMediaListId","ProviderId","DisplayTitle","Kind","Title","SourceUrl","Published","InDraft",
        "Rank","DurationSeconds","FragmentStartSeconds","FragmentEndSeconds","ParentProviderId","ContentKind",
        "ProviderMetadataJson","MetadataOverrideJson")
        VALUES (20,1,'published-only','New display',4,'New title','https://example.invalid/new',1,0,0,99,30,40,'new-parent',1,
        '{"title":"New provider title"}','{"title":"New override"}')"#).execute(&pool).await.unwrap();
    let snapshot = snapshot();
    let mut retained = load_staged_playlist_entries(
        &pool,
        1,
        AddonMediaListContentKind::MusicVideo,
        Some(&snapshot),
    )
    .await
    .unwrap();
    assert_eq!(retained.len(), 1);
    assert_eq!(retained[0].url, "https://example.invalid/old");
    assert_eq!(retained[0].content_kind, WireContentKind::Movie);
    assert_eq!(retained[0].duration_seconds, Some(10));
    assert_eq!(
        (
            retained[0].fragment_start_seconds,
            retained[0].fragment_end_seconds
        ),
        (Some(10), Some(20))
    );
    let reviewed = apply_snapshot_metadata(&snapshot, &mut retained).unwrap();
    assert_eq!(retained[0].title, "Old reviewed title");
    assert_eq!(reviewed["published-only"].tags, ["old"]);
    validate_snapshot_fragments(&retained).unwrap();
    assert!(validate_fragment_parents(&retained).is_err());
    let artifacts = snapshot_artifact_selection(&snapshot);
    assert_eq!(
        artifacts.fragment_windows["published-only"],
        (Some(10), Some(20))
    );
    assert_eq!(artifacts.artifacts.subtitles["published-only"], [70]);

    let ordinary =
        load_staged_playlist_entries(&pool, 1, AddonMediaListContentKind::MusicVideo, None)
            .await
            .unwrap();
    assert_eq!(ordinary[0].title, "New title");
    assert_eq!(ordinary[0].url, "https://example.invalid/new");
    assert_eq!(ordinary[0].content_kind, WireContentKind::MusicVideo);
}

#[test]
fn snapshot_membership_and_geometry_do_not_infer_draft_or_parent_membership() {
    let mut snapshot = snapshot();
    let mut draft_only = snapshot.items[0].clone();
    draft_only.provider_id = "draft-only".into();
    draft_only.published = false;
    draft_only.included = true;
    snapshot.items.push(draft_only);
    let mut entries = snapshot_playlist_entries(&snapshot).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].provider_id.as_deref(), Some("published-only"));
    validate_snapshot_fragments(&entries).unwrap();
    entries[0].fragment_end_seconds = entries[0].fragment_start_seconds;
    assert!(validate_snapshot_fragments(&entries).is_err());
    entries[0].fragment_end_seconds = Some(20);
    entries[0].parent_provider_id = Some("published-only".into());
    assert!(validate_snapshot_fragments(&entries).is_err());
}
