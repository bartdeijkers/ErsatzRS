//! Native originals to durable bounded editor projects, without import or publication.
//! Real POSIX producers inherit the parent's intentional Unix-only fixture scope.

use super::{authoring_metadata, channel_documents, designs, documents, generations, *};
use ersatzrs_addon_contract::TeletextPageKindWire;
use ersatzrs_app::{
    teletext_export_project::{
        parse_teletext_export_receipt_body, parse_teletext_export_sidecar,
        write_teletext_export_sidecar, TeletextExportAssetScope, TeletextExportFile,
        TeletextExportProjectBudget, TeletextExportReceiptBody,
    },
    teletext_tti::{parse_teletext_tti, TeletextTtiBudget, TeletextTtiRecord},
};
use ersatzrs_core::repositories::{TeletextDesignMutation, TeletextDesignRejection};
use ersatzrs_domain::{TeletextDesignRevision, TeletextPageAddress};
use ersatzrs_ffmpeg::teletext::{
    project_encoded_teletext_display, EncodedTeletextAuthoredPage, TeletextEncodedDisplayRecord,
};
use ersatzrs_web::host::{
    teletext_design_exports::{
        export_original_teletext_design, ExportedOriginalTeletextDesign, NativeTeletextExportError,
        NativeTeletextExportPolicy, NativeTeletextExportProjectError, NativeTeletextExportRequest,
    },
    teletext_designs::SavedOriginalTeletextDesign,
};
use serde_json::json;
use sha2::{Digest as _, Sha256};
use std::fmt::Write as _;

fn policy() -> NativeTeletextExportPolicy {
    NativeTeletextExportPolicy {
        document: designs::budget(),
        encoding: channel_documents::policy(),
        project: TeletextExportProjectBudget {
            max_bytes: 2 * 1024 * 1024,
            max_files: 64,
            max_producers: 8,
            max_file_bytes: 65_536,
            max_units: 4096,
        },
        tti: TeletextTtiBudget {
            max_bytes: 65_536,
            max_records: 256,
        },
    }
}

fn request(
    saved: &SavedOriginalTeletextDesign,
    pages: Option<Vec<TeletextPageAddress>>,
) -> NativeTeletextExportRequest {
    NativeTeletextExportRequest {
        design_id: saved.head().id,
        revision: saved.head().revision,
        design_digest: saved.digest().to_owned(),
        expected_design_head: saved.head().revision,
        pages,
    }
}

async fn export(
    fixture: &Fixture,
    saved: &SavedOriginalTeletextDesign,
    pages: Option<Vec<TeletextPageAddress>>,
) -> ExportedOriginalTeletextDesign {
    match export_original_teletext_design(
        &fixture.state.application,
        request(saved, pages),
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
    {
        TeletextDesignMutation::Saved(value) => value,
        TeletextDesignMutation::Rejected(reason) => {
            panic!("unexpected export rejection: {reason:?}")
        }
    }
}

async fn original(
    fixture: &Fixture,
    saved: &SavedOriginalTeletextDesign,
) -> TeletextDesignRevision {
    fixture
        .state
        .application
        .teletext_design_revision(
            saved.head().id,
            saved.head().revision,
            designs::budget().max_serialized_bytes,
        )
        .await
        .unwrap()
        .unwrap()
}

fn hash(bytes: &[u8]) -> String {
    let mut value = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(value, "{byte:02x}").unwrap();
    }
    value
}
fn file<'a>(files: &'a [TeletextExportFile], name: &str) -> &'a [u8] {
    &files.iter().find(|file| file.name == name).unwrap().bytes
}

