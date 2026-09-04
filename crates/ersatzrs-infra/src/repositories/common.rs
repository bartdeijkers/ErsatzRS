use crate::db::DbRow;
use crate::repositories::SqlxResultExt as _;
use chrono::{DateTime, Duration, Utc};
use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_domain::{
    AddonMediaListArtifactKind, AddonMediaListArtifactState, AddonMediaListAvailabilityReason,
    AddonMediaListContentKind, AddonMediaListDetailPolicy, AddonMediaListDetailState,
    AddonMediaListImportMode, AddonMediaListImportPurpose, AddonMediaListImportState,
    AddonMediaListIncrementalStrategy, AddonMediaListItemAvailability, AddonMediaListItemKind,
    AddonMediaListItemRole, AddonMediaListLifecycleJobKind, AddonMediaListLifecyclePhase,
    AddonMediaListPresentationKind, AddonMediaListPublicationEnqueueTrigger,
    AddonMediaListPublicationErrorCategory, AddonMediaListPublicationEventKind,
    AddonMediaListPublicationFilesystemTransition, AddonMediaListPublicationOutcome,
    AddonMediaListPublicationPhase, AddonMediaListPublicationRetryClass,
    AddonMediaListRecoveryAction, AddonMediaListRefreshCadence,
    AddonMediaListRefreshPublicationPolicy, AddonMediaListReviewState, AddonToolMaintenanceOutcome,
    AddonToolMaintenanceRollback, AddonToolMaintenanceState, AddonToolMaintenanceTrigger,
    AddonToolMaintenanceValidation, ArtworkKind, BlockStopScheduling, ChannelIdleBehavior,
    ChannelMusicVideoCreditsMode, ChannelPlayoutMode, ChannelPlayoutSource, ChannelSongVideoMode,
    ChannelStreamSelectorMode, ChannelSubtitleMode, ChannelTranscodeMode,
    ChannelWatermarkImageSource, ChannelWatermarkMode, CollectionType, ContentKind,
    DecoBreakPlacement, DecoMode, FFmpegProfileAudioFormat, FFmpegProfileBitDepth,
    FFmpegProfileTonemapAlgorithm, FFmpegProfileVideoFormat, FillWithGroupMode, FillerKind,
    FillerMode, FilterMode, FixedStartTimeBehavior, GraphicsElementKind, GuideMode,
    HardwareAccelerationKind, LibraryMediaKind, ManagedLibraryOperationKind,
    ManagedLibraryOperationState, MarathonGroupBy, MediaItemState, MediaSourceKind,
    MediaStreamKind, MetadataKind, MetadataSource, MultipleMode, NormalizeLoudnessMode,
    OutputDynamicRange, PlaybackOrder, PlayoutMode, PlayoutScheduleKind, RemoteStreamLiveness,
    ScalingBehavior, StartType, StreamingMode, SubtitleKind, TailMode, VaapiDriver, VideoScanKind,
    WatermarkLocation, WatermarkSize,
};
use serde::de::DeserializeOwned;
use sqlx::{Pool, Row};
use std::fmt::Write as _;

pub(crate) fn map_data_access_error(kind: &'static str, error: &sqlx::Error) -> RepositoryError {
    if matches!(
        &error,
        sqlx::Error::ColumnIndexOutOfBounds { .. }
            | sqlx::Error::ColumnNotFound(_)
            | sqlx::Error::ColumnDecode { .. }
            | sqlx::Error::Decode(_)
    ) {
        RepositoryError::Decode {
            kind,
            value: error.to_string(),
        }
    } else {
        RepositoryError::unavailable(kind, error.to_string())
    }
}

pub(crate) fn map_database_error(error: &sqlx::Error) -> RepositoryError {
    map_data_access_error("database operation", error)
}

pub fn decode_enum<T>(kind: &'static str, value: &str) -> Result<T>
where
    T: DbEnum,
{
    if let Ok(decoded) = serde_json::from_str(&format!("\"{value}\"")) {
        return Ok(decoded);
    }

    let int_value = value
        .parse::<i32>()
        .map_err(|source| decode_message(kind, &source.to_string()))?;

    T::from_db_int(int_value)
        .ok_or_else(|| decode_message(kind, &format!("unknown enum value {int_value}")))
}

pub fn decode_enum_column<T>(kind: &'static str, row: &DbRow, column: &'static str) -> Result<T>
where
    T: DbEnum,
{
    if let Ok(value) = row.try_get::<i32, _>(column) {
        T::from_db_int(value)
            .ok_or_else(|| decode_message(kind, &format!("unknown enum value {value}")))
    } else {
        let value = row.try_get::<String, _>(column).repository()?;
        decode_enum(kind, &value)
    }
}

pub fn decode_optional_enum_column<T>(
    kind: &'static str,
    row: &DbRow,
    column: &'static str,
) -> Result<Option<T>>
where
    T: DbEnum,
{
    match row.try_get::<Option<i32>, _>(column) {
        Ok(Some(value)) => T::from_db_int(value)
            .ok_or_else(|| decode_message(kind, &format!("unknown enum value {value}")))
            .map(Some),
        Ok(None) => Ok(None),
        Err(_) => row
            .try_get::<Option<String>, _>(column)
            .repository()?
            .map(|value| decode_enum(kind, &value))
            .transpose(),
    }
}

pub fn decode_bool_column(row: &DbRow, column: &'static str) -> Result<bool> {
    if let Ok(value) = row.try_get(column) {
        return Ok(value);
    }

    Ok(row.try_get::<i64, _>(column).repository()? != 0)
}

pub fn decode_optional_bool_column(row: &DbRow, column: &'static str) -> Result<Option<bool>> {
    if let Ok(value) = row.try_get(column) {
        return Ok(value);
    }

    Ok(row
        .try_get::<Option<i64>, _>(column)
        .repository()?
        .map(|value| value != 0))
}

pub fn decode_string_column(row: &DbRow, column: &'static str) -> Result<String> {
    if let Ok(value) = row.try_get(column) {
        return Ok(value);
    }

    decode_utf8_column(row.try_get(column).repository()?, column)
}

pub fn decode_optional_string_column(row: &DbRow, column: &'static str) -> Result<Option<String>> {
    if let Ok(value) = row.try_get(column) {
        return Ok(value);
    }

    row.try_get::<Option<Vec<u8>>, _>(column)
        .repository()?
        .map(|bytes| decode_utf8_column(bytes, column))
        .transpose()
}

fn decode_utf8_column(bytes: Vec<u8>, column: &'static str) -> Result<String> {
    String::from_utf8(bytes).map_err(|error| RepositoryError::Decode {
        kind: column,
        value: error.to_string(),
    })
}

pub trait DbEnum: DeserializeOwned + Sized {
    fn from_db_int(value: i32) -> Option<Self>;
    fn to_db_int(self) -> i32;
}

#[must_use]
pub fn encode_enum<T>(value: T) -> i32
where
    T: DbEnum,
{
    value.to_db_int()
}

macro_rules! db_enum {
    ($ty:ty { $($value:literal => $variant:path),+ $(,)? }) => {
        impl DbEnum for $ty {
            fn from_db_int(value: i32) -> Option<Self> {
                match value {
                    $($value => Some($variant),)+
                    _ => None,
                }
            }

            fn to_db_int(self) -> i32 {
                match self {
                    $($variant => $value,)+
                }
            }
        }
    };
}

pub fn decode_datetime(kind: &'static str, value: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|parsed| parsed.with_timezone(&Utc))
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f")
                .map(|parsed| parsed.and_utc())
        })
        .or_else(|_| {
            chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .map(|parsed| parsed.and_hms_opt(0, 0, 0).unwrap().and_utc())
        })
        .map_err(|source| decode_message(kind, &source.to_string()))
}

