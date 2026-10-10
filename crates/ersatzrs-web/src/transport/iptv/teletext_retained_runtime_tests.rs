//! Exact-runtime proof of the borrowed retained pump; HTTP/process ownership
//! and native packaged acceptance remain separate integration gates.

use std::{
    io,
    pin::Pin,
    process::Stdio,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    task::{Context, Poll},
};

use ersatzrs_ffmpeg::teletext::{TeletextClockConfig, TeletextClockValidator};
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _, ReadBuf},
    process::Command,
};

use super::super::{
    retained_stream_buffer::TERMINAL_SUCCESS, teletext_output::tests::authored_set,
};
use super::*;

const MUXRATE: u64 = 5_000_000;
const MAX_FIXTURE_BYTES: usize = 8 * 1024 * 1024;
const TOOL_TIMEOUT: Duration = Duration::from_secs(15);
pub(in crate::transport::iptv) const AV_DIGEST_ARGS: &[&str] = &[
    "-hide_banner",
    "-loglevel",
    "error",
    "-nostdin",
    "-f",
    "mpegts",
    "-i",
    "pipe:0",
    "-map",
    "0:v:0",
    "-map",
    "0:a",
    "-f",
    "framemd5",
    "pipe:1",
];

#[derive(Debug, thiserror::Error)]
pub(in crate::transport::iptv) enum FixtureError {
    #[error("synthetic runtime fixture I/O failed ({0:?})")]
    Io(io::ErrorKind),
    #[error("synthetic runtime fixture exceeded its output bound")]
    OutputLimit,
    #[error("synthetic runtime fixture process failed")]
    Failed,
    #[error("synthetic runtime fixture process deadline expired")]
    Deadline,
    #[error("synthetic runtime fixture cleanup is unconfirmed")]
    CleanupUnconfirmed,
}

impl From<io::Error> for FixtureError {
    fn from(error: io::Error) -> Self {
        Self::Io(error.kind())
    }
}

#[expect(
    clippy::disallowed_methods,
    reason = "Explicit opt-in exact-runtime fixture selection, never production configuration"
)]
pub(in crate::transport::iptv) fn exact_tool(name: &str) -> String {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .expect("select the exact runtime executable using the teletext test environment")
}

async fn bounded_read(
    reader: impl AsyncRead + Unpin,
    limit: usize,
) -> Result<Vec<u8>, FixtureError> {
    let mut bytes = Vec::new();
    reader
        .take(u64::try_from(limit + 1).unwrap())
        .read_to_end(&mut bytes)
        .await?;
    if bytes.len() > limit {
        return Err(FixtureError::OutputLimit);
    }
    Ok(bytes)
}

// Scoped fixture runner: bounded pipes and owned/reaped process, no raw stderr
// or command paths in failures. It deliberately has no production API surface.
pub(in crate::transport::iptv) async fn probe(
    executable: &str,
    args: &[&str],
    input: &[u8],
    limit: usize,
) -> Result<Vec<u8>, FixtureError> {
    let mut child = Command::new(executable)
        .args(args)
        .kill_on_drop(true)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdin = child.stdin.take().expect("configured fixture stdin");
    let stdout = child.stdout.take().expect("configured fixture stdout");
    let stderr = child.stderr.take().expect("configured fixture stderr");
    let work = async {
        let write = async move {
            stdin.write_all(input).await?;
            stdin.shutdown().await?;
            Ok::<_, FixtureError>(())
        };
        let wait = async {
            let status = child.wait().await?;
            if !status.success() {
                return Err(FixtureError::Failed);
            }
            Ok::<_, FixtureError>(())
        };
        let ((), output, _, ()) = tokio::try_join!(
            write,
            bounded_read(stdout, limit),
            bounded_read(stderr, 64 * 1024),
            wait,
        )?;
        Ok(output)
    };
    let result = tokio::time::timeout(TOOL_TIMEOUT, work)
        .await
        .unwrap_or(Err(FixtureError::Deadline));
    if result.is_err() {
        let _ = child.start_kill();
        if !matches!(
            tokio::time::timeout(Duration::from_secs(5), child.wait()).await,
            Ok(Ok(_))
        ) {
            return Err(FixtureError::CleanupUnconfirmed);
        }
    }
    result
}

