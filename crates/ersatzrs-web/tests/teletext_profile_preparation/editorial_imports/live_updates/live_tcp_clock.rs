//! Independent clock oracle for this finite, constant-rate synthetic live trial.
//! Retain the complete raw capture and `TSDuck` report. The pinned analyzer counts
//! adjacent per-PID PTS differences above 270,000 ticks without PMT context:
//! <https://github.com/tsduck/tsduck/blob/v3.44-4676/src/libtsduck/dtv/analysis/tsTSAnalyzer.cpp#L1531-L1544>
//! A qualified absence is not a zero-leap analyzer result or engine acceptance.

use ersatzrs_ffmpeg::teletext::source_pes::SuppliedTeletextPes;

const RATE: u128 = 4_953_600;
const PACKET_BITS: u128 = 1504;
const FRAME_TICKS: u64 = 1800;
const AUTHORED_PID: u16 = 512;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ClockGap {
    pub before_packet: usize,
    pub after_packet: usize,
    pub before_pts: u64,
    pub after_pts: u64,
    pub withdrawal_packet: usize,
    pub withdrawal_version: u8,
    pub announcement_packet: usize,
    pub announcement_version: u8,
}

#[derive(Clone, Copy)]
struct Announcement {
    packet: usize,
    version: u8,
}

struct Pmt {
    program: u16,
    pcr: u16,
    version: u8,
    authored: bool,
    ordinary: Vec<u8>,
}

fn section(payload: &[u8], table: u8) -> &[u8] {
    let start = 1 + usize::from(payload[0]);
    let data = payload.get(start..).expect("complete fixture PSI pointer");
    assert!(data.len() >= 12, "complete fixture PSI header");
    assert_eq!(data[0], table, "expected source PSI table");
    assert_eq!(data[1] & 0xf0, 0xb0);
    let extent = 3 + usize::from(u16::from_be_bytes([data[1] & 15, data[2]]));
    let data = data
        .get(..extent)
        .expect("complete single-packet fixture PSI");
    assert_eq!(data[5] & 1, 1, "current PSI required");
    assert_eq!(&data[6..8], &[0, 0], "single-section fixture PSI required");
    let mut crc = u32::MAX;
    for &byte in data {
        crc ^= u32::from(byte) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 == 0 {
                crc << 1
            } else {
                (crc << 1) ^ 0x04c1_1db7
            };
        }
    }
    assert_eq!(crc, 0, "source PSI CRC");
    data
}

fn pat(payload: &[u8]) -> (u16, u16) {
    let data = section(payload, 0);
    let entries = &data[8..data.len() - 4];
    assert_eq!(entries.len() % 4, 0);
    let mut programs = entries.chunks_exact(4).filter_map(|entry| {
        let program = u16::from_be_bytes([entry[0], entry[1]]);
        (program != 0).then(|| (program, u16::from_be_bytes([entry[2] & 31, entry[3]])))
    });
    let owner = programs.next().expect("sole actual source programme");
    assert!(programs.next().is_none(), "ambiguous source programme");
    owner
}

fn pmt(payload: &[u8]) -> Pmt {
    let data = section(payload, 2);
    let end = data.len() - 4;
    let header_end = 12 + usize::from(u16::from_be_bytes([data[10] & 15, data[11]]));
    assert!(header_end <= end);
    let mut ordinary = data[8..header_end].to_vec();
    let mut position = header_end;
    let mut authored = false;
    while position < end {
        assert!(position + 5 <= end);
        let pid = u16::from_be_bytes([data[position + 1] & 31, data[position + 2]]);
        let next = position
            + 5
            + usize::from(u16::from_be_bytes([
                data[position + 3] & 15,
                data[position + 4],
            ]));
        assert!(next <= end);
        if pid == AUTHORED_PID {
            assert!(!authored, "duplicate authored ES");
            assert_eq!(data[position], 6, "authored private data ES");
            let descriptors = &data[position + 5..next];
            assert!(descriptors.len() >= 7);
            assert_eq!(descriptors[0], 0x56, "actual teletext descriptor");
            assert_eq!(usize::from(descriptors[1]) + 2, descriptors.len());
            assert_eq!(descriptors[1] % 5, 0);
            authored = true;
        } else {
            ordinary.extend_from_slice(&data[position..next]);
        }
        position = next;
    }
    Pmt {
        program: u16::from_be_bytes([data[3], data[4]]),
        pcr: u16::from_be_bytes([data[8] & 31, data[9]]),
        version: (data[5] >> 1) & 31,
        authored,
        ordinary,
    }
}

