//! Standalone1.5 publication over two retained ordinary HTTP responses at limit2.
//! Reuse the original bounded TCP owner/capture, clock, AV and transport oracles.

use super::super::super::editorial_imports::{
    live_updates::live_tcp, observed_cycle_units, wait_observed_cycle, ObservedCycle,
};
use super::*;
use futures_util::FutureExt as _;
use std::{panic::AssertUnwindSafe, path::Path};

fn changed_day() -> Value {
    let mut value = response("day");
    value["pages"][1]["rows"][0]["runs"][0]["content"]["text"] = json!("LIVE 200/0020");
    value["page_enhancements"][1]["local_program"]["commands"][2]["operation"]["diacritic"] =
        json!(8);
    value
}

fn pattern(candidate: &AcceptedTeletextOutputCandidate) -> Vec<Vec<[u8; 46]>> {
    let mut carousel =
        TeletextCarousel::new_authored_shared(Arc::clone(candidate.service())).unwrap();
    let first = observed_cycle_units(&mut carousel);
    let repeat = observed_cycle_units(&mut carousel);
    assert_eq!(first.len(), repeat.len());
    first
        .into_iter()
        .zip(repeat)
        .map(|(a, b)| vec![a, b])
        .collect()
}

async fn wait_pair(
    origin: &live_tcp::Origin,
    after: [usize; 2],
    day: &AcceptedTeletextOutputCandidate,
    night: &AcceptedTeletextOutputCandidate,
) -> [ObservedCycle; 2] {
    let views = origin.captures();
    let day = pattern(day);
    let night = pattern(night);
    let (a, b) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(
            wait_observed_cycle(&views[0], after[0], &day),
            wait_observed_cycle(&views[2], after[1], &night)
        )
    })
    .await
    .expect("complete original carousel within unchanged5s bound");
    assert_eq!(origin.requests_seen(), [true, false, true]);
    [a, b]
}

fn positions(origin: &live_tcp::Origin) -> [usize; 2] {
    [
        origin.captures()[0].position(),
        origin.captures()[2].position(),
    ]
}

fn write_stage(directory: &Path, name: &str, cycles: &[ObservedCycle; 2]) -> [usize; 2] {
    for (i, cycle) in cycles.iter().enumerate() {
        live_tcp::write_new(
            &directory.join(format!("{name}-{i}-units.json")),
            &serde_json::to_vec(
                &cycle
                    .units
                    .iter()
                    .map(|unit| unit.to_vec())
                    .collect::<Vec<_>>(),
            )
            .unwrap(),
        )
        .unwrap();
    }
    cycles.each_ref().map(|cycle| cycle.end)
}

