//! Real ordinary delivery of the fixed G1 D5=0/later-spacing calibration.
//! Successful delivery and an external marker do not certify the stock client:
//! its known G1 failure is retained beside the independent imager result.

use super::super::{client_capture, ordinary_stream_flow};
use super::*;
use ersatzrs_domain::{ChannelTeletextBindingRead, TeletextProfileRead};
use ersatzrs_ffmpeg::teletext::source_header::{
    observe_dvb_teletext_header, TeletextHeaderObservation,
};
use futures_util::FutureExt as _;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap, collections::BTreeSet, io, panic::AssertUnwindSafe, path::PathBuf,
};

const REFERENCE_HASH: &str = "8621d2e1f401e4ce82311a30d114f64d77b7131771f8c71581f39cb63ca26ef9";
// This exact independently reviewed helper owns the imager/stock-negative oracle.
const DECODER_HASH: &str = "e1b0d81ecb3026da714cc49e5fd6766a65ab0be99dd399556503c23bae6ce881";
const SEPARATED: [u16; 10] = [0, 1950, 1950, 0, 1950, 1950, 1950, 0, 1950, 1950];
type Bodies = BTreeMap<u8, BTreeSet<Vec<u8>>>;

fn response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = external_editor_probe::editor_response(theme, page, subcode);
    let service = &mut value["service"];
    service["magazines"] = json!([]);
    for field in ["objects", "drcs", "mots", "empty_object_pages"] {
        service[field] = json!([]);
    }
    let style = json!({"foreground":"white","background":"black","height":"normal",
        "boxed":false,"flash":false,"conceal":false});
    for display in service["pages"].as_array_mut().unwrap() {
        display["national_option"] = json!(1);
        display["rows"] = json!([
            {"row":1,"runs":[
                {"column":8,"style":style,"content":{"kind":"mosaics","cells":[63,63],"separated":true}},
                {"column":12,"style":style,"content":{"kind":"mosaics","cells":[63,63],"separated":false}},
                {"column":20,"style":style,"content":{"kind":"mosaics","cells":[63,63],"separated":true}}
            ]},
            {"row":2,"runs":[{"column":1,"style":style,"content":{"kind":"text","text":"B"}}]}
        ]);
    }
    for enhancement in service["page_enhancements"].as_array_mut().unwrap() {
        enhancement["presentation"] = Value::Null;
        enhancement["compositional_links"] = Value::Null;
        enhancement["resources"] = json!({"objects":{"public":null,"global":null},
            "drcs":{"normal":null,"global":null},"defaults":[null,null,null,null]});
        enhancement["local_program"] = json!({"commands":[
            {"kind":"set_position","position":{"row":1,"column":8}},
            {"kind":"at_column","column":8,"operation":{"kind":"attributes",
                "double_height":false,"double_width":false,"boxed":false,"conceal":false,
                "invert":false,"underline_or_separated_mosaics":false}}
        ]});
    }
    for selection in service["mot_selections"].as_array_mut().unwrap() {
        selection["mot"] = Value::Null;
    }
    value
}

fn bodies(units: &[Vec<u8>]) -> Bodies {
    let mut result = Bodies::new();
    for unit in units {
        let unit: &[u8; 46] = unit.as_slice().try_into().unwrap();
        if let TeletextHeaderObservation::NotHeader(address) =
            observe_dvb_teletext_header(unit).unwrap()
        {
            if [1, 2, 26].contains(&address.packet_number) {
                result
                    .entry(address.packet_number)
                    .or_default()
                    .insert(unit[6..].to_vec());
            }
        }
    }
    assert_eq!(result.keys().copied().collect::<Vec<_>>(), [1, 2, 26]);
    result
}

