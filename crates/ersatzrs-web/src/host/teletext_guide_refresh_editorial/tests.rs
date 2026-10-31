//! Synthetic native rebase/effects proof; durable seed authentication is separate.
use super::*;
use crate::host::{
    teletext_channel_documents::{
        encode_original_producer_documents_with_editorial_resources, EditorialResourceDocuments,
        EncodedChannelTeletextOriginals,
    },
    teletext_runtime_policy::{
        parse_native_teletext_authoring_input, parse_native_teletext_authoring_policy,
    },
};
use ersatzrs_app::teletext_design_documents::{
    decode_teletext_design_documents, TeletextRestoredDesignDocumentParts,
    TeletextSavedEditorialProvenance,
};
use ersatzrs_domain::*;
use serde_json::{json, Value};
use std::time::Duration;

const ADDON: &str = "org.ersatzrs.synthetic-refresh-editorial";
fn owner() -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(200).unwrap(),
        subcode: TeletextSubcode::new(0).unwrap(),
    }
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(30)
}
fn policies() -> (
    TeletextDesignDocumentBudget,
    TeletextChannelDocumentEncodingPolicy,
) {
    let mut value: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/teletext-output-policy-v1.json"
    ))
    .unwrap();
    value.as_object_mut().unwrap().remove("source");
    value.as_object_mut().unwrap().remove("recovery_timeout_ms");
    value["operation_timeout_ms"] = json!(30_000);
    value["preview"] = json!({"max_pages":16,"max_model_records":32_768,"max_pixels":168_000,"max_diagnostics":1400,"max_work":50_000_000});
    let policy = parse_native_teletext_authoring_policy(&value.to_string()).unwrap();
    let input = parse_native_teletext_authoring_input(&json!({"input_version":1,"channel_id":1,"entry_addon_id":ADDON,"entry":owner(),"v1_region":"western_latin","resource_transmissions":[]}).to_string(), &policy).unwrap();
    (policy.design, policy.encoding_policy(&input).unwrap())
}
fn authoring() -> Value {
    json!({"schema_version":1,"template":{"id":"tvguide","revision":"1"},"theme":{"id":"classic","overrides":[]},"pages":[{"page":owner(),"regions":[
        {"id":"header","kind":"zone","area":"main","row":1,"column":2,"width":36,"height":1},
        {"id":"title","kind":"generated","area":"main","row":3,"column":2,"width":36,"height":1},
        {"id":"custom","kind":"free","area":"main","row":21,"column":2,"width":36,"height":1},
        {"id":"footer","kind":"zone","area":"main","row":22,"column":2,"width":36,"height":1}],
        "dynamic_fields":[{"id":"title","region":"title","binding":"guide.index"}]}]})
}
fn parts(title: &str, custom: &str, metadata: &Value) -> TeletextRestoredDesignDocumentParts {
    let rows: Vec<_> = [(1, "£HEADER"), (3, title), (21, custom), (22, "FOOTER")]
        .into_iter()
        .map(|(row, text)| {
            let mut runs = vec![TeletextRun {
                column: 2,
                style: TeletextStyle::default(),
                content: TeletextRunContent::Text {
                    text: text.to_owned(),
                },
            }];
            if row == 21 {
                // Retained producer controls bound editable flash provenance:
                // Flash35, then Steady37 before the protected columns38..39.
                runs.extend([
                    TeletextRun {
                        column: 36,
                        style: TeletextStyle {
                            flash: true,
                            ..Default::default()
                        },
                        content: TeletextRunContent::Text { text: " ".into() },
                    },
                    TeletextRun {
                        column: 38,
                        style: TeletextStyle::default(),
                        content: TeletextRunContent::Text { text: " ".into() },
                    },
                ]);
            }
            TeletextRow { row, runs }
        })
        .collect();
    let response = json!({"schema_version":3,"service":{"schema_version":2,"generation":"fixture","language":"eng","initial_page":200,"pages":[{"address":owner(),"kind":"full_page","language":"eng","national_option":0,"rows":rows,"navigation":[]}],"page_enhancements":[],"magazines":[],"objects":[],"drcs":[],"mots":[],"mot_selections":[{"display":owner(),"mot":null}],"empty_object_pages":[]},"authoring":metadata});
    ersatzrs_addon_contract::TeletextPagesV3Response::from_json(
        response.to_string().as_bytes(),
        "fixture",
    )
    .expect(
        "synthetic response must satisfy the closed V3 envelope and nested V2 service contract",
    );
    let binding = json!({"contract":3,"channel_id":1,"addon_id":ADDON,"binding_revision":1,"profile_id":1,"profile_revision":1,"effective_settings":[],"installation":{"addon_id":ADDON,"state_revision":"synthetic-installation","version":"0.1.1","package_sha256":"a".repeat(64),"content_sha256":"b".repeat(64),"manifest_sha256":"c".repeat(64)}});
    let original = json!({"format_version":1,"selection":{"channel_id":1,"bindings":[binding]},"generation":"fixture","entry_addon_id":ADDON,"entry":owner(),"v1_region":null,"edit_policy":{"policy_version":1,"kind":"original_only"},"resource_transmissions":[],"documents":[{"contract":3,"generation":"fixture","response":response.to_string()}]});
    decode_teletext_design_documents(&serde_json::to_vec(&original).unwrap(), policies().0)
        .unwrap()
        .into_parts()
}
fn encode(
    parts: TeletextRestoredDesignDocumentParts,
    edits: Option<&RebasedGuideEditorial>,
) -> EncodedChannelTeletextOriginals {
    encode_original_producer_documents_with_editorial_resources(
        Some(0),
        parts.documents,
        EditorialResourceDocuments {
            secondary_default_agreements: &[],
            base_text_edits: edits.map_or(&[], RebasedGuideEditorial::base_text_edits),
            main_programs: &[],
            main_presence: edits.map_or(&[], RebasedGuideEditorial::main_program_presence_overlays),
            programs: (
                edits.map_or_else(Vec::new, |value| value.rows().to_vec()),
                edits.map_or(&[], RebasedGuideEditorial::local_program_operands),
                &[],
                &[],
                &[],
            ),
            resource_operands: &[],
            resource_programs: &[],
            resource_additions: &[],
            object_resource_additions: &[],
            object_subpage_announcements: &[],
            object_page_selections: &[],
            main_resource_invocations: &[],
        },
        deadline(),
        None,
        policies().1,
    )
    .unwrap()
}
fn service(value: &EncodedChannelTeletextOriginals) -> &EncodedTeletextAuthoredService {
    value.encoded.as_ref().unwrap()
}
fn composition(
    value: &EncodedChannelTeletextOriginals,
) -> &ersatzrs_ffmpeg::teletext::TeletextMainComposition {
    match &service(value).pages()[0] {
        EncodedTeletextAuthoredPage::NativeDisplay(page) => page.composition(),
        EncodedTeletextAuthoredPage::CompiledDisplay(page) => page.normalized().original(),
        _ => panic!("synthetic fixture must contain its authored display page"),
    }
}
fn project(metadata: &Value) -> TeletextExportProject {
    serde_json::from_value(json!({"schema_version":1,"editor_profile":"native-teletext","original":{"design_id":1,"revision":1,"digest":"d".repeat(64),"expected_head":1,"document_version":1},"channel_id":1,"entry":owner(),"language":"eng","v1_region":null,
    "producers":[{"ordinal":0,"addon_id":ADDON,"contract":3,"binding_revision":1,"profile_id":1,"profile_revision":1,"installation":{"state_revision":"synthetic-installation","version":"0.1.1","package_sha256":"a".repeat(64),"content_sha256":"b".repeat(64),"manifest_sha256":"c".repeat(64)},"authoring":metadata}],
    "pages":[{"name":"p200.tti","page":owner(),"producer_ordinal":0,"editor_page":512,"editor_ordinal":0,"language":"eng","kind":"full_page","header_controls":128,"navigation":[],"materialized_magazine":null,"tti_bytes":1,"tti_sha256":"e".repeat(64)}],"assets":[]})).unwrap()
}
fn overlay(original: &EncodedChannelTeletextOriginals) -> TeletextSavedEditorialOverlay {
    let baseline = rows(service(original), owner(), policies().1.encoding.max_units).unwrap();
    let mut custom = baseline[&21];
    custom[2] = b'Z';
    custom[7] = 0x08;
    custom[37] = 0x09;
    let mut footer = baseline[&22];
    footer[2] = b'B';
    TeletextSavedEditorialOverlay::new(
        TeletextSavedEditorialProvenance {
            export_receipt_id: 1,
            export_receipt_digest: "e".repeat(64),
            original_design_id: 1,
            original_revision: 1,
            original_digest: "d".repeat(64),
        },
        vec![TeletextPhysicalBaseOverlay::new(
            owner(),
            vec![
                TeletextPhysicalBaseRow::new(21, custom).unwrap(),
                TeletextPhysicalBaseRow::new(22, footer).unwrap(),
            ],
        )
        .unwrap()],
    )
    .unwrap()
    .with_base_text_edits(vec![TeletextBaseTextEdit::new(
        owner(),
        TeletextEnhancementPosition::new(1, 2).unwrap(),
        '£',
        '¥',
    )
    .unwrap()])
    .unwrap()
}
fn seed_changes(overlay: &TeletextSavedEditorialOverlay) -> RebasedGuideEditorial {
    RebasedGuideEditorial {
        rows: overlay.pages().to_vec(),
        base_text: overlay.base_text_edits().to_vec(),
        operands: vec![],
        presence: vec![],
        project: project(&authoring()),
    }
}
fn intent() -> (
    AuthenticatedGuideRefreshIntent,
    EncodedChannelTeletextOriginals,
) {
    let original = encode(parts("OLD GUIDE", "ABCD DEFAULT", &authoring()), None);
    let overlay = overlay(&original);
    let edited = encode(
        parts("OLD GUIDE", "ABCD DEFAULT", &authoring()),
        Some(&seed_changes(&overlay)),
    );
    let intent = extract(
        &overlay,
        service(&original),
        service(&edited),
        project(&authoring()),
        None,
        policies().1.encoding.max_units,
        deadline(),
    )
    .unwrap();
    (intent, original)
}

