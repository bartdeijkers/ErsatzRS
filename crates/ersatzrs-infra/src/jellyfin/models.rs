use super::{DateTime, Deserialize, Utc};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct JellyfinLibraryResponse {
    pub(super) name: Option<String>,
    pub(super) collection_type: Option<String>,
    pub(super) item_id: Option<String>,
    pub(super) library_options: Option<JellyfinLibraryOptionsResponse>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub(super) struct JellyfinLibraryOptionsResponse {
    #[serde(default)]
    pub(super) path_infos: Vec<JellyfinPathInfoResponse>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct JellyfinPathInfoResponse {
    pub(super) path: Option<String>,
    pub(super) network_path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct JellyfinLibraryItemsResponse {
    #[serde(default)]
    pub(super) items: Vec<JellyfinLibraryItemResponse>,
    #[serde(rename = "TotalRecordCount")]
    pub(super) total_record_count: i32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct JellyfinLibraryItemResponse {
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
    pub(super) provider_ids: JellyfinProviderIdsResponse,
    #[serde(default)]
    pub(super) premiere_date: Option<DateTime<Utc>>,
    pub(super) location_type: Option<String>,
    pub(super) overview: Option<String>,
    #[serde(default)]
    pub(super) taglines: Vec<String>,
    #[serde(default)]
    pub(super) image_tags: JellyfinImageTagsResponse,
    #[serde(default)]
    pub(super) backdrop_image_tags: Vec<String>,
    #[serde(rename = "Type")]
    pub(super) item_type: Option<String>,
    pub(super) index_number: Option<i32>,
    #[serde(default)]
    pub(super) chapters: Vec<JellyfinChapterResponse>,
    #[serde(default)]
    pub(super) genres: Vec<String>,
    #[serde(default)]
    pub(super) tags: Vec<String>,
    #[serde(default)]
    pub(super) studios: Vec<JellyfinNameRefResponse>,
    #[serde(default)]
    pub(super) people: Vec<JellyfinPersonResponse>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct JellyfinProviderIdsResponse {
    pub(super) imdb: Option<String>,
    pub(super) tmdb: Option<String>,
    pub(super) tvdb: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct JellyfinImageTagsResponse {
    pub(super) primary: Option<String>,
    pub(super) thumb: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct JellyfinNameRefResponse {
    pub(super) name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct JellyfinPersonResponse {
    pub(super) name: Option<String>,
    pub(super) role: Option<String>,
    #[serde(rename = "Type")]
    pub(super) person_type: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct JellyfinPlaybackInfoResponse {
    #[serde(default)]
    pub(super) media_sources: Vec<JellyfinMediaSourceResponse>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct JellyfinMediaSourceResponse {
    pub(super) id: Option<String>,
    pub(super) path: Option<String>,
    #[serde(default)]
    pub(super) run_time_ticks: i64,
    #[serde(default)]
    pub(super) media_streams: Vec<JellyfinMediaStreamResponse>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct JellyfinMediaStreamResponse {
    #[serde(rename = "Type")]
    pub(super) stream_type: Option<String>,
    pub(super) codec: Option<String>,
    pub(super) profile: Option<String>,
    pub(super) language: Option<String>,
    pub(super) height: Option<i32>,
    pub(super) width: Option<i32>,
    #[serde(default)]
    pub(super) index: Option<i32>,
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
    pub(super) rotation: Option<i32>,
    pub(super) bit_depth: Option<i32>,
    pub(super) video_range_type: Option<String>,
    pub(super) dv_profile: Option<i32>,
    pub(super) dv_bl_signal_compatibility_id: Option<i32>,
    pub(super) bl_present_flag: Option<i32>,
    pub(super) el_present_flag: Option<i32>,
    pub(super) rpu_present_flag: Option<i32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct JellyfinChapterResponse {
    #[serde(default)]
    pub(super) start_position_ticks: i64,
    pub(super) name: Option<String>,
}
