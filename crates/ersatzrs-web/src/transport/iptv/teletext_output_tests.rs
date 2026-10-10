use std::{
    io,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
    time::Duration,
};

use ersatzrs_domain::{
    TeletextLanguage, TeletextNationalOption, TeletextPageAddress, TeletextPageDraft,
    TeletextPageKind, TeletextPageNumber, TeletextPageSetDraft, TeletextRow, TeletextRun,
    TeletextRunContent, TeletextStyle, TeletextSubcode,
};
use ersatzrs_ffmpeg::teletext::{
    encode_teletext_page_set, EncodedTeletextPageSet, TeletextClockConfig, TeletextEncodingRegion,
    TeletextOutputConfig,
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    time::Instant,
};

use super::{forward_teletext_output, TeletextPumpError};

pub(crate) fn policy() -> TeletextOutputConfig {
    TeletextOutputConfig {
        program: 10,
        elementary_pid: 512,
        clock: TeletextClockConfig {
            pcr_pid: 300,
            muxrate_bps: 5_000_000,
            rate_tolerance_ppm: 0,
            pcr_jitter_ticks: 0,
            max_pcr_gap_packets: 100,
            max_pcr_gap_ticks: 1_080_000,
            min_pcr_intervals: 2,
            max_window_packets: 1024,
        },
        max_buffer_packets: 1024,
        max_hold: Duration::from_millis(600),
        playout_delay: Duration::from_millis(100),
        max_lateness: Duration::from_millis(100),
    }
}

pub(crate) fn authored_set() -> EncodedTeletextPageSet {
    let page = TeletextPageNumber::new(200).unwrap();
    let language = TeletextLanguage::new("nld".into()).unwrap();
    encode_teletext_page_set(
        &TeletextPageSetDraft {
            initial_page: page,
            language: language.clone(),
            pages: vec![TeletextPageDraft {
                address: TeletextPageAddress {
                    page,
                    subcode: TeletextSubcode::new(0).unwrap(),
                },
                kind: TeletextPageKind::FullPage,
                language,
                national_option: TeletextNationalOption::default(),
                rows: vec![TeletextRow {
                    row: 1,
                    runs: vec![TeletextRun {
                        column: 2,
                        style: TeletextStyle::default(),
                        content: TeletextRunContent::Text {
                            text: "Synthetic television guide".into(),
                        },
                    }],
                }],
                navigation: Vec::new(),
            }],
        },
        TeletextEncodingRegion::WesternLatin,
    )
    .unwrap()
}

fn append_crc(section: &mut Vec<u8>) {
    let length = u16::try_from(section.len() + 1).unwrap().to_be_bytes();
    section[1] = 0xb0 | length[0];
    section[2] = length[1];
    let mut crc = u32::MAX;
    for byte in section.iter().copied() {
        crc ^= u32::from(byte) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 == 0 {
                crc << 1
            } else {
                (crc << 1) ^ 0x04c1_1db7
            };
        }
    }
    section.extend_from_slice(&crc.to_be_bytes());
}

fn section_packet(pid: u16, mut section: Vec<u8>) -> [u8; 188] {
    append_crc(&mut section);
    let mut packet = [0xff; 188];
    let pid = pid.to_be_bytes();
    packet[..5].copy_from_slice(&[0x47, 0x40 | pid[0], pid[1], 0x10, 0]);
    packet[5..5 + section.len()].copy_from_slice(&section);
    packet
}

fn pcr_packet(ticks: u64) -> [u8; 188] {
    let base = ticks / 300;
    let extension = ticks % 300;
    let mut packet = [0xff; 188];
    packet[..6].copy_from_slice(&[0x47, 1, 44, 0x20, 183, 0x10]);
    packet[6] = u8::try_from(base >> 25).unwrap();
    packet[7] = u8::try_from((base >> 17) & 255).unwrap();
    packet[8] = u8::try_from((base >> 9) & 255).unwrap();
    packet[9] = u8::try_from((base >> 1) & 255).unwrap();
    packet[10] = u8::try_from((base & 1) << 7).unwrap() | 0x7e;
    packet[10] |= u8::try_from(extension >> 8).unwrap();
    packet[11] = u8::try_from(extension & 255).unwrap();
    packet
}

