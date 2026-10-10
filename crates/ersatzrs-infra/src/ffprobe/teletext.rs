//! Bounded decoding of explicitly identified `FFmpeg`-packed teletext metadata.
//!
//! This is an ephemeral codec projection, not a raw DVB descriptor parser or a
//! complete inventory of advertised or transmitted pages. It does not inspect
//! packets, discover subcodes/character sets, or modify ordinary subtitle rows.
//!
//! Upstream `FFmpeg` n9.0 evidence:
//! - [MPEG-TS descriptor handling](https://github.com/FFmpeg/FFmpeg/blob/n9.0/libavformat/mpegts.c#L1998)
//!   stores three-letter languages as CSV and the remaining two bytes per entry
//!   as extradata. Repeated descriptors replace the buffer prefix and CSV; a
//!   shorter replacement can leave an unpaired stale tail. A larger replacement
//!   can fail without a corresponding warning in the final probe JSON.
//! - [MPEG-TS muxer](https://github.com/FFmpeg/FFmpeg/blob/n9.0/libavformat/mpegtsenc.c#L720)
//!   documents five type bits, three magazine bits and eight page bits.
//! - [Base64 writer](https://github.com/FFmpeg/FFmpeg/blob/n9.0/fftools/textformat/avtextformat.c#L514)
//!   emits an initial LF and LF-terminated chunks of at most 60 decoded bytes.
//!
//! Even matching language/extradata counts cannot establish descriptor
//! completeness. Entry ordinals refer only to the supplied projection. The
//! caller identifies codec/container and owns stream, PID and program identity.

use std::borrow::Cow;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use ersatzrs_domain::{TeletextLanguage, TeletextPageNumber};

/// Implementation resource bound, not the number of descriptors in a service.
pub const MAX_PROJECTED_TELETEXT_ENTRIES: usize = 1_024;
/// Two packed bytes per projected entry; language bytes are stored separately.
pub const MAX_PACKED_TELETEXT_BYTES: usize = MAX_PROJECTED_TELETEXT_ENTRIES * 2;
/// At most three language letters and one comma per projected entry.
pub const MAX_TELETEXT_LANGUAGE_CSV_BYTES: usize = MAX_PROJECTED_TELETEXT_ENTRIES * 4 - 1;
/// Base64 characters plus the documented leading LF and per-chunk trailing LF.
pub const MAX_FFPROBE9_TELETEXT_BASE64_BYTES: usize =
    MAX_PACKED_TELETEXT_BYTES.div_ceil(3) * 4 + MAX_PACKED_TELETEXT_BYTES.div_ceil(60) + 1;

/// Explicit encoding of codec extradata; raw five-byte DVB entries are excluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackedTeletextData<'a> {
    /// Already decoded two-byte pairs, in the supplied projection's order.
    Bytes(&'a [u8]),
    /// n9.0 `FFprobe` output requested with `-data_dump_format base64`.
    /// The default xxd dump is deliberately not accepted under this variant.
    Ffprobe9Base64(&'a str),
}

/// One language-associated pair from the supplied codec projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbedTeletextDescriptor {
    /// Zero-based position in this projection, not a wire descriptor identity.
    pub projection_ordinal: usize,
    /// Normalized three-letter syntax; no charset or registry claim is made.
    pub language: TeletextLanguage,
    /// Raw five-bit descriptor type, including reserved values.
    /// This is independent of authored page-header or subtitle selection flags.
    pub descriptor_type: u8,
    /// Raw three-bit magazine; zero represents magazine eight.
    pub wire_magazine: u8,
    /// Raw hexadecimal page byte, including non-display addresses.
    pub wire_page: u8,
    /// Ordinary decimal display address, when both page nibbles are decimal.
    pub display_page: Option<TeletextPageNumber>,
}

/// A usable language-associated prefix; it is never a complete page inventory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectedTeletextDescriptors {
    /// Entries with a supplied, validated language at the same ordinal.
    pub entries: Vec<ProbedTeletextDescriptor>,
    /// Unpaired bytes remain unassigned rather than receiving guessed languages.
    pub warning: Option<TeletextProjectionWarning>,
}

/// Nonfatal limitation of a useful codec projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextProjectionWarning {
    /// More pairs than languages, as after a shorter n9.0 descriptor replacement.
    /// The tail may be stale; only the paired prefix is returned.
    TrailingUnpairedPairs {
        /// Number of pairs excluded from the useful prefix.
        count: usize,
    },
}

