use super::fields::find_field_markers;
use super::import::{
    hamming_8_4, import_teletext_element_file, ElementImportError, ElementImportWarning,
};
use super::model::ElementPage;
use super::*;
use ersatzrs_domain::TeletextElementSourceFormat;

const HAMMING: [u8; 16] = [
    0x15, 0x02, 0x49, 0x5e, 0x64, 0x73, 0x38, 0x2f, 0xd0, 0xc7, 0x8c, 0x9b, 0xa1, 0xb6, 0xfd, 0xea,
];

fn row_text(text: &[u8]) -> [u8; 40] {
    let mut row = [0x20; 40];
    row[..text.len()].copy_from_slice(text);
    row
}

/// A QTeletextMaker-style TTI with trimmed rows, ESC control codes,
/// high-bit control codes, an editor-private record and two subpages.
fn sample_tti() -> Vec<u8> {
    let mut file = Vec::new();
    file.extend_from_slice(
        b"DE,Welkom\r\nDS,inserter\r\nPN,10001\r\nSC,0001\r\nPS,8000\r\nCT,8,T\r\n",
    );
    file.extend_from_slice(b"OL,1,\x1bD\x1b]\x1bMWELKOM\r\n");
    file.extend_from_slice(b"OL,4,\x86Kanaal: \x83%KANAAL%%%%%%%\r\n");
    file.extend_from_slice(b"FL,101,200,300,400,8FF,100\r\n");
    file.extend_from_slice(b"PN,10002\r\nSC,0002\r\nPS,8000\r\nOL,2,Tweede\r\n");
    file.extend_from_slice(b"PN,20000\r\nOL,1,Other page\r\n");
    file
}

#[test]
fn tti_import_is_tolerant_and_round_trips_through_canonical_tti() {
    let imported = import_teletext_element_file("welkom.tti", &sample_tti()).expect("import");
    assert_eq!(imported.format, TeletextElementSourceFormat::Tti);
    assert_eq!(imported.page.page, 0x100);
    assert_eq!(imported.page.description, "Welkom");
    assert_eq!(imported.page.subpages.len(), 2);
    assert!(imported
        .warnings
        .contains(&ElementImportWarning::ExtraPagesIgnored));
    let first = &imported.page.subpages[0];
    assert_eq!(first.rows[1], row_text(b"\x04\x1d\x0dWELKOM"));
    assert_eq!(&first.rows[4][..9], b"\x06Kanaal: ");
    assert_eq!(
        first.fast_links,
        Some([0x101, 0x200, 0x300, 0x400, 0x8ff, 0x100])
    );
    let markers = find_field_markers(&imported.page);
    assert_eq!(markers.len(), 1);
    assert_eq!(markers[0].name, "KANAAL");

    let canonical = imported.page.to_tti().expect("canonical");
    let restored = ElementPage::from_canonical_tti(&canonical).expect("restore");
    assert_eq!(restored, imported.page);
    // Canonical bytes are stable.
    assert_eq!(restored.to_tti().expect("again"), canonical);
}

#[test]
fn unsupported_and_empty_files_are_rejected_with_static_errors() {
    assert_eq!(
        import_teletext_element_file("page.png", b"x"),
        Err(ElementImportError::UnsupportedFormat)
    );
    assert_eq!(
        import_teletext_element_file("page.tti", b""),
        Err(ElementImportError::Empty)
    );
    assert_eq!(
        import_teletext_element_file("page.tti", b"DE,nothing\r\n"),
        Err(ElementImportError::NoPage)
    );
}

fn mrag(magazine: u8, packet: u8) -> [u8; 2] {
    let first = (magazine & 7) | ((packet & 1) << 3);
    [
        HAMMING[usize::from(first)],
        HAMMING[usize::from(packet >> 1)],
    ]
}

