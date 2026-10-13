//! Actual installed synthetic V3 originals to editor concepts and native previews.
//! POSIX fixtures inherit the parent's Unix-only scope. Regular cases check the
//! application flow; the opt-in external-editor case also captures ordinary TS.
//! Headed editor/client evidence and complete engine acceptance remain separate.

use super::{authoring_metadata, channel_documents, designs, generations, *};
use ersatzrs_app::{
    teletext_export_project::{
        write_teletext_export_sidecar, TeletextExportFile, TeletextExportProjectBudget,
    },
    teletext_tti::{
        parse_teletext_tti, write_teletext_tti, TeletextTtiBudget, TeletextTtiDocument,
        TeletextTtiRecord,
    },
};
use ersatzrs_core::repositories::TeletextDesignMutation;
use ersatzrs_domain::{
    TeletextColourPixel, TeletextColourPixelOptions, TeletextDesignExportReceipt,
    TeletextDesignRevision, TeletextFlashPhase, TeletextPublicationRequest,
};
use ersatzrs_ffmpeg::teletext::{
    project_encoded_teletext_display, EncodedTeletextAuthoredPage, TeletextEncodedDisplayRecord,
    TeletextRaster, TeletextRasterPixel,
};
use ersatzrs_web::host::{
    teletext_design_exports::{
        export_original_teletext_design, ExportedOriginalTeletextDesign,
        NativeTeletextExportPolicy, NativeTeletextExportRequest,
    },
    teletext_designs::{reload_original_teletext_design_revision, SavedOriginalTeletextDesign},
    teletext_editorial_designs::{
        import_teletext_editorial_design, NativeTeletextEditorialImportRequest,
        SavedEditorialTeletextDesign,
    },
    teletext_generations::{
        accept_original_teletext_generation, reload_accepted_teletext_generation,
        AcceptedTeletextOutputCandidate, TeletextGenerationTarget,
    },
    teletext_preview::project_teletext_preview,
};

#[path = "editorial_http.rs"]
mod http_routes;

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

struct Flow {
    saved: [SavedOriginalTeletextDesign; 2],
    originals: [TeletextDesignRevision; 2],
    exported: [ExportedOriginalTeletextDesign; 2],
    receipts: [TeletextDesignExportReceipt; 2],
    pending: [TeletextPublicationRequest; 2],
    accepted: [AcceptedTeletextOutputCandidate; 2],
    config: Vec<(Option<String>, Option<String>)>,
    calls: Vec<u8>,
}