fn check_frame(anchor: (usize, u64), packet: usize, pts: u64) {
    let offset = pts
        .checked_sub(anchor.1)
        .expect("authored PTS rebased backwards");
    assert!(
        offset > 0 && offset.is_multiple_of(FRAME_TICKS),
        "original authored PTS grid"
    );
    let distance = |ticks: u64| u128::from(ticks) * RATE / (90_000 * PACKET_BITS);
    let first = anchor.0 as u128 + 1 + distance(offset - FRAME_TICKS);
    let last = anchor.0 as u128 + 1 + distance(offset);
    assert!(
        (first..last).contains(&(packet as u128)),
        "authored PTS/source packet clock mismatch"
    );
}

fn header_pts(payload: &[u8]) -> u64 {
    assert!(
        payload.len() >= 46,
        "complete authored PES header in start packet"
    );
    assert_eq!(&payload[..4], &[0, 0, 1, 0xbd]);
    assert_eq!(&payload[6..9], &[0x84, 0x80, 36]);
    let data = &payload[9..14];
    assert_eq!(data[0] & 0xf1, 0x21, "PTS prefix and marker");
    assert_eq!(data[2] & 1, 1, "PTS middle marker");
    assert_eq!(data[4] & 1, 1, "PTS final marker");
    (u64::from((data[0] >> 1) & 7) << 30)
        | (u64::from(data[1]) << 22)
        | (u64::from(data[2] >> 1) << 15)
        | (u64::from(data[3]) << 7)
        | u64::from(data[4] >> 1)
}

#[derive(Default)]
struct Evidence {
    owner: Option<(u16, u16)>,
    original_pmt: Option<Pmt>,
    announced: bool,
    withdrawal: Option<Announcement>,
    resumed: Option<Announcement>,
    anchor: Option<(usize, u64)>,
    first_pcr: Option<(usize, u64)>,
    previous_pts: Option<(usize, u64)>,
    counter: Option<u8>,
    pes: Vec<u8>,
    pes_start: usize,
    gaps: Vec<ClockGap>,
    pcr_count: usize,
    pts_count: usize,
}

impl Evidence {
    fn observe_pcr(&mut self, packet: &[u8], index: usize, pid: u16) {
        assert!(packet[4] >= 7);
        let base = (u64::from(packet[6]) << 25)
            | (u64::from(packet[7]) << 17)
            | (u64::from(packet[8]) << 9)
            | (u64::from(packet[9]) << 1)
            | u64::from(packet[10] >> 7);
        let extension = u64::from(u16::from_be_bytes([packet[10] & 1, packet[11]]));
        assert_eq!(packet[10] & 0x7e, 0x7e, "PCR reserved marker bits");
        assert!(extension < 300);
        assert_eq!(
            pid,
            self.original_pmt
                .as_ref()
                .expect("PMT before source PCR")
                .pcr
        );
        // The ordinary FFmpeg route marks its first source PCR discontinuous.
        // It defines this new epoch's anchor; later resets remain forbidden.
        if self.first_pcr.is_some() {
            assert_eq!(packet[5] & 0x80, 0, "source PCR clock discontinuity");
        }
        let ticks = base * 300 + extension;
        let first = *self.first_pcr.get_or_insert((index, ticks));
        assert!(
            ticks >= first.1,
            "source PCR cannot rebase or wrap in this trial"
        );
        // Quantized PCR endpoints differ from exact packet time by less than
        // one 27-MHz tick; no measured/slack tolerance is added.
        let actual = u128::from(ticks - first.1) * RATE;
        let expected = (index - first.0) as u128 * PACKET_BITS * 27_000_000;
        assert!(
            actual.abs_diff(expected) < RATE,
            "source PCR/rate conservation"
        );
        self.anchor.get_or_insert((index, base));
        self.pcr_count += 1;
    }