/// Static reason metadata could not be projected; no input is copied into it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextMetadataUnavailable {
    /// None of the three metadata fields was supplied.
    NotProvided,
    /// No language CSV was supplied; languages cannot be guessed.
    MissingLanguage,
    /// No extradata was supplied, even if its size was reported.
    MissingExtradata,
    /// No reported byte count was supplied for verification.
    MissingExtradataSize,
}

/// Redacted problems with a supplied projection, not diagnoses of source media.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextProjectionProblem {
    /// The supplied CSV exceeds the implementation resource bound.
    #[error("teletext language projection exceeds its size limit")]
    LanguageCsvTooLarge,
    /// The supplied bytes or reported size exceed the implementation bound.
    #[error("packed teletext projection exceeds its size limit")]
    PackedDataTooLarge,
    /// The dump text exceeds its bound before decoding or allocating.
    #[error("teletext base64 projection exceeds its size limit")]
    EncodedDataTooLarge,
    /// LF separators or chunk lengths differ from the identified writer format.
    #[error("teletext base64 projection has unsupported chunk framing")]
    InvalidBase64Framing,
    /// Base64 alphabet, canonical padding or unused bits are invalid.
    #[error("teletext base64 projection cannot be decoded")]
    InvalidBase64,
    /// Reported byte count differs from the supplied decoded bytes.
    #[error("teletext projection does not match its reported byte count")]
    ExtradataSizeMismatch,
    /// Packed entries cannot consume an odd byte count.
    #[error("teletext projection contains an incomplete byte pair")]
    OddByteCount,
    /// CSV contains an empty entry or a code other than three ASCII letters.
    #[error("teletext projection contains an unusable language code")]
    InvalidLanguage,
    /// There is no pair for one or more reported languages.
    #[error("teletext projection has more languages than byte pairs")]
    MoreLanguagesThanPairs,
    /// Pairs were supplied without any language entries.
    #[error("teletext projection contains byte pairs without language metadata")]
    EmptyLanguageForPairs,
}

/// Outcome for the additional codec metadata; ordinary scan rows are unaffected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackedTeletextProjection {
    /// Missing metadata, distinct from a supplied empty projection.
    Unavailable(TeletextMetadataUnavailable),
    /// Supplied fields cannot form a useful bounded projection.
    Unusable(TeletextProjectionProblem),
    /// A useful prefix or explicitly empty projection, without completeness proof.
    Available(ProjectedTeletextDescriptors),
}

/// Project explicitly identified `FFmpeg`-packed metadata without opening media.
///
/// Missing fields are unavailable. Explicit empty CSV, empty bytes and size zero
/// produce an empty available projection. Three ASCII language letters are case
/// folded only here; ordinary subtitle metadata is not changed. A shorter CSV
/// returns the useful paired prefix with a warning, never stale-tail languages.
/// All allocations are bounded before consuming CSV or encoded dump text.
#[must_use]
pub fn project_packed_descriptors(
    language_csv: Option<&str>,
    extradata: Option<PackedTeletextData<'_>>,
    extradata_size: Option<usize>,
) -> PackedTeletextProjection {
    if language_csv.is_none() && extradata.is_none() && extradata_size.is_none() {
        return PackedTeletextProjection::Unavailable(TeletextMetadataUnavailable::NotProvided);
    }
    let Some(language_csv) = language_csv else {
        return PackedTeletextProjection::Unavailable(TeletextMetadataUnavailable::MissingLanguage);
    };
    let Some(extradata) = extradata else {
        return PackedTeletextProjection::Unavailable(
            TeletextMetadataUnavailable::MissingExtradata,
        );
    };
    let Some(extradata_size) = extradata_size else {
        return PackedTeletextProjection::Unavailable(
            TeletextMetadataUnavailable::MissingExtradataSize,
        );
    };
    match project_available(language_csv, extradata, extradata_size) {
        Ok(projected) => PackedTeletextProjection::Available(projected),
        Err(problem) => PackedTeletextProjection::Unusable(problem),
    }
}

