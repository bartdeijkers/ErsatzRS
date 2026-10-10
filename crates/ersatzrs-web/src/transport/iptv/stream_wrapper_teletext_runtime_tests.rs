//! Exact-runtime proof of one final legacy concat wrapper over finite HTTP
//! programmes. Capacity slates, full routed admission, Next and native packaged
//! acceptance remain separate gates; this fixture uses no live database.

use std::{
    collections::BTreeSet,
    io,
    sync::{atomic::AtomicUsize, Mutex},
};

use axum::{routing::get, Router};
use futures_util::StreamExt as _;
use tokio::io::AsyncWriteExt as _;

use super::super::{
    teletext_output::tests::authored_set,
    teletext_retained_output::runtime_tests::{
        decode_text, exact_tool, probe, verify_tsduck, AV_DIGEST_ARGS,
    },
};
use super::*;

const MUXRATE: u64 = 5_000_000;
const MAX_BYTES: usize = 8 * 1024 * 1024;
const CAPTIONS: [&str; 2] = ["Programme A caption page888", "Programme B caption page888"];

fn pid(packet: &[u8]) -> u16 {
    (u16::from(packet[1] & 0x1f) << 8) | u16::from(packet[2])
}

fn caption_fixture(caption: &str) -> Vec<u8> {
    fn wire(text: &[u8]) -> Vec<u8> {
        text.iter()
            .map(|byte| {
                let parity = if byte.count_ones() % 2 == 0 { 0x80 } else { 0 };
                (byte | parity).reverse_bits()
            })
            .collect()
    }
    let mut bytes = ersatzrs_ffmpeg::teletext_probe::TELETEXT_DECODE_FIXTURE.to_vec();
    let original = wire(b"Synthetic subtitle page 888");
    let replacement = wire(caption.as_bytes());
    assert_eq!(original.len(), replacement.len());
    let offsets: Vec<_> = bytes
        .windows(original.len())
        .enumerate()
        .filter_map(|(offset, value)| (value == original).then_some(offset))
        .collect();
    assert_eq!(
        offsets.len(),
        8,
        "all independently generated caption cycles"
    );
    for offset in offsets {
        bytes[offset..offset + original.len()].copy_from_slice(&replacement);
    }
    bytes
}

async fn programme(ffmpeg: &str, index: usize) -> Vec<u8> {
    let color = [
        "color=c=red:size=160x90:rate=25",
        "color=c=blue:size=160x90:rate=25",
    ][index];
    let english = [
        "sine=frequency=440:sample_rate=48000",
        "sine=frequency=880:sample_rate=48000",
    ][index];
    let dutch = [
        "sine=frequency=660:sample_rate=48000",
        "sine=frequency=1320:sample_rate=48000",
    ][index];
    let args = [
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostdin",
        "-f",
        "lavfi",
        "-i",
        color,
        "-f",
        "lavfi",
        "-i",
        english,
        "-f",
        "lavfi",
        "-i",
        dutch,
        "-f",
        "mpegts",
        "-i",
        "pipe:0",
        "-t",
        "2",
        "-map",
        "0:v:0",
        "-map",
        "1:a:0",
        "-map",
        "3:s:0",
        "-map",
        "2:a:0",
        "-c:v",
        "mpeg2video",
        "-b:v",
        "200k",
        "-c:a",
        "aac",
        "-b:a",
        "64k",
        "-c:s",
        "copy",
        "-metadata:s:a:0",
        "language=eng",
        "-metadata:s:a:1",
        "language=nld",
        "-muxrate",
        "5000000",
        "-pcr_period",
        "20",
        "-mpegts_start_pid",
        "256",
        "-mpegts_pmt_start_pid",
        "4096",
        "-f",
        "mpegts",
        "pipe:1",
    ];
    let bytes = probe(ffmpeg, &args, &caption_fixture(CAPTIONS[index]), MAX_BYTES)
        .await
        .expect("synthetic programme generation failed");
    assert_eq!(bytes.len() % 188, 0);
    assert!(bytes.chunks_exact(188).all(|packet| pid(packet) != 512));
    bytes
}

