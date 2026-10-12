//! Versioned, bounded fingerprint of the exact compared semantic context.
//! Framed fields stream directly to SHA-256; no source text is copied or logged.

use chrono::{DateTime, Duration, Utc};
use ersatzrs_core::{
    programme::{ProgrammeOccurrence, ProgrammeSegment},
    repositories::ArtworkReference,
    xmltv::{
        IconSizeHint, XmltvBlockBehavior, XmltvCredit, XmltvItemKind, XmltvProgramme,
        XmltvProgrammeContent,
    },
};
use ersatzrs_domain::{ArtworkKind, ChannelPlayoutSource, PlayoutScheduleKind};
use sha2::{Digest as _, Sha256};

use super::{
    CapturedGuideChannel, GuideSnapshot, GuideSnapshotPathAssociation, GuideSnapshotSource,
};

/// Stable framing algorithm version, independent of request/response schemas.
pub const GUIDE_SNAPSHOT_DIGEST_VERSION: u16 = 2;

/// Trusted cumulative streaming work limits, not source/publication authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuideSnapshotDigestLimits {
    /// Includes every primitive, optional discriminator, length and prefix field.
    pub max_fields: usize,
    /// Includes type tags, eight-byte lengths and every field's payload bytes.
    pub max_bytes: usize,
}

/// Numeric/static failures never retain input or a partially computed digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum GuideSnapshotDigestError {
    #[error("guide context fingerprint version is unsupported")]
    UnsupportedVersion,
    #[error("guide context fingerprint size arithmetic failed")]
    Arithmetic,
    #[error("guide context fingerprint {dimension} count {count} exceeds limit {maximum}")]
    Limit {
        dimension: &'static str,
        count: usize,
        maximum: usize,
    },
}

type Result<T> = std::result::Result<T, GuideSnapshotDigestError>;

pub(super) fn context_digest(
    snapshot: &GuideSnapshot,
    limits: GuideSnapshotDigestLimits,
) -> Result<[u8; 32]> {
    context_digest_for_version(snapshot, GUIDE_SNAPSHOT_DIGEST_VERSION, limits)
}

pub(super) fn context_digest_for_version(
    snapshot: &GuideSnapshot,
    version: u16,
    limits: GuideSnapshotDigestLimits,
) -> Result<[u8; 32]> {
    if !matches!(version, 1 | 2) {
        return Err(GuideSnapshotDigestError::UnsupportedVersion);
    }
    let mut writer = Writer {
        hash: Sha256::new(),
        limits,
        fields: 0,
        bytes: 0,
        version,
    };
    writer.frame(0, b"ErsatzRS.GuideSnapshot.Context")?;
    writer.frame(9, &version.to_be_bytes())?;
    writer.snapshot(snapshot)?;
    Ok(writer.hash.finalize().into())
}

struct Writer {
    hash: Sha256,
    limits: GuideSnapshotDigestLimits,
    fields: usize,
    bytes: usize,
    version: u16,
}

impl Writer {
    // Each field is tag:u8 || payload_length:u64be || payload. Reserve both
    // counters atomically before even the tag reaches the fixed-stack hasher.
    fn frame(&mut self, tag: u8, payload: &[u8]) -> Result<()> {
        let length =
            u64::try_from(payload.len()).map_err(|_| GuideSnapshotDigestError::Arithmetic)?;
        let fields = self
            .fields
            .checked_add(1)
            .ok_or(GuideSnapshotDigestError::Arithmetic)?;
        let bytes = payload
            .len()
            .checked_add(9)
            .and_then(|n| self.bytes.checked_add(n))
            .ok_or(GuideSnapshotDigestError::Arithmetic)?;
        for (dimension, count, maximum) in [
            ("fields", fields, self.limits.max_fields),
            ("bytes", bytes, self.limits.max_bytes),
        ] {
            if count > maximum {
                return Err(GuideSnapshotDigestError::Limit {
                    dimension,
                    count,
                    maximum,
                });
            }
        }
        self.fields = fields;
        self.bytes = bytes;
        self.hash.update([tag]);
        self.hash.update(length.to_be_bytes());
        self.hash.update(payload);
        Ok(())
    }

