//! EE1 qualification uses complete independent native factories, not word masks.
use super::*;
use ersatzrs_ffmpeg::teletext::{EncodedTeletextAuthoredPage, EncodedTeletextAuthoredService};

#[path = "complete_context/checks.rs"]
mod checks;
#[path = "complete_context/editor.rs"]
mod editor;
#[path = "complete_context/lifecycle.rs"]
mod lifecycle;

fn response(theme: &str, page: u16, subcode: u16, designation: u8) -> Value {
    let mut value = seed::response(theme, page, subcode);
    let local = crate::editorial_imports::local_object_glyphs::free(theme, page, subcode);
    value["service"]["objects"].as_array_mut().unwrap().extend(
        local["service"]["objects"]
            .as_array()
            .unwrap()
            .iter()
            .cloned(),
    );
    for page in value["service"]["page_enhancements"]
        .as_array_mut()
        .unwrap()
    {
        if local["service"]["objects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|object| object["identity"]["owner"] == page["address"])
        {
            assert!(page["local_program"].is_null());
            page["local_program"] = json!({"commands":[]});
        }
    }
    let rows = value["service"]["pages"][0]["rows"].as_array_mut().unwrap();
    assert!(!rows.iter().any(|row| row["row"] == 6));
    rows.push(json!({"row":6,"runs":[{"column":8,
        "style":{"foreground":"white","background":"black","height":"normal",
            "boxed":false,"flash":false,"conceal":false},
        "content":{"kind":"text","text":"é"}}]}));
    value["authoring"]["pages"][0]["regions"]
        .as_array_mut()
        .unwrap()
        .push(
            json!({"id":"context-factory-row","kind":"free","area":"main",
            "row":6,"column":0,"width":40,"height":1}),
        );
    let mut commands = vec![json!({"kind":"set_position","position":{"row":1,"column":8}})];
    commands.extend(
        local["service"]["page_enhancements"][0]["local_program"]["commands"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|command| command["kind"] == "invoke_object")
            .cloned(),
    );
    commands.extend([
        json!({"kind":"set_position","position":{"row":6,"column":0}}),
        json!({"kind":"at_column","column":0,"operation":{"kind":"designate_charset","designation":designation}}),
        json!({"kind":"at_column","column":9,"operation":{"kind":"g0","code":65}}),
    ]);
    value["service"]["page_enhancements"][0]["local_program"] = json!({"commands":commands});
    value
}

fn root(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, 0x57)
}

fn edited(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, 0x37)
}

fn edited_latin(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, 0)
}

fn display(
    service: &EncodedTeletextAuthoredService,
) -> &ersatzrs_ffmpeg::teletext::EncodedTeletextCompiledDisplayPage {
    display_page(service, 124)
}

fn display_page(
    service: &EncodedTeletextAuthoredService,
    page_number: u16,
) -> &ersatzrs_ffmpeg::teletext::EncodedTeletextCompiledDisplayPage {
    service
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::CompiledDisplay(page)
                if page.page().address() == channel_documents::address(page_number, 32) =>
            {
                Some(page)
            }
            _ => None,
        })
        .unwrap()
}

#[tokio::test]
async fn complete_main_context_factory_preserves_defaults_local_definitions_and_generated_base() {
    for factory in [edited, edited_latin] {
        Box::pin(run(factory, None)).await;
    }
}

#[tokio::test]
#[ignore = "requires pinned headed QTM1.0 via ERSATZRS_TELETEXT_COMPLETE_CONTEXT_EDITOR_DIR"]
async fn actual_qtm_complete_main_context_preserves_local_default_catalogue_and_root_undo() {
    let directory = std::path::PathBuf::from(crate::client_capture::selected(
        "ERSATZRS_TELETEXT_COMPLETE_CONTEXT_EDITOR_DIR",
    ));
    Box::pin(run(edited, Some(&directory))).await;
}

async fn run(factory: fn(&str, u16, u16) -> Value, directory: Option<&std::path::Path>) {
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        root,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let (target_fixture, target) = Box::pin(setup_with_fixture_policy_and_writer(
        factory,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let old = flow.accepted[0].service();
    let wanted = target.accepted[0].service();
    checks::qualified(old, wanted);
    checks::negatives(&fixture, &flow, &target).await;
    let request = checks::request(&flow, &target);
    let request = if let Some(directory) = directory {
        editor::edit(directory, &flow, &target, request).await
    } else {
        request
    };
    let concept = import(&fixture, request).await;
    lifecycle::finish(fixture, &flow, &concept, &target, directory).await;
    target_fixture.state.pool.close().await;
}

fn preview(service: &EncodedTeletextAuthoredService) -> TeletextRaster {
    let preview = project_teletext_preview(
        service,
        channel_documents::address(124, 32),
        TeletextColourPixelOptions {
            reveal_concealed: false,
            flash_phase: TeletextFlashPhase::Original,
        },
        preview_budget(),
    )
    .unwrap();
    assert!(preview.raster().diagnostics().is_empty());
    preview.raster().clone()
}
