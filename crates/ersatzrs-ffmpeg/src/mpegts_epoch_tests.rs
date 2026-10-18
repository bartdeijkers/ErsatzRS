use super::{MpegTsEpochError, MpegTsEpochRewriter, MAX_PENDING_PACKETS};

const RATE: u64 = 1_504_000; // Exactly one physical packet per millisecond.
const WRAP: u64 = 1 << 33;

fn packet(pid: u16, cc: u8, start: bool, data: &[u8]) -> [u8; 188] {
    assert!(data.len() <= 184);
    let mut out = [0xff; 188];
    out[0] = 0x47;
    out[1] = u8::try_from(pid >> 8).unwrap() | if start { 0x40 } else { 0 };
    out[2] = pid.to_le_bytes()[0];
    out[3] = 0x10 | cc;
    let offset = 188 - data.len();
    if offset > 4 {
        out[3] |= 0x20;
        out[4] = u8::try_from(offset - 5).unwrap();
        if offset > 5 {
            out[5] = 0;
        }
    }
    out[offset..].copy_from_slice(data);
    out
}

fn timestamp(value: u64, prefix: u8) -> [u8; 5] {
    [
        (prefix << 4) | u8::try_from((value >> 29) & 0x0e).unwrap() | 1,
        u8::try_from((value >> 22) & 255).unwrap(),
        u8::try_from((value >> 14) & 0xfe).unwrap() | 1,
        u8::try_from((value >> 7) & 255).unwrap(),
        u8::try_from((value << 1) & 0xfe).unwrap() | 1,
    ]
}

fn decode_timestamp(bytes: &[u8]) -> u64 {
    (u64::from(bytes[0] & 0x0e) << 29)
        | (u64::from(bytes[1]) << 22)
        | (u64::from(bytes[2] & 0xfe) << 14)
        | (u64::from(bytes[3]) << 7)
        | u64::from(bytes[4] >> 1)
}

fn pcr_bytes(ticks: u64) -> [u8; 6] {
    let base = ticks / 300;
    let ext = ticks % 300;
    [
        u8::try_from((base >> 25) & 255).unwrap(),
        u8::try_from((base >> 17) & 255).unwrap(),
        u8::try_from((base >> 9) & 255).unwrap(),
        u8::try_from((base >> 1) & 255).unwrap(),
        u8::try_from((base & 1) << 7).unwrap() | 0x7e | u8::try_from(ext >> 8).unwrap(),
        u8::try_from(ext & 255).unwrap(),
    ]
}

fn decode_pcr(bytes: &[u8]) -> u64 {
    let base = (u64::from(bytes[0]) << 25)
        | (u64::from(bytes[1]) << 17)
        | (u64::from(bytes[2]) << 9)
        | (u64::from(bytes[3]) << 1)
        | u64::from(bytes[4] >> 7);
    base * 300 + (u64::from(bytes[4] & 1) << 8) + u64::from(bytes[5])
}

fn clock(ticks: u64, cc: u8) -> [u8; 188] {
    let mut out = [0xff; 188];
    out[..6].copy_from_slice(&[0x47, 1, 0, 0x20 | cc, 183, 0x10]);
    out[6..12].copy_from_slice(&pcr_bytes(ticks));
    out
}

fn section(mut bytes: Vec<u8>) -> Vec<u8> {
    let length = u16::try_from(bytes.len() + 1).unwrap();
    bytes[1] = 0xb0 | u8::try_from(length >> 8).unwrap();
    bytes[2] = length.to_le_bytes()[0];
    bytes.extend_from_slice(&mpeg2ts_reader::mpegts_crc::sum32(&bytes).to_be_bytes());
    bytes
}

fn pat(version: u8, transport: u16) -> Vec<u8> {
    let [hi, lo] = transport.to_be_bytes();
    section(vec![
        0,
        0,
        0,
        hi,
        lo,
        0xc1 | version << 1,
        0,
        0,
        0,
        1,
        0xf0,
        0,
    ])
}