async fn assert_receipt(
    fixture: &Fixture,
    exported: &ExportedOriginalTeletextDesign,
) -> TeletextExportReceiptBody {
    let receipt = fixture
        .state
        .application
        .teletext_design_export_receipt(
            exported.metadata().id,
            policy().project.max_bytes,
            designs::budget().max_serialized_bytes,
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&receipt.metadata, exported.metadata());
    assert_eq!(hash(&receipt.receipt), exported.metadata().digest);
    assert_eq!(exported.sidecar().receipt_id, receipt.metadata.id);
    assert_eq!(exported.sidecar().receipt_digest, receipt.metadata.digest);
    let body = parse_teletext_export_receipt_body(&receipt.receipt, policy().project).unwrap();
    assert_eq!(&body.project, &exported.sidecar().project);
    assert_eq!(body.files, exported.files());
    for page in &body.project.pages {
        let bytes = file(&body.files, &page.name);
        assert_eq!(bytes.len(), page.tti_bytes);
        assert_eq!(hash(bytes), page.tti_sha256);
        let tti = parse_teletext_tti(bytes, policy().tti).unwrap();
        assert!(tti.records().iter().any(|record| matches!(record,
            TeletextTtiRecord::PageNumber { page: number, editor_ordinal: 0 } if *number == page.editor_page)));
        assert!(tti
            .records()
            .contains(&TeletextTtiRecord::SubpageOrdinal(0)));
    }
    for asset in &body.project.assets {
        let bytes = file(&body.files, &asset.name);
        assert_eq!(bytes.len(), asset.bytes);
        assert_eq!(bytes.len(), asset.unit_count * 46);
        assert_eq!(hash(bytes), asset.sha256);
    }
    let bytes = write_teletext_export_sidecar(exported.sidecar(), policy().project).unwrap();
    assert_eq!(
        parse_teletext_export_sidecar(&bytes, policy().project).unwrap(),
        *exported.sidecar()
    );
    let public: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(public.get("files").is_none());
    for producer in public["project"]["producers"].as_array().unwrap() {
        for private in ["effective_settings", "settings", "permissions"] {
            assert!(producer.get(private).is_none());
        }
    }
    body
}

