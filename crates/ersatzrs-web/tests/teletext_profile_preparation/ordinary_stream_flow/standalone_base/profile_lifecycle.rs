//! Shared profile invalidation and rejected controls on the ordinary HTTP route.
//! Reuses the existing installed producers, persistent DB, transport and AV gate.
use super::*;

#[path = "profile_lifecycle/active_shared.rs"]
mod active_shared;
#[path = "profile_lifecycle/mirror.rs"]
mod mirror;
use ersatzrs_core::repositories::{
    TeletextProfileRejection as Rejection, TeletextProfileSaveResult,
};

async fn write_new(root: &Path, name: &str, bytes: &[u8]) {
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join(name))
        .await
        .unwrap();
    file.write_all(bytes).await.unwrap();
}

async fn prepare_shared(fixture: &Fixture) -> (TeletextProfile, TeletextProfile) {
    let shared = fixture.profile("Shared base", "day", "good").await;
    for channel in [1, 2] {
        let bindings = fixture
            .state
            .application
            .channel_teletext_bindings(channel)
            .await
            .unwrap();
        assert_eq!(bindings.len(), 1);
        fixture
            .bind(channel, &shared, Some(bindings[0].binding.revision))
            .await;
    }
    install_secondary(fixture).await;
    // Match the existing persistent multi-producer fixture's real pin shape.
    // The preparation-only seed deliberately has a nonproduction placeholder.
    sqlx::query(r#"UPDATE "AddonInstallation" SET "PackageSha256"=? WHERE "AddonId"=?"#)
        .bind("b".repeat(64))
        .bind(SECONDARY)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    let secondary = fixture
        .profile_for(SECONDARY, "Channel one only", "day", "good")
        .await;
    fixture.bind(1, &secondary, None).await;
    (shared, secondary)
}

fn profile_save(profile: &TeletextProfile, theme: &str) -> TeletextProfileSave {
    TeletextProfileSave {
        addon_id: profile.addon_id.clone(),
        name: profile.name.clone(),
        settings: [("THEME", theme), ("RESPONSE", "good")]
            .map(|(key, value)| TeletextProfileSetting {
                key: key.into(),
                value: TeletextProfileValue::Literal(value.into()),
            })
            .into(),
        expected_installation: profile.validated_installation.clone(),
        existing: Some((profile.id, profile.revision)),
    }
}

fn binding_save(
    profile: &TeletextProfile,
    target: i64,
    revision: i64,
) -> ChannelTeletextBindingSave {
    ChannelTeletextBindingSave {
        channel_id: 1,
        addon_id: ADDON.into(),
        profile_id: target,
        enabled: true,
        expected_installation: profile.validated_installation.clone(),
        expected_profile_revision: profile.revision,
        expected_binding_revision: Some(revision),
    }
}

async fn rejected_controls(fixture: &Fixture, shared: &TeletextProfile, other: &TeletextProfile) {
    let deleted = fixture.profile("Deleted unused", "day", "good").await;
    let tombstone = saved(
        fixture
            .state
            .application
            .tombstone_teletext_profile(ADDON, deleted.id, deleted.revision)
            .await
            .unwrap(),
    );
    assert_eq!(tombstone.dependent_bindings_changed, 0);
    let before = designs::config(fixture).await;
    let profiles = fixture
        .state
        .application
        .list_teletext_profiles(ADDON)
        .await
        .unwrap();
    let bindings = fixture
        .state
        .application
        .channel_teletext_bindings(1)
        .await
        .unwrap();
    let first = bindings
        .iter()
        .find(|b| b.binding.addon_id == ADDON)
        .unwrap();
    for (id, reason) in [
        (i64::MAX, Rejection::MissingProfile),
        (other.id, Rejection::WrongAddon),
        (deleted.id, Rejection::DeletedProfile),
    ] {
        let result = fixture
            .state
            .application
            .save_channel_teletext_binding(&binding_save(shared, id, first.binding.revision))
            .await
            .unwrap();
        assert_eq!(result, TeletextProfileMutation::Rejected(reason));
    }
    let invalid = fixture
        .state
        .application
        .save_teletext_profile(&profile_save(shared, "invalid"))
        .await
        .unwrap();
    assert_eq!(
        invalid,
        TeletextProfileMutation::Rejected(Rejection::InvalidSettings)
    );
    assert_eq!(
        fixture
            .state
            .application
            .list_teletext_profiles(ADDON)
            .await
            .unwrap(),
        profiles
    );
    assert_eq!(
        fixture
            .state
            .application
            .channel_teletext_bindings(1)
            .await
            .unwrap(),
        bindings
    );
    assert_eq!(designs::config(fixture).await, before);
}

async fn reload_base(fixture: &Fixture, channel: i64) -> Option<AcceptedTeletextOutputCandidate> {
    reload_accepted_teletext_generation(
        &fixture.state.application,
        channel,
        designs::budget(),
        base_policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
}

async fn assert_pointers(fixture: &Fixture, accepted: &[AcceptedTeletextOutputCandidate; 2]) {
    for (channel, expected) in [1, 2].into_iter().zip(accepted) {
        assert_eq!(
            fixture
                .state
                .application
                .accepted_teletext_generation(channel)
                .await
                .unwrap(),
            Some(expected.generation().clone())
        );
        assert_eq!(
            fixture
                .state
                .application
                .teletext_publication_request(channel)
                .await
                .unwrap()
                .unwrap()
                .id,
            expected.generation().request.id
        );
    }
}

async fn capture_pair(
    fixture: &Fixture,
    tools: &Tools,
    source: &[u8],
    root: &Path,
    stage: &str,
    accepted: [Option<&AcceptedTeletextOutputCandidate>; 2],
) {
    let before = designs::config(fixture).await;
    let calls = tokio::fs::read(fixture.root.path().join("ordinary-calls"))
        .await
        .unwrap();
    let secondary_calls = tokio::fs::read(fixture.root.path().join("secondary-calls-day"))
        .await
        .unwrap();
    reset_source_start(fixture).await;
    let (first, second) = tokio::join!(capture(fixture, 1), capture(fixture, 2));
    for (index, bytes) in [first, second].iter().enumerate() {
        write_new(root, &format!("{stage}.channel-{}.ts", index + 1), bytes).await;
        if let Some(expected) = accepted[index] {
            client_capture::verify_transport(&tools.tsanalyze, &tools.ffprobe, bytes).await;
            client_capture::verify_av(&tools.ffmpeg, source, bytes).await;
            assert_emitted_model(bytes, expected.service());
            for (page, label) in [("200", "BASE 200/0000"), ("201", "BASE 201/0000")] {
                let decoded = client_capture::decode_page_text(&tools.ffmpeg, bytes, page).await;
                assert!(
                    decoded.contains(label),
                    "missing fixed standalone label {label}"
                );
            }
            if index == 0 {
                let decoded = client_capture::decode_page_text(&tools.ffmpeg, bytes, "223").await;
                assert!(
                    decoded.contains("Second day"),
                    "missing independent second producer"
                );
            }
        } else {
            verify_plain(tools, source, bytes).await;
        }
    }
    assert_eq!(designs::config(fixture).await, before);
    assert_eq!(
        tokio::fs::read(fixture.root.path().join("ordinary-calls"))
            .await
            .unwrap(),
        calls
    );
    assert_eq!(
        tokio::fs::read(fixture.root.path().join("secondary-calls-day"))
            .await
            .unwrap(),
        secondary_calls
    );
}

async fn update_shared(fixture: &Fixture, original: &TeletextProfile) -> TeletextProfileSaveResult {
    let result = saved(
        fixture
            .state
            .application
            .save_teletext_profile(&profile_save(original, "night"))
            .await
            .unwrap(),
    );
    assert_eq!(result.dependent_bindings_changed, 2);
    assert_eq!(result.profile.id, original.id);
    assert_eq!(result.profile.revision, original.revision + 1);
    result
}

async fn republish_pair(
    fixture: &Fixture,
    before: &[AcceptedTeletextOutputCandidate; 2],
) -> [AcceptedTeletextOutputCandidate; 2] {
    let mut values = Vec::new();
    for (channel, previous) in [1, 2].into_iter().zip(before) {
        let request_id = fixture
            .state
            .application
            .teletext_publication_request(channel)
            .await
            .unwrap()
            .unwrap()
            .id;
        let request = request_base(fixture, channel, Some(request_id)).await;
        values.push(publish_base(fixture, request, Some(previous.generation().id)).await);
    }
    values.try_into().unwrap()
}

fn assert_multi_binding(accepted: &[AcceptedTeletextOutputCandidate; 2], shared: &TeletextProfile) {
    for value in accepted {
        let main = value
            .selection()
            .bindings
            .iter()
            .find(|b| b.addon_id == ADDON)
            .unwrap();
        assert_eq!(main.profile_id, shared.id);
        assert_eq!(main.profile_revision, shared.revision);
        assert!(value.service().declared().resource_inventory().is_empty());
    }
    assert_eq!(accepted[0].selection().bindings.len(), 2);
    assert_eq!(accepted[1].selection().bindings.len(), 1);
    assert_eq!(accepted[0].service().pages().len(), 4);
    assert_eq!(accepted[1].service().pages().len(), 3);
}

async fn unavailable_secondary(fixture: &Fixture, profile: &TeletextProfile) {
    fixture
        .state
        .application
        .disable_addon(SECONDARY.into())
        .await
        .unwrap();
    let before = fixture
        .state
        .application
        .teletext_installation_snapshot(SECONDARY)
        .await
        .unwrap();
    let rows = fixture
        .state
        .application
        .channel_teletext_bindings(1)
        .await
        .unwrap();
    let binding = rows
        .iter()
        .find(|row| row.binding.addon_id == SECONDARY)
        .unwrap();
    let request = ChannelTeletextBindingSave {
        channel_id: 1,
        addon_id: SECONDARY.into(),
        profile_id: profile.id,
        enabled: true,
        expected_installation: profile.validated_installation.clone(),
        expected_profile_revision: profile.revision,
        expected_binding_revision: Some(binding.binding.revision),
    };
    assert_eq!(
        fixture
            .state
            .application
            .save_channel_teletext_binding(&request)
            .await
            .unwrap(),
        TeletextProfileMutation::Rejected(Rejection::InstallationUnavailable)
    );
    assert!(reload_base(fixture, 1).await.is_none());
    assert!(reload_base(fixture, 2).await.is_some());
    assert_eq!(
        fixture
            .state
            .application
            .teletext_installation_snapshot(SECONDARY)
            .await
            .unwrap(),
        before
    );
}

#[tokio::test]
#[ignore = "requires pinned FFmpeg9/ffprobe/TSDuck and fresh ERSATZRS_TELETEXT_PROFILE_LIFECYCLE_CAPTURE; receiver acceptance is separate"]
async fn ordinary_http_shared_profile_and_rejected_controls_preserve_base_playback() {
    let tools = Tools::load().await;
    let root = PathBuf::from(client_capture::selected(
        "ERSATZRS_TELETEXT_PROFILE_LIFECYCLE_CAPTURE",
    ));
    tokio::fs::create_dir(&root).await.unwrap();
    let (mut fixture, source) = installed_profiles_with(&tools, response, 12).await;
    let (shared, secondary) = prepare_shared(&fixture).await;
    let accepted = [
        accept_base(&fixture, 1).await,
        accept_base(&fixture, 2).await,
    ];
    assert_multi_binding(&accepted, &shared);
    enable_output_policy(&fixture).await;
    write_new(&root, "source.ts", &source).await;
    rejected_controls(&fixture, &shared, &secondary).await;
    fixture = fixture.reopen().await;
    for (channel, expected) in [1, 2].into_iter().zip(&accepted) {
        let restored = reload_base(&fixture, channel).await.unwrap();
        assert_eq!(restored.generation(), expected.generation());
        assert_eq!(
            designs::units(restored.service()),
            designs::units(expected.service())
        );
    }
    capture_pair(
        &fixture,
        &tools,
        &source,
        &root,
        "shared-day",
        [Some(&accepted[0]), Some(&accepted[1])],
    )
    .await;
    let changed = update_shared(&fixture, &shared).await;
    assert_pointers(&fixture, &accepted).await;
    fixture = fixture.reopen().await;
    for channel in [1, 2] {
        assert!(reload_base(&fixture, channel).await.is_none());
    }
    capture_pair(
        &fixture,
        &tools,
        &source,
        &root,
        "invalidated",
        [None, None],
    )
    .await;
    assert_pointers(&fixture, &accepted).await;
    let republished = republish_pair(&fixture, &accepted).await;
    assert_multi_binding(&republished, &changed.profile);
    for index in 0..2 {
        assert_ne!(
            designs::units(accepted[index].service()),
            designs::units(republished[index].service())
        );
    }
    fixture = fixture.reopen().await;
    capture_pair(
        &fixture,
        &tools,
        &source,
        &root,
        "republished-night",
        [Some(&republished[0]), Some(&republished[1])],
    )
    .await;
    unavailable_secondary(&fixture, &secondary).await;
    capture_pair(
        &fixture,
        &tools,
        &source,
        &root,
        "secondary-unavailable",
        [None, Some(&republished[1])],
    )
    .await;
    assert_pointers(&fixture, &republished).await;
    let removed = saved(
        fixture
            .state
            .application
            .tombstone_teletext_profile(ADDON, changed.profile.id, changed.profile.revision)
            .await
            .unwrap(),
    );
    assert_eq!(removed.dependent_bindings_changed, 2);
    fixture = fixture.reopen().await;
    for channel in [1, 2] {
        assert!(reload_base(&fixture, channel).await.is_none());
    }
    capture_pair(&fixture, &tools, &source, &root, "deleted", [None, None]).await;
    assert_pointers(&fixture, &republished).await;
    assert!(fixture.playout.try_recv().is_err());
    write_new(&root,"success.json",b"{\"scope\":\"Actual Router shared-profile two-channel/multibinding rejection/invalidation/republication/deletion with reopened SQLite; exact model/transport/sourceAV. No suitable receiver, Lua/mirror/full lifecycle/platform or base/9A acceptance\"}\n").await;
    fixture.state.pool.close().await;
}