pub fn decode_optional_datetime(
    kind: &'static str,
    value: Option<String>,
) -> Result<Option<DateTime<Utc>>> {
    value
        .filter(|inner| !inner.trim().is_empty())
        .map(|inner| decode_datetime(kind, &inner))
        .transpose()
}

#[must_use]
pub fn encode_datetime(value: DateTime<Utc>) -> String {
    let naive = value.naive_utc();
    let base = naive.format("%Y-%m-%d %H:%M:%S").to_string();
    let ticks = naive.and_utc().timestamp_subsec_nanos() / 100;
    if ticks == 0 {
        base
    } else {
        format!("{base}.{ticks:07}")
    }
}

#[must_use]
pub fn encode_optional_datetime(value: Option<DateTime<Utc>>) -> Option<String> {
    value.map(encode_datetime)
}

/// Encode a [`Duration`] as an ISO 8601 time span string (`HH:MM:SS`).
#[must_use]
pub fn encode_duration(value: Duration) -> String {
    let total_seconds = value.num_seconds().max(0);
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}

/// Encode a signed [`Duration`] as an ErsatzTV time span (`[-]HH:MM:SS`).
#[must_use]
pub fn encode_signed_duration(value: Duration) -> String {
    let total_seconds = value.num_seconds();
    let sign = if total_seconds < 0 { "-" } else { "" };
    let total_seconds = total_seconds.unsigned_abs();
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    format!("{sign}{hours:02}:{minutes:02}:{seconds:02}")
}

/// Decode an `HH:MM:SS` (or similar) TEXT duration stored by `ErsatzTV`.
#[must_use]
pub fn decode_duration_text(value: &str) -> Duration {
    try_decode_duration_text(value).unwrap_or_else(|_| Duration::zero())
}

/// Strictly decode an `HH:MM:SS` (or similar) duration stored by `ErsatzTV`.
///
/// # Errors
///
/// Returns a decode error for malformed, negative, or overflowing values.
pub fn try_decode_duration_text(value: &str) -> Result<Duration> {
    let duration = try_decode_signed_duration_text(value)?;
    if duration < Duration::zero() {
        return Err(decode_message("duration", "value must be non-negative"));
    }
    Ok(duration)
}

/// Strictly decode a signed ErsatzTV time span (`[-]HH:MM:SS`).
///
/// # Errors
///
/// Returns a decode error for malformed or overflowing values.
pub fn try_decode_signed_duration_text(value: &str) -> Result<Duration> {
    let (sign, value) = value
        .strip_prefix('-')
        .map_or((1_i32, value), |unsigned| (-1_i32, unsigned));
    if value.starts_with('+') || value.is_empty() {
        return Err(decode_message("duration", "expected [-]HH:MM:SS"));
    }
    let (day_text, time_text) = match (value.find('.'), value.find(':')) {
        (Some(day_separator), Some(first_colon)) if day_separator < first_colon => {
            (&value[..day_separator], &value[day_separator + 1..])
        }
        _ => ("", value),
    };
    let parts: Vec<&str> = time_text.split(':').collect();
    let [hours_text, minutes_text, seconds_text] = parts.as_slice() else {
        return Err(decode_message("duration", "expected HH:MM:SS"));
    };

    let parse_component = |label: &'static str, text: &str| {
        text.parse::<i64>()
            .map_err(|error| decode_message(label, &error.to_string()))
    };
    let days = if day_text.is_empty() {
        0
    } else {
        parse_component("duration days", day_text)?
    };
    let hours = parse_component("duration hours", hours_text)?;
    let minutes = parse_component("duration minutes", minutes_text)?;
    let (seconds_text, nanos) = seconds_text.split_once('.').map_or_else(
        || Ok((*seconds_text, 0)),
        |(whole, fraction)| {
            if fraction.is_empty()
                || fraction.len() > 9
                || !fraction.bytes().all(|byte| byte.is_ascii_digit())
            {
                return Err(decode_message(
                    "duration fraction",
                    "expected one to nine decimal digits",
                ));
            }
            let padded = format!("{fraction:0<9}");
            let nanos = padded
                .parse::<i64>()
                .map_err(|error| decode_message("duration fraction", &error.to_string()))?;
            Ok((whole, nanos))
        },
    )?;
    let seconds = parse_component("duration seconds", seconds_text)?;
    if days < 0 || hours < 0 || !(0..60).contains(&minutes) || !(0..60).contains(&seconds) {
        return Err(decode_message(
            "duration",
            "components must be non-negative and minutes/seconds below 60",
        ));
    }

    let duration = Duration::try_days(days)
        .and_then(|value| value.checked_add(&Duration::try_hours(hours)?))
        .and_then(|value| value.checked_add(&Duration::try_minutes(minutes)?))
        .and_then(|value| value.checked_add(&Duration::try_seconds(seconds)?))
        .and_then(|value| value.checked_add(&Duration::nanoseconds(nanos)))
        .ok_or_else(|| decode_message("duration", "value overflows supported range"))?;
    Ok(if sign < 0 { -duration } else { duration })
}

