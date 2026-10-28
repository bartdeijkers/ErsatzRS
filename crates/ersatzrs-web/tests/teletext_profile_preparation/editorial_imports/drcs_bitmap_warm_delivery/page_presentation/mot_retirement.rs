//! Same-response publication of a bankless successor to a delivered neutral MOT.
//! Uses the existing real HTTP, source-clock, AV and transport proof machinery.
//! Installed POSIX producers inherit the parent's intentional Unix-only scope.

use super::*;
use ersatzrs_domain::{
    TeletextRawDataPageNumber, TeletextResourceIdentity, TeletextResourceSubpageIndex,
};
use ersatzrs_ffmpeg::teletext::{
    source_header::{observe_dvb_teletext_header, TeletextHeaderObservation},
    TeletextResourceFullUpdate, TeletextServiceResourceTransmission,
};
use ersatzrs_web::host::teletext_channel_documents::TeletextChannelDocumentEncodingPolicy;

#[path = "mot_retirement/populated.rs"]
mod populated;

#[path = "mot_retirement/secondary_defaults.rs"]
mod secondary_defaults;

const DIRECTORY_ENV: &str = "ERSATZRS_TELETEXT_MOT_RETIREMENT_DIR";

fn response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = authoring_metadata::response(theme, page, subcode);
    let mut mot = drcs_companion::response(theme, page, subcode)["service"]["mots"][0].clone();
    mot["associations"] = json!([]);
    mot["links"]["normal_drcs"] = json!(vec![mot["links"]["global_drcs"].clone(); 7]);
    mot["links"]["counts"]["magazine_mask"] = json!(0);
    mot["links"]["counts"]["drcs_pages"] = json!(0);
    let identity = mot["identity"].clone();
    value["service"]["mots"] = json!([mot]);
    for selection in value["service"]["mot_selections"].as_array_mut().unwrap() {
        selection["mot"] = identity.clone();
    }
    value
}

fn encoding(
) -> ersatzrs_web::host::teletext_channel_documents::TeletextChannelDocumentEncodingPolicy {
    let mut value = channel_documents::policy();
    value.transmissions = vec![TeletextServiceResourceTransmission {
        resource: TeletextResourceIdentity::new(
            TeletextResourceKind::Mot,
            TeletextRawDataPageNumber::new(1, 0xfe).unwrap(),
            TeletextResourceSubpageIndex::new(0).unwrap(),
        )
        .unwrap(),
        counter: ersatzrs_domain::TeletextResourceUpdateCounter::new(1).unwrap(),
        update: TeletextResourceFullUpdate::Changed,
        last_subpage: true,
    }];
    value
}

async fn pair(
    origin: &live_tcp::Origin,
    offsets: [usize; 2],
    day: &Pattern,
    night: &Pattern,
) -> [ObservedCycle; 2] {
    let captures = origin.captures();
    let (a, b) = tokio::join!(
        wait_cycle(&captures[0], offsets[0], day),
        wait_cycle(&captures[2], offsets[1], night)
    );
    [a, b]
}