async fn generate_source(ffmpeg: &str, seconds: &str) -> Vec<u8> {
    let args = [
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostdin",
        "-f",
        "lavfi",
        "-i",
        "testsrc2=size=160x90:rate=25",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:sample_rate=48000",
        "-f",
        "mpegts",
        "-i",
        "pipe:0",
        "-t",
        seconds,
        "-map",
        "0:v:0",
        "-map",
        "1:a:0",
        "-map",
        "2:s:0",
        "-map",
        "1:a:0",
        "-program",
        "program_num=10:st=0:st=1:st=2:st=3",
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
    probe(
        ffmpeg,
        &args,
        ersatzrs_ffmpeg::teletext_probe::TELETEXT_DECODE_FIXTURE,
        MAX_FIXTURE_BYTES,
    )
    .await
    .expect("synthetic A/V, alternate-audio and source-subtitle fixture failed")
}

struct CountedSource<'a> {
    bytes: &'a [u8],
    consumed: Arc<AtomicUsize>,
    eof_polls: Arc<AtomicUsize>,
}

impl AsyncRead for CountedSource<'_> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        output: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.bytes.is_empty() {
            self.eof_polls.fetch_add(1, Ordering::AcqRel);
            return Poll::Ready(Ok(()));
        }
        let count = self.bytes.len().min(output.remaining()).min(READ_BYTES);
        output.put_slice(&self.bytes[..count]);
        self.bytes = &self.bytes[count..];
        self.consumed.fetch_add(count, Ordering::AcqRel);
        Poll::Ready(Ok(()))
    }
}

#[derive(Default)]
struct SinkProof {
    accepted: usize,
    flushes: usize,
    first_source_bytes: Option<usize>,
    first_write: Option<Instant>,
    last_write: Option<Instant>,
}

struct CountedSink {
    inner: tokio::io::DuplexStream,
    source: Arc<AtomicUsize>,
    proof: Arc<Mutex<SinkProof>>,
}

