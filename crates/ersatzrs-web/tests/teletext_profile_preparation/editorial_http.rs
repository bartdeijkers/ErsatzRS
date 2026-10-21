//! Bounded real Router editor adapters; no headed-admin acceptance claim.

use super::*;
use axum::{body::Body, http::Request, response::Response};
use ersatzrs_app::{
    commands::SetAdminSetting, teletext_export_project::parse_teletext_export_sidecar,
};
use ersatzrs_web::host::router::build_app;
use std::{collections::BTreeMap, io::Read as _};
use tower::ServiceExt as _;

const EXPORT: &str = "/admin-api/teletext/design/export";
const IMPORT: &str = "/admin-api/teletext/design/import";
const BOUNDARY: &str = "synthetic-teletext-editor-boundary";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn http_v3_only_addon_appears_in_channel_admin_before_profile_setup() {
    let mut fixture = Fixture::new_persistent().await;
    authoring_metadata::install(&fixture).await;
    let mut contract = manifest();
    contract
        .capabilities
        .retain(|capability| capability.id == "addon.check.v1");
    contract
        .capabilities
        .push(ersatzrs_addon_contract::AddonCapability {
            id: "teletext.pages.v3".to_owned(),
            options: Vec::new(),
        });
    contract.validate().unwrap();
    sqlx::query(r#"UPDATE "AddonInstallation" SET "ManifestJson"=? WHERE "AddonId"=?"#)
        .bind(serde_json::to_string(&contract).unwrap())
        .bind(ADDON)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    let response = build_app(fixture.state.clone())
        .oneshot(
            Request::builder()
                .uri("/channels/1/teletext")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = axum::body::to_bytes(response.into_body(), 2 * 1024 * 1024)
        .await
        .unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();
    assert!(html.contains(ADDON));
    assert!(html.contains("data-testid=\"teletext-profile-form\""));
    assert!(fixture.playout.try_recv().is_err());
}

async fn fixture() -> (Fixture, Flow) {
    let fixture = Fixture::new_persistent().await;
    configure_authoring_policy(&fixture).await;
    Box::pin(setup_with_fixture(
        external_editor_probe::editor_response,
        fixture,
    ))
    .await
}

pub(super) async fn configure_authoring_policy(fixture: &Fixture) {
    let mut policy: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/teletext-output-policy-v1.json")).unwrap();
    policy.as_object_mut().unwrap().remove("source");
    policy
        .as_object_mut()
        .unwrap()
        .remove("recovery_timeout_ms");
    policy["operation_timeout_ms"] = serde_json::json!(30_000);
    policy["preview"] = serde_json::json!({"max_pages":16,"max_model_records":32_768,
        "max_pixels":168_000,"max_diagnostics":1400,"max_work":50_000_000});
    fixture
        .state
        .application
        .set_admin_setting(SetAdminSetting {
            key: "teletext.authoring.policy",
            value: policy.to_string(),
        })
        .await
        .unwrap();
}

pub(super) async fn post(
    fixture: &Fixture,
    route: &str,
    content_type: &str,
    bytes: Vec<u8>,
) -> Response {
    build_app(fixture.state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(route)
                .header("content-type", content_type)
                .body(Body::from(bytes))
                .unwrap(),
        )
        .await
        .unwrap()
}

fn fields(flow: &Flow, channel: i64) -> BTreeMap<String, String> {
    let saved = &flow.saved[0];
    BTreeMap::from([
        ("channel_id".to_owned(), channel.to_string()),
        ("design_id".to_owned(), saved.head().id.to_string()),
        ("revision".to_owned(), "1".to_owned()),
        ("digest".to_owned(), saved.digest().to_owned()),
        ("expected_design_head".to_owned(), "1".to_owned()),
    ])
}

async fn export_response(fixture: &Fixture, fields: &BTreeMap<String, String>) -> Response {
    post(
        fixture,
        EXPORT,
        "application/x-www-form-urlencoded",
        serde_urlencoded::to_string(fields).unwrap().into_bytes(),
    )
    .await
}

pub(super) fn redirect(response: &Response, channel: i64) -> BTreeMap<String, String> {
    assert_eq!(response.status(), 303);
    let location = response.headers()["location"].to_str().unwrap();
    let (path, query) = location.split_once('?').unwrap();
    assert_eq!(path, format!("/channels/{channel}/teletext"));
    serde_urlencoded::from_str(query).unwrap()
}

async fn bundle(response: Response) -> BTreeMap<String, Vec<u8>> {
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["content-type"], "application/zip");
    assert!(response.headers()["content-disposition"]
        .to_str()
        .unwrap()
        .starts_with("attachment;"));
    let bytes = axum::body::to_bytes(response.into_body(), 2 * 1024 * 1024)
        .await
        .unwrap();
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut entries = BTreeMap::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).unwrap();
        assert!(entry.is_file());
        let name = entry.name().unwrap().into_owned();
        assert!(!name.contains(['/', '\\']));
        let mut body = Vec::new();
        entry.read_to_end(&mut body).unwrap();
        assert!(entries.insert(name, body).is_none());
    }
    entries
}

