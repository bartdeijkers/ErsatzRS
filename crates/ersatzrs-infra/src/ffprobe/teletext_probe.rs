//! Ephemeral associations from explicitly identified `FFprobe` 9 metadata.
//!
//! This projects a supplied MPEG-TS snapshot, not wire descriptors or a complete
//! service/page inventory. Source revision, provider and media-version identity
//! belong to the scanner; subcode, charset and region remain unknown here.
//! Ordinary stream language/dispositions and subtitle rows are not rewritten.
//!
//! n9.0 [ffprobe.c](https://github.com/FFmpeg/FFmpeg/blob/n9.0/fftools/ffprobe.c#L1888)
//! emits stream IDs as hexadecimal strings, extradata sizes only when positive,
//! and program identity plus member streams using their global stream indexes.
//! Missing sizes are not guessed from an empty dump. Nested program streams
//! provide associations only; their other metadata does not replace global rows.
//!
//! Resource caps below bound auxiliary projection after ordinary JSON DTO
//! deserialization. They do not bound JSON input allocations or process RSS.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use super::teletext::{
    project_packed_descriptors, PackedTeletextData, PackedTeletextProjection,
    ProjectedTeletextDescriptors, TeletextMetadataUnavailable, TeletextProjectionProblem,
    MAX_PACKED_TELETEXT_BYTES,
};
use super::{FfprobeOutput, FfprobeStream};

/// Resource policy for auxiliary projection, not a DVB service/stream limit.
pub const MAX_TELETEXT_PROBE_STREAMS: usize = 256;
/// Resource policy for supplied program rows, including unrelated programs.
pub const MAX_TELETEXT_PROBE_PROGRAMS: usize = 128;
/// Bounds both supplied memberships and cloned teletext/program associations.
pub const MAX_TELETEXT_PROBE_MEMBERSHIPS: usize = 1_024;
/// Total supplied packed pairs, including potentially stale unpaired tails.
pub const MAX_TELETEXT_PROBE_DESCRIPTOR_PAIRS: usize = 4_096;

/// Caller identification of the dump writer; JSON cannot identify it itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextProbeEncoding {
    /// Output explicitly requested with n9.0 `-data_dump_format base64`.
    Ffprobe9Base64,
}

/// Static reasons an auxiliary snapshot cannot be bound to fresh stream indexes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextProbeUnavailable {
    MissingContainer,
    UnsupportedContainer,
    MissingStreamIndex,
    MissingCodecType,
}

/// Static failures of supplied auxiliary identity or resource policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextProbeProblem {
    StreamLimit,
    InvalidStreamIndex,
    DuplicateStreamIndex,
    UnexpectedCodecType,
    DescriptorTotalLimit,
}

/// Additional metadata is isolated from the ordinary media-version projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TeletextProbeSnapshot {
    Unavailable(TeletextProbeUnavailable),
    Unusable(TeletextProbeProblem),
    /// Only explicitly named `dvb_teletext` streams from this supplied snapshot.
    Available(Vec<ProbedTeletextStream>),
}

/// Reported PES identity; absent, control/reserved and null-packet PIDs are explicit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextProbePid {
    Unavailable,
    Unusable,
    Available(u16),
}