type ProgrammeWrites = Arc<Mutex<Vec<tokio::task::JoinHandle<io::Result<()>>>>>;

#[derive(Clone)]
struct Programmes {
    bytes: [Arc<[u8]>; 2],
    requests: Arc<AtomicUsize>,
    opened: Arc<Mutex<Vec<usize>>>,
    writers: ProgrammeWrites,
}

impl Programmes {
    fn response(&self) -> Response {
        let request = self.requests.fetch_add(1, Ordering::AcqRel);
        if request >= 16 {
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
        let index = request % 2;
        self.opened.lock().unwrap().push(index);
        let bytes = Arc::clone(&self.bytes[index]);
        let cancel = Arc::new(Notify::new());
        let (reader, mut writer) = tokio::io::duplex(4096);
        let (body, completion) = stream_body_for_output(
            reader,
            Arc::clone(&cancel),
            &TeletextOutputAdmission::plain(TeletextOutputRole::InternalItem),
        );
        assert!(completion.is_none());
        let task = tokio::spawn(async move {
            tokio::select! {
                biased;
                () = cancel.notified() => return Ok(()),
                result = writer.write_all(&bytes) => result?,
            }
            writer.shutdown().await
        });
        self.writers.lock().unwrap().push(task);
        let mut response = body.into_response();
        response
            .headers_mut()
            .insert(header::CONTENT_TYPE, HeaderValue::from_static("video/mp2t"));
        assert!(!response.headers().contains_key(header::CONTENT_LENGTH));
        response
    }
}

struct ProgrammeServer {
    address: std::net::SocketAddr,
    task: Option<tokio::task::JoinHandle<()>>,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    programmes: Programmes,
}

impl ProgrammeServer {
    async fn start(bytes: [Arc<[u8]>; 2]) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let programmes = Programmes {
            bytes,
            requests: Arc::new(AtomicUsize::new(0)),
            opened: Arc::new(Mutex::new(Vec::new())),
            writers: Arc::new(Mutex::new(Vec::new())),
        };
        let served = programmes.clone();
        let playlist = ConcatPlaylist::new("http", address.to_string(), "1", "ts-legacy").render();
        let app = Router::new()
            .route(
                "/ffmpeg/stream/1",
                get(move || {
                    let served = served.clone();
                    async move { served.response() }
                }),
            )
            .route(
                "/ffmpeg/concat/1",
                get(move || {
                    let playlist = playlist.clone();
                    async move { playlist }
                }),
            );
        let (shutdown, receiver) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = receiver.await;
                })
                .await
                .unwrap();
        });
        Self {
            address,
            task: Some(task),
            shutdown: Some(shutdown),
            programmes,
        }
    }

    async fn stop(mut self) {
        let _ = self.shutdown.take().unwrap().send(());
        let mut task = self.task.take().unwrap();
        if tokio::time::timeout(Duration::from_secs(2), &mut task)
            .await
            .is_err()
        {
            task.abort();
            let _ = task.await;
        }
        let tasks = std::mem::take(&mut *self.programmes.writers.lock().unwrap());
        for mut task in tasks {
            if tokio::time::timeout(Duration::from_secs(2), &mut task)
                .await
                .is_err()
            {
                task.abort();
                let _ = task.await;
            }
        }
    }
}

impl Drop for ProgrammeServer {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(task) = &self.task {
            task.abort();
        }
        for task in self.programmes.writers.lock().unwrap().iter() {
            task.abort();
        }
    }
}