fn multipart_part(body: &mut Vec<u8>, name: &str, filename: Option<&str>, bytes: &[u8]) {
    body.extend_from_slice(
        format!("--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"{name}\"").as_bytes(),
    );
    if let Some(filename) = filename {
        body.extend_from_slice(format!("; filename=\"{filename}\"").as_bytes());
    }
    body.extend_from_slice(b"\r\n\r\n");
    body.extend_from_slice(bytes);
    body.extend_from_slice(b"\r\n");
}

fn multipart(request: &NativeTeletextEditorialImportRequest, channel: i64) -> Vec<u8> {
    let mut body = Vec::new();
    multipart_part(
        &mut body,
        "channel_id",
        None,
        channel.to_string().as_bytes(),
    );
    multipart_part(&mut body, "name", None, request.name.as_bytes());
    multipart_part(
        &mut body,
        "expected_design_head",
        None,
        request.expected_design_head.to_string().as_bytes(),
    );
    multipart_part(
        &mut body,
        "sidecar",
        Some("project.ersatzrs.json"),
        &request.sidecar,
    );
    for file in &request.files {
        multipart_part(&mut body, "files", Some(&file.name), &file.bytes);
    }
    body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());
    body
}

async fn import_response(
    fixture: &Fixture,
    request: &NativeTeletextEditorialImportRequest,
    channel: i64,
) -> Response {
    post(
        fixture,
        &format!("{IMPORT}?channel_id={channel}"),
        &format!("multipart/form-data; boundary={BOUNDARY}"),
        multipart(request, channel),
    )
    .await
}

#[tokio::test]
async fn http_export_download_preserves_complete_and_selected_page_files() {
    let (fixture, flow) = fixture().await;
    let mut submitted = fields(&flow, 1);
    let all = bundle(export_response(&fixture, &submitted).await).await;
    for file in flow.exported[0].files() {
        assert_eq!(all[&file.name], file.bytes);
    }
    assert_eq!(all.len(), flow.exported[0].files().len() + 1);
    let sidecar =
        parse_teletext_export_sidecar(&all["project.ersatzrs.json"], policy().project).unwrap();
    assert_eq!(sidecar.project, flow.exported[0].sidecar().project);
    submitted.insert("export_page".to_owned(), "124".to_owned());
    submitted.insert("export_subcode".to_owned(), "0020".to_owned());
    let selected = bundle(export_response(&fixture, &submitted).await).await;
    assert!(selected.contains_key("page-124-0020.tti"));
    assert!(!selected.contains_key("page-125-0020.tti"));
    assert_eq!(selected["page-124-0020.tti"], all["page-124-0020.tti"]);
    assert_original_pointers(&fixture, &flow).await;
    assert_retained_storage(&fixture, &flow).await;
}

