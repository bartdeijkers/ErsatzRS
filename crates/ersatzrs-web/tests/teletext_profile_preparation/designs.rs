//! File-backed immutable originals and historical native previews, not publication.
//! The parent intentionally limits these real POSIX producer fixtures to Unix.

use super::channel_documents::{
    self, address, candidate, install_contract, response, write_response,
};
use super::documents::{self, DocumentFixture};
use super::*;
use ersatzrs_app::teletext_design_documents::{
    TeletextDesignDocumentBudget, TeletextSavedEditPolicy,
};
use ersatzrs_core::repositories::{TeletextDesignMutation, TeletextDesignRejection};
use ersatzrs_domain::teletext::{
    TeletextColourPixel, TeletextColourPixelOptions, TeletextFlashPhase,
};
use ersatzrs_ffmpeg::teletext::{
    render_teletext_composition, EncodedTeletextAuthoredPage, EncodedTeletextAuthoredService,
    TeletextEncodingRegion, TeletextRaster, TeletextRasterBudget, TeletextRasterPixel,
};
use ersatzrs_web::host::{
    teletext_channel_documents::EncodedChannelTeletextDocumentCandidate,
    teletext_designs::{
        reload_original_teletext_design_revision, save_original_teletext_design_candidate,
        NativeTeletextDesignError, RestoredOriginalTeletextDesign, SavedOriginalTeletextDesign,
    },
};
use std::time::Instant;

pub(super) fn budget() -> TeletextDesignDocumentBudget {
    TeletextDesignDocumentBudget {
        max_serialized_bytes: 262_144,
        max_documents: 8,
        max_response_bytes: 65_536,
        max_context_bytes: 65_536,
        max_context_field_bytes: 4096,
        max_resource_transmissions: 16,
        validation: documents::validation_budget(),
    }
}

pub(super) fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(30)
}

