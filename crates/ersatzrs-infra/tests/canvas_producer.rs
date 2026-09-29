use std::{collections::BTreeMap, time::Duration};

use ersatzrs_ffmpeg::{
    build_canvas_command, CanvasRenderRequest, DrawvgOverlay, FrameRate, FrameSize,
    GraphicsPipelineOverlay, WatermarkOpacityClock,
};
use ersatzrs_infra::remote_stream::{
    PreparedRemoteStreamInput, RemoteStreamCommand, RemoteStreamSpawnContext,
};
use tokio::{io::AsyncReadExt as _, sync::oneshot, time::Instant};

#[tokio::test]
#[ignore = "requires the exact packaged FFmpeg 9 executable"]
#[allow(
    clippy::disallowed_methods,
    reason = "manual gate receives the pinned package from its runner"
)]
async fn canvas_producer_starts_before_eof_and_reaps_before_removing_graphics() {
    let executable = std::env::var("ERSATZRS_FFMPEG").expect("packaged FFmpeg executable");
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().to_owned();
    let script = path.join("canvas.vgs");
    std::fs::write(&script, "setcolor red\nrect 8 8 16 16 fill\n").unwrap();
    let overlay = DrawvgOverlay {
        script_path: script,
        x: 0,
        y: 0,
        width: 64,
        height: 64,
        enable: None,
    };
    let command = build_canvas_command(CanvasRenderRequest {
        resolution: FrameSize::new(64, 64),
        content_size: FrameSize::new(64, 64),
        frame_rate: FrameRate::new(4, 1).into(),
        duration: Duration::from_secs(3),
        opacity_clock: WatermarkOpacityClock::default(),
        overlays: vec![GraphicsPipelineOverlay::drawvg(0, overlay.filter())],
    })
    .unwrap();
    let command = RemoteStreamCommand {
        program: executable.clone(),
        args: command.arguments,
        tool_paths: Vec::new(),
        working_directory: path.clone(),
        environment: BTreeMap::new(),
    };
    let (_cancel, cancellation) = oneshot::channel();
    let input = PreparedRemoteStreamInput::start_with_graphics(
        &command,
        RemoteStreamSpawnContext::probe(),
        Instant::now() + Duration::from_secs(2),
        cancellation,
        directory,
        Vec::new(),
    )
    .await
    .expect("canvas admission must precede three-second EOF");
    assert!(!input.prefix().is_empty());
    assert!(path.join("canvas.vgs").exists());
    let (writer, mut reader) = tokio::io::duplex(4096);
    let mut transfer = input.transfer_to(writer).await.unwrap();
    let mut encoded = Vec::new();
    tokio::time::timeout(Duration::from_secs(6), reader.read_to_end(&mut encoded))
        .await
        .unwrap()
        .unwrap();
    transfer.wait().await.unwrap();
    assert!(
        !path.exists(),
        "only confirmed producer cleanup removes graphics"
    );

    let output = tempfile::tempdir().unwrap();
    let nut = output.path().join("canvas.nut");
    std::fs::write(&nut, encoded).unwrap();
    let decoded = tokio::process::Command::new(executable)
        .args(["-nostdin", "-v", "error", "-i"])
        .arg(nut)
        .args(["-pix_fmt", "bgra", "-f", "rawvideo", "pipe:1"])
        .kill_on_drop(true)
        .output()
        .await
        .unwrap();
    assert!(
        decoded.status.success(),
        "{}",
        String::from_utf8_lossy(&decoded.stderr)
    );
    assert_eq!(decoded.stdout.len(), 64 * 64 * 4 * 12);
    for frame in decoded.stdout.chunks_exact(64 * 64 * 4) {
        assert_eq!(frame[3], 0, "drawvg must preserve canvas background alpha");
        let pixel = &frame[(12 * 64 + 12) * 4..(12 * 64 + 12) * 4 + 4];
        assert_eq!(
            pixel,
            &[0, 0, 255, 255],
            "drawvg survives the supervised FFV1 transfer"
        );
    }
}
