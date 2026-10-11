//! Authored values and separately reported source metadata (ADRs 0120–0123).
//!
//! This is an ErsatzRS extension, not an ErsatzTV subtitle rewrite. Existing
//! `Subtitle`/`MediaStream` fields retain their source-selection ownership.
//! Structure alone is not publication readiness: the FFmpeg-side encoder must
//! also validate character representation and physical spacing-control cells.
//! Enhancement reference collection does not establish resolved object/display
//! semantics, physical resource allocation or Level 2.5 encoding readiness.

mod charset_values;
mod drcs_resources;
mod enhancement_objects;
mod enhancement_program;
mod enhancement_values;
mod enhancements;
mod identity;
mod objects;
mod pages;
mod presentation;
mod profiles;
mod resource_identity;
mod service;
mod source;

pub use charset_values::*;
pub use drcs_resources::*;
pub use enhancement_objects::*;
pub use enhancement_program::*;
pub use enhancement_values::*;
pub use enhancements::*;
pub use identity::*;
pub use objects::*;
pub use pages::*;
pub use presentation::*;
pub use profiles::*;
pub use resource_identity::*;
pub use service::*;
pub use source::*;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod enhancements_tests;

#[cfg(test)]
mod resource_identity_tests;

#[cfg(test)]
mod drcs_resources_tests;

#[cfg(test)]
mod charset_values_tests;

#[cfg(test)]
mod enhancement_values_tests;

#[cfg(test)]
mod objects_tests;

#[cfg(test)]
mod enhancement_program_tests;

#[cfg(test)]
mod enhancement_objects_tests;

#[cfg(test)]
mod presentation_tests;

#[cfg(test)]
mod service_tests;
