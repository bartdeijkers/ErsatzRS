//! Tolerant importers for files made by teletext editors.
//!
//! Formats follow the conventions of `QTeletextMaker`'s loaders: TTI text
//! records, T42 raw 42-byte packets, HTT (T42 with clock run-in and reversed
//! bit order, 45 bytes per packet) and EP1 (six-byte header and 24 rows of 40
//! bytes). Only the first page of a multi-page file is kept; its subpages are
//! kept. Every lossy step is reported as a warning, never silently.

use ersatzrs_domain::{
    TeletextElementSourceFormat, MAX_TELETEXT_ELEMENT_SUBPAGES, MAX_TELETEXT_ELEMENT_UPLOAD_BYTES,
};

use super::model::{ElementPacket, ElementPage, ElementSubpage};
use crate::teletext_tti::TeletextTtiCycleUnit;

/// A non-fatal import note shown to the user after upload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ElementImportWarning {
    /// The file contained more than one page; only the first is kept.
    ExtraPagesIgnored,
    /// Unknown or malformed records/packets were skipped.
    UnreadableRecordsSkipped,
    /// Enhancement data (X/26-X/28) could not be imported from this format.
    EnhancementsNotImported,
    /// More subpages than the library keeps.
    SubpagesTruncated,
    /// The EP1 language code was unknown; English was assumed.
    UnknownLanguage,
}

/// Fatal import failure; carries no file content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ElementImportError {
    #[error("unsupported teletext file format")]
    UnsupportedFormat,
    #[error("teletext file is empty")]
    Empty,
    #[error("teletext file is too large")]
    TooLarge,
    #[error("teletext file contains no page")]
    NoPage,
    #[error("teletext file is damaged")]
    Damaged,
}

/// The imported page plus everything the user should know about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementImport {
    pub format: TeletextElementSourceFormat,
    pub page: ElementPage,
    pub warnings: Vec<ElementImportWarning>,
}

fn warn(warnings: &mut Vec<ElementImportWarning>, warning: ElementImportWarning) {
    if !warnings.contains(&warning) {
        warnings.push(warning);
    }
}

/// Import a file; the format is chosen from the file name extension.
/// # Errors
/// Returns a static failure for unsupported, empty, oversized or unreadable files.
pub fn import_teletext_element_file(
    file_name: &str,
    bytes: &[u8],
) -> Result<ElementImport, ElementImportError> {
    let format = TeletextElementSourceFormat::from_file_name(file_name)
        .ok_or(ElementImportError::UnsupportedFormat)?;
    if bytes.is_empty() {
        return Err(ElementImportError::Empty);
    }
    if bytes.len() > MAX_TELETEXT_ELEMENT_UPLOAD_BYTES {
        return Err(ElementImportError::TooLarge);
    }
    let mut warnings = Vec::new();
    let mut page = match format {
        TeletextElementSourceFormat::Tti => import_tti(bytes, &mut warnings)?,
        TeletextElementSourceFormat::T42 => {
            import_packets(bytes.chunks_exact(42).map(<[u8]>::to_vec), &mut warnings)?
        }
        TeletextElementSourceFormat::Htt => import_packets(htt_packets(bytes)?, &mut warnings)?,
        TeletextElementSourceFormat::Ep1 => import_ep1(bytes, &mut warnings)?,
    };
    if page.subpages.len() > MAX_TELETEXT_ELEMENT_SUBPAGES {
        page.subpages.truncate(MAX_TELETEXT_ELEMENT_SUBPAGES);
        warn(&mut warnings, ElementImportWarning::SubpagesTruncated);
    }
    if page.description.is_empty() {
        page.description = file_stem(file_name);
    }
    Ok(ElementImport {
        format,
        page,
        warnings,
    })
}

fn file_stem(file_name: &str) -> String {
    let base = file_name.rsplit(['/', '\\']).next().unwrap_or(file_name);
    base.rsplit_once('.')
        .map_or(base, |(stem, _)| stem)
        .chars()
        .filter(|character| !character.is_control())
        .take(64)
        .collect()
}

