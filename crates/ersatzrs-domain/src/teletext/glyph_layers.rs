//! Original glyph contenders grouped by raw coordinates (EN 300 706 §13.2).
//!
//! All losers and original Base intent remain available. This diagnostic rank
//! does not execute charset or attributes, resolve footprints/panels, authorize
//! Adaptive overlap (Annex A.7), or establish a renderable winner/readiness.
//! Generated compatibility replacements are not promoted to Local candidates.

use super::{
    TeletextBaseDisplay, TeletextBaseDisplayCell, TeletextBaseDisplayCellKind,
    TeletextColumnOperation, TeletextEnhancementCommand, TeletextEnhancementSource,
    TeletextObjectType, TeletextPageAddress, TeletextSpatialEvent, TeletextSpatialInvocationId,
    TeletextSpatialRootSource, TeletextSpatialTrace, TELETEXT_COLUMNS, TELETEXT_ROWS,
};

/// Glyph rank, independent of object source scope and default-selection origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TeletextGlyphPriority {
    Base,
    Active,
    Local,
    Adaptive,
    Passive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Content {
    Base(TeletextBaseDisplayCell),
    Enhancement {
        operation: TeletextColumnOperation,
        event_ordinal: usize,
        source: TeletextEnhancementSource,
        root_source: TeletextSpatialRootSource,
        invocation: Option<TeletextSpatialInvocationId>,
    },
}

/// Original Base cell or one original enhancement glyph; fields are immutable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextGlyphCandidate {
    position: (u16, u16),
    priority: TeletextGlyphPriority,
    content: Content,
}
impl TeletextGlyphCandidate {
    /// Exact raw row/column; no clipping to eventual panel or display geometry.
    #[must_use]
    pub const fn position(&self) -> (u16, u16) {
        self.position
    }
    /// Hypothetical glyph rank only, not rendered validity or attribute rank.
    #[must_use]
    pub const fn priority(&self) -> TeletextGlyphPriority {
        self.priority
    }
    /// Original authored Unicode/mosaic intent, coded role and Base attributes.
    /// These attributes are not resolved attributes for an enhanced candidate.
    #[must_use]
    pub const fn base(&self) -> Option<TeletextBaseDisplayCell> {
        match self.content {
            Content::Base(cell) => Some(cell),
            Content::Enhancement { .. } => None,
        }
    }
    /// Original typed glyph operation; charset execution/raster lookup is pending.
    #[must_use]
    pub const fn operation(&self) -> Option<TeletextColumnOperation> {
        match self.content {
            Content::Base(_) => None,
            Content::Enhancement { operation, .. } => Some(operation),
        }
    }
    /// Original logical event ordinal, not physical packet transmission time.
    #[must_use]
    pub const fn event_ordinal(&self) -> Option<usize> {
        match self.content {
            Content::Base(_) => None,
            Content::Enhancement { event_ordinal, .. } => Some(event_ordinal),
        }
    }
    /// Full original object identity and body/main command ordinal.
    #[must_use]
    pub const fn source(&self) -> Option<TeletextEnhancementSource> {
        match self.content {
            Content::Base(_) => None,
            Content::Enhancement { source, .. } => Some(source),
        }
    }
    /// Original Local command or sparse default slot, never rebound to scope.
    #[must_use]
    pub const fn root_source(&self) -> Option<TeletextSpatialRootSource> {
        match self.content {
            Content::Base(_) => None,
            Content::Enhancement { root_source, .. } => Some(root_source),
        }
    }
    /// Actual executing object instance, or none for main/Base cells.
    #[must_use]
    pub const fn invocation(&self) -> Option<TeletextSpatialInvocationId> {
        match self.content {
            Content::Base(_) => None,
            Content::Enhancement { invocation, .. } => invocation,
        }
    }

    fn ordering_key(
        &self,
    ) -> (
        (u16, u16),
        TeletextGlyphPriority,
        Option<usize>,
        Option<usize>,
    ) {
        (
            self.position,
            self.priority,
            self.invocation().map(TeletextSpatialInvocationId::get),
            self.event_ordinal(),
        )
    }
}

