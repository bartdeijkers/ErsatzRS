//! Fixed wire vectors and combinations for the optional-header framing boundary.

use super::*;

const PTS: [u8; 5] = [0x31, 0, 5, 0xbf, 0x21]; // 90,000 with DTS present.
const DTS: [u8; 5] = [0x11, 0, 1, 0, 1];
const ESCR: [u8; 6] = [0xc4, 0, 4, 0, 4, 1];
const RATE: [u8; 3] = [0x80, 1, 1];
const PACK: [u8; 14] = [0, 0, 1, 0xba, 0x44, 0, 4, 0, 4, 1, 0, 1, 3, 0xf8];

fn packet(flags: u8, fields: &[u8]) -> Vec<u8> {
    assert!(fields.len() <= 36);
    let mut packet = fixture();
    packet[7] = flags;
    packet[9..45].fill(0xff);
    packet[9..9 + fields.len()].copy_from_slice(fields);
    packet
}

fn assert_preserved(flags: u8, fields: &[u8], pts: Option<u64>) {
    let bytes = packet(flags, fields);
    let parsed = SuppliedTeletextPes::parse(&bytes).unwrap();
    assert_eq!(parsed.pts().map(TeletextTimestamp::ticks), pts);
    assert_eq!(parsed.bytes(), bytes);
    assert!(std::ptr::eq(parsed.bytes().as_ptr(), bytes.as_ptr()));
    assert_eq!(parsed.units().count(), 3);
    assert_eq!(&bytes[45..], &fixture()[45..]);
    for index in 9 + fields.len()..45 {
        let mut damaged = bytes.clone();
        damaged[index] = 0xfe;
        assert_eq!(
            SuppliedTeletextPes::parse(&damaged),
            Err(TeletextPesError::HeaderStuffing)
        );
    }
}

#[test]
fn all_fitting_flag_combinations_preserve_pts_absence_and_exact_payload() {
    for flags in 0_u8..=255 {
        if flags >> 6 == 1 {
            assert_eq!(
                SuppliedTeletextPes::parse(&packet(flags, &[])),
                Err(TeletextPesError::Timestamp)
            );
            continue;
        }
        let mut fields = Vec::new();
        match flags >> 6 {
            2 => fields.extend([0x21, 0, 5, 0xbf, 0x21]),
            3 => {
                fields.extend(PTS);
                fields.extend(DTS);
            }
            _ => {}
        }
        for (flag, wire) in [
            (0x20, ESCR.as_slice()),
            (0x10, RATE.as_slice()),
            (8, [0x07].as_slice()), // Fast-forward, all frequencies.
            (4, [0x80].as_slice()),
            (2, [0x12, 0x34].as_slice()),
            (1, [0x0e].as_slice()), // Empty extension; reserved bits set.
        ] {
            if flags & flag != 0 {
                fields.extend(wire);
            }
        }
        let pts = (flags & 0x80 != 0).then_some(90_000);
        assert_preserved(flags, &fields, pts);
    }
}

#[test]
fn extension_fields_fit_independently_and_together_without_inventing_pts() {
    // Private data, sequence counter, P-STD and TREF, with reserved tail.
    let mut fields = vec![0xbf];
    fields.extend([0x55; 16]);
    fields.extend([0x85, 0xc0, 0x60, 1, 0x88, 0xfe]);
    fields.extend([0xf1, 0, 1, 0, 1]);
    fields.extend([0xff; 2]);
    assert_preserved(1, &fields, None);
    let mut with_pts = ZERO_PTS.to_vec();
    with_pts.extend(&fields);
    assert_preserved(0x81, &with_pts, Some(0));
    let mut pack = vec![0x4e, 14];
    pack.extend(PACK);
    assert_preserved(1, &pack, None);
    assert_preserved(1, &[0x0f, 0x81, 0xff], None); // No TREF.
}

#[test]
fn exact_header_fill_and_extension_lengths_cannot_escape_into_units() {
    let mut fields = vec![0x8f];
    fields.extend([0x55; 16]);
    fields.extend([0x92, 0xff]);
    fields.extend([0xff; 17]);
    assert_eq!(fields.len(), 36);
    assert_preserved(1, &fields, None);
    for length in 19..=127 {
        let mut oversized = fields.clone();
        oversized[17] = 0x80 | length;
        assert_eq!(
            SuppliedTeletextPes::parse(&packet(1, &oversized)),
            Err(TeletextPesError::OptionalHeader)
        );
    }
    // Valid field encodings whose combined widths exceed the fixed header.
    assert_eq!(
        SuppliedTeletextPes::parse(&packet(0xff, &[0xff; 36])),
        Err(TeletextPesError::Timestamp)
    );
    let mut oversized = PTS.to_vec();
    oversized.extend(DTS);
    oversized.push(0x8f);
    oversized.extend([0x55; 16]);
    oversized.extend([0x89, 0xfe, 0xf1, 0, 1, 0, 1]);
    assert_eq!(
        SuppliedTeletextPes::parse(&packet(0xc1, &oversized)),
        Err(TeletextPesError::OptionalHeader)
    );
}