// ---------------------------------------------------------------- TTI

fn parse_hex(value: &[u8]) -> Option<u64> {
    std::str::from_utf8(value)
        .ok()
        .and_then(|text| u64::from_str_radix(text.trim(), 16).ok())
}

fn parse_decimal(value: &[u8]) -> Option<u64> {
    std::str::from_utf8(value)
        .ok()
        .and_then(|text| text.trim().parse().ok())
}

/// Decode an OL payload into 40 Level 1 cells, as editors write it: ESC
/// escapes control codes, high-bit bytes carry 7-bit values, short rows are
/// padded with spaces and long rows are cut.
fn decode_ol_row(payload: &[u8]) -> [u8; 40] {
    let mut cells = [0x20; 40];
    let mut input = 0;
    let mut output = 0;
    while input < payload.len() && output < 40 {
        let byte = payload[input];
        cells[output] = if byte == 0x1b {
            input += 1;
            payload.get(input).map_or(0x20, |next| next & 0x3f)
        } else if byte >= 0x80 {
            byte & 0x7f
        } else {
            byte
        };
        input += 1;
        output += 1;
    }
    cells
}

fn decode_ol_packet(payload: &[u8]) -> Option<(u8, [u32; 13])> {
    let designation = *payload.first()?;
    if !(0x40..=0x4f).contains(&designation) || payload.len() < 40 {
        return None;
    }
    let mut words = [0; 13];
    for (index, word) in words.iter_mut().enumerate() {
        for part in 0..3 {
            let byte = payload[1 + index * 3 + part] & 0x3f;
            *word |= u32::from(byte) << (part * 6);
        }
    }
    Some((designation - 0x40, words))
}

fn import_tti(
    bytes: &[u8],
    warnings: &mut Vec<ElementImportWarning>,
) -> Result<ElementPage, ElementImportError> {
    let mut page = ElementPage {
        description: String::new(),
        page: 0,
        subpages: Vec::new(),
    };
    let mut stopped = false;
    for raw in bytes.split(|&byte| byte == b'\n') {
        let line = raw.strip_suffix(b"\r").unwrap_or(raw);
        if line.is_empty() {
            continue;
        }
        if line.len() < 3 || line[2] != b',' {
            warn(warnings, ElementImportWarning::UnreadableRecordsSkipped);
            continue;
        }
        let (command, value) = (&line[..2], &line[3..]);
        if command == b"PN" {
            let number = value
                .get(..3)
                .and_then(parse_hex)
                .and_then(|number| u16::try_from(number).ok())
                .filter(|number| (0x100..=0x8ff).contains(number));
            let Some(number) = number else {
                warn(warnings, ElementImportWarning::UnreadableRecordsSkipped);
                continue;
            };
            if page.subpages.is_empty() {
                page.page = number;
            } else if number != page.page {
                warn(warnings, ElementImportWarning::ExtraPagesIgnored);
                stopped = true;
                break;
            }
            page.subpages.push(ElementSubpage::default());
            continue;
        }
        if command == b"DE" {
            if page.description.is_empty() {
                page.description = String::from_utf8_lossy(value)
                    .chars()
                    .filter(|character| !character.is_control())
                    .take(64)
                    .collect();
            }
            continue;
        }
        let Some(subpage) = page.subpages.last_mut() else {
            // Records before the first PN (e.g. DS, SP) carry no page content.
            continue;
        };
        if !apply_tti_line(subpage, command, value) {
            warn(warnings, ElementImportWarning::UnreadableRecordsSkipped);
        }
    }
    let _ = stopped;
    if page.subpages.is_empty()
        || page
            .subpages
            .iter()
            .all(|sp| !sp.present.iter().any(|&p| p))
    {
        return Err(ElementImportError::NoPage);
    }
    Ok(page)
}

