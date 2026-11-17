//! Real SQLite/preparation and native field owner. Constructed generation tokens
//! test field loading only; they grant no publication, guide freshness or sink proof.
use super::*;
use ersatzrs_app::{
    teletext_design_documents::{
        encode_teletext_effective_design_documents_with_requests,
        TeletextEffectiveDesignDocumentInput, TeletextSavedV1Region,
    },
    teletext_effective_channel_documents::TeletextEffectiveChannelDocumentRefresh,
    teletext_guide::SuppliedTeletextGuideContext,
};
use ersatzrs_core::repositories::{
    ChannelRepository as _, FfmpegProfileRepository as _, TeletextDesignMutation,
    TeletextDesignRepository as _, TeletextDesignSave, TeletextProfileMutation,
    TeletextProfileRepository as _,
};
use ersatzrs_domain::{
    Channel, FFmpegProfile, TeletextPageAddress, TeletextPageNumber, TeletextPublicationRequest,
    TeletextSubcode,
};
use ersatzrs_infra::repositories::{
    make_repositories, SqliteTeletextDesignRepository, SqliteTeletextProfileRepository,
};

struct Fixture {
    state: crate::host::state::AppState,
    stored: ersatzrs_domain::TeletextDesignRevision,
    root: Arc<EncodedTeletextAuthoredService>,
    original: TeletextCustomPagesWire,
    budget: TeletextDesignDocumentBudget,
}

fn budget(policy: &TeletextChannelDocumentEncodingPolicy) -> TeletextDesignDocumentBudget {
    TeletextDesignDocumentBudget {
        max_serialized_bytes: 262_144,
        max_documents: 8,
        max_response_bytes: 65_536,
        max_context_bytes: 65_536,
        max_context_field_bytes: 4096,
        max_resource_transmissions: 16,
        validation: policy.validation,
    }
}

