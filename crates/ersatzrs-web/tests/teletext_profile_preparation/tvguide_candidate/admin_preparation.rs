//! Actual production guide preparation through native and HTTP admin boundaries.
//! Source-bound historical concepts still confer no live guide publication role.

use super::*;
use crate::editorial_imports::http_routes as editorial_http;
use crate::{authoring, channel_documents, designs, guide_capture, scripted_controls};
use chrono::Timelike as _;
use ersatzrs_app::teletext_design_documents::TeletextSavedGuideInputs;
use ersatzrs_core::repositories::{PlayoutRepository as _, TeletextGuideSourceFence};
use ersatzrs_domain::{PlayoutItem, TeletextPageContract};
use ersatzrs_ffmpeg::teletext::{EncodedTeletextAuthoredService, TeletextRaster};
use ersatzrs_web::host::{
    teletext_authoring::{
        observe_effective_teletext_authoring_selection, observe_teletext_authoring_selection,
        prepare_teletext_design, preview_original_teletext_design, TeletextAuthoringIdentity,
        TeletextAuthoringPrepare, TeletextAuthoringPreviewRequest,
        TeletextAuthoringPublicationState, TeletextAuthoringScope, TeletextAuthoringStatus,
    },
    teletext_designs::reload_original_teletext_design_revision,
    teletext_effective_designs::reload_effective_original_teletext_design_revision,
    teletext_work::{begin, MAX_CONCURRENT_SETTING},
};
use serde_json::{json, Value};
use std::{collections::BTreeMap, time::Instant};

const POLICY: &str = "teletext.authoring.policy";

#[path = "admin_preparation/live_publication.rs"]
mod live_publication;

fn policy() -> Value {
    let mut value = authoring::policy();
    value["version"] = json!(2);
    value["guide_limits"] = json!({
        "max_records":1000,"max_source_bytes":1_048_576,
        "max_model_nodes":10_000,"max_model_bytes":16_777_216,
        "max_digest_fields":100_000,"max_digest_bytes":16_777_216
    });
    value
}

async fn fixture() -> (Fixture, TeletextProfile) {
    let fixture = installed_fixture().await;
    let day = profile(&fixture, "Day", &[("HEADER", "DAY")]).await;
    let night = profile(
        &fixture,
        "Night",
        &[
            ("PAGE_START", "400"),
            ("PAGE_END", "499"),
            ("INDEX_PAGE", "410"),
            ("THEME", "night"),
            ("HEADER", "NIGHT"),
        ],
    )
    .await;
    fixture.bind(1, &day, None).await;
    fixture.bind(2, &night, None).await;
    guide_capture::seed_guide(&fixture).await;
    let repository = make_repositories(fixture.state.pool.clone());
    let at = Utc::now().with_nanosecond(0).unwrap();
    // Reuse the actual SQL fixture, updating its fixed historical occurrences.
    for (id, source, offset) in [(40, 0, -5), (41, 10, 5)] {
        repository
            .playout
            .upsert_item(&PlayoutItem {
                id,
                playout_id: 20,
                media_item_id: 20,
                guide_group: 7,
                start: at + chrono::Duration::minutes(offset),
                finish: at + chrono::Duration::minutes(offset + 10),
                in_point: chrono::Duration::minutes(source),
                out_point: chrono::Duration::minutes(source + 10),
                ..PlayoutItem::default()
            })
            .await
            .unwrap();
    }
    authoring::setting(&fixture, POLICY, &policy().to_string()).await;
    authoring::setting(&fixture, MAX_CONCURRENT_SETTING, "1").await;
    (fixture, night)
}