    fn observe_pmt(&mut self, payload: &[u8], index: usize) {
        let current = pmt(payload);
        assert_eq!(
            current.program,
            self.owner.unwrap().0,
            "unchanged source PMT programme"
        );
        let event = Announcement {
            packet: index,
            version: current.version,
        };
        if let Some(original) = &self.original_pmt {
            assert_eq!(current.pcr, original.pcr, "unchanged source PCR ownership");
            assert_eq!(
                current.ordinary, original.ordinary,
                "unchanged ordinary ES ownership"
            );
            if self.announced && !current.authored {
                assert!(self.pes.is_empty(), "withdrawal cannot cut an authored PES");
                self.withdrawal = Some(event);
                self.resumed = None;
            } else if !self.announced && current.authored {
                self.resumed = Some(event);
            }
        }
        self.announced = current.authored;
        self.original_pmt.get_or_insert(current);
    }

    fn observe_pts(&mut self, pts: u64) {
        check_frame(self.anchor.expect("source PCR anchor"), self.pes_start, pts);
        if let Some((before_packet, before_pts)) = self.previous_pts {
            assert!(
                pts > before_pts,
                "authored PTS cannot go backwards or repeat"
            );
            if pts - before_pts > 270_000 {
                let off = self
                    .withdrawal
                    .expect("large PTS gap without PMT withdrawal");
                let on = self
                    .resumed
                    .expect("large PTS gap without PMT reannouncement");
                assert!(
                    before_packet < off.packet
                        && off.packet < on.packet
                        && on.packet < self.pes_start
                );
                assert_eq!(
                    on.version,
                    (off.version + 1) & 31,
                    "next PMT generation after Off"
                );
                self.gaps.push(ClockGap {
                    before_packet,
                    after_packet: self.pes_start,
                    before_pts,
                    after_pts: pts,
                    withdrawal_packet: off.packet,
                    withdrawal_version: off.version,
                    announcement_packet: on.packet,
                    announcement_version: on.version,
                });
            }
        }
        self.previous_pts = Some((self.pes_start, pts));
        self.pts_count += 1;
    }

    fn observe_pes(&mut self, packet: &[u8], payload: &[u8], index: usize) {
        assert!(self.announced, "authored packet while PMT absent");
        if packet[3] & 0x20 != 0 && packet[4] > 0 {
            assert_eq!(
                packet[5] & 0x80,
                0,
                "authored continuity cannot reset across Off"
            );
        }
        let cc = packet[3] & 15;
        if let Some(previous) = self.counter {
            assert_eq!(cc, (previous + 1) & 15, "authored continuity across Off");
        }
        self.counter = Some(cc);
        if packet[1] & 0x40 != 0 {
            assert!(self.pes.is_empty(), "complete previous authored PES");
            self.pes_start = index;
            self.observe_pts(header_pts(payload));
        } else {
            assert!(!self.pes.is_empty(), "authored PES continuation");
        }
        self.pes.extend_from_slice(payload);
        assert!(self.pes.len() >= 6);
        let extent = 6 + usize::from(u16::from_be_bytes([self.pes[4], self.pes[5]]));
        assert!(
            extent <= 1472 && self.pes.len() <= extent,
            "bounded authored PES framing"
        );
        if self.pes.len() != extent {
            return;
        }
        let parsed = SuppliedTeletextPes::parse(&self.pes).expect("valid actual authored PES");
        assert!(parsed
            .units()
            .all(|unit| matches!(unit.bytes()[0], 2 | 3 | 0xff)));
        assert_eq!(
            parsed.pts().expect("authored PTS present").ticks(),
            self.previous_pts.unwrap().1,
            "complete PES retains independently checked header PTS"
        );
        self.pes.clear();
    }
}

