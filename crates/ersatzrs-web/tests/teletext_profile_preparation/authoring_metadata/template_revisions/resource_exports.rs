//! Actual native exports seal complete installed-version resource bodies.
use super::*;
use ersatzrs_app::{
    teletext_export_project::{
        parse_teletext_export_receipt_body, TeletextExportAssetScope, TeletextExportProjectBudget,
    },
    teletext_tti::TeletextTtiBudget,
};
use ersatzrs_core::repositories::TeletextDesignMutation;
use ersatzrs_domain::TeletextDesignExportReceipt;
use ersatzrs_web::host::teletext_design_exports::{
    export_original_teletext_design, NativeTeletextExportPolicy, NativeTeletextExportRequest,
};

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

pub(super) async fn capture(
    fixture: &Fixture,
    design: &SavedOriginalTeletextDesign,
    revision: u8,
) -> TeletextDesignExportReceipt {
    let exported = export_original_teletext_design(
        &fixture.state.application,
        NativeTeletextExportRequest {
            design_id: design.head().id,
            revision: 1,
            design_digest: design.digest().to_owned(),
            expected_design_head: 1,
            pages: None,
        },
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    let TeletextDesignMutation::Saved(exported) = exported else {
        panic!("installed original must export")
    };
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
    let body = parse_teletext_export_receipt_body(&receipt.receipt, policy().project).unwrap();
    assert_eq!(&body.project, &exported.sidecar().project);
    assert_eq!(body.files, exported.files());
    let service = design.candidate().encoded().unwrap();
    let pages = service
        .pages()
        .iter()
        .filter(|page| page.resource_header().is_some())
        .collect::<Vec<_>>();
    assert_eq!(pages.len(), if revision == 1 { 0 } else { 2 });
    for page in pages {
        let header = page.resource_header().unwrap();
        let expected = json!({"kind":if header.identity().kind()==ersatzrs_domain::TeletextResourceKind::Pop {"pop"} else {"gpop"},
            "page":{"magazine":1,"page":header.identity().page().page()},"subpage":0});
        let asset=body.project.assets.iter().find(|asset| {
            matches!(&asset.scope,TeletextExportAssetScope::Resource {identity,physical_subcode}
                if serde_json::to_value(identity).unwrap()==expected && *physical_subcode==header.physical_subcode())
        }).expect("exact complete resource identity, S1 and physical header");
        let file = body
            .files
            .iter()
            .find(|file| file.name == asset.name)
            .unwrap();
        assert_eq!(file.bytes, page.data_units().concat());
    }
    let mut objects = body
        .project
        .assets
        .iter()
        .filter_map(|asset| match &asset.scope {
            TeletextExportAssetScope::ObjectTtiV1 { object_id, .. }
            | TeletextExportAssetScope::ObjectContextTtiV1 { object_id, .. } => Some(*object_id),
            _ => None,
        })
        .collect::<Vec<_>>();
    objects.sort_unstable();
    assert_eq!(objects, if revision == 1 { vec![] } else { vec![31, 32] });
    receipt
}

pub(super) async fn verify(fixture: &Fixture, receipt: &TeletextDesignExportReceipt) {
    assert_eq!(
        fixture
            .state
            .application
            .teletext_design_export_receipt(
                receipt.metadata.id,
                policy().project.max_bytes,
                designs::budget().max_serialized_bytes,
            )
            .await
            .unwrap()
            .as_ref(),
        Some(receipt)
    );
}
