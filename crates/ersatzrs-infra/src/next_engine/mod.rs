//! Optional ErsatzTV Next process boundary (ADR 0095).
//!
//! Wire contracts originate in upstream `Core/Next/{Config/ChannelConfig,Playout}.cs`.
//! Runtime compatibility is checked against the separately packaged Next engine;
//! these types do not replace the native `FFmpeg` pipeline or scheduler.

pub mod config;
pub mod coordinator;
pub mod host;
pub mod process;
pub mod protocol;
pub mod publication;
pub mod runtime;
pub mod schedule;
pub mod selection;
pub mod session;
