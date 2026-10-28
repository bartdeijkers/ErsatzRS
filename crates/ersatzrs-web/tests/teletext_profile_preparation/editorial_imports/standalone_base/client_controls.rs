//! Join actual ordinary-stream captures to the existing base attribute oracle.
//! This prepares receiver trials; encoded flash/conceal flags are not proof of
//! timed flashing or manual reveal in Kodi. No physical device is contacted.

use super::super::super::client_capture;
use super::super::drcs_bitmap_delivery::captured_units;
use ersatzrs_ffmpeg::teletext::source_header::{
    observe_dvb_teletext_header, TeletextHeaderObservation,
};
use futures_util::FutureExt as _;
use serde_json::{json, Value};
use std::{io::Write as _, panic::AssertUnwindSafe, path::Path};

fn cells(controls: bool) -> Vec<Value> {
    let steady = json!([0, 0, false, false, false, false]);
    let cell = |row, column, expected: Value, attributes: Value| {
        json!({"row":row,"column":column,"l1":expected,"l15":expected,"l25":expected,
            "attributes":{"l1":attributes,"l15":attributes,"l25":attributes}})
    };
    let mut result = vec![cell(1, 8, json!([66, 15, 0, 1, 0, 0]), steady.clone())];
    for (column, enhanced) in [
        (8, json!([35, 7, 0, 7, 0, 0])),
        (9, json!([65, 0, 2, 7, 0, 0])),
        (10, json!([47, 26, 0, 7, 0, 0])),
    ] {
        let mut value = cell(2, column, json!([90, 15, 0, 7, 0, 0]), steady.clone());
        value["l15"] = enhanced.clone();
        value["l25"] = enhanced;
        result.push(value);
    }
    if controls {
        for (row, column, expected, attributes) in [
            (5, 8, json!([83, 15, 0, 7, 0, 0]), steady.clone()),
            (
                7,
                8,
                json!([72, 15, 0, 7, 0, 0]),
                json!([0, 0, true, false, false, false]),
            ),
            (
                9,
                8,
                json!([70, 15, 0, 7, 0, 0]),
                json!([1, 0, false, false, false, false]),
            ),
            (11, 8, json!([68, 15, 0, 7, 0, 1]), steady.clone()),
            (12, 8, json!([68, 15, 0, 7, 0, 2]), steady.clone()),
            (14, 8, json!([127, 24, 0, 5, 0, 0]), steady.clone()),
            (15, 8, json!([127, 25, 0, 6, 0, 0]), steady.clone()),
            (17, 15, json!([65, 15, 0, 7, 0, 0]), steady.clone()),
            (
                19,
                8,
                json!([66, 15, 0, 7, 0, 0]),
                json!([0, 0, false, true, false, false]),
            ),
        ] {
            result.push(cell(row, column, expected, attributes));
        }
    }
    result
}

fn acquisition(units: &[Vec<u8>], page: u16, subcode: u16) -> (usize, usize) {
    let mut first = None;
    for (index, unit) in units.iter().enumerate() {
        let unit: &[u8; 46] = unit.as_slice().try_into().unwrap();
        match observe_dvb_teletext_header(unit).unwrap() {
            TeletextHeaderObservation::Header(header)
            | TeletextHeaderObservation::Terminator(header) => {
                if let Some(first) = first {
                    return (first, index + 1); // Include the actual closing header, never synthesize one.
                }
                if header
                    .display_page()
                    .is_some_and(|value| value.get() == page)
                    && header.subcode == subcode
                {
                    first = Some(index);
                }
            }
            TeletextHeaderObservation::NotHeader(address) => {
                assert!(![28, 29].contains(&address.packet_number));
            }
            TeletextHeaderObservation::NotTeletext { .. } => {
                panic!("expected only checked authored packet envelopes");
            }
        }
    }
    panic!("missing complete captured page {page}/{subcode:04x}");
}

fn save(directory: &Path, name: &str, value: &Value) {
    std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(directory.join(name))
        .unwrap()
        .write_all(&serde_json::to_vec_pretty(value).unwrap())
        .unwrap();
}