pub(super) async fn persistent_pair(second_kind: DocumentFixture) -> (Fixture, [Vec<u8>; 2]) {
    let fixture = Fixture::new_persistent().await;
    install_secondary(&fixture).await;
    // Save the actual private installation pins; only this new fixture uses
    // production-shaped hashes. Existing preparation fixtures stay unchanged.
    for (addon, hash) in [(ADDON, "a".repeat(64)), (SECONDARY, "b".repeat(64))] {
        sqlx::query(r#"UPDATE "AddonInstallation" SET "PackageSha256"=? WHERE "AddonId"=?"#)
            .bind(hash)
            .bind(addon)
            .execute(&fixture.state.pool)
            .await
            .unwrap();
    }
    let values = [
        response(DocumentFixture::Base, "day", 123, 16, "nld"),
        response(second_kind, "night", 124, 32, "fra"),
    ];
    for ((addon, label), value) in [(ADDON, "first"), (SECONDARY, "second")]
        .into_iter()
        .zip(&values)
    {
        let schema = u32::try_from(value["schema_version"].as_u64().unwrap()).unwrap();
        install_contract(&fixture, addon, label, schema).await;
        write_response(&fixture, label, value);
        let profile = fixture.profile_for(addon, label, "night", "good").await;
        fixture.bind(1, &profile, None).await;
    }
    // Preserve noncanonical whitespace as well as nulls and raw aliases.
    let bytes = values.map(|value| serde_json::to_vec_pretty(&value).unwrap());
    for (label, bytes) in ["first", "second"].into_iter().zip(&bytes) {
        std::fs::write(
            fixture
                .root
                .path()
                .join(format!("channel-response-{label}.json")),
            bytes,
        )
        .unwrap();
    }
    (fixture, bytes)
}

pub(super) async fn save(
    fixture: &Fixture,
    candidate: EncodedChannelTeletextDocumentCandidate,
    existing: Option<(i64, i64)>,
) -> SavedOriginalTeletextDesign {
    match save_original_teletext_design_candidate(
        &fixture.state.application,
        "Synthetic historical design".to_owned(),
        existing,
        candidate,
        budget(),
        deadline(),
    )
    .await
    .unwrap()
    {
        TeletextDesignMutation::Saved(value) => value,
        TeletextDesignMutation::Rejected(reason) => {
            panic!("unexpected draft rejection: {reason:?}")
        }
    }
}

pub(super) fn units(encoded: &EncodedTeletextAuthoredService) -> Vec<Vec<[u8; 46]>> {
    encoded
        .pages()
        .iter()
        .map(|page| page.data_units().to_vec())
        .collect()
}

pub(super) async fn config(fixture: &Fixture) -> Vec<(Option<String>, Option<String>)> {
    sqlx::query_as(r#"SELECT "Key", "Value" FROM "ConfigElement" ORDER BY "Key""#)
        .fetch_all(&fixture.state.pool)
        .await
        .unwrap()
}

async fn revision_count(fixture: &Fixture) -> i64 {
    sqlx::query_scalar(r#"SELECT COUNT(*) FROM "TeletextDesignRevision""#)
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap()
}

fn themed_preview(encoded: &EncodedTeletextAuthoredService) -> TeletextRaster {
    let EncodedTeletextAuthoredPage::CompiledDisplay(page) = &encoded.pages()[1] else {
        panic!("the original Local theme must use the complete compiled display");
    };
    render_teletext_composition(
        page.normalized().original(),
        TeletextColourPixelOptions {
            reveal_concealed: false,
            flash_phase: TeletextFlashPhase::Original,
        },
        TeletextRasterBudget {
            max_pixels: 168_000,
            max_diagnostics: 1400,
        },
    )
    .unwrap()
}

fn assert_restored_originals(
    restored: &RestoredOriginalTeletextDesign,
    saved: &SavedOriginalTeletextDesign,
    source_bytes: &[Vec<u8>; 2],
) {
    let original = saved.candidate();
    assert_eq!(restored.selection(), original.selection());
    assert_eq!(restored.digest(), saved.digest());
    let guide_inputs = restored
        .guide_inputs()
        .expect("new captures retain request inputs");
    for contract in [
        ersatzrs_domain::TeletextPageContract::V1,
        ersatzrs_domain::TeletextPageContract::V2,
    ] {
        assert_eq!(
            guide_inputs.request(contract),
            original.requests().request(contract)
        );
    }
    assert_eq!(
        restored.edit_policy(),
        TeletextSavedEditPolicy::OriginalOnly
    );
    assert_eq!(
        restored.v1_region(),
        Some(TeletextEncodingRegion::WesternLatin)
    );
    assert!(restored.transmissions().is_empty());
    assert_eq!(restored.metadata().unwrap().entry, address(124, 32));
    assert_eq!(restored.metadata().unwrap().language.as_str(), "fra");
    for (index, source) in restored.sources().iter().enumerate() {
        assert_eq!(source.bytes(), source_bytes[index]);
        assert_eq!(source.generation(), GENERATION);
        assert_eq!(
            source.contract(),
            original.selection().bindings[index].contract
        );
        assert_eq!(
            restored.producers()[index].original(),
            original.producers()[index].original()
        );
    }
    // Original M29 remains original: the production cold compiler reconstructs
    // the composed Page override instead of storing it in place of the source.
    let second = restored.producers()[1].original().semantic();
    assert!(second.page_enhancements()[&address(124, 32)]
        .presentation
        .is_none());
    assert!(second.magazine_presentations().contains_key(&1));
    assert_eq!(
        units(restored.encoded().unwrap()),
        units(original.encoded().unwrap())
    );
    assert_eq!(
        restored.encoded().unwrap().magazine_units(),
        original.encoded().unwrap().magazine_units()
    );
    let preview = themed_preview(restored.encoded().unwrap());
    assert!(preview.diagnostics().is_empty());
    assert_eq!(preview.width(), 56 * 12);
    assert_eq!(
        preview.pixels(),
        themed_preview(original.encoded().unwrap()).pixels()
    );
    assert!(
        (144..156).any(|x| (10..20).any(|y| matches!(preview.pixel(x, y),
        Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(colour)))
            if colour.get() == [0, 0, 15])))
    );
    assert!(!format!("{restored:?}").contains(GENERATION));
    assert!(!format!("{restored:?}").contains("ZZ"));
}

#[tokio::test]
async fn mixed_originals_reopen_without_reinvocation_and_rebuild_exact_native_preview() {
    let (mut fixture, source_bytes) = persistent_pair(DocumentFixture::Authored).await;
    let before = config(&fixture).await;
    let bindings = fixture
        .state
        .application
        .channel_teletext_bindings(1)
        .await
        .unwrap();
    let prepared = candidate(&fixture, channel_documents::policy())
        .await
        .unwrap();
    channel_documents::assert_requests(&fixture, 1, 2);
    let saved = save(&fixture, prepared, None).await;
    assert_eq!(saved.head().revision, 1);
    assert_eq!(config(&fixture).await, before);
    assert_eq!(
        fixture
            .state
            .application
            .channel_teletext_bindings(1)
            .await
            .unwrap(),
        bindings
    );
    assert!(fixture.playout.try_recv().is_err());
    fixture = fixture.reopen().await;
    let restored = reload_original_teletext_design_revision(
        &fixture.state.application,
        saved.head().id,
        1,
        budget(),
        channel_documents::policy(),
        deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_restored_originals(&restored, &saved, &source_bytes);
    assert_eq!(config(&fixture).await, before);
    assert_eq!(
        fixture
            .state
            .application
            .channel_teletext_bindings(1)
            .await
            .unwrap(),
        bindings
    );
    channel_documents::assert_requests(&fixture, 1, 2);
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn deleted_profiles_and_removed_packages_do_not_destroy_saved_original_preview() {
    let (fixture, sources) = persistent_pair(DocumentFixture::Authored).await;
    let prepared = candidate(&fixture, channel_documents::policy())
        .await
        .unwrap();
    let saved = save(&fixture, prepared, None).await;
    let id = saved.head().id;
    let packets = units(saved.candidate().encoded().unwrap());
    let selected = saved.candidate().selection().clone();
    drop(saved);
    sqlx::query(r#"DELETE FROM "ChannelTeletextBinding""#)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    sqlx::query(r#"DELETE FROM "TeletextProfile""#)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    sqlx::query(r#"DELETE FROM "AddonInstallation""#)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    for package in ["package", "secondary"] {
        std::fs::remove_dir_all(fixture.root.path().join(package)).unwrap();
    }
    let mut fixture = fixture.reopen().await;
    let before = config(&fixture).await;
    let restored = reload_original_teletext_design_revision(
        &fixture.state.application,
        id,
        1,
        budget(),
        channel_documents::policy(),
        deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(restored.selection(), &selected);
    assert_eq!(restored.sources()[0].bytes(), sources[0]);
    assert_eq!(restored.sources()[1].bytes(), sources[1]);
    assert_eq!(units(restored.encoded().unwrap()), packets);
    assert!(fixture
        .state
        .application
        .channel_teletext_bindings(1)
        .await
        .unwrap()
        .is_empty());
    assert_eq!(config(&fixture).await, before);
    channel_documents::assert_requests(&fixture, 1, 2);
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn save_bounds_and_stale_cas_leave_immutable_history_unchanged() {
    let (mut fixture, _) = persistent_pair(DocumentFixture::Authored).await;
    let prepared = candidate(&fixture, channel_documents::policy())
        .await
        .unwrap();
    let mut limited = budget();
    limited.max_serialized_bytes = 1;
    assert!(matches!(
        save_original_teletext_design_candidate(
            &fixture.state.application,
            "Bounded".to_owned(),
            None,
            prepared,
            limited,
            deadline(),
        )
        .await,
        Err(NativeTeletextDesignError::Document(_))
    ));
    assert_eq!(revision_count(&fixture).await, 0);
    let prepared = candidate(&fixture, channel_documents::policy())
        .await
        .unwrap();
    let first = save(&fixture, prepared, None).await;
    let id = first.head().id;
    let digest = first.digest().to_owned();
    drop(first);
    let prepared = candidate(&fixture, channel_documents::policy())
        .await
        .unwrap();
    let second = save(&fixture, prepared, Some((id, 1))).await;
    assert_eq!(second.head().revision, 2);
    drop(second);
    let prepared = candidate(&fixture, channel_documents::policy())
        .await
        .unwrap();
    assert!(matches!(
        save_original_teletext_design_candidate(
            &fixture.state.application,
            "Stale".to_owned(),
            Some((id, 1)),
            prepared,
            budget(),
            deadline(),
        )
        .await
        .unwrap(),
        TeletextDesignMutation::Rejected(TeletextDesignRejection::Conflict)
    ));
    assert_eq!(revision_count(&fixture).await, 2);
    let first = reload_original_teletext_design_revision(
        &fixture.state.application,
        id,
        1,
        budget(),
        channel_documents::policy(),
        deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(first.digest(), digest);
    assert_eq!(
        fixture
            .state
            .application
            .teletext_design_head(id)
            .await
            .unwrap()
            .unwrap()
            .revision,
        2
    );
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn reload_rejects_corrupt_or_bounded_storage_and_native_recompilation_failure() {
    let (mut fixture, _) = persistent_pair(DocumentFixture::Authored).await;
    let prepared = candidate(&fixture, channel_documents::policy())
        .await
        .unwrap();
    let saved = save(&fixture, prepared, None).await;
    let id = saved.head().id;
    drop(saved);
    let mut limited = budget();
    limited.max_serialized_bytes = 1;
    assert!(matches!(
        reload_original_teletext_design_revision(
            &fixture.state.application,
            id,
            1,
            limited,
            channel_documents::policy(),
            deadline(),
        )
        .await,
        Err(NativeTeletextDesignError::Storage(_))
    ));
    let mut no_packets = channel_documents::policy();
    no_packets.encoding.max_units = 0;
    assert!(matches!(
        reload_original_teletext_design_revision(
            &fixture.state.application,
            id,
            1,
            budget(),
            no_packets,
            deadline(),
        )
        .await,
        Err(NativeTeletextDesignError::Encoding(_))
    ));
    assert!(reload_original_teletext_design_revision(
        &fixture.state.application,
        id,
        99,
        budget(),
        channel_documents::policy(),
        deadline(),
    )
    .await
    .unwrap()
    .is_none());
    // A corruption fixture deliberately bypasses the immutable-update trigger;
    // normal saves have already proved that history is append-only.
    sqlx::query(r#"DROP TRIGGER "TR_TeletextDesignRevision_ImmutableUpdate""#)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "TeletextDesignRevision" SET "Digest"=? WHERE "DesignId"=?"#)
        .bind("0".repeat(64))
        .bind(id)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    assert!(matches!(
        reload_original_teletext_design_revision(
            &fixture.state.application,
            id,
            1,
            budget(),
            channel_documents::policy(),
            deadline(),
        )
        .await,
        Err(NativeTeletextDesignError::Storage(_))
    ));
    assert_eq!(revision_count(&fixture).await, 1);
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

// This fixture-only application witness deliberately admits structural concept
// bytes to storage. It proves neither authenticated editor receipt provenance
// nor import permission nor complete native concept validation. Production
// reload must reject this document rather than drop its overlay under its digest.
struct StructuralEditorialFixtureConfirmation;

#[async_trait::async_trait]
impl ersatzrs_app::teletext_designs::TeletextOriginalDesignValidator
    for StructuralEditorialFixtureConfirmation
{
    async fn confirm_original(
        &self,
        document: ersatzrs_app::teletext_design_documents::EncodedTeletextDesignDocuments,
    ) -> Result<
        ersatzrs_app::teletext_design_documents::EncodedTeletextDesignDocuments,
        ersatzrs_app::teletext_designs::TeletextOriginalDesignValidationError,
    > {
        assert!(
            ersatzrs_app::teletext_design_documents::decode_teletext_design_documents(
                document.bytes(),
                budget()
            )
            .unwrap()
            .editorial_overlay()
            .is_some()
        );
        Ok(document)
    }
}

struct EditorialRejectionState {
    original: SavedOriginalTeletextDesign,
    sources: [Vec<u8>; 2],
    pending: ersatzrs_domain::TeletextPublicationRequest,
    accepted: ersatzrs_web::host::teletext_generations::AcceptedTeletextOutputCandidate,
    config: Vec<(Option<String>, Option<String>)>,
    bindings: Vec<ersatzrs_domain::ChannelTeletextBindingRead>,
    edited: ersatzrs_domain::TeletextDesignRevision,
}

fn structural_editorial_fixture_document(
    saved: &SavedOriginalTeletextDesign,
    original_bytes: &[u8],
) -> ersatzrs_app::teletext_design_documents::EncodedTeletextDesignDocuments {
    use ersatzrs_app::teletext_design_documents::{
        append_teletext_design_editorial_overlay, TeletextSavedEditorialOverlay,
        TeletextSavedEditorialProvenance,
    };
    use ersatzrs_domain::{TeletextPhysicalBaseOverlay, TeletextPhysicalBaseRow};
    use ersatzrs_ffmpeg::teletext::{
        project_encoded_teletext_display, TeletextEncodedDisplayRecord,
    };
    let encoded_page = saved
        .candidate()
        .encoded()
        .unwrap()
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::NativeDisplay(page)
                if page.encoded().page().address() == address(123, 16) =>
            {
                Some(page.encoded().page())
            }
            EncodedTeletextAuthoredPage::CompiledDisplay(page)
                if page.page().address() == address(123, 16) =>
            {
                Some(page.page())
            }
            _ => None,
        })
        .unwrap();
    let mut cells = project_encoded_teletext_display(encoded_page, 128)
        .unwrap()
        .records
        .into_iter()
        .find_map(|record| match record {
            TeletextEncodedDisplayRecord::BaseRow { row: 1, cells } => Some(cells),
            _ => None,
        })
        .unwrap();
    assert_eq!(cells[8], b'Z');
    cells[8] = b'Q';
    let overlay = TeletextSavedEditorialOverlay::new(
        TeletextSavedEditorialProvenance {
            // Valid-shaped synthetic receipt claims are intentionally not a
            // native export/import authority in this negative-control fixture.
            export_receipt_id: 1,
            export_receipt_digest: "d".repeat(64),
            original_design_id: saved.head().id,
            original_revision: 1,
            original_digest: saved.digest().to_owned(),
        },
        vec![TeletextPhysicalBaseOverlay::new(
            address(123, 16),
            vec![TeletextPhysicalBaseRow::new(1, cells).unwrap()],
        )
        .unwrap()],
    )
    .unwrap();
    append_teletext_design_editorial_overlay(original_bytes, &overlay, budget()).unwrap()
}

async fn save_structural_editorial_fixture(
    fixture: &Fixture,
    saved: &SavedOriginalTeletextDesign,
) -> ersatzrs_domain::TeletextDesignRevision {
    let original = fixture
        .state
        .application
        .teletext_design_revision(saved.head().id, 1, budget().max_serialized_bytes)
        .await
        .unwrap()
        .unwrap();
    let document = structural_editorial_fixture_document(saved, &original.document);
    let edited_head = fixture
        .state
        .application
        .save_original_teletext_design(
            "Synthetic unvalidated editor concept".to_owned(),
            Some((saved.head().id, 1)),
            document,
            &StructuralEditorialFixtureConfirmation,
            budget().max_serialized_bytes,
        )
        .await
        .unwrap();
    let TeletextDesignMutation::Saved(edited_head) = edited_head else {
        panic!("fixture-only structural save must append an immutable revision");
    };
    assert_eq!(edited_head.revision, 2);
    let edited = fixture
        .state
        .application
        .teletext_design_revision(saved.head().id, 2, budget().max_serialized_bytes)
        .await
        .unwrap()
        .unwrap();
    assert_ne!(edited.digest, saved.digest());
    assert!(
        ersatzrs_app::teletext_design_documents::decode_teletext_design_documents(
            &edited.document,
            budget()
        )
        .unwrap()
        .editorial_overlay()
        .is_some()
    );
    edited
}

async fn editorial_rejection_fixture() -> (Fixture, EditorialRejectionState) {
    let (fixture, sources) = persistent_pair(DocumentFixture::Authored).await;
    let prepared = candidate(&fixture, channel_documents::policy())
        .await
        .unwrap();
    let original = save(&fixture, prepared, None).await;
    let pending = super::generations::request(&fixture, &original, None).await;
    let accepted =
        super::generations::accepted(super::generations::accept(&fixture, &pending, 1, None).await);
    assert!(fixture
        .state
        .application
        .accepted_teletext_generation(1)
        .await
        .unwrap()
        .is_some());
    let config = config(&fixture).await;
    let bindings = fixture
        .state
        .application
        .channel_teletext_bindings(1)
        .await
        .unwrap();
    let edited = save_structural_editorial_fixture(&fixture, &original).await;
    (
        fixture,
        EditorialRejectionState {
            original,
            sources,
            pending,
            accepted,
            config,
            bindings,
            edited,
        },
    )
}

async fn assert_editorial_native_rejection(fixture: &Fixture, state: &EditorialRejectionState) {
    use ersatzrs_web::host::teletext_designs::{
        request_original_teletext_design_publication, NativeTeletextPublicationError,
    };
    assert!(matches!(
        reload_original_teletext_design_revision(
            &fixture.state.application,
            state.original.head().id,
            2,
            budget(),
            channel_documents::policy(),
            deadline(),
        )
        .await,
        Err(NativeTeletextDesignError::EditorialValidationUnavailable)
    ));
    assert!(matches!(
        request_original_teletext_design_publication(
            &fixture.state.application,
            state.original.head().id,
            2,
            state.edited.digest.clone(),
            2,
            Some(state.pending.id),
            budget(),
            channel_documents::policy(),
            deadline(),
        )
        .await,
        Err(NativeTeletextPublicationError::Design(
            NativeTeletextDesignError::EditorialValidationUnavailable
        ))
    ));
}

async fn assert_editorial_originals_and_pointers_preserved(
    fixture: &mut Fixture,
    state: &EditorialRejectionState,
) {
    let old = reload_original_teletext_design_revision(
        &fixture.state.application,
        state.original.head().id,
        1,
        budget(),
        channel_documents::policy(),
        deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(old.digest(), state.original.digest());
    let original_encoded = state.original.candidate().encoded().unwrap();
    assert_eq!(units(old.encoded().unwrap()), units(original_encoded));
    assert_eq!(
        old.encoded().unwrap().magazine_units(),
        original_encoded.magazine_units()
    );
    for (source, expected) in old.sources().iter().zip(&state.sources) {
        assert_eq!(source.bytes(), expected);
    }
    assert_eq!(
        fixture
            .state
            .application
            .teletext_publication_request(1)
            .await
            .unwrap(),
        Some(state.pending.clone())
    );
    assert_eq!(
        fixture
            .state
            .application
            .accepted_teletext_generation(1)
            .await
            .unwrap(),
        Some(state.accepted.generation().clone())
    );
    super::generations::assert_same_output(
        &super::generations::recover(fixture).await.unwrap(),
        &state.accepted,
    );
    assert_eq!(config(fixture).await, state.config);
    assert_eq!(
        fixture
            .state
            .application
            .channel_teletext_bindings(1)
            .await
            .unwrap(),
        state.bindings
    );
    assert_eq!(revision_count(fixture).await, 2);
    channel_documents::assert_requests(fixture, 1, 2);
    assert!(fixture.playout.try_recv().is_err());
}

#[tokio::test]
async fn saved_editorial_overlay_cannot_reload_or_publish_as_unchanged_original() {
    let (mut fixture, state) = editorial_rejection_fixture().await;
    for reopen in [false, true] {
        if reopen {
            fixture = fixture.reopen().await;
        }
        assert_editorial_native_rejection(&fixture, &state).await;
        assert_editorial_originals_and_pointers_preserved(&mut fixture, &state).await;
    }
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn saved_explicit_resource_lifecycle_survives_restart_without_policy_inference() {
    let (fixture, _) = persistent_pair(DocumentFixture::Companion).await;
    let mut explicit = channel_documents::policy();
    explicit
        .transmissions
        .push(documents::companion_transmission());
    let prepared = candidate(&fixture, explicit).await.unwrap();
    let saved = save(&fixture, prepared, None).await;
    let id = saved.head().id;
    let packets = units(saved.candidate().encoded().unwrap());
    assert_eq!(packets.len(), 3);
    drop(saved);
    let mut fixture = fixture.reopen().await;
    // Reload deliberately receives no lifecycle in caller policy. The exact
    // stored record, rather than an inferred counter/update, supplies it.
    let restored = reload_original_teletext_design_revision(
        &fixture.state.application,
        id,
        1,
        budget(),
        channel_documents::policy(),
        deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        restored.transmissions(),
        &[documents::companion_transmission()]
    );
    assert_eq!(units(restored.encoded().unwrap()), packets);
    channel_documents::assert_requests(&fixture, 1, 2);
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}