async fn export(
    fixture: &Fixture,
    saved: &SavedOriginalTeletextDesign,
) -> ExportedOriginalTeletextDesign {
    let result = export_original_teletext_design(
        &fixture.state.application,
        NativeTeletextExportRequest {
            design_id: saved.head().id,
            revision: 1,
            design_digest: saved.digest().to_owned(),
            expected_design_head: 1,
            pages: None,
        },
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    let TeletextDesignMutation::Saved(exported) = result else {
        panic!("synthetic original must export a durable project");
    };
    exported
}

async fn original(
    fixture: &Fixture,
    saved: &SavedOriginalTeletextDesign,
) -> TeletextDesignRevision {
    fixture
        .state
        .application
        .teletext_design_revision(saved.head().id, 1, designs::budget().max_serialized_bytes)
        .await
        .unwrap()
        .unwrap()
}

async fn receipt(
    fixture: &Fixture,
    exported: &ExportedOriginalTeletextDesign,
) -> TeletextDesignExportReceipt {
    fixture
        .state
        .application
        .teletext_design_export_receipt(
            exported.metadata().id,
            policy().project.max_bytes,
            designs::budget().max_serialized_bytes,
        )
        .await
        .unwrap()
        .unwrap()
}

async fn setup() -> (Fixture, Flow) {
    setup_with_response(authoring_metadata::response).await
}

async fn setup_with_response(response: fn(&str, u16, u16) -> serde_json::Value) -> (Fixture, Flow) {
    Box::pin(setup_with_fixture(
        response,
        Fixture::new_persistent().await,
    ))
    .await
}

async fn setup_with_fixture(
    response: fn(&str, u16, u16) -> serde_json::Value,
    fixture: Fixture,
) -> (Fixture, Flow) {
    authoring_metadata::install(&fixture).await;
    for (channel, theme) in [(1, "day"), (2, "night")] {
        authoring_metadata::write(&fixture, theme, &response(theme, 124, 32));
        let profile = fixture.profile(theme, theme, "good").await;
        fixture.bind(channel, &profile, None).await;
    }
    let first = authoring_metadata::candidate(&fixture, 1).await;
    let second = authoring_metadata::candidate(&fixture, 2).await;
    assert_eq!(
        first.selection().bindings[0].addon_id,
        second.selection().bindings[0].addon_id
    );
    assert_ne!(
        first.selection().bindings[0].profile_id,
        second.selection().bindings[0].profile_id
    );
    let saved = [
        designs::save(&fixture, first, None).await,
        designs::save(&fixture, second, None).await,
    ];
    let originals = [
        original(&fixture, &saved[0]).await,
        original(&fixture, &saved[1]).await,
    ];
    let pending = [
        generations::request(&fixture, &saved[0], None).await,
        generations::request(&fixture, &saved[1], None).await,
    ];
    let accepted = [
        generations::accepted(generations::accept(&fixture, &pending[0], 1, None).await),
        generations::accepted(generations::accept(&fixture, &pending[1], 1, None).await),
    ];
    let exported = [
        export(&fixture, &saved[0]).await,
        export(&fixture, &saved[1]).await,
    ];
    let receipts = [
        receipt(&fixture, &exported[0]).await,
        receipt(&fixture, &exported[1]).await,
    ];
    let config = designs::config(&fixture).await;
    let calls = std::fs::read(fixture.root.path().join("authoring-calls")).unwrap();
    (
        fixture,
        Flow {
            saved,
            originals,
            exported,
            receipts,
            pending,
            accepted,
            config,
            calls,
        },
    )
}

fn edit_cell(files: &mut [TeletextExportFile], name: &str, row: u8, column: usize, code: u8) {
    let file = files.iter_mut().find(|file| file.name == name).unwrap();
    let parsed = parse_teletext_tti(&file.bytes, policy().tti).unwrap();
    let mut records = parsed.records().to_vec();
    if let Some(TeletextTtiRecord::BaseRow { cells, .. }) = records.iter_mut().find(
        |record| matches!(record, TeletextTtiRecord::BaseRow { row: actual, .. } if *actual == row),
    ) {
        cells[column] = code;
    } else {
        let mut cells = [b' '; 40];
        cells[column] = code;
        let insertion = records
            .iter()
            .position(|record| {
                matches!(
                    record,
                    TeletextTtiRecord::Packet { .. } | TeletextTtiRecord::FastLinks(_)
                )
            })
            .unwrap_or(records.len());
        records.insert(insertion, TeletextTtiRecord::BaseRow { row, cells });
    }
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
}

fn request(flow: &Flow, row: u8, column: usize, code: u8) -> NativeTeletextEditorialImportRequest {
    let exported = &flow.exported[0];
    let mut files = exported.files().to_vec();
    edit_cell(
        &mut files,
        &exported.sidecar().project.pages[0].name,
        row,
        column,
        code,
    );
    NativeTeletextEditorialImportRequest {
        name: "Synthetic edited concept".to_owned(),
        receipt_id: exported.metadata().id,
        receipt_digest: exported.metadata().digest.clone(),
        expected_design_head: 1,
        sidecar: write_teletext_export_sidecar(exported.sidecar(), policy().project).unwrap(),
        files,
    }
}

async fn import(
    fixture: &Fixture,
    request: NativeTeletextEditorialImportRequest,
) -> SavedEditorialTeletextDesign {
    let result = import_teletext_editorial_design(
        &fixture.state.application,
        request,
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    let TeletextDesignMutation::Saved(saved) = result else {
        panic!("authorized synthetic Free-zone edit must save a concept");
    };
    saved
}

async fn assert_retained_storage(fixture: &Fixture, flow: &Flow) {
    for index in 0..2 {
        assert_eq!(
            original(fixture, &flow.saved[index]).await,
            flow.originals[index]
        );
        assert_eq!(
            receipt(fixture, &flow.exported[index]).await,
            flow.receipts[index]
        );
    }
    assert_eq!(
        std::fs::read(fixture.root.path().join("authoring-calls")).unwrap(),
        flow.calls
    );
    assert_eq!(designs::config(fixture).await, flow.config);
}

async fn recover(fixture: &Fixture, channel: i64) -> AcceptedTeletextOutputCandidate {
    reload_accepted_teletext_generation(
        &fixture.state.application,
        channel,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap()
}

async fn assert_original_pointers(fixture: &Fixture, flow: &Flow) {
    for index in 0..2 {
        let channel = i64::try_from(index + 1).unwrap();
        assert_eq!(
            fixture
                .state
                .application
                .teletext_publication_request(channel)
                .await
                .unwrap(),
            Some(flow.pending[index].clone())
        );
        assert_eq!(
            fixture
                .state
                .application
                .accepted_teletext_generation(channel)
                .await
                .unwrap(),
            Some(flow.accepted[index].generation().clone())
        );
        generations::assert_same_output(&recover(fixture, channel).await, &flow.accepted[index]);
    }
    assert_retained_storage(fixture, flow).await;
}

fn assert_rgb(raster: &TeletextRaster, expected: [u8; 3]) {
    assert!(raster.diagnostics().is_empty());
    assert!((144..156).any(|x| {
        (10..20).any(|y| {
            matches!(raster.pixel(x, y),
        Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(rgb))) if rgb.get() == expected)
        })
    }));
}

async fn assert_concept_preview(
    fixture: &Fixture,
    flow: &Flow,
    edited: &SavedEditorialTeletextDesign,
    edited_row: u8,
) -> TeletextRaster {
    let restored = reload_original_teletext_design_revision(
        &fixture.state.application,
        edited.head().id,
        edited.head().revision,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    let original = flow.saved[0].candidate();
    assert_eq!(restored.digest(), edited.digest());
    assert_eq!(restored.selection(), original.selection());
    assert_eq!(restored.metadata(), original.metadata());
    assert_eq!(restored.transmissions(), original.transmissions());
    for (source, expected) in restored.sources().iter().zip(original.sources()) {
        assert_eq!(source.bytes(), expected.bytes());
        assert_eq!(source.authoring(), expected.authoring());
    }
    assert_eq!(
        restored.producers()[0].original(),
        original.producers()[0].original()
    );
    let encoded = restored.encoded().unwrap();
    assert_ne!(
        designs::units(encoded),
        designs::units(original.encoded().unwrap())
    );
    assert_eq!(
        encoded.magazine_units(),
        original.encoded().unwrap().magazine_units()
    );
    let page = encoded
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::NativeDisplay(page) => Some(page.encoded().page()),
            EncodedTeletextAuthoredPage::CompiledDisplay(page) => Some(page.page()),
            _ => None,
        })
        .unwrap();
    assert!(project_encoded_teletext_display(page, 128).unwrap().records.iter().any(|record|
        matches!(record, TeletextEncodedDisplayRecord::BaseRow { row, cells } if *row == edited_row && cells[8] == b'Q')));
    let preview = project_teletext_preview(
        encoded,
        channel_documents::address(124, 32),
        TeletextColourPixelOptions {
            reveal_concealed: false,
            flash_phase: TeletextFlashPhase::Original,
        },
        preview_budget(),
    )
    .unwrap();
    assert_eq!(
        preview.navigation(),
        original.producers()[0].original().semantic().base().pages[0].navigation
    );
    assert_ne!(
        preview.raster(),
        generations::preview(original.encoded().unwrap()).raster()
    );
    assert_rgb(preview.raster(), [15, 0, 0]);
    assert_rgb(
        generations::preview(flow.saved[1].candidate().encoded().unwrap()).raster(),
        [0, 0, 15],
    );
    preview.raster().clone()
}

