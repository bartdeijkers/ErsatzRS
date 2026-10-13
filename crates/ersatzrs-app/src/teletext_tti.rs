//! Bounded, byte-oriented syntax for the pinned `QTeletextMaker` 1.0 TTI profile.
//!
//! This module preserves record and payload order; it does not interpret authored
//! subcodes, editor edit permissions or Level 2.5/3.5 semantics. Editor ordinals
//! must be reconciled with a trusted export receipt before creating a draft.
//! Writing canonicalizes numeric spelling and line endings to CRLF. It does not
//! claim byte-identical metadata spelling or publication compatibility.

use std::{collections::BTreeSet, fmt};

/// Host-supplied bounds; neither value may be zero. No product defaults exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextTtiBudget {
    pub max_bytes: usize,
    pub max_records: usize,
}

/// Static failure category; errors never include authored input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextTtiErrorKind {
    InvalidBudget,
    BytesExceeded,
    RecordsExceeded,
    EmptyDocument,
    BlankRecord,
    LineEnding,
    UnknownRecord,
    InvalidValue,
    Overflow,
    InvalidOrder,
    DuplicateRecord,
    DifferentPage,
    InvalidRowWidth,
    InvalidCharacter,
    InvalidEscape,
    InvalidPacket,
    InvalidMagazineScope,
}

/// One-based file location, optionally with an OL row and one-based payload cell.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("TTI {kind:?} at line {line}, row {row:?}, column {column:?}")]
pub struct TeletextTtiError {
    pub kind: TeletextTtiErrorKind,
    pub line: usize,
    pub row: Option<u8>,
    pub column: Option<usize>,
}

impl TeletextTtiError {
    const fn at(kind: TeletextTtiErrorKind, line: usize) -> Self {
        Self {
            kind,
            line,
            row: None,
            column: None,
        }
    }

    const fn cell(kind: TeletextTtiErrorKind, line: usize, row: u8, column: usize) -> Self {
        Self {
            kind,
            line,
            row: Some(row),
            column: Some(column),
        }
    }
}

/// CT's raw timing unit, without applying a service scheduling policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextTtiCycleUnit {
    Time,
    Count,
}

/// A closed TTI record. Public payloads are checked on document construction.
///
/// `PageNumber` and `SubpageOrdinal` contain decimal editor ordinals, never
/// authored hexadecimal subcodes. `Packet` retains its exact packet number;
/// packet 29 is permitted only on an explicit magazine page `<m>FF`. PN wraps
/// the editor ordinal to eight bits when saved by the editor; SC does not, so
/// the two fields are retained independently and are never forced to agree.
#[derive(Clone, PartialEq, Eq)]
pub enum TeletextTtiRecord {
    Description(String),
    PageNumber {
        page: u16,
        editor_ordinal: u64,
    },
    SubpageOrdinal(u64),
    PageStatus(u16),
    Region(u64),
    Cycle {
        value: u64,
        unit: TeletextTtiCycleUnit,
    },
    FastLinks([u16; 6]),
    BaseRow {
        row: u8,
        cells: [u8; 40],
    },
    Packet {
        row: u8,
        designation: u8,
        words: [u32; 13],
    },
}

impl fmt::Debug for TeletextTtiRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self {
            Self::Description(_) => "DE",
            Self::PageNumber { .. } => "PN",
            Self::SubpageOrdinal(_) => "SC",
            Self::PageStatus(_) => "PS",
            Self::Region(_) => "RE",
            Self::Cycle { .. } => "CT",
            Self::FastLinks(_) => "FL",
            Self::BaseRow { .. } | Self::Packet { .. } => "OL",
        };
        f.debug_struct("TeletextTtiRecord")
            .field("kind", &kind)
            .finish_non_exhaustive()
    }
}

/// A checked ordered document for one base page with independent editor ordinals.
#[derive(Clone, PartialEq, Eq)]
pub struct TeletextTtiDocument {
    records: Vec<TeletextTtiRecord>,
}

impl fmt::Debug for TeletextTtiDocument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TeletextTtiDocument")
            .field("record_count", &self.records.len())
            .finish()
    }
}

