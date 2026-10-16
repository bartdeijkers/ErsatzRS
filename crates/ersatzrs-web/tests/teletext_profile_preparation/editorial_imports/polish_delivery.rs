//! Genuine QTM Polish Base concept to cold ordinary TCP delivery on two profiles.
//! Captures establish native transport/model equality, not independent decoding,
//! headed clients, warm handover or complete authored Level 2.5 acceptance.

use super::super::{client_capture, ordinary_stream_flow};
use super::*;
use ersatzrs_domain::{ChannelTeletextBindingRead, TeletextProfileRead};
use ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService;
use futures_util::FutureExt as _;
use serde_json::{json, Value};
use std::{panic::AssertUnwindSafe, path::PathBuf};

pub(super) struct Retained {
    package: String,
    profiles: Vec<TeletextProfileRead>,
    bindings: [Vec<ChannelTeletextBindingRead>; 2],
    inputs: Vec<(String, String)>,
}

impl Retained {
    pub(super) async fn capture(fixture: &Fixture, flow: &Flow, actual: &Path) -> Self {
        let mut names: Vec<_> = flow.exported[0]
            .files()
            .iter()
            .map(|file| format!("native-original-1/{}", file.name))
            .collect();
        names.push("editor-1/edited.tti".into());
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
            inputs: names
                .into_iter()
                .map(|name| {
                    let bytes = drcs_bitmap::read_retained_editor_file(&actual.join(&name));
                    (name, live_client::hash(&bytes))
                })
                .collect(),
        }
    }

    pub(super) async fn assert_unchanged(
        &self,
        fixture: &mut Fixture,
        flow: &Flow,
        accepted: &[AcceptedTeletextOutputCandidate; 2],
        actual: &Path,
    ) {
        assert_retained_storage(fixture, flow).await;
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
            let channel = i64::try_from(index + 1).unwrap();
            assert_eq!(
                fixture
                    .state
                    .application
                    .channel_teletext_bindings(channel)
                    .await
                    .unwrap(),
                *expected
            );
            generations::assert_same_output(&recover(fixture, channel).await, &accepted[index]);
        }
        generations::assert_same_output(&accepted[1], &flow.accepted[1]);
        for (name, hash) in &self.inputs {
            assert_eq!(
                live_client::hash(&drcs_bitmap::read_retained_editor_file(&actual.join(name))),
                *hash
            );
        }
        assert!(
            fixture.playout.try_recv().is_err(),
            "publication must not refresh playout"
        );
    }
}

pub(super) fn write_json(directory: &Path, name: &str, value: &impl serde::Serialize) {
    let bytes = serde_json::to_vec(value).unwrap();
    assert!(bytes.len() <= 128 * 1024);
    live_client::write_new(&directory.join(name), &bytes).unwrap();
}

fn preview_summary(image: &TeletextRaster) -> Value {
    let pixels: Vec<u8> = image
        .pixels()
        .iter()
        .flat_map(|pixel| match pixel {
            TeletextRasterPixel::ReceiverHeader => [0, 0, 0, 0],
            TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(rgb)) => {
                let [red, green, blue] = rgb.get();
                [1, red, green, blue]
            }
            TeletextRasterPixel::Colour(TeletextColourPixel::Video) => [2, 0, 0, 0],
            TeletextRasterPixel::Unresolved(_) => panic!("shared preview must be fully resolved"),
        })
        .collect();
    json!({"width":image.width(),"height":image.height(),"pixels_sha256":live_client::hash(&pixels),
        "hash_encoding":"row-major four bytes per pixel: receiver=[0,0,0,0], RGB4=[1,r,g,b], video=[2,0,0,0]"})
}

fn assert_theme_preview(service: &EncodedTeletextAuthoredService, foreground: [u8; 3]) -> Value {
    let composition = service
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::NativeDisplay(page)
                if page.composition().owner() == channel_documents::address(125, 32) =>
            {
                Some(page.composition())
            }
            EncodedTeletextAuthoredPage::CompiledDisplay(page)
                if page.page().address() == channel_documents::address(125, 32) =>
            {
                Some(page.normalized().original())
            }
            _ => None,
        })
        .unwrap();
    let left = usize::from(composition.display_geometry().left_width());
    let raster = external_editor_probe::preview_page(service, 125);
    let rows = [
        0x801_u16, 0x001, 0x002, 0x004, 0x008, 0x010, 0x020, 0x040, 0x080, 0x900,
    ];
    for column in [8, 10] {
        for (row, pixels) in rows.into_iter().enumerate() {
            for x in 0..12 {
                let rgb = if pixels & (1 << (11 - x)) == 0 {
                    [0, 0, 0]
                } else {
                    foreground
                };
                assert!(
                    matches!(raster.pixel((left + column) * 12 + x, 10 + row),
                    Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(value))) if value.get() == rgb),
                    "page125 column{column} pixel{x},{row}"
                );
            }
        }
    }
    json!({"page":125,"columns":[8,10],"foreground_rgb4":foreground,
        "background_rgb4":[0,0,0],"exact_bitmap_pixels_checked":240,"preview":preview_summary(&raster)})
}