/// Qualify every analyzer-sized PTS gap against full raw PMT and clock evidence.
/// This is intentionally the existing 20-ms, nonwrapping synthetic trial oracle;
/// its PSI shape/rate assertions are not production engine restrictions.
/// The caller still runs the full `TSDuck`, ffprobe and unchanged source-AV gates.
pub(super) fn verify_clock_gaps(bytes: &[u8]) -> Vec<ClockGap> {
    let evidence = read_clock_evidence(bytes, true);
    assert!(
        evidence.pcr_count >= 2 && evidence.pts_count >= 2,
        "complete clock evidence"
    );
    evidence.gaps
}

/// A literal Off response has no authored clock. Preserve every source-PCR,
/// ownership and framing check, while requiring no teletext declaration/data.
/// The caller still verifies ordinary video/audio PTS and AV through `TSDuck`.
pub(super) fn verify_plain_source_clock(bytes: &[u8]) {
    let evidence = read_clock_evidence(bytes, false);
    assert!(evidence.pcr_count >= 2, "complete source PCR evidence");
    assert_eq!(evidence.pts_count, 0);
    assert!(evidence.gaps.is_empty());
}

fn read_clock_evidence(bytes: &[u8], allow_authored: bool) -> Evidence {
    assert_eq!(bytes.len() % 188, 0, "all complete captured packets");
    assert!(bytes.len() <= super::MAX_CAPTURE);
    let mut evidence = Evidence::default();
    for (index, packet) in bytes.chunks_exact(188).enumerate() {
        assert_eq!(packet[0], 0x47, "source sync");
        assert_eq!(packet[1] & 0x80, 0, "source transport error");
        assert_eq!(packet[3] & 0xc0, 0, "clear synthetic source");
        let pid = u16::from_be_bytes([packet[1] & 31, packet[2]]);
        let control = packet[3] & 0x30;
        assert_ne!(control, 0, "valid source adaptation control");
        if pid == AUTHORED_PID {
            assert!(allow_authored, "literal Off cannot carry authored packets");
            assert!(evidence.announced, "authored packet while PMT absent");
        }
        let payload_start = if control & 0x20 != 0 {
            let end = 5 + usize::from(packet[4]);
            assert!(end <= 188);
            if packet[4] > 0 && packet[5] & 0x10 != 0 {
                evidence.observe_pcr(packet, index, pid);
            }
            end
        } else {
            4
        };
        if control & 0x10 == 0 {
            continue;
        }
        let payload = &packet[payload_start..];
        assert!(!payload.is_empty());
        let start = packet[1] & 0x40 != 0;
        if pid == 0 && start {
            let current = pat(payload);
            assert_eq!(
                *evidence.owner.get_or_insert(current),
                current,
                "unchanged source PAT ownership"
            );
        }
        if evidence.owner.is_some_and(|(_, pmt_pid)| pid == pmt_pid) && start {
            evidence.observe_pmt(payload, index);
            assert!(
                allow_authored || !evidence.announced,
                "literal Off cannot announce teletext"
            );
        }
        if pid == AUTHORED_PID {
            evidence.observe_pes(packet, payload, index);
        }
    }
    evidence
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pmt_packet(version: u8, authored: bool) -> [u8; 188] {
        let mut data = vec![
            2,
            0xb0,
            0,
            0,
            1,
            0xc1 | (version << 1),
            0,
            0,
            0xe1,
            0,
            0xf0,
            0,
            0x1b,
            0xe1,
            0,
            0xf0,
            0,
            0x0f,
            0xe1,
            1,
            0xf0,
            0,
        ];
        if authored {
            data.extend_from_slice(&[6, 0xe2, 0, 0xf0, 7, 0x56, 5, b'n', b'l', b'd', 9, 0x24]);
        }
        super::super::section_packet(4096, data)
    }

    fn pts_packet(pts: u64, counter: u8) -> [u8; 188] {
        let mut packet = [0xff; 188];
        packet[..4].copy_from_slice(&[0x47, 0x42, 0, 0x10 | counter]);
        let pes = &mut packet[4..];
        pes[..9].copy_from_slice(&[0, 0, 1, 0xbd, 0, 178, 0x84, 0x80, 36]);
        pes[9..14].copy_from_slice(&[
            0x21 | u8::try_from((pts >> 29) & 0x0e).unwrap(),
            u8::try_from((pts >> 22) & 255).unwrap(),
            u8::try_from(((pts >> 14) & 0xfe) | 1).unwrap(),
            u8::try_from((pts >> 7) & 255).unwrap(),
            u8::try_from(((pts << 1) & 0xfe) | 1).unwrap(),
        ]);
        pes[45] = 0x10;
        for position in [47, 93, 139] {
            pes[position] = 44;
        }
        packet
    }

    fn fixture() -> (Vec<[u8; 188]>, ClockGap) {
        let first = |frame: u64| {
            4 + usize::try_from(
                u128::from((frame - 1) * FRAME_TICKS) * RATE / (90_000 * PACKET_BITS),
            )
            .unwrap()
                + 1
        };
        let before = first(2);
        let after = first(200);
        let off = before + 2;
        let on = after - 2;
        let mut null = [0xff; 188];
        null[..4].copy_from_slice(&[0x47, 31, 255, 0x10]);
        let mut packets = vec![null; first(202)];
        packets[0] =
            super::super::section_packet(0, vec![0, 0xb0, 0, 0, 1, 0xc1, 0, 0, 0, 1, 0xf0, 0]);
        packets[1] = pmt_packet(0, false);
        for index in (3..packets.len()).step_by(25) {
            let ticks = 83 * 300
                + u64::try_from((index - 3) as u128 * PACKET_BITS * 27_000_000 / RATE).unwrap();
            let base = ticks / 300;
            let extension = ticks % 300;
            let mut packet = [0xff; 188];
            packet[..6].copy_from_slice(&[0x47, 1, 0, 0x20, 183, 0x10]);
            packet[6..12].copy_from_slice(&[
                u8::try_from(base >> 25).unwrap(),
                u8::try_from((base >> 17) & 255).unwrap(),
                u8::try_from((base >> 9) & 255).unwrap(),
                u8::try_from((base >> 1) & 255).unwrap(),
                u8::try_from((base & 1) << 7).unwrap()
                    | 0x7e
                    | u8::try_from(extension >> 8).unwrap(),
                u8::try_from(extension & 255).unwrap(),
            ]);
            packets[index] = packet;
        }
        packets[3][5] |= 0x80;
        packets[10] = pmt_packet(1, true);
        packets[before] = pts_packet(83 + 2 * FRAME_TICKS, 0);
        packets[off] = pmt_packet(2, false);
        packets[on] = pmt_packet(3, true);
        packets[after] = pts_packet(83 + 200 * FRAME_TICKS, 1);
        packets[first(201)] = pts_packet(83 + 201 * FRAME_TICKS, 2);
        (
            packets,
            ClockGap {
                before_packet: before,
                after_packet: after,
                before_pts: 3683,
                after_pts: 360_083,
                withdrawal_packet: off,
                withdrawal_version: 2,
                announcement_packet: on,
                announcement_version: 3,
            },
        )
    }

    fn bytes(packets: &[[u8; 188]]) -> Vec<u8> {
        packets.iter().flatten().copied().collect()
    }

    #[test]
    fn literal_off_interval_qualifies_only_conserved_clock_gap_with_exact_provenance() {
        let (packets, expected) = fixture();
        assert_eq!(verify_clock_gaps(&bytes(&packets)), [expected]);
    }

    #[test]
    fn arbitrary_backward_or_rebased_pts_is_rejected_even_with_real_off_interval() {
        for value in [360_084, 3683, 1883, 361_883] {
            let (mut packets, gap) = fixture();
            packets[gap.after_packet] = pts_packet(value, 1);
            assert!(std::panic::catch_unwind(|| verify_clock_gaps(&bytes(&packets))).is_err());
        }
    }

    #[test]
    fn missing_withdrawal_reannouncement_or_fake_absence_cannot_explain_gap() {
        for mode in 0..3 {
            let (mut packets, gap) = fixture();
            match mode {
                0 => packets[gap.withdrawal_packet] = pmt_packet(2, true),
                1 => packets[gap.announcement_packet] = pmt_packet(3, false),
                _ => packets[gap.withdrawal_packet][25] ^= 1, // Corrupt actual PMT CRC/body.
            }
            assert!(std::panic::catch_unwind(|| verify_clock_gaps(&bytes(&packets))).is_err());
        }
    }

    #[test]
    fn authored_packets_while_off_and_continuity_reset_are_rejected() {
        for mode in 0..2 {
            let (mut packets, gap) = fixture();
            if mode == 0 {
                packets[gap.withdrawal_packet + 1] = pts_packet(gap.before_pts + FRAME_TICKS, 1);
            } else {
                packets[gap.after_packet][3] = 0x10;
            }
            assert!(std::panic::catch_unwind(|| verify_clock_gaps(&bytes(&packets))).is_err());
        }
    }

    #[test]
    fn pcr_rate_and_pmt_source_ownership_changes_are_rejected() {
        for mode in 0..4 {
            let (mut packets, gap) = fixture();
            match mode {
                0 => packets[28][9] ^= 1,
                1 => {
                    packets[gap.announcement_packet] = super::super::section_packet(
                        4096,
                        vec![
                            2, 0xb0, 0, 0, 1, 0xc7, 0, 0, 0xe1, 1, 0xf0, 0, 0x1b, 0xe1, 0, 0xf0, 0,
                        ],
                    );
                }
                2 => {
                    packets[gap.announcement_packet] = super::super::section_packet(
                        4096,
                        vec![
                            2, 0xb0, 0, 0, 2, 0xc7, 0, 0, 0xe1, 0, 0xf0, 0, 0x1b, 0xe1, 0, 0xf0, 0,
                        ],
                    );
                }
                _ => packets[28][5] |= 0x80,
            }
            assert!(std::panic::catch_unwind(|| verify_clock_gaps(&bytes(&packets))).is_err());
        }
    }

    #[test]
    fn plain_off_keeps_exact_source_clock_and_rejects_declaration_data_drift_or_missing_pcr() {
        let (original, _) = fixture();
        let mut null = [0xff; 188];
        null[..4].copy_from_slice(&[0x47, 31, 255, 0x10]);
        let plain: Vec<_> = original
            .into_iter()
            .map(|packet| {
                let pid = u16::from_be_bytes([packet[1] & 31, packet[2]]);
                match pid {
                    4096 => pmt_packet(0, false),
                    AUTHORED_PID => null,
                    _ => packet,
                }
            })
            .collect();
        verify_plain_source_clock(&bytes(&plain));
        // The existing authored oracle still requires actual teletext PTS.
        assert!(std::panic::catch_unwind(|| verify_clock_gaps(&bytes(&plain))).is_err());
        for mode in 0..4 {
            let mut broken = plain.clone();
            match mode {
                0 => broken[10] = pmt_packet(1, true),
                1 => broken[11] = pts_packet(83 + FRAME_TICKS, 0),
                2 => broken[28][9] ^= 1,
                _ => {
                    for packet in broken.iter_mut().skip(4) {
                        if packet[3] & 0x20 != 0 && packet[4] > 0 && packet[5] & 0x10 != 0 {
                            *packet = null;
                        }
                    }
                }
            }
            assert!(
                std::panic::catch_unwind(|| verify_plain_source_clock(&bytes(&broken))).is_err(),
                "mode{mode}"
            );
        }
    }
}