async fn transition(
    fixture: &Fixture,
    flow: &Flow,
    origin: &live_tcp::Origin,
    directory: &Path,
    populated: bool,
) -> (AcceptedTeletextOutputCandidate, usize) {
    let old = pattern(&flow.accepted[0], 1);
    let night = pattern(&flow.accepted[1], 1);
    let before = pair(origin, [0; 2], &old, &night).await;
    for (index, cycle) in before.iter().enumerate() {
        write_units(
            directory,
            &format!("channel-{}-original-units.json", index + 1),
            cycle,
        );
    }
    let successor = if populated {
        external_editor_probe::editor_response("day", 124, 32)
    } else {
        authoring_metadata::response("day", 124, 32)
    };
    authoring_metadata::write(fixture, "day", &successor);
    let candidate = prepare_with_policy(fixture, 1, channel_documents::policy()).await;
    assert_eq!(candidate.selection(), flow.accepted[0].selection());
    let saved = designs::save(fixture, candidate, None).await;
    assert!(saved
        .candidate()
        .encoded()
        .unwrap()
        .declared()
        .mots()
        .is_empty());
    old_activation(fixture, flow, None).await;
    let pending = generations::request(fixture, &saved, Some(flow.pending[0].id)).await;
    old_activation(fixture, flow, Some(&pending)).await;
    let offsets = [
        origin.captures()[0].position(),
        origin.captures()[2].position(),
    ];
    let held = pair(origin, offsets, &old, &night).await;
    let offsets = [
        origin.captures()[0].position(),
        origin.captures()[2].position(),
    ];
    let accepted = generations::accepted(
        generations::accept(fixture, &pending, 1, Some(flow.accepted[0].generation().id)).await,
    );
    assert_ne!(accepted.generation().id, flow.accepted[0].generation().id);
    let next = pattern(&accepted, 0);
    let changed = pair(origin, offsets, &next, &night).await;
    if populated {
        populated::assert_prefix(&origin.captures()[0], offsets[0], &next, directory);
    }
    let repeated = pair(
        origin,
        changed.each_ref().map(|value| value.end),
        &next,
        &night,
    )
    .await;
    for index in 0..2 {
        write_units(
            directory,
            &format!("channel-{}-updated-units.json", index + 1),
            &changed[index],
        );
        write_units(
            directory,
            &format!("channel-{}-repeat-units.json", index + 1),
            &repeated[index],
        );
    }
    live_tcp::write_new(&directory.join("wire-offsets.json"), &serde_json::to_vec_pretty(&json!({
        "old_complete_pes_end":before.each_ref().map(|c|c.end),"pending_complete_pes_end":held.each_ref().map(|c|c.end),
        "pre_accept_offsets":offsets,"new_complete_pes_end":changed.each_ref().map(|c|c.end),"repeat_complete_pes_end":repeated.each_ref().map(|c|c.end),
        "old_generation_id":flow.accepted[0].generation().id,"new_generation_id":accepted.generation().id,"same_responses":true
    })).unwrap()).unwrap();
    // The existing literal audio oracle requires at least one second of source
    // tone; keep both already-open responses running beyond the wire handover.
    tokio::time::sleep(Duration::from_secs(2)).await;
    (accepted, changed[0].end)
}

fn is_mot_header(unit: &[u8; 46]) -> bool {
    matches!(observe_dvb_teletext_header(unit).unwrap(), TeletextHeaderObservation::Header(value)
        if value.packet_address.wire_magazine == 1 && value.wire_page == 0xfe)
}

fn assert_retired_tail(capture: &Capture, after: usize) {
    let mut cursor = after;
    let mut observed = false;
    loop {
        let units = capture.ordered_units_after(cursor, MAX_UNITS);
        observed |= !units.is_empty();
        assert!(units.iter().all(|(_, unit)| !is_mot_header(unit)));
        if units.len() < MAX_UNITS {
            break;
        }
        // A chunk can end inside one PES. Revisit its whole final group rather
        // than skipping units sharing that end offset. Bttx limits one PES to31
        // units, so every full512-unit chunk has an earlier complete group.
        let last = units.last().unwrap().0;
        let complete = units
            .iter()
            .rev()
            .find(|(offset, _)| *offset < last)
            .unwrap()
            .0;
        assert!(complete > cursor);
        cursor = complete;
    }
    assert!(observed, "the published successor must keep emitting");
}

async fn verify_captures(
    directory: &Path,
    tools: &Tools,
    source: &[u8],
    origin: &live_tcp::Origin,
) -> Vec<Value> {
    let mut receipts = Vec::new();
    for (channel, index) in [(1, 0), (2, 2)] {
        let mut bytes = origin.captures()[index].bytes();
        bytes.truncate(bytes.len() / 188 * 188);
        assert!(bytes.len() >= 188 * 1000);
        live_tcp::assert_uninterrupted_source_clock(&bytes);
        client_capture::verify_transport_with_qualified_authored_pts_gaps(
            &tools.tsanalyze,
            &tools.ffprobe,
            &bytes,
            0,
            Some(&directory.join(format!("channel-{channel}-tsduck.json"))),
            true,
        )
        .await;
        client_capture::verify_av_with_audio_bound(&tools.ffmpeg, source, &bytes, 6 * 1024 * 1024)
            .await;
        receipts.push(json!({"channel":channel,"bytes":bytes.len(),"sha256":live_tcp::hash(&bytes),"transport_clock_av_passed":true}));
    }
    receipts
}

#[tokio::test]
#[ignore = "requires FFmpeg9/TSDuck and a create-new neutral-MOT ordinary publication directory"]
async fn ordinary_two_channel_publication_retires_neutral_mot_on_original_responses() {
    run_case(false).await;
}

