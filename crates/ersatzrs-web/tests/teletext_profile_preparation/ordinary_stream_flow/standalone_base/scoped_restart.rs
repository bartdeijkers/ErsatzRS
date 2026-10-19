//! Offline verification of real-host scoped Lua/restart ordinary captures.
//! No live clients, timers, restarts or authority are simulated by this verifier.

use super::*;

async fn export_units(prefix: &str, suffix: &str, bytes: &[u8]) {
    let mut assembler = SuppliedTeletextAssembler::new(512).unwrap();
    let mut units = Vec::new();
    for packet in bytes.chunks_exact(188) {
        if let TeletextAssemblyEvent::Complete(pes) =
            assembler.push(packet.try_into().unwrap()).unwrap()
        {
            units.extend(
                pes.units()
                    .filter_map(|unit| unit.teletext_unit())
                    .map(|unit| unit.to_vec()),
            );
        }
    }
    assembler.finish().unwrap();
    let mut output = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(format!("{prefix}.{suffix}.units.json"))
        .await
        .unwrap();
    output
        .write_all(&serde_json::to_vec(&units).unwrap())
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "requires actual headed scoped-host restart captures and pinned FFmpeg9/TSDuck"]
async fn served_level15_scoped_restart_retains_model_and_source_av() {
    let tools = Tools::load().await;
    let prefix = client_capture::selected("ERSATZRS_TELETEXT_SCOPED_RESTART_CAPTURE");
    let (fixture, _) = installed_profiles_with(&tools, response, 12).await;
    let source = tokio::fs::read(format!("{prefix}.source.ts"))
        .await
        .unwrap();
    let accepted = [
        accept_base(&fixture, 1).await,
        accept_base(&fixture, 2).await,
    ];
    for candidate in &accepted {
        assert_base(candidate.service());
    }
    for (suffix, index) in [
        ("on.before", 1),
        ("on.after", 1),
        ("isolated.night", 1),
        ("expired.day", 0),
        ("expired.night", 1),
    ] {
        let bytes = tokio::fs::read(format!("{prefix}.{suffix}.ts"))
            .await
            .unwrap();
        super::super::super::editorial_imports::live_updates::live_tcp::assert_uninterrupted_source_clock(&bytes);
        client_capture::verify_transport_with_qualified_authored_pts_gaps(
            &tools.tsanalyze,
            &tools.ffprobe,
            &bytes,
            0,
            Some(&PathBuf::from(format!("{prefix}.{suffix}.analyze.json"))),
            false,
        )
        .await;
        client_capture::verify_av(&tools.ffmpeg, &source, &bytes).await;
        assert_emitted_model(&bytes, accepted[index].service());
        export_units(&prefix, suffix, &bytes).await;
    }
    for suffix in ["off.before", "off.after"] {
        let bytes = tokio::fs::read(format!("{prefix}.{suffix}.ts"))
            .await
            .unwrap();
        super::super::super::editorial_imports::live_updates::live_tcp::assert_plain_source_clock(
            &bytes,
        );
        verify_plain(&tools, &source, &bytes).await;
        let streams: Value = serde_json::from_slice(
            &client_capture::probe_with_input(
                &tools.ffprobe,
                &[
                    "-v",
                    "error",
                    "-show_streams",
                    "-print_format",
                    "json",
                    "pipe:0",
                ],
                &bytes,
                262_144,
                client_capture::ToolInput::ProbePrefix,
            )
            .await,
        )
        .unwrap();
        assert!(streams["streams"]
            .as_array()
            .unwrap()
            .iter()
            .all(|stream| stream["codec_name"] != "dvb_teletext" && stream["id"] != "0x200"));
        let mut output = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(format!("{prefix}.{suffix}.analyze.json"))
            .await
            .unwrap();
        output
            .write_all(&client_capture::probe(&tools.tsanalyze, &["--json"], &bytes, 262_144).await)
            .await
            .unwrap();
    }
    fixture.state.pool.close().await;
}
