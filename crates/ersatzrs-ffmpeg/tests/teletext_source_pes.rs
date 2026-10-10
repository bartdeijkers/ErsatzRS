use ersatzrs_ffmpeg::teletext::{
    source_header::{observe_dvb_teletext_header, TeletextHeaderObservation},
    source_pes::{SuppliedTeletextPes, TeletextPesError},
};

// The complete first PES from the independent Python calibration producer:
// scripts/calibration/generate-teletext-fixture.py::generate_fixture.
// Original synthetic page 200, PTS 90000, then two length-coded stuffing units.
static CALIBRATION_PES: [u8; 184] = [
    0, 0, 1, 189, 0, 178, 132, 128, 36, 33, 0, 5, 191, 33, 255, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 16, 2, 44, 224, 228, 146, 168, 168, 168, 168, 11, 168, 168, 168, 64, 162,
    74, 203, 131, 42, 91, 74, 203, 4, 203, 155, 115, 42, 19, 162, 42, 146, 194, 4, 42, 162, 203,
    42, 4, 4, 4, 4, 4, 4, 4, 4, 4, 255, 44, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 44, 255, 255, 255,
    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 255,
];

#[test]
fn complete_supplied_pes_feeds_independent_header_without_page_readiness() {
    let pes = SuppliedTeletextPes::parse(&CALIBRATION_PES).unwrap();
    assert_eq!(pes.bytes(), &CALIBRATION_PES);
    assert_eq!(pes.bytes().as_ptr(), CALIBRATION_PES.as_ptr());
    assert_eq!(pes.data_identifier(), 0x10);
    assert_eq!(pes.pts().unwrap().ticks(), 90_000);
    let mut units = pes.units();
    let first = units.next().unwrap();
    assert_eq!(first.id(), 2);
    assert_eq!(first.length(), 44);
    assert_eq!(first.bytes().as_ptr(), CALIBRATION_PES[46..].as_ptr());
    let header_unit = first.teletext_unit().unwrap();
    let TeletextHeaderObservation::Header(header) =
        observe_dvb_teletext_header(header_unit).unwrap()
    else {
        panic!("the complete supplied calibration PES must contain a page header");
    };
    assert_eq!(header.display_page().unwrap().get(), 200);
    assert_eq!(header.subcode, 0);
    assert_eq!(header.controls.raw(), 129);
    assert_eq!(header.corrected_header_bytes, 0);
    for unit in units {
        assert_eq!(unit.id(), 255);
        assert_eq!(unit.length(), 44);
        assert!(unit.teletext_unit().is_none());
        assert!(unit.payload().iter().all(|byte| *byte == 255));
    }
    assert_eq!(pes.units().count(), 3);
}

#[test]
fn malformed_final_unit_rejects_the_entire_pes_before_exposing_valid_header() {
    let mut malformed = CALIBRATION_PES;
    // The third unit claims one byte beyond the exact declared PES end. Its
    // preceding independent page header remains byte-for-byte intact.
    malformed[139] = 45;
    assert_eq!(&malformed[46..92], &CALIBRATION_PES[46..92]);
    assert!(matches!(
        SuppliedTeletextPes::parse(&malformed),
        Err(TeletextPesError::UnitBoundary)
    ));
}