fn t42_header(magazine: u8, page: u8, national: u8) -> Vec<u8> {
    let mut packet = mrag(magazine, 0).to_vec();
    let c12_c14 = ((national & 1) << 3) | ((national & 2) << 1) | ((national & 4) >> 1);
    for nibble in [page & 0xf, page >> 4, 0, 0, 0, 0, 0, c12_c14] {
        packet.push(HAMMING[usize::from(nibble)]);
    }
    let mut header = [0x20_u8; 32];
    header[..8].copy_from_slice(b"ErsatzRS");
    packet.extend_from_slice(&header);
    packet
}

fn t42_row(magazine: u8, row: u8, text: &[u8]) -> Vec<u8> {
    let mut packet = mrag(magazine, row).to_vec();
    packet.extend_from_slice(&row_text(text));
    packet
}

fn sample_t42() -> Vec<u8> {
    let mut file = Vec::new();
    file.extend(t42_header(1, 0x23, 1));
    file.extend(t42_row(1, 1, b"\x01Hello T42"));
    file.extend(t42_row(1, 26, b"enhancement"));
    file.extend(t42_header(1, 0x24, 0));
    file
}

#[test]
fn hamming_decoding_corrects_single_bit_errors() {
    for (value, code) in (0_u8..).zip(HAMMING.iter()) {
        assert_eq!(hamming_8_4(*code), Some(value));
        assert_eq!(hamming_8_4(code ^ 0x10), Some(value));
    }
    assert_eq!(hamming_8_4(0x15 ^ 0x03), None);
}

#[test]
fn t42_and_htt_import_level_one_rows_header_and_national_option() {
    let imported = import_teletext_element_file("p123.t42", &sample_t42()).expect("t42");
    assert_eq!(imported.page.page, 0x123);
    let subpage = &imported.page.subpages[0];
    assert_eq!(subpage.rows[1], row_text(b"\x01Hello T42"));
    assert_eq!(&subpage.rows[0][8..16], b"ErsatzRS");
    assert_eq!(subpage.national_option(), 1);
    assert!(imported
        .warnings
        .contains(&ElementImportWarning::EnhancementsNotImported));
    assert!(imported
        .warnings
        .contains(&ElementImportWarning::ExtraPagesIgnored));
    assert_eq!(imported.page.description, "p123");

    let mut htt = Vec::new();
    for packet in sample_t42().chunks_exact(42) {
        htt.extend_from_slice(&[0xaa, 0xaa, 0xe4]);
        htt.extend(packet.iter().map(|byte| byte.reverse_bits()));
    }
    let from_htt = import_teletext_element_file("p123.htt", &htt).expect("htt");
    assert_eq!(from_htt.format, TeletextElementSourceFormat::Htt);
    assert_eq!(from_htt.page, imported.page);
    assert!(import_teletext_element_file("bad.htt", &[0_u8; 45]).is_err());
}

#[test]
fn ep1_import_reads_rows_and_language() {
    let mut file = vec![0xfe, 0x01, 0x0d, 0x00, 0x00, 0x00];
    for row in 0..24_u8 {
        let mut cells = [0x20_u8; 40];
        if row == 3 {
            cells[..6].copy_from_slice(b"\x02EP1 !");
        }
        file.extend_from_slice(&cells);
    }
    file.extend_from_slice(&[0, 0]);
    let imported = import_teletext_element_file("page.ep1", &file).expect("ep1");
    let subpage = &imported.page.subpages[0];
    assert_eq!(&subpage.rows[3][..6], b"\x02EP1 !");
    assert!(subpage.present[3]);
    assert!(!subpage.present[4]);
    assert_eq!(subpage.national_option(), 1);
    assert!(import_teletext_element_file("page.ep1", &file[..100]).is_err());
}

#[test]
fn error_codes_are_stable_for_redirects() {
    assert_eq!(TeletextElementError::Stale.code(), "stale");
    assert_eq!(
        TeletextElementError::Import(ElementImportError::TooLarge).code(),
        "too-large"
    );
    assert_eq!(unique_name("Welkom", &["Welkom".to_owned()]), "Welkom (2)");
}