db_enum!(ArtworkKind {
    0 => ArtworkKind::Poster,
    1 => ArtworkKind::Thumbnail,
    2 => ArtworkKind::Logo,
    3 => ArtworkKind::FanArt,
    4 => ArtworkKind::Watermark,
});
db_enum!(BlockStopScheduling {
    0 => BlockStopScheduling::AfterDurationEnd,
    1 => BlockStopScheduling::BeforeDurationEnd,
});
db_enum!(ChannelIdleBehavior {
    0 => ChannelIdleBehavior::StopOnDisconnect,
    1 => ChannelIdleBehavior::KeepRunning,
});
db_enum!(ChannelMusicVideoCreditsMode {
    0 => ChannelMusicVideoCreditsMode::None,
    1 => ChannelMusicVideoCreditsMode::GenerateSubtitles,
});
db_enum!(ChannelPlayoutMode {
    0 => ChannelPlayoutMode::Continuous,
    1 => ChannelPlayoutMode::OnDemand,
});
db_enum!(ChannelPlayoutSource {
    0 => ChannelPlayoutSource::Generated,
    1 => ChannelPlayoutSource::Mirror,
});
db_enum!(ChannelSongVideoMode {
    0 => ChannelSongVideoMode::Default,
    1 => ChannelSongVideoMode::WithProgress,
});
db_enum!(ChannelStreamSelectorMode {
    0 => ChannelStreamSelectorMode::Default,
    1 => ChannelStreamSelectorMode::Custom,
    100 => ChannelStreamSelectorMode::Troubleshooting,
});
db_enum!(ChannelSubtitleMode {
    0 => ChannelSubtitleMode::None,
    1 => ChannelSubtitleMode::Forced,
    2 => ChannelSubtitleMode::Default,
    3 => ChannelSubtitleMode::Any,
});
db_enum!(ChannelTranscodeMode {
    0 => ChannelTranscodeMode::OnDemand,
});
db_enum!(ChannelWatermarkImageSource {
    0 => ChannelWatermarkImageSource::Custom,
    1 => ChannelWatermarkImageSource::ChannelLogo,
    100 => ChannelWatermarkImageSource::Resource,
});
db_enum!(ChannelWatermarkMode {
    0 => ChannelWatermarkMode::None,
    1 => ChannelWatermarkMode::Permanent,
    2 => ChannelWatermarkMode::Intermittent,
    3 => ChannelWatermarkMode::OpacityExpression,
});
db_enum!(CollectionType {
    0 => CollectionType::Collection,
    1 => CollectionType::TelevisionShow,
    2 => CollectionType::TelevisionSeason,
    3 => CollectionType::Artist,
    4 => CollectionType::MultiCollection,
    5 => CollectionType::SmartCollection,
    6 => CollectionType::Playlist,
    7 => CollectionType::RerunFirstRun,
    8 => CollectionType::RerunRerun,
    9 => CollectionType::SearchQuery,
    10 => CollectionType::Movie,
    20 => CollectionType::Episode,
    30 => CollectionType::MusicVideo,
    40 => CollectionType::OtherVideo,
    50 => CollectionType::Song,
    60 => CollectionType::Image,
    70 => CollectionType::RemoteStream,
    100 => CollectionType::FakeCollection,
    101 => CollectionType::FakePlaylistItem,
});
db_enum!(ContentKind {
    0 => ContentKind::Primary,
    1 => ContentKind::Extra,
    2 => ContentKind::AlternateVersion,
});
db_enum!(DecoBreakPlacement {
    0 => DecoBreakPlacement::BlockStart,
    1 => DecoBreakPlacement::BlockFinish,
    2 => DecoBreakPlacement::BetweenBlockItems,
    3 => DecoBreakPlacement::ChapterMarkers,
});
db_enum!(DecoMode {
    0 => DecoMode::Inherit,
    1 => DecoMode::Disable,
    2 => DecoMode::Override,
    3 => DecoMode::Merge,
});
db_enum!(FFmpegProfileAudioFormat {
    0 => FFmpegProfileAudioFormat::None,
    1 => FFmpegProfileAudioFormat::Aac,
    2 => FFmpegProfileAudioFormat::Ac3,
    3 => FFmpegProfileAudioFormat::AacLatm,
    99 => FFmpegProfileAudioFormat::Copy,
});
db_enum!(FFmpegProfileBitDepth {
    0 => FFmpegProfileBitDepth::EightBit,
    1 => FFmpegProfileBitDepth::TenBit,
});
db_enum!(OutputDynamicRange {
    0 => OutputDynamicRange::SdrBt709,
    1 => OutputDynamicRange::Hdr10Bt2020Pq,
});
db_enum!(FFmpegProfileTonemapAlgorithm {
    0 => FFmpegProfileTonemapAlgorithm::Linear,
    1 => FFmpegProfileTonemapAlgorithm::Clip,
    2 => FFmpegProfileTonemapAlgorithm::Gamma,
    3 => FFmpegProfileTonemapAlgorithm::Reinhard,
    4 => FFmpegProfileTonemapAlgorithm::Mobius,
    5 => FFmpegProfileTonemapAlgorithm::Hable,
});
db_enum!(FFmpegProfileVideoFormat {
    0 => FFmpegProfileVideoFormat::None,
    1 => FFmpegProfileVideoFormat::H264,
    2 => FFmpegProfileVideoFormat::Hevc,
    3 => FFmpegProfileVideoFormat::Mpeg2Video,
    4 => FFmpegProfileVideoFormat::Av1,
    99 => FFmpegProfileVideoFormat::Copy,
});
db_enum!(FillerKind {
    0 => FillerKind::None,
    1 => FillerKind::PreRoll,
    2 => FillerKind::MidRoll,
    3 => FillerKind::PostRoll,
    4 => FillerKind::Tail,
    5 => FillerKind::Fallback,
    99 => FillerKind::GuideMode,
    100 => FillerKind::DecoDefault,
});
db_enum!(FillerMode {
    0 => FillerMode::None,
    1 => FillerMode::Duration,
    2 => FillerMode::Count,
    3 => FillerMode::Pad,
    4 => FillerMode::RandomCount,
});
db_enum!(FillWithGroupMode {
    0 => FillWithGroupMode::None,
    1 => FillWithGroupMode::FillWithOrderedGroups,
    2 => FillWithGroupMode::FillWithShuffledGroups,
});
db_enum!(FilterMode {
    0 => FilterMode::HardwareIfPossible,
    1 => FilterMode::Software,
});
db_enum!(FixedStartTimeBehavior {
    0 => FixedStartTimeBehavior::Strict,
    1 => FixedStartTimeBehavior::Flexible,
});
db_enum!(GraphicsElementKind {
    0 => GraphicsElementKind::Image,
    1 => GraphicsElementKind::Text,
    2 => GraphicsElementKind::Subtitle,
    3 => GraphicsElementKind::Motion,
    4 => GraphicsElementKind::Script,
});
db_enum!(GuideMode {
    0 => GuideMode::Normal,
    1 => GuideMode::Filler,
});
db_enum!(HardwareAccelerationKind {
    0 => HardwareAccelerationKind::None,
    1 => HardwareAccelerationKind::Qsv,
    2 => HardwareAccelerationKind::Nvenc,
    3 => HardwareAccelerationKind::Vaapi,
    4 => HardwareAccelerationKind::VideoToolbox,
    5 => HardwareAccelerationKind::Amf,
    6 => HardwareAccelerationKind::V4l2m2m,
    7 => HardwareAccelerationKind::Rkmpp,
});
db_enum!(LibraryMediaKind {
    1 => LibraryMediaKind::Movies,
    2 => LibraryMediaKind::Shows,
    3 => LibraryMediaKind::MusicVideos,
    4 => LibraryMediaKind::OtherVideos,
    5 => LibraryMediaKind::Songs,
    6 => LibraryMediaKind::Images,
    7 => LibraryMediaKind::RemoteStreams,
});
db_enum!(MarathonGroupBy {
    0 => MarathonGroupBy::None,
    1 => MarathonGroupBy::Show,
    2 => MarathonGroupBy::Season,
    3 => MarathonGroupBy::Artist,
    4 => MarathonGroupBy::Album,
    5 => MarathonGroupBy::Director,
});
db_enum!(MediaItemState {
    0 => MediaItemState::Normal,
    1 => MediaItemState::FileNotFound,
    2 => MediaItemState::Unavailable,
    3 => MediaItemState::RemoteOnly,
});
db_enum!(MediaSourceKind {
    1 => MediaSourceKind::Local,
    2 => MediaSourceKind::Plex,
    3 => MediaSourceKind::Jellyfin,
    4 => MediaSourceKind::Emby,
});
db_enum!(ManagedLibraryOperationKind {
    0 => ManagedLibraryOperationKind::ManualScan,
    1 => ManagedLibraryOperationKind::ScheduledScan,
    2 => ManagedLibraryOperationKind::StartupReplayScan,
    3 => ManagedLibraryOperationKind::Publication,
    4 => ManagedLibraryOperationKind::PublicationChildScan,
    5 => ManagedLibraryOperationKind::ProviderRefresh,
    6 => ManagedLibraryOperationKind::Recovery,
    7 => ManagedLibraryOperationKind::Reset,
    8 => ManagedLibraryOperationKind::Reconcile,
});
db_enum!(ManagedLibraryOperationState {
    0 => ManagedLibraryOperationState::Queued,
    1 => ManagedLibraryOperationState::Running,
    2 => ManagedLibraryOperationState::WaitingToRetry,
    3 => ManagedLibraryOperationState::Completed,
    4 => ManagedLibraryOperationState::Failed,
});
db_enum!(AddonMediaListPublicationPhase {
    0 => AddonMediaListPublicationPhase::AwaitingLease,
    1 => AddonMediaListPublicationPhase::Staging,
    2 => AddonMediaListPublicationPhase::Scanning,
    3 => AddonMediaListPublicationPhase::Committing,
    4 => AddonMediaListPublicationPhase::RestoringFiles,
    5 => AddonMediaListPublicationPhase::RestoringProjection,
    6 => AddonMediaListPublicationPhase::Reconciling,
    7 => AddonMediaListPublicationPhase::Complete,
    8 => AddonMediaListPublicationPhase::Verifying,
    9 => AddonMediaListPublicationPhase::CleaningUp,
});
db_enum!(AddonMediaListPublicationOutcome {
    0 => AddonMediaListPublicationOutcome::Published,
    1 => AddonMediaListPublicationOutcome::RestoredAfterFailure,
});
db_enum!(AddonMediaListPublicationEventKind {
    0 => AddonMediaListPublicationEventKind::ExistingJobAdopted,
    1 => AddonMediaListPublicationEventKind::Enqueued,
    2 => AddonMediaListPublicationEventKind::LeaseWaitStarted,
    3 => AddonMediaListPublicationEventKind::LeaseAcquired,
    4 => AddonMediaListPublicationEventKind::LeaseReclaimed,
    5 => AddonMediaListPublicationEventKind::PhaseAdvanced,
    6 => AddonMediaListPublicationEventKind::ChildScanStarted,
    7 => AddonMediaListPublicationEventKind::ChildScanCompleted,
    8 => AddonMediaListPublicationEventKind::ChildScanFailed,
    9 => AddonMediaListPublicationEventKind::OriginalFailure,
    10 => AddonMediaListPublicationEventKind::RecoveryFailure,
    11 => AddonMediaListPublicationEventKind::RetryScheduled,
    12 => AddonMediaListPublicationEventKind::RecoveryRequested,
    13 => AddonMediaListPublicationEventKind::Completed,
    14 => AddonMediaListPublicationEventKind::HistoryTruncated,
});
db_enum!(AddonMediaListPublicationEnqueueTrigger {
    0 => AddonMediaListPublicationEnqueueTrigger::Operator,
});
db_enum!(AddonMediaListPublicationFilesystemTransition {
    0 => AddonMediaListPublicationFilesystemTransition::PreviousBackedUp,
    1 => AddonMediaListPublicationFilesystemTransition::CandidateCommitted,
    2 => AddonMediaListPublicationFilesystemTransition::PreviousRestored,
    3 => AddonMediaListPublicationFilesystemTransition::ProjectionRestored,
    4 => AddonMediaListPublicationFilesystemTransition::RevisionVerified,
    5 => AddonMediaListPublicationFilesystemTransition::CleanupCompleted,
});
db_enum!(AddonMediaListPublicationRetryClass {
    0 => AddonMediaListPublicationRetryClass::TransientRecovery,
});
db_enum!(AddonMediaListPublicationErrorCategory {
    0 => AddonMediaListPublicationErrorCategory::FilesystemIo,
    1 => AddonMediaListPublicationErrorCategory::PermissionDenied,
    2 => AddonMediaListPublicationErrorCategory::SharingViolation,
    3 => AddonMediaListPublicationErrorCategory::UnsafeFilesystemState,
    4 => AddonMediaListPublicationErrorCategory::InvalidPublicationMarker,
    5 => AddonMediaListPublicationErrorCategory::PublicationIntegrity,
});
db_enum!(AddonMediaListRecoveryAction {
    0 => AddonMediaListRecoveryAction::Retry,
    1 => AddonMediaListRecoveryAction::RestorePreviousRevision,
});
db_enum!(MediaStreamKind {
    1 => MediaStreamKind::Video,
    2 => MediaStreamKind::Audio,
    3 => MediaStreamKind::Subtitle,
    4 => MediaStreamKind::Attachment,
    5 => MediaStreamKind::ExternalSubtitle,
});
db_enum!(MetadataKind {
    0 => MetadataKind::Fallback,
    1 => MetadataKind::Sidecar,
    2 => MetadataKind::External,
    3 => MetadataKind::Embedded,
});
db_enum!(MetadataSource {
    0 => MetadataSource::Fallback,
    1 => MetadataSource::Sidecar,
});
db_enum!(MultipleMode {
    0 => MultipleMode::Count,
    1 => MultipleMode::CollectionSize,
    2 => MultipleMode::PlaylistItemSize,
    3 => MultipleMode::MultiEpisodeGroupSize,
});
db_enum!(NormalizeLoudnessMode {
    0 => NormalizeLoudnessMode::Off,
    1 => NormalizeLoudnessMode::LoudNorm,
});
db_enum!(PlaybackOrder {
    0 => PlaybackOrder::None,
    1 => PlaybackOrder::Chronological,
    2 => PlaybackOrder::Random,
    3 => PlaybackOrder::Shuffle,
    4 => PlaybackOrder::ShuffleInOrder,
    5 => PlaybackOrder::MultiEpisodeShuffle,
    6 => PlaybackOrder::SeasonEpisode,
    7 => PlaybackOrder::RandomRotation,
    8 => PlaybackOrder::Marathon,
});
db_enum!(PlayoutMode {
    1 => PlayoutMode::Flood,
    2 => PlayoutMode::One,
    3 => PlayoutMode::Multiple,
    4 => PlayoutMode::Duration,
});
db_enum!(PlayoutScheduleKind {
    0 => PlayoutScheduleKind::None,
    1 => PlayoutScheduleKind::Classic,
    2 => PlayoutScheduleKind::Block,
    3 => PlayoutScheduleKind::Sequential,
    4 => PlayoutScheduleKind::Scripted,
    20 => PlayoutScheduleKind::ExternalJson,
});
db_enum!(ScalingBehavior {
    0 => ScalingBehavior::ScaleAndPad,
    1 => ScalingBehavior::Stretch,
    2 => ScalingBehavior::Crop,
});
db_enum!(StartType {
    1 => StartType::Dynamic,
    2 => StartType::Fixed,
});
db_enum!(StreamingMode {
    1 => StreamingMode::TransportStream,
    2 => StreamingMode::HttpLiveStreamingDirect,
    4 => StreamingMode::HttpLiveStreamingSegmenter,
    5 => StreamingMode::TransportStreamHybrid,
});
db_enum!(SubtitleKind {
    0 => SubtitleKind::Embedded,
    1 => SubtitleKind::Sidecar,
    99 => SubtitleKind::Generated,
});
db_enum!(TailMode {
    0 => TailMode::None,
    1 => TailMode::Offline,
    2 => TailMode::Slate,
    3 => TailMode::Filler,
});
db_enum!(AddonMediaListItemKind {
    0 => AddonMediaListItemKind::Movie,
    1 => AddonMediaListItemKind::Show,
    2 => AddonMediaListItemKind::Season,
    3 => AddonMediaListItemKind::Episode,
    4 => AddonMediaListItemKind::RemoteStream,
});
db_enum!(AddonMediaListItemAvailability {
    0 => AddonMediaListItemAvailability::Available,
    1 => AddonMediaListItemAvailability::Unavailable,
    2 => AddonMediaListItemAvailability::Unknown,
});
db_enum!(AddonMediaListAvailabilityReason {
    0 => AddonMediaListAvailabilityReason::AuthenticationRequired,
    1 => AddonMediaListAvailabilityReason::ContentRestricted,
    2 => AddonMediaListAvailabilityReason::Unavailable,
});
db_enum!(AddonMediaListContentKind {
    0 => AddonMediaListContentKind::Auto,
    1 => AddonMediaListContentKind::TelevisionEpisode,
    2 => AddonMediaListContentKind::Movie,
    3 => AddonMediaListContentKind::MusicVideo,
    4 => AddonMediaListContentKind::OtherVideo,
    5 => AddonMediaListContentKind::Song,
    6 => AddonMediaListContentKind::Image,
});
db_enum!(AddonMediaListDetailPolicy {
    0 => AddonMediaListDetailPolicy::Automatic,
    1 => AddonMediaListDetailPolicy::ManualOnly,
});
db_enum!(AddonMediaListRefreshCadence {
    0 => AddonMediaListRefreshCadence::Daily,
    1 => AddonMediaListRefreshCadence::Weekly,
    2 => AddonMediaListRefreshCadence::Monthly,
    3 => AddonMediaListRefreshCadence::Yearly,
    4 => AddonMediaListRefreshCadence::Never,
});
db_enum!(AddonMediaListRefreshPublicationPolicy {
    0 => AddonMediaListRefreshPublicationPolicy::Review,
    1 => AddonMediaListRefreshPublicationPolicy::AutoPublishAdditions,
});
db_enum!(AddonMediaListImportPurpose {
    0 => AddonMediaListImportPurpose::Refresh,
    1 => AddonMediaListImportPurpose::Reconcile,
    2 => AddonMediaListImportPurpose::ResetReload,
});
db_enum!(AddonMediaListImportMode {
    0 => AddonMediaListImportMode::Full,
    1 => AddonMediaListImportMode::Incremental,
});
db_enum!(AddonMediaListIncrementalStrategy {
    0 => AddonMediaListIncrementalStrategy::None,
    1 => AddonMediaListIncrementalStrategy::SkipKnown,
    2 => AddonMediaListIncrementalStrategy::NewestFirstEarlyStop,
});
db_enum!(AddonMediaListLifecycleJobKind {
    0 => AddonMediaListLifecycleJobKind::ResetReload,
    1 => AddonMediaListLifecycleJobKind::Reconcile,
});
db_enum!(AddonMediaListLifecyclePhase {
    0 => AddonMediaListLifecyclePhase::AwaitingLease,
    1 => AddonMediaListLifecyclePhase::Quiescing,
    2 => AddonMediaListLifecyclePhase::UnpublishingFiles,
    3 => AddonMediaListLifecyclePhase::UnpublishingProjection,
    4 => AddonMediaListLifecyclePhase::ClearingProviderState,
    5 => AddonMediaListLifecyclePhase::ClearingCheckpoint,
    6 => AddonMediaListLifecyclePhase::VerifyingEmpty,
    7 => AddonMediaListLifecyclePhase::QueueingInitialImport,
    8 => AddonMediaListLifecyclePhase::EnumeratingSource,
    9 => AddonMediaListLifecyclePhase::ComparingCandidate,
    10 => AddonMediaListLifecyclePhase::Complete,
});
db_enum!(AddonMediaListArtifactKind {
    0 => AddonMediaListArtifactKind::Subtitle,
    1 => AddonMediaListArtifactKind::ProcessedMedia,
});
db_enum!(AddonMediaListArtifactState {
    0 => AddonMediaListArtifactState::Reserved,
    1 => AddonMediaListArtifactState::Staged,
    2 => AddonMediaListArtifactState::Ready,
    3 => AddonMediaListArtifactState::Failed,
});
db_enum!(AddonToolMaintenanceTrigger {
    0 => AddonToolMaintenanceTrigger::Startup,
    1 => AddonToolMaintenanceTrigger::Daily,
    2 => AddonToolMaintenanceTrigger::Manual,
});
db_enum!(AddonToolMaintenanceState {
    0 => AddonToolMaintenanceState::Queued,
    1 => AddonToolMaintenanceState::Running,
    2 => AddonToolMaintenanceState::WaitingToRetry,
    3 => AddonToolMaintenanceState::Completed,
    4 => AddonToolMaintenanceState::Failed,
});
db_enum!(AddonToolMaintenanceValidation {
    0 => AddonToolMaintenanceValidation::NotRun,
    1 => AddonToolMaintenanceValidation::Passed,
    2 => AddonToolMaintenanceValidation::Failed,
});
db_enum!(AddonToolMaintenanceRollback {
    0 => AddonToolMaintenanceRollback::NotRequired,
    1 => AddonToolMaintenanceRollback::Succeeded,
    2 => AddonToolMaintenanceRollback::Failed,
});
db_enum!(AddonToolMaintenanceOutcome {
    0 => AddonToolMaintenanceOutcome::Updated,
    1 => AddonToolMaintenanceOutcome::AlreadyCurrent,
    2 => AddonToolMaintenanceOutcome::ManualUpdateRequired,
    3 => AddonToolMaintenanceOutcome::RolledBack,
    4 => AddonToolMaintenanceOutcome::Failed,
});
db_enum!(AddonMediaListImportState {
    0 => AddonMediaListImportState::Queued,
    1 => AddonMediaListImportState::Discovering,
    2 => AddonMediaListImportState::Verifying,
    3 => AddonMediaListImportState::Ready,
    4 => AddonMediaListImportState::Paused,
    5 => AddonMediaListImportState::NeedsAttention,
    6 => AddonMediaListImportState::Cancelled,
    7 => AddonMediaListImportState::Superseded,
});
db_enum!(AddonMediaListDetailState {
    0 => AddonMediaListDetailState::NotRequested,
    1 => AddonMediaListDetailState::Queued,
    2 => AddonMediaListDetailState::WaitingForSlot,
    3 => AddonMediaListDetailState::Running,
    4 => AddonMediaListDetailState::WaitingToRetry,
    5 => AddonMediaListDetailState::Complete,
    6 => AddonMediaListDetailState::Skipped,
    7 => AddonMediaListDetailState::Failed,
});

