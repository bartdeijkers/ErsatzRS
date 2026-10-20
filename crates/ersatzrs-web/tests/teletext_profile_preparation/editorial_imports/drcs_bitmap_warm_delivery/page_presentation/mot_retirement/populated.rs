//! Active MOT/DRCS links retire before unchanged basic pages on original responses.

use super::*;

pub(super) const DIRECTORY_ENV: &str = "ERSATZRS_TELETEXT_POPULATED_MOT_RETIREMENT_DIR";

pub(super) fn assert_models(
    old: &EncodedTeletextAuthoredService,
    next: &EncodedTeletextAuthoredService,
) {
    assert_eq!(
        old.declared().semantic().base(),
        next.declared().semantic().base()
    );
    assert_eq!(old.declared().semantic().base().pages.len(), 2);
    assert_eq!(old.declared().mots().len(), 1);
    assert!(!old.declared().semantic().drcs().subtables().is_empty());
    assert!(next.declared().mots().is_empty());
    assert!(next.declared().semantic().drcs().subtables().is_empty());
    assert!(next
        .pages()
        .iter()
        .all(|page| page.resource_header().is_none()));
}

pub(super) fn assert_prefix(capture: &Capture, after: usize, next: &Pattern, directory: &Path) {
    let observed = capture.ordered_units_after(after, MAX_UNITS);
    let successor = observed
        .windows(next.len())
        .position(|window| {
            window
                .iter()
                .zip(next)
                .all(|((_, unit), aliases)| aliases.contains(unit))
        })
        .unwrap();
    let clearing = |unit: &[u8; 46]| {
        matches!(observe_dvb_teletext_header(unit).unwrap(),
        TeletextHeaderObservation::Header(header) if header.packet_address.wire_magazine == 1
        && header.wire_page == 0xfe && header.subcode == 0x1520)
    };
    let start = observed
        .iter()
        .position(|(_, unit)| clearing(unit))
        .unwrap();
    assert!(
        start + 20 <= successor,
        "full clearing precedes the first exact successor cycle"
    );
    assert_eq!(
        observed.iter().filter(|(_, unit)| clearing(unit)).count(),
        1
    );
    let units: Vec<_> = observed[start..start + 20]
        .iter()
        .map(|(_, unit)| *unit)
        .collect();
    let TeletextHeaderObservation::Header(header) = observe_dvb_teletext_header(&units[0]).unwrap()
    else {
        panic!("full neutral MOT header required")
    };
    assert!(header.controls.erase_page());
    assert_ne!(header.controls.raw() & 16, 0, "full changed update sets C8");
    for (unit, row) in units[1..19].iter().zip([
        28, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 19, 20, 21,
    ]) {
        let TeletextHeaderObservation::NotHeader(address) =
            observe_dvb_teletext_header(unit).unwrap()
        else {
            panic!("complete neutral MOT body required")
        };
        assert_eq!((address.wire_magazine, address.packet_number), (1, row));
        if row == 28 {
            continue;
        }
        assert_neutral_body(unit, row);
    }
    assert!(matches!(
        observe_dvb_teletext_header(&units[19]).unwrap(),
        TeletextHeaderObservation::Terminator(_)
    ));
    write_units(
        directory,
        "retirement-prefix-units.json",
        &ObservedCycle {
            end: observed[start + 19].0,
            units,
        },
    );
}

fn assert_neutral_body(unit: &[u8; 46], row: u8) {
    let mut nibbles = [0_usize; 40];
    let offsets: &[usize] = if matches!(row, 19 | 20) {
        &[0, 10, 20, 30]
    } else if row == 21 {
        &[0, 4, 8, 12, 16, 20, 24, 28]
    } else {
        &[]
    };
    for &offset in offsets {
        nibbles[offset] = 1;
        nibbles[offset + 1] = 15;
        nibbles[offset + 2] = 15;
    }
    let codes = [
        0x15_u8, 0x02, 0x49, 0x5e, 0x64, 0x73, 0x38, 0x2f, 0xd0, 0xc7, 0x8c, 0x9b, 0xa1, 0xb6,
        0xfd, 0xea,
    ];
    assert_eq!(
        &unit[6..],
        &nibbles.map(|value| codes[value].reverse_bits()),
        "neutral MOT row {row}"
    );
}

#[tokio::test]
#[ignore = "requires FFmpeg9/TSDuck and a create-new populated-MOT ordinary publication directory"]
async fn ordinary_two_channel_publication_retires_populated_mot_on_original_responses() {
    run_case(true).await;
}