fn wrapper_output() -> TeletextOutputAdmission {
    let binding = TeletextOutputBinding::new(
        ersatzrs_ffmpeg::teletext::TeletextOutputConfig {
            program: 1,
            elementary_pid: 512,
            clock: ersatzrs_ffmpeg::teletext::TeletextClockConfig {
                pcr_pid: 256,
                muxrate_bps: MUXRATE,
                rate_tolerance_ppm: 1000,
                pcr_jitter_ticks: 14,
                max_pcr_gap_packets: 134,
                max_pcr_gap_ticks: 1_080_000,
                min_pcr_intervals: 2,
                max_window_packets: 256,
            },
            max_buffer_packets: 8192,
            max_hold: Duration::from_secs(2),
            playout_delay: Duration::from_millis(100),
            max_lateness: Duration::from_millis(100),
        },
        authored_set(),
        Duration::from_secs(2),
    )
    .unwrap();
    TeletextOutputAdmission::with_binding(TeletextOutputRole::FinalWrapperMpegTs, Arc::new(binding))
        .unwrap()
}

fn verify_wrapper_clock(bytes: &[u8], config: ersatzrs_ffmpeg::teletext::TeletextClockConfig) {
    let mut validator =
        ersatzrs_ffmpeg::teletext::TeletextClockValidator::new_source_epoch(config).unwrap();
    for window in bytes.chunks(256 * 188) {
        let packets: Vec<[u8; 188]> = window
            .chunks_exact(188)
            .map(|packet| packet.try_into().unwrap())
            .collect();
        validator.observe(&packets).unwrap();
    }
    let report = validator.report().unwrap();
    assert!(report.ready);
    assert!(report.validated_intervals > 200);
}

async fn forward_wrapper(ffmpeg: &str, server: &ProgrammeServer) -> Vec<u8> {
    let mut command = build_mpegts_concat_command(
        format!("http://{}/ffmpeg/concat/1?mode=ts-legacy", server.address),
        &OutputMetadata::default(),
        Some(i64::try_from(MUXRATE).unwrap()),
    );
    command.executable = ffmpeg.to_owned();
    // Bound only the fixture's output duration: retain the production concat
    // input, realtime readrate and stream_loop so programme reopening is real.
    let stdout = command.arguments.pop().unwrap();
    assert_eq!(stdout, "pipe:1");
    command.arguments.extend(["-t".into(), "6".into(), stdout]);
    let mut child = spawn_stream_process(&command, None, "synthetic-wrapper-handover").unwrap();
    let output = wrapper_output();
    let cancel = Arc::new(Notify::new());
    let (reader, mut writer) = tokio::io::duplex(188);
    let (body, completion) = stream_body_for_output(reader, Arc::clone(&cancel), &output);
    let operation = async {
        let forward = async {
            let outcome = supervise_stream_process_with_retry_context(
                &mut child,
                &mut writer,
                &cancel,
                "synthetic-wrapper-handover",
                None,
                None,
                &output,
            )
            .await;
            completion
                .unwrap()
                .send(stream_output_result(&outcome))
                .unwrap();
            drop(writer);
            outcome
        };
        let receive = async {
            let mut bytes = Vec::new();
            let mut stream = body.into_data_stream();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(io::Error::other)?;
                if bytes.len() + chunk.len() > MAX_BYTES {
                    return Err(io::Error::other(
                        "synthetic wrapper exceeded its output bound",
                    ));
                }
                bytes.extend_from_slice(&chunk);
            }
            Ok::<_, io::Error>(bytes)
        };
        tokio::join!(forward, receive)
    };
    let result = tokio::time::timeout(Duration::from_secs(15), operation).await;
    if result.is_err() {
        reap_stream_process(&mut child)
            .await
            .expect("wrapper cleanup must be confirmed");
    }
    let (outcome, bytes) = result.expect("wrapper handover fixture exceeded its deadline");
    match outcome {
        DirectStreamOutcome::Completed => (),
        DirectStreamOutcome::TransportFailed(error) => {
            panic!("synthetic wrapper transport failed: {error:?}")
        }
        DirectStreamOutcome::Failed(failure) => {
            panic!(
                "synthetic wrapper producer failed: {}",
                failure.redacted_stderr
            )
        }
        DirectStreamOutcome::Cancelled => panic!("synthetic wrapper was cancelled"),
        DirectStreamOutcome::CleanupUnconfirmed => panic!("synthetic wrapper cleanup unconfirmed"),
    }
    assert!(child.try_wait().unwrap().unwrap().success());
    let bytes = bytes.expect("the single final authored wrapper must end cleanly");
    assert!(server.programmes.requests.load(Ordering::Acquire) >= 3);
    assert_eq!(&server.programmes.opened.lock().unwrap()[..3], &[0, 1, 0]);
    verify_wrapper_clock(&bytes, output.binding().unwrap().config().clock);
    bytes
}

