//! Authored values and separately reported source metadata (ADRs 0120/0121).
//!
//! This is an ErsatzRS extension, not an ErsatzTV subtitle rewrite. Existing
//! `Subtitle`/`MediaStream` fields retain their source-selection ownership.
//! Structure alone is not publication readiness: the FFmpeg-side encoder must
//! also validate character representation and physical spacing-control cells.

mod identity;
mod pages;
mod profiles;
mod source;

pub use identity::*;
pub use pages::*;
pub use profiles::*;
pub use source::*;

#[cfg(test)]
mod tests;