fn pmt(version: u8, extra: bool) -> Vec<u8> {
    let mut bytes = vec![2, 0, 0, 0, 1, 0xc1 | version << 1, 0, 0, 0xe1, 0, 0xf0, 0];
    bytes.extend_from_slice(&[0x1b, 0xe1, 0, 0xf0, 0]);
    bytes.extend_from_slice(&[0x0f, 0xe1, 1, 0xf0, 6, 0x0a, 4, b'e', b'n', b'g', 0]);
    if extra {
        bytes.extend_from_slice(&[0x81, 0xe1, 2, 0xf0, 6, 0x0a, 4, b'n', b'l', b'd', 0]);
        bytes.extend_from_slice(&[6, 0xe1, 3, 0xf0, 7, 0x56, 5, b'n', b'l', b'd', 0x11, 0x88]);
    }
    section(bytes)
}

fn psi(pid: u16, cc: u8, section: &[u8]) -> [u8; 188] {
    let mut payload = vec![0];
    payload.extend_from_slice(section);
    packet(pid, cc, true, &payload)
}

fn drain(writer: &mut MpegTsEpochRewriter) -> Vec<[u8; 188]> {
    std::iter::from_fn(|| writer.pop_ready_packet()).collect()
}

fn seed(writer: &mut MpegTsEpochRewriter, rate: u64, ticks: u64, extra: bool) -> Vec<[u8; 188]> {
    writer.begin_epoch(rate).unwrap();
    writer.push_packet(psi(0, 0, &pat(0, 1))).unwrap();
    writer.push_packet(psi(0x1000, 0, &pmt(0, extra))).unwrap();
    assert!(drain(writer).is_empty(), "PSI alone is not a clock");
    writer.push_packet(clock(ticks, 0)).unwrap();
    drain(writer)
}

fn pes(pts: u64, dts: Option<u64>) -> Vec<u8> {
    let mut bytes = vec![
        0,
        0,
        1,
        0xe0,
        0,
        0,
        0x80,
        if dts.is_some() { 0xc0 } else { 0x80 },
        if dts.is_some() { 10 } else { 5 },
    ];
    bytes.extend_from_slice(&timestamp(pts, if dts.is_some() { 3 } else { 2 }));
    if let Some(dts) = dts {
        bytes.extend_from_slice(&timestamp(dts, 1));
    }
    bytes
}

fn payload(bytes: &[u8; 188]) -> &[u8] {
    &bytes[if bytes[3] & 0x20 != 0 {
        5 + usize::from(bytes[4])
    } else {
        4
    }..]
}

#[test]
fn physical_packet_distance_accounts_for_both_epoch_bitrates() {
    let mut writer = MpegTsEpochRewriter::default();
    let initial = seed(&mut writer, RATE, 27_000_000, false);
    assert_eq!(decode_pcr(&initial[2][6..12]), 27_000_000);
    for _ in 0..7 {
        writer
            .push_packet(packet(0x1fff, 0, false, &[0xff; 184]))
            .unwrap();
        drain(&mut writer);
    }
    writer.finish_epoch().unwrap();
    // Eight old-rate packets after the old PCR start, two new-rate packets
    // before the new PCR: 8 ms + 2 * 0.5 ms = 9 ms.
    let next = seed(&mut writer, RATE * 2, 90_000, true);
    assert_eq!(decode_pcr(&next[2][6..12]), 27_243_000);
    assert_eq!(next[0][3] & 15, 1);
    assert_eq!(next[1][3] & 15, 1);
    let mut expected = pmt(1, true);
    assert_eq!(&payload(&next[1])[1..], expected.as_slice());
    expected = pat(0, 1);
    assert_eq!(&payload(&next[0])[1..], expected.as_slice());
    writer.finish_epoch().unwrap();
}