fn frame_hashes(bytes: Vec<u8>) -> [Vec<String>; 3] {
    let text = String::from_utf8(bytes).unwrap();
    let mut hashes: [Vec<String>; 3] = std::array::from_fn(|_| Vec::new());
    for line in text
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
    {
        let fields: Vec<_> = line.split(',').map(str::trim).collect();
        // Copy-mode framemd5 can append side-data records; field five remains
        // the packet checksum, separate from those ancillary checksums.
        assert!(fields.len() >= 6);
        let stream = fields[0].parse::<usize>().unwrap();
        hashes[stream].push(fields[5].to_owned());
    }
    hashes
}

async fn verify_programme_content(ffmpeg: &str, sources: &[Arc<[u8]>; 2], output: &[u8]) {
    let hashes = frame_hashes(
        probe(ffmpeg, AV_DIGEST_ARGS, output, 512 * 1024)
            .await
            .unwrap(),
    );
    let mut reference: Vec<[Vec<String>; 3]> = Vec::new();
    for source in sources {
        reference.push(frame_hashes(
            probe(ffmpeg, AV_DIGEST_ARGS, source, 512 * 1024)
                .await
                .unwrap(),
        ));
    }
    for stream in 0..1 {
        let first: BTreeSet<_> = reference[0][stream].iter().collect();
        let second: BTreeSet<_> = reference[1][stream].iter().collect();
        assert!(
            first.is_disjoint(&second),
            "programmes must have distinct decoded video"
        );
        for (programme, expected) in [&first, &second].into_iter().enumerate() {
            let matches = hashes[stream]
                .iter()
                .filter(|hash| expected.contains(hash))
                .count();
            assert!(
                matches >= 20,
                "decoded video stream {stream}, programme {programme}: only {matches} matching frames"
            );
        }
    }
    // Independent AAC decoder state spans the programme boundary; decoded
    // frame checksums from separately opened sources are not a stable oracle.
    // Prove exact compressed audio carriage and independently decoded tones.
    let mut copy_args = AV_DIGEST_ARGS.to_vec();
    let format = copy_args.iter().rposition(|arg| *arg == "-f").unwrap();
    copy_args.splice(format..format, ["-c", "copy"]);
    let copied = frame_hashes(probe(ffmpeg, &copy_args, output, 512 * 1024).await.unwrap());
    let mut copy_reference = Vec::new();
    for source in sources {
        copy_reference.push(frame_hashes(
            probe(ffmpeg, &copy_args, source, 512 * 1024).await.unwrap(),
        ));
    }
    for (audio, tones) in [[440, 880], [660, 1320]].into_iter().enumerate() {
        let stream = audio + 1;
        let first: BTreeSet<_> = copy_reference[0][stream].iter().collect();
        let second: BTreeSet<_> = copy_reference[1][stream].iter().collect();
        assert!(
            first.is_disjoint(&second),
            "distinct compressed source audio"
        );
        for expected in [&first, &second] {
            assert!(
                copied[stream]
                    .iter()
                    .filter(|hash| expected.contains(hash))
                    .count()
                    >= 20
            );
        }
        verify_decoded_tones(ffmpeg, output, audio, &tones).await;
        for (source, tone) in sources.iter().zip(tones) {
            verify_decoded_tones(ffmpeg, source, audio, &[tone]).await;
        }
    }
    for (index, source) in sources.iter().enumerate() {
        let text = decode_text(ffmpeg, source, "888", "0:s:0").await;
        assert!(String::from_utf8(text).unwrap().contains(CAPTIONS[index]));
    }
    let captions = String::from_utf8(decode_text(ffmpeg, output, "888", "0:s:0").await).unwrap();
    for caption in CAPTIONS {
        assert!(captions.contains(caption));
    }
    let authored = String::from_utf8(decode_text(ffmpeg, output, "200", "0:s:1").await).unwrap();
    assert!(authored.contains("Synthetic television guide"));
}