impl TeletextTtiDocument {
    /// Validate values, state, record count and canonical encoded size atomically.
    pub fn checked_from_records(
        records: Vec<TeletextTtiRecord>,
        budget: TeletextTtiBudget,
    ) -> Result<Self, TeletextTtiError> {
        validate_records(&records, budget)?;
        Ok(Self { records })
    }

    /// Records in their original order; payloads cannot be mutated unchecked.
    #[must_use]
    pub fn records(&self) -> &[TeletextTtiRecord] {
        &self.records
    }
}

fn validate_budget(budget: TeletextTtiBudget) -> Result<(), TeletextTtiError> {
    if budget.max_bytes == 0 || budget.max_records == 0 {
        return Err(TeletextTtiError::at(TeletextTtiErrorKind::InvalidBudget, 0));
    }
    Ok(())
}

/// Parse exact record bytes without trimming, padding or parity masking.
///
/// LF and CRLF are accepted, including a final record without a newline. Blank
/// records and bare CR are rejected. Description bytes must be UTF-8 without
/// controls; all other metadata is ASCII. Unknown commands are rejected.
pub fn parse_teletext_tti(
    input: &[u8],
    budget: TeletextTtiBudget,
) -> Result<TeletextTtiDocument, TeletextTtiError> {
    validate_budget(budget)?;
    if input.len() > budget.max_bytes {
        return Err(TeletextTtiError::at(TeletextTtiErrorKind::BytesExceeded, 0));
    }
    if input.is_empty() {
        return Err(TeletextTtiError::at(TeletextTtiErrorKind::EmptyDocument, 1));
    }
    let mut records = Vec::new();
    let mut lines = input.split(|&byte| byte == b'\n').peekable();
    let mut line_number = 0;
    while let Some(raw) = lines.next() {
        line_number += 1;
        if raw.is_empty() && lines.peek().is_none() && input.ends_with(b"\n") {
            break;
        }
        let line = raw.strip_suffix(b"\r").unwrap_or(raw);
        if raw.ends_with(b"\r") && lines.peek().is_none() && !input.ends_with(b"\n") {
            return Err(TeletextTtiError::at(
                TeletextTtiErrorKind::LineEnding,
                line_number,
            ));
        }
        if line.contains(&b'\r') {
            return Err(TeletextTtiError::at(
                TeletextTtiErrorKind::LineEnding,
                line_number,
            ));
        }
        if line.is_empty() {
            return Err(TeletextTtiError::at(
                TeletextTtiErrorKind::BlankRecord,
                line_number,
            ));
        }
        if records.len() == budget.max_records {
            return Err(TeletextTtiError::at(
                TeletextTtiErrorKind::RecordsExceeded,
                line_number,
            ));
        }
        records.push(parse_record(line, line_number)?);
    }
    TeletextTtiDocument::checked_from_records(records, budget)
}

fn decimal(bytes: &[u8], line: usize) -> Result<u64, TeletextTtiError> {
    number(bytes, 10, line)
}

fn number(bytes: &[u8], radix: u32, line: usize) -> Result<u64, TeletextTtiError> {
    if bytes.is_empty() {
        return Err(TeletextTtiError::at(
            TeletextTtiErrorKind::InvalidValue,
            line,
        ));
    }
    bytes.iter().try_fold(0_u64, |value, byte| {
        let digit = match byte {
            b'0'..=b'9' => u32::from(byte - b'0'),
            b'A'..=b'F' if radix == 16 => u32::from(byte - b'A') + 10,
            b'a'..=b'f' if radix == 16 => u32::from(byte - b'a') + 10,
            _ => {
                return Err(TeletextTtiError::at(
                    TeletextTtiErrorKind::InvalidValue,
                    line,
                ))
            }
        };
        if digit >= radix {
            return Err(TeletextTtiError::at(
                TeletextTtiErrorKind::InvalidValue,
                line,
            ));
        }
        value
            .checked_mul(u64::from(radix))
            .and_then(|n| n.checked_add(u64::from(digit)))
            .ok_or_else(|| TeletextTtiError::at(TeletextTtiErrorKind::Overflow, line))
    })
}

