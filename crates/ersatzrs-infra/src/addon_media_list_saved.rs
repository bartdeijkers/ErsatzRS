//! ADR 0093 saved list content, independent of installation-local projections.
//!
//! The format is under implementation. No runtime writer or scanner currently
//! enables this document; exporting an incomplete draft is not file authority.

pub mod assets;
pub mod capture;
pub mod document_io;
pub mod files;
pub mod model;
pub mod projection;
pub mod published;
pub mod receiving;
pub mod receiving_artwork;
pub mod sources;
pub mod validation;