// 0 and 1 are frozen: rows written before the presentation set grew to the
// full library-kind range still decode to the same two projections.
db_enum!(AddonMediaListPresentationKind {
    0 => AddonMediaListPresentationKind::RemoteStreams,
    1 => AddonMediaListPresentationKind::TelevisionShow,
    2 => AddonMediaListPresentationKind::Auto,
    3 => AddonMediaListPresentationKind::Movies,
    4 => AddonMediaListPresentationKind::MusicVideos,
    5 => AddonMediaListPresentationKind::OtherVideos,
    6 => AddonMediaListPresentationKind::Songs,
    7 => AddonMediaListPresentationKind::Images,
});

db_enum!(AddonMediaListReviewState {
    0 => AddonMediaListReviewState::Draft,
    1 => AddonMediaListReviewState::Ready,
    2 => AddonMediaListReviewState::Published,
    3 => AddonMediaListReviewState::Publishing,
});

db_enum!(AddonMediaListItemRole {
    0 => AddonMediaListItemRole::Auto,
    1 => AddonMediaListItemRole::Primary,
    2 => AddonMediaListItemRole::Extra,
});
db_enum!(RemoteStreamLiveness {
    0 => RemoteStreamLiveness::Unknown,
    1 => RemoteStreamLiveness::Finite,
    2 => RemoteStreamLiveness::Live,
});
db_enum!(VaapiDriver {
    0 => VaapiDriver::Auto,
    1 => VaapiDriver::IHD,
    2 => VaapiDriver::I965,
    3 => VaapiDriver::RadeonSI,
    4 => VaapiDriver::Nouveau,
});
db_enum!(VideoScanKind {
    0 => VideoScanKind::Unknown,
    1 => VideoScanKind::Progressive,
    2 => VideoScanKind::Interlaced,
});
db_enum!(WatermarkLocation {
    0 => WatermarkLocation::BottomRight,
    1 => WatermarkLocation::BottomLeft,
    2 => WatermarkLocation::TopRight,
    3 => WatermarkLocation::TopLeft,
    4 => WatermarkLocation::TopMiddle,
    5 => WatermarkLocation::RightMiddle,
    6 => WatermarkLocation::BottomMiddle,
    7 => WatermarkLocation::LeftMiddle,
    8 => WatermarkLocation::MiddleCenter,
});
db_enum!(WatermarkSize {
    0 => WatermarkSize::Scaled,
    1 => WatermarkSize::ActualSize,
});

