use ersatzrs_ffmpeg::teletext::{
    source_observation::{SuppliedTeletextHeaderObserver, SuppliedTeletextHeaderWindow},
    source_psi::{ObservedSourcePsi, SourceTeletextPsiObserver},
};

fn observe(bytes: &[u8], pid: u16) -> (ObservedSourcePsi, SuppliedTeletextHeaderWindow) {
    let mut psi = SourceTeletextPsiObserver::new(pid).unwrap();
    let mut pages = SuppliedTeletextHeaderObserver::new(pid).unwrap();
    let packets = bytes.chunks_exact(188);
    assert!(packets.remainder().is_empty());
    for packet in packets {
        let packet = packet.try_into().unwrap();
        psi.push(packet).unwrap();
        pages.push(packet).unwrap();
    }
    (psi.observation().unwrap(), pages.finish().unwrap())
}

#[test]
fn independent_source_window_separates_wire_membership_descriptors_and_actual_headers() {
    let bytes = include_str!("fixtures/teletext-source-assembly.hex")
        .split_whitespace()
        .map(|value| u8::from_str_radix(value, 16).unwrap())
        .collect::<Vec<_>>();
    let (psi, headers) = observe(&bytes, 0x100);
    assert_eq!(psi.pat.table_id_extension, 1);
    assert_eq!(psi.pat.version, 0);
    assert!(psi.cat.is_none());
    let memberships: Vec<_> = psi.memberships().collect();
    assert_eq!(memberships.len(), 1);
    let (program, stream) = memberships[0];
    assert_eq!(
        (program.program_number, program.pmt_pid, program.pcr_pid),
        (1, 0x1000, 0x100)
    );
    assert_eq!((stream.pid, stream.stream_type), (0x100, 6));
    assert_eq!(
        stream.descriptors,
        [0x56, 10, b'e', b'n', b'g', 0x0a, 0, b'e', b'n', b'g', 0x10, 0x88]
    );
    assert_eq!(
        headers
            .headers()
            .iter()
            .map(|record| record.header.display_page().unwrap().get())
            .collect::<Vec<_>>(),
        [200, 201, 888, 899]
    );
    // Descriptors advertise200/888 while the actual window also observes201/899.
    // Neither collection is promoted to a complete page inventory or selection.
    let (absent, absent_headers) = observe(&bytes, 0x101);
    assert_eq!(absent.memberships().count(), 0);
    assert!(absent_headers.headers().is_empty());
}

#[test]
#[ignore = "requires the synthetic two-audio remux used by the Local scanner native proof"]
#[allow(
    clippy::disallowed_methods,
    reason = "Only this opt-in native fixture reads an explicitly provided synthetic input path"
)]
fn native_source_window_observes_real_remux_headers_and_exact_program_membership() {
    let path = std::env::var_os("ERSATZRS_TELETEXT_SOURCE_WINDOW_FIXTURE")
        .expect("set synthetic input fixture");
    let bytes = std::fs::read(path).unwrap();
    assert!(bytes.len() <= 188 * 32_768);
    let (psi, headers) = observe(&bytes, 0x103);
    let memberships: Vec<_> = psi.memberships().collect();
    assert_eq!(memberships.len(), 1);
    let (program, stream) = memberships[0];
    assert_eq!(
        (program.program_number, program.pmt_pid, program.pcr_pid),
        (1, 0x1000, 0x100)
    );
    assert_eq!((stream.pid, stream.stream_type), (0x103, 6));
    assert_eq!(
        stream.descriptors,
        [0x56, 10, b'e', b'n', b'g', 0x0a, 0, b'e', b'n', b'g', 0x10, 0x88]
    );
    assert_eq!(headers.packets(), bytes.len() / 188);
    assert_eq!(headers.complete_pes(), 14);
    assert_eq!(
        headers
            .headers()
            .iter()
            .map(|record| record.header.display_page().unwrap().get())
            .collect::<Vec<_>>(),
        [200, 201, 888, 899, 200, 201, 888, 899]
    );
    assert!(!headers.has_unobserved_prefix());
}