async fn request(
    fixture: &Fixture,
    channel: i64,
    index: u16,
    scope: TeletextAuthoringScope,
    sources: &[i64],
) -> TeletextAuthoringPrepare {
    let selection_digest = match scope {
        TeletextAuthoringScope::Base => observe_teletext_authoring_selection(
            &fixture.state.application,
            channel,
            designs::deadline(),
        )
        .await
        .unwrap(),
        TeletextAuthoringScope::Effective => {
            let observed = observe_effective_teletext_authoring_selection(
                &fixture.state.application,
                channel,
                designs::deadline(),
            )
            .await
            .unwrap();
            assert!(!observed.is_absent);
            assert!(observed.valid_until.is_some());
            observed.selection_digest
        }
    };
    TeletextAuthoringPrepare {
        scope,
        channel_id: channel,
        selection_digest,
        name: "Production SQL-guide concept".to_owned(),
        input_json: json!({"input_version":2,"channel_id":channel,"entry_addon_id":TVGUIDE,
            "entry":{"page":index,"subcode":0},"v1_region":"western_latin",
            "resource_transmissions":[],"guide_channel_ids":sources})
        .to_string(),
    }
}

async fn http_prepare(fixture: &Fixture) -> TeletextAuthoringIdentity {
    let request = request(fixture, 1, 200, TeletextAuthoringScope::Base, &[2]).await;
    let fields = BTreeMap::from([
        ("channel_id", "1".to_owned()),
        ("selection_digest", request.selection_digest),
        ("name", request.name),
        ("entry_addon_id", TVGUIDE.to_owned()),
        ("entry_page", "200".to_owned()),
        ("entry_subcode", "0000".to_owned()),
        ("v1_region", "western_latin".to_owned()),
        ("resource_transmissions", "[]".to_owned()),
        ("guide_channel_id", "2".to_owned()),
    ]);
    let response = editorial_http::post(
        fixture,
        "/admin-api/teletext/design/prepare",
        "application/x-www-form-urlencoded",
        serde_urlencoded::to_string(fields).unwrap().into_bytes(),
    )
    .await;
    let redirect = editorial_http::redirect(&response, 1);
    assert_eq!(
        redirect.get("saved").map(String::as_str),
        Some("draft"),
        "production guide preparation redirect: {redirect:?}"
    );
    assert!(!redirect.contains_key("error"));
    TeletextAuthoringIdentity {
        channel_id: 1,
        design_id: redirect["design_id"].parse().unwrap(),
        revision: redirect["revision"].parse().unwrap(),
    }
}

fn assert_document(
    encoded: &EncodedTeletextAuthoredService,
    source: &TeletextGuideSourceFence,
    inputs: &TeletextSavedGuideInputs,
    title: &str,
    index: u16,
) -> (TeletextGuideSourceFence, Vec<Vec<[u8; 46]>>) {
    assert_eq!(source.channel_ids, [2]);
    assert_eq!(
        encoded.declared().semantic().base().initial_page.get(),
        index
    );
    let request: Value =
        serde_json::from_slice(inputs.request(TeletextPageContract::V3).unwrap()).unwrap();
    assert_eq!(request["channels"][0]["id"], 2);
    assert_eq!(request["channels"][0]["programmes"][0]["title"], title);
    assert!(request["channels"][0]["context"]["current"].is_object());
    let text = encoded
        .declared()
        .semantic()
        .base()
        .pages
        .iter()
        .flat_map(|page| &page.rows)
        .flat_map(|row| &row.runs)
        .filter_map(|run| match &run.content {
            TeletextRunContent::Text { text } => Some(text.as_str()),
            TeletextRunContent::Mosaics { .. } => None,
        })
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        text.contains(title),
        "actual programme must reach the generated pages"
    );
    (source.clone(), designs::units(encoded))
}

#[derive(Debug, PartialEq, Eq)]
struct Proof {
    source: TeletextGuideSourceFence,
    units: Vec<Vec<[u8; 46]>>,
    raster: TeletextRaster,
}