/// Decode an optional TEXT duration.
#[must_use]
pub fn decode_optional_duration_text(value: Option<&str>) -> Option<Duration> {
    value.map(decode_duration_text)
}

pub fn decode_message(kind: &'static str, message: &str) -> RepositoryError {
    RepositoryError::Decode {
        kind,
        value: message.to_owned(),
    }
}

pub fn not_found(kind: &'static str, id: i64, error: sqlx::Error) -> RepositoryError {
    match error {
        sqlx::Error::RowNotFound => RepositoryError::NotFound { kind, id },
        other => map_data_access_error(kind, &other),
    }
}

pub async fn replace_ids(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    delete_sql: &str,
    insert_sql: &str,
    parent_id: i64,
    ids: &[i64],
) -> Result<()> {
    sqlx::query(crate::db::assert_sql_safe(delete_sql))
        .bind(parent_id)
        .execute(&mut **tx)
        .await
        .repository()?;

    for id in ids {
        sqlx::query(crate::db::assert_sql_safe(insert_sql))
            .bind(parent_id)
            .bind(id)
            .execute(&mut **tx)
            .await
            .repository()?;
    }

    Ok(())
}

pub async fn load_ids(pool: &Pool<crate::db::Db>, sql: &str, parent_id: i64) -> Result<Vec<i64>> {
    sqlx::query_scalar::<_, i64>(crate::db::assert_sql_safe(sql))
        .bind(parent_id)
        .fetch_all(pool)
        .await
        .repository()
}