fn hex16(bytes: &[u8], line: usize) -> Result<u16, TeletextTtiError> {
    u16::try_from(number(bytes, 16, line)?)
        .map_err(|_| TeletextTtiError::at(TeletextTtiErrorKind::Overflow, line))
}

fn parse_links(value: &[u8], index: usize) -> Result<[u16; 6], TeletextTtiError> {
    let mut links = [0; 6];
    let mut fields = value.split(|&byte| byte == b',');
    for link in &mut links {
        let field = fields
            .next()
            .ok_or_else(|| TeletextTtiError::at(TeletextTtiErrorKind::InvalidValue, index))?;
        if field.len() != 3 {
            return Err(TeletextTtiError::at(
                TeletextTtiErrorKind::InvalidValue,
                index,
            ));
        }
        *link = hex16(field, index)?;
    }
    if fields.next().is_some() {
        return Err(TeletextTtiError::at(
            TeletextTtiErrorKind::InvalidValue,
            index,
        ));
    }
    Ok(links)
}

fn parse_record(line: &[u8], index: usize) -> Result<TeletextTtiRecord, TeletextTtiError> {
    if line.len() < 3 || line[2] != b',' {
        return Err(TeletextTtiError::at(
            TeletextTtiErrorKind::UnknownRecord,
            index,
        ));
    }
    let value = &line[3..];
    Ok(match &line[..2] {
        b"DE" => TeletextTtiRecord::Description(
            std::str::from_utf8(value)
                .map_err(|_| TeletextTtiError::at(TeletextTtiErrorKind::InvalidCharacter, index))?
                .to_owned(),
        ),
        b"PN" => {
            if value.len() < 5 {
                return Err(TeletextTtiError::at(
                    TeletextTtiErrorKind::InvalidValue,
                    index,
                ));
            }
            TeletextTtiRecord::PageNumber {
                page: hex16(&value[..3], index)?,
                editor_ordinal: decimal(&value[3..], index)?,
            }
        }
        b"SC" => {
            if value.len() < 4 {
                return Err(TeletextTtiError::at(
                    TeletextTtiErrorKind::InvalidValue,
                    index,
                ));
            }
            TeletextTtiRecord::SubpageOrdinal(decimal(value, index)?)
        }
        b"PS" => TeletextTtiRecord::PageStatus(hex16(value, index)?),
        b"RE" => TeletextTtiRecord::Region(decimal(value, index)?),
        b"CT" => {
            let Some((value, unit)) = value.split_last() else {
                return Err(TeletextTtiError::at(
                    TeletextTtiErrorKind::InvalidValue,
                    index,
                ));
            };
            let Some(digits) = unit.strip_suffix(b",") else {
                return Err(TeletextTtiError::at(
                    TeletextTtiErrorKind::InvalidValue,
                    index,
                ));
            };
            let unit = match value {
                b'T' => TeletextTtiCycleUnit::Time,
                b'C' => TeletextTtiCycleUnit::Count,
                _ => {
                    return Err(TeletextTtiError::at(
                        TeletextTtiErrorKind::InvalidValue,
                        index,
                    ))
                }
            };
            TeletextTtiRecord::Cycle {
                value: decimal(digits, index)?,
                unit,
            }
        }
        b"FL" => TeletextTtiRecord::FastLinks(parse_links(value, index)?),
        b"OL" => {
            let separator = value
                .iter()
                .position(|&byte| byte == b',')
                .ok_or_else(|| TeletextTtiError::at(TeletextTtiErrorKind::InvalidValue, index))?;
            let row = u8::try_from(decimal(&value[..separator], index)?)
                .map_err(|_| TeletextTtiError::at(TeletextTtiErrorKind::InvalidValue, index))?;
            parse_row(row, &value[separator + 1..], index)?
        }
        _ => {
            return Err(TeletextTtiError::at(
                TeletextTtiErrorKind::UnknownRecord,
                index,
            ))
        }
    })
}

