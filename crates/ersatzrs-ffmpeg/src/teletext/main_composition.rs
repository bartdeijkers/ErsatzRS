//! Ordered, unpublished main-program composition from exact base-cell intent.
//!
//! Original commands retain their values and order. Western Latin replacements
//! carry distinct provenance: this intermediate does not settle object overlap,
//! glyph footprints, side panels, non-Latin fallback reinterpretation or packet
//! capacity. Row-zero commands and original MOT default ownership remain intact
//! for later context-aware wire lowering. Neither this plan nor its successful
//! reference collection grants publication or page readiness.

use std::collections::BTreeMap;

use ersatzrs_domain::{
    resolve_teletext_spatial_trace, TeletextBaseDisplay, TeletextCharsetPair,
    TeletextCharsetValueError, TeletextCollectedEnhancementReferences, TeletextColumnOperation,
    TeletextEffectiveProgramSource, TeletextEnhancementCharsetDesignation,
    TeletextEnhancementCollectionBudget, TeletextEnhancementCollectionFailure,
    TeletextEnhancementCommand, TeletextEnhancementPosition, TeletextEnhancementProgram,
    TeletextEnhancementProgramError, TeletextEnhancementSource, TeletextG3Compatibility,
    TeletextObjectIdentity, TeletextPageAddress, TeletextSemanticService, TeletextServiceError,
    TeletextSpatialTrace, TeletextSpatialTraceBudget, TeletextSpatialTraceError,
};

use super::base_display::{trace_teletext_base_display, TeletextBaseDisplayError};
use super::colour_context::{
    resolve_teletext_colour_context, TeletextColourContext, TeletextColourContextError,
};
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
    /// A body ordinal remains in its original full object identity.
    Object {
        identity: TeletextObjectIdentity,
        command: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TeletextMainCompositionFailure {
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
    #[error(transparent)]
    BaseDisplay(#[from] TeletextBaseDisplayError),
    #[error(transparent)]
    SpatialTrace(#[from] TeletextSpatialTraceError),
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
    spatial_trace: TeletextSpatialTrace,
    collected: TeletextCollectedEnhancementReferences,
}

impl TeletextMainComposition {
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
    /// Original command coordinates and repeated invocation identities, before
    /// base-glyph insertion. Attribute inheritance and footprints remain unresolved.
    #[must_use]
    pub const fn spatial_trace(&self) -> &TeletextSpatialTrace {
        &self.spatial_trace
    }
    #[must_use]
    pub const fn collected(&self) -> &TeletextCollectedEnhancementReferences {
        &self.collected
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
}

/// Compose one exact existing display's effective main with positioned fallback
/// replacements. Raw counts fence work before indexing; a sizing pass charges
/// the exact output before allocating it. Suppression is limited to explicit
/// compatible G2/G0-diacritic/Level-1.5-G3 replacements at that same base cell.
/// Original glyphs, attributes, invocation origins/offsets and aliases survive.
///
/// Temporary Latin designations restore the original same-row designation;
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
    let (source, original) = service
        .effective_program(owner)
        .map_err(|reason| error(owner, None, reason.into()))?;
    check_input(
        owner,
        original.commands().len(),
        0,
        budget.max_input_commands,
    )?;
    let page = service
        .base()
        .pages
        .iter()
        .find(|page| page.address == owner)
        .ok_or_else(|| error(owner, None, TeletextMainCompositionFailure::SelectionState))?;
    let layout = layout::compile_for_composition(page)
        .map_err(|reason| error(owner, None, reason.into()))?;
    check_input(
        owner,
        original.commands().len(),
        layout.base_glyphs().len(),
        budget.max_input_commands,
    )?;
    let primary = service
        .effective_presentation(owner)
        .ok_or_else(|| error(owner, None, TeletextMainCompositionFailure::SelectionState))?
        .primary_designation()
        .and_then(|value| TeletextEnhancementCharsetDesignation::new(value.get()))
        .map_err(|reason| error(owner, None, reason.into()))?;
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
    let presentation = service
        .effective_presentation(owner)
        .ok_or_else(|| error(owner, None, TeletextMainCompositionFailure::SelectionState))?;
    let base_display = trace_teletext_base_display(page, &layout, presentation, &colour_context)
        .map_err(|reason| error(owner, None, reason.into()))?;
    let collected = service
        .objects()
        .collect_display_references(owner, &program, primary.pair(), budget.collection)
        .map_err(|failure| {
            error(
                owner,
                mapped_location(&built.provenance, failure.source_location),
                failure.reason.into(),
            )
        })?;
    let spatial_trace = resolve_teletext_spatial_trace(
        service,
        owner,
        TeletextSpatialTraceBudget {
            max_events: budget.collection.max_commands,
            max_invocations: budget.collection.max_commands,
        },
    )
    .map_err(|reason| {
        let location = reason
            .location
            .and_then(|location| original_location(&source, location));
        error(owner, location, reason.into())
    })?;
    Ok(TeletextMainComposition {
        owner,
        original_program_source: source,
        program,
        provenance: built.provenance,
        colour_context,
        base_display,
        spatial_trace,
        collected,
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
    let latin_needed = output.charset.pair() != TeletextCharsetPair::LatinLatin;
    if latin_needed {
        let designation = TeletextEnhancementCharsetDesignation::new(0).map_err(|reason| {
            error(
                input.owner,
                Some(location(TeletextBaseCellCommandKind::LatinCharset)),
                reason.into(),
            )
        })?;
        output.emit(
            input,
            TeletextEnhancementCommand::AtColumn {
                column: position.column(),
                operation: TeletextColumnOperation::DesignateCharset { designation },
            },
            location(TeletextBaseCellCommandKind::LatinCharset),
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
