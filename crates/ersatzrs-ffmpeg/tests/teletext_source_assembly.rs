use ersatzrs_ffmpeg::teletext::{
    source_assembly::{SuppliedTeletextAssembler, TeletextAssemblyError, TeletextAssemblyEvent},
    source_header::{observe_dvb_teletext_header, TeletextDataUnitKind, TeletextHeaderObservation},
    source_pes::TeletextPesError,
};

// Exact first cycle from the independent original Python calibration producer:
// scripts/calibration/generate-teletext-fixture.py::generate_fixture()[..3008].
// It includes PAT/PMT packets, adaptation-only clocks, four synthetic headers,
// three body PES packets and stuffing. No operator or NOS content is present.
fn calibration_packets() -> Vec<[u8; 188]> {
    let bytes = include_str!("fixtures/teletext-source-assembly.hex")
        .split_whitespace()
        .map(|byte| u8::from_str_radix(byte, 16).unwrap())
        .collect::<Vec<_>>();
    let chunks = bytes.chunks_exact(188);
    assert!(chunks.remainder().is_empty());
    assert_eq!(bytes.len(), 3008);
    chunks.map(|packet| packet.try_into().unwrap()).collect()
}

#[test]
fn supplied_ts_cycle_assembles_only_complete_pes_and_observes_independent_headers() {
    let mut assembler = SuppliedTeletextAssembler::new(0x100).unwrap();
    let mut headers = Vec::new();
    let (mut ignored, mut no_payload, mut complete) = (0, 0, 0);
    for packet in calibration_packets() {
        match assembler.push(&packet).unwrap() {
            TeletextAssemblyEvent::IgnoredPid => ignored += 1,
            TeletextAssemblyEvent::NoPayload => no_payload += 1,
            TeletextAssemblyEvent::Complete(pes) => {
                complete += 1;
                assert_eq!(pes.bytes(), &packet[4..]);
                for unit in pes.units().filter_map(|unit| unit.teletext_unit()) {
                    match observe_dvb_teletext_header(unit).unwrap() {
                        TeletextHeaderObservation::Header(header) => {
                            headers.push((
                                header.display_page().unwrap().get(),
                                pes.pts().unwrap().ticks(),
                                header.packet_address.data_unit_kind,
                                header.controls.raw(),
                            ));
                        }
                        TeletextHeaderObservation::NotHeader(_) => {}
                        other => panic!("unexpected calibration header observation: {other:?}"),
                    }
                }
            }
            other => panic!("unexpected calibration assembly event: {other:?}"),
        }
    }
    assert_eq!((ignored, no_payload, complete), (2, 7, 7));
    assert_eq!(
        headers,
        [
            (200, 90_000, TeletextDataUnitKind::NonSubtitle, 129),
            (201, 108_000, TeletextDataUnitKind::NonSubtitle, 129),
            (888, 126_000, TeletextDataUnitKind::Subtitle, 141),
            (899, 162_000, TeletextDataUnitKind::NonSubtitle, 129),
        ]
    );
    // A clean finite boundary proves framing for this supplied fixture only.
    // These observations still provide no acquired body/charset or inventory.
    assembler.finish().unwrap();
}

#[test]
fn malformed_calibration_pes_fences_the_supplied_epoch_without_partial_headers() {
    let mut packets = calibration_packets();
    packets[3][4 + 139] = 45;
    let mut assembler = SuppliedTeletextAssembler::new(0x100).unwrap();
    for packet in &packets[..3] {
        assembler.push(packet).unwrap();
    }
    let expected = TeletextAssemblyError::Pes(TeletextPesError::UnitBoundary);
    assert_eq!(assembler.push(&packets[3]).unwrap_err(), expected);
    for packet in &packets[4..] {
        assert_eq!(assembler.push(packet).unwrap_err(), expected);
    }
    assert_eq!(assembler.finish(), Err(expected));
}