fn parse_row(row: u8, bytes: &[u8], line: usize) -> Result<TeletextTtiRecord, TeletextTtiError> {
    if row <= 25 {
        let mut cells = [0; 40];
        let mut input = 0;
        let mut output = 0;
        while input < bytes.len() {
            if output == 40 {
                return Err(TeletextTtiError::cell(
                    TeletextTtiErrorKind::InvalidRowWidth,
                    line,
                    row,
                    output + 1,
                ));
            }
            let byte = bytes[input];
            cells[output] = if byte == 0x1b {
                input += 1;
                let escaped = bytes
                    .get(input)
                    .copied()
                    .filter(|value| (0x40..=0x5f).contains(value))
                    .ok_or_else(|| {
                        TeletextTtiError::cell(
                            TeletextTtiErrorKind::InvalidEscape,
                            line,
                            row,
                            output + 1,
                        )
                    })?;
                escaped - 0x40
            } else if (0x20..=0x7f).contains(&byte) {
                byte
            } else {
                return Err(TeletextTtiError::cell(
                    TeletextTtiErrorKind::InvalidCharacter,
                    line,
                    row,
                    output + 1,
                ));
            };
            input += 1;
            output += 1;
        }
        if output != 40 {
            return Err(TeletextTtiError::cell(
                TeletextTtiErrorKind::InvalidRowWidth,
                line,
                row,
                output + 1,
            ));
        }
        return Ok(TeletextTtiRecord::BaseRow { row, cells });
    }
    if !(26..=29).contains(&row) || bytes.len() != 40 {
        return Err(TeletextTtiError::cell(
            TeletextTtiErrorKind::InvalidPacket,
            line,
            row,
            1,
        ));
    }
    if !(0x40..=0x4f).contains(&bytes[0]) {
        return Err(TeletextTtiError::cell(
            TeletextTtiErrorKind::InvalidPacket,
            line,
            row,
            1,
        ));
    }
    let mut words = [0; 13];
    for (i, word) in words.iter_mut().enumerate() {
        for part in 0..3 {
            let column = 1 + i * 3 + part;
            let byte = bytes[column];
            if !(0x40..=0x7f).contains(&byte) {
                return Err(TeletextTtiError::cell(
                    TeletextTtiErrorKind::InvalidPacket,
                    line,
                    row,
                    column + 1,
                ));
            }
            *word |= u32::from(byte - 0x40) << (part * 6);
        }
    }
    Ok(TeletextTtiRecord::Packet {
        row,
        designation: bytes[0] - 0x40,
        words,
    })
}

fn block_key(
    record: &TeletextTtiRecord,
    line: usize,
    page: u16,
) -> Result<(u8, u8), TeletextTtiError> {
    let invalid = || TeletextTtiError::at(TeletextTtiErrorKind::InvalidValue, line);
    Ok(match record {
        TeletextTtiRecord::SubpageOrdinal(_) => (0, 0),
        TeletextTtiRecord::PageStatus(_) => (1, 0),
        TeletextTtiRecord::Region(_) => (2, 0),
        TeletextTtiRecord::Cycle { value, .. } => {
            if *value == 0 {
                return Err(invalid());
            }
            (3, 0)
        }
        TeletextTtiRecord::FastLinks(links) => {
            if links.iter().any(|&link| link > 0xfff) {
                return Err(invalid());
            }
            (4, 0)
        }
        TeletextTtiRecord::BaseRow { row, cells } => {
            if *row > 25 {
                return Err(invalid());
            }
            if let Some(column) = cells.iter().position(|&byte| byte > 0x7f) {
                return Err(TeletextTtiError::cell(
                    TeletextTtiErrorKind::InvalidCharacter,
                    line,
                    *row,
                    column + 1,
                ));
            }
            (10 + *row, 0)
        }
        TeletextTtiRecord::Packet {
            row,
            designation,
            words,
        } => {
            if !(26..=29).contains(row) || *designation > 15 {
                return Err(invalid());
            }
            if *row == 29 && page & 0xff != 0xff {
                return Err(TeletextTtiError::cell(
                    TeletextTtiErrorKind::InvalidMagazineScope,
                    line,
                    *row,
                    1,
                ));
            }
            if let Some(column) = words.iter().position(|&word| word > 0x3ffff) {
                return Err(TeletextTtiError::cell(
                    TeletextTtiErrorKind::InvalidPacket,
                    line,
                    *row,
                    2 + column * 3,
                ));
            }
            (10 + *row, *designation)
        }
        TeletextTtiRecord::Description(_) | TeletextTtiRecord::PageNumber { .. } => {
            return Err(invalid())
        }
    })
}

