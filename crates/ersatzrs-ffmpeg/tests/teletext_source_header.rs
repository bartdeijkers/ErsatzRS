use ersatzrs_ffmpeg::teletext::source_header::{
    observe_dvb_teletext_header, TeletextDataUnitKind, TeletextHeaderObservation,
};

// Exact page 200/201/888 units from the existing independent Python producer:
// scripts/calibration/generate-teletext-fixture.py::data_unit/page_header.
// These original synthetic headers contain no operator or NOS content.
const CALIBRATION_HEADERS: [[u8; 46]; 3] = [
    [
        2, 44, 224, 228, 146, 168, 168, 168, 168, 11, 168, 168, 168, 64, 162, 74, 203, 131, 42, 91,
        74, 203, 4, 203, 155, 115, 42, 19, 162, 42, 146, 194, 4, 42, 162, 203, 42, 4, 4, 4, 4, 4,
        4, 4, 4, 4,
    ],
    [
        2, 44, 224, 228, 146, 168, 64, 168, 168, 11, 168, 168, 168, 64, 162, 74, 203, 131, 42, 91,
        74, 203, 4, 203, 155, 115, 42, 19, 162, 42, 146, 194, 4, 42, 162, 203, 42, 4, 4, 4, 4, 4,
        4, 4, 4, 4,
    ],
    [
        3, 44, 224, 228, 168, 168, 11, 11, 168, 11, 168, 11, 64, 64, 162, 74, 203, 131, 42, 91, 74,
        203, 4, 203, 155, 115, 42, 19, 162, 42, 146, 194, 4, 42, 162, 203, 42, 4, 4, 4, 4, 4, 4, 4,
        4, 4,
    ],
];

#[test]
fn supplied_headers_match_independent_calibration_units_without_page_readiness() {
    for (unit, (page, kind, controls)) in CALIBRATION_HEADERS.into_iter().zip([
        (200, TeletextDataUnitKind::NonSubtitle, 129),
        (201, TeletextDataUnitKind::NonSubtitle, 129),
        (888, TeletextDataUnitKind::Subtitle, 141),
    ]) {
        let TeletextHeaderObservation::Header(header) = observe_dvb_teletext_header(&unit).unwrap()
        else {
            panic!("the supplied calibration unit must contain a header");
        };
        assert_eq!(header.display_page().unwrap().get(), page);
        assert_eq!(header.packet_address.data_unit_kind, kind);
        assert_eq!(header.packet_address.packet_number, 0);
        assert!(header.packet_address.field_parity);
        assert_eq!(header.packet_address.line_offset, 0);
        assert_eq!(header.packet_address.corrected_address_bytes, 0);
        assert_eq!(header.subcode, 0);
        assert_eq!(header.controls.raw(), controls);
        assert_eq!(header.controls.c12_c14_bits(), 0);
        assert_eq!(header.corrected_header_bytes, 0);

        // Header text is outside this metadata-only observation. Damaged text
        // must neither invent language/charset nor imply a complete page body.
        let mut damaged_text = unit;
        damaged_text[14..].fill(0);
        assert_eq!(
            observe_dvb_teletext_header(&damaged_text).unwrap(),
            TeletextHeaderObservation::Header(header)
        );
    }
}