async fn proof(
    fixture: &Fixture,
    identity: TeletextAuthoringIdentity,
    scope: TeletextAuthoringScope,
    title: &str,
    index: u16,
) -> Proof {
    let (source, units) = match scope {
        TeletextAuthoringScope::Base => {
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
            assert!(restored.metadata().is_some());
            assert_document(
                restored.encoded().unwrap(),
                restored.guide_source().unwrap(),
                restored.guide_inputs().unwrap(),
                title,
                index,
            )
        }
        TeletextAuthoringScope::Effective => {
            let restored = reload_effective_original_teletext_design_revision(
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
            assert!(restored.metadata().is_some());
            assert!(restored.selection().valid_until.is_some());
            assert_document(
                restored.encoded().unwrap(),
                restored.guide_source().unwrap(),
                restored.guide_inputs().unwrap(),
                title,
                index,
            )
        }
    };
    let preview = preview_original_teletext_design(
        &fixture.state.application,
        TeletextAuthoringPreviewRequest {
            channel_id: identity.channel_id,
            design_id: identity.design_id,
            revision: identity.revision,
            owner: Some(channel_documents::address(index, 0)),
        },
        designs::deadline(),
    )
    .await
    .unwrap();
    assert_eq!(preview.scope(), scope);
    let publication = preview.publication().unwrap();
    assert_eq!(
        publication.state,
        TeletextAuthoringPublicationState::Unrequested
    );
    // The additive source-aware publication controller now offers an explicit
    // request; this optimistic flag still grants no active generation.
    assert!(publication.can_request && !publication.can_publish);
    assert!(preview.raster().unwrap().diagnostics().is_empty());
    assert!(!preview.authoring().is_empty());
    Proof {
        source,
        units,
        raster: preview.raster().unwrap().clone(),
    }
}

async fn no_publication(fixture: &Fixture) {
    for (table, query) in [
        (
            "TeletextPublicationRequest",
            "SELECT COUNT(*) FROM TeletextPublicationRequest",
        ),
        (
            "TeletextAcceptedGeneration",
            "SELECT COUNT(*) FROM TeletextAcceptedGeneration",
        ),
        (
            "TeletextPublicationRequestCurrent",
            "SELECT COUNT(*) FROM TeletextPublicationRequestCurrent",
        ),
        (
            "TeletextAcceptedGenerationCurrent",
            "SELECT COUNT(*) FROM TeletextAcceptedGenerationCurrent",
        ),
        (
            "TeletextEffectivePublicationRequestCurrent",
            "SELECT COUNT(*) FROM TeletextEffectivePublicationRequestCurrent",
        ),
        (
            "TeletextEffectiveAcceptedGenerationCurrent",
            "SELECT COUNT(*) FROM TeletextEffectiveAcceptedGenerationCurrent",
        ),
    ] {
        let count: i64 = sqlx::query_scalar(query)
            .fetch_one(&fixture.state.pool)
            .await
            .unwrap();
        assert_eq!(count, 0, "preparation cannot publish via {table}");
    }
}

async fn count(fixture: &Fixture) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM TeletextDesignRevision")
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap()
}

async fn refusals(fixture: &Fixture) {
    let before = count(fixture).await;
    for (sources, configured, expired, expected) in [
        (
            vec![999_999],
            policy(),
            false,
            TeletextAuthoringStatus::Unavailable,
        ),
        (
            vec![2],
            {
                let mut limits = policy();
                limits["guide_limits"]["max_records"] = json!(0);
                limits
            },
            false,
            TeletextAuthoringStatus::ResourceLimit,
        ),
        (vec![2], policy(), true, TeletextAuthoringStatus::TimedOut),
    ] {
        authoring::setting(fixture, POLICY, &configured.to_string()).await;
        let request = request(fixture, 1, 200, TeletextAuthoringScope::Base, &sources).await;
        let deadline = if expired {
            Instant::now()
        } else {
            designs::deadline()
        };
        let error = prepare_teletext_design(&fixture.state, request, deadline)
            .await
            .unwrap_err();
        assert_eq!(error.status, expected);
        assert_eq!(count(fixture).await, before);
        drop(begin(&fixture.state.application, None).await.unwrap());
    }
    authoring::setting(fixture, POLICY, &policy().to_string()).await;
}

