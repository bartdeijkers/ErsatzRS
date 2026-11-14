//! Exact physical source frames, with a borrowed TS188 payload view.
//!
//! The M2TS prefix remains opaque source data. Detection neither resynchronizes
//! input nor grants any timestamp, PSI, source-epoch or playback authority.

use std::{
    fs::File,
    io::{Seek as _, SeekFrom},
};

use super::headers;
use crate::ffprobe::{FfprobeError, Result};

const TS_BYTES: usize = 188;
const MAX_FRAME_BYTES: usize = 192;
const PROBE_FRAMES: usize = 3;
const LABEL: &str = "source teletext packet framing";

/// Physical framing of original source and unmodified retained frame bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceTeletextPacketFormat {
    /// One original 188-byte transport packet per frame.
    Ts188,
    /// Four opaque source bytes followed by one 188-byte transport packet.
    M2ts192,
}

impl SourceTeletextPacketFormat {
    /// Total physical frame width, including an M2TS prefix when present.
    #[must_use]
    pub const fn packet_bytes(self) -> usize {
        match self {
            Self::Ts188 => TS_BYTES,
            Self::M2ts192 => MAX_FRAME_BYTES,
        }
    }

    /// Position of the transport sync byte within its physical frame.
    #[must_use]
    pub const fn sync_offset(self) -> usize {
        match self {
            Self::Ts188 => 0,
            Self::M2ts192 => 4,
        }
    }

    /// Convert an exact physical frame-start position, never its inner sync byte.
    #[must_use]
    pub fn source_packet_at(self, byte_position: u64) -> Option<u64> {
        let stride = self.packet_bytes() as u64;
        byte_position
            .is_multiple_of(stride)
            .then_some(byte_position / stride)
    }
}

/// Complete original frame bytes without allocating or interpreting its prefix.
#[derive(Debug)]
pub(super) struct SourcePacketFrame {
    pub(super) packet: [u8; TS_BYTES],
    prefix: [u8; 4],
    format: SourceTeletextPacketFormat,
}

/// Fixed-memory positioned reads for a complete sequential source walk.
/// Every returned frame retains its original bytes and per-frame stop check.
pub(super) struct SourcePacketReader<'a> {
    file: &'a File,
    len: u64,
    format: SourceTeletextPacketFormat,
    offset: u64,
    buffer: Vec<u8>,
    cursor: usize,
    filled: usize,
}

impl<'a> SourcePacketReader<'a> {
    pub(super) fn new(file: &'a File, len: u64, format: SourceTeletextPacketFormat) -> Self {
        Self {
            file,
            len,
            format,
            offset: 0,
            buffer: vec![0; 64 * 1024],
            cursor: 0,
            filled: 0,
        }
    }

    pub(super) fn read_next<T>(
        &mut self,
        stop: &headers::ReadStop<'_, T>,
    ) -> Result<SourcePacketFrame> {
        stop.check_source_guard()?;
        let stride = self.format.packet_bytes();
        let end = self.offset.checked_add(stride as u64).ok_or_else(invalid)?;
        if end > self.len {
            return Err(invalid());
        }
        if self.filled - self.cursor < stride {
            let remaining = self.len.checked_sub(self.offset).ok_or_else(invalid)?;
            let count =
                usize::try_from(remaining.min(self.buffer.len() as u64)).map_err(|_| invalid())?;
            read_bytes(self.file, &mut self.buffer[..count], self.offset, stop)?;
            self.cursor = 0;
            self.filled = count;
        }
        let frame = parse_frame(&self.buffer[self.cursor..self.cursor + stride], self.format)?;
        self.cursor += stride;
        self.offset = end;
        stop.check_source_guard()?;
        Ok(frame)
    }
}

impl SourcePacketFrame {
    /// Original byte order for hashing or writing: opaque prefix, then TS188.
    pub(super) fn parts(&self) -> [&[u8]; 2] {
        [&self.prefix[..self.format.sync_offset()], &self.packet]
    }
}

