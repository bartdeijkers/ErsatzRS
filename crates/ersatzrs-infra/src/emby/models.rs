use super::{DateTime, Deserialize, Utc};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct EmbyLibraryResponse {
    pub(super) name: Option<String>,
    pub(super) collection_type: Option<String>,
    pub(super) item_id: Option<String>,
    pub(super) library_options: Option<EmbyLibraryOptionsResponse>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub(super) struct EmbyLibraryOptionsResponse {
    #[serde(default)]
    pub(super) path_infos: Vec<EmbyPathInfoResponse>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct EmbyPathInfoResponse {
    pub(super) path: Option<String>,
    pub(super) network_path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct EmbyLibraryItemsResponse {
    #[serde(default)]
    pub(super) items: Vec<EmbyLibraryItemResponse>,
    #[serde(rename = "TotalRecordCount")]
    pub(super) total_record_count: i32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct EmbyLibraryItemResponse {
    pub(super) name: Option<String>,
    pub(super) id: Option<String>,
    pub(super) etag: Option<String>,
    pub(super) path: Option<String>,
    pub(super) official_rating: Option<String>,
    #[serde(default)]
    pub(super) date_created: Option<DateTime<Utc>>,
    #[serde(default)]
    pub(super) run_time_ticks: i64,
    #[serde(default)]
    pub(super) production_year: Option<i32>,
    #[serde(default)]
    pub(super) provider_ids: EmbyProviderIdsResponse,
    #[serde(default)]
    pub(super) premiere_date: Option<DateTime<Utc>>,
    pub(super) location_type: Option<String>,
    pub(super) overview: Option<String>,
    #[serde(default)]
    pub(super) taglines: Vec<String>,
    #[serde(default)]
    pub(super) image_tags: EmbyImageTagsResponse,
    #[serde(default)]
    pub(super) backdrop_image_tags: Vec<String>,
    #[serde(rename = "Type")]
    pub(super) item_type: Option<String>,
    pub(super) index_number: Option<i32>,
    #[serde(default)]
    pub(super) chapters: Vec<EmbyChapterResponse>,
    #[serde(default)]
    pub(super) genres: Vec<String>,
    #[serde(default)]
    pub(super) tag_items: Vec<EmbyNameRefResponse>,
    #[serde(default)]
    pub(super) studios: Vec<EmbyNameRefResponse>,
    #[serde(default)]
    pub(super) people: Vec<EmbyPersonResponse>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct EmbyProviderIdsResponse {
    pub(super) imdb: Option<String>,
    pub(super) tmdb: Option<String>,
    pub(super) tvdb: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct EmbyImageTagsResponse {
    pub(super) primary: Option<String>,
    pub(super) thumb: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct EmbyNameRefResponse {
    pub(super) name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct EmbyPersonResponse {
    pub(super) name: Option<String>,
    pub(super) role: Option<String>,
    #[serde(rename = "Type")]
    pub(super) person_type: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct EmbyPlaybackInfoResponse {
    #[serde(default)]
    pub(super) media_sources: Vec<EmbyMediaSourceResponse>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct EmbyMediaSourceResponse {
    #[serde(default)]
    pub(super) run_time_ticks: i64,
    #[serde(default)]
    pub(super) media_streams: Vec<EmbyMediaStreamResponse>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct EmbyMediaStreamResponse {
    #[serde(rename = "Type")]
    pub(super) stream_type: Option<String>,
    pub(super) codec: Option<String>,
    pub(super) profile: Option<String>,
    pub(super) language: Option<String>,
    pub(super) height: Option<i32>,
    pub(super) width: Option<i32>,
    #[serde(default)]
    pub(super) index: i32,
    #[serde(default)]
    pub(super) is_default: bool,
    #[serde(default)]
    pub(super) is_forced: bool,
    #[serde(default)]
    pub(super) is_external: bool,
    pub(super) aspect_ratio: Option<String>,
    pub(super) channels: Option<i32>,
    pub(super) real_frame_rate: Option<f64>,
    pub(super) title: Option<String>,
    pub(super) pixel_format: Option<String>,
    pub(super) color_range: Option<String>,
    pub(super) color_space: Option<String>,
    pub(super) color_transfer: Option<String>,
    pub(super) color_primaries: Option<String>,
    pub(super) bit_depth: Option<i32>,
    pub(super) video_range_type: Option<String>,
    pub(super) extended_video_type: Option<String>,
    pub(super) extended_video_sub_type: Option<String>,
    pub(super) dv_profile: Option<i32>,
    pub(super) dv_bl_signal_compatibility_id: Option<i32>,
    pub(super) bl_present_flag: Option<i32>,
    pub(super) el_present_flag: Option<i32>,
    pub(super) rpu_present_flag: Option<i32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct EmbyChapterResponse {
    #[serde(default)]
    pub(super) start_position_ticks: i64,
    pub(super) name: Option<String>,
}