#[test]
fn guide_refresh_rebases_only_header_footer_custom_intent_and_preserves_fresh_generated_rows() {
    let (intent, old) = intent();
    let fresh = encode(parts("NEW GUIDE", "ABXY DEFAULT", &authoring()), None);
    let sources = parts("NEW GUIDE", "ABXY DEFAULT", &authoring()).sources;
    let rebased = intent
        .rebase(
            &GuideRefreshEditorialBaseline {
                encoded: service(&fresh),
                sources: &sources,
                region: None,
            },
            policies().1.encoding.max_units,
            deadline(),
        )
        .unwrap();
    let edited = encode(
        parts("NEW GUIDE", "ABXY DEFAULT", &authoring()),
        Some(&rebased),
    );
    let validation = rebased
        .validate(
            service(&fresh),
            service(&edited),
            &policies().1.encoding,
            deadline(),
        )
        .unwrap();
    assert!(validation.matches(service(&edited)));
    assert!(!validation.matches(service(&old)));
    let next = rows(service(&edited), owner(), policies().1.encoding.max_units).unwrap();
    let current = rows(service(&fresh), owner(), policies().1.encoding.max_units).unwrap();
    assert_eq!(next[&3], current[&3]);
    assert_ne!(
        next[&3],
        rows(service(&old), owner(), policies().1.encoding.max_units).unwrap()[&3]
    );
    assert_eq!(next[&21][2], b'Z');
    assert_eq!(&next[&21][4..6], b"XY");
    assert_eq!(next[&21][7], 0x08);
    assert_eq!(next[&21][35], 0x08);
    assert_eq!(next[&21][37], 0x09);
    assert!(composition(&edited)
        .base_display()
        .cell(21, 8)
        .unwrap()
        .attributes()
        .flash());
    let protected_before = composition(&fresh).base_display().cell(21, 38).unwrap();
    let protected_after = composition(&edited).base_display().cell(21, 38).unwrap();
    assert_eq!(protected_before.attributes(), protected_after.attributes());
    assert!(!protected_after.attributes().flash());
    assert_eq!(next[&22][2], b'B');
    let header = &service(&edited).declared().semantic().base().pages[0].rows[0].runs[0].content;
    assert!(matches!(header,TeletextRunContent::Text{text} if text.starts_with('¥')));
}