fn apply_tti_line(subpage: &mut ElementSubpage, command: &[u8], value: &[u8]) -> bool {
    match command {
        b"SC" => parse_decimal(value)
            .or_else(|| parse_hex(value))
            .map(|value| subpage.subcode = value)
            .is_some(),
        b"PS" => parse_hex(value)
            .and_then(|value| u16::try_from(value).ok())
            .map(|value| subpage.page_status = value)
            .is_some(),
        b"RE" => parse_decimal(value)
            .filter(|value| *value <= 15)
            .map(|value| subpage.region = Some(value))
            .is_some(),
        b"CT" => {
            let text = String::from_utf8_lossy(value);
            let mut parts = text.split(',');
            let amount = parts
                .next()
                .and_then(|part| part.trim().parse::<u64>().ok());
            let unit = match parts.next().map(str::trim) {
                Some("C") => Some(TeletextTtiCycleUnit::Count),
                Some("T") => Some(TeletextTtiCycleUnit::Time),
                _ => None,
            };
            match (amount, unit) {
                (Some(amount), Some(unit)) if amount > 0 => {
                    subpage.cycle = Some((amount, unit));
                    true
                }
                _ => false,
            }
        }
        b"FL" => {
            let links: Vec<u16> = value
                .split(|&byte| byte == b',')
                .filter_map(|part| parse_hex(part).and_then(|link| u16::try_from(link).ok()))
                .filter(|link| *link <= 0xfff)
                .collect();
            if let Ok(links) = <[u16; 6]>::try_from(links) {
                subpage.fast_links = Some(links);
                true
            } else {
                false
            }
        }
        b"OL" => {
            let Some(separator) = value.iter().position(|&byte| byte == b',') else {
                return false;
            };
            let Some(row) = parse_decimal(&value[..separator]) else {
                return false;
            };
            let payload = &value[separator + 1..];
            match row {
                0..=25 => {
                    subpage.set_row(usize::try_from(row).unwrap_or(0), &decode_ol_row(payload));
                    true
                }
                26..=28 => match decode_ol_packet(payload) {
                    Some((designation, words)) => {
                        subpage.packets.push(ElementPacket {
                            row: u8::try_from(row).unwrap_or(26),
                            designation,
                            words,
                        });
                        true
                    }
                    None => false,
                },
                _ => false,
            }
        }
        // Service-level or editor-private records with no page content.
        b"DS" | b"SP" | b"CY" | b"MS" | b"PF" | b"RD" | b"RT" | b"SN" | b"TS" | b"HE" | b"FT" => {
            true
        }
        _ => false,
    }
}

// ---------------------------------------------------------------- T42 / HTT

const HAMMING_8_4: [u8; 16] = [
    0x15, 0x02, 0x49, 0x5e, 0x64, 0x73, 0x38, 0x2f, 0xd0, 0xc7, 0x8c, 0x9b, 0xa1, 0xb6, 0xfd, 0xea,
];

/// Decode a Hamming 8/4 byte, correcting a single bit error.
#[must_use]
pub fn hamming_8_4(byte: u8) -> Option<u8> {
    let mut best = None;
    for (value, &code) in (0_u8..).zip(HAMMING_8_4.iter()) {
        match (code ^ byte).count_ones() {
            0 => return Some(value),
            1 => best = Some(value),
            _ => {}
        }
    }
    best
}

fn htt_packets(bytes: &[u8]) -> Result<Vec<Vec<u8>>, ElementImportError> {
    let mut packets = Vec::new();
    for chunk in bytes.chunks_exact(45) {
        if chunk[..3] != [0xaa, 0xaa, 0xe4] {
            if packets.is_empty() {
                return Err(ElementImportError::Damaged);
            }
            break;
        }
        packets.push(chunk[3..].iter().map(|byte| byte.reverse_bits()).collect());
    }
    Ok(packets)
}

#[derive(Clone, Copy)]
struct PacketHeader {
    magazine: u8,
    packet: u8,
}