#[test]
fn split_pts_dts_wrap_reordering_and_interleaved_pids_use_one_offset() {
    for split in 1..19 {
        let mut writer = MpegTsEpochRewriter::default();
        seed(&mut writer, RATE, (WRAP - 10) * 300, true);
        writer.finish_epoch().unwrap();
        let next = seed(&mut writer, RATE, 0, true);
        let offset = decode_pcr(&next[2][6..12]) / 300;
        let header = pes(WRAP - 3, Some(WRAP - 9));
        let first = packet(0x100, 0, true, &header[..split]);
        writer.push_packet(first).unwrap();
        assert!(drain(&mut writer).is_empty());
        let mut caption = pes(7, None);
        caption[3] = 0xbd;
        writer
            .push_packet(packet(0x103, 0, true, &caption))
            .unwrap();
        assert!(drain(&mut writer).is_empty());
        writer
            .push_packet(packet(0x100, 1, false, &header[split..]))
            .unwrap();
        let output = drain(&mut writer);
        let joined = [payload(&output[0]), payload(&output[2])].concat();
        assert_eq!(decode_timestamp(&joined[9..14]), (WRAP - 3 + offset) % WRAP);
        assert_eq!(
            decode_timestamp(&joined[14..19]),
            (WRAP - 9 + offset) % WRAP
        );
        assert_eq!(
            decode_timestamp(&payload(&output[1])[9..14]),
            (7 + offset) % WRAP
        );
        assert_eq!(&joined[..9], &header[..9]);
        writer.finish_epoch().unwrap();
    }
}

#[test]
fn duplicates_before_and_after_drain_share_payload_but_not_adaptation() {
    let mut writer = MpegTsEpochRewriter::default();
    seed(&mut writer, RATE, 27_000, false);
    writer.finish_epoch().unwrap();
    seed(&mut writer, RATE, 0, false);
    let header = pes(500, Some(400));
    let first = packet(0x100, 7, true, &header[..12]);
    writer.push_packet(first).unwrap();
    writer.push_packet(first).unwrap();
    writer
        .push_packet(packet(0x100, 8, false, &header[12..]))
        .unwrap();
    let output = drain(&mut writer);
    assert_eq!(output[0], output[1]);
    assert_eq!(output[2][3] & 15, 8);
    let tail = packet(0x100, 8, false, &header[12..]);
    writer.push_packet(tail).unwrap();
    assert_eq!(drain(&mut writer)[0], output[2]);
    writer.push_packet(clock(27_000, 8)).unwrap();
    assert_eq!(drain(&mut writer)[0][3] & 15, 8);
    writer.finish_epoch().unwrap();
}

#[test]
fn pmt_full_inventory_and_descriptors_survive_split_and_version_wrap() {
    let mut writer = MpegTsEpochRewriter::default();
    writer.begin_epoch(RATE).unwrap();
    writer.push_packet(psi(0, 0, &pat(31, 1))).unwrap();
    writer.push_packet(psi(0x1000, 0, &pmt(31, false))).unwrap();
    writer.push_packet(clock(0, 0)).unwrap();
    drain(&mut writer);
    writer.finish_epoch().unwrap();
    writer.begin_epoch(RATE).unwrap();
    writer.push_packet(psi(0, 0, &pat(5, 2))).unwrap();
    let table = pmt(17, true);
    let split = 20;
    let mut first = vec![0];
    first.extend_from_slice(&table[..split]);
    writer.push_packet(packet(0x1000, 0, true, &first)).unwrap();
    writer.push_packet(clock(0, 0)).unwrap();
    assert!(drain(&mut writer).is_empty());
    writer
        .push_packet(packet(0x1000, 1, false, &table[split..]))
        .unwrap();
    let output = drain(&mut writer);
    let actual = [&payload(&output[1])[1..], payload(&output[3])].concat();
    assert_eq!(actual, pmt(0, true));
    assert_eq!(mpeg2ts_reader::mpegts_crc::sum32(&actual), 0);
    assert_eq!(&payload(&output[0])[1..], pat(0, 2));
    writer.finish_epoch().unwrap();
}

