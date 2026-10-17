//! Reuse the unchanged genuine editor input; qualify growth independently.
use super::*;
use ersatzrs_domain::TeletextObjectIdentity;
use ersatzrs_ffmpeg::teletext::{EncodedTeletextAuthoredService, EncodedTeletextObjectPage};

pub(super) fn encoding(
) -> ersatzrs_web::host::teletext_channel_documents::TeletextChannelDocumentEncodingPolicy {
    use ersatzrs_ffmpeg::teletext::TeletextResourceFullUpdate;
    let mut policy = seed::encoding();
    for transmission in &mut policy.transmissions {
        transmission.update = if transmission.resource == seed::target(21).resource().unwrap() {
            TeletextResourceFullUpdate::Changed
        } else {
            TeletextResourceFullUpdate::Unchanged
        };
    }
    assert_eq!(policy.transmissions.len(), 7);
    assert_eq!(
        policy
            .transmissions
            .iter()
            .filter(|t| t.update == TeletextResourceFullUpdate::Changed)
            .count(),
        1
    );
    policy
}

pub(super) fn response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = wide_response(theme, page, subcode);
    let objects = value["service"]["objects"].as_array_mut().unwrap();
    let mut following = objects
        .iter()
        .find(|v| v["identity"]["object_id"] == 21)
        .unwrap()
        .clone();
    following["identity"]["object_id"] = json!(33);
    following["program"]["commands"] = json!([]);
    objects.push(following);
    value
}
fn grown_response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = response(theme, page, subcode);
    let object = value["service"]["objects"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|v| v["identity"]["object_id"] == 21)
        .unwrap();
    let commands = object["program"]["commands"].as_array_mut().unwrap();
    assert_eq!(commands.len(), 5);
    commands.extend((0..20).map(|_| {
        json!({"kind":"at_column","column":2,
        "operation":{"kind":"g0","code":80}})
    }));
    value
}
pub(super) struct Grown {
    pub(super) tti: Vec<u8>,
    pub(super) service: Arc<EncodedTeletextAuthoredService>,
}
pub(super) async fn grown() -> Grown {
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        grown_response,
        Fixture::new_persistent().await,
        encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let export = &flow.exported[0];
    let tti = export
        .files()
        .iter()
        .find(|f| f.name == companion(export))
        .unwrap()
        .bytes
        .clone();
    let service = Arc::clone(flow.accepted[0].service());
    fixture.state.pool.close().await;
    Grown { tti, service }
}

pub(super) fn actual_saved(flow: &Flow, directory: &Path) -> Vec<u8> {
    let read =
        |name: &str| live_client::read_bounded(&directory.join(name), 2 * 1024 * 1024).unwrap();
    let ready: Value = serde_json::from_slice(&read("ready.json")).unwrap();
    let gui: Value = serde_json::from_slice(&read("editor-gui.json")).unwrap();
    let success: Value = serde_json::from_slice(&read("success.json")).unwrap();
    assert_eq!(ready["case"], CASE);
    assert_eq!(ready["editor"]["version"], "1.0");
    assert_eq!(
        ready["editor"]["source_commit"],
        "cac2167bac933a6160cca45a45d5d4ee95355898"
    );
    assert_eq!(
        ready["editor"]["binary_sha256"],
        "af7cb11220a411011917db0739f27a35b5c6ab725552b52b8a09bc01e296568a"
    );
    assert_eq!(
        ready["edit"],
        json!({"kind":"resource_glyph_insert_copy","identity":seed::target(21),
        "source_command":4,"editor_word":7,"column":1,"copy_editor_word":8,"copy_column":2,
        "mode":9,"code":80,"consumers":[124,125,126]})
    );
    let saved = read("edited.tti");
    assert_eq!(gui["saved_sha256"], live_tcp::hash(&saved));
    assert_eq!(success["saved_sha256"], live_tcp::hash(&saved));
    assert_eq!(gui["actual_gui_insert_copy_column_spin_and_save_as"], true);
    assert_eq!(success["explicit_publication_and_reopen"], true);
    let input = ready["input"].as_str().unwrap();
    assert_eq!(input, format!("original/{}", companion(&flow.exported[0])));
    let original = read(input);
    assert_eq!(ready["input_sha256"], live_tcp::hash(&original));
    assert_eq!(gui["input_sha256"], live_tcp::hash(&original));
    assert_eq!(
        original,
        flow.exported[0]
            .files()
            .iter()
            .find(|f| f.name == companion(&flow.exported[0]))
            .unwrap()
            .bytes
    );
    // A fresh complete native original adds an unused following definition.
    // Reuse only this byte-identical editor input/save, never its old receipt.
    saved
}

fn object(service: &EncodedTeletextAuthoredService) -> &EncodedTeletextObjectPage {
    service
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::Object(p)
                if p.header().identity() == seed::target(21).resource().unwrap() =>
            {
                Some(p)
            }
            _ => None,
        })
        .unwrap()
}
fn position(page: &EncodedTeletextObjectPage, target: TeletextObjectIdentity) -> u16 {
    page.definition_positions()
        .iter()
        .find(|p| p.target() == target)
        .unwrap()
        .absolute_index()
}
pub(super) fn assert_services(
    flow: &Flow,
    stages: &[Current; 3],
    grown: &EncodedTeletextAuthoredService,
    pixels: &[TeletextRaster],
) {
    assert_added_p(flow, pixels);
    for stage in stages {
        assert_eq!(
            (124..127)
                .map(|p| external_editor_probe::preview_page(stage.accepted.service(), p))
                .collect::<Vec<_>>(),
            pixels
        );
    }
    let initial = object(flow.accepted[0].service());
    let first = object(stages[0].accepted.service());
    let large = object(stages[1].accepted.service());
    let last = object(stages[2].accepted.service());
    assert_ne!(initial.header().last_packet(), large.header().last_packet());
    assert!(position(large, seed::target(33)) > position(first, seed::target(33)));
    assert_ne!(first.header().last_packet(), large.header().last_packet());
    assert_eq!(last.data_units(), first.data_units());
    assert_eq!(
        position(last, seed::target(33)),
        position(first, seed::target(33))
    );
    assert_eq!(stages[1].accepted.service().inventory(), grown.inventory());
    assert_eq!(
        stages[1].accepted.service().magazine_units(),
        grown.magazine_units()
    );
    assert_eq!(
        stages[1]
            .accepted
            .service()
            .pages()
            .iter()
            .map(EncodedTeletextAuthoredPage::data_units)
            .collect::<Vec<_>>(),
        grown
            .pages()
            .iter()
            .map(EncodedTeletextAuthoredPage::data_units)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        stages[0]
            .accepted
            .service()
            .pages()
            .iter()
            .map(EncodedTeletextAuthoredPage::data_units)
            .collect::<Vec<_>>(),
        stages[2]
            .accepted
            .service()
            .pages()
            .iter()
            .map(EncodedTeletextAuthoredPage::data_units)
            .collect::<Vec<_>>()
    );
}