async fn transition(
    fixture: &Fixture,
    directory: &Path,
    origin: &live_tcp::Origin,
    originals: &[AcceptedTeletextOutputCandidate; 2],
) -> AcceptedTeletextOutputCandidate {
    let original = wait_pair(origin, [0; 2], &originals[0], &originals[1]).await;
    let original_end = write_stage(directory, "original", &original);
    // This is upstream producer data outside the immutable installed package;
    // neither a template/package edit nor an editor roundtrip is claimed here.
    tokio::fs::write(
        fixture.root.path().join("ordinary-response-day.json"),
        serde_json::to_vec(&changed_day()).unwrap(),
    )
    .await
    .unwrap();
    let pending = request_base(fixture, 1, Some(originals[0].generation().request.id)).await;
    for (channel, original) in [1, 2].into_iter().zip(originals) {
        assert_eq!(
            fixture
                .state
                .application
                .accepted_teletext_generation(channel)
                .await
                .unwrap(),
            Some(original.generation().clone())
        );
    }
    let after_request = positions(origin);
    wait_pair(origin, after_request, &originals[0], &originals[1]).await;
    // A second complete acquisition after multiple real poll opportunities
    // prevents a queued pre-request cycle from proving pending-state isolation.
    tokio::time::sleep(Duration::from_millis(150)).await;
    let held_after = positions(origin);
    let held = wait_pair(origin, held_after, &originals[0], &originals[1]).await;
    let before_publish = write_stage(directory, "pending", &held);
    let edited = publish_base(fixture, pending, Some(originals[0].generation().id)).await;
    assert_ne!(edited.generation().id, originals[0].generation().id);
    for index in [0, 2] {
        assert_eq!(
            edited.service().pages()[index].data_units(),
            originals[0].service().pages()[index].data_units()
        );
    }
    assert_ne!(
        edited.service().pages()[1].data_units(),
        originals[0].service().pages()[1].data_units()
    );
    let changed = wait_pair(origin, before_publish, &edited, &originals[1]).await;
    let changed_end = write_stage(directory, "published", &changed);
    let repeated = wait_pair(origin, changed_end, &edited, &originals[1]).await;
    let repeated_end = write_stage(directory, "repeated", &repeated);
    // Keep enough actual source audio for the unchanged one-second AV oracle.
    // Pure raster checks run after capture shutdown, not on this test runtime's
    // single-thread real-time forwarding lane.
    tokio::time::sleep(Duration::from_secs(2)).await;
    let tail = positions(origin);
    wait_pair(origin, tail, &edited, &originals[1]).await;
    live_tcp::write_new(&directory.join("wire-offsets.json"),&serde_json::to_vec_pretty(&json!({"original":original_end,"request":after_request,"pending":before_publish,"published":changed_end,"repeated":repeated_end,"only_two_requests":true,"capacity":2})).unwrap()).unwrap();
    edited
}

async fn close(origin: &mut live_tcp::Origin, observers: &mut [live_tcp::Observer; 2]) {
    let mut failures = Vec::new();
    for observer in observers {
        if let Err(error) = observer.close().await {
            failures.push(error.to_string());
        }
    }
    if let Err(error) = origin.close().await {
        failures.push(error.to_string());
    }
    if let Err(error) = live_tcp::wait_for_cleanup().await {
        failures.push(error.to_string());
    }
    assert_eq!(
        ersatzrs_infra::transcode_capacity::active_transcode_snapshot(2).active,
        0
    );
    assert!(
        failures.is_empty(),
        "all owners closed; observed failures: {failures:?}"
    );
}

async fn verify(directory: &Path, tools: &Tools, source: &[u8], origin: &live_tcp::Origin) {
    for (i, viewer) in [0, 2].into_iter().enumerate() {
        let mut bytes = origin.captures()[viewer].bytes();
        bytes.truncate(bytes.len() / 188 * 188);
        live_tcp::assert_uninterrupted_source_clock(&bytes);
        client_capture::verify_transport_with_qualified_authored_pts_gaps(
            &tools.tsanalyze,
            &tools.ffprobe,
            &bytes,
            0,
            Some(&directory.join(format!("viewer-{i}-tsduck.json"))),
            true,
        )
        .await;
        client_capture::verify_av_with_audio_bound(&tools.ffmpeg, source, &bytes, 6 * 1024 * 1024)
            .await;
        let text =
            client_capture::decode_page_text_with_discovery(&tools.ffmpeg, &bytes, "200", true)
                .await;
        assert!(text.contains("BASE 200/"));
        if i == 1 {
            assert!(!text.contains("LIVE "));
        }
        live_tcp::write_new(&directory.join(format!("viewer-{i}-ordinary.ts")), &bytes).unwrap();
    }
}