#[test]
fn multi_section_pat_is_held_until_the_complete_current_table() {
    let mut first = vec![0, 0, 0, 0, 1, 0xc1, 0, 1, 0, 0, 0xe0, 0x10];
    first = section(first);
    let second = section(vec![0, 0, 0, 0, 1, 0xc1, 1, 1, 0, 1, 0xf0, 0]);
    let mut writer = MpegTsEpochRewriter::default();
    writer.begin_epoch(RATE).unwrap();
    writer.push_packet(psi(0, 0, &first)).unwrap();
    assert!(drain(&mut writer).is_empty());
    writer.push_packet(psi(0, 1, &second)).unwrap();
    writer.push_packet(psi(0x1000, 0, &pmt(0, false))).unwrap();
    writer.push_packet(clock(0, 0)).unwrap();
    let output = drain(&mut writer);
    assert_eq!(&payload(&output[0])[1..], first);
    assert_eq!(&payload(&output[1])[1..], second);
    writer.finish_epoch().unwrap();
}

#[test]
fn empty_retries_do_not_advance_the_response_clock_or_continuity() {
    let mut writer = MpegTsEpochRewriter::default();
    seed(&mut writer, RATE, 27_000_000, false);
    writer.finish_epoch().unwrap();
    writer.begin_epoch(RATE * 4).unwrap();
    writer.finish_epoch().unwrap();
    let next = seed(&mut writer, RATE, 0, false);
    assert_eq!(decode_pcr(&next[2][6..12]), 27_081_000);
    assert_eq!(next[0][3] & 15, 1);
    writer.finish_epoch().unwrap();
}

#[test]
fn malformed_split_pes_is_sticky_and_never_drains_its_prefix() {
    let mut writer = MpegTsEpochRewriter::default();
    seed(&mut writer, RATE, 0, false);
    let mut header = pes(50, Some(30));
    header[16] &= !1;
    writer
        .push_packet(packet(0x100, 0, true, &header[..12]))
        .unwrap();
    assert_eq!(
        writer.push_packet(packet(0x100, 1, false, &header[12..])),
        Err(MpegTsEpochError::Pes)
    );
    assert!(drain(&mut writer).is_empty());
    assert_eq!(writer.finish_epoch(), Err(MpegTsEpochError::Pes));
}

#[test]
fn invalid_crc_incomplete_header_unknown_tracks_and_pes_extensions_fail_closed() {
    let mut writer = MpegTsEpochRewriter::default();
    writer.begin_epoch(RATE).unwrap();
    let mut bad = pat(0, 1);
    bad[8] ^= 1;
    assert_eq!(
        writer.push_packet(psi(0, 0, &bad)),
        Err(MpegTsEpochError::Psi)
    );
    assert!(drain(&mut writer).is_empty());

    let mut writer = MpegTsEpochRewriter::default();
    seed(&mut writer, RATE, 0, false);
    writer
        .push_packet(packet(0x100, 0, true, &[0, 0, 1]))
        .unwrap();
    assert_eq!(writer.finish_epoch(), Err(MpegTsEpochError::Incomplete));
    assert!(drain(&mut writer).is_empty());

    {
        let mut writer = MpegTsEpochRewriter::default();
        seed(&mut writer, RATE, 0, false);
        let mut header = pes(10, None);
        header[7] |= 2;
        assert_eq!(
            writer.push_packet(packet(0x100, 0, true, &header)),
            Err(MpegTsEpochError::UnsupportedPes)
        );
    }
    let mut writer = MpegTsEpochRewriter::default();
    seed(&mut writer, RATE, 0, false);
    assert_eq!(
        writer.push_packet(packet(0x110, 0, true, &pes(10, None))),
        Err(MpegTsEpochError::Programme)
    );
}

#[test]
fn pending_transport_has_a_fixed_storage_bound() {
    let mut writer = MpegTsEpochRewriter::default();
    writer.begin_epoch(RATE).unwrap();
    for _ in 0..MAX_PENDING_PACKETS {
        writer
            .push_packet(packet(0x1fff, 0, false, &[0xff; 184]))
            .unwrap();
    }
    assert_eq!(
        writer.push_packet(packet(0x1fff, 0, false, &[0xff; 184])),
        Err(MpegTsEpochError::Capacity)
    );
    assert!(drain(&mut writer).is_empty());
}