/// Turns a plain `INSERT INTO` statement into a cascade-safe upsert.
///
/// `REPLACE INTO` is not an update: it deletes the conflicting row before
/// inserting the replacement, and that delete fires `ON DELETE CASCADE`. Any
/// table whose primary key is a cascade parent therefore loses all of its
/// children every time a row is "replaced", while the row itself looks
/// untouched. Editing an `FFmpeg` profile destroyed every channel using it
/// exactly this way on 2026-08-12.
///
/// A real upsert never deletes the row, so no cascade runs. It also preserves
/// columns the statement does not list, where `REPLACE` silently reset them to
/// their schema defaults.
///
/// `insert_sql` must be an `INSERT INTO "Table" ("A", "B", ...) VALUES (...)`
/// statement; `conflict_columns` are the key columns that identify an existing
/// row and are excluded from the update list.
/// `keys[0]` must be the primary key. Any further entries are the table's other
/// UNIQUE indexes: `REPLACE` resolved a conflict on *any* unique index, so each
/// one needs its own upsert clause or the statement would start raising
/// `UNIQUE constraint failed` where it previously succeeded. The primary key is
/// never assigned, so a conflict on a secondary key updates the existing row in
/// place instead of moving its identity out from under its children.
pub(crate) fn cascade_safe_upsert(insert_sql: &str, keys: &[&[&str]]) -> String {
    let columns = insert_columns(insert_sql);
    let primary_key = keys.first().copied().unwrap_or(&[]);

    let assignments = |target: &[&str]| {
        columns
            .iter()
            .filter(|column| {
                !target.contains(&column.as_str()) && !primary_key.contains(&column.as_str())
            })
            .map(|column| format!("\"{column}\" = excluded.\"{column}\""))
            .collect::<Vec<_>>()
            .join(", ")
    };

    let mut sql = insert_sql.to_owned();
    for target in keys {
        let updates = assignments(target);
        let targets = quoted_list(target);
        if updates.is_empty() {
            let _ = write!(sql, "\nON CONFLICT({targets}) DO NOTHING");
        } else {
            let _ = write!(sql, "\nON CONFLICT({targets}) DO UPDATE SET {updates}");
        }
    }
    sql
}

