//! Authored values and separately reported source metadata (ADRs 0120–0123).
//!
//! This is an ErsatzRS extension, not an ErsatzTV subtitle rewrite. Existing
//! `Subtitle`/`MediaStream` fields retain their source-selection ownership.
//! Structure alone is not publication readiness: the FFmpeg-side encoder must
//! also validate character representation and physical spacing-control cells.
//! Enhancement reference collection does not establish resolved object/display
//! semantics, physical resource allocation or Level 2.5 encoding readiness.

mod attribute_execution;
mod attribute_observations;
mod attribute_transmission;
mod attribute_values;
mod base_display;
mod base_mosaics;
mod base_text_edit;
mod base_text_spelling;
pub use base_text_edit::*;
pub use base_text_spelling::{TeletextBaseTextSpelling, TeletextBaseTextSpellingError};
mod base_footprints;
mod charset_execution;
mod charset_execution_values;
mod charset_values;
mod compositional_links;
mod declared_links;
mod declared_service;
mod design_exports;
mod designs;
mod display_geometry;
mod display_geometry_open_count;
mod display_geometry_successive_count;
mod display_geometry_values;
mod drcs_bitmap_overlay;
mod drcs_execution;
mod drcs_resources;
mod effective_selection;
mod enhancement_objects;
mod enhancement_program;
mod enhancement_values;
mod enhancements;
mod glyph_intent;
mod glyph_intent_values;
mod glyph_layers;
mod identity;
mod local_object_glyph_program;
mod local_object_operand_overlay;
mod local_program_operand_overlay;
mod main_local_invocation_program;
mod main_program_presence_overlay;
mod main_resource_invocation_overlay;
mod mot;
mod mot_presentation;
mod mot_resolution;
mod national_conversion;
mod object_resource_addition;
mod objects;
mod output_observation;
mod overrides;
mod pages;
mod presentation;
mod producer_composition;
mod producer_presentation;
mod profiles;
mod publications;
mod resource_header;
mod resource_identity;
mod resource_object_definition_addition;
mod resource_object_definition_removal;
mod resource_object_operand_overlay;
mod resource_object_program_overlay;
mod service;
mod source;
mod source_headers;
mod source_pages;
mod spatial_trace;

pub use attribute_execution::*;
pub use attribute_observations::*;
pub use attribute_transmission::*;
pub use attribute_values::*;
pub use base_display::*;
pub use base_footprints::*;
pub use base_mosaics::{TeletextBaseMosaicState, TeletextHeldMosaic, TeletextHeldMosaicSource};
pub use charset_execution::*;
pub use charset_execution_values::*;
pub use charset_values::*;
pub use compositional_links::*;
pub use declared_service::*;
pub use design_exports::*;
pub use designs::*;
pub use display_geometry::*;
pub use display_geometry_open_count::*;
pub use display_geometry_successive_count::*;
pub use display_geometry_values::*;
pub use drcs_bitmap_overlay::*;
pub use drcs_execution::*;
pub use drcs_resources::*;
pub use effective_selection::*;
pub use enhancement_objects::*;
pub use enhancement_program::*;
pub use enhancement_values::*;
pub use enhancements::*;
pub use glyph_intent::*;
pub use glyph_intent_values::*;
pub use glyph_layers::*;
pub use identity::*;
pub use local_object_glyph_program::*;
pub use local_object_operand_overlay::*;
pub use local_program_operand_overlay::*;
pub use main_local_invocation_program::*;
pub use main_program_presence_overlay::*;
pub use main_resource_invocation_overlay::*;
pub use mot::*;
pub use mot_presentation::*;
pub use mot_resolution::*;
pub use national_conversion::*;
pub use object_resource_addition::*;
pub use objects::*;
pub use output_observation::*;
pub use overrides::*;
pub use pages::*;
pub use presentation::*;
pub use producer_composition::*;
pub use producer_presentation::*;
pub use profiles::*;
pub use publications::*;
pub use resource_header::*;
pub use resource_identity::*;
pub use resource_object_definition_addition::*;
pub use resource_object_definition_removal::*;
pub use resource_object_operand_overlay::*;
pub use resource_object_program_overlay::*;
pub use service::*;
pub use source::*;
pub use source_headers::*;
pub use source_pages::*;
pub use spatial_trace::*;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod declared_service_tests;

#[cfg(test)]
mod enhancements_tests;

#[cfg(test)]
mod resource_header_tests;
#[cfg(test)]
mod resource_identity_tests;

#[cfg(test)]
mod drcs_resources_tests;

#[cfg(test)]
mod drcs_execution_tests;

#[cfg(test)]
mod display_geometry_tests;

#[cfg(test)]
mod display_geometry_open_count_tests;

#[cfg(test)]
mod display_geometry_successive_count_tests;

#[cfg(test)]
mod service_invocation_admission_tests;

#[cfg(test)]
mod charset_values_tests;

#[cfg(test)]
mod compositional_links_tests;

#[cfg(test)]
mod mot_tests;

#[cfg(test)]
mod mot_resolution_tests;

#[cfg(test)]
mod national_conversion_tests;

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

#[cfg(test)]
mod spatial_trace_tests;

#[cfg(test)]
mod glyph_layers_tests;

#[cfg(test)]
mod attribute_execution_tests;

#[cfg(test)]
mod attribute_observation_tests;

#[cfg(test)]
mod charset_execution_tests;

#[cfg(test)]
mod glyph_intent_tests;

#[cfg(test)]
mod base_footprints_tests;

mod display_resolution;
mod physical_base_overlay;
pub use physical_base_overlay::*;
mod display_resolution_values;
mod display_resolution_writers;
pub use display_resolution::*;
pub use display_resolution_values::*;

mod colour_composition;
mod glyph_mask;
pub use colour_composition::*;
pub use glyph_mask::*;

#[cfg(test)]
mod colour_composition_tests;
#[cfg(test)]
mod glyph_mask_tests;

#[cfg(test)]
mod display_resolution_tests;

#[cfg(test)]
mod mot_presentation_tests;

#[cfg(test)]
mod physical_spacing_tests;
