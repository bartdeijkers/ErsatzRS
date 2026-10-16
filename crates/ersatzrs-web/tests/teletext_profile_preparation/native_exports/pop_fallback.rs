//! Receipt-qualified editor-only POP presentation and unpublished Main-row concepts.
//! Native X28/M29 absence and decoder-dependent secondary selection remain intact.

use super::*;
use ersatzrs_app::teletext_tti::{write_teletext_tti, TeletextTtiDocument};
use ersatzrs_domain::teletext::*;
use ersatzrs_ffmpeg::teletext::{
    EncodedTeletextAuthoredService, TeletextRaster, TeletextResourceFullUpdate,
    TeletextServiceResourceTransmission,
};
use ersatzrs_web::host::teletext_channel_documents::prepare_channel_teletext_document_candidate;
use ersatzrs_web::host::{
    teletext_designs::reload_original_teletext_design_revision,
    teletext_editorial_designs::{
        import_teletext_editorial_design, NativeTeletextEditorialImportError,
        NativeTeletextEditorialImportRequest,
    },
    teletext_preview::project_teletext_preview,
};
use std::{
    io::Read as _,
    path::{Path, PathBuf},
    time::Duration,
};

fn resource(kind: TeletextResourceKind, page: u8) -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        kind,
        TeletextRawDataPageNumber::new(1, page).unwrap(),
        TeletextResourceSubpageIndex::new(0).unwrap(),
    )
    .unwrap()
}

fn response() -> serde_json::Value {
    let mut value = authoring_metadata::response("day", 124, 32);
    let pop = json!({"kind":"pop","page":{"magazine":1,"page":160},"subpage":0});
    let mot = json!({"kind":"mot","page":{"magazine":1,"page":254},"subpage":0});
    let unused = json!({"kind":"unspecified","magazine":1});
    let empty = json!({"target":unused,"extent":0,"supersedable":false,
        "raw_fallback":0,"defaults":[{"kind":"no_object","raw_number":0},
            {"kind":"no_object","raw_number":0}]});
    let mut public = vec![empty.clone(); 7];
    public[0]["target"] = json!({"kind":"page","page":{"magazine":1,"page":160}});
    public[0]["raw_fallback"] = json!(14);
    let drcs = json!({"target":unused,"extent":0,"supersedable":false});
    value["service"]["magazines"] = json!([]);
    value["service"]["page_enhancements"][0]["presentation"] = serde_json::Value::Null;
    value["service"]["page_enhancements"][0]["local_program"] = serde_json::Value::Null;
    value["service"]["page_enhancements"][0]["resources"]["objects"]["public"] =
        json!({"magazine":1,"page":160});
    value["service"]["objects"] = json!([{
        "identity":{"scope":"public","resource":pop,"object_id":65535},
        "object_type":"passive","program":{"commands":[]}}]);
    value["service"]["mots"] = json!([{"identity":mot,
        "associations":[{"page":36,"objects":{"global":false,"public":1},
            "drcs":{"global":false,"public":null}}],
        "links":{"global_object":empty,"public_objects":public,
            "global_drcs":drcs,"normal_drcs":vec![drcs;7],
            "counts":{"magazine_mask":1,"object_pages":1,"drcs_pages":0}}}]);
    value["service"]["mot_selections"][0]["mot"] = mot;
    value
}

async fn prepared_export() -> (
    Fixture,
    SavedOriginalTeletextDesign,
    ExportedOriginalTeletextDesign,
) {
    let fixture = Fixture::new_persistent().await;
    authoring_metadata::install(&fixture).await;
    let value = response();
    let bytes = authoring_metadata::write(&fixture, "day", &value);
    let profile = fixture.profile("POP fallback export", "day", "good").await;
    fixture.bind(1, &profile, None).await;
    let mut export_policy = policy();
    export_policy.encoding.transmissions = [
        resource(TeletextResourceKind::Pop, 0xa0),
        resource(TeletextResourceKind::Mot, 0xfe),
    ]
    .into_iter()
    .map(|resource| TeletextServiceResourceTransmission {
        resource,
        counter: TeletextResourceUpdateCounter::new(1).unwrap(),
        update: TeletextResourceFullUpdate::Changed,
        last_subpage: true,
    })
    .collect();
    let candidate = prepare_channel_teletext_document_candidate(
        &fixture.state.application,
        1,
        Some(ADDON),
        &guide(&[]),
        export_policy.encoding.clone(),
    )
    .await
    .unwrap();
    assert_original_model(candidate.producers(), candidate.encoded().unwrap());
    assert_eq!(candidate.sources()[0].bytes(), bytes);
    let saved = designs::save(&fixture, candidate, None).await;
    let before = original(&fixture, &saved).await;
    let exported = export_original_teletext_design(
        &fixture.state.application,
        request(&saved, None),
        export_policy,
        designs::deadline(),
    )
    .await
    .unwrap();
    let TeletextDesignMutation::Saved(exported) = exported else {
        panic!("valid POP-dependent original must export without broadcast mutation");
    };
    assert_eq!(original(&fixture, &saved).await, before);
    (fixture, saved, exported)
}