/// One sparse coordinate span in the shared contender vector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextGlyphSlot {
    position: (u16, u16),
    start: usize,
    finish: usize,
}
impl TeletextGlyphSlot {
    /// Exact supplied coordinate, including retained out-of-area/header events.
    #[must_use]
    pub const fn position(&self) -> (u16, u16) {
        self.position
    }
    /// Broad logical address region only: rows 0–24, columns 0–71, excluding
    /// broadcast-owned header columns 0–7 and header side-panel columns 40–71
    /// (EN 300 706 §12.5.2). Actual panel enable/width, written areas, overlap,
    /// footprints and glyph visibility remain unvalidated.
    #[must_use]
    pub const fn logical_addressable(&self) -> bool {
        self.position.0 <= 24
            && self.position.1 <= 71
            && !(self.position.0 == 0 && (self.position.1 < 8 || self.position.1 >= 40))
    }
}

/// Immutable contender index for one exact owner; no generation/publication proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextGlyphLayers {
    owner: TeletextPageAddress,
    candidates: Vec<TeletextGlyphCandidate>,
    slots: Vec<TeletextGlyphSlot>,
}
impl TeletextGlyphLayers {
    /// Exact base page/subcode shared by both input traces.
    #[must_use]
    pub const fn owner(&self) -> TeletextPageAddress {
        self.owner
    }
    /// Grouped by raw position, then ascending priority, invocation ID and original
    /// event ordinal. All losing contenders remain present; this is not source
    /// transmission order. Base records include meaningful control/blank cells.
    #[must_use]
    pub fn candidates(&self) -> &[TeletextGlyphCandidate] {
        &self.candidates
    }
    /// Sparse sorted coordinate spans; no per-cell candidate-vector allocation.
    #[must_use]
    pub fn slots(&self) -> &[TeletextGlyphSlot] {
        &self.slots
    }
    /// All contenders in ascending diagnostic rank/tie order at an exact position.
    #[must_use]
    pub fn candidates_at(&self, row: u16, column: u16) -> &[TeletextGlyphCandidate] {
        self.slots
            .binary_search_by_key(&(row, column), TeletextGlyphSlot::position)
            .map_or(&[], |index| {
                let slot = self.slots[index];
                &self.candidates[slot.start..slot.finish]
            })
    }
    /// Highest diagnostic rank, ignoring attributes, charset execution, geometry,
    /// overlap validity and footprints. Same-class object ties use the latest
    /// actual invocation; repeated writes within that instance/main use the later
    /// original event. In particular, selecting an Adaptive contender here does
    /// not accept otherwise-invalid Adaptive overlap or yield a renderable winner.
    #[must_use]
    pub fn highest_priority_candidate(
        &self,
        row: u16,
        column: u16,
    ) -> Option<&TeletextGlyphCandidate> {
        self.candidates_at(row, column).last()
    }
}

