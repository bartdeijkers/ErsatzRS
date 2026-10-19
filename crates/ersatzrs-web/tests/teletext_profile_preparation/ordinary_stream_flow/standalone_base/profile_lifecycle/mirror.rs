//! Mirror programme source and teletext output ownership stay independent.
use super::*;

#[path = "mirror/active_scoped.rs"]
mod active_scoped;

async fn install_mirror(fixture: &Fixture) {
    let repositories = make_repositories(fixture.state.pool.clone());
    let mut channel = repositories.channel.get_with_profile(1).await.unwrap();
    channel.id = 3;
    channel.unique_id = Uuid::from_u128(3);
    channel.number = "3".into();
    channel.name = "Synthetic mirror".into();
    channel.playout_source = ersatzrs_domain::ChannelPlayoutSource::Mirror;
    channel.mirror_source_channel_id = Some(1);
    channel.playout_offset = None;
    channel.playout_ids.clear();
    repositories.channel.upsert(&channel).await.unwrap();
    let persisted = repositories.channel.get_with_profile(3).await.unwrap();
    assert_eq!(
        persisted.playout_source,
        ersatzrs_domain::ChannelPlayoutSource::Mirror
    );
    assert_eq!(persisted.mirror_source_channel_id, Some(1));
    assert!(fixture
        .state
        .application
        .channel_teletext_bindings(3)
        .await
        .unwrap()
        .is_empty());
}

async fn capture_mirror(
    fixture: &Fixture,
    tools: &Tools,
    source: &[u8],
    root: &Path,
    stage: &str,
    expected: Option<&AcceptedTeletextOutputCandidate>,
) {
    let config = designs::config(fixture).await;
    let calls = tokio::fs::read(fixture.root.path().join("ordinary-calls"))
        .await
        .unwrap();
    reset_source_start(fixture).await;
    let bytes = capture(fixture, 3).await;
    write_new(root, &format!("{stage}.mirror-3.ts"), &bytes).await;
    if let Some(expected) = expected {
        assert_eq!(expected.selection().channel_id, 3);
        assert_eq!(expected.selection().bindings[0].channel_id, 3);
        client_capture::verify_transport(&tools.tsanalyze, &tools.ffprobe, &bytes).await;
        client_capture::verify_av(&tools.ffmpeg, source, &bytes).await;
        assert_emitted_model(&bytes, expected.service());
        let decoded = client_capture::decode_page_text(&tools.ffmpeg, &bytes, "200").await;
        assert!(decoded.contains("BASE 200/0000"));
    } else {
        verify_plain(tools, source, &bytes).await;
    }
    assert_eq!(designs::config(fixture).await, config);
    assert_eq!(
        tokio::fs::read(fixture.root.path().join("ordinary-calls"))
            .await
            .unwrap(),
        calls
    );
}

#[tokio::test]
#[ignore = "requires pinned FFmpeg9/ffprobe/TSDuck and fresh ERSATZRS_TELETEXT_PROFILE_MIRROR_CAPTURE; receiver acceptance is separate"]
async fn ordinary_http_mirror_uses_own_teletext_binding_and_off_without_source_inheritance() {
    let tools = Tools::load().await;
    let root = PathBuf::from(client_capture::selected(
        "ERSATZRS_TELETEXT_PROFILE_MIRROR_CAPTURE",
    ));
    tokio::fs::create_dir(&root).await.unwrap();
    let (mut fixture, source) = installed_profiles_with(&tools, response, 12).await;
    install_mirror(&fixture).await;
    let source_day = accept_base(&fixture, 1).await;
    enable_output_policy(&fixture).await;
    write_new(&root, "source.ts", &source).await;
    capture_mirror(&fixture, &tools, &source, &root, "unbound", None).await;
    let night = fixture
        .state
        .application
        .list_teletext_profiles(ADDON)
        .await
        .unwrap()
        .into_iter()
        .find(|p| p.profile.name == "night")
        .unwrap()
        .profile;
    let binding = fixture.bind(3, &night, None).await;
    let mirror_night = accept_base(&fixture, 3).await;
    assert_ne!(
        designs::units(source_day.service()),
        designs::units(mirror_night.service())
    );
    fixture = fixture.reopen().await;
    let recovered = reload_base(&fixture, 3).await.unwrap();
    assert_eq!(recovered.generation(), mirror_night.generation());
    capture_mirror(
        &fixture,
        &tools,
        &source,
        &root,
        "own-night",
        Some(&mirror_night),
    )
    .await;
    let off = fixture
        .state
        .application
        .disable_channel_teletext_binding(3, ADDON, binding.revision)
        .await
        .unwrap();
    assert!(matches!(off, TeletextProfileMutation::Saved(_)));
    fixture = fixture.reopen().await;
    assert!(reload_base(&fixture, 3).await.is_none());
    assert_eq!(
        reload_base(&fixture, 1).await.unwrap().generation(),
        source_day.generation()
    );
    capture_mirror(&fixture, &tools, &source, &root, "own-off", None).await;
    assert_eq!(
        fixture
            .state
            .application
            .accepted_teletext_generation(3)
            .await
            .unwrap(),
        Some(mirror_night.generation().clone())
    );
    assert!(fixture.playout.try_recv().is_err());
    write_new(&root,"success.json",b"{\"scope\":\"Actual mirror programme source1/output3, unbound and ownOff remain plainAV despite sourceDay; ownNight after explicit publish/reopen matches its exact model/transport/sourceAV. No receiver/Lua/full lifecycle/platform/base9A acceptance\"}\n").await;
    fixture.state.pool.close().await;
}