fn preview_budget() -> ersatzrs_web::host::teletext_preview::TeletextPreviewBudget {
    ersatzrs_web::host::teletext_preview::TeletextPreviewBudget {
        max_pages: 16,
        max_model_records: 32_768,
        max_pixels: 168_000,
        max_diagnostics: 1400,
        max_work: 50_000_000,
    }
}

async fn publish_and_accept(
    fixture: &Fixture,
    flow: &Flow,
    edited: &SavedEditorialTeletextDesign,
) -> AcceptedTeletextOutputCandidate {
    use ersatzrs_core::repositories::TeletextPublicationMutation;
    let pending =
        ersatzrs_web::host::teletext_designs::request_original_teletext_design_publication(
            &fixture.state.application,
            edited.head().id,
            edited.head().revision,
            edited.digest().to_owned(),
            2,
            Some(flow.pending[0].id),
            designs::budget(),
            channel_documents::policy(),
            designs::deadline(),
        )
        .await
        .unwrap();
    let TeletextPublicationMutation::Requested(pending) = pending else {
        panic!("checked concept must admit explicit publication intent");
    };
    // Pending intent still leaves the exact old accepted output in place.
    assert_eq!(
        fixture
            .state
            .application
            .accepted_teletext_generation(1)
            .await
            .unwrap(),
        Some(flow.accepted[0].generation().clone())
    );
    generations::accepted(
        accept_original_teletext_generation(
            &fixture.state.application,
            TeletextGenerationTarget {
                request: pending.request().clone(),
                expected_design_head: 2,
                expected_generation_id: Some(flow.accepted[0].generation().id),
            },
            designs::budget(),
            channel_documents::policy(),
            designs::deadline(),
        )
        .await
        .unwrap(),
    )
}

