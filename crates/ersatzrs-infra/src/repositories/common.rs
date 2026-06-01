use crate::db::DbRow;
use chrono::{DateTime, Duration, Utc};
use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_domain::{
    ArtworkKind, BlockStopScheduling, ChannelIdleBehavior, ChannelMusicVideoCreditsMode,
    ChannelPlayoutMode, ChannelPlayoutSource, ChannelSongVideoMode, ChannelStreamSelectorMode,
    ChannelSubtitleMode, ChannelTranscodeMode, ChannelWatermarkImageSource, ChannelWatermarkMode,
    CollectionType, DecoBreakPlacement, DecoMode, FFmpegProfileAudioFormat, FFmpegProfileBitDepth,
    FFmpegProfileTonemapAlgorithm, FFmpegProfileVideoFormat, FillWithGroupMode, FillerKind,
    FillerMode, FilterMode, FixedStartTimeBehavior, GraphicsElementKind, GuideMode,
    HardwareAccelerationKind, LibraryMediaKind, MarathonGroupBy, MediaItemState, MediaSourceKind,
    MediaStreamKind, MetadataKind, MetadataSource, MultipleMode, NormalizeLoudnessMode,
    PlaybackOrder, PlayoutMode, PlayoutScheduleKind, ScalingBehavior, StartType, StreamingMode,
    SubtitleKind, TailMode, TraktListItemKind, VaapiDriver, VideoScanKind, WatermarkLocation,
    WatermarkSize,
};
use serde::de::DeserializeOwned;
use sqlx::{Pool, Row};

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
        let value = row.try_get::<String, _>(column)?;
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
            .try_get::<Option<String>, _>(column)?
            .map(|value| decode_enum(kind, &value))
            .transpose(),
    }
}

pub fn decode_bool_column(row: &DbRow, column: &'static str) -> Result<bool> {
    if let Ok(value) = row.try_get(column) {
        return Ok(value);
    }

    Ok(row.try_get::<i64, _>(column)? != 0)
}

pub fn decode_optional_bool_column(row: &DbRow, column: &'static str) -> Result<Option<bool>> {
    if let Ok(value) = row.try_get(column) {
        return Ok(value);
    }

    Ok(row
        .try_get::<Option<i64>, _>(column)?
        .map(|value| value != 0))
}

pub fn decode_string_column(row: &DbRow, column: &'static str) -> Result<String> {
    if let Ok(value) = row.try_get(column) {
        return Ok(value);
    }

    decode_utf8_column(row.try_get(column)?, column)
}

pub fn decode_optional_string_column(row: &DbRow, column: &'static str) -> Result<Option<String>> {
    if let Ok(value) = row.try_get(column) {
        return Ok(value);
    }

    row.try_get::<Option<Vec<u8>>, _>(column)?
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

/// Decode an `HH:MM:SS` (or similar) TEXT duration stored by `ErsatzTV`.
#[must_use]
pub fn decode_duration_text(value: &str) -> Duration {
    let (day_text, time_text) = value.rsplit_once('.').map_or(("", value), |(left, right)| {
        if left.contains(':') {
            ("", value)
        } else {
            (left, right)
        }
    });
    let parts: Vec<&str> = time_text.split(':').collect();
    let [hours_text, minutes_text, seconds_text] = parts.as_slice() else {
        return Duration::zero();
    };

    let days = day_text.parse::<i64>().unwrap_or(0);
    let hours = hours_text.parse::<i64>().unwrap_or(0);
    let minutes = minutes_text.parse::<i64>().unwrap_or(0);
    let (seconds, nanos) =
        seconds_text
            .split_once('.')
            .map_or((*seconds_text, 0), |(whole, fraction)| {
                let padded = format!("{fraction:0<9}");
                let nanos = padded[..9].parse::<i64>().unwrap_or(0);
                (whole, nanos)
            });
    let seconds = seconds.parse::<i64>().unwrap_or(0);

    Duration::days(days)
        + Duration::hours(hours)
        + Duration::minutes(minutes)
        + Duration::seconds(seconds)
        + Duration::nanoseconds(nanos)
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
db_enum!(TraktListItemKind {
    0 => TraktListItemKind::Movie,
    1 => TraktListItemKind::Show,
    2 => TraktListItemKind::Season,
    3 => TraktListItemKind::Episode,
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
    RepositoryError::Database(sqlx::Error::Decode(
        format!("failed to decode {kind}: {message}").into(),
    ))
}

pub fn not_found(kind: &'static str, id: i64, error: sqlx::Error) -> RepositoryError {
    match error {
        sqlx::Error::RowNotFound => RepositoryError::NotFound { kind, id },
        other => RepositoryError::Database(other),
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
        .await?;

    for id in ids {
        sqlx::query(crate::db::assert_sql_safe(insert_sql))
            .bind(parent_id)
            .bind(id)
            .execute(&mut **tx)
            .await?;
    }

    Ok(())
}

pub async fn load_ids(pool: &Pool<crate::db::Db>, sql: &str, parent_id: i64) -> Result<Vec<i64>> {
    sqlx::query_scalar::<_, i64>(crate::db::assert_sql_safe(sql))
        .bind(parent_id)
        .fetch_all(pool)
        .await
        .map_err(RepositoryError::from)
}