/// The separately reported program fields, without inferred service provenance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProbedTeletextProgram {
    pub program_id: u32,
    pub program_number: u32,
    pub pmt_pid: u16,
    pub pcr_pid: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextProgramsUnavailable {
    NotProvided,
    MissingIdentityOrMembership,
    NoReportedMembership,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextProgramsProblem {
    InvalidShape,
    InvalidIdentity,
    UnknownStreamIndex,
    DuplicateMembership,
    ConflictingProgramIdentity,
    ProgramLimit,
    MembershipLimit,
}

/// Every supplied membership is preserved; the first program is never selected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TeletextProbePrograms {
    Unavailable(TeletextProgramsUnavailable),
    Unusable(TeletextProgramsProblem),
    Available(Vec<ProbedTeletextProgram>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextDescriptorProblem {
    InvalidExtradataShape,
    InvalidSizeShape,
    Packed(TeletextProjectionProblem),
}

/// Same packed-parser distinctions plus malformed optional JSON field shapes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TeletextProbeDescriptors {
    Unavailable(TeletextMetadataUnavailable),
    Unusable(TeletextDescriptorProblem),
    Available(ProjectedTeletextDescriptors),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbedTeletextStream {
    /// Nonnegative global index, unique across every supplied global stream row.
    pub stream_index: i32,
    pub pid: TeletextProbePid,
    pub programs: TeletextProbePrograms,
    pub descriptors: TeletextProbeDescriptors,
}

pub(super) fn project(
    probe: &FfprobeOutput,
    encoding: TeletextProbeEncoding,
) -> TeletextProbeSnapshot {
    let Some(container) = probe
        .format
        .as_ref()
        .and_then(|format| format.format_name.as_deref())
    else {
        return TeletextProbeSnapshot::Unavailable(TeletextProbeUnavailable::MissingContainer);
    };
    if !container.split(',').any(|name| name == "mpegts") {
        return TeletextProbeSnapshot::Unavailable(TeletextProbeUnavailable::UnsupportedContainer);
    }
    if probe.streams.len() > MAX_TELETEXT_PROBE_STREAMS {
        return TeletextProbeSnapshot::Unusable(TeletextProbeProblem::StreamLimit);
    }
    let indexes = match global_indexes(&probe.streams) {
        Ok(indexes) => indexes,
        Err(snapshot) => return snapshot,
    };
    let mut pairs = 0;
    for stream in probe.streams.iter().filter(|stream| is_teletext(stream)) {
        match stream.codec_type.as_deref() {
            None => {
                return TeletextProbeSnapshot::Unavailable(
                    TeletextProbeUnavailable::MissingCodecType,
                )
            }
            Some("subtitle") => {}
            Some(_) => {
                return TeletextProbeSnapshot::Unusable(TeletextProbeProblem::UnexpectedCodecType)
            }
        }
        // Only sizes the packed parser can decode contribute allocations. Bad
        // or excessive per-stream sizes become local unusable metadata below.
        if let Some(size) = stream.extradata_size.as_ref().and_then(Value::as_u64) {
            if size <= MAX_PACKED_TELETEXT_BYTES as u64 {
                // This value is already bounded by the small packed-byte cap.
                pairs += usize::try_from(size)
                    .expect("bounded packed size")
                    .div_ceil(2);
                if pairs > MAX_TELETEXT_PROBE_DESCRIPTOR_PAIRS {
                    return TeletextProbeSnapshot::Unusable(
                        TeletextProbeProblem::DescriptorTotalLimit,
                    );
                }
            }
        }
    }
    let programs = program_memberships(probe.programs.as_ref(), &indexes);
    let duplicate_pids = duplicate_pids(&probe.streams);
    let mut streams = Vec::new();
    for stream in probe.streams.iter().filter(|stream| is_teletext(stream)) {
        let Some(stream_index) = stream.index else {
            return TeletextProbeSnapshot::Unavailable(
                TeletextProbeUnavailable::MissingStreamIndex,
            );
        };
        let pid = match stream_pid(stream.id.as_ref()) {
            TeletextProbePid::Available(pid) if duplicate_pids.contains(&pid) => {
                TeletextProbePid::Unusable
            }
            pid => pid,
        };
        streams.push(ProbedTeletextStream {
            stream_index,
            pid,
            programs: programs.for_stream(stream_index),
            descriptors: descriptors(stream, encoding),
        });
    }
    TeletextProbeSnapshot::Available(streams)
}

fn is_teletext(stream: &FfprobeStream) -> bool {
    stream.codec_name.as_deref() == Some("dvb_teletext")
}

fn global_indexes(streams: &[FfprobeStream]) -> Result<BTreeSet<i32>, TeletextProbeSnapshot> {
    let mut indexes = BTreeSet::new();
    for stream in streams {
        let Some(index) = stream.index else {
            return Err(TeletextProbeSnapshot::Unavailable(
                TeletextProbeUnavailable::MissingStreamIndex,
            ));
        };
        if index < 0 {
            return Err(TeletextProbeSnapshot::Unusable(
                TeletextProbeProblem::InvalidStreamIndex,
            ));
        }
        if !indexes.insert(index) {
            return Err(TeletextProbeSnapshot::Unusable(
                TeletextProbeProblem::DuplicateStreamIndex,
            ));
        }
    }
    Ok(indexes)
}

fn stream_pid(value: Option<&Value>) -> TeletextProbePid {
    let Some(value) = value else {
        return TeletextProbePid::Unavailable;
    };
    let parsed = value.as_str().and_then(|id| id.strip_prefix("0x"))
        .filter(|digits| {
            !digits.is_empty()
                && digits.len() <= 4
                && digits.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
        .and_then(|digits| u16::from_str_radix(digits, 16).ok())
        // These are control/reserved or null-packet PIDs, not usable PES
        // identity. Do not impose additional DVB PID allocation conventions.
        .filter(|pid| (0x0010..0x1fff).contains(pid));
    parsed.map_or(TeletextProbePid::Unusable, TeletextProbePid::Available)
}

fn duplicate_pids(streams: &[FfprobeStream]) -> BTreeSet<u16> {
    let mut seen = BTreeSet::new();
    let mut duplicates = BTreeSet::new();
    for stream in streams {
        if let TeletextProbePid::Available(pid) = stream_pid(stream.id.as_ref()) {
            if !seen.insert(pid) {
                duplicates.insert(pid);
            }
        }
    }
    duplicates
}

fn descriptors(
    stream: &FfprobeStream,
    encoding: TeletextProbeEncoding,
) -> TeletextProbeDescriptors {
    let dump = match stream.extradata.as_ref() {
        None => None,
        Some(Value::String(dump)) => Some(match encoding {
            TeletextProbeEncoding::Ffprobe9Base64 => PackedTeletextData::Ffprobe9Base64(dump),
        }),
        Some(_) => {
            return TeletextProbeDescriptors::Unusable(
                TeletextDescriptorProblem::InvalidExtradataShape,
            )
        }
    };
    let size = match stream.extradata_size.as_ref() {
        None => None,
        Some(size) => {
            let Some(size) = size.as_u64() else {
                return TeletextProbeDescriptors::Unusable(
                    TeletextDescriptorProblem::InvalidSizeShape,
                );
            };
            if size > MAX_PACKED_TELETEXT_BYTES as u64 {
                return TeletextProbeDescriptors::Unusable(TeletextDescriptorProblem::Packed(
                    TeletextProjectionProblem::PackedDataTooLarge,
                ));
            }
            Some(usize::try_from(size).expect("bounded packed size"))
        }
    };
    match project_packed_descriptors(
        stream
            .tags
            .as_ref()
            .and_then(|tags| tags.language.as_deref()),
        dump,
        size,
    ) {
        PackedTeletextProjection::Unavailable(reason) => {
            TeletextProbeDescriptors::Unavailable(reason)
        }
        PackedTeletextProjection::Unusable(problem) => {
            TeletextProbeDescriptors::Unusable(TeletextDescriptorProblem::Packed(problem))
        }
        PackedTeletextProjection::Available(projected) => {
            TeletextProbeDescriptors::Available(projected)
        }
    }
}

enum ProgramMemberships {
    Unavailable(TeletextProgramsUnavailable),
    Unusable(TeletextProgramsProblem),
    Available(BTreeMap<i32, Vec<ProbedTeletextProgram>>),
}

impl ProgramMemberships {
    fn for_stream(&self, index: i32) -> TeletextProbePrograms {
        match self {
            Self::Unavailable(reason) => TeletextProbePrograms::Unavailable(*reason),
            Self::Unusable(problem) => TeletextProbePrograms::Unusable(*problem),
            Self::Available(memberships) => memberships.get(&index).map_or(
                TeletextProbePrograms::Unavailable(
                    TeletextProgramsUnavailable::NoReportedMembership,
                ),
                |programs| TeletextProbePrograms::Available(programs.clone()),
            ),
        }
    }
}

fn program_memberships(raw: Option<&Value>, indexes: &BTreeSet<i32>) -> ProgramMemberships {
    let Some(raw) = raw else {
        return ProgramMemberships::Unavailable(TeletextProgramsUnavailable::NotProvided);
    };
    let Some(programs) = raw.as_array() else {
        return ProgramMemberships::Unusable(TeletextProgramsProblem::InvalidShape);
    };
    if programs.len() > MAX_TELETEXT_PROBE_PROGRAMS {
        return ProgramMemberships::Unusable(TeletextProgramsProblem::ProgramLimit);
    }
    let mut membership_count = 0usize;
    // Check the complete supplied membership budget before constructing maps.
    for program in programs {
        match program.get("streams") {
            None | Some(Value::Null) => {
                return ProgramMemberships::Unavailable(
                    TeletextProgramsUnavailable::MissingIdentityOrMembership,
                )
            }
            Some(Value::Array(streams)) => {
                if streams.len() > MAX_TELETEXT_PROBE_MEMBERSHIPS - membership_count {
                    return ProgramMemberships::Unusable(TeletextProgramsProblem::MembershipLimit);
                }
                membership_count += streams.len();
            }
            Some(_) => return ProgramMemberships::Unusable(TeletextProgramsProblem::InvalidShape),
        }
    }
    let mut identities = BTreeMap::new();
    let mut numbers = BTreeSet::new();
    let mut memberships: BTreeMap<i32, Vec<ProbedTeletextProgram>> = BTreeMap::new();
    for program in programs {
        let identity = match program_identity(program) {
            Ok(identity) => identity,
            Err(problem) => return problem,
        };
        // FFmpeg's MPEG-TS PAT parser sets both ID and number from the same
        // service number. Distinct records claiming that number are ambiguous;
        // no source-service provenance is inferred from either field.
        if identities.insert(identity.program_id, identity).is_some()
            || !numbers.insert(identity.program_number)
        {
            return ProgramMemberships::Unusable(
                TeletextProgramsProblem::ConflictingProgramIdentity,
            );
        }
        let mut seen = BTreeSet::new();
        // The budget pass established an array. Invalid shapes still return a
        // typed problem instead of affecting ordinary stream deserialization.
        let Some(streams) = program.get("streams").and_then(Value::as_array) else {
            return ProgramMemberships::Unusable(TeletextProgramsProblem::InvalidShape);
        };
        for stream in streams {
            let Some(index) = stream
                .get("index")
                .and_then(Value::as_i64)
                .and_then(|index| i32::try_from(index).ok())
            else {
                return ProgramMemberships::Unusable(TeletextProgramsProblem::InvalidShape);
            };
            if !indexes.contains(&index) {
                return ProgramMemberships::Unusable(TeletextProgramsProblem::UnknownStreamIndex);
            }
            if !seen.insert(index) {
                return ProgramMemberships::Unusable(TeletextProgramsProblem::DuplicateMembership);
            }
            memberships.entry(index).or_default().push(identity);
        }
    }
    ProgramMemberships::Available(memberships)
}

fn program_identity(program: &Value) -> Result<ProbedTeletextProgram, ProgramMemberships> {
    for field in ["program_id", "program_num", "pmt_pid", "pcr_pid"] {
        if program.get(field).is_none_or(Value::is_null) {
            return Err(ProgramMemberships::Unavailable(
                TeletextProgramsUnavailable::MissingIdentityOrMembership,
            ));
        }
    }
    let integer = |field| {
        program
            .get(field)
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
    };
    let pid = |field| {
        program
            .get(field)
            .and_then(Value::as_u64)
            .filter(|pid| *pid <= 0x1fff)
            .and_then(|pid| u16::try_from(pid).ok())
    };
    let (Some(program_id), Some(program_number), Some(pmt_pid), Some(pcr_pid)) = (
        integer("program_id"),
        integer("program_num"),
        pid("pmt_pid"),
        pid("pcr_pid"),
    ) else {
        return Err(ProgramMemberships::Unusable(
            TeletextProgramsProblem::InvalidIdentity,
        ));
    };
    // n9.0 ignores PAT PMT entries with control/reserved or null PIDs. PCR
    // retains the reported null PID: unlike a PES PID, that can mean no PCR.
    if !(0x0010..0x1fff).contains(&pmt_pid) {
        return Err(ProgramMemberships::Unusable(
            TeletextProgramsProblem::InvalidIdentity,
        ));
    }
    Ok(ProbedTeletextProgram {
        program_id,
        program_number,
        pmt_pid,
        pcr_pid,
    })
}

#[cfg(test)]
#[path = "teletext_probe_tests.rs"]
mod tests;