impl AsyncWrite for CountedSink {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        let result = Pin::new(&mut self.inner).poll_write(cx, bytes);
        if let Poll::Ready(Ok(count)) = &result {
            if *count > 0 {
                let mut proof = self.proof.lock().unwrap();
                proof.accepted += *count;
                proof
                    .first_source_bytes
                    .get_or_insert(self.source.load(Ordering::Acquire));
                proof.first_write.get_or_insert_with(Instant::now);
                proof.last_write = Some(Instant::now());
            }
        }
        result
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let result = Pin::new(&mut self.inner).poll_flush(cx);
        if matches!(result, Poll::Ready(Ok(()))) {
            self.proof.lock().unwrap().flushes += 1;
        }
        result
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

fn runtime_binding(policy: ReservePolicy) -> TeletextOutputBinding {
    TeletextOutputBinding::new(
        TeletextOutputConfig {
            program: 10,
            elementary_pid: 512,
            clock: TeletextClockConfig {
                pcr_pid: 256,
                muxrate_bps: MUXRATE,
                rate_tolerance_ppm: 1000,
                pcr_jitter_ticks: 14,
                max_pcr_gap_packets: 134,
                max_pcr_gap_ticks: 1_080_000,
                min_pcr_intervals: 2,
                max_window_packets: 256,
            },
            max_buffer_packets: policy.capacity_bytes().div_ceil(PACKET_BYTES),
            max_hold: Duration::from_secs(9),
            playout_delay: Duration::from_millis(100),
            max_lateness: Duration::from_millis(100),
        },
        authored_set(),
        Duration::from_secs(1),
    )
    .unwrap()
}

async fn forward_fixture(source_bytes: &[u8], seconds: u64) -> Vec<u8> {
    let policy = ReservePolicy::new(MUXRATE, Some(Duration::from_secs(seconds))).unwrap();
    let binding = runtime_binding(policy);
    let consumed = Arc::new(AtomicUsize::new(0));
    let eof_polls = Arc::new(AtomicUsize::new(0));
    let proof = Arc::new(Mutex::new(SinkProof::default()));
    let mut source = CountedSource {
        bytes: source_bytes,
        consumed: Arc::clone(&consumed),
        eof_polls: Arc::clone(&eof_polls),
    };
    let (inner, mut receiver) = tokio::io::duplex(PACKET_BYTES);
    let mut sink = CountedSink {
        inner,
        source: Arc::clone(&consumed),
        proof: Arc::clone(&proof),
    };
    let terminal = AtomicU8::new(TERMINAL_SUCCESS);
    let cancel = Notify::new();
    let started = Instant::now();
    let forward = async {
        let result = forward_retained_ts_with_teletext(
            &mut source,
            &mut sink,
            policy,
            &terminal,
            started + Duration::from_secs(2),
            &cancel,
            &binding,
        )
        .await;
        drop(sink);
        result
    };
    let receive = bounded_read(&mut receiver, MAX_FIXTURE_BYTES);
    let (result, output) = tokio::time::timeout(Duration::from_secs(14), async {
        tokio::join!(forward, receive)
    })
    .await
    .expect("retained async runtime fixture exceeded its own bounded drain");
    result.expect("actual retained output failed transport, pacing or EOF admission");
    let output = output.expect("synthetic response sink failed");
    assert_eq!(output.len(), source_bytes.len());
    assert_eq!(consumed.load(Ordering::Acquire), source_bytes.len());
    assert_eq!(eof_polls.load(Ordering::Acquire), 1);
    let proof = proof.lock().unwrap();
    assert_eq!(proof.accepted, source_bytes.len());
    assert!(proof.flushes > 200);
    let first_source = proof.first_source_bytes.unwrap();
    if seconds <= 8 {
        assert_eq!(first_source, source_bytes.len());
    } else {
        assert!(first_source >= policy.reserve_bytes());
        assert!(
            first_source < source_bytes.len(),
            "reserve was not replenished during delivery"
        );
    }
    let physical =
        Duration::from_nanos(u64::try_from(source_bytes.len()).unwrap() * 8_000_000_000 / MUXRATE);
    assert!(proof.first_write.unwrap().duration_since(started) >= binding.config().playout_delay);
    assert!(
        proof
            .last_write
            .unwrap()
            .duration_since(proof.first_write.unwrap())
            >= physical.saturating_sub(Duration::from_millis(150))
    );
    output
}

fn preserve_source_packets(source: &[u8], output: &[u8]) {
    assert_eq!(source.len() % PACKET_BYTES, 0);
    assert_eq!(output.len(), source.len());
    for expected in [256, 257, 258, 259] {
        assert!(source
            .chunks_exact(PACKET_BYTES)
            .any(|packet| pid(packet) == expected));
    }
    for (before, after) in source
        .chunks_exact(PACKET_BYTES)
        .zip(output.chunks_exact(PACKET_BYTES))
    {
        if ![4096, 8191].contains(&pid(before)) {
            assert_eq!(before, after, "preserved source PID changed");
        }
    }
    assert!(output
        .chunks_exact(PACKET_BYTES)
        .any(|packet| pid(packet) == 512));
}

fn pid(packet: &[u8]) -> u16 {
    (u16::from(packet[1] & 0x1f) << 8) | u16::from(packet[2])
}

pub(in crate::transport::iptv) fn verify_clock(bytes: &[u8], config: TeletextClockConfig) {
    let mut validator = TeletextClockValidator::new(config).unwrap();
    for window in bytes.chunks(256 * PACKET_BYTES) {
        let packets: Vec<[u8; PACKET_BYTES]> = window
            .chunks_exact(PACKET_BYTES)
            .map(|packet| packet.try_into().unwrap())
            .collect();
        validator.observe(&packets).unwrap();
    }
    let report = validator.report().unwrap();
    assert!(report.ready);
    assert!(report.validated_intervals > 200);
}

pub(in crate::transport::iptv) async fn decode_text(
    ffmpeg: &str,
    bytes: &[u8],
    page: &str,
    stream: &str,
) -> Vec<u8> {
    probe(
        ffmpeg,
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-txt_page",
            page,
            "-txt_format",
            "text",
            "-txt_chop_top",
            "0",
            "-txt_duration",
            "1000",
            "-f",
            "mpegts",
            "-i",
            "pipe:0",
            "-map",
            stream,
            "-c:s",
            "srt",
            "-f",
            "srt",
            "pipe:1",
        ],
        bytes,
        256 * 1024,
    )
    .await
    .expect("synthetic subtitle/page decoding failed")
}