fn reference() -> (Bodies, Value) {
    let path = PathBuf::from(client_capture::selected(
        "ERSATZRS_TELETEXT_G1_REFERENCE_FIXTURE",
    ));
    let raw = live_client::read_bounded(&path, 2 * 1024 * 1024).unwrap();
    assert_eq!(live_client::hash(&raw), REFERENCE_HASH);
    let request: Value = serde_json::from_slice(&raw).unwrap();
    assert_eq!(request["expected_version"], "0.2.0");
    let cases = request["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 4);
    let case = cases
        .iter()
        .find(|case| case["name"] == "later_base_spacing_reenables_separation")
        .unwrap();
    let units: Vec<Vec<u8>> = serde_json::from_value(case["units"].clone()).unwrap();
    assert!(units.len() <= 512);
    assert_eq!(case["page"], 0x200);
    assert_ne!(case["subcode"], 32);
    (
        bodies(&units),
        json!({"sha256":REFERENCE_HASH,"case":case["name"],
        "reference_raw_page":case["page"],"reference_subcode":case["subcode"],
        "compared_packet_bodies":[1,2,26],"body_bytes":40,
        "native_pages":[124,125],"native_magazine":1,"native_subcode":32,
        "header_and_packet_addresses_compared":false,
        "navigation":"Native reciprocal124/125 Index links differ from the old calibration; neither is rewritten."}),
    )
}

fn closed_prefix(mut units: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
    assert!(units.len() <= 512);
    let mut active = None;
    let mut rows = BTreeSet::new();
    let mut completed = BTreeSet::new();
    let mut boundary = 0;
    for (index, unit) in units.iter().enumerate() {
        let unit: &[u8; 46] = unit.as_slice().try_into().unwrap();
        match observe_dvb_teletext_header(unit).unwrap() {
            TeletextHeaderObservation::Header(header)
            | TeletextHeaderObservation::Terminator(header) => {
                if let Some(page) = active {
                    if [1, 2, 26].into_iter().all(|row| rows.contains(&row)) {
                        completed.insert(page);
                    }
                }
                active = header
                    .display_page()
                    .map(ersatzrs_domain::TeletextPageNumber::get);
                rows.clear();
                if let Some(page) = active {
                    assert!([124, 125].contains(&page));
                    assert_eq!(header.packet_address.wire_magazine, 1);
                    assert_eq!(header.subcode, 32);
                    if completed == BTreeSet::from([124, 125]) {
                        boundary = index + 1;
                    }
                }
            }
            TeletextHeaderObservation::NotHeader(address) => {
                if active.is_some() {
                    rows.insert(address.packet_number);
                }
            }
            TeletextHeaderObservation::NotTeletext { .. } => {
                panic!("the fixed G1 prefix must contain only actual teletext units");
            }
        }
    }
    assert!(
        boundary > 0,
        "bounded actual prefix must close both complete displays"
    );
    // A byte-identical contiguous prefix, including its actual closing header.
    // No interior packet is filtered, reordered, synthesized or rewritten.
    units.truncate(boundary);
    units
}

fn preview(service: &ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService) -> Vec<Value> {
    assert!(service.magazine_units().is_empty());
    assert_eq!(service.pages().len(), 2);
    assert!(service
        .pages()
        .iter()
        .all(|page| page.resource_header().is_none()));
    [124, 125].into_iter().map(|page| {
        let image = external_editor_probe::preview_page(service, page);
        assert_eq!((image.width(), image.height()), (480, 250));
        let mut cells = Vec::new();
        for column in [8, 9, 12, 13, 20, 21, 1] {
            let row = if column == 1 {2} else {1};
            let mut bits = [0u16; 10];
            for (y, bits) in bits.iter_mut().enumerate() {
                for x in 0..12 {
                    let TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(rgb)) =
                        image.pixel(column * 12 + x, row * 10 + y).unwrap() else {
                            panic!("G1 calibration must retain resolved opaque RGB pixels");
                        };
                    assert!([[0,0,0], [15,15,15]].contains(&rgb.get()));
                    if rgb.get() == [15,15,15] { *bits |= 1 << (11-x); }
                }
            }
            if column == 1 {
                assert!(bits.iter().any(|bits| *bits != 0), "B calibration must remain visible");
            } else {
                assert_eq!(bits, if column >= 20 {SEPARATED} else {[4095;10]});
            }
            cells.push(json!({"row":row,"column":column,"bitmap_rows":bits}));
        }
        json!({"page":page,"subcode":32,"width":image.width(),"height":image.height(),"cells":cells})
    }).collect()
}

struct Retained {
    package: String,
    profiles: Vec<TeletextProfileRead>,
    bindings: [Vec<ChannelTeletextBindingRead>; 2],
}

impl Retained {
    async fn capture(fixture: &Fixture) -> Self {
        Self {
            package: calculate_addon_content_sha256(&fixture.root.path().join("package"))
                .await
                .unwrap(),
            profiles: fixture
                .state
                .application
                .list_teletext_profiles(ADDON)
                .await
                .unwrap(),
            bindings: [
                fixture
                    .state
                    .application
                    .channel_teletext_bindings(1)
                    .await
                    .unwrap(),
                fixture
                    .state
                    .application
                    .channel_teletext_bindings(2)
                    .await
                    .unwrap(),
            ],
        }
    }