#[test]
fn guide_refresh_rebase_rejects_incompatible_bindings_regions_charsets_and_unexpected_free_content()
{
    let (intent, _) = intent();
    for mutation in ["binding", "region", "generated", "charset", "expected"] {
        let mut metadata = authoring();
        if mutation == "binding" {
            metadata["pages"][0]["dynamic_fields"][0]["binding"] = json!("guide.channel-2");
        }
        if mutation == "region" {
            metadata["pages"][0]["regions"][2]["width"] = json!(35);
        }
        if mutation == "generated" {
            metadata["pages"][0]["regions"][2]["kind"] = json!("generated");
        }
        let custom = if mutation == "expected" {
            "QBCD DEFAULT"
        } else {
            "ABCD DEFAULT"
        };
        let fresh_parts = parts("NEW GUIDE", custom, &metadata);
        if mutation == "charset" {
            if let ersatzrs_app::teletext_profile_documents::TeletextProfileDocument::Authored(_) =
                &fresh_parts.documents[0]
            {
                // A different explicit V1 interpretation is rejected before any
                // physical patch even if its current bytes happen to coincide.
                let fresh = encode(fresh_parts, None);
                let sources = parts("NEW GUIDE", custom, &metadata).sources;
                assert!(intent
                    .rebase(
                        &GuideRefreshEditorialBaseline {
                            encoded: service(&fresh),
                            sources: &sources,
                            region: Some(TeletextEncodingRegion::WesternLatin)
                        },
                        policies().1.encoding.max_units,
                        deadline()
                    )
                    .is_err());
                continue;
            }
        }
        let fresh = encode(fresh_parts, None);
        let sources = parts("NEW GUIDE", custom, &metadata).sources;
        assert!(
            intent
                .rebase(
                    &GuideRefreshEditorialBaseline {
                        encoded: service(&fresh),
                        sources: &sources,
                        region: None
                    },
                    policies().1.encoding.max_units,
                    deadline()
                )
                .is_err(),
            "{mutation}"
        );
    }
}