async fn verify_decoders(ffmpeg: &str, source: &[u8], output: &[u8]) {
    let (before, after) = tokio::join!(
        probe(ffmpeg, AV_DIGEST_ARGS, source, 512 * 1024),
        probe(ffmpeg, AV_DIGEST_ARGS, output, 512 * 1024),
    );
    assert_eq!(
        before.unwrap(),
        after.unwrap(),
        "decoded A/V frames or timestamps changed"
    );
    let (before, after) = tokio::join!(
        decode_text(ffmpeg, source, "888", "0:s:0"),
        decode_text(ffmpeg, output, "888", "0:s:0"),
    );
    assert_eq!(before, after, "source subtitle text/timing changed");
    assert!(String::from_utf8(before)
        .unwrap()
        .contains("Synthetic subtitle page 888"));
    let authored = decode_text(ffmpeg, output, "200", "0:s:1").await;
    assert!(String::from_utf8(authored)
        .unwrap()
        .contains("Synthetic television guide"));
}

pub(in crate::transport::iptv) async fn verify_tsduck(tsanalyze: &str, output: &[u8]) {
    let analysis = probe(tsanalyze, &["--json"], output, 256 * 1024)
        .await
        .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&analysis).unwrap();
    assert!(
        report["ts"]["pcr-bitrate"]
            .as_u64()
            .unwrap()
            .abs_diff(MUXRATE)
            < 2000
    );
    for field in ["invalid-syncs", "transport-errors"] {
        assert_eq!(report["ts"]["packets"][field], 0);
    }
    assert_eq!(report["ts"]["pids"]["unreferenced"], 0);
    let pids = report["pids"].as_array().unwrap();
    for expected in [256, 257, 258, 259, 4096, 512] {
        assert!(pids.iter().any(|pid| pid["id"] == expected));
    }
    let authored = pids.iter().find(|pid| pid["id"] == 512).unwrap();
    assert_eq!(authored["pes-stream-id"], 0xbd);
    assert_eq!(authored["invalid-pes-prefix"], 0);
    assert_eq!(authored["language"], "nld");
    assert!(authored["description"]
        .as_str()
        .unwrap()
        .contains("Initial Teletext page"));
    for pid in pids {
        for field in ["discontinuities", "pts-leap", "pcr-leap"] {
            assert_eq!(pid["packets"][field], 0);
        }
    }
}

async fn verify_runtime(seconds: u64) {
    let ffmpeg = exact_tool("ERSATZRS_TELETEXT_TEST_FFMPEG");
    let tsanalyze = exact_tool("ERSATZRS_TELETEXT_TEST_TSANALYZE");
    let source = generate_source(&ffmpeg, &seconds.to_string()).await;
    let output = forward_fixture(&source, seconds).await;
    preserve_source_packets(&source, &output);
    let policy = ReservePolicy::new(MUXRATE, Some(Duration::from_secs(seconds))).unwrap();
    let clock = runtime_binding(policy).config().clock;
    verify_clock(&source, clock);
    verify_clock(&output, clock);
    verify_decoders(&ffmpeg, &source, &output).await;
    verify_tsduck(&tsanalyze, &output).await;
}

#[tokio::test]
#[ignore = "requires exact FFmpeg 9/libzvbi and TSDuck tsanalyze executables"]
async fn real_runtime_retained_short_eof_preserves_av_subtitles_and_decodes_authored_pages() {
    verify_runtime(6).await;
}

#[tokio::test]
#[ignore = "requires exact FFmpeg 9/libzvbi and TSDuck tsanalyze executables"]
async fn real_runtime_retained_live_reserve_refills_and_drains_finite_nine_seconds() {
    verify_runtime(9).await;
}
