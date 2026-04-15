use serde::{Deserialize, Serialize};

pub type EntityId = i64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CollectionEnumeratorState {
    pub seed: i32,
    pub index: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LanguageCode {
    pub id: EntityId,
    pub three_code_1: String,
    pub three_code_2: Option<String>,
    pub two_code: Option<String>,
    pub english_name: String,
    pub french_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PlaybackOrder {
    #[default]
    None,
    Chronological,
    Random,
    Shuffle,
    ShuffleInOrder,
    MultiEpisodeShuffle,
    SeasonEpisode,
    RandomRotation,
    Marathon,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FilterMode {
    #[default]
    HardwareIfPossible,
    Software,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ScalingBehavior {
    #[default]
    ScaleAndPad,
    Stretch,
    Crop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum StartType {
    #[default]
    Dynamic,
    Fixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum GuideMode {
    #[default]
    Normal,
    Filler,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FillWithGroupMode {
    #[default]
    None,
    FillWithOrderedGroups,
    FillWithShuffledGroups,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MultipleMode {
    #[default]
    Count,
    CollectionSize,
    PlaylistItemSize,
    MultiEpisodeGroupSize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MarathonGroupBy {
    #[default]
    None,
    Show,
    Season,
    Artist,
    Album,
    Director,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TailMode {
    #[default]
    None,
    Offline,
    Slate,
    Filler,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum StreamingMode {
    #[default]
    TransportStream,
    HttpLiveStreamingDirect,
    HttpLiveStreamingSegmenter,
    TransportStreamHybrid,
}
