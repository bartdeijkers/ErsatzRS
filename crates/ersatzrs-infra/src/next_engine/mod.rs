//! Optional ErsatzTV Next process boundary (ADR 0095).
//!
//! Wire contracts originate in upstream `Core/Next/{Config/ChannelConfig,Playout}.cs`.
//! Runtime compatibility is checked against the separately packaged Next engine;
//! these types do not replace the native `FFmpeg` pipeline or scheduler.

pub(crate) mod burn_cache;
pub(crate) mod burn_subtitles;
pub mod canvas;
pub mod canvas_host;
mod canvas_render;
pub mod config;
pub mod connections;
pub mod coordinator;
mod credits;
pub mod diagnostics;
mod dynamic_host;
mod dynamic_item;
mod dynamic_projection;
mod dynamic_provider_identity;
mod fallback;
mod graphics;
pub mod host;
mod live_subtitles;
pub mod manager;
pub mod media;
mod on_demand;
pub mod probe_hint;
pub mod process;
mod producer_host;
pub mod protocol;
pub mod publication;
mod remote_projection;
mod retained_host;
pub mod runtime;
pub mod schedule;
pub mod selection;
pub mod session;
pub mod session_transfers;
pub mod song_background;
mod song_host;
mod source;
pub mod troubleshooting;
pub mod version;

mod watermark;
