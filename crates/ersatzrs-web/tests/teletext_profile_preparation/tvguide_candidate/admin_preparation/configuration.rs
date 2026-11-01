//! Production saved configuration through the normal HTTP/native boundaries.
//! Preparation and historical reopening confer no publication or stream authority.

use super::*;
use ersatzrs_domain::{ChannelTeletextBindingRead, TeletextDesignRevision, TeletextProfileRead};
use ersatzrs_web::admin::server_fns::teletext::TeletextInstallationData;
use std::path::PathBuf;

const EDITED: [(&str, &str); 8] = [
    ("PAGE_START", "300"),
    ("PAGE_END", "399"),
    ("INDEX_PAGE", "310"),
    ("THEME", "contrast"),
    ("ACCENT", "magenta"),
    ("LAYOUT", "compact"),
    ("HEADER", "EDITED HEADER"),
    ("FOOTER", "CUSTOM FOOTER"),
];

async fn selected_profile(fixture: &Fixture, id: i64) -> TeletextProfile {
    fixture
        .state
        .application
        .list_teletext_profiles(TVGUIDE)
        .await
        .unwrap()
        .into_iter()
        .find(|read| read.profile.id == id)
        .unwrap()
        .profile
}

async fn profile_post(
    fixture: &Fixture,
    profile: &TeletextProfile,
    channel: i64,
    settings: &[(&str, &str)],
) -> BTreeMap<String, String> {
    let installation = TeletextInstallationData::from(profile.validated_installation.clone());
    let mut fields = BTreeMap::from([
        ("channel_id".to_owned(), channel.to_string()),
        ("addon_id".to_owned(), TVGUIDE.to_owned()),
        ("name".to_owned(), profile.name.clone()),
        ("profile_id".to_owned(), profile.id.to_string()),
        (
            "expected_profile_revision".to_owned(),
            profile.revision.to_string(),
        ),
        (
            "expected_installation".to_owned(),
            serde_json::to_string(&installation).unwrap(),
        ),
        ("setting_count".to_owned(), settings.len().to_string()),
    ]);
    for (index, (key, value)) in settings.iter().enumerate() {
        for (suffix, value) in [
            ("key", *key),
            ("mode", "literal"),
            ("value", *value),
            ("reference_kind", ""),
            ("reference", ""),
        ] {
            fields.insert(format!("setting.{index}.{suffix}"), value.to_owned());
        }
    }
    let response = editorial_http::post(
        fixture,
        "/admin-api/teletext/profile/save",
        "application/x-www-form-urlencoded",
        serde_urlencoded::to_string(fields).unwrap().into_bytes(),
    )
    .await;
    editorial_http::redirect(&response, channel)
}

async fn edit(
    fixture: &Fixture,
    profile: &TeletextProfile,
    channel: i64,
    settings: &[(&str, &str)],
) -> TeletextProfile {
    let response = profile_post(fixture, profile, channel, settings).await;
    assert_eq!(response.get("saved").map(String::as_str), Some("profile"));
    assert!(!response.contains_key("error"));
    let current = selected_profile(fixture, profile.id).await;
    assert_eq!(current.revision, profile.revision + 1);
    assert_eq!(current.settings.len(), settings.len());
    for (key, value) in settings {
        assert!(current.settings.iter().any(|setting| setting.key == *key
            && setting.value == TeletextProfileValue::Literal((*value).to_owned())));
    }
    current
}

async fn prepare(fixture: &Fixture, channel: i64, index: u16) -> TeletextAuthoringIdentity {
    let input = request(fixture, channel, index, TeletextAuthoringScope::Base, &[2]).await;
    prepare_teletext_design(&fixture.state, input, designs::deadline())
        .await
        .unwrap()
        .identity
}

#[derive(Debug, PartialEq, Eq)]
struct Concept {
    stored: TeletextDesignRevision,
    output: Proof,
}

async fn concept(
    fixture: &Fixture,
    identity: TeletextAuthoringIdentity,
    index: u16,
    publication: TeletextAuthoringPublicationState,
) -> Concept {
    Concept {
        stored: fixture
            .state
            .application
            .teletext_design_revision(
                identity.design_id,
                identity.revision,
                designs::budget().max_serialized_bytes,
            )
            .await
            .unwrap()
            .unwrap(),
        output: proof_with_publication(
            fixture,
            identity,
            TeletextAuthoringScope::Base,
            "Captured Guide",
            index,
            publication,
        )
        .await,
    }
}