fn validate_description(
    value: &str,
    line: usize,
    described: bool,
    page: Option<u16>,
) -> Result<(), TeletextTtiError> {
    if described {
        return Err(TeletextTtiError::at(
            TeletextTtiErrorKind::DuplicateRecord,
            line,
        ));
    }
    if page.is_some() {
        return Err(TeletextTtiError::at(
            TeletextTtiErrorKind::InvalidOrder,
            line,
        ));
    }
    if value.chars().any(char::is_control) {
        return Err(TeletextTtiError::at(
            TeletextTtiErrorKind::InvalidCharacter,
            line,
        ));
    }
    Ok(())
}

fn validate_records(
    records: &[TeletextTtiRecord],
    budget: TeletextTtiBudget,
) -> Result<usize, TeletextTtiError> {
    validate_budget(budget)?;
    if records.is_empty() {
        return Err(TeletextTtiError::at(TeletextTtiErrorKind::EmptyDocument, 1));
    }
    if records.len() > budget.max_records {
        return Err(TeletextTtiError::at(
            TeletextTtiErrorKind::RecordsExceeded,
            budget.max_records + 1,
        ));
    }
    let mut page = None;
    let mut ordinal = None;
    let mut blocks = BTreeSet::new();
    let mut subpage_ordinal = None;
    let mut block = BTreeSet::new();
    let mut described = false;
    let mut size = 0_usize;
    for (i, record) in records.iter().enumerate() {
        let line = i + 1;
        let invalid = || TeletextTtiError::at(TeletextTtiErrorKind::InvalidValue, line);
        let key = match record {
            TeletextTtiRecord::Description(value) => {
                validate_description(value, line, described, page)?;
                described = true;
                None
            }
            TeletextTtiRecord::PageNumber {
                page: address,
                editor_ordinal,
            } => {
                if !(0x100..=0x8ff).contains(address) {
                    return Err(invalid());
                }
                if page.is_some_and(|previous| previous != *address) {
                    return Err(TeletextTtiError::at(
                        TeletextTtiErrorKind::DifferentPage,
                        line,
                    ));
                }
                if ordinal.is_some_and(|previous| !blocks.insert((previous, subpage_ordinal))) {
                    return Err(TeletextTtiError::at(
                        TeletextTtiErrorKind::DuplicateRecord,
                        line,
                    ));
                }
                page = Some(*address);
                ordinal = Some(*editor_ordinal);
                subpage_ordinal = None;
                block.clear();
                None
            }
            other => {
                if page.is_none() {
                    return Err(TeletextTtiError::at(
                        TeletextTtiErrorKind::InvalidOrder,
                        line,
                    ));
                }
                if let TeletextTtiRecord::SubpageOrdinal(value) = other {
                    subpage_ordinal = Some(*value);
                }
                Some(block_key(other, line, page.ok_or_else(invalid)?)?)
            }
        };
        if key.is_some_and(|key| !block.insert(key)) {
            return Err(TeletextTtiError::at(
                TeletextTtiErrorKind::DuplicateRecord,
                line,
            ));
        }
        size = size
            .checked_add(encoded_len(record))
            .ok_or_else(|| TeletextTtiError::at(TeletextTtiErrorKind::BytesExceeded, line))?;
        if size > budget.max_bytes {
            return Err(TeletextTtiError::at(
                TeletextTtiErrorKind::BytesExceeded,
                line,
            ));
        }
    }
    if page.is_none() {
        return Err(TeletextTtiError::at(
            TeletextTtiErrorKind::InvalidOrder,
            records.len(),
        ));
    }
    if ordinal.is_some_and(|previous| !blocks.insert((previous, subpage_ordinal))) {
        return Err(TeletextTtiError::at(
            TeletextTtiErrorKind::DuplicateRecord,
            records.len(),
        ));
    }
    Ok(size)
}