#[test]
fn pts_dts_and_tref_keep_exact_prefixes_and_each_timestamp_marker() {
    for (flags, fields, starts, prefixes) in [
        (
            0xc0,
            [PTS.as_slice(), DTS.as_slice()].concat(),
            vec![0, 5],
            vec![3, 1],
        ),
        (
            1,
            vec![0x0f, 0x86, 0xfe, 0xf1, 0, 1, 0, 1],
            vec![3],
            vec![15],
        ),
    ] {
        for (start, expected) in starts.into_iter().zip(prefixes) {
            for prefix in 0..16 {
                let mut changed = fields.clone();
                changed[start] = (prefix << 4) | (changed[start] & 0x0f);
                if prefix == expected {
                    assert!(SuppliedTeletextPes::parse(&packet(flags, &changed)).is_ok());
                } else {
                    assert_eq!(
                        SuppliedTeletextPes::parse(&packet(flags, &changed)),
                        Err(TeletextPesError::Timestamp)
                    );
                }
            }
            for offset in [0, 2, 4] {
                let mut changed = fields.clone();
                changed[start + offset] &= 0xfe;
                assert_eq!(
                    SuppliedTeletextPes::parse(&packet(flags, &changed)),
                    Err(TeletextPesError::Timestamp)
                );
            }
        }
    }
}

#[test]
fn optional_field_markers_and_extension_prefixes_remain_required() {
    for (flags, fields, markers) in [
        (0x20, ESCR.to_vec(), vec![(0, 4), (2, 4), (4, 4), (5, 1)]),
        (0x10, RATE.to_vec(), vec![(0, 0x80), (2, 1)]),
        (4, vec![0x80], vec![(0, 0x80)]),
        (
            1,
            vec![0x2e, 0x80, 0xc0],
            vec![(0, 2), (0, 4), (0, 8), (1, 0x80), (2, 0x80)],
        ),
        (
            1,
            vec![0x0f, 0x81, 0xff],
            vec![(1, 0x80), (2, 0x80), (2, 2)],
        ),
    ] {
        for (index, mask) in markers {
            let mut changed = fields.clone();
            changed[index] &= !mask;
            assert_eq!(
                SuppliedTeletextPes::parse(&packet(flags, &changed)),
                Err(TeletextPesError::OptionalHeader)
            );
        }
    }
    for prefix in [0, 0x80, 0xc0] {
        assert_eq!(
            SuppliedTeletextPes::parse(&packet(1, &[0x1e, prefix, 0])),
            Err(TeletextPesError::OptionalHeader)
        );
    }
    for fields in [
        vec![0x4e, 255],              // Pack extends beyond the optional-header area.
        vec![0x0f, 0x80],             // Zero-length extension 2 lacks its flags byte.
        vec![0x0f, 0x81, 0xfe],       // TREF cannot borrow following PES stuffing.
        vec![0x0f, 0x82, 0xff, 0xfe], // Reserved extension tail.
        vec![0x0f, 0x81, 0x01],       // Stream-ID extension is illegal with BD.
    ] {
        assert_eq!(
            SuppliedTeletextPes::parse(&packet(1, &fields)),
            Err(TeletextPesError::OptionalHeader)
        );
    }
}

#[test]
fn escr_keeps_reserved_bits_and_modulo_300_extension_without_becoming_pts() {
    for extension in 0_u16..512 {
        let mut escr = ESCR;
        escr[4] |= u8::try_from(extension >> 7).unwrap();
        escr[5] |= u8::try_from((extension & 127) << 1).unwrap();
        let bytes = packet(0x20, &escr);
        if extension < 300 {
            assert_eq!(SuppliedTeletextPes::parse(&bytes).unwrap().pts(), None);
        } else {
            assert_eq!(
                SuppliedTeletextPes::parse(&bytes),
                Err(TeletextPesError::OptionalHeader)
            );
        }
    }
    for reserved in [0, 0x40, 0x80] {
        let mut escr = ESCR;
        escr[0] = (escr[0] & 0x3f) | reserved;
        assert_eq!(
            SuppliedTeletextPes::parse(&packet(0x20, &escr)),
            Err(TeletextPesError::OptionalHeader)
        );
    }
}