fn assert_original_model(
    producers: &[TeletextProducerPresentation],
    encoded: &EncodedTeletextAuthoredService,
) {
    let snapshot = producers[0]
        .page(channel_documents::address(124, 32))
        .unwrap();
    let TeletextProducerPresentationSource::MotFallback { original_pop, .. } = snapshot.source()
    else {
        panic!("fixture must select its original associated POP fallback");
    };
    assert_eq!(original_pop.flags.get(), 14);
    assert_eq!(
        snapshot.effective_defaults().panels.level25_widths(),
        (8, 8)
    );
    assert!(snapshot.effective_defaults().black_background_substitution);
    assert_eq!(
        snapshot.secondary_selection(),
        TeletextProducerSecondarySelection::DecoderDependent
    );
    assert!(encoded.magazine_units().is_empty());
    let display = encoded
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::NativeDisplay(page) => Some(page.encoded().page()),
            EncodedTeletextAuthoredPage::CompiledDisplay(page) => Some(page.page()),
            _ => None,
        })
        .unwrap();
    let projection = project_encoded_teletext_display(display, 128).unwrap();
    assert!(!projection.records.iter().any(|record| matches!(
        record,
        TeletextEncodedDisplayRecord::Triplets {
            row: 28,
            designation: 0,
            ..
        }
    )));
}

fn assert_exported_presentation(body: &TeletextExportReceiptBody) {
    assert_eq!(body.project.pages.len(), 1);
    let mapping = &body.project.pages[0];
    assert_eq!(mapping.materialized_magazine, None);
    let tti = parse_teletext_tti(file(&body.files, &mapping.name), policy().tti).unwrap();
    // The derived editor packet must retain all fixed original presentation
    // values; its secondary sentinel belongs only to pinned QTM's editor policy.
    assert!(tti.records().iter().any(|record| matches!(record,
        TeletextTtiRecord::Packet { row:28, designation:0, .. })),
        "standalone TTI lacks the original POP-fallback panel presentation; a faithful editor projection is required without inventing secondary selection");
    assert_eq!(body.project.editor_profile, "qteletextmaker-1.0-carrier-v5");
    assert_eq!(
        serde_json::to_value(&mapping.editor_presentation).unwrap(),
        json!({
            "kind":"qtm10_original_pop_fallback_v1", "national_option":1, "pop_flags":14
        })
    );
    let words = tti
        .records()
        .iter()
        .find_map(|record| match record {
            TeletextTtiRecord::Packet {
                row: 28,
                designation: 0,
                words,
            } => Some(words),
            _ => None,
        })
        .unwrap();
    // EN300706 Tables4/11: primary1, QTM-only secondary7f, both panels8+8,
    // Level2.5 visibility and black-background substitution. Native stays absent.
    assert_eq!(words[0] & 0x7f, 0);
    assert_eq!((words[0] >> 7) & 0x7f, 1);
    assert_eq!((words[0] >> 14) | ((words[1] & 7) << 4), 0x7f);
    assert_eq!(words[1] & 0x3f8, 0x238);
    assert_eq!((words[12] >> 14) & 1, 1);
}

fn import_request(
    exported: &ExportedOriginalTeletextDesign,
    edited_bytes: Vec<u8>,
) -> NativeTeletextEditorialImportRequest {
    let mut files = exported.files().to_vec();
    let name = &exported.sidecar().project.pages[0].name;
    files
        .iter_mut()
        .find(|file| &file.name == name)
        .unwrap()
        .bytes = edited_bytes;
    NativeTeletextEditorialImportRequest {
        name: "Synthetic POP fallback concept".into(),
        receipt_id: exported.metadata().id,
        receipt_digest: exported.metadata().digest.clone(),
        expected_design_head: 1,
        sidecar: write_teletext_export_sidecar(exported.sidecar(), policy().project).unwrap(),
        files,
    }
}