fn digits(value: u64) -> usize {
    if value == 0 {
        1
    } else {
        value.ilog10() as usize + 1
    }
}

fn encoded_len(record: &TeletextTtiRecord) -> usize {
    5 + match record {
        TeletextTtiRecord::Description(value) => value.len(),
        TeletextTtiRecord::PageNumber { editor_ordinal, .. } => 3 + digits(*editor_ordinal).max(2),
        TeletextTtiRecord::SubpageOrdinal(value) => digits(*value).max(4),
        TeletextTtiRecord::PageStatus(_) => 4,
        TeletextTtiRecord::Region(value) => digits(*value),
        TeletextTtiRecord::Cycle { value, .. } => digits(*value) + 2,
        TeletextTtiRecord::FastLinks(_) => 23,
        TeletextTtiRecord::BaseRow { row, cells } => {
            digits(u64::from(*row)) + 1 + 40 + cells.iter().filter(|&&byte| byte < 0x20).count()
        }
        TeletextTtiRecord::Packet { row, .. } => digits(u64::from(*row)) + 1 + 40,
    }
}

/// Write checked canonical CRLF bytes without altering record/payload order.
///
/// PN has a three-digit hexadecimal page and a decimal ordinal with minimum
/// width two; SC has minimum width four. Numeric widths are not maximum values.
pub fn write_teletext_tti(
    document: &TeletextTtiDocument,
    budget: TeletextTtiBudget,
) -> Result<Vec<u8>, TeletextTtiError> {
    let size = validate_records(&document.records, budget)?;
    let mut output = Vec::with_capacity(size);
    for record in &document.records {
        match record {
            TeletextTtiRecord::Description(value) => {
                output.extend_from_slice(b"DE,");
                output.extend_from_slice(value.as_bytes());
            }
            TeletextTtiRecord::PageNumber {
                page,
                editor_ordinal,
            } => output.extend_from_slice(format!("PN,{page:03X}{editor_ordinal:02}").as_bytes()),
            TeletextTtiRecord::SubpageOrdinal(value) => {
                output.extend_from_slice(format!("SC,{value:04}").as_bytes());
            }
            TeletextTtiRecord::PageStatus(value) => {
                output.extend_from_slice(format!("PS,{value:04X}").as_bytes());
            }
            TeletextTtiRecord::Region(value) => {
                output.extend_from_slice(format!("RE,{value}").as_bytes());
            }
            TeletextTtiRecord::Cycle { value, unit } => output.extend_from_slice(
                format!(
                    "CT,{value},{}",
                    match unit {
                        TeletextTtiCycleUnit::Time => 'T',
                        TeletextTtiCycleUnit::Count => 'C',
                    }
                )
                .as_bytes(),
            ),
            TeletextTtiRecord::FastLinks(links) => {
                output.extend_from_slice(b"FL,");
                for (i, link) in links.iter().enumerate() {
                    if i != 0 {
                        output.push(b',');
                    }
                    output.extend_from_slice(format!("{link:03X}").as_bytes());
                }
            }
            TeletextTtiRecord::BaseRow { row, cells } => {
                output.extend_from_slice(format!("OL,{row},").as_bytes());
                for &byte in cells {
                    if byte < 0x20 {
                        output.extend_from_slice(&[0x1b, byte | 0x40]);
                    } else {
                        output.push(byte);
                    }
                }
            }
            TeletextTtiRecord::Packet {
                row,
                designation,
                words,
            } => {
                output.extend_from_slice(format!("OL,{row},").as_bytes());
                output.push(*designation | 0x40);
                for word in words {
                    for shift in [0, 6, 12] {
                        output.push(((word >> shift).to_le_bytes()[0] & 0x3f) | 0x40);
                    }
                }
            }
        }
        output.extend_from_slice(b"\r\n");
    }
    Ok(output)
}

#[cfg(test)]
mod tests;