fn project_available(
    language_csv: &str,
    extradata: PackedTeletextData<'_>,
    extradata_size: usize,
) -> Result<ProjectedTeletextDescriptors, TeletextProjectionProblem> {
    if language_csv.len() > MAX_TELETEXT_LANGUAGE_CSV_BYTES {
        return Err(TeletextProjectionProblem::LanguageCsvTooLarge);
    }
    if extradata_size > MAX_PACKED_TELETEXT_BYTES {
        return Err(TeletextProjectionProblem::PackedDataTooLarge);
    }
    let bytes = packed_bytes(extradata)?;
    if bytes.len() != extradata_size {
        return Err(TeletextProjectionProblem::ExtradataSizeMismatch);
    }
    if bytes.len() % 2 != 0 {
        return Err(TeletextProjectionProblem::OddByteCount);
    }
    let pair_count = bytes.len() / 2;
    if language_csv.is_empty() {
        return if pair_count == 0 {
            Ok(ProjectedTeletextDescriptors {
                entries: Vec::new(),
                warning: None,
            })
        } else {
            Err(TeletextProjectionProblem::EmptyLanguageForPairs)
        };
    }
    let language_count = language_csv.split(',').count();
    if language_count > pair_count {
        return Err(TeletextProjectionProblem::MoreLanguagesThanPairs);
    }
    let mut entries = Vec::with_capacity(language_count);
    for (projection_ordinal, (code, pair)) in language_csv
        .split(',')
        .zip(bytes.chunks_exact(2))
        .enumerate()
    {
        if code.len() != 3 || !code.bytes().all(|byte| byte.is_ascii_alphabetic()) {
            return Err(TeletextProjectionProblem::InvalidLanguage);
        }
        let language = TeletextLanguage::new(code.to_ascii_lowercase())
            .map_err(|_| TeletextProjectionProblem::InvalidLanguage)?;
        let wire_magazine = pair[0] & 7;
        let wire_page = pair[1];
        entries.push(ProbedTeletextDescriptor {
            projection_ordinal,
            language,
            descriptor_type: pair[0] >> 3,
            wire_magazine,
            wire_page,
            display_page: display_page(wire_magazine, wire_page),
        });
    }
    Ok(ProjectedTeletextDescriptors {
        entries,
        warning: (pair_count > language_count).then_some(
            TeletextProjectionWarning::TrailingUnpairedPairs {
                count: pair_count - language_count,
            },
        ),
    })
}

fn display_page(wire_magazine: u8, wire_page: u8) -> Option<TeletextPageNumber> {
    let tens = wire_page >> 4;
    let units = wire_page & 15;
    if tens > 9 || units > 9 {
        return None;
    }
    let magazine = if wire_magazine == 0 { 8 } else { wire_magazine };
    TeletextPageNumber::new(u16::from(magazine) * 100 + u16::from(tens) * 10 + u16::from(units))
        .ok()
}

fn packed_bytes(
    extradata: PackedTeletextData<'_>,
) -> Result<Cow<'_, [u8]>, TeletextProjectionProblem> {
    match extradata {
        PackedTeletextData::Bytes(bytes) => {
            if bytes.len() > MAX_PACKED_TELETEXT_BYTES {
                return Err(TeletextProjectionProblem::PackedDataTooLarge);
            }
            Ok(Cow::Borrowed(bytes))
        }
        PackedTeletextData::Ffprobe9Base64(dump) => decode_ffprobe9_base64(dump).map(Cow::Owned),
    }
}

fn decode_ffprobe9_base64(dump: &str) -> Result<Vec<u8>, TeletextProjectionProblem> {
    if dump.len() > MAX_FFPROBE9_TELETEXT_BASE64_BYTES {
        return Err(TeletextProjectionProblem::EncodedDataTooLarge);
    }
    if dump == "\n" {
        return Ok(Vec::new());
    }
    let body = dump
        .strip_prefix('\n')
        .and_then(|body| body.strip_suffix('\n'))
        .ok_or(TeletextProjectionProblem::InvalidBase64Framing)?;
    let mut lines = body.split('\n').peekable();
    let mut bytes = Vec::new();
    while let Some(line) = lines.next() {
        let final_line = lines.peek().is_none();
        if line.is_empty()
            || line.len() > 80
            || line.len() % 4 != 0
            || (!final_line && line.len() != 80)
        {
            return Err(TeletextProjectionProblem::InvalidBase64Framing);
        }
        let mut chunk = [0; 60];
        let length = STANDARD
            .decode_slice(line, &mut chunk)
            .map_err(|_| TeletextProjectionProblem::InvalidBase64)?;
        if !final_line && length != 60 {
            return Err(TeletextProjectionProblem::InvalidBase64Framing);
        }
        if bytes.len() + length > MAX_PACKED_TELETEXT_BYTES {
            return Err(TeletextProjectionProblem::PackedDataTooLarge);
        }
        bytes.extend_from_slice(&chunk[..length]);
    }
    Ok(bytes)
}

#[cfg(test)]
#[path = "teletext_tests.rs"]
mod tests;