/// Builds a cascade-safe upsert once and reuses it.
///
/// Wraps [`cascade_safe_upsert`] in a call-site `static` so the statement is
/// assembled a single time rather than on every write, which matters on scan
/// hot paths. Takes the plain `INSERT INTO ...` statement and the key columns
/// that identify an existing row.
macro_rules! upsert_sql {
    ($insert:expr, $keys:expr $(,)?) => {{
        static UPSERT_SQL: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
            $crate::repositories::common::cascade_safe_upsert($insert, $keys)
        });
        UPSERT_SQL.as_str()
    }};
}

pub(crate) use upsert_sql;

fn quoted_list(columns: &[&str]) -> String {
    columns
        .iter()
        .map(|column| format!("\"{column}\""))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Extracts the column names from the leading column list of an `INSERT`.
fn insert_columns(insert_sql: &str) -> Vec<String> {
    let open = insert_sql.find('(').unwrap_or(0);
    let close = insert_sql[open..]
        .find(')')
        .map_or(insert_sql.len(), |offset| open + offset);
    insert_sql[open + 1..close]
        .split(',')
        .map(|column| column.trim().trim_matches('"').trim().to_owned())
        .filter(|column| !column.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        cascade_safe_upsert, encode_signed_duration, insert_columns, map_data_access_error,
        not_found, try_decode_duration_text, try_decode_signed_duration_text,
    };
    use chrono::Duration;
    use ersatzrs_core::repositories::RepositoryError;

    #[test]
    fn sqlx_mapping_preserves_decode_unavailable_and_not_found_categories() {
        assert!(matches!(
            map_data_access_error(
                "channel row",
                &sqlx::Error::Decode("invalid integer".into()),
            ),
            RepositoryError::Decode {
                kind: "channel row",
                ..
            }
        ));
        assert!(matches!(
            map_data_access_error("load channels", &sqlx::Error::PoolClosed),
            RepositoryError::Unavailable {
                operation: "load channels",
                ..
            }
        ));
        assert!(matches!(
            not_found("channel", 12, sqlx::Error::RowNotFound),
            RepositoryError::NotFound {
                kind: "channel",
                id: 12
            }
        ));
    }

    #[test]
    fn signed_duration_round_trips_channel_playout_offsets() {
        for hours in [-12, -1, 0, 1, 12] {
            let duration = Duration::hours(hours);
            let encoded = encode_signed_duration(duration);
            assert_eq!(try_decode_signed_duration_text(&encoded).unwrap(), duration);
        }

        assert_eq!(
            try_decode_signed_duration_text("-1.01:02:03.5").unwrap(),
            -Duration::days(1)
                - Duration::hours(1)
                - Duration::minutes(2)
                - Duration::seconds(3)
                - Duration::milliseconds(500)
        );
        assert!(try_decode_duration_text("-01:00:00").is_err());
        assert!(try_decode_signed_duration_text("+01:00:00").is_err());
        assert!(try_decode_signed_duration_text("-01:60:00").is_err());
    }

    const CHANNEL_INSERT: &str = r#"INSERT INTO "Channel" (
                "Id", "Name", "Group"
             ) VALUES (?, ?, ?)"#;

    #[test]
    fn insert_columns_are_parsed_from_the_leading_list_only() {
        assert_eq!(insert_columns(CHANNEL_INSERT), ["Id", "Name", "Group"]);
    }

    #[test]
    fn sqlite_upsert_updates_every_non_key_column() {
        let sql = cascade_safe_upsert(CHANNEL_INSERT, &[&["Id"]]);
        assert!(sql.starts_with("INSERT INTO \"Channel\""), "{sql}");
        assert!(
            sql.ends_with(
                "ON CONFLICT(\"Id\") DO UPDATE SET \
                 \"Name\" = excluded.\"Name\", \"Group\" = excluded.\"Group\""
            ),
            "{sql}"
        );
        assert!(
            !sql.contains("REPLACE INTO"),
            "the cascade-firing form must be gone: {sql}"
        );
    }

    #[test]
    fn key_only_statements_keep_the_existing_row() {
        // Join tables such as PlayoutItemWatermark are entirely key columns, so
        // there is nothing to SET. The row and its children must survive.
        let join =
            r#"INSERT INTO "PlayoutItemWatermark" ("PlayoutItemId", "WatermarkId") VALUES (?, ?)"#;
        assert!(
            cascade_safe_upsert(join, &[&["PlayoutItemId", "WatermarkId"]])
                .ends_with("ON CONFLICT(\"PlayoutItemId\", \"WatermarkId\") DO NOTHING")
        );
    }
}