async fn effective_prepare(
    fixture: &Fixture,
    night: &TeletextProfile,
) -> TeletextAuthoringIdentity {
    scripted_controls::seed_publication_inputs(fixture).await;
    // The shared helper also seeds channel2; retain its existing actual guide owner.
    sqlx::query("DELETE FROM Playout WHERE Id=94")
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    let script=format!("assert(teletext_profile('{TVGUIDE}',{}).ok)\nassert(teletext_enabled('{TVGUIDE}',true).ok)\nadd_count('clip',3)",night.id);
    let result = scripted_controls::build_at(
        fixture,
        &script,
        1,
        true,
        (Utc::now() - chrono::Duration::seconds(1))
            .with_nanosecond(0)
            .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(result.teletext_intervals.len(), 1);
    scripted_controls::publish_result(fixture, &result).await;
    let request = request(fixture, 1, 410, TeletextAuthoringScope::Effective, &[2]).await;
    let prepared = prepare_teletext_design(&fixture.state, request, designs::deadline())
        .await
        .unwrap();
    let preview = prepared.preview.unwrap();
    assert_eq!(preview.scope(), TeletextAuthoringScope::Effective);
    assert!(preview.valid_until().is_some());
    prepared.identity
}

async fn initial_concepts(fixture: &Fixture) -> [(TeletextAuthoringIdentity, Proof); 2] {
    let first = http_prepare(fixture).await;
    let before = proof(
        fixture,
        first,
        TeletextAuthoringScope::Base,
        "Captured Guide",
        200,
    )
    .await;
    let request = request(fixture, 2, 410, TeletextAuthoringScope::Base, &[2]).await;
    let other = prepare_teletext_design(&fixture.state, request, designs::deadline())
        .await
        .unwrap();
    assert!(other
        .preview
        .as_ref()
        .unwrap()
        .raster()
        .unwrap()
        .diagnostics()
        .is_empty());
    let other_identity = other.identity;
    drop(other);
    let other_before = proof(
        fixture,
        other_identity,
        TeletextAuthoringScope::Base,
        "Captured Guide",
        410,
    )
    .await;
    assert_ne!(before.raster, other_before.raster);
    [(first, before), (other_identity, other_before)]
}

#[tokio::test]
#[ignore = "requires the exact production addon bundle and isolated state/data directories"]
async fn production_guide_admin_prepares_current_sql_http_base_and_real_lua_effective_concepts() {
    let started = Utc::now();
    let (mut fixture, night) = fixture().await;
    let initial_configuration = designs::config(&fixture).await;
    let installation = fixture
        .state
        .application
        .teletext_installation_snapshot(TVGUIDE)
        .await
        .unwrap()
        .snapshot;
    let [(first, before), (other, other_before)] = initial_concepts(&fixture).await;
    assert!(before.source.captured_at >= started && before.source.captured_at <= Utc::now());
    refusals(&fixture).await;
    sqlx::query(r#"UPDATE "MovieMetadata" SET "Title"='Refreshed Guide' WHERE "Id"=30"#)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    let refreshed = http_prepare(&fixture).await;
    let refreshed_before = proof(
        &fixture,
        refreshed,
        TeletextAuthoringScope::Base,
        "Refreshed Guide",
        200,
    )
    .await;
    assert_ne!(
        refreshed_before.source.context_digest,
        before.source.context_digest
    );
    let effective = effective_prepare(&fixture, &night).await;
    let effective_before = proof(
        &fixture,
        effective,
        TeletextAuthoringScope::Effective,
        "Refreshed Guide",
        410,
    )
    .await;
    assert_eq!(designs::config(&fixture).await, initial_configuration);
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
    let global: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM AddonSetting WHERE AddonId=?")
        .bind(TVGUIDE)
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap();
    assert_eq!(global, 0);
    no_publication(&fixture).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture = fixture.reopen().await;
    for (identity, scope, title, index, expected) in [
        (
            first,
            TeletextAuthoringScope::Base,
            "Captured Guide",
            200,
            before,
        ),
        (
            other,
            TeletextAuthoringScope::Base,
            "Captured Guide",
            410,
            other_before,
        ),
        (
            refreshed,
            TeletextAuthoringScope::Base,
            "Refreshed Guide",
            200,
            refreshed_before,
        ),
        (
            effective,
            TeletextAuthoringScope::Effective,
            "Refreshed Guide",
            410,
            effective_before,
        ),
    ] {
        assert_eq!(
            proof(&fixture, identity, scope, title, index).await,
            expected
        );
    }
    no_publication(&fixture).await;
    assert!(fixture.playout.try_recv().is_err());
}