async fn run_case(populated: bool) {
    let directory_env = if populated {
        populated::DIRECTORY_ENV
    } else {
        DIRECTORY_ENV
    };
    let directory = PathBuf::from(client_capture::selected(directory_env));
    std::fs::create_dir(&directory).unwrap();
    live_tcp::terminal_trace(&directory).unwrap();
    let tools = Tools::load().await;
    let response: fn(&str, u16, u16) -> Value = if populated {
        drcs_companion::response
    } else {
        response
    };
    let encoding: fn() -> TeletextChannelDocumentEncodingPolicy = if populated {
        drcs_companion::encoding_policy
    } else {
        encoding
    };
    let (mut fixture, flow, source) = Box::pin(super::super::source_fixture_with_capacity(
        &tools,
        &directory,
        response,
        encoding,
        authoring_metadata::write,
        2,
    ))
    .await;
    assert_eq!(
        ersatzrs_infra::transcode_capacity::load_active_transcode_limit(&fixture.state.pool)
            .await
            .unwrap(),
        Some(2)
    );
    let retained = Retained::capture(&fixture).await;
    let mut origin = live_tcp::Origin::start(&fixture, &directory).await.unwrap();
    let mut observers = [
        live_tcp::Observer::start_for(&origin, 1, "day"),
        live_tcp::Observer::start_for(&origin, 2, "night"),
    ];
    let outcome = AssertUnwindSafe(async {
        await_two_channels(&origin).await;
        Box::pin(transition(&fixture, &flow, &origin, &directory, populated)).await
    })
    .catch_unwind()
    .await;
    let stopped = AssertUnwindSafe(async {
        for observer in &mut observers {
            observer.close().await.unwrap();
        }
        origin.close().await.unwrap();
        live_tcp::wait_for_cleanup().await.unwrap();
    })
    .catch_unwind()
    .await;
    live_tcp::save_captures(&directory, &origin).unwrap();
    if let Err(panic) = stopped {
        fixture.state.pool.close().await;
        std::panic::resume_unwind(panic);
    }
    let (accepted, after) = match outcome {
        Ok(value) => value,
        Err(panic) => {
            fixture.state.pool.close().await;
            std::panic::resume_unwind(panic)
        }
    };
    assert_eq!(origin.requests_seen(), [true, false, true]);
    assert_retired_tail(&origin.captures()[0], after);
    assert!(accepted.service().declared().mots().is_empty());
    assert_eq!(
        flow.accepted[0].service().magazine_units(),
        accepted.service().magazine_units()
    );
    if populated {
        populated::assert_models(flow.accepted[0].service(), accepted.service());
    } else {
        assert_identical_displays(flow.accepted[0].service(), accepted.service());
    }
    retained.assert_unchanged(&fixture, &flow).await;
    generations::assert_same_output(&recover(&fixture, 1).await, &accepted);
    fixture = fixture.reopen().await;
    retained.assert_unchanged(&fixture, &flow).await;
    generations::assert_same_output(&recover(&fixture, 1).await, &accepted);
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
    let receipts = verify_captures(&directory, &tools, &source, &origin).await;
    let name = if populated {
        "populated-mot-retirement.json"
    } else {
        "neutral-mot-retirement.json"
    };
    live_tcp::write_new(&directory.join(name), &serde_json::to_vec_pretty(&json!({
        "schema_version":1,"capacity":2,"requests_per_channel":1,"source_resets_after_start":0,
        "source_sha256":live_tcp::hash(&source),"channels":receipts,"channel2_unchanged":true,"explicit_publication":true,
        "pending_keeps_old_output":true,"old_mot_fully_delivered":true,"old_mot_populated":populated,"new_mot_absent":true,"entire_captured_tail_checked":true,"durable_reopen":true,"owned_cleanup":true,
        "limitations":"Same-response MOT publication/AV/transport only; not independent warm-client rendering, M29/full resource reconciliation, editor, platform or complete engine acceptance."
    })).unwrap()).unwrap();
}

async fn await_two_channels(origin: &live_tcp::Origin) {
    tokio::time::timeout(Duration::from_secs(15), async {
        while origin.requests_seen() != [true, false, true]
            || [0, 2]
                .iter()
                .any(|&index| origin.captures()[index].position() < 200 * 188)
        {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("the two original ordinary responses must attach at capacity2");
    assert_eq!(
        ersatzrs_infra::transcode_capacity::active_transcode_snapshot(2).active,
        2
    );
}

fn assert_identical_displays(
    old: &EncodedTeletextAuthoredService,
    next: &EncodedTeletextAuthoredService,
) {
    let displays = |service: &EncodedTeletextAuthoredService| {
        service
            .pages()
            .iter()
            .filter(|page| page.resource_header().is_none())
            .map(|page| page.data_units().to_vec())
            .collect::<Vec<_>>()
    };
    assert_eq!(displays(old), displays(next));
}
