//! Receipt-bound national contexts at Level 1.5, without optional object banks.
//! The ignored test uses the existing pinned headed editor driver unchanged.

use super::*;

#[derive(Clone, Copy)]
enum Context {
    Romanian,
    Greek,
}

impl Context {
    fn name(self) -> &'static str {
        match self {
            Self::Romanian => "romanian",
            Self::Greek => "greek",
        }
    }

    fn primary(self) -> u8 {
        match self {
            Self::Romanian => 0x1f,
            Self::Greek => 0x37,
        }
    }

    fn headline(self) -> &'static str {
        match self {
            Self::Romanian => "ĂÂ 200 ăâ",
            Self::Greek => "Αθήνα 200",
        }
    }

    fn response(self) -> fn(&str, u16, u16) -> Value {
        match self {
            Self::Romanian => romanian,
            Self::Greek => greek,
        }
    }
}

fn national_response(context: Context, theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = response(theme, page, subcode);
    let mut presentation = super::super::super::documents::document(
        super::super::super::documents::DocumentFixture::Authored,
        theme,
    )["magazines"][0]["presentation"]
        .clone();
    presentation["primary"] = json!(context.primary());
    presentation["panels"] = json!({"left":false,"right":false,"status":"level25_and35","width":0});
    for enhancement in value["service"]["page_enhancements"]
        .as_array_mut()
        .unwrap()
    {
        enhancement["presentation"] = presentation.clone();
    }
    for page in value["service"]["pages"].as_array_mut().unwrap() {
        page["national_option"] = json!(7);
        page["rows"][0]["runs"][0]["content"]["text"] = json!(context.headline());
        // Page125 has no Main programme: its native national line stays visible.
        // An extra row beside page124's full Main would exceed its 208 words.
    }
    value
}

fn romanian(theme: &str, page: u16, subcode: u16) -> Value {
    national_response(Context::Romanian, theme, page, subcode)
}

fn greek(theme: &str, page: u16, subcode: u16) -> Value {
    national_response(Context::Greek, theme, page, subcode)
}

pub(super) fn assert_files(actual: &[TeletextExportFile], expected: &[TeletextExportFile]) {
    assert_eq!(actual.len(), expected.len());
    for before in expected {
        let after = actual.iter().find(|file| file.name == before.name).unwrap();
        let actual = parse_teletext_tti(&after.bytes, policy().tti).unwrap();
        let expected = parse_teletext_tti(&before.bytes, policy().tti).unwrap();
        // Pinned QTM moves FL after the rows. Its location has no TTI meaning;
        // retain its exact value/count and all other records in original order.
        for fast_links in [false, true] {
            let selected = |record: &&TeletextTtiRecord| {
                matches!(record, TeletextTtiRecord::FastLinks(_)) == fast_links
            };
            assert_eq!(
                actual.records().iter().filter(selected).collect::<Vec<_>>(),
                expected
                    .records()
                    .iter()
                    .filter(selected)
                    .collect::<Vec<_>>(),
                "national context, base rows or navigation changed in {}",
                before.name
            );
        }
    }
}

#[tokio::test]
async fn romanian_and_greek_level15_main_concepts_preserve_national_context() {
    for context in [Context::Romanian, Context::Greek] {
        Box::pin(run(context, None)).await;
    }
}

#[tokio::test]
#[ignore = "requires two actual pinned headed QTM1.0 saves in fresh ERSATZRS_TELETEXT_LEVEL15_NATIONAL_DIR"]
async fn actual_qtm_romanian_and_greek_level15_edit_preview_publish_reopen() {
    let root = std::path::PathBuf::from(super::super::super::client_capture::selected(
        "ERSATZRS_TELETEXT_LEVEL15_NATIONAL_DIR",
    ));
    std::fs::create_dir(&root).expect("national editor destination must be new");
    for context in [Context::Romanian, Context::Greek] {
        Box::pin(run(context, Some(&root.join(context.name())))).await;
    }
}