    async fn assert_unchanged(&self, fixture: &mut Fixture, flow: &Flow) {
        assert_original_pointers(fixture, flow).await;
        assert_eq!(
            calculate_addon_content_sha256(&fixture.root.path().join("package"))
                .await
                .unwrap(),
            self.package
        );
        assert_eq!(
            fixture
                .state
                .application
                .list_teletext_profiles(ADDON)
                .await
                .unwrap(),
            self.profiles
        );
        for (index, expected) in self.bindings.iter().enumerate() {
            assert_eq!(
                fixture
                    .state
                    .application
                    .channel_teletext_bindings(i64::try_from(index + 1).unwrap())
                    .await
                    .unwrap(),
                *expected
            );
        }
        assert!(fixture.playout.try_recv().is_err());
    }
}

fn write_json(directory: &Path, name: &str, value: &impl serde::Serialize) {
    live_client::write_new(
        &directory.join(name),
        &serde_json::to_vec_pretty(value).unwrap(),
    )
    .unwrap();
}

async fn verify_capture(
    directory: &Path,
    tools: &drcs_bitmap_delivery::Tools,
    source: &[u8],
    channel: usize,
    bytes: &[u8],
    flow: &Flow,
    reference: &Bodies,
) -> Value {
    let capture = format!("channel-{channel}.ts");
    live_client::write_new(&directory.join(&capture), bytes).unwrap();
    let _report = client_capture::verify_transport_with_qualified_authored_pts_gaps(
        &tools.tsanalyze,
        &tools.ffprobe,
        bytes,
        0,
        Some(&directory.join(format!("channel-{channel}-tsduck.json"))),
        false,
    )
    .await;
    client_capture::verify_av(&tools.ffmpeg, source, bytes).await;
    ordinary_stream_flow::assert_emitted_model(bytes, flow.accepted[channel - 1].service());
    let units = closed_prefix(drcs_bitmap_delivery::captured_units(bytes));
    assert_eq!(&bodies(&units), reference);
    let encoded = serde_json::to_vec(&units).unwrap();
    let units_name = format!("channel-{channel}-g1-units.json");
    let units_path = directory.join(&units_name);
    live_client::write_new(&units_path, &encoded).unwrap();
    let script =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/e2e/teletext-g1-live-decoder.py");
    let helper = live_client::read_bounded(&script, 65_536).unwrap();
    assert_eq!(
        live_client::hash(&helper),
        DECODER_HASH,
        "review and pin the G1 helper before running"
    );
    let imager = client_capture::selected("ERSATZRS_TELETEXT_TEST_IMAGER_DIR");
    // run.sh selects managed FFmpeg libraries for its Rust test process.
    // Only this independent stock-decoder child clears that inherited path;
    // its own guard still verifies the exact actually mapped stock library.
    let output = client_capture::probe_with_environment(
        "python3",
        &[
            script.to_str().unwrap(),
            units_path.to_str().unwrap(),
            "--imager-root",
            &imager,
        ],
        &[("LD_LIBRARY_PATH", "")],
        262_144,
    )
    .await;
    let decoder: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(decoder["imager_conformance"], true);
    assert_eq!(decoder["stock_g1_conformance"], false);
    assert_eq!(decoder["known_later_spacing_failure_reproduced"], true);
    assert_eq!(decoder["units_sha256"], live_client::hash(&encoded));
    assert_eq!(decoder["unit_count"], units.len());
    live_client::write_new(
        &directory.join(format!("channel-{channel}-independent.json")),
        &output,
    )
    .unwrap();
    json!({"channel_id":channel,"capture":capture,"sha256":live_client::hash(bytes),"bytes":bytes.len(),
        "units_file":units_name,"units_sha256":live_client::hash(&encoded),"unit_count":units.len(),
        "unit_scope":"Exact contiguous prefix ending just after a real124/125 header which closes prior complete displays; not source EOF.",
        "reference_row1_row2_x26_bodies_equal":true,"complete_emitted_model_equal":true,
        "source_av_passed":true,"tsduck_transport_passed":true,"decoder":decoder})
}