#[tokio::test]
#[ignore = "requires pinned FFmpeg9/TSDuck and a create-new standalone update directory; native ordinary TCP proof, not headed receiver"]
async fn same_two_ordinary_responses_adopt_explicit_level15_subpage_publication() {
    let directory = PathBuf::from(client_capture::selected(
        "ERSATZRS_TELETEXT_LEVEL15_UPDATE_DIR",
    ));
    std::fs::create_dir(&directory).unwrap();
    live_tcp::terminal_trace(&directory).unwrap();
    let tools = Tools::load().await;
    let (mut fixture, source) = installed_profiles_with(&tools, response, 60).await;
    let limit =
        ersatzrs_infra::transcode_capacity::load_active_transcode_limit(&fixture.state.pool)
            .await
            .unwrap()
            .unwrap_or(ersatzrs_infra::transcode_capacity::DEFAULT_ACTIVE_TRANSCODE_LIMIT);
    assert_eq!(limit, 2);
    let mut output: Value = serde_json::from_str(POLICY).unwrap();
    output["version"] = json!(2);
    output["live_updates"] = json!({"poll_interval_ms":25,"recovery_timeout_ms":2000});
    setting(
        &fixture,
        POLICY_KEY,
        &serde_json::to_string(&output).unwrap(),
    )
    .await;
    let originals = [
        accept_base(&fixture, 1).await,
        accept_base(&fixture, 2).await,
    ];
    let configuration = designs::config(&fixture).await;
    let package = calculate_addon_content_sha256(&fixture.root.path().join("package"))
        .await
        .unwrap();
    live_tcp::write_new(&directory.join("source.ts"), &source).unwrap();
    reset_source_start(&fixture).await;
    let mut origin = live_tcp::Origin::start(&fixture, &directory).await.unwrap();
    let mut observers = [
        live_tcp::Observer::start_for(&origin, 1, "day"),
        live_tcp::Observer::start_for(&origin, 2, "night"),
    ];
    let outcome = AssertUnwindSafe(async {
        tokio::time::timeout(
            Duration::from_secs(30),
            transition(&fixture, &directory, &origin, &originals),
        )
        .await
        .expect("bounded standalone update")
    })
    .catch_unwind()
    .await;
    let stopped = AssertUnwindSafe(close(&mut origin, &mut observers))
        .catch_unwind()
        .await;
    // Keep raw negative captures too, without replacing an earlier artifact.
    for (i, index) in [0, 2].into_iter().enumerate() {
        live_tcp::write_new(
            &directory.join(format!("raw-viewer-{i}.ts")),
            &origin.captures()[index].bytes(),
        )
        .unwrap();
    }
    if let Err(panic) = stopped {
        fixture.state.pool.close().await;
        std::panic::resume_unwind(panic);
    }
    let edited = match outcome {
        Ok(value) => value,
        Err(panic) => {
            fixture.state.pool.close().await;
            std::panic::resume_unwind(panic)
        }
    };
    assert_base(edited.service());
    assert_eq!(origin.requests_seen(), [true, false, true]);
    assert_eq!(designs::config(&fixture).await, configuration);
    assert_eq!(
        calculate_addon_content_sha256(&fixture.root.path().join("package"))
            .await
            .unwrap(),
        package
    );
    assert_eq!(
        tokio::fs::read(fixture.root.path().join("ordinary-calls"))
            .await
            .unwrap(),
        b"day\nnight\nday\n"
    );
    assert!(fixture.playout.try_recv().is_err());
    fixture = fixture.reopen().await;
    assert_eq!(reload(&fixture, 1).await.generation(), edited.generation());
    assert_eq!(
        reload(&fixture, 2).await.generation(),
        originals[1].generation()
    );
    fixture.state.pool.close().await;
    verify(&directory, &tools, &source, &origin).await;
    live_tcp::write_new(&directory.join("success.json"),&serde_json::to_vec_pretty(&json!({"scope":"Native same-response standalone publication:one changed subpage,two themes,default capacity2; no new editor/headed client/platform acceptance","requests":[1,0,1],"configuration_retained":true,"package_retained":true,"no_addon_replay":true,"source_av_passed":true,"tsduck_passed":true,"source_clock_passed":true,"cleanup_confirmed":true})).unwrap()).unwrap();
}