fn packet_header(packet: &[u8]) -> Option<PacketHeader> {
    let first = hamming_8_4(packet[0])?;
    let second = hamming_8_4(packet[1])?;
    Some(PacketHeader {
        magazine: first & 0x07,
        packet: (first >> 3) | (second << 1),
    })
}

#[allow(clippy::too_many_lines)] // one linear pass over the packet stream
fn import_packets(
    packets: impl IntoIterator<Item = Vec<u8>>,
    warnings: &mut Vec<ElementImportWarning>,
) -> Result<ElementPage, ElementImportError> {
    let mut page = ElementPage {
        description: String::new(),
        page: 0,
        subpages: Vec::new(),
    };
    let mut current_magazine = None;
    let mut body_seen = false;
    for packet in packets {
        if packet.len() != 42 {
            continue;
        }
        let Some(header) = packet_header(&packet) else {
            warn(warnings, ElementImportWarning::UnreadableRecordsSkipped);
            continue;
        };
        if header.packet == 0 {
            let Some(decoded) = packet[2..10]
                .iter()
                .map(|&byte| hamming_8_4(byte))
                .collect::<Option<Vec<_>>>()
            else {
                warn(warnings, ElementImportWarning::UnreadableRecordsSkipped);
                continue;
            };
            let number = (decoded[1] << 4) | decoded[0];
            if number == 0xff {
                continue; // time filling header
            }
            let magazine = if header.magazine == 0 {
                8
            } else {
                header.magazine
            };
            let address = (u16::from(magazine) << 8) | u16::from(number);
            if let Some(found) = current_magazine {
                if found != header.magazine {
                    continue; // parallel magazine
                }
                if address != page.page {
                    warn(warnings, ElementImportWarning::ExtraPagesIgnored);
                    break;
                }
                if !body_seen {
                    continue;
                }
            } else {
                current_magazine = Some(header.magazine);
                page.page = address;
            }
            body_seen = false;
            let mut subpage = ElementSubpage::default();
            let s1 = u64::from(decoded[2]);
            let s2 = u64::from(decoded[3] & 0x7);
            let s3 = u64::from(decoded[4]);
            let s4 = u64::from(decoded[5] & 0x3);
            subpage.subcode = s1 | (s2 << 4) | (s3 << 8) | (s4 << 12);
            // Control bits as QTeletextMaker reads them into PS.
            let mut status = super::model::DEFAULT_PAGE_STATUS;
            if decoded[3] & 0x08 != 0 {
                status |= 0x4000; // C4 erase page
            }
            if decoded[5] & 0x04 != 0 {
                status |= 0x0001; // C5 newsflash
            }
            if decoded[5] & 0x08 != 0 {
                status |= 0x0002; // C6 subtitle
            }
            for bit in 0..4 {
                if decoded[6] & (1 << bit) != 0 {
                    status |= 0x0004 << bit; // C7..C10
                }
            }
            if decoded[7] & 0x01 != 0 {
                status |= 0x0040; // C11 serial
            }
            subpage.page_status = status;
            let national = ((decoded[7] & 0x08) >> 3)
                | ((decoded[7] & 0x04) >> 1)
                | ((decoded[7] & 0x02) << 1);
            subpage.set_national_option(national);
            let mut header_row = [0x20; 40];
            for (cell, byte) in header_row[8..].iter_mut().zip(&packet[10..42]) {
                *cell = byte & 0x7f;
            }
            if header_row[8..].iter().any(|&cell| cell != 0x20) {
                subpage.set_row(0, &header_row);
            }
            page.subpages.push(subpage);
            continue;
        }
        if current_magazine != Some(header.magazine) {
            continue;
        }
        let Some(subpage) = page.subpages.last_mut() else {
            continue;
        };
        body_seen = true;
        match header.packet {
            1..=24 => {
                let cells: Vec<u8> = packet[2..42].iter().map(|byte| byte & 0x7f).collect();
                subpage.set_row(usize::from(header.packet), &cells);
            }
            27 => {
                if let Some(links) = fast_links(&packet[2..42], header.magazine) {
                    subpage.fast_links = Some(links);
                }
            }
            25..=28 => warn(warnings, ElementImportWarning::EnhancementsNotImported),
            _ => {}
        }
    }
    if page.subpages.is_empty() {
        return Err(ElementImportError::NoPage);
    }
    Ok(page)
}