async fn assert_edited_map(fixture: &Fixture, identity: TeletextAuthoringIdentity) {
    let restored = reload_original_teletext_design_revision(
        &fixture.state.application,
        identity.design_id,
        identity.revision,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    let pages = &restored
        .encoded()
        .unwrap()
        .declared()
        .semantic()
        .base()
        .pages;
    assert!(
        pages.len() > 1,
        "the complete guide includes its index and programme pages"
    );
    for page in pages {
        assert!((300..=399).contains(&page.address.page.get()));
        for (row, text) in [(1, "EDITED HEADER"), (22, "CUSTOM FOOTER")] {
            let runs = &page
                .rows
                .iter()
                .find(|value| value.row == row)
                .unwrap()
                .runs;
            assert_eq!(
                runs[0].content,
                TeletextRunContent::Text {
                    text: text.to_owned()
                }
            );
            assert_eq!(
                runs[0].style.foreground,
                ersatzrs_domain::TeletextColour::Magenta
            );
        }
    }
    assert_eq!(restored.sources().len(), 1);
    let metadata = serde_json::to_value(restored.sources()[0].authoring().unwrap()).unwrap();
    assert_eq!(
        metadata["theme"],
        json!({"id":"contrast","overrides":[
            {"key":"accent","value":{"kind":"colour","red":15,"green":0,"blue":15}},
            {"key":"layout","value":{"kind":"layout","id":"compact"}}
        ]})
    );
    let declarations = metadata["pages"].as_array().unwrap();
    assert_eq!(declarations.len(), pages.len());
    for declaration in declarations {
        for (id, kind, row) in [
            ("header", "zone", 1),
            ("guide", "generated", 4),
            ("footer", "zone", 22),
        ] {
            let region = declaration["regions"]
                .as_array()
                .unwrap()
                .iter()
                .find(|region| region["id"] == id)
                .unwrap();
            assert_eq!(region["kind"], kind);
            assert_eq!(region["row"], row);
        }
    }
}

type RevisionRows = Vec<(i64, i64, i64, String, Vec<u8>)>;
type GlobalRows = Vec<(String, String, Option<String>)>;

#[derive(Debug, PartialEq, Eq)]
struct State {
    heads: Vec<(i64, String, i64, bool)>,
    revisions: RevisionRows,
    profiles: Vec<TeletextProfileRead>,
    bindings: Vec<ChannelTeletextBindingRead>,
    settings: Vec<(Option<String>, Option<String>)>,
    globals: GlobalRows,
}

async fn state(fixture: &Fixture) -> State {
    let application = &fixture.state.application;
    let mut bindings = application.channel_teletext_bindings(1).await.unwrap();
    bindings.extend(application.channel_teletext_bindings(2).await.unwrap());
    no_publication(fixture).await;
    State {
        heads: sqlx::query_as("SELECT Id,Name,Revision,IsDeleted FROM TeletextDesign ORDER BY Id")
            .fetch_all(&fixture.state.pool).await.unwrap(),
        revisions: sqlx::query_as("SELECT DesignId,Revision,DocumentVersion,Digest,Document FROM TeletextDesignRevision ORDER BY DesignId,Revision")
            .fetch_all(&fixture.state.pool).await.unwrap(),
        profiles: application.list_teletext_profiles(TVGUIDE).await.unwrap(),
        bindings,
        settings: designs::config(fixture).await,
        globals: sqlx::query_as("SELECT AddonId,Key,Value FROM AddonSetting ORDER BY AddonId,Key")
            .fetch_all(&fixture.state.pool).await.unwrap(),
    }
}

async fn package_bytes(fixture: &Fixture) -> BTreeMap<PathBuf, Vec<u8>> {
    let path: String =
        sqlx::query_scalar("SELECT InstallPath FROM AddonInstallation WHERE AddonId=?")
            .bind(TVGUIDE)
            .fetch_one(&fixture.state.pool)
            .await
            .unwrap();
    let root = PathBuf::from(path);
    let mut pending = vec![root.clone()];
    let mut result = BTreeMap::new();
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            let kind = entry.file_type().unwrap();
            assert!(!kind.is_symlink());
            if kind.is_dir() {
                pending.push(entry.path());
            } else {
                assert!(kind.is_file());
                result.insert(
                    entry.path().strip_prefix(&root).unwrap().to_owned(),
                    std::fs::read(entry.path()).unwrap(),
                );
            }
        }
    }
    assert!(!result.is_empty());
    result
}

