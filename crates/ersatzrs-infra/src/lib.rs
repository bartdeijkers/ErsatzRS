#![forbid(unsafe_code)]

pub mod cache_cleanup;
pub(crate) mod condition;
pub mod db;
pub mod emby;
pub mod ffmpeg_config;
pub mod ffprobe;
pub mod file_system_layout;
pub mod graphics_import;
pub mod graphics_template;
pub mod health;
pub mod hls;
pub mod hls_worker;
pub mod jellyfin;
pub mod logging;
pub mod maintenance;
pub mod metadata;
pub mod music_video_credits;
pub mod playback;
pub mod plex;
pub mod repositories;
pub mod subtitles;
pub mod trakt;
pub mod trakt_sync;
pub mod xmltv;