#[test]
fn input_cc_and_mid_epoch_discontinuity_are_not_silently_repaired() {
    for discontinuity in [false, true] {
        let mut writer = MpegTsEpochRewriter::default();
        seed(&mut writer, RATE, 0, false);
        writer
            .push_packet(packet(0x100, 0, true, &pes(10, None)))
            .unwrap();
        drain(&mut writer);
        let mut next = packet(
            0x100,
            if discontinuity { 1 } else { 3 },
            true,
            &pes(20, None),
        );
        if discontinuity {
            next[5] |= 0x80;
        }
        assert_eq!(writer.push_packet(next), Err(MpegTsEpochError::Transport));
    }
}

fn escr(value: u64) -> [u8; 6] {
    let base = value / 300;
    let ext = value % 300;
    // Pack the syntax bit fields independently of the production byte masks.
    let fields = [
        (0, 2),
        (base >> 30, 3),
        (1, 1),
        ((base >> 15) & 0x7fff, 15),
        (1, 1),
        (base & 0x7fff, 15),
        (1, 1),
        (ext, 9),
        (1, 1),
    ];
    let bits = fields
        .into_iter()
        .fold(0_u64, |bits, (value, length)| (bits << length) | value);
    bits.to_be_bytes()[2..].try_into().unwrap()
}

#[test]
fn opcr_escr_and_seamless_splice_clock_fields_share_the_epoch_offset() {
    let mut writer = MpegTsEpochRewriter::default();
    seed(&mut writer, RATE, 27_000_000, false);
    writer.finish_epoch().unwrap();
    let next = seed(&mut writer, RATE, 0, false);
    let offset = decode_pcr(&next[2][6..12]);
    let mut adaptation = clock(27_000, 0);
    adaptation[5] = 0x19; // PCR, OPCR, adaptation extension.
    adaptation[12..18].copy_from_slice(&pcr_bytes(20_123));
    adaptation[18] = 6;
    adaptation[19] = 0x3f; // seamless splice, reserved bits.
    adaptation[20..25].copy_from_slice(&timestamp(100, 7));
    writer.push_packet(adaptation).unwrap();
    let result = drain(&mut writer)[0];
    assert_eq!(decode_pcr(&result[6..12]), 27_000 + offset);
    assert_eq!(decode_pcr(&result[12..18]), 20_123 + offset);
    assert_eq!(&result[20..25], &timestamp(100 + offset / 300, 7));

    let mut header = pes(500, None);
    header[7] |= 0x20;
    header[8] += 6;
    header.extend_from_slice(&escr(33_127));
    writer.push_packet(packet(0x100, 0, true, &header)).unwrap();
    let result = drain(&mut writer)[0];
    assert_eq!(&payload(&result)[14..20], &escr(33_127 + offset));
    writer.finish_epoch().unwrap();
}

#[test]
fn maximum_optional_pes_header_is_bounded_and_preserves_stuffing() {
    let mut writer = MpegTsEpochRewriter::default();
    seed(&mut writer, RATE, 0, false);
    let mut header = pes(100, Some(90));
    header[8] = 255;
    header.resize(264, 0xff);
    writer
        .push_packet(packet(0x100, 0, true, &header[..180]))
        .unwrap();
    assert!(drain(&mut writer).is_empty());
    writer
        .push_packet(packet(0x100, 1, false, &header[180..]))
        .unwrap();
    let result = drain(&mut writer);
    assert_eq!([payload(&result[0]), payload(&result[1])].concat(), header);
    writer.finish_epoch().unwrap();
}