pub(super) fn assert_published_previews(
    directory: &Path,
    service: &EncodedTeletextAuthoredService,
    accepted: &[AcceptedTeletextOutputCandidate; 2],
) {
    let mut previews = Vec::new();
    for page in [124, 125] {
        let before = external_editor_probe::preview_page(service, page);
        let published = external_editor_probe::preview_page(accepted[0].service(), page);
        assert_eq!(
            before, published,
            "shared concept and publication preview differ"
        );
        previews.push(json!({"page":page,"concept_equals_published":true,"preview":preview_summary(&published)}));
    }
    let day_theme = assert_theme_preview(accepted[0].service(), [15, 0, 0]);
    let night_theme = assert_theme_preview(accepted[1].service(), [0, 0, 15]);
    write_json(
        directory,
        "native-preview.json",
        &json!({"probe_version":1,
        "day_base_row":1,"day_base_column":7,"original_code":65,"concept_code":91,
        "primary":8,"day":previews,"page125_theme":[day_theme,night_theme],
        "night":preview_summary(&external_editor_probe::preview_page(accepted[1].service(),124))}),
    );
}

async fn publish_genuine(
    fixture: &Fixture,
    flow: &Flow,
    actual: &Path,
    directory: &Path,
) -> [AcceptedTeletextOutputCandidate; 2] {
    for original in &flow.accepted {
        polish_carrier::assert_base(polish_carrier::composition(original.service()), b'A');
    }
    let concept = import(
        fixture,
        polish_carrier::genuine_polish_request(flow, actual),
    )
    .await;
    assert_eq!(concept.head().id, flow.saved[0].head().id);
    assert_eq!(concept.head().revision, 2);
    assert_original_pointers(fixture, flow).await;
    let restored = drcs_bitmap::restored(fixture, &concept).await;
    let service = restored.encoded().unwrap();
    polish_carrier::assert_base(polish_carrier::composition(service), 0x5b);
    assert_eq!(restored.selection(), flow.saved[0].candidate().selection());
    assert_eq!(
        restored.transmissions(),
        flow.saved[0].candidate().transmissions()
    );
    assert_eq!(
        flow.accepted[0].service().pages().len(),
        service.pages().len()
    );
    for (before, after) in flow.accepted[0]
        .service()
        .pages()
        .iter()
        .zip(service.pages())
    {
        if matches!(before, EncodedTeletextAuthoredPage::CompiledDisplay(page)
            if page.page().address().page.get() == 124)
        {
            continue;
        }
        assert_eq!(before.data_units(), after.data_units());
    }
    let raster = external_editor_probe::preview_page(service, 124);
    assert_ne!(
        raster,
        external_editor_probe::preview_page(flow.accepted[0].service(), 124)
    );
    let day = publish_and_accept(fixture, flow, &concept).await;
    assert_ne!(day.generation().id, flow.accepted[0].generation().id);
    assert_eq!(day.generation().request.design_id, concept.head().id);
    assert_eq!(day.generation().request.revision, 2);
    assert_eq!(day.generation().request.digest, concept.digest());
    polish_carrier::assert_base(polish_carrier::composition(day.service()), 0x5b);
    let accepted = [day, recover(fixture, 2).await];
    generations::assert_same_output(&accepted[1], &flow.accepted[1]);
    let first = &accepted[0].selection().bindings[0];
    let second = &accepted[1].selection().bindings[0];
    assert_eq!(first.addon_id, second.addon_id);
    assert_ne!(first.profile_id, second.profile_id);
    assert_published_previews(directory, service, &accepted);
    accepted
}