fn prepared_q_edit(exported: &ExportedOriginalTeletextDesign) -> Vec<u8> {
    let mut files = exported.files().to_vec();
    let name = &exported.sidecar().project.pages[0].name;
    super::super::editorial_imports::edit_cell(&mut files, name, 2, 8, b'Q');
    file(&files, name).to_vec()
}

async fn assert_unpublished(
    fixture: &Fixture,
    saved: &SavedOriginalTeletextDesign,
    root: &TeletextDesignRevision,
    configuration: &[(Option<String>, Option<String>)],
    revision: i64,
) {
    assert_eq!(original(fixture, saved).await, *root);
    assert_eq!(
        fixture
            .state
            .application
            .teletext_design_head(saved.head().id)
            .await
            .unwrap()
            .unwrap()
            .revision,
        revision
    );
    let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "TeletextDesignRevision""#)
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap();
    assert_eq!(count, revision);
    assert!(fixture
        .state
        .application
        .teletext_publication_request(1)
        .await
        .unwrap()
        .is_none());
    assert!(fixture
        .state
        .application
        .accepted_teletext_generation(1)
        .await
        .unwrap()
        .is_none());
    assert_eq!(designs::config(fixture).await, configuration);
}

async fn assert_tampered_rejected(
    fixture: &Fixture,
    exported: &ExportedOriginalTeletextDesign,
    edited: &[u8],
    descriptor: bool,
) {
    let mut request = import_request(exported, edited.to_vec());
    if descriptor {
        let mut sidecar: serde_json::Value = serde_json::from_slice(&request.sidecar).unwrap();
        sidecar["project"]["pages"][0]["editor_presentation"]["pop_flags"] = json!(12);
        request.sidecar = serde_json::to_vec(&sidecar).unwrap();
    } else {
        let name = &exported.sidecar().project.pages[0].name;
        let page = request
            .files
            .iter_mut()
            .find(|file| &file.name == name)
            .unwrap();
        let mut records = parse_teletext_tti(&page.bytes, policy().tti)
            .unwrap()
            .records()
            .to_vec();
        let words = records
            .iter_mut()
            .find_map(|record| match record {
                TeletextTtiRecord::Packet {
                    row: 28,
                    designation: 0,
                    words,
                } => Some(words),
                _ => None,
            })
            .unwrap();
        words[1] ^= 8; // Valid triplet coding, changed protected left-panel flag.
        page.bytes = write_teletext_tti(
            &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
            policy().tti,
        )
        .unwrap();
    }
    let result = import_teletext_editorial_design(
        &fixture.state.application,
        request,
        policy(),
        designs::deadline(),
    )
    .await;
    if descriptor {
        assert!(
            matches!(result, Err(NativeTeletextEditorialImportError::Association)),
            "{result:?}"
        );
    } else {
        let Err(NativeTeletextEditorialImportError::Tti(error)) = result else {
            panic!("changed derived X28 must be rejected at its exact packet: {result:?}");
        };
        assert_eq!(error.failure,
            ersatzrs_web::host::teletext_editorial_designs::NativeTeletextEditorialTtiFailure::Packet);
        assert_eq!(
            (error.page, error.subcode, error.row, error.designation),
            (124, 32, Some(28), Some(0))
        );
    }
}