async fn receipt_count(fixture: &Fixture) -> i64 {
    sqlx::query_scalar(r#"SELECT COUNT(*) FROM "TeletextDesignExportReceipt""#)
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap()
}

fn assert_theme_export(
    body: &TeletextExportReceiptBody,
    saved: &SavedOriginalTeletextDesign,
    profile_id: i64,
    value: &serde_json::Value,
) {
    assert_eq!(
        body.project.channel_id,
        saved.candidate().selection().channel_id
    );
    assert_eq!(body.project.producers.len(), 1);
    let producer = &body.project.producers[0];
    assert_eq!(producer.addon_id, ADDON);
    assert_eq!(producer.contract, 3);
    assert_eq!(producer.profile_id, profile_id);
    assert_eq!(
        serde_json::to_value(producer.authoring.as_ref().unwrap()).unwrap(),
        value["authoring"]
    );
    assert_eq!(body.project.pages.len(), 1);
    let page = &body.project.pages[0];
    assert_eq!(page.page.page, 124);
    assert_eq!(page.page.subcode, 32);
    assert_eq!(page.editor_page, 0x124);
    assert_eq!(page.editor_ordinal, 0);
    assert_eq!(page.language, "nld");
    assert_eq!(page.kind, TeletextPageKindWire::FullPage);
    assert_eq!(page.materialized_magazine, Some(1));
    let tti = parse_teletext_tti(file(&body.files, &page.name), policy().tti).unwrap();
    assert!(tti.records().iter().any(|record| matches!(
        record,
        TeletextTtiRecord::Packet {
            row: 28,
            designation: 0,
            ..
        }
    )));
    let original_units = saved.candidate().encoded().unwrap().magazine_units();
    let asset = body
        .project
        .assets
        .iter()
        .find(|asset| {
            matches!(
                asset.scope,
                TeletextExportAssetScope::Magazine { magazine: 1 }
            )
        })
        .unwrap();
    assert_eq!(file(&body.files, &asset.name), original_units.concat());
}

#[tokio::test]
async fn native_exports_keep_two_channel_v3_themes_and_originals_durable_without_reinvocation() {
    let mut fixture = Fixture::new_persistent().await;
    authoring_metadata::install(&fixture).await;
    let values = [
        authoring_metadata::response("day", 124, 32),
        authoring_metadata::response("night", 124, 32),
    ];
    for (theme, value) in ["day", "night"].into_iter().zip(&values) {
        authoring_metadata::write(&fixture, theme, value);
    }
    let day = fixture.profile("Export day", "day", "good").await;
    let night = fixture.profile("Export night", "night", "good").await;
    fixture.bind(1, &day, None).await;
    fixture.bind(2, &night, None).await;
    let first = authoring_metadata::candidate(&fixture, 1).await;
    let second = authoring_metadata::candidate(&fixture, 2).await;
    let saved = [
        designs::save(&fixture, first, None).await,
        designs::save(&fixture, second, None).await,
    ];
    let originals = [
        original(&fixture, &saved[0]).await,
        original(&fixture, &saved[1]).await,
    ];
    let requests = [
        generations::request(&fixture, &saved[0], None).await,
        generations::request(&fixture, &saved[1], None).await,
    ];
    let accepted =
        generations::accepted(generations::accept(&fixture, &requests[0], 1, None).await);
    let configuration = designs::config(&fixture).await;
    let calls = std::fs::read(fixture.root.path().join("authoring-calls")).unwrap();
    let exported = [
        export(&fixture, &saved[0], None).await,
        export(&fixture, &saved[1], None).await,
    ];
    assert_ne!(exported[0].metadata().id, exported[1].metadata().id);
    let bodies = [
        assert_receipt(&fixture, &exported[0]).await,
        assert_receipt(&fixture, &exported[1]).await,
    ];
    assert_eq!(
        [bodies[0].project.channel_id, bodies[1].project.channel_id],
        [1, 2]
    );
    for (index, body) in bodies.iter().enumerate() {
        assert_theme_export(
            body,
            &saved[index],
            [day.id, night.id][index],
            &values[index],
        );
    }
    assert_ne!(
        bodies[0].project.pages[0].tti_sha256,
        bodies[1].project.pages[0].tti_sha256
    );
    fixture = fixture.reopen().await;
    for (index, result) in exported.iter().enumerate() {
        assert_eq!(assert_receipt(&fixture, result).await, bodies[index]);
        assert_eq!(original(&fixture, &saved[index]).await, originals[index]);
        assert_eq!(
            fixture
                .state
                .application
                .teletext_design_head(saved[index].head().id)
                .await
                .unwrap()
                .unwrap(),
            *saved[index].head()
        );
        assert_eq!(
            fixture
                .state
                .application
                .teletext_publication_request(i64::try_from(index + 1).unwrap())
                .await
                .unwrap(),
            Some(requests[index].clone())
        );
    }
    generations::assert_same_output(&generations::recover(&fixture).await.unwrap(), &accepted);
    assert_eq!(designs::config(&fixture).await, configuration);
    assert_eq!(
        std::fs::read(fixture.root.path().join("authoring-calls")).unwrap(),
        calls
    );
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

fn assert_original_x28(
    saved: &SavedOriginalTeletextDesign,
    address: TeletextPageAddress,
    tti: &ersatzrs_app::teletext_tti::TeletextTtiDocument,
) {
    // The retained V1 encoding region already emits page X28. It must remain
    // exact, rather than being omitted or mistaken for editor-only M29 context.
    let native = saved
        .candidate()
        .encoded()
        .unwrap()
        .pages()
        .iter()
        .find_map(|page| {
            let display = match page {
                EncodedTeletextAuthoredPage::NativeDisplay(value) => value.encoded().page(),
                EncodedTeletextAuthoredPage::CompiledDisplay(value) => value.page(),
                _ => return None,
            };
            (display.address() == address).then_some(display)
        })
        .unwrap();
    let projected = project_encoded_teletext_display(native, policy().project.max_units).unwrap();
    let original_x28: Vec<_> = projected
        .records
        .iter()
        .filter_map(|record| match record {
            TeletextEncodedDisplayRecord::Triplets {
                row: 28,
                designation,
                words,
            } => Some((*designation, *words)),
            _ => None,
        })
        .collect();
    let exported_x28: Vec<_> = tti
        .records()
        .iter()
        .filter_map(|record| match record {
            TeletextTtiRecord::Packet {
                row: 28,
                designation,
                words,
            } => Some((*designation, *words)),
            _ => None,
        })
        .collect();
    assert!(!original_x28.is_empty());
    assert_eq!(exported_x28, original_x28);
}

#[tokio::test]
async fn native_subset_can_omit_entry_and_keeps_decimal_address_kind_language_and_subcode() {
    let (mut fixture, _) = designs::persistent_pair(documents::DocumentFixture::Authored).await;
    let mut first =
        channel_documents::response(documents::DocumentFixture::Base, "day", 123, 16, "nld");
    first["pages"][0]["kind"] = json!("newsflash");
    // Newsflash content must be boxed in the ordinary fallback, too.
    for row in first["pages"][0]["rows"].as_array_mut().unwrap() {
        for run in row["runs"].as_array_mut().unwrap() {
            run["style"]["boxed"] = json!(true);
        }
    }
    channel_documents::write_response(&fixture, "first", &first);
    let candidate = channel_documents::candidate(&fixture, channel_documents::policy())
        .await
        .unwrap();
    let saved = designs::save(&fixture, candidate, None).await;
    let before = original(&fixture, &saved).await;
    let configuration = designs::config(&fixture).await;
    let calls = std::fs::read(fixture.root.path().join("channel-calls")).unwrap();
    let exported = export(
        &fixture,
        &saved,
        Some(vec![channel_documents::address(123, 16)]),
    )
    .await;
    let body = assert_receipt(&fixture, &exported).await;
    assert_eq!(body.project.entry.page, 124);
    assert_eq!(body.project.language, "fra");
    assert_eq!(body.project.pages.len(), 1);
    let page = &body.project.pages[0];
    assert_eq!(page.page.page, 123);
    assert_eq!(page.page.subcode, 16);
    assert_eq!(page.editor_page, 0x123);
    assert_eq!(page.editor_ordinal, 0);
    assert_eq!(page.language, "nld");
    assert_eq!(page.kind, TeletextPageKindWire::Newsflash);
    assert_eq!(page.header_controls & 6, 2);
    assert_eq!(page.materialized_magazine, None);
    assert_eq!(page.producer_ordinal, 0);
    let tti = parse_teletext_tti(file(&body.files, &page.name), policy().tti).unwrap();
    assert!(!tti
        .records()
        .iter()
        .any(|record| matches!(record, TeletextTtiRecord::PageNumber { page: 0x124, .. })));
    assert_original_x28(&saved, channel_documents::address(123, 16), &tti);
    fixture = fixture.reopen().await;
    assert_eq!(assert_receipt(&fixture, &exported).await, body);
    assert_eq!(original(&fixture, &saved).await, before);
    assert_eq!(designs::config(&fixture).await, configuration);
    assert_eq!(
        std::fs::read(fixture.root.path().join("channel-calls")).unwrap(),
        calls
    );
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn native_export_retains_complete_resource_packets_outside_editable_tti() {
    let (mut fixture, _) = designs::persistent_pair(documents::DocumentFixture::Companion).await;
    let mut encoding = channel_documents::policy();
    encoding
        .transmissions
        .push(documents::companion_transmission());
    let candidate = channel_documents::candidate(&fixture, encoding)
        .await
        .unwrap();
    let saved = designs::save(&fixture, candidate, None).await;
    let before = original(&fixture, &saved).await;
    let configuration = designs::config(&fixture).await;
    let calls = std::fs::read(fixture.root.path().join("channel-calls")).unwrap();
    let exported = export(&fixture, &saved, None).await;
    let body = assert_receipt(&fixture, &exported).await;
    assert_eq!(body.project.pages.len(), 2);
    let resource = saved
        .candidate()
        .encoded()
        .unwrap()
        .pages()
        .iter()
        .find(|page| {
            matches!(
                page,
                EncodedTeletextAuthoredPage::Object(_) | EncodedTeletextAuthoredPage::Resource(_)
            )
        })
        .unwrap();
    let header = resource.resource_header().unwrap();
    let assets = body
        .project
        .assets
        .iter()
        .filter(|asset| matches!(asset.scope, TeletextExportAssetScope::Resource { .. }))
        .collect::<Vec<_>>();
    assert_eq!(assets.len(), 1);
    let asset = assets[0];
    assert_eq!(asset.unit_count, resource.data_units().len());
    assert_eq!(
        file(&body.files, &asset.name),
        resource.data_units().concat()
    );
    let TeletextExportAssetScope::Resource {
        identity,
        physical_subcode,
    } = &asset.scope
    else {
        unreachable!()
    };
    assert_eq!(identity.page.magazine, header.identity().page().magazine());
    assert_eq!(identity.page.page, header.identity().page().page());
    assert_eq!(*physical_subcode, header.physical_subcode());
    fixture = fixture.reopen().await;
    assert_eq!(assert_receipt(&fixture, &exported).await, body);
    assert_eq!(original(&fixture, &saved).await, before);
    assert_eq!(designs::config(&fixture).await, configuration);
    assert_eq!(
        std::fs::read(fixture.root.path().join("channel-calls")).unwrap(),
        calls
    );
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn native_export_rejects_stale_invalid_and_bounded_requests_without_publication_damage() {
    let (mut fixture, saved, pending) = generations::prepared().await;
    let accepted = generations::accepted(generations::accept(&fixture, &pending, 1, None).await);
    let before = original(&fixture, &saved).await;
    let configuration = designs::config(&fixture).await;
    let calls = std::fs::read(fixture.root.path().join("channel-calls")).unwrap();
    let mut wrong_digest = request(&saved, None);
    wrong_digest.design_digest = "f".repeat(64);
    let mut wrong_head = request(&saved, None);
    wrong_head.expected_design_head = 2;
    for (index, invalid) in [
        wrong_digest,
        wrong_head,
        request(&saved, Some(Vec::new())),
        request(&saved, Some(vec![channel_documents::address(125, 0)])),
        request(&saved, Some(vec![channel_documents::address(123, 16); 2])),
    ]
    .into_iter()
    .enumerate()
    {
        let result = export_original_teletext_design(
            &fixture.state.application,
            invalid,
            policy(),
            designs::deadline(),
        )
        .await;
        match index {
            0 | 1 => assert!(matches!(
                result,
                Ok(TeletextDesignMutation::Rejected(
                    TeletextDesignRejection::Conflict
                ))
            )),
            2 => assert!(matches!(
                result,
                Ok(TeletextDesignMutation::Rejected(
                    TeletextDesignRejection::InvalidValues
                ))
            )),
            _ => assert!(
                matches!(result, Err(NativeTeletextExportError::Mapping(error)) if matches!(*error, NativeTeletextExportProjectError::Association))
            ),
        }
    }
    let mut limited = policy();
    limited.project.max_bytes = 1;
    let result = export_original_teletext_design(
        &fixture.state.application,
        request(&saved, None),
        limited,
        designs::deadline(),
    )
    .await;
    assert!(
        matches!(result, Err(NativeTeletextExportError::Mapping(error)) if matches!(*error, NativeTeletextExportProjectError::Budget))
    );
    assert_eq!(receipt_count(&fixture).await, 0);
    assert_eq!(original(&fixture, &saved).await, before);
    fixture = fixture.reopen().await;
    assert_eq!(receipt_count(&fixture).await, 0);
    assert_eq!(original(&fixture, &saved).await, before);
    assert_eq!(
        fixture
            .state
            .application
            .teletext_design_head(saved.head().id)
            .await
            .unwrap()
            .unwrap(),
        *saved.head()
    );
    assert_eq!(
        fixture
            .state
            .application
            .teletext_publication_request(1)
            .await
            .unwrap(),
        Some(pending)
    );
    generations::assert_same_output(&generations::recover(&fixture).await.unwrap(), &accepted);
    assert_eq!(designs::config(&fixture).await, configuration);
    assert_eq!(
        std::fs::read(fixture.root.path().join("channel-calls")).unwrap(),
        calls
    );
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}
