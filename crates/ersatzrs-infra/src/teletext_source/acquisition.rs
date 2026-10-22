//! Domain projection of an already held native acquisition; no I/O or authority.

use super::{SourceTeletextParents, MAX_SOURCE_TELETEXT_DESCRIPTORS, MAX_SOURCE_TELETEXT_PROGRAMS};
use crate::ffprobe::teletext::TeletextProjectionWarning;
use crate::ffprobe::teletext_probe::{
    ProbedTeletextStream, TeletextProbeDescriptors, TeletextProbePid, TeletextProbePrograms,
};
use crate::ffprobe::LocalTeletextAcquisition;
use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_domain::{
    MediaSourceKind, SourceTeletextBinding, SourceTeletextBindingIdentity,
    SourceTeletextDescriptor, SourceTeletextDescriptors, SourceTeletextObservation,
    SourceTeletextObservedPages, SourceTeletextPid, SourceTeletextProgram, SourceTeletextPrograms,
    SourceTeletextProvenance,
};

/// Bind a native observation to exact fresh rows after the caller has verified
/// provider source identity. This projection alone grants no write authority.
/// # Errors
/// Returns an opaque validation failure for malformed or excessive observations.
pub fn acquired_observation(
    source_id: i64,
    provider: MediaSourceKind,
    parents: SourceTeletextParents,
    acquisition: &LocalTeletextAcquisition,
    stream: &ProbedTeletextStream,
) -> Result<SourceTeletextObservation> {
    let binding = SourceTeletextBindingIdentity::new(
        source_id,
        provider,
        parents.media_version_id,
        parents.media_file_id,
        parents.media_stream_id,
        acquisition.revision().clone(),
    )
    .map_err(|_| conflict("invalid source teletext parent binding"))?;
    let pid = match stream.pid {
        TeletextProbePid::Unavailable => SourceTeletextPid::Unavailable,
        TeletextProbePid::Unusable => SourceTeletextPid::Unusable,
        TeletextProbePid::Available(pid) => SourceTeletextPid::Reported(pid),
    };
    let mut observation = SourceTeletextObservation::new(
        SourceTeletextBinding::Bound(binding),
        stream.stream_index,
        SourceTeletextProvenance::Ffprobe9PackedDescriptorProjection,
        pid,
        programs(&stream.programs)?,
        descriptors(&stream.descriptors)?,
    )
    .map_err(|_| conflict("invalid source teletext projection"))?;
    if let Some(headers) = acquisition.headers().for_stream(stream.stream_index) {
        observation.observed_pages = match acquisition.page_window(stream.stream_index) {
            Some(pages) => SourceTeletextObservedPages::PagesObserved {
                headers: Box::new(headers.clone()),
                pages: Box::new(pages.clone()),
            },
            None => SourceTeletextObservedPages::HeadersObserved(Box::new(headers.clone())),
        };
        observation
            .validate()
            .map_err(|_| conflict("invalid source teletext header observation"))?;
    }
    Ok(observation)
}

fn programs(programs: &TeletextProbePrograms) -> Result<SourceTeletextPrograms> {
    Ok(match programs {
        TeletextProbePrograms::Unavailable(_) => SourceTeletextPrograms::Unavailable,
        TeletextProbePrograms::Unusable(_) => SourceTeletextPrograms::Unusable,
        TeletextProbePrograms::Available(programs) => {
            if programs.len() > MAX_SOURCE_TELETEXT_PROGRAMS {
                return Err(conflict(
                    "source teletext program projection exceeds its bound",
                ));
            }
            SourceTeletextPrograms::Reported(
                programs
                    .iter()
                    .map(|program| SourceTeletextProgram {
                        program_id: program.program_id,
                        program_number: program.program_number,
                        pmt_pid: program.pmt_pid,
                        pcr_pid: program.pcr_pid,
                    })
                    .collect(),
            )
        }
    })
}

fn descriptors(descriptors: &TeletextProbeDescriptors) -> Result<SourceTeletextDescriptors> {
    Ok(match descriptors {
        TeletextProbeDescriptors::Unavailable(_) => SourceTeletextDescriptors::Unavailable,
        TeletextProbeDescriptors::Unusable(_) => SourceTeletextDescriptors::Unusable,
        TeletextProbeDescriptors::Available(descriptors) => {
            if descriptors.entries.len() > MAX_SOURCE_TELETEXT_DESCRIPTORS {
                return Err(conflict(
                    "source teletext descriptor projection exceeds its bound",
                ));
            }
            let entries = descriptors
                .entries
                .iter()
                .map(|entry| {
                    SourceTeletextDescriptor::new(
                        u32::try_from(entry.projection_ordinal)
                            .map_err(|_| conflict("source teletext ordinal exceeds its bound"))?,
                        entry.language.clone(),
                        entry.descriptor_type,
                        entry.wire_magazine,
                        entry.wire_page,
                    )
                    .map_err(|_| conflict("invalid source teletext descriptor"))
                })
                .collect::<Result<Vec<_>>>()?;
            let trailing_unpaired_pairs = descriptors
                .warning
                .map(|warning| match warning {
                    TeletextProjectionWarning::TrailingUnpairedPairs { count } => {
                        u32::try_from(count).map_err(|_| {
                            conflict("source teletext trailing pairs exceed their bound")
                        })
                    }
                })
                .transpose()?;
            SourceTeletextDescriptors::Reported {
                entries,
                trailing_unpaired_pairs,
            }
        }
    })
}

fn conflict(message: &'static str) -> RepositoryError {
    RepositoryError::Conflict(message.to_owned())
}