fn ready(origin: &live_client::LiveOrigin, flow: &Flow, source: &[u8]) -> Value {
    let channels: Vec<_> = flow.accepted.iter().enumerate().map(|(index,candidate)| {
        let binding = &candidate.selection().bindings[0];
        json!({"channel_id":index+1,"theme":if index==0 {"day"} else {"night"},
            "url":origin.url(i64::try_from(index+1).unwrap()),"addon_id":binding.addon_id,
            "profile_id":binding.profile_id,"profile_revision":binding.profile_revision,
            "generation_id":candidate.generation().id,"request_id":candidate.generation().request.id,
            "expected_pages":[124,125],"subcode":32,"expected_navigation":[124,125,124],"edited_q_page":null,
            "calibration_palette":"white_on_black_on_both_profiles","g1_case":"later_base_spacing_reenables_separation"})
    }).collect();
    json!({"probe_version":1,"scope":"Actual ordinary G1 calibration URLs; stock conformance remains negative",
        "channels":channels,"source":{"file":"source.ts","sha256":live_client::hash(source),"bytes":source.len(),"duration_seconds":60},
        "active_transcode_limit":4,"completion_file":"client-complete.json","completion_timeout_seconds":180,
        "expected_completion":{"probe_version":1,"client_name":"VLC","client_version":"3.0.20","decoder_version":"0.2.42","proof_suffix":"live-ordinary"},
        "client_actions":"Open both ordinary URLs and navigate124 to125 to124. Both profiles are white/black. Inspect col8/9,12/13,20/21 and retain the stock G1 mismatch; an external marker certifies no conformance.",
        "limitations":"MAIN only; no G1 stock-client, distinct-palette, panel, full Level2.5, warm or platform acceptance."})
}

async fn served(
    fixture: &mut Fixture,
    flow: &Flow,
    retained: &Retained,
    origin: &live_client::LiveOrigin,
    directory: &Path,
    media: (&drcs_bitmap_delivery::Tools, &[u8]),
    reference: &Bodies,
) -> io::Result<Value> {
    let (tools, source) = media;
    ordinary_stream_flow::reset_source_start(fixture).await;
    let (day, night) = tokio::join!(
        drcs_bitmap_delivery::capture_tcp(origin.url(1)),
        drcs_bitmap_delivery::capture_tcp(origin.url(2))
    );
    let day = verify_capture(directory, tools, source, 1, &day, flow, reference).await;
    let night = verify_capture(directory, tools, source, 2, &night, flow, reference).await;
    write_json(
        directory,
        "stream-proof.json",
        &json!({"channels":[day,night],"stock_g1_conformance":false}),
    );
    ordinary_stream_flow::reset_source_start(fixture).await;
    write_json(directory, "ready.json", &ready(origin, flow, source));
    let completion = live_client::wait_for_client(directory).await?;
    retained.assert_unchanged(fixture, flow).await;
    Ok(serde_json::to_value(completion).unwrap())
}

// Intentional Linux-x64 manual decoder/client diagnostic; other supported
// platforms require their own native acceptance. Product behavior is unchanged.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires a new G1 evidence directory, pinned reference/imager, exact FFmpeg9/TSDuck and separately operated headed stock clients"]
async fn published_g1_later_spacing_serves_ordinary_urls_with_stock_negative_evidence() {
    let directory = PathBuf::from(client_capture::selected("ERSATZRS_TELETEXT_G1_LIVE_DIR"));
    std::fs::create_dir(&directory).expect("G1 proof directory must be new");
    let (reference, identity) = reference();
    write_json(&directory, "reference.json", &identity);
    let tools = drcs_bitmap_delivery::Tools::load().await;
    let (fixture, source) = drcs_bitmap_delivery::playback_fixture(&tools, &directory).await;
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
        response,
        fixture,
        channel_documents::policy,
    ))
    .await;
    let retained = Retained::capture(&fixture).await;
    write_json(
        &directory,
        "native-preview.json",
        &json!({"channels":flow.accepted.iter().enumerate().map(|(index,candidate)|
        json!({"channel_id":index+1,"pages":preview(candidate.service())})).collect::<Vec<_>>(),"distinct_palette_claimed":false}),
    );
    let mut origin = live_client::LiveOrigin::start(&fixture).await.unwrap();
    let outcome = AssertUnwindSafe(served(
        &mut fixture,
        &flow,
        &retained,
        &origin,
        &directory,
        (&tools, &source),
        &reference,
    ))
    .catch_unwind()
    .await;
    let closed = origin.close().await;
    let completion = match outcome {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => {
            fixture.state.pool.close().await;
            panic!("G1 trial failed: {error}");
        }
        Err(error) => {
            fixture.state.pool.close().await;
            std::panic::resume_unwind(error);
        }
    };
    closed.expect("owned G1 server must close");
    let mut fixture = fixture.reopen().await;
    retained.assert_unchanged(&mut fixture, &flow).await;
    fixture.state.pool.close().await;
    write_json(
        &directory,
        "success.json",
        &json!({"completion":completion,"native_delivery_state_and_reopen_passed":true,
        "stock_g1_conformance":false,"actual_client_conformance_proven":false,"full_level25_proven":false,
        "limitations":"Attach actual stock-client images; the known later-spacing defect remains acceptance debt."}),
    );
}