pub(super) async fn verify_capture(
    directory: &Path,
    tools: &drcs_bitmap_delivery::Tools,
    source: &[u8],
    channel: i64,
    bytes: &[u8],
    accepted: &AcceptedTeletextOutputCandidate,
) -> Value {
    let capture = format!("channel-{channel}.ts");
    live_client::write_new(&directory.join(&capture), bytes).unwrap();
    let report_name = format!("channel-{channel}-tsduck.json");
    let report = client_capture::verify_transport_with_qualified_authored_pts_gaps(
        &tools.tsanalyze,
        &tools.ffprobe,
        bytes,
        0,
        Some(&directory.join(&report_name)),
        false,
    )
    .await;
    client_capture::verify_av(&tools.ffmpeg, source, bytes).await;
    ordinary_stream_flow::assert_emitted_model(bytes, accepted.service());
    let units = drcs_bitmap_delivery::captured_units(bytes);
    let units_name = format!("channel-{channel}-units.json");
    write_json(directory, &units_name, &units);
    let units_json = serde_json::to_vec(&units).unwrap();
    let binding = &accepted.selection().bindings[0];
    json!({"channel_id":channel,"theme":if channel==1 {"day"} else {"night"},
        "addon_id":binding.addon_id,"profile_id":binding.profile_id,"profile_revision":binding.profile_revision,
        "design_id":accepted.generation().request.design_id,"revision":accepted.generation().request.revision,
        "digest":accepted.generation().request.digest,"generation_id":accepted.generation().id,
        "request_id":accepted.generation().request.id,"capture":capture,"capture_bytes":bytes.len(),
        "capture_sha256":live_client::hash(bytes),"transport_report":report_name,"transport_report_sha256":live_client::hash(&report),
        "complete_emitted_model_equal":true,"source_av_passed":true,"tsduck_transport_passed":true,
        "units_file":units_name,"unit_count":units.len(),"units_sha256":live_client::hash(&units_json),
        "units_scope":"chronological first at most512 actual teletext units; may end within acquisition; full TS prefix is retained unchanged",
        "independent_decoding_proven":false,"actual_client_proven":false})
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires retained genuine QTM Polish Save As and exact FFmpeg9/ffprobe/TSDuck; no independent or headed client claim"]
async fn actual_qtm_polish_concept_serves_two_ordinary_channels() {
    let directory = PathBuf::from(client_capture::selected(
        "ERSATZRS_TELETEXT_POLISH_DELIVERY_DIR",
    ));
    std::fs::create_dir(&directory).expect("Polish delivery destination must be new");
    let actual = PathBuf::from(client_capture::selected(
        "ERSATZRS_QTM_POLISH_CALIBRATION_DIR",
    ));
    let tools = drcs_bitmap_delivery::Tools::load().await;
    let (fixture, source) = drcs_bitmap_delivery::playback_fixture(&tools, &directory).await;
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
        polish_carrier::response,
        fixture,
        drcs_companion::dense_encoding_policy,
    ))
    .await;
    let retained = Retained::capture(&fixture, &flow, &actual).await;
    let accepted = Box::pin(publish_genuine(&fixture, &flow, &actual, &directory)).await;
    retained
        .assert_unchanged(&mut fixture, &flow, &accepted, &actual)
        .await;
    ordinary_stream_flow::reset_source_start(&fixture).await;
    let mut origin = live_client::LiveOrigin::start(&fixture).await.unwrap();
    let outcome = AssertUnwindSafe(async {
        let (day, night) = tokio::join!(
            drcs_bitmap_delivery::capture_tcp(origin.url(1)),
            drcs_bitmap_delivery::capture_tcp(origin.url(2))
        );
        let day = verify_capture(&directory, &tools, &source, 1, &day, &accepted[0]).await;
        let night = verify_capture(&directory, &tools, &source, 2, &night, &accepted[1]).await;
        retained.assert_unchanged(&mut fixture, &flow, &accepted, &actual).await;
        let proof = json!({"probe_version":1,"route":"/iptv/hls-direct/{channel}.ts",
            "transport":"actual application loopback TCP host","scope":"genuine QTM Polish Base concept, explicit publication, cold ordinary delivery",
            "source_file":"source.ts","source_sha256":live_client::hash(&source),"source_bytes":source.len(),
            "channels":[day,night],"genuine_inputs":retained.inputs,
            "originals_receipts_profiles_bindings_package_config_unchanged":true,
            "independent_decoding_proven":false,"actual_client_proven":false,"full_level25_proven":false});
        write_json(&directory, "stream-proof.json", &proof);
    }).catch_unwind().await;
    let closed = origin.close().await;
    let cleaned = live_updates::live_tcp::wait_for_cleanup().await;
    if let Err(panic) = outcome {
        fixture.state.pool.close().await;
        std::panic::resume_unwind(panic);
    }
    closed.expect("owned Polish delivery origin must close");
    cleaned.expect("ordinary response producers must release owned capacity");
    fixture = fixture.reopen().await;
    retained
        .assert_unchanged(&mut fixture, &flow, &accepted, &actual)
        .await;
    polish_carrier::assert_base(
        polish_carrier::composition(recover(&fixture, 1).await.service()),
        0x5b,
    );
    fixture.state.pool.close().await;
    write_json(
        &directory,
        "success.json",
        &json!({"probe_version":1,
        "genuine_qtm_import_preview_explicit_publication_and_ordinary_delivery_passed":true,
        "source_av_transport_complete_model_and_isolation_reopen_passed":true,"owned_origin_closed":true,"capacity_released":true,
        "independent_decoding_proven":false,"actual_client_proven":false,"full_level25_proven":false,
        "limitations":"Cold native ordinary TCP delivery only; independent decoder, headed admin/client, warm transition, panels, full engine and platform acceptance remain separate"}),
    );
}