/// Located rejection, retaining only checked identifiers and static reasons.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("teletext glyph layers for {owner:?} at {location:?}: {reason}")]
pub struct TeletextGlyphLayersError {
    pub owner: TeletextPageAddress,
    pub location: Option<TeletextEnhancementSource>,
    pub root_source: Option<TeletextSpatialRootSource>,
    pub invocation: Option<TeletextSpatialInvocationId>,
    pub reason: TeletextGlyphLayersFailure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextGlyphLayersFailure {
    #[error("original spatial trace owner {original:?} differs from Base owner")]
    OwnerMismatch { original: TeletextPageAddress },
    #[error("enhancement glyph candidate budget {maximum} exhausted")]
    CandidateBudget { maximum: usize },
    #[error("spatial event references missing invocation {id:?}")]
    MissingInvocation { id: TeletextSpatialInvocationId },
    #[error("glyph candidate allocation size exceeds storage range")]
    AllocationSize,
}

/// Retain the 992 non-header Base cells and every original enhancement glyph.
///
/// The caller must supply traces from the same immutable service; matching owners
/// alone cannot certify generation identity. Counting and instance validation
/// precede allocation. No generated base replacement, attribute, AP movement or
/// call itself becomes an enhancement glyph. Raw wide/header positions survive.
/// # Errors
/// Rejects different exact owners, the first glyph beyond the trusted enhancement
/// budget, or inconsistent private trace instance references/allocation size.
/// No partial index escapes and neither input is mutated.
pub fn build_teletext_glyph_layers(
    base: &TeletextBaseDisplay,
    original: &TeletextSpatialTrace,
    max_enhancement_candidates: usize,
) -> Result<TeletextGlyphLayers, TeletextGlyphLayersError> {
    let owner = base.owner();
    let fail = |reason| TeletextGlyphLayersError {
        owner,
        location: None,
        root_source: None,
        invocation: None,
        reason,
    };
    if original.owner() != owner {
        return Err(fail(TeletextGlyphLayersFailure::OwnerMismatch {
            original: original.owner(),
        }));
    }
    let mut enhanced = 0_usize;
    for &event in original.events() {
        if glyph_operation(event).is_some() {
            if enhanced >= max_enhancement_candidates {
                return Err(event_error(
                    owner,
                    event,
                    TeletextGlyphLayersFailure::CandidateBudget {
                        maximum: max_enhancement_candidates,
                    },
                ));
            }
            enhanced += 1;
            event_priority(original, event)?;
        }
    }
    let total = (TELETEXT_ROWS * TELETEXT_COLUMNS - 8)
        .checked_add(enhanced)
        .ok_or_else(|| fail(TeletextGlyphLayersFailure::AllocationSize))?;
    let mut candidates = Vec::with_capacity(total);
    for &cell in base.cells().iter().flatten() {
        if cell.kind() != TeletextBaseDisplayCellKind::HeaderOwned {
            candidates.push(TeletextGlyphCandidate {
                position: (
                    u16::from(cell.position().row()),
                    u16::from(cell.position().column()),
                ),
                priority: TeletextGlyphPriority::Base,
                content: Content::Base(cell),
            });
        }
    }
    for (event_ordinal, &event) in original.events().iter().enumerate() {
        if let Some(operation) = glyph_operation(event) {
            candidates.push(TeletextGlyphCandidate {
                position: (
                    event.absolute_after().row(),
                    event.absolute_after().column(),
                ),
                priority: event_priority(original, event)?,
                content: Content::Enhancement {
                    operation,
                    event_ordinal,
                    source: event.source(),
                    root_source: event.root_source(),
                    invocation: event.invocation(),
                },
            });
        }
    }
    // Every enhancement has a unique original event ordinal; Base coordinates
    // are unique. Thus this total key makes unstable sort ties impossible.
    candidates.sort_unstable_by_key(TeletextGlyphCandidate::ordering_key);
    let slots = index_slots(&candidates);
    Ok(TeletextGlyphLayers {
        owner,
        candidates,
        slots,
    })
}

fn glyph_operation(event: TeletextSpatialEvent) -> Option<TeletextColumnOperation> {
    match event.command() {
        TeletextEnhancementCommand::AtColumn {
            operation:
                operation @ (TeletextColumnOperation::G0 { .. }
                | TeletextColumnOperation::G1 { .. }
                | TeletextColumnOperation::G2 { .. }
                | TeletextColumnOperation::G3 { .. }
                | TeletextColumnOperation::G0Diacritic { .. }
                | TeletextColumnOperation::Drcs { .. }),
            ..
        } => Some(operation),
        _ => None,
    }
}

fn event_priority(
    original: &TeletextSpatialTrace,
    event: TeletextSpatialEvent,
) -> Result<TeletextGlyphPriority, TeletextGlyphLayersError> {
    let Some(id) = event.invocation() else {
        return Ok(TeletextGlyphPriority::Local);
    };
    let invocation = original
        .invocations()
        .get(id.get())
        .filter(|value| value.id() == id)
        .ok_or_else(|| {
            event_error(
                original.owner(),
                event,
                TeletextGlyphLayersFailure::MissingInvocation { id },
            )
        })?;
    Ok(match invocation.object_type() {
        TeletextObjectType::Active => TeletextGlyphPriority::Active,
        TeletextObjectType::Adaptive => TeletextGlyphPriority::Adaptive,
        TeletextObjectType::Passive => TeletextGlyphPriority::Passive,
    })
}

fn event_error(
    owner: TeletextPageAddress,
    event: TeletextSpatialEvent,
    reason: TeletextGlyphLayersFailure,
) -> TeletextGlyphLayersError {
    TeletextGlyphLayersError {
        owner,
        location: Some(event.source()),
        root_source: Some(event.root_source()),
        invocation: event.invocation(),
        reason,
    }
}

fn index_slots(candidates: &[TeletextGlyphCandidate]) -> Vec<TeletextGlyphSlot> {
    let count = usize::from(!candidates.is_empty())
        + candidates
            .windows(2)
            .filter(|pair| pair[0].position != pair[1].position)
            .count();
    let mut slots: Vec<TeletextGlyphSlot> = Vec::with_capacity(count);
    for (index, candidate) in candidates.iter().enumerate() {
        match slots.last_mut() {
            Some(slot) if slot.position == candidate.position => slot.finish = index + 1,
            _ => slots.push(TeletextGlyphSlot {
                position: candidate.position,
                start: index,
                finish: index + 1,
            }),
        }
    }
    slots
}