async fn verify_decoded_tones(ffmpeg: &str, bytes: &[u8], audio: usize, tones: &[usize]) {
    let mapping = format!("0:a:{audio}");
    // This synthetic transport has already passed full AV/PSI inspection.
    // Bound audio-only input probing: default probing stalls the exact Windows
    // runtime on this finite pipe before PCM output, independently of Tokio.
    let pcm = probe(
        ffmpeg,
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-probesize",
            "32768",
            "-f",
            "mpegts",
            "-i",
            "pipe:0",
            "-map",
            &mapping,
            "-ac",
            "1",
            "-ar",
            "48000",
            "-f",
            "s16le",
            "pipe:1",
        ],
        bytes,
        1024 * 1024,
    )
    .await
    .unwrap();
    assert_eq!(pcm.len() % 2, 0);
    let samples: Vec<_> = pcm
        .chunks_exact(2)
        .map(|sample| i16::from_le_bytes(sample.try_into().unwrap()))
        .collect();
    for &tone in tones {
        let matching = samples
            .chunks_exact(4800)
            .filter(|chunk| {
                let crossings = chunk
                    .windows(2)
                    .filter(|pair| (pair[0] < 0) != (pair[1] < 0))
                    .count();
                let frequency = crossings * 48_000 / (2 * chunk.len());
                frequency.abs_diff(tone) <= 20
                    && chunk.iter().any(|sample| sample.unsigned_abs() > 100)
            })
            .count();
        assert!(matching >= 10,
            "audio {audio} must decode at least one second of programme tone {tone}; got {matching} segments");
    }
}

#[tokio::test]
#[ignore = "requires exact FFmpeg 9/libzvbi, TSDuck and loopback HTTP"]
async fn real_legacy_wrapper_keeps_one_authored_epoch_across_two_distinct_programmes() {
    let ffmpeg = exact_tool("ERSATZRS_TELETEXT_TEST_FFMPEG");
    let tsanalyze = exact_tool("ERSATZRS_TELETEXT_TEST_TSANALYZE");
    let version = probe(&ffmpeg, &["-version"], &[], 64 * 1024).await.unwrap();
    let version = String::from_utf8(version).unwrap();
    assert_eq!(
        ersatzrs_ffmpeg::version::parse_version(version.lines().next().unwrap())
            .unwrap()
            .major,
        9
    );
    let sources: [Arc<[u8]>; 2] = [
        Arc::from(programme(&ffmpeg, 0).await),
        Arc::from(programme(&ffmpeg, 1).await),
    ];
    let server = ProgrammeServer::start(sources.clone()).await;
    let output = forward_wrapper(&ffmpeg, &server).await;
    server.stop().await;
    verify_programme_content(&ffmpeg, &sources, &output).await;
    verify_tsduck(&tsanalyze, &output).await;
    let analysis = probe(&tsanalyze, &["--json"], &output, 256 * 1024)
        .await
        .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&analysis).unwrap();
    let pids = report["pids"].as_array().unwrap();
    for (expected, language) in [(257, "eng"), (259, "nld")] {
        let audio = pids.iter().find(|pid| pid["id"] == expected).unwrap();
        assert_eq!(audio["language"], language);
    }
    let captions = pids.iter().find(|pid| pid["id"] == 258).unwrap();
    assert_eq!(captions["language"], "eng");
}