#[test]
fn native_dirac_stream_id_extension_and_private_fields_survive_every_split() {
    // FFmpeg 9's VC-2 MPEG-TS output uses stream_id=0xfd and extension2=0x60.
    for extension in [
        vec![0x01, 0x81, 0x60],
        [
            vec![0xb1],
            vec![0x55; 16],
            vec![0x82, 0x80, 0x40, 0x10, 0x81, 0x71],
        ]
        .concat(),
    ] {
        let mut header = pes(126_000, None);
        header[3] = 0xfd;
        header[7] |= 1;
        header[8] += u8::try_from(extension.len()).unwrap();
        header.extend_from_slice(&extension);
        header.extend_from_slice(&[0xaa, 0xbb, 0xcc]);
        for split in 1..header.len() - 3 {
            let mut writer = MpegTsEpochRewriter::default();
            seed(&mut writer, RATE, 0, false);
            writer
                .push_packet(packet(0x100, 0, true, &header[..split]))
                .unwrap();
            writer
                .push_packet(packet(0x100, 1, false, &header[split..]))
                .unwrap();
            let result = drain(&mut writer);
            assert_eq!([payload(&result[0]), payload(&result[1])].concat(), header);
            writer.finish_epoch().unwrap();
        }
    }
}

#[test]
fn extension_tref_uses_the_shared_offset_and_rejects_invalid_extent() {
    for valid in [true, false] {
        let mut writer = MpegTsEpochRewriter::default();
        seed(&mut writer, RATE, 27_000_000, false);
        writer.finish_epoch().unwrap();
        let next = seed(&mut writer, RATE, 0, false);
        let offset = decode_pcr(&next[2][6..12]) / 300;
        let mut header = pes(500, None);
        header[7] |= 1;
        header[8] += 8;
        header.extend_from_slice(&[1, if valid { 0x86 } else { 0x85 }, 0xfe]);
        header.extend_from_slice(&timestamp(WRAP - 1, 0xf));
        let result = writer.push_packet(packet(0x100, 0, true, &header));
        if valid {
            result.unwrap();
            let result = drain(&mut writer)[0];
            assert_eq!(
                decode_timestamp(&payload(&result)[17..22]),
                (WRAP - 1 + offset) % WRAP
            );
            assert_eq!(&payload(&result)[14..17], &[1, 0x86, 0xfe]);
            writer.finish_epoch().unwrap();
        } else {
            assert_eq!(result, Err(MpegTsEpochError::Pes));
            assert!(drain(&mut writer).is_empty());
        }
    }
}

#[test]
fn embedded_pack_clock_and_conflicting_duplicate_header_are_rejected() {
    let mut writer = MpegTsEpochRewriter::default();
    seed(&mut writer, RATE, 0, false);
    let mut header = pes(50, None);
    header[7] |= 1;
    header[8] += 1;
    header.push(0x40);
    assert_eq!(
        writer.push_packet(packet(0x100, 0, true, &header)),
        Err(MpegTsEpochError::UnsupportedPes)
    );
    let mut writer = MpegTsEpochRewriter::default();
    seed(&mut writer, RATE, 0, false);
    let header = pes(50, None);
    writer.push_packet(packet(0x100, 0, true, &header)).unwrap();
    drain(&mut writer);
    assert_eq!(
        writer.push_packet(packet(0x100, 0, false, &header)),
        Err(MpegTsEpochError::Transport)
    );
}

#[test]
fn duplicated_initial_discontinuity_is_cleared_without_masking_a_later_reset() {
    let mut writer = MpegTsEpochRewriter::default();
    seed(&mut writer, RATE, 0, true);
    let mut initial = packet(0x101, 0, true, &pes(50, None));
    initial[5] |= 0x80;
    writer.push_packet(initial).unwrap();
    writer.push_packet(initial).unwrap();
    let result = drain(&mut writer);
    assert_eq!(result[0], result[1]);
    assert_eq!(result[0][5] & 0x80, 0);
    writer.push_packet(initial).unwrap();
    assert_eq!(drain(&mut writer)[0], result[0]);
    let subsequent = packet(0x101, 1, true, &pes(70, None));
    writer.push_packet(subsequent).unwrap();
    drain(&mut writer);
    let mut reset = subsequent;
    reset[5] |= 0x80;
    assert_eq!(writer.push_packet(reset), Err(MpegTsEpochError::Transport));
}
