//! Ordered, unpublished main-program composition from exact base-cell intent.
//!
//! Original commands retain their values and order. Charset-bound replacements
//! carry distinct provenance: this intermediate does not settle object overlap,
//! glyph footprints, side panels, non-Latin fallback reinterpretation or packet
//! capacity. Row-zero commands and original MOT default ownership remain intact
//! for later context-aware wire lowering. Neither this plan nor its successful
//! reference collection grants publication or page readiness.

use std::collections::BTreeMap;

use ersatzrs_domain::{
    build_teletext_glyph_layers, resolve_teletext_base_footprints,
    resolve_teletext_charset_execution, resolve_teletext_colour_composition,
    resolve_teletext_display_geometry, resolve_teletext_drcs_execution,
    resolve_teletext_glyph_intent, TeletextAttributeExecution, TeletextAttributeExecutionError,
    TeletextAttributeTransmissionError, TeletextAttributeTransmissionEvidence, TeletextBaseDisplay,
    TeletextBaseFootprintError, TeletextBaseFootprints, TeletextCharsetExecution,
    TeletextCharsetExecutionError, TeletextCharsetPair, TeletextCharsetValueError,
    TeletextCollectedEnhancementReferences, TeletextColourComposition, TeletextColumnOperation,
    TeletextDisplayGeometry, TeletextDisplayGeometryBudget, TeletextDisplayGeometryError,
    TeletextDisplayResolution, TeletextDisplayResolutionBudget, TeletextDisplayResolutionError,
    TeletextDisplayResolutionInputs, TeletextDrcsExecution, TeletextDrcsExecutionBudget,
    TeletextDrcsExecutionError, TeletextDrcsProgramExecutionError, TeletextEffectiveProgramSource,
    TeletextEnhancementCharsetDesignation, TeletextEnhancementCollectionBudget,
    TeletextEnhancementCollectionFailure, TeletextEnhancementCommand,
    TeletextEnhancementObjectCatalog, TeletextEnhancementPosition, TeletextEnhancementProgram,
    TeletextEnhancementProgramError, TeletextEnhancementSource, TeletextG3Compatibility,
    TeletextGlyphIdentity, TeletextGlyphIntent, TeletextGlyphIntentError, TeletextGlyphLayers,
    TeletextGlyphLayersError, TeletextObjectIdentity, TeletextPageAddress, TeletextSemanticService,
    TeletextServiceError, TeletextSpatialTrace, TeletextSpatialTraceBudget,
    TeletextSpatialTraceError, TELETEXT_COLUMNS, TELETEXT_ROWS,
};

use super::base_display::{trace_teletext_base_display, TeletextBaseDisplayError};
use super::colour_context::{
    resolve_teletext_colour_context, resolve_teletext_program_colour_context,
    TeletextColourContext, TeletextColourContextError,
};
use super::compile_context::TeletextCompileContext;
use super::layout::{self, PositionedBaseGlyph};
use super::TeletextEncodingError;

/// Trusted host resource limits; these are not serialized producer values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextMainCompositionBudget {
    /// Original effective main commands plus positioned base replacements.
    pub max_input_commands: usize,
    /// Exact commands after suppression, positioning and repertoire restoration.
    pub max_output_commands: usize,
    /// Expanded commands, repertoire pairs and references, including objects.
    pub collection: TeletextEnhancementCollectionBudget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextBaseCellCommandKind {
    Position,
    LatinCharset,
    GlyphCharset,
    Glyph,
    CharsetRestore,
}

