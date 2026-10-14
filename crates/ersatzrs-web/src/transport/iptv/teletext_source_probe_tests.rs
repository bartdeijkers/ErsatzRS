//! Actual byte ownership and policy outcomes, without a source reopen or sink.

use super::super::teletext_output::tests::{authored_set, fixture, flatten};
use super::*;
use ersatzrs_ffmpeg::teletext::TeletextMultiplexError;
use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::ReadBuf;

fn service() -> TeletextServiceDescriptor {
    TeletextServiceDescriptor::from_page_set(&authored_set())
}
fn policy(maximum_bytes: usize) -> TeletextSourceProbePolicy {
    TeletextSourceProbePolicy {
        maximum_bytes,
        deadline: Instant::now() + Duration::from_secs(1),
        first_byte_timeout: Duration::from_millis(100),
    }
}

// Rewrite existing literal valid PSI fixtures; retain their original field
// layout and use independent MPEG CRC arithmetic for the changed section.
fn checksum(packet: &mut [u8; 188], extent: usize) {
    let length = u16::try_from(extent - 3).unwrap().to_be_bytes();
    packet[6] = 0xb0 | length[0];
    packet[7] = length[1];
    let mut crc = u32::MAX;
    for &byte in &packet[5..5 + extent - 4] {
        crc ^= u32::from(byte) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 == 0 {
                crc << 1
            } else {
                (crc << 1) ^ 0x04c1_1db7
            };
        }
    }
    packet[5 + extent - 4..5 + extent].copy_from_slice(&crc.to_be_bytes());
}
fn actual() -> [[u8; 188]; 3] {
    let original = fixture(1);
    let mut pat = original[0];
    pat[14] = 37;
    checksum(&mut pat, 16);
    let mut pmt = original[1];
    pmt[9] = 37;
    pmt[13] = 0xe3;
    pmt[14] = 0x85; // Actual selected PCR PID 901, absent from policy fixtures.
    checksum(&mut pmt, 21);
    let mut pcr = original[2];
    pcr[1] = 3;
    pcr[2] = 0x85;
    [pat, pmt, pcr]
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
    polls: usize,
    tail: Tail,
}
impl Source {
    fn new(bytes: Vec<u8>, chunk: usize, tail: Tail) -> Self {
        Self {
            bytes,
            position: 0,
            chunk,
            polls: 0,
            tail,
        }
    }
}
impl AsyncRead for Source {
    fn poll_read(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        output: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        this.polls += 1;
        if this.position == this.bytes.len() {
            return match this.tail {
                Tail::Eof => Poll::Ready(Ok(())),
                Tail::Stall => Poll::Pending,
                Tail::Error(kind) => Poll::Ready(Err(io::Error::from(kind))),
            };
        }
        let count = this
            .chunk
            .min(output.remaining())
            .min(this.bytes.len() - this.position);
        output.put_slice(&this.bytes[this.position..this.position + count]);
        this.position += count;
        Poll::Ready(Ok(()))
    }
}

#[tokio::test(start_paused = true)]
async fn non_one_program_and_actual_pcr_are_discovered_after_whole_chunk() {
    let bytes = flatten(&actual());
    let mut source = Source::new(bytes.clone(), usize::MAX, Tail::Error(io::ErrorKind::Other));
    let admitted = probe_teletext_source(&mut source, &service(), 512, policy(8 * 188))
        .await
        .unwrap();
    assert_eq!(
        (
            admitted.plan.program_number(),
            admitted.plan.pmt_pid(),
            admitted.plan.pcr_pid()
        ),
        (37, 3337, 901)
    );
    assert_eq!(admitted.prefix.bytes(), bytes);
    assert_eq!(source.position, bytes.len());
    assert_eq!(source.polls, 1);
    assert_eq!(admitted.prefix.first_arrival(), Duration::ZERO);
    assert!(admitted.prefix.epoch() <= Instant::now());
    assert!(!format!("{admitted:?}").contains("source:"));
}

#[tokio::test(start_paused = true)]
async fn initially_absent_probe_retains_exact_pmt_version_and_source_bytes() {
    let packets = actual();
    let bytes = flatten(&packets);
    let mut source = Source::new(bytes.clone(), usize::MAX, Tail::Error(io::ErrorKind::Other));
    let admitted = probe_teletext_source_optional(&mut source, None, 512, policy(8 * 188))
        .await
        .unwrap();
    assert_eq!(
        (
            admitted.plan.program_number(),
            admitted.plan.pmt_pid(),
            admitted.plan.pcr_pid()
        ),
        (37, 3337, 901)
    );
    assert_eq!(admitted.plan.elementary_pid(), 512);
    assert_eq!(admitted.plan.section(), &packets[1][5..26]);
    assert_eq!(admitted.plan.section(), admitted.plan.source_section());
    assert_eq!(admitted.plan.section()[5], 0xc1);
    assert_eq!(admitted.prefix.bytes(), bytes);
    assert_eq!(source.polls, 1);
    assert_eq!(source.position, bytes.len());
}