pub(crate) fn fixture(frames: u64) -> Vec<[u8; 188]> {
    let count = usize::try_from(3 + frames * 5_000_000 / (50 * 188 * 8)).unwrap();
    let mut null = [0xff; 188];
    null[..4].copy_from_slice(&[0x47, 0x1f, 0xff, 0x10]);
    let mut packets = vec![null; count];
    packets[0] = section_packet(0, vec![0, 0xb0, 0, 0, 1, 0xc1, 0, 0, 0, 10, 0xed, 9]);
    packets[1] = section_packet(
        3337,
        vec![
            2, 0xb0, 0, 0, 10, 0xc1, 0, 0, 0xe1, 44, 0xf0, 0, 0x1b, 0xe1, 44, 0xf0, 0,
        ],
    );
    for (sample, position) in (2..packets.len()).step_by(5).enumerate() {
        packets[position] = pcr_packet(900_000 + u64::try_from(sample).unwrap() * 40_608);
    }
    packets
}

pub(crate) fn flatten(packets: &[[u8; 188]]) -> Vec<u8> {
    packets.iter().flatten().copied().collect()
}

enum Tail {
    Eof,
    Stall,
    Error(io::ErrorKind),
}

struct Source {
    bytes: Vec<u8>,
    position: usize,
    chunk: usize,
    tail: Tail,
}

impl Source {
    fn new(bytes: Vec<u8>, chunk: usize, tail: Tail) -> Self {
        Self {
            bytes,
            position: 0,
            chunk,
            tail,
        }
    }
}

impl AsyncRead for Source {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        output: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.position == self.bytes.len() {
            return match self.tail {
                Tail::Eof => Poll::Ready(Ok(())),
                Tail::Stall => Poll::Pending,
                Tail::Error(kind) => Poll::Ready(Err(io::Error::from(kind))),
            };
        }
        let count = self
            .chunk
            .min(output.remaining())
            .min(self.bytes.len() - self.position);
        output.put_slice(&self.bytes[self.position..self.position + count]);
        self.position += count;
        Poll::Ready(Ok(()))
    }
}

#[derive(Default)]
struct Writes {
    bytes: Vec<u8>,
    times: Vec<Instant>,
}

enum SinkMode {
    Ready,
    Short,
    Stall,
    FlushStall,
    Error(io::ErrorKind),
}

struct Sink {
    writes: Arc<Mutex<Writes>>,
    mode: SinkMode,
}

#[derive(Clone, Copy)]
enum FinalTailMode {
    Ready,
    WriteFault,
    FlushFault,
    WriteStall,
    FlushStall,
}

struct FinalTailSink {
    inner: Sink,
    prefix_bytes: usize,
    mode: FinalTailMode,
    entered: Arc<tokio::sync::Notify>,
}

impl FinalTailSink {
    fn new(prefix_bytes: usize, mode: FinalTailMode) -> Self {
        Self {
            inner: Sink::new(SinkMode::Ready),
            prefix_bytes,
            mode,
            entered: Arc::default(),
        }
    }
}