#[tokio::test]
async fn actual_v3_editor_concept_preview_explicit_publication_reopen_and_channel_isolation() {
    let (mut fixture, flow) = setup().await;
    let edited = import(&fixture, request(&flow, 2, 8, b'Q')).await;
    assert_eq!(edited.head().id, flow.saved[0].head().id);
    assert_eq!(edited.head().revision, 2);
    assert_ne!(edited.digest(), flow.saved[0].digest());
    assert_original_pointers(&fixture, &flow).await;
    let raster = assert_concept_preview(&fixture, &flow, &edited, 2).await;
    fixture = fixture.reopen().await;
    assert_original_pointers(&fixture, &flow).await;
    assert_eq!(
        assert_concept_preview(&fixture, &flow, &edited, 2).await,
        raster
    );
    let accepted = publish_and_accept(&fixture, &flow, &edited).await;
    assert_ne!(accepted.generation().id, flow.accepted[0].generation().id);
    assert_eq!(generations::preview(accepted.service()).raster(), &raster);
    fixture = fixture.reopen().await;
    generations::assert_same_output(&recover(&fixture, 1).await, &accepted);
    generations::assert_same_output(&recover(&fixture, 2).await, &flow.accepted[1]);
    assert_retained_storage(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn actual_zone_edit_reuses_immutable_original_receipt_with_fresh_head() {
    let (mut fixture, flow) = setup().await;
    let first = import(&fixture, request(&flow, 2, 8, b'Q')).await;
    assert_eq!(first.head().revision, 2);
    let mut fresh = request(&flow, 3, 8, b'Q');
    fresh.expected_design_head = 2;
    let zone = import(&fixture, fresh).await;
    assert_eq!(zone.head().revision, 3);
    assert_ne!(zone.digest(), first.digest());
    assert_original_pointers(&fixture, &flow).await;
    let preview = assert_concept_preview(&fixture, &flow, &zone, 3).await;
    fixture = fixture.reopen().await;
    assert_original_pointers(&fixture, &flow).await;
    assert_eq!(
        assert_concept_preview(&fixture, &flow, &zone, 3).await,
        preview
    );
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[derive(Clone, Copy, Debug)]
enum Tamper {
    Theme,
    Region,
    ReceiptId,
    ReceiptDigest,
    FileIdentity,
    PageIdentity,
    Asset,
    MissingAsset,
    Outside,
    Generated,
    SpacingEscape,
    StaleHead,
}

fn tampered(flow: &Flow, kind: Tamper) -> NativeTeletextEditorialImportRequest {
    let mut request = request(flow, 2, 8, b'Q');
    let page_name = &flow.exported[0].sidecar().project.pages[0].name;
    match kind {
        Tamper::Theme | Tamper::Region => {
            let mut sidecar: serde_json::Value = serde_json::from_slice(&request.sidecar).unwrap();
            if matches!(kind, Tamper::Theme) {
                sidecar["project"]["producers"][0]["authoring"]["theme"]["id"] =
                    serde_json::json!("forged-theme");
            } else {
                sidecar["project"]["producers"][0]["authoring"]["pages"][0]["regions"][0]
                    ["width"] = serde_json::json!(30);
            }
            request.sidecar = serde_json::to_vec(&sidecar).unwrap();
        }
        Tamper::ReceiptId => request.receipt_id = i64::MAX,
        Tamper::ReceiptDigest => request.receipt_digest = "0".repeat(64),
        Tamper::FileIdentity => {
            "foreign.tti".clone_into(
                &mut request
                    .files
                    .iter_mut()
                    .find(|file| &file.name == page_name)
                    .unwrap()
                    .name,
            );
        }
        Tamper::PageIdentity => {
            let file = request
                .files
                .iter_mut()
                .find(|file| &file.name == page_name)
                .unwrap();
            let mut records = parse_teletext_tti(&file.bytes, policy().tti)
                .unwrap()
                .records()
                .to_vec();
            for record in &mut records {
                if let TeletextTtiRecord::PageNumber { page, .. } = record {
                    *page = 0x125;
                }
            }
            file.bytes = write_teletext_tti(
                &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
                policy().tti,
            )
            .unwrap();
        }
        Tamper::Asset => {
            let name = &flow.exported[0].sidecar().project.assets[0].name;
            request
                .files
                .iter_mut()
                .find(|file| &file.name == name)
                .unwrap()
                .bytes[0] ^= 1;
        }
        Tamper::MissingAsset => {
            let name = &flow.exported[0].sidecar().project.assets[0].name;
            request.files.retain(|file| &file.name != name);
        }
        Tamper::Outside => edit_cell(&mut request.files, page_name, 2, 25, b'X'),
        Tamper::Generated => edit_cell(&mut request.files, page_name, 4, 8, b'X'),
        Tamper::SpacingEscape => edit_cell(&mut request.files, page_name, 3, 19, 0x02),
        Tamper::StaleHead => request.expected_design_head = 2,
    }
    request
}

#[tokio::test]
async fn editorial_tampering_and_protected_effects_preserve_original_pointers_and_storage() {
    let (mut fixture, flow) = setup().await;
    for kind in [
        Tamper::Theme,
        Tamper::Region,
        Tamper::ReceiptId,
        Tamper::ReceiptDigest,
        Tamper::FileIdentity,
        Tamper::PageIdentity,
        Tamper::Asset,
        Tamper::MissingAsset,
        Tamper::Outside,
        Tamper::Generated,
        Tamper::SpacingEscape,
        Tamper::StaleHead,
    ] {
        let result = import_teletext_editorial_design(
            &fixture.state.application,
            tampered(&flow, kind),
            policy(),
            designs::deadline(),
        )
        .await;
        assert!(
            matches!(result, Err(_) | Ok(TeletextDesignMutation::Rejected(_))),
            "{kind:?} must not save a concept"
        );
        assert_original_pointers(&fixture, &flow).await;
        for saved in &flow.saved {
            assert_eq!(
                fixture
                    .state
                    .application
                    .teletext_design_head(saved.head().id)
                    .await
                    .unwrap()
                    .unwrap()
                    .revision,
                1
            );
        }
    }
    fixture = fixture.reopen().await;
    assert_original_pointers(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

/// Opt-in file handshake for the separately operated pinned headed editor.
/// This probe never launches the editor or manufactures its saved TTI bytes.
mod external_editor_probe {
    use super::super::{client_capture, ordinary_stream_flow};
    use super::*;
    use std::{io, path::Path};

    const DIRECTORY_ENV: &str = "ERSATZRS_TELETEXT_QTM_INTERCHANGE_DIR";
    const EDITOR_COMMIT: &str = "cac2167bac933a6160cca45a45d5d4ee95355898";

    fn hash(bytes: &[u8]) -> String {
        use sha2::Digest as _;
        lower_hex(&sha2::Sha256::digest(bytes))
    }

    fn lower_hex(bytes: &[u8]) -> String {
        use std::fmt::Write as _;
        let mut text = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            write!(text, "{byte:02x}").unwrap();
        }
        text
    }

    fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
        use std::io::Write as _;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        file.write_all(bytes)
    }

    fn read_bounded(path: &Path, maximum: usize) -> io::Result<Vec<u8>> {
        use std::io::Read as _;
        let metadata = std::fs::symlink_metadata(path)?;
        if !metadata.file_type().is_file() || metadata.len() > u64::try_from(maximum).unwrap() {
            return Err(io::Error::other(
                "editor probe input is not a bounded regular file",
            ));
        }
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take(u64::try_from(maximum).unwrap() + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > maximum {
            return Err(io::Error::other(
                "editor probe input exceeded its read limit",
            ));
        }
        Ok(bytes)
    }

    pub(super) fn editor_response(theme: &str, page: u16, subcode: u16) -> serde_json::Value {
        assert_eq!((page, subcode), (124, 32));
        let mut response = authoring_metadata::response(theme, page, subcode);
        // Pinned editor load couples its X28 NOS to header controls. The
        // separate positive fixture uses unambiguous zero on both planes;
        // actual-1 retains the nonzero loss/rejection as open fidelity evidence.
        response["service"]["pages"][0]["national_option"] = serde_json::json!(0);
        response["service"]["magazines"][0]["presentation"]["primary"] = serde_json::json!(0);
        let mut second = response.clone();
        let owner = serde_json::json!({"page":125,"subcode":32});
        second["service"]["pages"][0]["address"] = owner.clone();
        second["service"]["page_enhancements"][0]["address"] = owner.clone();
        second["service"]["mot_selections"][0]["display"] = owner.clone();
        second["authoring"]["pages"][0]["page"] = owner;
        response["service"]["pages"][0]["navigation"][0]["target"]["page"] = serde_json::json!(125);
        second["service"]["pages"][0]["navigation"][0]["target"]["page"] = serde_json::json!(124);
        for field in ["pages", "page_enhancements", "mot_selections"] {
            response["service"][field]
                .as_array_mut()
                .unwrap()
                .push(second["service"][field][0].take());
        }
        response["authoring"]["pages"]
            .as_array_mut()
            .unwrap()
            .push(second["authoring"]["pages"][0].take());
        response
    }

    fn editor_geometry(flow: &Flow, index: usize, page: u16, subcode: u16) -> serde_json::Value {
        let owner = channel_documents::address(page, subcode);
        let composition = flow.saved[index]
            .candidate()
            .encoded()
            .unwrap()
            .pages()
            .iter()
            .find_map(|page| match page {
                EncodedTeletextAuthoredPage::NativeDisplay(page)
                    if page.composition().owner() == owner =>
                {
                    Some(page.composition())
                }
                EncodedTeletextAuthoredPage::CompiledDisplay(page)
                    if page.page().address() == owner =>
                {
                    Some(page.normalized().original())
                }
                _ => None,
            })
            .unwrap();
        let geometry = composition.display_geometry();
        serde_json::json!({"left_columns":geometry.left_width(),
            "right_columns":geometry.right_width(),"total_columns":geometry.total_width(),
            "main_origin_column":geometry.left_width(),"target_visual_column":geometry.left_width()+8})
    }

    fn write_ready(directory: &Path, flow: &Flow) -> io::Result<()> {
        let mut channels = Vec::new();
        for (index, exported) in flow.exported.iter().enumerate() {
            let prefix = format!("channel-{}", index + 1);
            let channel = directory.join(&prefix);
            std::fs::create_dir(&channel)?;
            let sidecar =
                write_teletext_export_sidecar(exported.sidecar(), policy().project).unwrap();
            write_new(&channel.join("project.ersatzrs.json"), &sidecar)?;
            let mut files = Vec::new();
            for file in exported.files() {
                write_new(&channel.join(&file.name), &file.bytes)?;
                files.push(serde_json::json!({"name":file.name,"bytes":file.bytes.len(),"sha256":hash(&file.bytes)}));
            }
            let page = &exported.sidecar().project.pages[0];
            channels.push(serde_json::json!({"channel_id":index+1,
                "profile_id":exported.sidecar().project.producers[0].profile_id,
                "theme":exported.sidecar().project.producers[0].authoring.as_ref().unwrap().theme.id,
                "design_id":flow.saved[index].head().id,"original_revision":1,
                "original_digest":flow.saved[index].digest(),
                "receipt_id":exported.metadata().id,"receipt_digest":exported.metadata().digest,
                "sidecar":format!("{prefix}/project.ersatzrs.json"),"sidecar_sha256":hash(&sidecar),
                "page":{"page":page.page.page,"subcode":page.page.subcode,"editor_page":page.editor_page,"editor_ordinal":page.editor_ordinal},
                "editor_input":format!("{prefix}/{}",page.name),
                "editor_geometry":editor_geometry(flow,index,page.page.page,page.page.subcode),
                "pages":exported.sidecar().project.pages,"files":files}));
        }
        let ready = serde_json::json!({"probe_version":1,"channels":channels,
            "edit":{"channel_id":1,"row":2,"column":8,"code":81},
            "save_as":"edited.tti","completion":"editor-saved.json",
            "editor":{"name":"QTeletextMaker","version":"1.0","source_commit":EDITOR_COMMIT},
            "publication":"only after explicit native test request and acceptance"});
        write_new(
            &directory.join("ready.json"),
            &serde_json::to_vec_pretty(&ready).unwrap(),
        )
    }

    #[derive(serde::Deserialize, serde::Serialize)]
    #[serde(deny_unknown_fields)]
    struct EditorSaved {
        probe_version: u16,
        editor_name: String,
        editor_version: String,
        editor_source_commit: String,
        editor_binary_sha256: String,
        saved_file: String,
    }

    fn read_completion(directory: &Path) -> io::Result<EditorSaved> {
        let bytes = read_bounded(&directory.join("editor-saved.json"), 4096)?;
        let marker: EditorSaved = serde_json::from_slice(&bytes)
            .map_err(|_| io::Error::other("editor completion does not match its schema"))?;
        if marker.probe_version != 1
            || marker.editor_name != "QTeletextMaker"
            || marker.editor_version != "1.0"
            || marker.editor_source_commit != EDITOR_COMMIT
            || marker.saved_file != "edited.tti"
            || marker.editor_binary_sha256.len() != 64
            || !marker
                .editor_binary_sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        {
            return Err(io::Error::other(
                "editor completion does not match the pinned probe",
            ));
        }
        Ok(marker)
    }

    async fn wait_for_editor(directory: &Path) -> io::Result<EditorSaved> {
        let end = tokio::time::Instant::now() + Duration::from_mins(10);
        tokio::time::timeout_at(end, async {
            loop {
                if tokio::fs::try_exists(directory.join("editor-saved.json")).await? {
                    return read_completion(directory);
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .map_err(|_| io::Error::other("external editor did not complete within 600 seconds"))?
    }

    fn read_editor_request(
        directory: &Path,
        flow: &Flow,
    ) -> io::Result<NativeTeletextEditorialImportRequest> {
        for (index, exported) in flow.exported.iter().enumerate() {
            let channel = directory.join(format!("channel-{}", index + 1));
            let sidecar = read_bounded(
                &channel.join("project.ersatzrs.json"),
                policy().project.max_bytes,
            )?;
            if sidecar
                != write_teletext_export_sidecar(exported.sidecar(), policy().project).unwrap()
            {
                return Err(io::Error::other("editor changed protected sidecar bytes"));
            }
            for file in exported.files() {
                if read_bounded(&channel.join(&file.name), policy().project.max_file_bytes)?
                    != file.bytes
                {
                    return Err(io::Error::other(
                        "editor changed protected original project files",
                    ));
                }
            }
        }
        let exported = &flow.exported[0];
        let page_name = &exported.sidecar().project.pages[0].name;
        let mut files = Vec::new();
        for file in exported.files() {
            let bytes = if &file.name == page_name {
                read_bounded(&directory.join("edited.tti"), policy().tti.max_bytes)?
            } else {
                read_bounded(
                    &directory.join("channel-1").join(&file.name),
                    policy().project.max_file_bytes,
                )?
            };
            files.push(TeletextExportFile {
                name: file.name.clone(),
                bytes,
            });
        }
        Ok(NativeTeletextEditorialImportRequest {
            name: "Synthetic actual QTeletextMaker edit".to_owned(),
            receipt_id: exported.metadata().id,
            receipt_digest: exported.metadata().digest.clone(),
            expected_design_head: 1,
            sidecar: read_bounded(
                &directory.join("channel-1/project.ersatzrs.json"),
                policy().project.max_bytes,
            )?,
            files,
        })
    }

    async fn checked_external_request(
        directory: &Path,
        fixture: &Fixture,
        flow: &Flow,
    ) -> (EditorSaved, NativeTeletextEditorialImportRequest) {
        let result = async {
            let marker = wait_for_editor(directory).await?;
            let request = read_editor_request(directory, flow)?;
            Ok::<_, io::Error>((marker, request))
        }
        .await;
        match result {
            Ok(value) => value,
            Err(error) => {
                assert_original_pointers(fixture, flow).await;
                write_new(
                    &directory.join("failed.json"),
                    &serde_json::to_vec_pretty(&serde_json::json!({
                    "probe_version":1,"stage":"external_editor_files","old_pointers_preserved":true,
                    "failure":error.to_string()}))
                    .unwrap(),
                )
                .unwrap();
                panic!("external editor probe failed before import: {error}");
            }
        }
    }

    async fn checked_external_import(
        directory: &Path,
        fixture: &Fixture,
        flow: &Flow,
        request: NativeTeletextEditorialImportRequest,
    ) -> SavedEditorialTeletextDesign {
        match import_teletext_editorial_design(
            &fixture.state.application,
            request,
            policy(),
            designs::deadline(),
        )
        .await
        {
            Ok(TeletextDesignMutation::Saved(saved)) => saved,
            other => {
                assert_original_pointers(fixture, flow).await;
                let failure = match other {
                    Err(error) => error.to_string(),
                    Ok(TeletextDesignMutation::Rejected(reason)) => format!("{reason:?}"),
                    Ok(TeletextDesignMutation::Saved(_)) => unreachable!(),
                };
                write_new(&directory.join("failed.json"), &serde_json::to_vec_pretty(&serde_json::json!({
                    "probe_version":1,"stage":"native_import","old_pointers_preserved":true,"failure":failure})).unwrap()).unwrap();
                panic!("actual editor result failed native import");
            }
        }
    }

    fn raster_hash(raster: &TeletextRaster) -> String {
        use sha2::Digest as _;
        let mut digest = sha2::Sha256::new();
        for pixel in raster.pixels() {
            match pixel {
                TeletextRasterPixel::ReceiverHeader => digest.update([0]),
                TeletextRasterPixel::Unresolved(index) => {
                    digest.update([1]);
                    digest.update(u64::try_from(*index).unwrap().to_le_bytes());
                }
                TeletextRasterPixel::Colour(TeletextColourPixel::Video) => digest.update([2]),
                TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(rgb)) => {
                    digest.update([3]);
                    digest.update(rgb.get());
                }
            }
        }
        lower_hex(&digest.finalize())
    }

    struct StreamTools {
        ffmpeg: String,
        ffprobe: String,
        tsanalyze: String,
    }

    impl StreamTools {
        async fn load() -> Self {
            let tools = Self {
                ffmpeg: client_capture::selected("ERSATZRS_TELETEXT_TEST_FFMPEG"),
                ffprobe: client_capture::selected("FFPROBE_PATH"),
                tsanalyze: client_capture::selected("ERSATZRS_TELETEXT_TEST_TSANALYZE"),
            };
            client_capture::check_tools(&tools.ffmpeg, &tools.ffprobe).await;
            tools
        }
    }

    async fn prepare_editor_playback_fixture(
        directory: &Path,
        tools: &StreamTools,
    ) -> (Fixture, std::path::PathBuf) {
        let fixture = Fixture::new_persistent().await;
        let source_path = client_capture::generate_source(&tools.ffmpeg, fixture.root.path()).await;
        client_capture::configure_source_for_channels(&fixture, &source_path, &[1, 2]).await;
        ordinary_stream_flow::enable_output_policy(&fixture).await;
        let source = read_bounded(&source_path, 8 * 1024 * 1024).unwrap();
        write_new(&directory.join("source.ts"), &source).unwrap();
        (fixture, source_path)
    }

    fn preview_page(
        service: &ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService,
        page: u16,
    ) -> TeletextRaster {
        let preview = project_teletext_preview(
            service,
            channel_documents::address(page, 32),
            TeletextColourPixelOptions {
                reveal_concealed: false,
                flash_phase: TeletextFlashPhase::Original,
            },
            preview_budget(),
        )
        .unwrap();
        assert!(preview.raster().diagnostics().is_empty());
        assert_eq!(
            preview.navigation()[0].target.page.get(),
            if page == 124 { 125 } else { 124 }
        );
        preview.raster().clone()
    }

    async fn decoded_delivery_pages(
        directory: &Path,
        tools: &StreamTools,
        bytes: &[u8],
        channel: i64,
        service: &ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService,
    ) -> Vec<serde_json::Value> {
        let mut pages = Vec::new();
        for page in [124, 125] {
            let text =
                client_capture::decode_page_text(&tools.ffmpeg, bytes, &page.to_string()).await;
            let filename = format!("channel-{channel}-page-{page}.srt");
            write_new(&directory.join(&filename), text.as_bytes()).unwrap();
            assert!(
                text.contains("AZ"),
                "actual decode must retain the original enhanced row"
            );
            let edited = channel == 1 && page == 124;
            assert_eq!(
                text.contains('Q'),
                edited,
                "actual decoder must observe Q only on the edited page/channel"
            );
            let preview = preview_page(service, page);
            pages.push(serde_json::json!({"page":page,"subcode":32,
                "decoded_text":filename,"decoded_sha256":hash(text.as_bytes()),
                "edited_q_present":edited,"preview_sha256":raster_hash(&preview),
                "navigation_target":if page==124 {125} else {124}}));
        }
        pages
    }

    async fn verify_delivery(
        directory: &Path,
        tools: &StreamTools,
        source: &[u8],
        bytes: &[u8],
        channel: i64,
        accepted: &AcceptedTeletextOutputCandidate,
    ) -> serde_json::Value {
        let filename = format!("channel-{channel}.ts");
        // Retain exact route bytes if an independent check discovers a defect;
        // the later proof claims success only after every check has passed.
        write_new(&directory.join(&filename), bytes).unwrap();
        client_capture::verify_transport(&tools.tsanalyze, &tools.ffprobe, bytes).await;
        client_capture::verify_av(&tools.ffmpeg, source, bytes).await;
        ordinary_stream_flow::assert_emitted_model(bytes, accepted.service());
        let pages =
            decoded_delivery_pages(directory, tools, bytes, channel, accepted.service()).await;
        serde_json::json!({"channel_id":channel,"capture":filename,"bytes":bytes.len(),
            "sha256":hash(bytes),"generation_id":accepted.generation().id,
            "request_id":accepted.generation().request.id,"revision":accepted.generation().request.revision,
            "theme_rgb4":if channel==1 {[15,0,0]} else {[0,0,15]},
            "tsduck_transport_passed":true,"source_av_passed":true,"complete_emitted_model_equal":true,
            "pages":pages})
    }

    async fn capture_edited_native_outputs(
        directory: &Path,
        fixture: &mut Fixture,
        flow: &Flow,
        edited: &AcceptedTeletextOutputCandidate,
        tools: &StreamTools,
        source_path: &Path,
    ) -> serde_json::Value {
        let source = read_bounded(source_path, 8 * 1024 * 1024).unwrap();
        assert_eq!(
            read_bounded(&directory.join("source.ts"), 8 * 1024 * 1024).unwrap(),
            source
        );
        // Source/policy preparation preceded the original captured selections.
        // Only the existing disposable schedule's start is reset after the
        // bounded editor wait; no selection/configuration fence is relaxed.
        ordinary_stream_flow::reset_source_start(fixture).await;
        let (first, second) = tokio::join!(
            ordinary_stream_flow::capture(fixture, 1),
            ordinary_stream_flow::capture(fixture, 2),
        );
        let day = verify_delivery(directory, tools, &source, &first, 1, edited).await;
        let night = verify_delivery(directory, tools, &source, &second, 2, &flow.accepted[1]).await;
        assert_eq!(
            preview_page(edited.service(), 125),
            preview_page(flow.saved[0].candidate().encoded().unwrap(), 125)
        );
        assert_rgb(&preview_page(edited.service(), 124), [15, 0, 0]);
        assert_rgb(&preview_page(flow.accepted[1].service(), 124), [0, 0, 15]);
        generations::assert_same_output(&recover(fixture, 1).await, edited);
        generations::assert_same_output(&recover(fixture, 2).await, &flow.accepted[1]);
        assert_retained_storage(fixture, flow).await;
        assert!(fixture.playout.try_recv().is_err());
        let proof = serde_json::json!({"probe_version":1,
            "route":"/iptv/hls-direct/{channel}.ts","source":"source.ts",
            "source_sha256":hash(&source),"source_bytes":source.len(),
            "muxrate_bps":client_capture::muxrate(),"channels":[day,night],
            "headed_client_navigation":"requires independent actual client proof for 124 to 125 to 124"});
        write_new(
            &directory.join("stream-proof.json"),
            &serde_json::to_vec_pretty(&proof).unwrap(),
        )
        .unwrap();
        proof
    }

    #[tokio::test]
    #[ignore = "requires a create-new disposable directory, separately operated pinned headed QTeletextMaker 1.0, FFmpeg/ffprobe9 and TSDuck"]
    async fn pinned_external_editor_files_import_preview_publication_and_reopen() {
        let directory =
            std::path::PathBuf::from(super::super::client_capture::selected(DIRECTORY_ENV));
        std::fs::create_dir(&directory).expect("probe destination must not already exist");
        let tools = StreamTools::load().await;
        let (playback_fixture, source_path) =
            prepare_editor_playback_fixture(&directory, &tools).await;
        let (mut fixture, flow) =
            Box::pin(setup_with_fixture(editor_response, playback_fixture)).await;
        write_ready(&directory, &flow).unwrap();
        let (marker, request) = checked_external_request(&directory, &fixture, &flow).await;
        let edited_hash = hash(
            &request
                .files
                .iter()
                .find(|file| file.name == flow.exported[0].sidecar().project.pages[0].name)
                .unwrap()
                .bytes,
        );
        let edited = checked_external_import(&directory, &fixture, &flow, request).await;
        assert_eq!(edited.head().id, flow.saved[0].head().id);
        assert_eq!(edited.head().revision, 2);
        assert_original_pointers(&fixture, &flow).await;
        let preview = assert_concept_preview(&fixture, &flow, &edited, 2).await;
        fixture = fixture.reopen().await;
        assert_original_pointers(&fixture, &flow).await;
        assert_eq!(
            assert_concept_preview(&fixture, &flow, &edited, 2).await,
            preview
        );
        let accepted = publish_and_accept(&fixture, &flow, &edited).await;
        assert_eq!(generations::preview(accepted.service()).raster(), &preview);
        fixture = fixture.reopen().await;
        generations::assert_same_output(&recover(&fixture, 1).await, &accepted);
        generations::assert_same_output(&recover(&fixture, 2).await, &flow.accepted[1]);
        assert_retained_storage(&fixture, &flow).await;
        assert!(fixture.playout.try_recv().is_err());
        let stream = capture_edited_native_outputs(
            &directory,
            &mut fixture,
            &flow,
            &accepted,
            &tools,
            &source_path,
        )
        .await;
        let success = serde_json::json!({"probe_version":1,"scope":"native application interchange and ordinary MPEG-TS flow",
            "editor_completion":marker,"edited_tti_sha256":edited_hash,
            "edited_design_id":edited.head().id,"edited_revision":edited.head().revision,"edited_digest":edited.digest(),
            "preview":{"width":preview.width(),"height":preview.height(),"diagnostics":preview.diagnostics().len(),"sha256":raster_hash(&preview)},
            "accepted_generation_id":accepted.generation().id,"accepted_request_id":accepted.generation().request.id,
            "stream":stream,
            "second_channel_unchanged":true,"originals_and_receipts_unchanged":true,
            "headed_editor_and_client_evidence":"must be attached independently"});
        write_new(
            &directory.join("success.json"),
            &serde_json::to_vec_pretty(&success).unwrap(),
        )
        .unwrap();
        fixture.state.pool.close().await;
    }
}