/// Original or derived location, also usable for expanded collector diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextMainCompositionSource {
    OriginalPage {
        command: usize,
    },
    DefaultSlot {
        slot: u8,
    },
    BaseCell {
        position: TeletextEnhancementPosition,
        kind: TeletextBaseCellCommandKind,
    },
    /// A real, derived attribute writer materializes this original position.
    /// It is never an authored operand or original C.2 transmission witness.
    HeaderPosition {
        command: usize,
    },
    /// A body ordinal remains in its original full object identity.
    Object {
        identity: TeletextObjectIdentity,
        command: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TeletextMainCompositionFailure {
    #[error("object header-position work exceeds trusted maximum {maximum}")]
    ObjectHeaderWorkBudget { maximum: usize },
    #[error("object header-position storage arithmetic exceeds supported range")]
    ObjectHeaderStorage,
    #[error(transparent)]
    ObjectCatalog(#[from] ersatzrs_domain::TeletextObjectCatalogError),
    #[error("main and base input work exceeds trusted budget {maximum}")]
    InputBudget { maximum: usize },
    #[error("composed main commands exceed trusted budget {maximum}")]
    OutputBudget { maximum: usize },
    #[error(transparent)]
    Base(#[from] TeletextEncodingError),
    #[error(transparent)]
    Service(#[from] TeletextServiceError),
    #[error(transparent)]
    ColourContext(#[from] TeletextColourContextError),
    /// Reusing original Base/footprints requires unchanged planes. `None` is
    /// the screen; `Some(row)` is the first differing row, not a guessed writer.
    #[error("supplied program changes original colour plane {row:?}")]
    PlaneContextMismatch { row: Option<u8> },
    #[error(transparent)]
    BaseDisplay(#[from] TeletextBaseDisplayError),
    #[error(transparent)]
    BaseFootprints(#[from] TeletextBaseFootprintError),
    #[error(transparent)]
    SpatialTrace(#[from] TeletextSpatialTraceError),
    #[error(transparent)]
    GlyphLayers(#[from] TeletextGlyphLayersError),
    #[error(transparent)]
    AttributeExecution(#[from] TeletextAttributeExecutionError),
    #[error(transparent)]
    Transmission(#[from] TeletextAttributeTransmissionError),
    #[error(transparent)]
    CharsetExecution(#[from] TeletextCharsetExecutionError),
    #[error(transparent)]
    GlyphIntent(#[from] TeletextGlyphIntentError),
    #[error(transparent)]
    DrcsExecution(#[from] TeletextDrcsExecutionError),
    #[error(transparent)]
    DrcsProgram(#[from] TeletextDrcsProgramExecutionError),
    #[error(transparent)]
    DisplayGeometry(#[from] TeletextDisplayGeometryError),
    #[error(transparent)]
    DisplayResolution(#[from] TeletextDisplayResolutionError),
    #[error("base and enhancement glyph candidate budget exceeds storage range")]
    GlyphIntentBudget,
    #[error("original display resolution budget exceeds storage range")]
    DisplayResolutionBudget,
    #[error(transparent)]
    Charset(#[from] TeletextCharsetValueError),
    #[error(transparent)]
    Program(#[from] TeletextEnhancementProgramError),
    #[error(transparent)]
    Collection(#[from] TeletextEnhancementCollectionFailure),
    #[error("compatible glyph at {position:?} repeats earlier command {first:?}")]
    DuplicateCompatibleGlyph {
        position: TeletextEnhancementPosition,
        first: TeletextMainCompositionSource,
    },
    #[error("active position moves backwards from {previous:?} to {requested:?}")]
    ActivePositionOrder {
        previous: (u8, u8),
        requested: (u8, u8),
    },
    #[error("effective program and its source locations disagree")]
    SelectionState,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("teletext page {owner:?}, location {location:?}: {reason}")]
pub struct TeletextMainCompositionError {
    pub owner: TeletextPageAddress,
    pub location: Option<TeletextMainCompositionSource>,
    pub reason: TeletextMainCompositionFailure,
}

/// Immutable lowering intermediate. The collected source ordinals index this
/// plan; `source_location` recovers authored/default/base or object provenance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextMainComposition {
    owner: TeletextPageAddress,
    original_program_source: TeletextEffectiveProgramSource,
    program: TeletextEnhancementProgram,
    provenance: Vec<TeletextMainCompositionSource>,
    colour_context: TeletextColourContext,
    base_display: TeletextBaseDisplay,
    base_footprints: TeletextBaseFootprints,
    spatial_trace: TeletextSpatialTrace,
    glyph_layers: TeletextGlyphLayers,
    attribute_execution: TeletextAttributeExecution,
    charset_execution: TeletextCharsetExecution,
    glyph_intent: TeletextGlyphIntent,
    drcs_execution: TeletextDrcsExecution,
    display_geometry: TeletextDisplayGeometry,
    display_resolution: TeletextDisplayResolution,
    colour_composition: TeletextColourComposition,
    base_replacements: Vec<PositionedBaseGlyph>,
    collected: TeletextCollectedEnhancementReferences,
}

impl TeletextMainComposition {
    /// The palette and C.1 planes selected by the same immutable service as
    /// the original display. This does not grant publication authority.
    #[must_use]
    pub const fn colour_composition(&self) -> &TeletextColourComposition {
        &self.colour_composition
    }

    pub(super) fn base_replacement(&self, row: u8, column: u8) -> Option<TeletextColumnOperation> {
        self.base_replacements
            .binary_search_by_key(&(row, column), |glyph| {
                (glyph.position().row(), glyph.position().column())
            })
            .ok()
            .map(|index| TeletextColumnOperation::from(&self.base_replacements[index]))
    }
    pub(super) fn base_replacement_charset(
        &self,
        row: u8,
        column: u8,
    ) -> Option<TeletextEnhancementCharsetDesignation> {
        self.base_replacements
            .binary_search_by_key(&(row, column), |glyph| {
                (glyph.position().row(), glyph.position().column())
            })
            .ok()
            .map(|index| self.base_replacements[index].charset())
    }
    pub(super) fn base_replacement_identity(
        &self,
        row: u8,
        column: u8,
    ) -> Option<Result<TeletextGlyphIdentity, ersatzrs_domain::TeletextGlyphMaskError>> {
        let operation = self.base_replacement(row, column)?;
        let charset = self.base_replacement_charset(row, column)?;
        Some(ersatzrs_domain::resolve_teletext_replacement_identity(
            operation,
            charset.pair(),
        ))
    }
    #[must_use]
    pub const fn owner(&self) -> TeletextPageAddress {
        self.owner
    }
    #[must_use]
    pub const fn original_program_source(&self) -> &TeletextEffectiveProgramSource {
        &self.original_program_source
    }
    #[must_use]
    pub const fn program(&self) -> &TeletextEnhancementProgram {
        &self.program
    }
    #[must_use]
    pub fn provenance(&self) -> &[TeletextMainCompositionSource] {
        &self.provenance
    }
    #[must_use]
    pub const fn colour_context(&self) -> &TeletextColourContext {
        &self.colour_context
    }
    /// Coded fallback and original base intent for subsequent display resolution.
    /// This trace does not resolve object overlap or grant rendering readiness.
    #[must_use]
    pub const fn base_display(&self) -> &TeletextBaseDisplay {
        &self.base_display
    }
    /// Coded-base fragments and spacing-driven main-row suppression. These
    /// exact origins reference the retained Base trace, before any enhancement
    /// overlays. Pending boundaries and final layer-aware C.3 remain explicit.
    #[must_use]
    pub const fn base_footprints(&self) -> &TeletextBaseFootprints {
        &self.base_footprints
    }
    /// Original command coordinates and repeated invocation identities, before
    /// base-glyph insertion. Final attribute arbitration and footprints remain open.
    #[must_use]
    pub const fn spatial_trace(&self) -> &TeletextSpatialTrace {
        &self.spatial_trace
    }
    /// Original glyph contenders with separate base and object priority. Their
    /// diagnostic ranking does not select attributes, footprints or wire words.
    #[must_use]
    pub const fn glyph_layers(&self) -> &TeletextGlyphLayers {
        &self.glyph_layers
    }
    /// Separate original-command attribute inheritance over the logical areas.
    /// Pending arbitration and extent diagnostics prohibit treating this
    /// intermediate as a completed display or a publication authorization.
    #[must_use]
    pub const fn attribute_execution(&self) -> &TeletextAttributeExecution {
        &self.attribute_execution
    }
    /// Original per-contender repertoire context, separate from coded Base
    /// fallback and generated compatibility words. Pending source order,
    /// geometry and physical receiver equivalence remain open.
    #[must_use]
    pub const fn charset_execution(&self) -> &TeletextCharsetExecution {
        &self.charset_execution
    }
    /// Audited original table positions from this exact charset execution.
    /// Losing candidates and pending registry indices are preserved. Assignment
    /// evidence does not certify rasterization, arbitration or publication.
    #[must_use]
    pub const fn glyph_intent(&self) -> &TeletextGlyphIntent {
        &self.glyph_intent
    }
    /// Exact original DRCS candidates resolved through this immutable service's
    /// supplied links and checked catalog. No bitmap grants geometry, renderer
    /// readiness or publication authority; losing candidates remain present.
    #[must_use]
    pub const fn drcs_execution(&self) -> &TeletextDrcsExecution {
        &self.drcs_execution
    }
    /// Original candidates placed using this service's selected panel metadata.
    /// Unplaced content remains explicit; final object admissibility, occupied
    /// footprints, geometry-aware attributes and publication remain separate.
    #[must_use]
    pub const fn display_geometry(&self) -> &TeletextDisplayGeometry {
        &self.display_geometry
    }
    /// Original layer-aware fragments and deferred attribute writers from these
    /// exact immutable inputs. Pending evidence remains visible; this result
    /// grants no physical equivalence, rasterization or publication authority.
    #[must_use]
    pub const fn display_resolution(&self) -> &TeletextDisplayResolution {
        &self.display_resolution
    }
    #[must_use]
    pub const fn collected(&self) -> &TeletextCollectedEnhancementReferences {
        &self.collected
    }
    /// Settle the same composition under the service's shared update quota.
    pub(super) fn with_transmission_context(
        mut self,
        service: &TeletextSemanticService,
        transmission: &TeletextAttributeTransmissionEvidence,
        maximum: usize,
        context: &mut TeletextCompileContext<'_, '_>,
    ) -> Result<Self, TeletextMainCompositionError> {
        let source = &self.original_program_source;
        let (attributes, display) = display_after_transmission(
            &self.base_display,
            &self.base_footprints,
            &self.colour_context,
            OriginalDisplayBorrowed {
                spatial: &self.spatial_trace,
                glyphs: &self.glyph_layers,
                attributes: &self.attribute_execution,
                intent: &self.glyph_intent,
                drcs: &self.drcs_execution,
                geometry: &self.display_geometry,
            },
            source,
            (transmission, maximum),
            context,
        )?;
        let presentation = service.effective_presentation(self.owner).ok_or_else(|| {
            error(
                self.owner,
                None,
                TeletextMainCompositionFailure::SelectionState,
            )
        })?;
        let page = service
            .base()
            .pages
            .iter()
            .find(|page| page.address == self.owner)
            .ok_or_else(|| {
                error(
                    self.owner,
                    None,
                    TeletextMainCompositionFailure::SelectionState,
                )
            })?;
        self.colour_composition = resolve_teletext_colour_composition(
            &display,
            presentation,
            self.colour_context.row_colours(),
            self.colour_context.screen_colour(),
            page.kind,
        );
        self.attribute_execution = attributes;
        self.display_resolution = display;
        Ok(self)
    }

    /// Map a source from this plan's collected references without cloning them.
    /// Foreign root ordinals return `None`; nested body identities stay exact.
    #[must_use]
    pub fn source_location(
        &self,
        location: TeletextEnhancementSource,
    ) -> Option<TeletextMainCompositionSource> {
        mapped_location(&self.provenance, location)
    }

    /// Replace only the physical main and its private emitter map. The authored
    /// execution, Base, glyphs and attributes remain the immutable target; the
    /// supplied candidate must still pass complete physical comparison.
    pub(super) fn with_physical_main(
        mut self,
        service: &TeletextSemanticService,
        program: TeletextEnhancementProgram,
        provenance: Vec<TeletextMainCompositionSource>,
        budget: TeletextMainCompositionBudget,
        context: &mut TeletextCompileContext<'_, '_>,
    ) -> Result<Self, TeletextMainCompositionError> {
        if program.commands().len() != provenance.len() {
            return Err(error(
                self.owner,
                None,
                TeletextMainCompositionFailure::SelectionState,
            ));
        }
        if program.commands().len() > budget.max_output_commands {
            return Err(error(
                self.owner,
                None,
                TeletextMainCompositionFailure::OutputBudget {
                    maximum: budget.max_output_commands,
                },
            ));
        }
        let primary = service
            .effective_presentation(self.owner)
            .ok_or_else(|| {
                error(
                    self.owner,
                    None,
                    TeletextMainCompositionFailure::SelectionState,
                )
            })?
            .primary_designation()
            .map_err(|reason| error(self.owner, None, reason.into()))?
            .pair();
        let collected = collect_composed_program(
            context,
            service,
            self.owner,
            &program,
            primary,
            budget.collection,
            &provenance,
        )?;
        self.program = program;
        self.provenance = provenance;
        self.collected = collected;
        Ok(self)
    }
}

/// Compose one exact existing display's effective main with positioned fallback
/// replacements. Raw counts fence work before indexing; a sizing pass charges
/// the exact output before allocating it. Suppression is limited to explicit
/// compatible G2/G0-diacritic/Level-1.5-G3 replacements at that same base cell.
/// Original glyphs, attributes, invocation origins/offsets and aliases survive.
///
/// Temporary glyph designations restore the original same-row designation;
/// actual row changes reset to the chosen primary. An invocation never flushes
/// later base cells ahead of itself. A colour-only header selector is not
/// invented: original row-zero positions remain semantic commands and the
/// original colour prepass accompanies the plan for later wire lowering.
/// # Errors
/// Rejects missing owners, budgets, invalid fallback layout, duplicate compatible
/// replacements, backwards main positions or failed full expanded collection.
/// No partial program or caller mutation is returned. Physical X26 capacity,
/// final base/object semantics and rendering acceptance remain separate gates.
pub fn compose_teletext_main_program(
    service: &TeletextSemanticService,
    owner: TeletextPageAddress,
    budget: TeletextMainCompositionBudget,
) -> Result<TeletextMainComposition, TeletextMainCompositionError> {
    compose_teletext_main_program_with_context(
        service,
        owner,
        budget,
        &mut TeletextCompileContext::new(None, None, None),
    )
}

/// Compose unchanged authored values while debiting the shared display quota.
pub(super) fn compose_teletext_main_program_with_context(
    service: &TeletextSemanticService,
    owner: TeletextPageAddress,
    budget: TeletextMainCompositionBudget,
    context: &mut TeletextCompileContext<'_, '_>,
) -> Result<TeletextMainComposition, TeletextMainCompositionError> {
    let (source, original) = service
        .effective_program(owner)
        .map_err(|reason| error(owner, None, reason.into()))?;
    check_input(
        owner,
        original.commands().len(),
        0,
        budget.max_input_commands,
    )?;
    let BaseInputs {
        page,
        layout,
        presentation,
        primary,
    } = checked_base_inputs(
        service,
        owner,
        original.commands().len(),
        budget.max_input_commands,
    )?;
    let input = MergeInput {
        owner,
        original: &original,
        source: &source,
        glyphs: layout.base_glyphs(),
        primary,
        maximum: budget.max_output_commands,
    };
    let compatible = compatible_positions(&input)?;
    let count = merge(&input, &compatible, None)?.count;
    let built = merge(&input, &compatible, Some(count))?;
    let program = TeletextEnhancementProgram::new(built.commands)
        .map_err(|reason| error(owner, None, reason.into()))?;
    let colour_context =
        resolve_teletext_colour_context(service, owner, budget.collection.max_commands).map_err(
            |reason| {
                let location = reason
                    .location
                    .and_then(|location| original_location(&source, location));
                error(owner, location, reason.into())
            },
        )?;
    let base_display = trace_teletext_base_display(page, &layout, presentation, &colour_context)
        .map_err(|reason| error(owner, None, reason.into()))?;
    let base_footprints =
        resolve_teletext_base_footprints(&base_display, colour_context.row_colours())
            .map_err(|reason| error(owner, None, reason.into()))?;
    let collected = collect_composed_program(
        context,
        service,
        owner,
        &program,
        primary.pair(),
        budget.collection,
        &built.provenance,
    )?;
    let original_display = original_display_execution(
        service,
        &base_display,
        &colour_context,
        &source,
        budget.collection,
        context,
    )?;
    let display_resolution = original_display_resolution(
        &base_display,
        &base_footprints,
        &colour_context,
        original_display.borrowed(),
        &source,
        budget.collection.max_commands,
        context,
    )?;
    let colour_composition = resolve_teletext_colour_composition(
        &display_resolution,
        presentation,
        colour_context.row_colours(),
        colour_context.screen_colour(),
        page.kind,
    );
    Ok(TeletextMainComposition {
        owner,
        original_program_source: source,
        program,
        provenance: built.provenance,
        colour_context,
        base_display,
        base_footprints,
        spatial_trace: original_display.spatial,
        glyph_layers: original_display.glyphs,
        attribute_execution: original_display.attributes,
        charset_execution: original_display.charsets,
        glyph_intent: original_display.intent,
        drcs_execution: original_display.drcs,
        display_geometry: original_display.geometry,
        display_resolution,
        colour_composition,
        base_replacements: layout.base_glyphs().to_vec(),
        collected,
    })
}

/// A location in the supplied physical main or its exact object catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextPhysicalDisplaySource {
    Main {
        command: usize,
    },
    Object {
        identity: TeletextObjectIdentity,
        command: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("physical teletext display {owner:?} at {location:?}: {reason}")]
pub struct TeletextPhysicalDisplayReplayError {
    pub owner: TeletextPageAddress,
    pub location: Option<TeletextPhysicalDisplaySource>,
    pub reason: TeletextMainCompositionFailure,
}

/// Separate immutable diagnostic of the actual supplied semantic physical main.
/// Main ordinals and invocation IDs belong to this replay, not the original
/// target. This is not final-word evidence, packing admission or equivalence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextPhysicalDisplayReplay {
    owner: TeletextPageAddress,
    execution: OriginalDisplayExecution,
    display_resolution: TeletextDisplayResolution,
    colour_composition: TeletextColourComposition,
}
impl TeletextPhysicalDisplayReplay {
    #[must_use]
    pub const fn owner(&self) -> TeletextPageAddress {
        self.owner
    }
    #[must_use]
    pub const fn spatial_trace(&self) -> &TeletextSpatialTrace {
        &self.execution.spatial
    }
    #[must_use]
    pub const fn glyph_layers(&self) -> &TeletextGlyphLayers {
        &self.execution.glyphs
    }
    #[must_use]
    pub const fn attribute_execution(&self) -> &TeletextAttributeExecution {
        &self.execution.attributes
    }
    #[must_use]
    pub const fn charset_execution(&self) -> &TeletextCharsetExecution {
        &self.execution.charsets
    }
    #[must_use]
    pub const fn glyph_intent(&self) -> &TeletextGlyphIntent {
        &self.execution.intent
    }
    #[must_use]
    pub const fn drcs_execution(&self) -> &TeletextDrcsExecution {
        &self.execution.drcs
    }
    #[must_use]
    pub const fn display_geometry(&self) -> &TeletextDisplayGeometry {
        &self.execution.geometry
    }
    #[must_use]
    pub const fn display_resolution(&self) -> &TeletextDisplayResolution {
        &self.display_resolution
    }
    #[must_use]
    pub const fn colour_composition(&self) -> &TeletextColourComposition {
        &self.colour_composition
    }
    /// Factory-only settlement using final packing of this exact replay input.
    /// The factory retains the same service/composition/program/catalog pairing;
    /// this method is not detached evidence or public publication authority.
    #[cfg(test)]
    pub(super) fn with_transmission(
        self,
        service: &TeletextSemanticService,
        original: &TeletextMainComposition,
        transmission: &TeletextAttributeTransmissionEvidence,
        maximum: usize,
    ) -> Result<Self, TeletextPhysicalDisplayReplayError> {
        self.with_transmission_context(
            service,
            original,
            transmission,
            maximum,
            &mut TeletextCompileContext::new(None, None, None),
        )
    }

    /// Settle physical replay under the same service-owned update quota.
    pub(super) fn with_transmission_context(
        mut self,
        service: &TeletextSemanticService,
        original: &TeletextMainComposition,
        transmission: &TeletextAttributeTransmissionEvidence,
        maximum: usize,
        context: &mut TeletextCompileContext<'_, '_>,
    ) -> Result<Self, TeletextPhysicalDisplayReplayError> {
        let mut update = || -> Result<_, TeletextMainCompositionError> {
            let source = TeletextEffectiveProgramSource::Local;
            let (attributes, display) = display_after_transmission(
                &original.base_display,
                &original.base_footprints,
                &original.colour_context,
                self.execution.borrowed(),
                &source,
                (transmission, maximum),
                context,
            )?;
            let presentation = service.effective_presentation(self.owner).ok_or_else(|| {
                error(
                    self.owner,
                    None,
                    TeletextMainCompositionFailure::SelectionState,
                )
            })?;
            let page = service
                .base()
                .pages
                .iter()
                .find(|page| page.address == self.owner)
                .ok_or_else(|| {
                    error(
                        self.owner,
                        None,
                        TeletextMainCompositionFailure::SelectionState,
                    )
                })?;
            let colours = resolve_teletext_colour_composition(
                &display,
                presentation,
                original.colour_context.row_colours(),
                original.colour_context.screen_colour(),
                page.kind,
            );
            Ok((attributes, display, colours))
        };
        let (attributes, display, colours) = update().map_err(physical_replay_error)?;
        self.execution.attributes = attributes;
        self.display_resolution = display;
        self.colour_composition = colours;
        Ok(self)
    }
}

/// Replay an already-composed or augmented main through the shared display chain.
///
/// Reuses the composition's original Base, spacing footprints and colour planes;
/// no layout merge, fallback insertion, target replacement or service clone occurs.
/// A bounded actual-program colour prepass must equal every retained original
/// plane before spatial/glyph allocation. Changed design planes require a fresh
/// original composition; this is a replay-input invariant, not an authored-page
/// prohibition. Plane mismatches identify the screen or first differing row;
/// no contributor command is guessed from the value-only context.
/// The caller must pair the composition with its same immutable original service.
/// Matching owners alone do not certify that pairing. The supplied catalog is
/// checked, but this analysis grants no selected object-link/default authorization,
/// physical transmission ordering, raster, readiness or publication authority.
/// Pending source/attribute/geometry evidence stays explicit.
/// # Errors
/// Rejects trusted work/storage bounds, broken supplied execution or whole-program
/// DRCS collection/dependencies. Root diagnostics use physical main ordinals.
/// Inputs are unchanged and no partial replay escapes.
pub fn replay_teletext_physical_display(
    service: &TeletextSemanticService,
    composition: &TeletextMainComposition,
    program: &TeletextEnhancementProgram,
    catalog: &TeletextEnhancementObjectCatalog,
    budget: TeletextEnhancementCollectionBudget,
) -> Result<TeletextPhysicalDisplayReplay, TeletextPhysicalDisplayReplayError> {
    replay_teletext_physical_display_with_context(
        service,
        composition,
        program,
        catalog,
        budget,
        &mut TeletextCompileContext::new(None, None, None),
    )
}

/// Replay the original pairing under the service's shared display quota.
pub(super) fn replay_teletext_physical_display_with_context(
    service: &TeletextSemanticService,
    composition: &TeletextMainComposition,
    program: &TeletextEnhancementProgram,
    catalog: &TeletextEnhancementObjectCatalog,
    budget: TeletextEnhancementCollectionBudget,
    context: &mut TeletextCompileContext<'_, '_>,
) -> Result<TeletextPhysicalDisplayReplay, TeletextPhysicalDisplayReplayError> {
    let owner = composition.owner;
    let maximum = budget.max_commands;
    let mut build = || -> Result<TeletextPhysicalDisplayReplay, TeletextMainCompositionError> {
        check_input(owner, program.commands().len(), 0, maximum)?;
        let source = TeletextEffectiveProgramSource::Local;
        check_physical_colour_context(service, composition, program, catalog, maximum)?;
        let spatial = context
            .resolve_spatial_program(
                owner,
                program,
                catalog,
                TeletextSpatialTraceBudget {
                    max_events: maximum,
                    max_invocations: maximum,
                },
            )
            .map_err(|reason| {
                let location = reason
                    .location
                    .and_then(|value| original_location(&source, value));
                error(owner, location, reason.into())
            })?;
        let execution = display_execution_from_spatial(
            &composition.base_display,
            &composition.colour_context,
            spatial,
            &source,
            budget,
            DrcsContext::SuppliedProgram {
                service,
                program,
                catalog,
            },
            context,
        )?;
        let display_resolution = original_display_resolution(
            &composition.base_display,
            &composition.base_footprints,
            &composition.colour_context,
            execution.borrowed(),
            &source,
            maximum,
            context,
        )?;
        let presentation = service
            .effective_presentation(owner)
            .ok_or_else(|| error(owner, None, TeletextMainCompositionFailure::SelectionState))?;
        let page = service
            .base()
            .pages
            .iter()
            .find(|page| page.address == owner)
            .ok_or_else(|| error(owner, None, TeletextMainCompositionFailure::SelectionState))?;
        let colour_composition = resolve_teletext_colour_composition(
            &display_resolution,
            presentation,
            composition.colour_context.row_colours(),
            composition.colour_context.screen_colour(),
            page.kind,
        );
        Ok(TeletextPhysicalDisplayReplay {
            owner,
            execution,
            display_resolution,
            colour_composition,
        })
    };
    build().map_err(physical_replay_error)
}

fn physical_replay_error(
    error: TeletextMainCompositionError,
) -> TeletextPhysicalDisplayReplayError {
    TeletextPhysicalDisplayReplayError {
        owner: error.owner,
        location: error.location.and_then(|source| match source {
            TeletextMainCompositionSource::OriginalPage { command } => {
                Some(TeletextPhysicalDisplaySource::Main { command })
            }
            TeletextMainCompositionSource::Object { identity, command } => {
                Some(TeletextPhysicalDisplaySource::Object { identity, command })
            }
            _ => None,
        }),
        reason: error.reason,
    }
}

fn check_physical_colour_context(
    service: &TeletextSemanticService,
    composition: &TeletextMainComposition,
    program: &TeletextEnhancementProgram,
    catalog: &TeletextEnhancementObjectCatalog,
    maximum: usize,
) -> Result<(), TeletextMainCompositionError> {
    let owner = composition.owner;
    let actual = resolve_teletext_program_colour_context(service, owner, program, catalog, maximum)
        .map_err(|reason| {
            let location = reason
                .location
                .and_then(|value| original_location(&TeletextEffectiveProgramSource::Local, value));
            error(owner, location, reason.into())
        })?;
    if actual != composition.colour_context {
        let row = if actual.screen_colour() == composition.colour_context.screen_colour() {
            (0_u8..25)
                .find(|&row| actual.row_colour(row) != composition.colour_context.row_colour(row))
        } else {
            None
        };
        return Err(error(
            owner,
            None,
            TeletextMainCompositionFailure::PlaneContextMismatch { row },
        ));
    }
    Ok(())
}

struct BaseInputs<'a> {
    page: &'a ersatzrs_domain::TeletextPageDraft,
    layout: layout::Layout,
    presentation: ersatzrs_domain::TeletextEffectivePresentation<'a>,
    primary: TeletextEnhancementCharsetDesignation,
}

fn checked_base_inputs(
    service: &TeletextSemanticService,
    owner: TeletextPageAddress,
    command_count: usize,
    maximum: usize,
) -> Result<BaseInputs<'_>, TeletextMainCompositionError> {
    let page = service
        .base()
        .pages
        .iter()
        .find(|page| page.address == owner)
        .ok_or_else(|| error(owner, None, TeletextMainCompositionFailure::SelectionState))?;
    let layout = layout::compile_for_service_composition(service, page)
        .map_err(|reason| error(owner, None, reason.into()))?;
    check_input(owner, command_count, layout.base_glyphs().len(), maximum)?;
    let presentation = service
        .effective_presentation(owner)
        .ok_or_else(|| error(owner, None, TeletextMainCompositionFailure::SelectionState))?;
    let primary = presentation
        .primary_designation()
        .and_then(|value| TeletextEnhancementCharsetDesignation::new(value.get()))
        .map_err(|reason| error(owner, None, reason.into()))?;
    Ok(BaseInputs {
        page,
        layout,
        presentation,
        primary,
    })
}

fn original_glyph_intent(
    charset: &TeletextCharsetExecution,
    source: &TeletextEffectiveProgramSource,
    maximum: usize,
) -> Result<TeletextGlyphIntent, TeletextMainCompositionError> {
    let owner = charset.owner();
    let max_candidates = original_candidate_budget(owner, maximum)?;
    resolve_teletext_glyph_intent(charset, max_candidates).map_err(|reason| {
        let location = reason
            .location
            .and_then(|location| original_location(source, location));
        error(owner, location, reason.into())
    })
}

fn original_candidate_budget(
    owner: TeletextPageAddress,
    maximum: usize,
) -> Result<usize, TeletextMainCompositionError> {
    (TELETEXT_ROWS * TELETEXT_COLUMNS - 8)
        .checked_add(maximum)
        .ok_or_else(|| {
            error(
                owner,
                None,
                TeletextMainCompositionFailure::GlyphIntentBudget,
            )
        })
}

fn original_drcs_execution(
    drcs_context: DrcsContext<'_>,
    intent: &TeletextGlyphIntent,
    source: &TeletextEffectiveProgramSource,
    budget: TeletextEnhancementCollectionBudget,
    context: &mut TeletextCompileContext<'_, '_>,
) -> Result<TeletextDrcsExecution, TeletextMainCompositionError> {
    let owner = intent.owner();
    let limits = TeletextDrcsExecutionBudget {
        max_candidates: original_candidate_budget(owner, budget.max_commands)?,
        max_references: budget.max_drcs_references,
    };
    match drcs_context {
        DrcsContext::SelectedService(service) => {
            resolve_teletext_drcs_execution(service, intent, limits).map_err(|reason| {
                let location = reason
                    .location
                    .and_then(|value| original_location(source, value));
                error(owner, location, reason.into())
            })
        }
        DrcsContext::SuppliedProgram {
            service,
            program,
            catalog,
        } => context
            .resolve_drcs_program(service, program, catalog, intent, budget, limits)
            .map_err(|reason| {
                let location = reason
                    .location()
                    .and_then(|value| original_location(source, value));
                error(owner, location, reason.into())
            }),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OriginalDisplayExecution {
    spatial: TeletextSpatialTrace,
    glyphs: TeletextGlyphLayers,
    attributes: TeletextAttributeExecution,
    charsets: TeletextCharsetExecution,
    intent: TeletextGlyphIntent,
    drcs: TeletextDrcsExecution,
    geometry: TeletextDisplayGeometry,
}

#[derive(Clone, Copy)]
struct OriginalDisplayBorrowed<'a> {
    spatial: &'a TeletextSpatialTrace,
    glyphs: &'a TeletextGlyphLayers,
    attributes: &'a TeletextAttributeExecution,
    intent: &'a TeletextGlyphIntent,
    drcs: &'a TeletextDrcsExecution,
    geometry: &'a TeletextDisplayGeometry,
}
impl OriginalDisplayExecution {
    fn borrowed(&self) -> OriginalDisplayBorrowed<'_> {
        OriginalDisplayBorrowed {
            spatial: &self.spatial,
            glyphs: &self.glyphs,
            attributes: &self.attributes,
            intent: &self.intent,
            drcs: &self.drcs,
            geometry: &self.geometry,
        }
    }
}

fn display_after_transmission(
    base: &TeletextBaseDisplay,
    footprints: &TeletextBaseFootprints,
    colours: &TeletextColourContext,
    execution: OriginalDisplayBorrowed<'_>,
    source: &TeletextEffectiveProgramSource,
    transmission: (&TeletextAttributeTransmissionEvidence, usize),
    context: &mut TeletextCompileContext<'_, '_>,
) -> Result<(TeletextAttributeExecution, TeletextDisplayResolution), TeletextMainCompositionError> {
    let (transmission, maximum) = transmission;
    let attributes = context
        .resolve_attributes(
            base,
            execution.spatial,
            execution.glyphs,
            colours.row_colours(),
            maximum,
            Some(transmission),
        )
        .map_err(|reason| {
            let location = reason
                .location
                .and_then(|value| original_location(source, value));
            error(base.owner(), location, reason.into())
        })?;
    let display = original_display_resolution(
        base,
        footprints,
        colours,
        OriginalDisplayBorrowed {
            attributes: &attributes,
            ..execution
        },
        source,
        maximum,
        context,
    )?;
    Ok((attributes, display))
}

fn original_display_resolution(
    base: &TeletextBaseDisplay,
    base_footprints: &TeletextBaseFootprints,
    colours: &TeletextColourContext,
    original: OriginalDisplayBorrowed<'_>,
    source: &TeletextEffectiveProgramSource,
    maximum: usize,
    context: &mut TeletextCompileContext<'_, '_>,
) -> Result<TeletextDisplayResolution, TeletextMainCompositionError> {
    let owner = base.owner();
    let max_candidates = original_candidate_budget(owner, maximum)?;
    let exhausted = || {
        error(
            owner,
            None,
            TeletextMainCompositionFailure::DisplayResolutionBudget,
        )
    };
    let max_writers = (50_usize + TELETEXT_ROWS * TELETEXT_COLUMNS)
        .checked_add(maximum)
        .ok_or_else(exhausted)?;
    let max_origins = maximum
        .checked_mul(72)
        .and_then(|count| count.checked_add(max_candidates))
        .ok_or_else(exhausted)?;
    // Conservative work ceiling, not allocation size: original command replay
    // can visit all logical areas and attribute groups. The resolver preflights
    // exact writer/origin storage independently before allocating it.
    let max_updates = max_candidates
        .checked_add(maximum)
        .and_then(|count| count.checked_add(1))
        .and_then(|count| count.checked_mul(TELETEXT_ROWS * 72 * 16))
        .ok_or_else(exhausted)?;
    context
        .resolve_display(
            TeletextDisplayResolutionInputs {
                base,
                base_footprints,
                original: original.spatial,
                layers: original.glyphs,
                attributes: original.attributes,
                intent: original.intent,
                drcs: original.drcs,
                geometry: original.geometry,
                row_colours: colours.row_colours(),
            },
            TeletextDisplayResolutionBudget {
                max_candidates,
                max_commands: maximum,
                max_writers,
                max_origins,
                max_updates,
            },
        )
        .map_err(|reason| {
            let location = reason
                .location
                .and_then(|location| original_location(source, location));
            error(owner, location, reason.into())
        })
}

fn collect_composed_program(
    context: &mut TeletextCompileContext<'_, '_>,
    service: &TeletextSemanticService,
    owner: TeletextPageAddress,
    program: &TeletextEnhancementProgram,
    primary: TeletextCharsetPair,
    budget: TeletextEnhancementCollectionBudget,
    provenance: &[TeletextMainCompositionSource],
) -> Result<TeletextCollectedEnhancementReferences, TeletextMainCompositionError> {
    context
        .collect_display_references(service.objects(), owner, program, primary, budget)
        .map_err(|failure| {
            error(
                owner,
                mapped_location(provenance, failure.source_location),
                failure.reason.into(),
            )
        })
}

fn original_display_execution(
    service: &TeletextSemanticService,
    base: &TeletextBaseDisplay,
    colours: &TeletextColourContext,
    source: &TeletextEffectiveProgramSource,
    budget: TeletextEnhancementCollectionBudget,
    context: &mut TeletextCompileContext<'_, '_>,
) -> Result<OriginalDisplayExecution, TeletextMainCompositionError> {
    let maximum = budget.max_commands;
    let spatial = context
        .resolve_spatial_trace(
            service,
            base.owner(),
            TeletextSpatialTraceBudget {
                max_events: maximum,
                max_invocations: maximum,
            },
        )
        .map_err(|reason| {
            let location = reason
                .location
                .and_then(|location| original_location(source, location));
            error(base.owner(), location, reason.into())
        })?;
    display_execution_from_spatial(
        base,
        colours,
        spatial,
        source,
        budget,
        DrcsContext::SelectedService(service),
        context,
    )
}

#[derive(Clone, Copy)]
enum DrcsContext<'a> {
    SelectedService(&'a TeletextSemanticService),
    SuppliedProgram {
        service: &'a TeletextSemanticService,
        program: &'a TeletextEnhancementProgram,
        catalog: &'a TeletextEnhancementObjectCatalog,
    },
}

fn display_execution_from_spatial(
    base: &TeletextBaseDisplay,
    colours: &TeletextColourContext,
    spatial: TeletextSpatialTrace,
    source: &TeletextEffectiveProgramSource,
    budget: TeletextEnhancementCollectionBudget,
    drcs_context: DrcsContext<'_>,
    context: &mut TeletextCompileContext<'_, '_>,
) -> Result<OriginalDisplayExecution, TeletextMainCompositionError> {
    let maximum = budget.max_commands;
    let glyphs = original_glyph_layers(base, &spatial, source, maximum)?;
    let attributes =
        original_attribute_execution(base, &spatial, &glyphs, colours, source, maximum, context)?;
    let charsets =
        resolve_teletext_charset_execution(base, &spatial, &glyphs, maximum).map_err(|reason| {
            let location = reason
                .location
                .and_then(|location| original_location(source, location));
            error(base.owner(), location, reason.into())
        })?;
    let intent = original_glyph_intent(&charsets, source, maximum)?;
    let drcs = original_drcs_execution(drcs_context, &intent, source, budget, context)?;
    let geometry = original_display_geometry(base, &spatial, &glyphs, source, maximum)?;
    Ok(OriginalDisplayExecution {
        spatial,
        glyphs,
        attributes,
        charsets,
        intent,
        drcs,
        geometry,
    })
}

fn original_display_geometry(
    base: &TeletextBaseDisplay,
    spatial: &TeletextSpatialTrace,
    glyphs: &TeletextGlyphLayers,
    source: &TeletextEffectiveProgramSource,
    maximum: usize,
) -> Result<TeletextDisplayGeometry, TeletextMainCompositionError> {
    let owner = base.owner();
    let budget = TeletextDisplayGeometryBudget {
        max_candidates: original_candidate_budget(owner, maximum)?,
        max_events: maximum,
        max_invocations: maximum,
    };
    resolve_teletext_display_geometry(base, spatial, glyphs, budget).map_err(|reason| {
        let location = reason
            .location
            .and_then(|location| original_location(source, location));
        error(owner, location, reason.into())
    })
}

fn original_attribute_execution(
    base: &TeletextBaseDisplay,
    spatial: &TeletextSpatialTrace,
    glyphs: &TeletextGlyphLayers,
    colours: &TeletextColourContext,
    source: &TeletextEffectiveProgramSource,
    maximum: usize,
    context: &mut TeletextCompileContext<'_, '_>,
) -> Result<TeletextAttributeExecution, TeletextMainCompositionError> {
    context
        .resolve_attributes(base, spatial, glyphs, colours.row_colours(), maximum, None)
        .map_err(|reason| {
            let location = reason
                .location
                .and_then(|location| original_location(source, location));
            error(base.owner(), location, reason.into())
        })
}

fn original_glyph_layers(
    base: &TeletextBaseDisplay,
    spatial: &TeletextSpatialTrace,
    source: &TeletextEffectiveProgramSource,
    maximum: usize,
) -> Result<TeletextGlyphLayers, TeletextMainCompositionError> {
    build_teletext_glyph_layers(base, spatial, maximum).map_err(|reason| {
        let location = reason
            .location
            .and_then(|location| original_location(source, location));
        error(base.owner(), location, reason.into())
    })
}

fn check_input(
    owner: TeletextPageAddress,
    commands: usize,
    glyphs: usize,
    maximum: usize,
) -> Result<(), TeletextMainCompositionError> {
    if commands
        .checked_add(glyphs)
        .is_none_or(|count| count > maximum)
    {
        return Err(error(
            owner,
            None,
            TeletextMainCompositionFailure::InputBudget { maximum },
        ));
    }
    Ok(())
}

fn error(
    owner: TeletextPageAddress,
    location: Option<TeletextMainCompositionSource>,
    reason: TeletextMainCompositionFailure,
) -> TeletextMainCompositionError {
    TeletextMainCompositionError {
        owner,
        location,
        reason,
    }
}

fn root_location(
    source: &TeletextEffectiveProgramSource,
    command: usize,
) -> Option<TeletextMainCompositionSource> {
    match source {
        TeletextEffectiveProgramSource::Local => {
            Some(TeletextMainCompositionSource::OriginalPage { command })
        }
        TeletextEffectiveProgramSource::Defaults { slots } => slots
            .get(command)
            .map(|&slot| TeletextMainCompositionSource::DefaultSlot { slot }),
        TeletextEffectiveProgramSource::BaseOnly => None,
    }
}

fn original_location(
    source: &TeletextEffectiveProgramSource,
    location: TeletextEnhancementSource,
) -> Option<TeletextMainCompositionSource> {
    location.object.map_or_else(
        || root_location(source, location.command),
        |identity| {
            Some(TeletextMainCompositionSource::Object {
                identity,
                command: location.command,
            })
        },
    )
}

fn mapped_location(
    provenance: &[TeletextMainCompositionSource],
    location: TeletextEnhancementSource,
) -> Option<TeletextMainCompositionSource> {
    location.object.map_or_else(
        || provenance.get(location.command).copied(),
        |identity| {
            Some(TeletextMainCompositionSource::Object {
                identity,
                command: location.command,
            })
        },
    )
}

struct MergeInput<'a> {
    owner: TeletextPageAddress,
    original: &'a TeletextEnhancementProgram,
    source: &'a TeletextEffectiveProgramSource,
    glyphs: &'a [PositionedBaseGlyph],
    primary: TeletextEnhancementCharsetDesignation,
    maximum: usize,
}

type CompatiblePositions = BTreeMap<(u8, u8), TeletextMainCompositionSource>;

fn compatible_positions(
    input: &MergeInput<'_>,
) -> Result<CompatiblePositions, TeletextMainCompositionError> {
    let mut position = (0, 0);
    let mut compatible = BTreeMap::new();
    for (ordinal, command) in input.original.commands().iter().enumerate() {
        let location = root_location(input.source, ordinal).ok_or_else(|| {
            error(
                input.owner,
                None,
                TeletextMainCompositionFailure::SelectionState,
            )
        })?;
        if let Some(requested) = command_position(*command, position.0) {
            ensure_order(input.owner, location, position, requested)?;
            position = requested;
        }
        if let TeletextEnhancementCommand::AtColumn { operation, .. } = *command {
            if compatible_glyph(operation) {
                if let Some(first) = compatible.insert(position, location) {
                    return Err(error(
                        input.owner,
                        Some(location),
                        TeletextMainCompositionFailure::DuplicateCompatibleGlyph {
                            position: TeletextEnhancementPosition::new(position.0, position.1)
                                .map_err(|_| {
                                    error(
                                        input.owner,
                                        Some(location),
                                        TeletextMainCompositionFailure::SelectionState,
                                    )
                                })?,
                            first,
                        },
                    ));
                }
            }
        }
    }
    Ok(compatible)
}

fn compatible_glyph(operation: TeletextColumnOperation) -> bool {
    matches!(
        operation,
        TeletextColumnOperation::G2 { .. }
            | TeletextColumnOperation::G0Diacritic { .. }
            | TeletextColumnOperation::G3 {
                compatibility: TeletextG3Compatibility::Level15,
                ..
            }
    )
}

fn enhanced_glyph(operation: TeletextColumnOperation) -> bool {
    matches!(
        operation,
        TeletextColumnOperation::G0 { .. }
            | TeletextColumnOperation::G1 { .. }
            | TeletextColumnOperation::G1Alpha { .. }
            | TeletextColumnOperation::Drcs { .. }
            | TeletextColumnOperation::G3 {
                compatibility: TeletextG3Compatibility::Level25,
                ..
            }
    )
}

fn command_position(command: TeletextEnhancementCommand, row: u8) -> Option<(u8, u8)> {
    match command {
        TeletextEnhancementCommand::SetPosition { position } => {
            Some((position.row(), position.column()))
        }
        TeletextEnhancementCommand::SetRowColour { row, .. } => Some((row, 0)),
        TeletextEnhancementCommand::AtColumn { column, .. } => Some((row, column)),
        _ => None,
    }
}

fn ensure_order(
    owner: TeletextPageAddress,
    location: TeletextMainCompositionSource,
    previous: (u8, u8),
    requested: (u8, u8),
) -> Result<(), TeletextMainCompositionError> {
    if requested < previous {
        return Err(error(
            owner,
            Some(location),
            TeletextMainCompositionFailure::ActivePositionOrder {
                previous,
                requested,
            },
        ));
    }
    Ok(())
}

struct MergeOutput {
    count: usize,
    commands: Vec<TeletextEnhancementCommand>,
    provenance: Vec<TeletextMainCompositionSource>,
    retain: bool,
    position: (u8, u8),
    charset: TeletextEnhancementCharsetDesignation,
}

impl MergeOutput {
    fn emit(
        &mut self,
        input: &MergeInput<'_>,
        command: TeletextEnhancementCommand,
        location: TeletextMainCompositionSource,
    ) -> Result<(), TeletextMainCompositionError> {
        if self.count >= input.maximum {
            return Err(error(
                input.owner,
                Some(location),
                TeletextMainCompositionFailure::OutputBudget {
                    maximum: input.maximum,
                },
            ));
        }
        if let Some(requested) = command_position(command, self.position.0) {
            ensure_order(input.owner, location, self.position, requested)?;
            if self.position.0 != requested.0 {
                self.charset = input.primary;
            }
            self.position = requested;
        }
        if let TeletextEnhancementCommand::AtColumn {
            operation: TeletextColumnOperation::DesignateCharset { designation },
            ..
        } = command
        {
            self.charset = designation;
        }
        self.count += 1;
        if self.retain {
            self.commands.push(command);
            self.provenance.push(location);
        }
        Ok(())
    }
}

fn merge(
    input: &MergeInput<'_>,
    compatible: &CompatiblePositions,
    capacity: Option<usize>,
) -> Result<MergeOutput, TeletextMainCompositionError> {
    let mut output = MergeOutput {
        count: 0,
        commands: Vec::with_capacity(capacity.unwrap_or(0)),
        provenance: Vec::with_capacity(capacity.unwrap_or(0)),
        retain: capacity.is_some(),
        position: (0, 0),
        charset: input.primary,
    };
    let mut cursor = 0;
    let mut position = (0, 0);
    let mut charset = input.primary;
    for (ordinal, command) in input.original.commands().iter().copied().enumerate() {
        let location = root_location(input.source, ordinal).ok_or_else(|| {
            error(
                input.owner,
                None,
                TeletextMainCompositionFailure::SelectionState,
            )
        })?;
        let requested = command_position(command, position.0).unwrap_or(position);
        let inclusive = matches!(command, TeletextEnhancementCommand::AtColumn { operation, .. } if enhanced_glyph(operation));
        flush(
            input,
            compatible,
            &mut output,
            &mut cursor,
            FlushContext {
                boundary: requested,
                inclusive,
                original_position: position,
                original_charset: charset,
            },
        )?;
        if requested.0 != position.0 {
            charset = input.primary;
        }
        position = requested;
        if let TeletextEnhancementCommand::AtColumn {
            operation: TeletextColumnOperation::DesignateCharset { designation },
            ..
        } = command
        {
            charset = designation;
        }
        output.emit(input, command, location)?;
    }
    flush(
        input,
        compatible,
        &mut output,
        &mut cursor,
        FlushContext {
            boundary: (24, 39),
            inclusive: true,
            original_position: position,
            original_charset: charset,
        },
    )?;
    Ok(output)
}

#[derive(Clone, Copy)]
struct FlushContext {
    boundary: (u8, u8),
    inclusive: bool,
    original_position: (u8, u8),
    original_charset: TeletextEnhancementCharsetDesignation,
}

fn flush(
    input: &MergeInput<'_>,
    compatible: &CompatiblePositions,
    output: &mut MergeOutput,
    cursor: &mut usize,
    context: FlushContext,
) -> Result<(), TeletextMainCompositionError> {
    while let Some(glyph) = input.glyphs.get(*cursor) {
        let position = glyph.position();
        let key = (position.row(), position.column());
        if key > context.boundary || (key == context.boundary && !context.inclusive) {
            break;
        }
        *cursor += 1;
        if compatible.contains_key(&key) {
            continue;
        }
        let restore = if position.row() == context.original_position.0 {
            context.original_charset
        } else {
            input.primary
        };
        emit_glyph(input, output, *glyph, restore)?;
    }
    Ok(())
}

fn emit_glyph(
    input: &MergeInput<'_>,
    output: &mut MergeOutput,
    glyph: PositionedBaseGlyph,
    restore: TeletextEnhancementCharsetDesignation,
) -> Result<(), TeletextMainCompositionError> {
    let position = glyph.position();
    let location = |kind| TeletextMainCompositionSource::BaseCell { position, kind };
    if output.position.0 != position.row() {
        output.emit(
            input,
            TeletextEnhancementCommand::SetPosition { position },
            location(TeletextBaseCellCommandKind::Position),
        )?;
    }
    if output.charset.pair() != glyph.charset().pair() {
        let kind = if glyph.charset().pair() == TeletextCharsetPair::LatinLatin {
            TeletextBaseCellCommandKind::LatinCharset
        } else {
            TeletextBaseCellCommandKind::GlyphCharset
        };
        output.emit(
            input,
            TeletextEnhancementCommand::AtColumn {
                column: position.column(),
                operation: TeletextColumnOperation::DesignateCharset {
                    designation: glyph.charset(),
                },
            },
            location(kind),
        )?;
    }
    output.emit(
        input,
        TeletextEnhancementCommand::AtColumn {
            column: position.column(),
            operation: TeletextColumnOperation::from(&glyph),
        },
        location(TeletextBaseCellCommandKind::Glyph),
    )?;
    if output.charset != restore {
        output.emit(
            input,
            TeletextEnhancementCommand::AtColumn {
                column: position.column(),
                operation: TeletextColumnOperation::DesignateCharset {
                    designation: restore,
                },
            },
            location(TeletextBaseCellCommandKind::CharsetRestore),
        )?;
    }
    Ok(())
}