impl AsyncWrite for FinalTailSink {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        let final_tail = self.inner.writes.lock().unwrap().bytes.len() >= self.prefix_bytes;
        if final_tail {
            match self.mode {
                FinalTailMode::WriteFault => {
                    self.entered.notify_one();
                    return Poll::Ready(Err(io::ErrorKind::BrokenPipe.into()));
                }
                FinalTailMode::WriteStall => {
                    self.entered.notify_one();
                    return Poll::Pending;
                }
                FinalTailMode::Ready | FinalTailMode::FlushFault | FinalTailMode::FlushStall => (),
            }
        }
        Pin::new(&mut self.inner).poll_write(cx, bytes)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let final_tail = self.inner.writes.lock().unwrap().bytes.len() > self.prefix_bytes;
        if final_tail {
            match self.mode {
                FinalTailMode::FlushFault => {
                    self.entered.notify_one();
                    return Poll::Ready(Err(io::ErrorKind::ConnectionReset.into()));
                }
                FinalTailMode::FlushStall => {
                    self.entered.notify_one();
                    return Poll::Pending;
                }
                FinalTailMode::Ready | FinalTailMode::WriteFault | FinalTailMode::WriteStall => (),
            }
        }
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

impl Sink {
    fn new(mode: SinkMode) -> Self {
        Self {
            writes: Arc::default(),
            mode,
        }
    }
}

impl AsyncWrite for Sink {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.mode {
            SinkMode::Ready | SinkMode::Short | SinkMode::FlushStall => {
                let count = if matches!(self.mode, SinkMode::Short) {
                    bytes.len().min(7)
                } else {
                    bytes.len()
                };
                let mut writes = self.writes.lock().unwrap();
                writes.bytes.extend_from_slice(&bytes[..count]);
                writes.times.push(Instant::now());
                Poll::Ready(Ok(count))
            }
            SinkMode::Stall => Poll::Pending,
            SinkMode::Error(kind) => Poll::Ready(Err(io::Error::from(kind))),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if matches!(self.mode, SinkMode::FlushStall) {
            Poll::Pending
        } else {
            Poll::Ready(Ok(()))
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

async fn pump(source: &mut Source, sink: &mut Sink) -> Result<u64, TeletextPumpError> {
    forward_teletext_output(
        source,
        sink,
        policy(),
        authored_set(),
        Duration::from_millis(40),
    )
    .await
}

#[tokio::test]
async fn delivered_bytes_preserve_original_positions_and_follow_the_physical_clock() {
    let original = fixture(6);
    let mut source = Source::new(flatten(&original), usize::MAX, Tail::Eof);
    let mut sink = Sink::new(SinkMode::Ready);
    let started = Instant::now();
    let count = pump(&mut source, &mut sink).await.unwrap();
    let writes = sink.writes.lock().unwrap();
    assert_eq!(count, u64::try_from(original.len() * 188).unwrap());
    assert_eq!(writes.bytes.len(), original.len() * 188);
    for (before, after) in original.iter().zip(writes.bytes.chunks_exact(188)) {
        if before[1] & 0x1f == 1 && before[2] == 44 {
            assert_eq!(before.as_slice(), after);
        }
    }
    assert_eq!(&writes.bytes[..188], original[0]);
    assert!(writes
        .bytes
        .chunks_exact(188)
        .any(|p| p[1] & 0x1f == 2 && p[2] == 0));
    let first = writes.times.first().unwrap();
    let last = writes.times.last().unwrap();
    assert!(first.duration_since(started) >= Duration::from_millis(90));
    assert!(last.duration_since(*first) >= Duration::from_millis(90));
    assert!(last.duration_since(started) < Duration::from_millis(400));
}

#[tokio::test]
async fn split_headers_and_partial_transport_packets_are_reassembled_before_delivery() {
    let original = fixture(3);
    let mut source = Source::new(flatten(&original), 7, Tail::Eof);
    let mut sink = Sink::new(SinkMode::Ready);
    assert_eq!(
        pump(&mut source, &mut sink).await.unwrap(),
        188 * u64::try_from(original.len()).unwrap()
    );
    let writes = sink.writes.lock().unwrap();
    assert_eq!(writes.bytes.len(), original.len() * 188);
    assert_eq!(&writes.bytes[..188], original[0]);
}

#[tokio::test]
async fn short_sink_writes_are_completed_without_dropping_transport_bytes() {
    let original = fixture(3);
    let mut source = Source::new(flatten(&original), usize::MAX, Tail::Eof);
    let mut sink = Sink::new(SinkMode::Short);
    assert_eq!(
        pump(&mut source, &mut sink).await.unwrap(),
        188 * u64::try_from(original.len()).unwrap()
    );
    let writes = sink.writes.lock().unwrap();
    assert_eq!(writes.bytes.len(), original.len() * 188);
    assert_eq!(&writes.bytes[..188], original[0]);
    assert!(writes.times.len() > original.len());
}

#[tokio::test]
async fn first_byte_timeout_ends_an_idle_source_without_any_output() {
    let mut source = Source::new(Vec::new(), 1, Tail::Stall);
    let mut sink = Sink::new(SinkMode::Ready);
    assert!(matches!(
        pump(&mut source, &mut sink).await,
        Err(TeletextPumpError::FirstByteTimeout)
    ));
    assert!(sink.writes.lock().unwrap().bytes.is_empty());
}

#[tokio::test]
async fn invalid_first_byte_timing_policy_fails_before_polling_borrowed_io() {
    for timeout in [Duration::ZERO, Duration::MAX] {
        let mut source = Source::new(flatten(&fixture(3)), usize::MAX, Tail::Eof);
        let mut sink = Sink::new(SinkMode::Ready);
        assert!(matches!(
            forward_teletext_output(&mut source, &mut sink, policy(), authored_set(), timeout)
                .await,
            Err(TeletextPumpError::Configuration)
        ));
        assert_eq!(source.position, 0);
        assert!(sink.writes.lock().unwrap().bytes.is_empty());
    }
}

#[tokio::test]
async fn source_stall_with_retained_partial_input_expires_without_output() {
    let mut source = Source::new(vec![0x47], 1, Tail::Stall);
    let mut sink = Sink::new(SinkMode::Ready);
    let started = Instant::now();
    assert!(matches!(
        pump(&mut source, &mut sink).await,
        Err(TeletextPumpError::Gate(_))
    ));
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(sink.writes.lock().unwrap().bytes.is_empty());
}

#[tokio::test]
async fn blocked_sink_write_expires_instead_of_waiting_forever() {
    let mut source = Source::new(flatten(&fixture(3)), usize::MAX, Tail::Eof);
    let mut sink = Sink::new(SinkMode::Stall);
    let started = Instant::now();
    assert!(matches!(
        pump(&mut source, &mut sink).await,
        Err(TeletextPumpError::Gate(_))
    ));
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(sink.writes.lock().unwrap().bytes.is_empty());
}

#[tokio::test]
async fn stalled_flush_fences_the_first_window_before_the_next_can_be_delivered() {
    let mut source = Source::new(flatten(&fixture(3)), usize::MAX, Tail::Eof);
    let mut sink = Sink::new(SinkMode::FlushStall);
    assert!(matches!(
        pump(&mut source, &mut sink).await,
        Err(TeletextPumpError::Gate(_))
    ));
    assert_eq!(
        sink.writes.lock().unwrap().bytes.len(),
        fixture(1).len() * 188
    );
}

#[tokio::test]
async fn source_and_sink_errors_preserve_io_failure_classification() {
    let mut source = Source::new(Vec::new(), 1, Tail::Error(io::ErrorKind::ConnectionReset));
    let mut sink = Sink::new(SinkMode::Ready);
    assert!(matches!(
        pump(&mut source, &mut sink).await,
        Err(TeletextPumpError::Read(io::ErrorKind::ConnectionReset))
    ));
    assert!(sink.writes.lock().unwrap().bytes.is_empty());

    let mut source = Source::new(
        flatten(&fixture(3)[..13]),
        usize::MAX,
        Tail::Error(io::ErrorKind::UnexpectedEof),
    );
    assert!(matches!(
        pump(&mut source, &mut sink).await,
        Err(TeletextPumpError::Read(io::ErrorKind::UnexpectedEof))
    ));
    assert!(sink.writes.lock().unwrap().bytes.is_empty());

    let mut source = Source::new(flatten(&fixture(3)), usize::MAX, Tail::Eof);
    let mut sink = Sink::new(SinkMode::Error(io::ErrorKind::BrokenPipe));
    assert!(matches!(
        pump(&mut source, &mut sink).await,
        Err(TeletextPumpError::Write(io::ErrorKind::BrokenPipe))
    ));
}

#[tokio::test]
async fn partial_packet_and_unclosed_cycle_eof_are_reported_as_failures() {
    let mut source = Source::new(vec![0x47], 1, Tail::Eof);
    let mut sink = Sink::new(SinkMode::Ready);
    assert!(matches!(
        pump(&mut source, &mut sink).await,
        Err(TeletextPumpError::Gate(_))
    ));
    assert!(sink.writes.lock().unwrap().bytes.is_empty());

    let mut source = Source::new(flatten(&fixture(1)), usize::MAX, Tail::Eof);
    let mut sink = Sink::new(SinkMode::Ready);
    assert!(matches!(
        pump(&mut source, &mut sink).await,
        Err(TeletextPumpError::Gate(_))
    ));
}

#[tokio::test]
async fn dropping_the_future_cancels_retention_and_leaves_borrowed_io_owned_by_the_caller() {
    use tokio::io::AsyncWriteExt;

    let mut source = Source::new(flatten(&fixture(3)), usize::MAX, Tail::Stall);
    let mut sink = Sink::new(SinkMode::Ready);
    assert!(
        tokio::time::timeout(Duration::from_millis(30), pump(&mut source, &mut sink))
            .await
            .is_err()
    );
    assert!(sink.writes.lock().unwrap().bytes.is_empty());
    assert!(source.position > 0);
    sink.write_all(b"caller still owns this sink")
        .await
        .unwrap();
    assert_eq!(
        sink.writes.lock().unwrap().bytes,
        b"caller still owns this sink"
    );
}

#[tokio::test]
async fn invalid_pmt_or_clock_is_rejected_before_any_prefix_is_exposed() {
    for corrupt_pmt in [true, false] {
        let mut raw = fixture(3);
        if corrupt_pmt {
            raw[1][18] ^= 1;
        } else {
            raw[12] = pcr_packet(900_000 + 2 * 40_608 + 1);
        }
        let mut source = Source::new(flatten(&raw), usize::MAX, Tail::Eof);
        let mut sink = Sink::new(SinkMode::Ready);
        assert!(matches!(
            pump(&mut source, &mut sink).await,
            Err(TeletextPumpError::Gate(_))
        ));
        assert!(sink.writes.lock().unwrap().bytes.is_empty());
    }
}

#[tokio::test]
async fn real_duplex_backpressure_recovers_within_budget_and_preserves_the_complete_cycle() {
    use tokio::io::{duplex, AsyncReadExt, AsyncWriteExt};

    let original = fixture(3);
    let input = flatten(&original);
    let (mut producer, mut source) = duplex(32 * 188);
    let (mut sink, mut receiver) = duplex(188);
    let receive = async {
        tokio::time::sleep(Duration::from_millis(140)).await;
        let mut output = Vec::new();
        receiver.read_to_end(&mut output).await.unwrap();
        output
    };
    let produce = async {
        producer.write_all(&input).await.unwrap();
        producer.shutdown().await.unwrap();
    };
    let forward = async {
        let result = forward_teletext_output(
            &mut source,
            &mut sink,
            policy(),
            authored_set(),
            Duration::from_millis(40),
        )
        .await;
        drop(sink);
        result
    };
    let ((), result, output) = tokio::time::timeout(Duration::from_secs(2), async {
        tokio::join!(produce, forward, receive)
    })
    .await
    .unwrap();
    assert_eq!(result.unwrap(), u64::try_from(input.len()).unwrap());
    assert_eq!(output.len(), input.len());
    assert_eq!(&output[..188], original[0]);
    for (before, after) in original.iter().zip(output.chunks_exact(188)) {
        if before[1] & 0x1f == 1 && before[2] == 44 {
            assert_eq!(before.as_slice(), after);
        }
    }
}

#[tokio::test]
async fn cancelling_a_real_blocked_sink_does_not_resume_old_output_when_capacity_returns() {
    use tokio::io::{duplex, AsyncReadExt, AsyncWriteExt};

    let original = fixture(3);
    let mut source = Source::new(flatten(&original), usize::MAX, Tail::Eof);
    let (mut sink, mut receiver) = duplex(188);
    let result = tokio::time::timeout(
        Duration::from_millis(150),
        forward_teletext_output(
            &mut source,
            &mut sink,
            policy(),
            authored_set(),
            Duration::from_millis(40),
        ),
    )
    .await;
    assert!(result.is_err());
    let mut accepted = [0_u8; 188];
    receiver.read_exact(&mut accepted).await.unwrap();
    assert_eq!(accepted, original[0]);
    assert!(
        tokio::time::timeout(Duration::from_millis(20), receiver.read(&mut accepted))
            .await
            .is_err()
    );
    sink.write_all(b"caller-owned write").await.unwrap();
    let mut own = [0_u8; 18];
    receiver.read_exact(&mut own).await.unwrap();
    assert_eq!(&own, b"caller-owned write");
}

#[tokio::test]
async fn finite_fractional_windows_preserve_every_source_packet_and_its_pacing() {
    let prefix_packets = fixture(3).len();
    for tail_packets in [1, 17, 65] {
        let original = fixture(4)[..prefix_packets + tail_packets].to_vec();
        let mut source = Source::new(flatten(&original), 7, Tail::Eof);
        let mut sink = Sink::new(SinkMode::Ready);
        let started = Instant::now();
        let delivered = pump(&mut source, &mut sink).await.unwrap();
        let writes = sink.writes.lock().unwrap();
        assert_eq!(delivered, u64::try_from(original.len() * 188).unwrap());
        assert_eq!(writes.bytes.len(), original.len() * 188);
        assert_eq!(writes.times.len(), original.len());
        assert_eq!(
            &writes.bytes[prefix_packets * 188..],
            flatten(&original[prefix_packets..])
        );
        for (before, after) in original.iter().zip(writes.bytes.chunks_exact(188)) {
            if before[1] & 0x1f == 1 && before[2] == 44 {
                assert_eq!(before.as_slice(), after);
            }
        }
        for (position, written) in writes.times.iter().enumerate() {
            let offset = u64::try_from(position).unwrap() * 188 * 8 * 1_000_000_000 / 5_000_000;
            assert!(
                written.duration_since(started)
                    >= policy().playout_delay + Duration::from_nanos(offset)
            );
        }
        assert!(started.elapsed() < Duration::from_millis(500));
    }
}

#[tokio::test]
async fn finite_eof_inside_a_later_carousel_cycle_preserves_all_complete_transport_packets() {
    let original = fixture(4);
    let mut source = Source::new(flatten(&original), usize::MAX, Tail::Eof);
    let mut sink = Sink::new(SinkMode::Ready);
    assert_eq!(
        pump(&mut source, &mut sink).await.unwrap(),
        u64::try_from(original.len() * 188).unwrap()
    );
    let writes = sink.writes.lock().unwrap();
    assert_eq!(writes.bytes.len(), original.len() * 188);
    assert_eq!(&writes.bytes[..188], original[0]);
    for (before, after) in original.iter().zip(writes.bytes.chunks_exact(188)) {
        if before[1] & 0x1f == 1 && before[2] == 44 {
            assert_eq!(before.as_slice(), after);
        }
    }
    assert!(writes.bytes[fixture(3).len() * 188..]
        .chunks_exact(188)
        .any(|packet| packet[1] & 0x1f == 2 && packet[2] == 0));
}

#[tokio::test]
async fn finite_tail_sink_faults_remain_terminal_after_a_complete_cycle_was_delivered() {
    let prefix_bytes = fixture(3).len() * 188;
    let original = fixture(4)[..fixture(3).len() + 17].to_vec();
    for (mode, kind, accepted) in [
        (
            FinalTailMode::WriteFault,
            io::ErrorKind::BrokenPipe,
            prefix_bytes,
        ),
        (
            FinalTailMode::FlushFault,
            io::ErrorKind::ConnectionReset,
            original.len() * 188,
        ),
    ] {
        let mut source = Source::new(flatten(&original), usize::MAX, Tail::Eof);
        let mut sink = FinalTailSink::new(prefix_bytes, mode);
        let result = forward_teletext_output(
            &mut source,
            &mut sink,
            policy(),
            authored_set(),
            Duration::from_millis(40),
        )
        .await;
        assert!(matches!(result, Err(TeletextPumpError::Write(actual)) if actual == kind));
        assert_eq!(sink.inner.writes.lock().unwrap().bytes.len(), accepted);
        assert_eq!(source.position, source.bytes.len());
    }
}

#[tokio::test]
async fn finite_tail_write_and_flush_stalls_expire_after_the_complete_cycle() {
    let prefix_bytes = fixture(3).len() * 188;
    let original = fixture(4)[..fixture(3).len() + 17].to_vec();
    for (mode, accepted) in [
        (FinalTailMode::WriteStall, prefix_bytes),
        (FinalTailMode::FlushStall, original.len() * 188),
    ] {
        let mut source = Source::new(flatten(&original), usize::MAX, Tail::Eof);
        let mut sink = FinalTailSink::new(prefix_bytes, mode);
        let result = tokio::time::timeout(
            Duration::from_secs(2),
            forward_teletext_output(
                &mut source,
                &mut sink,
                policy(),
                authored_set(),
                Duration::from_millis(40),
            ),
        )
        .await
        .expect("the final tail's own deadline must end the stalled operation");
        assert!(matches!(result, Err(TeletextPumpError::Gate(_))));
        assert_eq!(sink.inner.writes.lock().unwrap().bytes.len(), accepted);
    }
}

#[tokio::test]
async fn dropping_pending_final_tail_io_never_resumes_output_or_releases_borrowed_ownership() {
    use tokio::io::AsyncWriteExt as _;

    let prefix_bytes = fixture(3).len() * 188;
    let original = fixture(4)[..fixture(3).len() + 17].to_vec();
    for (mode, accepted) in [
        (FinalTailMode::WriteStall, prefix_bytes),
        (FinalTailMode::FlushStall, original.len() * 188),
    ] {
        let mut source = Source::new(flatten(&original), usize::MAX, Tail::Eof);
        let mut sink = FinalTailSink::new(prefix_bytes, mode);
        let entered = Arc::clone(&sink.entered);
        let writes = Arc::clone(&sink.inner.writes);
        {
            let operation = forward_teletext_output(
                &mut source,
                &mut sink,
                policy(),
                authored_set(),
                Duration::from_millis(40),
            );
            tokio::pin!(operation);
            tokio::select! {
                () = entered.notified() => (),
                result = &mut operation => panic!("final tail must remain pending: {result:?}"),
            }
            assert_eq!(writes.lock().unwrap().bytes.len(), accepted);
            assert!(futures_util::poll!(&mut operation).is_pending());
        }
        assert_eq!(source.position, source.bytes.len());
        sink.mode = FinalTailMode::Ready;
        tokio::task::yield_now().await;
        assert_eq!(writes.lock().unwrap().bytes.len(), accepted);
        sink.write_all(b"caller-owned write").await.unwrap();
        let writes = writes.lock().unwrap();
        assert_eq!(&writes.bytes[accepted..], b"caller-owned write");
    }
}

#[tokio::test]
async fn partial_transport_or_psi_at_finite_eof_never_exposes_the_final_tail() {
    let prefix = fixture(3);
    let prefix_bytes = prefix.len() * 188;
    let mut unfinished_pmt = prefix[1];
    unfinished_pmt[3] = 0x11;
    unfinished_pmt[6] = 0xb1;
    unfinished_pmt[7] = 44;
    for tail in [vec![0x47], unfinished_pmt.to_vec()] {
        let mut input = flatten(&prefix);
        input.extend_from_slice(&tail);
        let mut source = Source::new(input, prefix_bytes, Tail::Eof);
        let mut sink = Sink::new(SinkMode::Ready);
        assert!(matches!(
            pump(&mut source, &mut sink).await,
            Err(TeletextPumpError::Gate(_))
        ));
        assert_eq!(sink.writes.lock().unwrap().bytes.len(), prefix_bytes);
        assert_eq!(source.position, source.bytes.len());
    }
}