async fn refusals(fixture: &Fixture, original: &TeletextProfile) -> TeletextProfile {
    let before = state(fixture).await;
    let response = profile_post(fixture, original, 1, &[("THEME", "unknown")]).await;
    assert_eq!(response.get("error").map(String::as_str), Some("invalid"));
    assert!(!response.contains_key("saved"));
    assert_eq!(state(fixture).await, before);
    drop(begin(&fixture.state.application, None).await.unwrap());
    let oversized = "X".repeat(37);
    let mut current = original.clone();
    for entries in [
        vec![("PAGE_START", "300"), ("PAGE_END", "299")],
        vec![("INDEX_PAGE", "300")],
        vec![("HEADER", oversized.as_str())],
        vec![("PAGE_END", "200")],
    ] {
        // Manifest-valid scalars are saved normally. Relational range/zone/map
        // constraints belong to the real production producer, not forged SQL.
        current = edit(fixture, &current, 1, &entries).await;
        let before = state(fixture).await;
        let input = request(fixture, 1, 200, TeletextAuthoringScope::Base, &[2]).await;
        let error = prepare_teletext_design(&fixture.state, input, designs::deadline())
            .await
            .unwrap_err();
        assert_eq!(error.status, TeletextAuthoringStatus::ProducerFailed);
        assert_eq!(state(fixture).await, before);
        drop(begin(&fixture.state.application, None).await.unwrap());
    }
    edit(fixture, &current, 1, &EDITED).await
}

#[tokio::test]
#[ignore = "requires the exact production addon bundle and isolated state/data directories"]
async fn production_guide_saved_profile_edits_and_refusals_preserve_peer_and_originals() {
    use TeletextAuthoringPublicationState::{Unavailable, Unrequested};

    let (mut fixture, night) = fixture().await;
    // Prepare both channels in the same numeric range under separate profiles.
    // Cross-producer collisions within one channel retain their existing owner.
    let peer = edit(
        &fixture,
        &night,
        2,
        &[
            ("PAGE_START", "300"),
            ("PAGE_END", "399"),
            ("INDEX_PAGE", "310"),
            ("THEME", "night"),
            ("HEADER", "NIGHT"),
        ],
    )
    .await;
    let day = fixture
        .state
        .application
        .channel_teletext_bindings(1)
        .await
        .unwrap()[0]
        .binding
        .profile_id;
    let day = selected_profile(&fixture, day).await;
    let original_id = prepare(&fixture, 1, 200).await;
    let original = concept(&fixture, original_id, 200, Unrequested).await;
    let peer_id = prepare(&fixture, 2, 310).await;
    let peer_before = concept(&fixture, peer_id, 310, Unrequested).await;
    let peer_bindings = fixture
        .state
        .application
        .channel_teletext_bindings(2)
        .await
        .unwrap();
    let initial = state(&fixture).await;
    let package = package_bytes(&fixture).await;
    let installation = fixture
        .state
        .application
        .teletext_installation_snapshot(TVGUIDE)
        .await
        .unwrap()
        .snapshot;
    let edited_profile = edit(&fixture, &day, 1, &EDITED).await;
    let edited_id = prepare(&fixture, 1, 310).await;
    let edited = concept(&fixture, edited_id, 310, Unrequested).await;
    assert_edited_map(&fixture, edited_id).await;
    assert_ne!(original.output.units, edited.output.units);
    assert_ne!(original.output.raster, edited.output.raster);
    assert_ne!(peer_before.output.raster, edited.output.raster);
    let restored_profile = refusals(&fixture, &edited_profile).await;
    let stable = state(&fixture).await;
    assert_eq!(stable.settings, initial.settings);
    assert_eq!(stable.globals, initial.globals);
    assert_eq!(selected_profile(&fixture, peer.id).await, peer);
    assert_eq!(
        fixture
            .state
            .application
            .channel_teletext_bindings(2)
            .await
            .unwrap(),
        peer_bindings
    );
    assert_eq!(package_bytes(&fixture).await, package);
    assert_eq!(
        fixture
            .state
            .application
            .teletext_installation_snapshot(TVGUIDE)
            .await
            .unwrap()
            .snapshot,
        installation
    );
    assert!(fixture.playout.try_recv().is_err());
    fixture = fixture.reopen().await;
    assert_eq!(state(&fixture).await, stable);
    assert_eq!(selected_profile(&fixture, day.id).await, restored_profile);
    assert_eq!(
        concept(&fixture, original_id, 200, Unavailable).await,
        original
    );
    assert_eq!(
        concept(&fixture, peer_id, 310, Unrequested).await,
        peer_before
    );
    assert_eq!(concept(&fixture, edited_id, 310, Unavailable).await, edited);
    assert_edited_map(&fixture, edited_id).await;
    assert_eq!(package_bytes(&fixture).await, package);
    assert!(fixture.playout.try_recv().is_err());
}