fn assert_original_context(flow: &Flow) {
    for exported in &flow.exported {
        assert!(exported.sidecar().project.assets.is_empty());
        assert_eq!(exported.files().len(), 2);
    }
    for accepted in &flow.accepted {
        assert!(accepted
            .service()
            .declared()
            .resource_inventory()
            .is_empty());
        assert!(accepted.service().compiler_drcs_resources().is_empty());
        assert!(accepted
            .service()
            .declared()
            .semantic()
            .objects()
            .definitions()
            .is_empty());
        for page in &accepted.service().declared().semantic().base().pages {
            assert_eq!(page.national_option.get(), 7);
        }
    }
}

async fn run(context: Context, editor: Option<&Path>) {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        context.response(),
        Fixture::new_persistent().await,
        channel_documents::policy,
        authoring_metadata::write_compact,
    ))
    .await;
    assert_original_context(&flow);
    let expected = edited_request(&flow);
    let request = if let Some(directory) = editor {
        genuine_edit(directory, &flow).await
    } else {
        edited_request(&flow)
    };
    assert_files(&request.files, &expected.files);
    assert_eq!(request.sidecar, expected.sidecar);
    let concept = import(&fixture, request).await;
    let restored = drcs_bitmap::restored(&fixture, &concept).await;
    let original = flow.saved[0].candidate();
    assert_eq!(restored.selection(), original.selection());
    assert_eq!(restored.metadata(), original.metadata());
    assert_eq!(restored.transmissions(), original.transmissions());
    assert_eq!(restored.sources().len(), original.sources().len());
    for (before, after) in original.sources().iter().zip(restored.sources()) {
        assert_eq!(before.bytes(), after.bytes());
        assert_eq!(before.authoring(), after.authoring());
    }
    let preview = external_editor_probe::preview_page(restored.encoded().unwrap(), 124);
    let before = external_editor_probe::preview_page(flow.accepted[0].service(), 124);
    assert_eq!((preview.width(), preview.height()), (480, 250));
    assert_ne!(preview, before);
    // Only row6's one G2 cell changes. Page125's native national line survives.
    for y in 0..250 {
        for x in 0..480 {
            if !(60..70).contains(&y) || x >= 12 {
                assert_eq!(preview.pixel(x, y), before.pixel(x, y));
            }
        }
    }
    assert_eq!(
        external_editor_probe::preview_page(restored.encoded().unwrap(), 125),
        external_editor_probe::preview_page(flow.accepted[0].service(), 125)
    );
    assert_original_pointers_with_width(&fixture, &flow, 480).await;
    let accepted = publish_and_accept(&fixture, &flow, &concept).await;
    assert_eq!(
        external_editor_probe::preview_page(accepted.service(), 124),
        preview
    );
    let reexported = drcs_bitmap::reexport(&fixture, &concept).await;
    assert_files(reexported.files(), &expected.files);
    fixture = fixture.reopen().await;
    for (channel, expected) in [(1, &accepted), (2, &flow.accepted[1])] {
        let recovered = recover(&fixture, channel).await;
        assert_eq!(recovered.generation(), expected.generation());
        assert_eq!(
            designs::units(recovered.service()),
            designs::units(expected.service())
        );
    }
    assert_retained_storage(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    if let Some(directory) = editor {
        qtm::json_new(
            directory,
            "success.json",
            &json!({
                "scope":"Actual pinned headed editor -> native concept/preview -> explicit publication/reexport -> SQLite reopen; no ordinary stream/client/fullLevel15 claim",
                "context":context.name(),"primary":context.primary(),"header_national_option":7,
                "headline":context.headline(),"main_words":208,"edited_word":206,
                "resource_assets":0,"other_records_preserved":true,"unchanged_pixels_outside_cell":true,
                "original_sources_metadata_config_receipts_and_night_preserved":true,
                "editor":"QTeletextMaker1.0/cac2167/af7cb",
                "design_id":concept.head().id,"revision":concept.head().revision,
                "generation_id":accepted.generation().id
            }),
        );
    }
    fixture.state.pool.close().await;
}