#[tokio::test]
async fn http_import_saves_exact_revision_for_shared_preview_without_publication() {
    let (fixture, flow) = fixture().await;
    let submitted = request(&flow, 2, 8, b'Q');
    let response = import_response(&fixture, &submitted, 1).await;
    let query = redirect(&response, 1);
    assert_eq!(query["saved"], "draft");
    assert_eq!(query["design_id"], flow.saved[0].head().id.to_string());
    assert_eq!(query["revision"], "2");
    assert_eq!(query["page"], "124");
    assert_eq!(query["subcode"], "0020");
    let restored = reload_original_teletext_design_revision(
        &fixture.state.application,
        flow.saved[0].head().id,
        2,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_ne!(
        designs::units(restored.encoded().unwrap()),
        designs::units(flow.saved[0].candidate().encoded().unwrap())
    );
    assert_eq!(restored.selection(), flow.saved[0].candidate().selection());
    let preview = project_teletext_preview(
        restored.encoded().unwrap(),
        channel_documents::address(124, 32),
        TeletextColourPixelOptions {
            reveal_concealed: false,
            flash_phase: TeletextFlashPhase::Original,
        },
        preview_budget(),
    )
    .unwrap();
    assert_rgb(preview.raster(), [15, 0, 0]);
    assert_eq!(preview.navigation()[0].target.page.get(), 125);
    assert_original_pointers(&fixture, &flow).await;
    assert_retained_storage(&fixture, &flow).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn http_resource_bank_download_and_multipart_import_preserve_output_until_publication() {
    let fixture = Fixture::new_persistent().await;
    configure_authoring_policy(&fixture).await;
    let (fixture, flow) = Box::pin(setup_with_fixture_policy(
        drcs_bank::dense_response,
        fixture,
        drcs_companion::dense_encoding_policy,
    ))
    .await;
    let package = calculate_addon_content_sha256(&fixture.root.path().join("package"))
        .await
        .unwrap();
    let mut downloaded = bundle(export_response(&fixture, &fields(&flow, 1)).await).await;
    let edited = drcs_bank::bitmap_request(&flow, false, 1);
    let bank = &drcs_companion::indexed_companion(&flow, 0).0.name;
    assert_eq!(
        downloaded[bank],
        flow.exported[0]
            .files()
            .iter()
            .find(|file| &file.name == bank)
            .unwrap()
            .bytes
    );
    let sidecar = downloaded.remove("project.ersatzrs.json").unwrap();
    let metadata = parse_teletext_export_sidecar(&sidecar, policy().project).unwrap();
    let mut submitted = NativeTeletextEditorialImportRequest {
        name: "HTTP resource-bank concept".to_owned(),
        receipt_id: metadata.receipt_id,
        receipt_digest: metadata.receipt_digest,
        expected_design_head: 1,
        sidecar,
        files: downloaded
            .into_iter()
            .map(|(name, bytes)| TeletextExportFile { name, bytes })
            .collect(),
    };
    submitted
        .files
        .iter_mut()
        .find(|file| &file.name == bank)
        .unwrap()
        .bytes
        .clone_from(
            &edited
                .files
                .iter()
                .find(|file| &file.name == bank)
                .unwrap()
                .bytes,
        );
    let foreign = import_response(&fixture, &submitted, 2).await;
    assert_eq!(redirect(&foreign, 2)["error"], "foreign-channel");
    let index = submitted
        .files
        .iter()
        .position(|file| &file.name == bank)
        .unwrap();
    let removed = submitted.files.remove(index);
    assert!(redirect(&import_response(&fixture, &submitted, 1).await, 1).contains_key("error"));
    submitted.files.insert(index, removed);
    assert_eq!(head_revision(&fixture, &flow).await, 1);
    assert_original_pointers(&fixture, &flow).await;
    let query = redirect(&import_response(&fixture, &submitted, 1).await, 1);
    assert_eq!(query["saved"], "draft");
    assert_eq!(query["design_id"], flow.saved[0].head().id.to_string());
    assert_eq!(query["revision"], "2");
    let restored = reload_original_teletext_design_revision(
        &fixture.state.application,
        flow.saved[0].head().id,
        2,
        designs::budget(),
        drcs_companion::dense_encoding_policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    drcs_bank::assert_native_edit(
        flow.accepted[0].service(),
        restored.encoded().unwrap(),
        false,
    );
    assert_original_pointers(&fixture, &flow).await;
    assert_retained_storage(&fixture, &flow).await;
    assert_eq!(
        calculate_addon_content_sha256(&fixture.root.path().join("package"))
            .await
            .unwrap(),
        package
    );
    fixture.state.pool.close().await;
}

async fn head_revision(fixture: &Fixture, flow: &Flow) -> i64 {
    fixture
        .state
        .application
        .teletext_design_head(flow.saved[0].head().id)
        .await
        .unwrap()
        .unwrap()
        .revision
}

#[tokio::test]
async fn http_foreign_channel_export_and_import_reject_before_storage_mutation() {
    let (fixture, flow) = fixture().await;
    let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM \"TeletextDesignExportReceipt\"")
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap();
    let exported = export_response(&fixture, &fields(&flow, 2)).await;
    assert_eq!(redirect(&exported, 2)["error"], "foreign-channel");
    let imported = import_response(&fixture, &request(&flow, 2, 8, b'Q'), 2).await;
    assert_eq!(redirect(&imported, 2)["error"], "foreign-channel");
    let mut forged = request(&flow, 2, 8, b'Q');
    let mut sidecar: serde_json::Value = serde_json::from_slice(&forged.sidecar).unwrap();
    sidecar["project"]["channel_id"] = serde_json::json!(2);
    forged.sidecar = serde_json::to_vec(&sidecar).unwrap();
    let imported = import_response(&fixture, &forged, 2).await;
    assert_eq!(redirect(&imported, 2)["error"], "foreign-channel");
    let imported = post(
        &fixture,
        &format!("{IMPORT}?channel_id=2"),
        &format!("multipart/form-data; boundary={BOUNDARY}"),
        multipart(&request(&flow, 2, 8, b'Q'), 1),
    )
    .await;
    assert_eq!(redirect(&imported, 2)["error"], "foreign-channel");
    let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM \"TeletextDesignExportReceipt\"")
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap();
    assert_eq!(before, after);
    assert_eq!(head_revision(&fixture, &flow).await, 1);
    assert_original_pointers(&fixture, &flow).await;
    assert_retained_storage(&fixture, &flow).await;
}

#[tokio::test]
async fn http_oversized_editor_upload_rejects_without_saving_or_publication() {
    let (fixture, flow) = fixture().await;
    let mut submitted = request(&flow, 2, 8, b'Q');
    submitted.files[0].bytes =
        vec![b'x'; ersatzrs_web::api::teletext_editorial::MAX_TELETEXT_EDITOR_UPLOAD_BYTES + 1];
    let response = import_response(&fixture, &submitted, 1).await;
    assert_eq!(redirect(&response, 1)["error"], "editorial-limit");
    assert_eq!(head_revision(&fixture, &flow).await, 1);
    assert_original_pointers(&fixture, &flow).await;
    assert_retained_storage(&fixture, &flow).await;
}

#[tokio::test]
async fn http_partial_duplicate_unsafe_or_protected_files_preserve_selected_output() {
    let (fixture, flow) = fixture().await;
    for variant in 0..4 {
        let mut submitted = request(&flow, 2, 8, b'Q');
        match variant {
            0 => {
                submitted.files.pop();
            }
            1 => submitted.files.push(submitted.files[0].clone()),
            2 => submitted.files[0].name = "../private-input.tti".to_owned(),
            _ => {
                let name = flow.exported[0].sidecar().project.pages[0].name.clone();
                edit_cell(&mut submitted.files, &name, 4, 8, b'Q');
            }
        }
        let result = import_response(&fixture, &submitted, 1).await;
        assert!(redirect(&result, 1).contains_key("error"));
        assert_eq!(head_revision(&fixture, &flow).await, 1);
        assert_original_pointers(&fixture, &flow).await;
        assert_retained_storage(&fixture, &flow).await;
    }
}