#[tokio::test(start_paused = true)]
async fn initially_absent_probe_rejects_late_pid_collision_and_preserves_partial_tail() {
    let mut packets = actual().to_vec();
    let mut collision = packets[2];
    collision[1] = 2;
    collision[2] = 0;
    packets.push(collision);
    let mut bytes = flatten(&packets);
    bytes.extend_from_slice(b"partial");
    let mut source = Source::new(bytes.clone(), usize::MAX, Tail::Eof);
    let failure = probe_teletext_source_optional(&mut source, None, 512, policy(8 * 188))
        .await
        .unwrap_err();
    assert_eq!(
        failure.reason,
        TeletextSourceProbeError::Psi(TeletextPsiError::Multiplex(
            TeletextMultiplexError::PidCollision { pid: 512 }
        ))
    );
    assert_eq!(failure.raw_prefix, bytes);
    assert_eq!(source.position, bytes.len());
    assert_eq!(source.polls, 1);
}

#[tokio::test(start_paused = true)]
async fn initially_absent_probe_still_requires_complete_bounded_topology() {
    let bytes = flatten(&actual());
    let mut source = Source::new(bytes.clone(), usize::MAX, Tail::Eof);
    let failure = probe_teletext_source_optional(&mut source, None, 512, policy(191))
        .await
        .unwrap_err();
    assert_eq!(failure.reason, TeletextSourceProbeError::Capacity);
    assert_eq!(failure.raw_prefix, &bytes[..191]);
    assert_eq!(source.position, 191);
    assert_eq!(source.polls, 1);
    let mut source = Source::new(bytes, usize::MAX, Tail::Eof);
    let failure = probe_teletext_source_optional(&mut source, None, 512, policy(0))
        .await
        .unwrap_err();
    assert_eq!(failure.reason, TeletextSourceProbeError::Configuration);
    assert!(failure.raw_prefix.is_empty());
    assert_eq!(source.polls, 0);
}

#[tokio::test(start_paused = true)]
async fn late_collision_in_same_ready_chunk_rejects_without_losing_tail_or_hidden_reads() {
    let mut packets = actual().to_vec();
    let mut collision = packets[2];
    collision[1] = 2;
    collision[2] = 0;
    packets.push(collision);
    let mut bytes = flatten(&packets);
    bytes.extend_from_slice(b"private");
    let mut source = Source::new(bytes.clone(), usize::MAX, Tail::Eof);
    let failure = probe_teletext_source(&mut source, &service(), 512, policy(8 * 188))
        .await
        .unwrap_err();
    assert_eq!(
        failure.reason,
        TeletextSourceProbeError::Psi(TeletextPsiError::Multiplex(
            TeletextMultiplexError::PidCollision { pid: 512 }
        ))
    );
    assert!(!format!("{failure:?}").contains("private"));
    assert_eq!(failure.raw_prefix, bytes);
    assert_eq!(source.position, bytes.len());
    assert_eq!(source.polls, 1);
}

#[tokio::test(start_paused = true)]
async fn all_advertised_programs_are_parsed_before_visible_ambiguity() {
    let [mut pat, first, _] = actual();
    pat[17..21].copy_from_slice(&[0, 41, 0xed, 0x80]);
    checksum(&mut pat, 20);
    let mut second = first;
    second[1] = 0x4d;
    second[2] = 0x80;
    second[9] = 41;
    checksum(&mut second, 21);
    let bytes = flatten(&[pat, first, second]);
    let mut source = Source::new(bytes.clone(), 2 * 188, Tail::Eof);
    let failure = probe_teletext_source(&mut source, &service(), 512, policy(8 * 188))
        .await
        .unwrap_err();
    assert_eq!(
        failure.reason,
        TeletextSourceProbeError::Psi(TeletextPsiError::AmbiguousProgram)
    );
    assert_eq!(failure.raw_prefix, bytes);
    assert_eq!(source.position, bytes.len());
    assert_eq!(source.polls, 2);
}