    fn string(&mut self, value: &str) -> Result<()> {
        self.frame(1, value.as_bytes())
    }
    fn signed(&mut self, value: i64) -> Result<()> {
        self.frame(2, &value.to_be_bytes())
    }
    fn integer(&mut self, value: i32) -> Result<()> {
        self.frame(3, &value.to_be_bytes())
    }
    fn enumeration(&mut self, tag: u8) -> Result<()> {
        self.frame(8, &[tag])
    }
    fn length(&mut self, value: usize) -> Result<()> {
        let value = u64::try_from(value).map_err(|_| GuideSnapshotDigestError::Arithmetic)?;
        self.frame(5, &value.to_be_bytes())
    }
    fn instant(&mut self, value: &DateTime<Utc>) -> Result<()> {
        let mut bytes = [0; 12];
        bytes[..8].copy_from_slice(&value.timestamp().to_be_bytes());
        bytes[8..].copy_from_slice(&value.timestamp_subsec_nanos().to_be_bytes());
        self.frame(6, &bytes)
    }
    fn duration(&mut self, value: &Duration) -> Result<()> {
        let mut bytes = [0; 12];
        bytes[..8].copy_from_slice(&value.num_seconds().to_be_bytes());
        bytes[8..].copy_from_slice(&value.subsec_nanos().to_be_bytes());
        self.frame(7, &bytes)
    }
    fn option<T: ?Sized>(
        &mut self,
        value: Option<&T>,
        write: fn(&mut Self, &T) -> Result<()>,
    ) -> Result<()> {
        self.frame(4, &[u8::from(value.is_some())])?;
        if let Some(value) = value {
            write(self, value)?;
        }
        Ok(())
    }
    fn optional_string(&mut self, value: Option<&str>) -> Result<()> {
        self.option(value, Self::string)
    }
    fn sequence<T>(&mut self, values: &[T], write: fn(&mut Self, &T) -> Result<()>) -> Result<()> {
        self.length(values.len())?;
        for value in values {
            write(self, value)?;
        }
        Ok(())
    }
    fn strings(&mut self, values: &[String]) -> Result<()> {
        self.sequence(values, |writer, value| writer.string(value))
    }