async fn restored_preview(
    fixture: &Fixture,
    saved: &SavedOriginalTeletextDesign,
    edited: &ersatzrs_web::host::teletext_editorial_designs::SavedEditorialTeletextDesign,
) -> TeletextRaster {
    let restored = reload_original_teletext_design_revision(
        &fixture.state.application,
        edited.head().id,
        2,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(restored.digest(), edited.digest());
    assert_eq!(restored.selection(), saved.candidate().selection());
    assert_eq!(
        restored.sources()[0].bytes(),
        saved.candidate().sources()[0].bytes()
    );
    assert_eq!(
        restored.sources()[0].authoring(),
        saved.candidate().sources()[0].authoring()
    );
    assert_original_model(restored.producers(), restored.encoded().unwrap());
    let owner = channel_documents::address(124, 32);
    let original = saved.candidate().encoded().unwrap();
    let encoded = restored.encoded().unwrap();
    assert_eq!(encoded.magazine_units(), original.magazine_units());
    assert_eq!(encoded.pages().len(), original.pages().len());
    for (before, after) in original.pages().iter().zip(encoded.pages()) {
        assert_eq!(before.resource_header(), after.resource_header());
        if before.resource_header().is_some() {
            assert_eq!(before.data_units(), after.data_units());
        }
    }
    let preview = |service| {
        project_teletext_preview(
            service,
            owner,
            TeletextColourPixelOptions {
                reveal_concealed: false,
                flash_phase: TeletextFlashPhase::Original,
            },
            super::super::editorial_imports::preview_budget(),
        )
        .unwrap()
        .raster()
        .clone()
    };
    let before = preview(original);
    let after = preview(encoded);
    assert_eq!((before.width(), before.height()), (672, 250));
    assert_eq!((after.width(), after.height()), (672, 250));
    assert!(after.diagnostics().is_empty());
    let mut changed = 0;
    for (index, (a, b)) in before.pixels().iter().zip(after.pixels()).enumerate() {
        if a != b {
            changed += 1;
            assert!((192..204).contains(&(index % 672)) && (20..30).contains(&(index / 672)));
        }
    }
    assert!(changed > 0);
    let display = encoded
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::NativeDisplay(page) => Some(page.encoded().page()),
            EncodedTeletextAuthoredPage::CompiledDisplay(page) => Some(page.page()),
            _ => None,
        })
        .unwrap();
    assert!(project_encoded_teletext_display(display,128).unwrap().records.iter().any(|record|
        matches!(record,TeletextEncodedDisplayRecord::BaseRow {row:2,cells} if cells[8]==b'Q')));
    after
}