#[tokio::test]
#[ignore = "requires owned ordinary capture/source and pinned imager; not actual client acceptance"]
async fn ordinary_client_controls_capture_preserves_base_attributes_and_selected_audio() {
    let directory = client_capture::selected("ERSATZRS_TELETEXT_LEVEL15_CLIENT_DIR");
    let directory = Path::new(&directory);
    let controls = client_capture::selected("ERSATZRS_TELETEXT_CLIENT_LEVEL15_CONTROLS") == "1";
    let ready: Value =
        serde_json::from_slice(&std::fs::read(directory.join("host-ready.json")).unwrap()).unwrap();
    assert_eq!(
        ready["fixture_kind"],
        if controls {
            "level15-controls"
        } else {
            "standalone-base"
        }
    );
    let capture = directory.join("ordinary-prefix.ts");
    let metadata = std::fs::symlink_metadata(&capture).unwrap();
    assert!(metadata.is_file() && (188..=8 * 1024 * 1024).contains(&metadata.len()));
    let bytes = std::fs::read(&capture).unwrap();
    let units = captured_units(&bytes);
    let mut boundaries = Vec::new();
    let cases: Vec<_> = [(200, 0), (200, 32), (201, 0)].into_iter().map(|(page, subcode)| {
        let (first, end) = acquisition(&units, page, subcode);
        boundaries.push(json!({"page":page,"subcode":subcode,"first":first,"end":end}));
        json!({"name":format!("base_client_{page}_{subcode}"),"page":if page==200 {0x200} else {0x201},
            "subcode":subcode,"units":&units[first..end],"cells":cells(controls)})
    }).collect();
    let request = json!({"expected_version":"0.2.0","cases":cases});
    let proof = directory.join("native-proof");
    std::fs::create_dir(&proof).unwrap();
    save(&proof, "request.json", &request);
    let report = verify_decoder(&request, controls).await;
    let audio = verify_av(directory, &proof, &bytes, controls).await;
    save(
        &proof,
        "result.json",
        &json!({"independent_decoder":report,"page_boundaries":boundaries,
        "transport":"passed","source_video":"literal calibration frame preserved","selected_audio":audio,
        "tone_hz":if controls {660} else {440},"negative_flags":if controls {2} else {0},
        "scope":"Base attributes/AV preparation only; existing oracle also checks advanced decoder mode on the same base pages. No physical client, timed flash/reveal or full1.5 acceptance."}),
    );
}

async fn verify_decoder(request: &Value, controls: bool) -> Value {
    let helper = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/e2e/teletext-base-footprint-decoder.py");
    let helper = helper.to_str().unwrap();
    let report = client_capture::probe(
        "python3",
        &[helper],
        &serde_json::to_vec(request).unwrap(),
        8192,
    )
    .await;
    let report: Value = serde_json::from_slice(&report).unwrap();
    assert_eq!(report["cases_passed"], 3);
    if controls {
        for (row, attribute, expected_text) in [(7, 2, "attributes 7,8"), (9, 0, "attributes 9,8")]
        {
            let mut wrong = request.clone();
            let cell = wrong["cases"][0]["cells"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|cell| cell["row"] == row)
                .unwrap();
            cell["attributes"]["l15"][attribute] = if attribute == 0 {
                json!(0)
            } else {
                json!(false)
            };
            let error = AssertUnwindSafe(client_capture::probe(
                "python3",
                &[helper],
                &serde_json::to_vec(&wrong).unwrap(),
                8192,
            ))
            .catch_unwind()
            .await
            .expect_err("wrong control flag must fail");
            let message = error.downcast_ref::<String>().unwrap();
            assert!(message.contains(expected_text), "{message}");
        }
    }
    report
}

async fn verify_av(directory: &Path, proof: &Path, bytes: &[u8], controls: bool) -> Vec<Value> {
    let ffmpeg = client_capture::selected("ERSATZRS_TELETEXT_TEST_FFMPEG");
    let ffprobe = client_capture::selected("FFPROBE_PATH");
    let tsanalyze = client_capture::selected("ERSATZRS_TELETEXT_TEST_TSANALYZE");
    client_capture::verify_transport_for_audio(
        &tsanalyze,
        &ffprobe,
        bytes,
        0,
        Some(&proof.join("tsduck.json")),
        false,
        if controls { "nld" } else { "eng" },
    )
    .await;
    // The fixture is a literal constant calibration clip. Read one real source
    // frame from the offline copy rather than imposing a duration/EOF limit on
    // the longer human-controlled fixture; never rewrite the delivered capture.
    let source = directory.join("source.ts");
    let reference = client_capture::probe(
        &ffmpeg,
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-i",
            source.to_str().unwrap(),
            "-map",
            "0:v:0",
            "-an",
            "-frames:v",
            "1",
            "-c:v",
            "copy",
            "-f",
            "mpegts",
            "pipe:1",
        ],
        &[],
        1024 * 1024,
    )
    .await;
    client_capture::verify_av_with_tone(
        &ffmpeg,
        &reference,
        bytes,
        2 * 1024 * 1024,
        if controls { 660 } else { 440 },
    )
    .await;
    let streams = client_capture::probe(
        &ffprobe,
        &[
            "-v",
            "error",
            "-show_streams",
            "-of",
            "json",
            "-f",
            "mpegts",
            "pipe:0",
        ],
        bytes,
        262_144,
    )
    .await;
    let streams: Value = serde_json::from_slice(&streams).unwrap();
    let audio: Vec<_> = streams["streams"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|stream| stream["codec_type"] == "audio")
        .collect();
    assert_eq!(audio.len(), 1);
    assert_eq!(
        audio[0]["tags"]["language"],
        if controls { "nld" } else { "eng" }
    );
    audio.into_iter().cloned().collect()
}