#[tokio::test(start_paused = true)]
async fn byte_fragments_and_pcr_before_pat_keep_exact_prefix_and_partial_tail() {
    let [pat, pmt, pcr] = actual();
    let packets = [pcr, pat, pmt];
    let mut full = flatten(&packets);
    full.extend_from_slice(b"partial");
    for chunk in [1, full.len()] {
        let mut source = Source::new(full.clone(), chunk, Tail::Eof);
        let admitted = probe_teletext_source(&mut source, &service(), 512, policy(8 * 188))
            .await
            .unwrap();
        let consumed = if chunk == 1 {
            packets.len() * 188
        } else {
            full.len()
        };
        assert_eq!(admitted.prefix.bytes(), &full[..consumed]);
        assert_eq!(&admitted.prefix.bytes()[..188], pcr);
        assert_eq!(source.position, consumed);
        assert_eq!(admitted.plan.pcr_pid(), 901);
        let mut remainder = Vec::new();
        source.read_to_end(&mut remainder).await.unwrap();
        assert_eq!(remainder, full[consumed..]);
    }
}

#[tokio::test(start_paused = true)]
async fn partial_byte_cap_rejects_before_the_next_source_byte() {
    let bytes = flatten(&actual());
    let mut source = Source::new(bytes.clone(), usize::MAX, Tail::Eof);
    let failure = probe_teletext_source(&mut source, &service(), 512, policy(191))
        .await
        .unwrap_err();
    assert_eq!(failure.reason, TeletextSourceProbeError::Capacity);
    assert_eq!(failure.raw_prefix, &bytes[..191]);
    assert_eq!(source.position, 191);
    assert_eq!(source.polls, 1);
    let mut remainder = Vec::new();
    source.read_to_end(&mut remainder).await.unwrap();
    assert_eq!(remainder, bytes[191..]);
}

#[tokio::test(start_paused = true)]
async fn probe_deadline_and_first_byte_timeout_are_distinct_and_keep_consumed_bytes() {
    let bytes = vec![0x47];
    let mut source = Source::new(bytes.clone(), 1, Tail::Stall);
    let mut limits = policy(8 * 188);
    limits.deadline = Instant::now() + Duration::from_millis(30);
    let epoch = Instant::now();
    let failure = probe_teletext_source(&mut source, &service(), 512, limits)
        .await
        .unwrap_err();
    assert_eq!(failure.reason, TeletextSourceProbeError::Deadline);
    assert_eq!(failure.raw_prefix, bytes);
    assert_eq!(
        Instant::now().checked_duration_since(epoch).unwrap(),
        Duration::from_millis(30)
    );
    let mut empty = Source::new(Vec::new(), 1, Tail::Stall);
    let failure = probe_teletext_source(&mut empty, &service(), 512, policy(188))
        .await
        .unwrap_err();
    assert_eq!(failure.reason, TeletextSourceProbeError::FirstByteTimeout);
    assert!(failure.raw_prefix.is_empty());
    assert_eq!(empty.position, 0);
}

#[tokio::test(start_paused = true)]
async fn actual_read_error_empty_and_incomplete_eof_do_not_become_authored_absence() {
    for (bytes, tail, expected) in [
        (
            vec![0x47],
            Tail::Error(io::ErrorKind::UnexpectedEof),
            TeletextSourceProbeError::Read(io::ErrorKind::UnexpectedEof),
        ),
        (Vec::new(), Tail::Eof, TeletextSourceProbeError::Empty),
        (
            vec![0x47],
            Tail::Eof,
            TeletextSourceProbeError::Psi(TeletextPsiError::Incomplete),
        ),
    ] {
        let mut source = Source::new(bytes.clone(), 1, tail);
        let failure = probe_teletext_source(&mut source, &service(), 512, policy(188))
            .await
            .unwrap_err();
        assert_eq!(failure.reason, expected);
        assert_eq!(failure.raw_prefix, bytes);
        assert_eq!(source.position, bytes.len());
    }
}

#[tokio::test(start_paused = true)]
async fn invalid_policy_and_expired_probe_never_poll_stdout() {
    for limits in [
        policy(0),
        policy(MAXIMUM_BYTES + 1),
        TeletextSourceProbePolicy {
            first_byte_timeout: Duration::ZERO,
            ..policy(188)
        },
        TeletextSourceProbePolicy {
            deadline: Instant::now(),
            ..policy(188)
        },
    ] {
        let mut source = Source::new(flatten(&actual()), 1, Tail::Eof);
        let failure = probe_teletext_source(&mut source, &service(), 512, limits)
            .await
            .unwrap_err();
        assert!(matches!(
            failure.reason,
            TeletextSourceProbeError::Configuration | TeletextSourceProbeError::Deadline
        ));
        assert!(failure.raw_prefix.is_empty());
        assert_eq!(source.polls, 0);
        assert_eq!(source.position, 0);
    }
}