async fn linked_host(state: &crate::host::state::AppState) {
    // Only host membership is relevant. This latest source is deliberately
    // unreadable: published loading must never invoke the current planner.
    let id: i64 = sqlx::query_scalar(r#"INSERT INTO "TeletextElement"
        ("Name","SourceFormat","SourceFileName","Document","PageNumber","SubpageCount","CycleSeconds","Bindings","Revision","UpdatedAt")
        VALUES ('Later invalid fixture',1,'fixture.tti',X'41',337,1,10,'[]',99,'2026-11-15T00:00:00Z') RETURNING "Id""#)
        .fetch_one(&state.pool).await.unwrap();
    sqlx::query(r#"INSERT INTO "ChannelTeletextElement" ("ChannelId","TeletextElementId","IsEnabled") VALUES (999,?,1)"#)
        .bind(id).execute(&state.pool).await.unwrap();
}

async fn seed(state: &crate::host::state::AppState) {
    sqlx::query(
        r#"INSERT INTO "Resolution" ("Id","Name","Width","Height") VALUES (1,'Fixture',160,90)"#,
    )
    .execute(&state.pool)
    .await
    .unwrap();
    let repos = make_repositories(state.pool.clone());
    repos
        .ffmpeg_profile
        .upsert(&FFmpegProfile {
            id: 1,
            name: "Fixture".to_owned(),
            resolution_id: 1,
            ..Default::default()
        })
        .await
        .unwrap();
    repos
        .channel
        .upsert(&Channel {
            id: 999,
            unique_id: uuid::Uuid::new_v4(),
            number: "999".to_owned(),
            name: "Fixture".to_owned(),
            ffmpeg_profile_id: 1,
            ..Default::default()
        })
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT INTO "Playout" ("Id","ChannelId","ScheduleKind","Seed") VALUES (999,999,30,1)"#,
    )
    .execute(&state.pool)
    .await
    .unwrap();
    let profile: i64 = sqlx::query_scalar(r#"INSERT INTO "TeletextProfile"
        ("AddonId","Name","InstallationStateRevision","InstallationVersion","InstallationPackageSha256","InstallationContentSha256","InstallationManifestSha256")
        VALUES ('a.fixture.off','Off','unused','unused','unused','unused','unused') RETURNING "Id""#)
        .fetch_one(&state.pool).await.unwrap();
    sqlx::query(r#"INSERT INTO "ChannelTeletextBinding" ("ChannelId","AddonId","ProfileId","Enabled","Revision") VALUES (999,'a.fixture.off',?,0,1)"#)
        .bind(profile).execute(&state.pool).await.unwrap();
    sqlx::query(r#"INSERT INTO "PlayoutTeletextInterval" ("PlayoutId","AddonId","Start","Finish","State") VALUES (999,'a.fixture.off',?,?,0)"#)
        .bind((Utc::now()-ersatzrs_time::Duration::minutes(1)).naive_utc().format("%F %T%.f").to_string())
        .bind((Utc::now()+ersatzrs_time::Duration::minutes(10)).naive_utc().format("%F %T%.f").to_string())
        .execute(&state.pool).await.unwrap();
}

async fn original_bytes(
    state: &crate::host::state::AppState,
    original: &TeletextCustomPagesWire,
    published_host: bool,
    policy: &TeletextChannelDocumentEncodingPolicy,
) -> Vec<u8> {
    let TeletextProfileMutation::Saved(selection) =
        SqliteTeletextProfileRepository(state.pool.clone())
            .effective_channel_snapshot(999, Utc::now())
            .await
            .unwrap()
    else {
        panic!("scope unavailable")
    };
    let guard = teletext_work::begin(&state.application, Some(deadline()))
        .await
        .unwrap();
    let context = SuppliedTeletextGuideContext {
        generation: "published",
        locale: "en-us",
        supported_locales: &["en-us"],
        timezone: "UTC",
        captured_at: Utc::now(),
        channels: &[],
    };
    let prepared = state
        .application
        .invoke_effective_channel_teletext_documents_for_refresh(
            999,
            published_host.then_some(ersatzrs_domain::TELETEXT_CUSTOM_PAGES_PRODUCER_ID),
            &context,
            policy.input,
            TeletextEffectiveChannelDocumentRefresh {
                selection: &selection,
                custom_pages: published_host.then_some(original),
                deadline: deadline(),
                hold: guard.hold_work(),
            },
        )
        .await
        .unwrap()
        .validate(policy.validation)
        .unwrap()
        .into_parts();
    assert_eq!(
        prepared.producer_bindings,
        if published_host { vec![1] } else { vec![] }
    );
    encode_teletext_effective_design_documents_with_requests(
        TeletextEffectiveDesignDocumentInput {
            selection: &selection,
            producer_bindings: &prepared.producer_bindings,
            generation: &prepared.generation,
            sources: &prepared.sources,
            entry_addon_id: prepared.entry_addon_id.as_deref(),
            entry: published_host.then_some(TeletextPageAddress {
                page: TeletextPageNumber::new(151).unwrap(),
                subcode: TeletextSubcode::new(0).unwrap(),
            }),
            v1_region: published_host.then_some(TeletextSavedV1Region::WesternLatin),
            resource_transmissions: &[],
        },
        &prepared.requests,
        budget(policy),
    )
    .unwrap()
    .into_bytes()
}

async fn fixture(published_host: bool) -> Fixture {
    let state = application().await;
    seed(&state).await;
    if published_host {
        linked_host(&state).await;
    }
    let policy = policy();
    let mut original = document();
    original.pages.retain(|page| page.page >= 151);
    let bytes = original_bytes(&state, &original, published_host, &policy).await;
    let repo = SqliteTeletextDesignRepository(state.pool.clone());
    let TeletextDesignMutation::Saved(head) = repo
        .save(
            &TeletextDesignSave {
                name: "Effective fields fixture".to_owned(),
                document_version: 2,
                document: bytes,
                existing: None,
            },
            budget(&policy).max_serialized_bytes,
        )
        .await
        .unwrap()
    else {
        panic!("save rejected")
    };
    let stored = repo
        .revision(head.id, head.revision, budget(&policy).max_serialized_bytes)
        .await
        .unwrap()
        .unwrap();
    let mut final_rows = original.clone();
    for page in &mut final_rows.pages {
        let mut row = page.rows[0].decode().unwrap();
        row[20..25].copy_from_slice(b"FINAL");
        page.rows[0] = TeletextCustomRowWire::new(1, &row);
    }
    let root = Arc::clone(initial_source(final_rows, &policy, deadline()).service());
    Fixture {
        state,
        stored,
        root,
        original,
        budget: budget(&policy),
    }
}

fn generation(fixture: &Fixture, artifact_version: u16) -> TeletextAcceptedGeneration {
    TeletextAcceptedGeneration {
        id: 1,
        artifact_version,
        artifact_digest: "a".repeat(64),
        request: TeletextPublicationRequest {
            id: 1,
            channel_id: 999,
            design_id: fixture.stored.design_id,
            revision: fixture.stored.revision,
            digest: fixture.stored.digest.clone(),
        },
    }
}

#[tokio::test]
async fn effective_field_owner_loads_published_recipe_after_link_removal_and_retains_final_rows() {
    let fixture = fixture(true).await;
    sqlx::query(r#"DELETE FROM "ChannelTeletextElement" WHERE "ChannelId"=999"#)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    for role in [2, 4, 6] {
        let owner = PublishedCustomRefresh::load(
            context(&fixture.state, &policy(), deadline()),
            &generation(&fixture, role),
            Arc::clone(&fixture.root),
            fixture.budget,
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(owner.document.pages[0].element_revision, 7);
        assert_eq!(
            owner.document.pages[0].dynamic_fields,
            fixture.original.pages[0].dynamic_fields
        );
        for page in &owner.document.pages {
            assert_eq!(&page.rows[0].decode().unwrap()[20..25], b"FINAL");
        }
        assert_native_rows(&owner);
        let guard = teletext_work::begin(&fixture.state.application, Some(deadline()))
            .await
            .unwrap();
        let later = Utc::now() + ersatzrs_time::Duration::minutes(1);
        let refreshed = owner
            .refresh_held_with_clock(
                context(&fixture.state, &policy(), deadline()),
                guard.hold_work(),
                false,
                || later,
            )
            .await
            .unwrap();
        assert_eq!(refreshed.revision, owner.revision + 1);
        assert_eq!(
            &cells(&refreshed.document, 0)[..5],
            FieldValues::clock(later).time.as_bytes()
        );
        for page in &refreshed.document.pages {
            assert_eq!(&page.rows[0].decode().unwrap()[20..25], b"FINAL");
        }
        assert_native_rows(&refreshed);
    }
}

#[tokio::test]
async fn effective_field_owner_keeps_published_absence_after_new_invalid_link() {
    let fixture = fixture(false).await;
    linked_host(&fixture.state).await;
    for role in [2, 4, 6] {
        assert!(PublishedCustomRefresh::load(
            context(&fixture.state, &policy(), deadline()),
            &generation(&fixture, role),
            Arc::clone(&fixture.root),
            fixture.budget
        )
        .await
        .unwrap()
        .is_none());
    }
}

#[tokio::test]
async fn effective_field_loading_rejects_wrong_role_channel_digest_budget_and_original_deadline() {
    let fixture = fixture(true).await;
    for role in [0, 1, 3, 5, u16::MAX] {
        assert!(PublishedCustomRefresh::load(
            context(&fixture.state, &policy(), deadline()),
            &generation(&fixture, role),
            Arc::clone(&fixture.root),
            fixture.budget
        )
        .await
        .is_err());
    }
    let mut wrong = generation(&fixture, 4);
    wrong.request.digest = "f".repeat(64);
    assert!(PublishedCustomRefresh::load(
        context(&fixture.state, &policy(), deadline()),
        &wrong,
        Arc::clone(&fixture.root),
        fixture.budget
    )
    .await
    .is_err());
    let mut other = context(&fixture.state, &policy(), deadline());
    other.channel = 998;
    assert!(PublishedCustomRefresh::load(
        other,
        &generation(&fixture, 4),
        Arc::clone(&fixture.root),
        fixture.budget
    )
    .await
    .is_err());
    let mut tiny = fixture.budget;
    tiny.max_serialized_bytes = 1;
    assert!(PublishedCustomRefresh::load(
        context(&fixture.state, &policy(), deadline()),
        &generation(&fixture, 4),
        Arc::clone(&fixture.root),
        tiny
    )
    .await
    .is_err());
    assert!(PublishedCustomRefresh::load(
        context(&fixture.state, &policy(), Instant::now()),
        &generation(&fixture, 4),
        Arc::clone(&fixture.root),
        fixture.budget
    )
    .await
    .is_err());
}