const EDITOR_DIRECTORY: &str = "ERSATZRS_TELETEXT_POP_EDITOR_DIR";
#[expect(
    clippy::disallowed_methods,
    reason = "Optional create-new synthetic editor proof selection, outside product configuration"
)]
fn editor_directory() -> Option<PathBuf> {
    std::env::var_os(EDITOR_DIRECTORY).map(PathBuf::from)
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
fn write_new(path: &Path, bytes: &[u8]) {
    use std::io::Write as _;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .unwrap()
        .write_all(bytes)
        .unwrap();
}
fn read_bounded(path: &Path, maximum: usize) -> Vec<u8> {
    let metadata = std::fs::symlink_metadata(path).unwrap();
    assert!(metadata.file_type().is_file() && metadata.len() <= u64::try_from(maximum).unwrap());
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .unwrap()
        .take(u64::try_from(maximum).unwrap() + 1)
        .read_to_end(&mut bytes)
        .unwrap();
    assert!(bytes.len() <= maximum);
    bytes
}

async fn editor_files(
    exported: &ExportedOriginalTeletextDesign,
) -> (Vec<u8>, Option<(PathBuf, EditorSaved)>) {
    let Some(directory) = editor_directory() else {
        return (prepared_q_edit(exported), None);
    };
    std::fs::create_dir(&directory).expect("POP editor proof directory must be create-new");
    let original = directory.join("original");
    std::fs::create_dir(&original).unwrap();
    let sidecar = write_teletext_export_sidecar(exported.sidecar(), policy().project).unwrap();
    write_new(&original.join("project.ersatzrs.json"), &sidecar);
    for file in exported.files() {
        write_new(&original.join(&file.name), &file.bytes);
    }
    let mapping = &exported.sidecar().project.pages[0];
    let ready = json!({"probe_version":1,"editor_profile":exported.sidecar().project.editor_profile,
        "design_id":exported.sidecar().project.original.design_id,
        "receipt_id":exported.metadata().id,"receipt_digest":exported.metadata().digest,
        "input":format!("original/{}",mapping.name),"input_sha256":mapping.tti_sha256,
        "sidecar":"original/project.ersatzrs.json","sidecar_sha256":hash(&sidecar),
        "page":mapping.page,"editor_presentation":mapping.editor_presentation,
        "files":exported.files().iter().map(|file| json!({"name":file.name,
            "path":format!("original/{}",file.name),"bytes":file.bytes.len(),"sha256":hash(&file.bytes)})).collect::<Vec<_>>(),
        "editor_geometry":{"left_columns":8,"right_columns":8,"total_columns":56,
            "main_origin_column":8,"target_visual_column":16},
        "edit":{"row":2,"column":8,"code":81},"save_as":"edited.tti","completion":"editor-saved.json",
        "editor":{"name":"QTeletextMaker","version":"1.0","source_commit":"cac2167bac933a6160cca45a45d5d4ee95355898"}});
    write_new(
        &directory.join("ready.json"),
        &serde_json::to_vec_pretty(&ready).unwrap(),
    );
    tokio::time::timeout(Duration::from_mins(10), async {
        while !tokio::fs::try_exists(directory.join("editor-saved.json"))
            .await
            .unwrap()
        {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("pinned QTM must complete within600seconds");
    let marker: EditorSaved =
        serde_json::from_slice(&read_bounded(&directory.join("editor-saved.json"), 4096)).unwrap();
    assert_eq!(marker.probe_version, 1);
    assert_eq!(marker.editor_name, "QTeletextMaker");
    assert_eq!(marker.editor_version, "1.0");
    assert_eq!(
        marker.editor_source_commit,
        "cac2167bac933a6160cca45a45d5d4ee95355898"
    );
    assert_eq!(
        marker.editor_binary_sha256,
        "af7cb11220a411011917db0739f27a35b5c6ab725552b52b8a09bc01e296568a"
    );
    assert_eq!(marker.saved_file, "edited.tti");
    assert_eq!(
        read_bounded(
            &original.join("project.ersatzrs.json"),
            policy().project.max_bytes
        ),
        sidecar
    );
    for file in exported.files() {
        assert_eq!(
            read_bounded(&original.join(&file.name), policy().project.max_file_bytes),
            file.bytes
        );
    }
    let edited = read_bounded(&directory.join("edited.tti"), policy().tti.max_bytes);
    (edited, Some((directory, marker)))
}

#[tokio::test]
async fn pop_fallback_standalone_export_retains_effective_panel_presentation() {
    let (mut fixture, saved, exported) = prepared_export().await;
    let body = assert_receipt(&fixture, &exported).await;
    assert_exported_presentation(&body);
    let root = original(&fixture, &saved).await;
    let configuration = designs::config(&fixture).await;
    let (edited, editor) = editor_files(&exported).await;
    for descriptor in [false, true] {
        assert_tampered_rejected(&fixture, &exported, &edited, descriptor).await;
        assert_unpublished(&fixture, &saved, &root, &configuration, 1).await;
        assert_eq!(assert_receipt(&fixture, &exported).await, body);
    }
    let result = import_teletext_editorial_design(
        &fixture.state.application,
        import_request(&exported, edited.clone()),
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    let TeletextDesignMutation::Saved(concept) = result else {
        panic!("allowed Main-row Q must save an unpublished concept");
    };
    assert_eq!(
        (concept.head().id, concept.head().revision),
        (saved.head().id, 2)
    );
    let preview = restored_preview(&fixture, &saved, &concept).await;
    assert_unpublished(&fixture, &saved, &root, &configuration, 2).await;
    fixture = fixture.reopen().await;
    assert_eq!(restored_preview(&fixture, &saved, &concept).await, preview);
    assert_unpublished(&fixture, &saved, &root, &configuration, 2).await;
    assert_eq!(assert_receipt(&fixture, &exported).await, body);
    assert!(fixture.playout.try_recv().is_err());
    if let Some((directory, marker)) = editor {
        write_new(&directory.join("success.json"),&serde_json::to_vec_pretty(&json!({
            "probe_version":1,"scope":"pinned QTM POP-fallback editor-only export/import/concept/preview; no publication or stream claim",
            "editor_completion":marker,"edited_tti_sha256":hash(&edited),
            "concept_design_id":concept.head().id,"concept_revision":2,"concept_digest":concept.digest(),
            "preview":{"width":preview.width(),"height":preview.height(),"diagnostics":preview.diagnostics().len()},
            "native_x28_absent":true,"native_m29_absent":true,"native_secondary":"decoder_dependent",
            "tampered_x28_rejected":true,"tampered_descriptor_rejected":true,
            "original_and_receipt_retained":true,"generation_and_config_unchanged":true,"reopen_preview_equal":true
        })).unwrap());
    }
    fixture.state.pool.close().await;
}
