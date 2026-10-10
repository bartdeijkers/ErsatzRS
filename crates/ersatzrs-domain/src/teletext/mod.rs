//! Authored values and separately reported source metadata (ADRs 0120/0121).
//!
//! This is an ErsatzRS extension, not an ErsatzTV subtitle rewrite. Existing
//! `Subtitle`/`MediaStream` fields retain their source-selection ownership.
//! Structure alone is not publication readiness: the FFmpeg-side encoder must
//! also validate character representation and physical spacing-control cells.

mod drcs_resources;
mod enhancements;
mod identity;
mod pages;
mod profiles;
mod resource_identity;
mod source;

pub use drcs_resources::*;
pub use enhancements::*;
pub use identity::*;
pub use pages::*;
pub use profiles::*;
pub use resource_identity::*;
pub use source::*;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod enhancements_tests;

#[cfg(test)]
mod resource_identity_tests;

#[cfg(test)]
mod drcs_resources_tests;