    fn snapshot(&mut self, value: &GuideSnapshot) -> Result<()> {
        let GuideSnapshot {
            channels,
            page_language,
            raw_block_behavior,
            block_behavior,
            start,
            finish,
            source_usage: _,
            expansion_usage: _,
        } = value;
        self.instant(start)?;
        self.instant(finish)?;
        self.optional_string(page_language.as_deref())?;
        self.optional_string(raw_block_behavior.as_deref())?;
        self.enumeration(match block_behavior {
            XmltvBlockBehavior::SplitTimeEvenly => 0,
            XmltvBlockBehavior::UseActualTimes => 1,
        })?;
        self.sequence(channels, Self::channel)
    }
    fn channel(&mut self, value: &CapturedGuideChannel) -> Result<()> {
        let CapturedGuideChannel {
            id,
            number,
            name,
            playout_source,
            source_channel_id,
            playout_offset,
            occurrences,
            sources,
            path_associations,
        } = value;
        self.signed(*id)?;
        self.string(number)?;
        self.string(name)?;
        self.enumeration(match playout_source {
            ChannelPlayoutSource::Generated => 0,
            ChannelPlayoutSource::Mirror => 1,
        })?;
        self.signed(*source_channel_id)?;
        self.duration(playout_offset)?;
        self.sequence(sources, Self::source)?;
        self.sequence(path_associations, Self::path)?;
        self.sequence(occurrences, Self::occurrence)
    }
    fn source(&mut self, value: &GuideSnapshotSource) -> Result<()> {
        match value {
            GuideSnapshotSource::Sql {
                playout_id,
                schedule_kind,
                raw_rows,
            } => {
                self.enumeration(0)?;
                self.signed(*playout_id)?;
                self.enumeration(match schedule_kind {
                    PlayoutScheduleKind::None => 0,
                    PlayoutScheduleKind::Classic => 1,
                    PlayoutScheduleKind::Block => 2,
                    PlayoutScheduleKind::Sequential => 3,
                    PlayoutScheduleKind::Scripted => 4,
                    PlayoutScheduleKind::ExternalJson => 5,
                })?;
                self.length(*raw_rows)
            }
            GuideSnapshotSource::Accepted {
                playout_id,
                revision,
                raw_programs,
                document_digest,
            } => {
                self.enumeration(1)?;
                self.signed(*playout_id)?;
                self.signed(*revision)?;
                self.length(*raw_programs)?;
                if self.version == 2 {
                    self.frame(10, document_digest)?;
                }
                Ok(())
            }
        }
    }
    fn path(&mut self, value: &GuideSnapshotPathAssociation) -> Result<()> {
        let GuideSnapshotPathAssociation {
            playout_id,
            configured_schedule_file,
            accepted_schedule_file,
        } = value;
        self.signed(*playout_id)?;
        self.optional_string(configured_schedule_file.as_deref())?;
        self.optional_string(accepted_schedule_file.as_deref())
    }
    fn occurrence(&mut self, value: &ProgrammeOccurrence) -> Result<()> {
        let ProgrammeOccurrence {
            playout_id,
            first_item_id,
            programme,
            segments,
        } = value;
        self.signed(*playout_id)?;
        self.signed(*first_item_id)?;
        self.programme(programme)?;
        self.sequence(segments, Self::segment)
    }
    fn programme(&mut self, value: &XmltvProgramme) -> Result<()> {
        let XmltvProgramme {
            channel_number,
            start,
            stop,
            custom_title,
            title,
            content,
        } = value;
        self.string(channel_number)?;
        self.instant(start)?;
        self.instant(stop)?;
        self.optional_string(custom_title.as_deref())?;
        self.string(title)?;
        self.content(content)
    }
    fn segment(&mut self, value: &ProgrammeSegment) -> Result<()> {
        let ProgrammeSegment {
            item_id,
            media_item_id,
            start,
            finish,
            in_point,
            out_point,
        } = value;
        self.signed(*item_id)?;
        self.option(media_item_id.as_ref(), |writer, value| {
            writer.signed(*value)
        })?;
        self.instant(start)?;
        self.instant(finish)?;
        self.duration(in_point)?;
        self.duration(out_point)
    }
    fn content(&mut self, value: &XmltvProgrammeContent) -> Result<()> {
        let XmltvProgrammeContent {
            kind,
            sub_title,
            description,
            year,
            genres,
            tags,
            artist_genres,
            content_rating,
            icon_artwork,
            icon_size_hint,
            thumbnail_artwork,
            thumbnail_size_hint,
            season_number,
            episode_number,
            guids,
            episode_guids,
            track,
            album,
            release_date,
            collection,
            people,
            original_broadcasters,
            broadcasters,
            artists,
            album_artists,
            comment,
            studios,
            directors,
        } = value;
        self.enumeration(match kind {
            XmltvItemKind::Movie => 0,
            XmltvItemKind::Episode => 1,
            XmltvItemKind::MusicVideo => 2,
            XmltvItemKind::Song => 3,
            XmltvItemKind::OtherVideo => 4,
            XmltvItemKind::RemoteStream => 5,
        })?;
        for value in [
            sub_title,
            description,
            content_rating,
            track,
            album,
            release_date,
            collection,
            comment,
        ] {
            self.optional_string(value.as_deref())?;
        }
        for value in [year, season_number, episode_number] {
            self.option(value.as_ref(), |writer, value| writer.integer(*value))?;
        }
        for values in [
            genres,
            tags,
            artist_genres,
            guids,
            episode_guids,
            original_broadcasters,
            broadcasters,
            artists,
            album_artists,
            studios,
            directors,
        ] {
            self.strings(values)?;
        }
        self.option(icon_artwork.as_ref(), Self::artwork)?;
        self.hint(*icon_size_hint)?;
        self.option(thumbnail_artwork.as_ref(), Self::artwork)?;
        self.hint(*thumbnail_size_hint)?;
        self.sequence(people, Self::credit)
    }
    fn hint(&mut self, value: IconSizeHint) -> Result<()> {
        self.enumeration(match value {
            IconSizeHint::Poster => 0,
            IconSizeHint::Thumbnail => 1,
        })
    }
    fn artwork(&mut self, value: &ArtworkReference) -> Result<()> {
        let ArtworkReference { id, kind, path } = value;
        self.signed(*id)?;
        self.enumeration(match kind {
            ArtworkKind::Poster => 0,
            ArtworkKind::Thumbnail => 1,
            ArtworkKind::Logo => 2,
            ArtworkKind::FanArt => 3,
            ArtworkKind::Watermark => 4,
        })?;
        self.string(path)
    }
    fn credit(&mut self, value: &XmltvCredit) -> Result<()> {
        let XmltvCredit { name, role } = value;
        self.string(name)?;
        self.string(role)
    }
}

#[cfg(test)]
#[path = "digest_tests.rs"]
mod tests;