/// Decode packet X/27/0 editorial links into absolute TTI FL page numbers.
fn fast_links(data: &[u8], magazine: u8) -> Option<[u16; 6]> {
    if hamming_8_4(data[0])? != 0 {
        return None;
    }
    let mut links = [0x8ff; 6];
    for (index, link) in links.iter_mut().enumerate() {
        let base = 1 + index * 6;
        let units = hamming_8_4(data[base])?;
        let tens = hamming_8_4(data[base + 1])?;
        let m1 = hamming_8_4(data[base + 3])? >> 3;
        let m23 = hamming_8_4(data[base + 5])? >> 2;
        let relative = m1 | (m23 << 1);
        let target = (magazine ^ relative) & 0x7;
        let target = if target == 0 { 8 } else { target };
        *link = (u16::from(target) << 8) | (u16::from(tens) << 4) | u16::from(units);
    }
    Some(links)
}

// ---------------------------------------------------------------- EP1

/// EP1 language byte to Table 32 national option (default region).
fn ep1_national(code: u8) -> Option<u8> {
    Some(match code {
        0x09 => 0,
        0x0d => 1,
        0x18 => 2,
        0x11 => 3,
        0x0b => 4,
        0x17 => 5,
        0x07 => 6,
        _ => return None,
    })
}

fn import_ep1(
    bytes: &[u8],
    warnings: &mut Vec<ElementImportWarning>,
) -> Result<ElementPage, ElementImportError> {
    let mut page = ElementPage {
        description: String::new(),
        page: 0x100,
        subpages: Vec::new(),
    };
    let mut position = 0;
    let mut remaining = 1_usize;
    if bytes.len() >= 6 && bytes[..3] == *b"JWC" {
        remaining = usize::from(bytes[3]).max(1);
        position = 6;
    }
    while remaining > 0 {
        let header = bytes
            .get(position..position + 6)
            .ok_or(ElementImportError::Damaged)?;
        if header[0] != 0xfe || header[1] != 0x01 {
            return Err(ElementImportError::Damaged);
        }
        position += 6;
        let mut subpage = ElementSubpage::default();
        match ep1_national(header[2]) {
            Some(option) => subpage.set_national_option(option),
            None => warn(warnings, ElementImportWarning::UnknownLanguage),
        }
        if header[3] == 0xca {
            let extension = bytes
                .get(position..position + 4)
                .ok_or(ElementImportError::Damaged)?;
            let length = usize::from(extension[2]) | (usize::from(extension[3]) << 8);
            position += 4 + length.div_ceil(40) * 40;
            warn(warnings, ElementImportWarning::EnhancementsNotImported);
        }
        for row in 0..24 {
            let cells = bytes
                .get(position..position + 40)
                .ok_or(ElementImportError::Damaged)?;
            position += 40;
            if cells.iter().any(|&cell| cell != 0x20) {
                let mut row_cells = [0x20; 40];
                for (target, byte) in row_cells.iter_mut().zip(cells) {
                    *target = byte & 0x7f;
                }
                if row == 0 {
                    row_cells[..8].fill(0x20);
                }
                subpage.set_row(row, &row_cells);
            }
        }
        page.subpages.push(subpage);
        remaining -= 1;
        if remaining > 0 {
            // Each further subpage follows a 40-byte buffer and 2-byte terminator.
            position += 42;
            if position >= bytes.len() {
                break;
            }
        }
    }
    if page.subpages.is_empty() {
        return Err(ElementImportError::NoPage);
    }
    Ok(page)
}