/// Inspect at most three complete frames per supported format at source byte zero.
/// Detection leaves the held cursor unchanged. Remaining source framing
/// and partial tails must still be checked by the consuming bounded reader.
pub(super) fn detect<T>(
    file: &File,
    len: u64,
    stop: &headers::ReadStop<'_, T>,
) -> Result<SourceTeletextPacketFormat> {
    if len < TS_BYTES as u64 {
        return Err(invalid());
    }
    let mut prefix = [0; MAX_FRAME_BYTES * PROBE_FRAMES];
    let length = usize::try_from(len.min(prefix.len() as u64)).map_err(|_| invalid())?;
    // Windows seek_read changes the shared cursor, unlike Unix read_at. Header
    // acquisition resumes sequentially after detection, including after errors.
    let mut cursor = file;
    let original_position = cursor.stream_position().map_err(|_| invalid())?;
    let read_result = read_bytes(file, &mut prefix[..length], 0, stop);
    cursor
        .seek(SeekFrom::Start(original_position))
        .map_err(|_| invalid())?;
    read_result?;
    let mut detected = None;
    for format in [
        SourceTeletextPacketFormat::Ts188,
        SourceTeletextPacketFormat::M2ts192,
    ] {
        let stride = format.packet_bytes();
        let count = (length / stride).min(PROBE_FRAMES);
        if count > 0
            && (0..count).all(|ordinal| {
                let start = ordinal * stride + format.sync_offset();
                packet_header(&prefix[start..start + TS_BYTES])
            })
        {
            if detected.is_some() {
                return Err(invalid());
            }
            detected = Some(format);
        }
    }
    detected.ok_or_else(invalid)
}

pub(super) fn read<T>(
    file: &File,
    ordinal: u64,
    format: SourceTeletextPacketFormat,
    stop: &headers::ReadStop<'_, T>,
) -> Result<SourcePacketFrame> {
    let offset = ordinal
        .checked_mul(format.packet_bytes() as u64)
        .ok_or_else(invalid)?;
    let mut raw = [0; MAX_FRAME_BYTES];
    read_bytes(file, &mut raw[..format.packet_bytes()], offset, stop)?;
    parse_frame(&raw[..format.packet_bytes()], format)
}

fn parse_frame(raw: &[u8], format: SourceTeletextPacketFormat) -> Result<SourcePacketFrame> {
    let packet = &raw[format.sync_offset()..format.sync_offset() + TS_BYTES];
    if !packet_header(packet) {
        return Err(invalid());
    }
    Ok(SourcePacketFrame {
        packet: packet.try_into().map_err(|_| invalid())?,
        prefix: raw[..4].try_into().map_err(|_| invalid())?,
        format,
    })
}

/// Guarded exact read at an explicit offset. Windows also advances the cursor;
/// consumers must not combine these reads with cursor-dependent sequential work.
pub(super) fn read_bytes<T>(
    file: &File,
    raw: &mut [u8],
    offset: u64,
    stop: &headers::ReadStop<'_, T>,
) -> Result<()> {
    stop.check_source_guard()?;
    offset
        .checked_add(u64::try_from(raw.len()).map_err(|_| invalid())?)
        .ok_or_else(invalid)?;
    let mut done = 0;
    while done < raw.len() {
        stop.check_source_guard()?;
        let position = offset
            .checked_add(u64::try_from(done).map_err(|_| invalid())?)
            .ok_or_else(invalid)?;
        #[cfg(unix)]
        let result = {
            use std::os::unix::fs::FileExt as _;
            file.read_at(&mut raw[done..], position)
        };
        #[cfg(windows)]
        let result = {
            use std::os::windows::fs::FileExt as _;
            file.seek_read(&mut raw[done..], position)
        };
        #[cfg(not(any(unix, windows)))]
        let result: std::io::Result<usize> = Err(std::io::ErrorKind::Unsupported.into());
        match result {
            Ok(0) => return Err(invalid()),
            Ok(count) => done += count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return Err(invalid()),
        }
    }
    stop.check_source_guard()
}

fn packet_header(raw: &[u8]) -> bool {
    // Scrambling, TEI, adaptation and all semantic checks belong to the existing
    // transport consumers. Only sync/header positions establish this framing.
    raw[0] == 0x47 && raw[3] & 0x30 != 0
}

fn invalid() -> FfprobeError {
    FfprobeError::InvalidOutput { path: LABEL.into() }
}

#[cfg(test)]
#[path = "source_packet_tests.rs"]
mod tests;
