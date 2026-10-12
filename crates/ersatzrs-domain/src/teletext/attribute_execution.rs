//! Bounded execution of original four-group attributes, not a full renderer.
//!
//! Charset, full written-area/footprint rules, C.3, Adaptive overlap validity and
//! wire timing remain separate obligations. Exact C.2 r3 ties and ambiguous
//! Adaptive AP/call tails remain typed pending; logical event order is not time.

use super::attribute_observations::{observe_candidates, preflight_observations};

use super::{
    TeletextAttributePending, TeletextAttributePendingReason, TeletextAttributeSource,
    TeletextAttributeTransmissionEvidence, TeletextBaseAttributeSource, TeletextBaseDisplay,
    TeletextBaseDisplayCell, TeletextColourReference, TeletextColumnOperation,
    TeletextEnhancementCommand, TeletextEnhancementSource, TeletextExecutedAttribute,
    TeletextExecutedAttributes, TeletextExecutedFlash, TeletextExecutedGlyphAttributes,
    TeletextFlashMode, TeletextFlashRate, TeletextGlyphLayers, TeletextGlyphPriority,
    TeletextObjectType, TeletextPageAddress, TeletextSpatialEvent, TeletextSpatialInvocationId,
    TeletextSpatialRootSource, TeletextSpatialTrace,
};
use std::collections::BTreeMap;

const WIDTH: usize = 72;
const HEIGHT: usize = 25;

/// Immutable original-attribute projection. None cells are not logical display cells.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextAttributeExecution {
    owner: TeletextPageAddress,
    transmission: Option<TeletextAttributeTransmissionEvidence>,
    cells: Vec<Option<TeletextExecutedAttributes>>,
    before_adaptive: Vec<Option<TeletextExecutedAttributes>>,
    before_passive: Vec<Option<TeletextExecutedAttributes>>,
    records: Vec<TeletextExecutedGlyphAttributes>,
    pending: Vec<TeletextAttributePending>,
}
impl TeletextAttributeExecution {
    /// Final-emitter evidence, absent in standalone analysis. No cross-instance order.
    #[must_use]
    pub fn transmission_evidence(&self) -> Option<&TeletextAttributeTransmissionEvidence> {
        self.transmission.as_ref()
    }

    #[must_use]
    pub const fn owner(&self) -> TeletextPageAddress {
        self.owner
    }
    /// Broad logical areas only; actual panel enable/width is not validated.
    #[must_use]
    pub fn cell(&self, row: u8, column: u8) -> Option<&TeletextExecutedAttributes> {
        stage_cell(&self.cells, row, column)
    }
    /// Shared projection after Local/Active tails, before Adaptive changes.
    /// Original controls/events remain authoritative for future C.3 replay.
    #[must_use]
    pub fn before_adaptive_cell(&self, row: u8, column: u8) -> Option<&TeletextExecutedAttributes> {
        stage_cell(&self.before_adaptive, row, column)
    }
    /// Shared projection after Adaptive changes, before Passive composites.
    /// Neither stage is a footprint/visibility or final-rendering certificate.
    #[must_use]
    pub fn before_passive_cell(&self, row: u8, column: u8) -> Option<&TeletextExecutedAttributes> {
        stage_cell(&self.before_passive, row, column)
    }
    /// Exactly aligned with the original glyph layers, including every loser.
    #[must_use]
    pub fn records(&self) -> &[TeletextExecutedGlyphAttributes] {
        &self.records
    }
    /// Append-only diagnostic provenance, including superseded contenders.
    /// Per-field pending handles identify current value uncertainty; the log
    /// alone does not establish that every recorded tie still affects a cell.
    #[must_use]
    pub fn pending(&self) -> &[TeletextAttributePending] {
        &self.pending
    }
}
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("teletext attribute execution for {owner:?} at {location:?}: {reason}")]
pub struct TeletextAttributeExecutionError {
    pub owner: TeletextPageAddress,
    pub location: Option<TeletextEnhancementSource>,
    pub root_source: Option<TeletextSpatialRootSource>,
    pub invocation: Option<TeletextSpatialInvocationId>,
    pub reason: TeletextAttributeExecutionFailure,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextAttributeExecutionFailure {
    #[error("input owner {other:?} differs from Base owner")]
    OwnerMismatch { other: TeletextPageAddress },
    #[error("candidate count {count} exceeds budget {maximum}")]
    CandidateBudget { count: usize, maximum: usize },
    #[error("original candidate {candidate} has inconsistent Base/event source")]
    CandidateMismatch { candidate: usize },
    #[error("original input correspondence failed: {0}")]
    OriginalSource(super::TeletextDisplayGeometryFailure),
    #[error("attribute observation storage/work arithmetic exceeds supported range")]
    AllocationSize,
    #[error("original command budget {maximum} exhausted")]
    CommandBudget { maximum: usize },
    #[error("transmission evidence does not match exact original trace")]
    TransmissionMismatch,
    #[error("bounded attribute update budget exceeds storage range")]
    WorkBudget,
    #[error("missing original invocation {id:?}")]
    MissingInvocation { id: TeletextSpatialInvocationId },
}

/// Execute original attributes over actual checked Base spacing state.
///
/// Matching owners alone is not generation proof: the authoritative caller must
/// supply all inputs from the same immutable service. Explicit defaults never
/// defeat explicit commands. C.2 compares effective start then placement, while
/// exact ties retain every contender through bounded linked pending records.
/// # Errors
/// Rejects different owners or the first original command beyond the trusted
/// limit, candidate count/correspondence, invalid private instance references,
/// or storage/work arithmetic overflow. Original-input validation does not run
/// the geometry region or simultaneous-open gates.
/// No partial result escapes and inputs remain unchanged.
pub fn resolve_teletext_attribute_execution(
    base: &TeletextBaseDisplay,
    original: &TeletextSpatialTrace,
    glyph_layers: &TeletextGlyphLayers,
    row_colours: &[TeletextColourReference; 25],
    max_commands: usize,
) -> Result<TeletextAttributeExecution, TeletextAttributeExecutionError> {
    resolve_teletext_attribute_execution_with_usage(
        base,
        original,
        glyph_layers,
        row_colours,
        max_commands,
        None,
    )
    .0
}

/// Execute unchanged attributes with optional admitted cell-update credit.
/// The existing event-derived stage ceiling still applies. Only tail, Adaptive
/// and winning Passive updates are counted; scans, snapshots, private state and
/// candidate observations are not cell updates or total CPU/memory accounting.
/// # Errors
/// Preserves ordinary located failures and reports prior admitted updates on
/// either outcome. Denied updates and preflight/initialization failures charge zero.
pub fn resolve_teletext_attribute_execution_with_usage(
    base: &TeletextBaseDisplay,
    original: &TeletextSpatialTrace,
    glyph_layers: &TeletextGlyphLayers,
    row_colours: &[TeletextColourReference; 25],
    max_commands: usize,
    max_updates: Option<usize>,
) -> (
    Result<TeletextAttributeExecution, TeletextAttributeExecutionError>,
    usize,
) {
    execute_attributes(
        base,
        original,
        glyph_layers,
        row_colours,
        max_commands,
        None,
        max_updates,
    )
}

/// Execute with actual final producer positions; unknown/sparse emitter ties stay pending.
/// # Errors
/// In addition to ordinary execution errors, rejects evidence from a different
/// exact original trace. Matching page IDs alone never establish correspondence.
pub fn resolve_teletext_attribute_execution_with_transmission(
    base: &TeletextBaseDisplay,
    original: &TeletextSpatialTrace,
    glyph_layers: &TeletextGlyphLayers,
    row_colours: &[TeletextColourReference; 25],
    max_commands: usize,
    transmission: &TeletextAttributeTransmissionEvidence,
) -> Result<TeletextAttributeExecution, TeletextAttributeExecutionError> {
    resolve_teletext_attribute_execution_with_transmission_and_usage(
        base,
        original,
        glyph_layers,
        row_colours,
        max_commands,
        transmission,
        None,
    )
    .0
}

/// Execute exact transmission settlement with optional cell-update credit.
/// Usage remains outside the immutable model and includes only the existing
/// admitted update meter, retaining earlier charges after a later failure.
/// # Errors
/// Preserves ordinary located errors and exact-trace transmission admission.
pub fn resolve_teletext_attribute_execution_with_transmission_and_usage(
    base: &TeletextBaseDisplay,
    original: &TeletextSpatialTrace,
    glyph_layers: &TeletextGlyphLayers,
    row_colours: &[TeletextColourReference; 25],
    max_commands: usize,
    transmission: &TeletextAttributeTransmissionEvidence,
    max_updates: Option<usize>,
) -> (
    Result<TeletextAttributeExecution, TeletextAttributeExecutionError>,
    usize,
) {
    execute_attributes(
        base,
        original,
        glyph_layers,
        row_colours,
        max_commands,
        Some(transmission),
        max_updates,
    )
}

fn execute_attributes(
    base: &TeletextBaseDisplay,
    original: &TeletextSpatialTrace,
    glyph_layers: &TeletextGlyphLayers,
    row_colours: &[TeletextColourReference; 25],
    max_commands: usize,
    transmission: Option<&TeletextAttributeTransmissionEvidence>,
    max_updates: Option<usize>,
) -> (
    Result<TeletextAttributeExecution, TeletextAttributeExecutionError>,
    usize,
) {
    let (mut executor, diagnostic_limit) = match prepare_executor(
        base,
        original,
        glyph_layers,
        row_colours,
        max_commands,
        transmission,
        max_updates,
    ) {
        Ok(prepared) => prepared,
        Err(error) => return (Err(error), 0),
    };
    let initial_credit = executor.remaining;
    let result = (|| {
        executor.capture_unaddressable();
        executor.execute_tails()?;
        let before_adaptive = executor.cells.clone();
        executor.execute_adaptive()?;
        let before_passive = executor.cells.clone();
        let mut private = vec![None; original.events().len()];
        executor.execute_passive(&mut private)?;
        let records = observe_candidates(glyph_layers, &before_passive, &private)?;
        if executor.pending.len() > diagnostic_limit {
            return Err(TeletextAttributeExecutionError {
                owner: base.owner(),
                location: None,
                root_source: None,
                invocation: None,
                reason: TeletextAttributeExecutionFailure::WorkBudget,
            });
        }
        Ok((before_adaptive, before_passive, records))
    })();
    let admitted = initial_credit - executor.remaining;
    (
        result.map(
            |(before_adaptive, before_passive, records)| TeletextAttributeExecution {
                owner: base.owner(),
                transmission: transmission.cloned(),
                cells: executor.cells,
                before_adaptive,
                before_passive,
                records,
                pending: executor.pending,
            },
        ),
        admitted,
    )
}

fn prepare_executor<'a>(
    base: &TeletextBaseDisplay,
    original: &'a TeletextSpatialTrace,
    glyph_layers: &'a TeletextGlyphLayers,
    row_colours: &[TeletextColourReference; 25],
    max_commands: usize,
    transmission: Option<&'a TeletextAttributeTransmissionEvidence>,
    max_updates: Option<usize>,
) -> Result<(Executor<'a>, usize), TeletextAttributeExecutionError> {
    let owner = base.owner();
    let fail = |reason| TeletextAttributeExecutionError {
        owner,
        location: None,
        root_source: None,
        invocation: None,
        reason,
    };
    for other in [original.owner(), glyph_layers.owner()] {
        if other != owner {
            return Err(fail(TeletextAttributeExecutionFailure::OwnerMismatch {
                other,
            }));
        }
    }
    if let Some(&event) = original.events().get(max_commands) {
        return Err(event_error(
            owner,
            event,
            TeletextAttributeExecutionFailure::CommandBudget {
                maximum: max_commands,
            },
        ));
    }
    if transmission.is_some_and(|evidence| !evidence.matches(original)) {
        return Err(fail(
            TeletextAttributeExecutionFailure::TransmissionMismatch,
        ));
    }
    let maximum = original
        .events()
        .len()
        .checked_mul(WIDTH)
        .ok_or_else(|| fail(TeletextAttributeExecutionFailure::WorkBudget))?;
    // Nine resolved fields per cell (three colour/flash and six Display),
    // plus private-state, source capture and tail records.
    // This bounds diagnostics without reserving a producer-sized buffer.
    let diagnostic_limit = original
        .events()
        .len()
        .checked_mul(WIDTH * 9 + 13)
        .ok_or_else(|| fail(TeletextAttributeExecutionFailure::WorkBudget))?;
    preflight_observations(base, original, glyph_layers, max_commands)?;
    let cells = initial_cells(base, row_colours)?;
    let executor = Executor {
        transmission,
        original,
        layers: glyph_layers,
        cells,
        pending: Vec::new(),
        remaining: max_updates.map_or(maximum, |limit| maximum.min(limit)),
    };
    Ok((executor, diagnostic_limit))
}

fn stage_cell(
    cells: &[Option<TeletextExecutedAttributes>],
    row: u8,
    column: u8,
) -> Option<&TeletextExecutedAttributes> {
    if usize::from(row) >= HEIGHT || usize::from(column) >= WIDTH {
        return None;
    }
    cells[usize::from(row) * WIDTH + usize::from(column)].as_ref()
}

fn initial_cells(
    base: &TeletextBaseDisplay,
    row_colours: &[TeletextColourReference; 25],
) -> Result<Vec<Option<TeletextExecutedAttributes>>, TeletextAttributeExecutionError> {
    let mut cells = Vec::with_capacity(HEIGHT * WIDTH);
    for (row, &row_colour) in row_colours.iter().enumerate() {
        for column in 0..WIDTH {
            cells.push(if row == 0 && !(8..40).contains(&column) {
                None
            } else if column < 40 {
                base.cells()[row].get(column).copied().map(base_attributes)
            } else {
                Some(default_attributes(
                    TeletextAttributeSource::PanelDefault {
                        row: u8::try_from(row).map_err(|_| TeletextAttributeExecutionError {
                            owner: base.owner(),
                            location: None,
                            root_source: None,
                            invocation: None,
                            reason: TeletextAttributeExecutionFailure::WorkBudget,
                        })?,
                    },
                    row_colour,
                ))
            });
        }
    }
    Ok(cells)
}

fn base_attributes(cell: TeletextBaseDisplayCell) -> TeletextExecutedAttributes {
    let values = cell.attributes().values();
    let origins = cell.attributes().sources();
    let position = (
        u16::from(cell.position().row()),
        u16::from(cell.position().column()),
    );
    let source = |source| TeletextAttributeSource::Base { position, source };
    let default = source(TeletextBaseAttributeSource::RowDefault {
        row: cell.position().row(),
        start_column: if cell.position().row() == 0 { 8 } else { 0 },
    });
    TeletextExecutedAttributes {
        foreground: TeletextExecutedAttribute::known(values.foreground, source(origins.foreground)),
        background: TeletextExecutedAttribute::known(values.background, source(origins.background)),
        flash: TeletextExecutedAttribute::known(
            TeletextExecutedFlash {
                mode: if values.flash {
                    TeletextFlashMode::Normal
                } else {
                    TeletextFlashMode::Steady
                },
                rate: TeletextFlashRate::Slow1Hz,
            },
            source(origins.flash),
        ),
        double_height: TeletextExecutedAttribute::known(
            values.double_height,
            source(origins.height),
        ),
        double_width: TeletextExecutedAttribute::known(false, source(origins.height)),
        boxed: TeletextExecutedAttribute::known(values.boxed, source(origins.boxed)),
        conceal: TeletextExecutedAttribute::known(values.conceal, source(origins.conceal)),
        invert: TeletextExecutedAttribute::known(false, default),
        underline_or_separated_mosaics: TeletextExecutedAttribute::known(false, default),
        spacing_separated: TeletextExecutedAttribute::known(
            values.separated,
            source(origins.separated),
        ),
    }
}
fn default_attributes(
    source: TeletextAttributeSource,
    background: TeletextColourReference,
) -> TeletextExecutedAttributes {
    let flag = TeletextExecutedAttribute::known(false, source);
    TeletextExecutedAttributes {
        foreground: TeletextExecutedAttribute::known(TeletextColourReference::WHITE, source),
        background: TeletextExecutedAttribute::known(background, source),
        flash: TeletextExecutedAttribute::known(
            TeletextExecutedFlash {
                mode: TeletextFlashMode::Steady,
                rate: TeletextFlashRate::Slow1Hz,
            },
            source,
        ),
        double_height: flag,
        double_width: flag,
        boxed: flag,
        conceal: flag,
        invert: flag,
        underline_or_separated_mosaics: flag,
        spacing_separated: flag,
    }
}

fn event_type(
    original: &TeletextSpatialTrace,
    event: TeletextSpatialEvent,
) -> Result<Option<TeletextObjectType>, TeletextAttributeExecutionError> {
    event
        .invocation()
        .map(|id| {
            original
                .invocations()
                .get(id.get())
                .filter(|entry| entry.id() == id)
                .map(|entry| entry.object_type())
                .ok_or_else(|| {
                    event_error(
                        original.owner(),
                        event,
                        TeletextAttributeExecutionFailure::MissingInvocation { id },
                    )
                })
        })
        .transpose()
}
fn event_error(
    owner: TeletextPageAddress,
    event: TeletextSpatialEvent,
    reason: TeletextAttributeExecutionFailure,
) -> TeletextAttributeExecutionError {
    TeletextAttributeExecutionError {
        owner,
        location: Some(event.source()),
        root_source: Some(event.root_source()),
        invocation: event.invocation(),
        reason,
    }
}
fn position(event: TeletextSpatialEvent) -> (u16, u16) {
    (
        event.absolute_after().row(),
        event.absolute_after().column(),
    )
}
pub(super) fn area(position: (u16, u16)) -> Option<(usize, usize, usize)> {
    let (row, column) = position;
    if row >= 25 || column >= 72 || (row == 0 && !(8..40).contains(&column)) {
        return None;
    }
    Some((
        usize::from(row),
        usize::from(column),
        match column {
            0..=39 => 39,
            40..=55 => 55,
            _ => 71,
        },
    ))
}
fn event_source(ordinal: usize, event: TeletextSpatialEvent) -> TeletextAttributeSource {
    TeletextAttributeSource::Enhancement {
        event_ordinal: ordinal,
        location: event.source(),
        root_source: event.root_source(),
        invocation: event.invocation(),
        effective_start: position(event),
        placed_position: position(event),
    }
}
fn pending_event(
    event: TeletextSpatialEvent,
    reason: TeletextAttributePendingReason,
    at: (u16, u16),
) -> TeletextAttributePending {
    TeletextAttributePending {
        position: at,
        location: event.source(),
        root_source: event.root_source(),
        invocation: event.invocation(),
        reason,
        first_source: None,
        second_source: None,
        previous: None,
        incoming_previous: None,
    }
}

#[derive(Clone, Copy, Default)]
struct Overrides {
    foreground: Option<TeletextExecutedAttribute<TeletextColourReference>>,
    background: Option<TeletextExecutedAttribute<TeletextColourReference>>,
    flash: Option<TeletextExecutedAttribute<TeletextExecutedFlash>>,
    double_height: Option<TeletextExecutedAttribute<bool>>,
    double_width: Option<TeletextExecutedAttribute<bool>>,
    boxed: Option<TeletextExecutedAttribute<bool>>,
    conceal: Option<TeletextExecutedAttribute<bool>>,
    invert: Option<TeletextExecutedAttribute<bool>>,
    d5: Option<TeletextExecutedAttribute<bool>>,
}
#[derive(Clone, Copy)]
struct Extent {
    first: usize,
    last: usize,
    last_column: Option<usize>,
    tail: Option<(usize, TeletextSpatialEvent)>,
}
type ExtentKey = (usize, usize, usize);
struct Executor<'a> {
    transmission: Option<&'a TeletextAttributeTransmissionEvidence>,
    original: &'a TeletextSpatialTrace,
    layers: &'a TeletextGlyphLayers,
    cells: Vec<Option<TeletextExecutedAttributes>>,
    pending: Vec<TeletextAttributePending>,
    remaining: usize,
}
impl Executor<'_> {
    fn capture_unaddressable(&mut self) {
        for &event in self.original.events() {
            if matches!(event.command(), TeletextEnhancementCommand::AtColumn { .. })
                && area(position(event)).is_none()
            {
                self.pending.push(pending_event(
                    event,
                    TeletextAttributePendingReason::Unaddressable,
                    position(event),
                ));
            }
        }
    }
    fn charge(
        &mut self,
        event: TeletextSpatialEvent,
    ) -> Result<(), TeletextAttributeExecutionError> {
        self.remaining = self.remaining.checked_sub(1).ok_or_else(|| {
            event_error(
                self.original.owner(),
                event,
                TeletextAttributeExecutionFailure::WorkBudget,
            )
        })?;
        Ok(())
    }
    fn execute_tails(&mut self) -> Result<(), TeletextAttributeExecutionError> {
        for (ordinal, &event) in self.original.events().iter().enumerate() {
            if matches!(
                event_type(self.original, event)?,
                None | Some(TeletextObjectType::Active)
            ) {
                if let TeletextEnhancementCommand::AtColumn { operation, .. } = event.command() {
                    if let Some((row, column, finish)) = area(position(event)) {
                        if is_attribute(operation) {
                            for col in column..=finish {
                                self.charge(event)?;
                                let index = row * WIDTH + col;
                                if let Some(mut cell) = self.cells[index] {
                                    apply_operation(
                                        &mut cell,
                                        operation,
                                        event_source(ordinal, event),
                                        event,
                                        coordinate(row, col, self.original.owner(), event)?,
                                        &mut AttributeMerge {
                                            pending: &mut self.pending,
                                            transmission: self.transmission,
                                        },
                                    );
                                    self.cells[index] = Some(cell);
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
    fn adaptive_extents(
        &self,
    ) -> Result<BTreeMap<ExtentKey, Extent>, TeletextAttributeExecutionError> {
        let mut extents: BTreeMap<ExtentKey, Extent> = BTreeMap::new();
        for (ordinal, &event) in self.original.events().iter().enumerate() {
            if event_type(self.original, event)? != Some(TeletextObjectType::Adaptive) {
                continue;
            }
            let Some(id) = event.invocation() else {
                continue;
            };
            let Some((row, column, end)) = area(position(event)) else {
                continue;
            };
            let key = (id.get(), row, end);
            let extent = extents.entry(key).or_insert(Extent {
                first: column,
                last: column,
                last_column: None,
                tail: None,
            });
            match event.command() {
                TeletextEnhancementCommand::AtColumn { .. } => {
                    extent.first = extent.first.min(column);
                    extent.last = extent.last.max(column);
                    extent.last_column = Some(ordinal);
                }
                TeletextEnhancementCommand::SetPosition { .. }
                | TeletextEnhancementCommand::SetRowColour { .. }
                | TeletextEnhancementCommand::InvokeObject { .. } => {
                    extent.tail = Some((ordinal, event));
                }
                TeletextEnhancementCommand::SetScreenColour { .. } => {}
            }
        }
        Ok(extents)
    }
    fn execute_adaptive(&mut self) -> Result<(), TeletextAttributeExecutionError> {
        let extents = self.adaptive_extents()?;
        let mut states: BTreeMap<ExtentKey, Overrides> = BTreeMap::new();
        for (ordinal, &event) in self.original.events().iter().enumerate() {
            if event_type(self.original, event)? != Some(TeletextObjectType::Adaptive) {
                continue;
            }
            let Some(id) = event.invocation() else {
                continue;
            };
            let Some((row, column, end)) = area(position(event)) else {
                continue;
            };
            if let TeletextEnhancementCommand::AtColumn { operation, .. } = event.command() {
                let key = (id.get(), row, end);
                let Some(extent) = extents.get(&key) else {
                    continue;
                };
                let state = states.entry(key).or_default();
                update_overrides(
                    state,
                    operation,
                    event_source(ordinal, event),
                    event,
                    position(event),
                    &mut AttributeMerge {
                        pending: &mut self.pending,
                        transmission: self.transmission,
                    },
                );
                for col in column..=extent.last {
                    self.charge(event)?;
                    let index = row * WIDTH + col;
                    if let Some(mut cell) = self.cells[index] {
                        project_overrides(
                            &mut cell,
                            state,
                            self.original,
                            event,
                            coordinate(row, col, self.original.owner(), event)?,
                            &mut AttributeMerge {
                                pending: &mut self.pending,
                                transmission: self.transmission,
                            },
                        );
                        self.cells[index] = Some(cell);
                    }
                }
            }
        }
        // AP/call-only endings cannot certify a final Adaptive endpoint. Mark
        // only explicitly changed fields in the potentially affected same area.
        for (key, extent) in extents {
            if let Some((ordinal, event)) = extent.tail {
                if extent.last_column.is_none_or(|last| ordinal > last) {
                    self.mark_adaptive_tail(
                        key,
                        extent,
                        event,
                        states.get(&key).unwrap_or(&Overrides::default()),
                    )?;
                }
            }
        }
        Ok(())
    }
    fn mark_adaptive_tail(
        &mut self,
        key: ExtentKey,
        extent: Extent,
        event: TeletextSpatialEvent,
        state: &Overrides,
    ) -> Result<(), TeletextAttributeExecutionError> {
        let row = key.1;
        let start = if extent.last_column.is_some() {
            extent.last + 1
        } else {
            extent.first
        };
        self.pending.push(pending_event(
            event,
            TeletextAttributePendingReason::AdaptiveExtent,
            position(event),
        ));
        for column in start..=key.2 {
            self.charge(event)?;
            let at = coordinate(row, column, self.original.owner(), event)?;
            if let Some(cell) = &mut self.cells[row * WIDTH + column] {
                mark_overrides_pending(cell, state, event, at, &mut self.pending);
            }
        }
        Ok(())
    }
    fn execute_passive(
        &mut self,
        private: &mut [Option<TeletextExecutedAttributes>],
    ) -> Result<(), TeletextAttributeExecutionError> {
        let mut states: BTreeMap<usize, TeletextExecutedAttributes> = BTreeMap::new();
        for (ordinal, &event) in self.original.events().iter().enumerate() {
            if event_type(self.original, event)? != Some(TeletextObjectType::Passive) {
                continue;
            }
            let Some(id) = event.invocation() else {
                continue;
            };
            if let TeletextEnhancementCommand::AtColumn { operation, .. } = event.command() {
                let state = states.entry(id.get()).or_insert_with(|| {
                    default_attributes(
                        TeletextAttributeSource::PassiveDefault { invocation: id },
                        TeletextColourReference::BLACK,
                    )
                });
                if is_attribute(operation) {
                    apply_operation(
                        state,
                        operation,
                        event_source(ordinal, event),
                        event,
                        position(event),
                        &mut AttributeMerge {
                            pending: &mut self.pending,
                            transmission: self.transmission,
                        },
                    );
                }
                if is_glyph(operation) {
                    // Capture every private glyph before placement/winner tests.
                    // Losing, wide and header candidates retain their own state.
                    private[ordinal] = Some(*state);
                    if let Some((row, column, _)) = area(position(event)) {
                        if self
                            .layers
                            .highest_priority_candidate(position(event).0, position(event).1)
                            .is_some_and(|candidate| {
                                candidate.priority() == TeletextGlyphPriority::Passive
                                    && candidate.invocation() == Some(id)
                            })
                        {
                            self.charge(event)?;
                            self.cells[row * WIDTH + column] = Some(*state);
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

fn is_attribute(operation: TeletextColumnOperation) -> bool {
    matches!(
        operation,
        TeletextColumnOperation::Foreground { .. }
            | TeletextColumnOperation::Background { .. }
            | TeletextColumnOperation::Flash { .. }
            | TeletextColumnOperation::Attributes { .. }
    )
}
fn is_glyph(operation: TeletextColumnOperation) -> bool {
    matches!(
        operation,
        TeletextColumnOperation::G0 { .. }
            | TeletextColumnOperation::G1 { .. }
            | TeletextColumnOperation::G1Alpha { .. }
            | TeletextColumnOperation::G2 { .. }
            | TeletextColumnOperation::G3 { .. }
            | TeletextColumnOperation::G0Diacritic { .. }
            | TeletextColumnOperation::Drcs { .. }
    )
}

struct AttributeMerge<'a> {
    pending: &'a mut Vec<TeletextAttributePending>,
    transmission: Option<&'a TeletextAttributeTransmissionEvidence>,
}
fn merge<T: Copy>(
    current: &mut TeletextExecutedAttribute<T>,
    incoming: TeletextExecutedAttribute<T>,
    event: TeletextSpatialEvent,
    at: (u16, u16),
    context: &mut AttributeMerge<'_>,
) {
    match incoming.source.key().cmp(&current.source.key()) {
        std::cmp::Ordering::Greater => *current = incoming,
        std::cmp::Ordering::Equal
            if current.source != incoming.source && incoming.source.key().is_some() =>
        {
            // Existing unresolved branches cannot be erased by resolving only
            // the latest pair. A different emitter remains a genuine unknown.
            if current.pending.is_none() && incoming.pending.is_none() {
                if let Some(order) = context
                    .transmission
                    .and_then(|evidence| evidence.compare(incoming.source, current.source))
                {
                    if order.is_gt() {
                        *current = incoming;
                    }
                    return;
                }
            }
            let index = context.pending.len();
            let mut record =
                pending_event(event, TeletextAttributePendingReason::TransmissionOrder, at);
            record.first_source = Some(current.source);
            record.second_source = Some(incoming.source);
            record.previous = current.pending;
            record.incoming_previous = incoming.pending;
            context.pending.push(record);
            *current = TeletextExecutedAttribute {
                value: None,
                source: incoming.source,
                pending: Some(index),
            };
        }
        _ => {}
    }
}
fn apply_operation(
    cell: &mut TeletextExecutedAttributes,
    operation: TeletextColumnOperation,
    source: TeletextAttributeSource,
    event: TeletextSpatialEvent,
    at: (u16, u16),
    context: &mut AttributeMerge<'_>,
) {
    match operation {
        TeletextColumnOperation::Foreground { colour } => merge(
            &mut cell.foreground,
            TeletextExecutedAttribute::known(colour, source),
            event,
            at,
            context,
        ),
        TeletextColumnOperation::Background { colour } => merge(
            &mut cell.background,
            TeletextExecutedAttribute::known(colour, source),
            event,
            at,
            context,
        ),
        TeletextColumnOperation::Flash { mode, rate } => merge(
            &mut cell.flash,
            TeletextExecutedAttribute::known(TeletextExecutedFlash { mode, rate }, source),
            event,
            at,
            context,
        ),
        TeletextColumnOperation::Attributes {
            double_height,
            double_width,
            boxed,
            conceal,
            invert,
            underline_or_separated_mosaics,
        } => {
            for (field, value) in [
                (&mut cell.double_height, double_height),
                (&mut cell.double_width, double_width),
                (&mut cell.boxed, boxed),
                (&mut cell.conceal, conceal),
                (&mut cell.invert, invert),
                (
                    &mut cell.underline_or_separated_mosaics,
                    underline_or_separated_mosaics,
                ),
            ] {
                merge(
                    field,
                    TeletextExecutedAttribute::known(value, source),
                    event,
                    at,
                    context,
                );
            }
        }
        _ => {}
    }
}
fn update_overrides(
    state: &mut Overrides,
    operation: TeletextColumnOperation,
    source: TeletextAttributeSource,
    event: TeletextSpatialEvent,
    at: (u16, u16),
    context: &mut AttributeMerge<'_>,
) {
    fn update<T: Copy>(
        field: &mut Option<TeletextExecutedAttribute<T>>,
        value: T,
        source: TeletextAttributeSource,
        event: TeletextSpatialEvent,
        at: (u16, u16),
        context: &mut AttributeMerge<'_>,
    ) {
        let next = TeletextExecutedAttribute::known(value, source);
        if let Some(current) = field {
            merge(current, next, event, at, context);
        } else {
            *field = Some(next);
        }
    }
    match operation {
        TeletextColumnOperation::Foreground { colour } => {
            update(&mut state.foreground, colour, source, event, at, context);
        }
        TeletextColumnOperation::Background { colour } => {
            update(&mut state.background, colour, source, event, at, context);
        }
        TeletextColumnOperation::Flash { mode, rate } => update(
            &mut state.flash,
            TeletextExecutedFlash { mode, rate },
            source,
            event,
            at,
            context,
        ),
        TeletextColumnOperation::Attributes {
            double_height,
            double_width,
            boxed,
            conceal,
            invert,
            underline_or_separated_mosaics,
        } => {
            for (field, value) in [
                (&mut state.double_height, double_height),
                (&mut state.double_width, double_width),
                (&mut state.boxed, boxed),
                (&mut state.conceal, conceal),
                (&mut state.invert, invert),
                (&mut state.d5, underline_or_separated_mosaics),
            ] {
                update(field, value, source, event, at, context);
            }
        }
        _ => {}
    }
}
fn project_overrides(
    cell: &mut TeletextExecutedAttributes,
    state: &Overrides,
    original: &TeletextSpatialTrace,
    event: TeletextSpatialEvent,
    at: (u16, u16),
    context: &mut AttributeMerge<'_>,
) {
    fn project<T: Copy>(
        field: &mut TeletextExecutedAttribute<T>,
        value: Option<TeletextExecutedAttribute<T>>,
        original: &TeletextSpatialTrace,
        event: TeletextSpatialEvent,
        at: (u16, u16),
        context: &mut AttributeMerge<'_>,
    ) {
        if let Some(value) = value {
            let adaptive = match field.source {
                TeletextAttributeSource::Enhancement {
                    invocation: Some(id),
                    ..
                } => original
                    .invocations()
                    .get(id.get())
                    .is_some_and(|entry| entry.object_type() == TeletextObjectType::Adaptive),
                _ => false,
            };
            if adaptive {
                merge(field, value, event, at, context);
            } else {
                *field = value;
            }
        }
    }
    project(
        &mut cell.foreground,
        state.foreground,
        original,
        event,
        at,
        context,
    );
    project(
        &mut cell.background,
        state.background,
        original,
        event,
        at,
        context,
    );
    project(&mut cell.flash, state.flash, original, event, at, context);
    for (field, value) in [
        (&mut cell.double_height, state.double_height),
        (&mut cell.double_width, state.double_width),
        (&mut cell.boxed, state.boxed),
        (&mut cell.conceal, state.conceal),
        (&mut cell.invert, state.invert),
        (&mut cell.underline_or_separated_mosaics, state.d5),
    ] {
        project(field, value, original, event, at, context);
    }
}
fn mark_overrides_pending(
    cell: &mut TeletextExecutedAttributes,
    state: &Overrides,
    event: TeletextSpatialEvent,
    at: (u16, u16),
    pending: &mut Vec<TeletextAttributePending>,
) {
    fn mark<T: Copy>(
        field: &mut TeletextExecutedAttribute<T>,
        changed: Option<TeletextExecutedAttribute<T>>,
        event: TeletextSpatialEvent,
        at: (u16, u16),
        pending: &mut Vec<TeletextAttributePending>,
    ) {
        if let Some(changed) = changed {
            let index = pending.len();
            let mut record =
                pending_event(event, TeletextAttributePendingReason::AdaptiveExtent, at);
            record.first_source = Some(changed.source);
            record.second_source = Some(field.source);
            record.previous = changed.pending;
            record.incoming_previous = field.pending;
            pending.push(record);
            field.value = None;
            field.pending = Some(index);
        }
    }
    mark(&mut cell.foreground, state.foreground, event, at, pending);
    mark(&mut cell.background, state.background, event, at, pending);
    mark(&mut cell.flash, state.flash, event, at, pending);
    for (field, changed) in [
        (&mut cell.double_height, state.double_height),
        (&mut cell.double_width, state.double_width),
        (&mut cell.boxed, state.boxed),
        (&mut cell.conceal, state.conceal),
        (&mut cell.invert, state.invert),
        (&mut cell.underline_or_separated_mosaics, state.d5),
    ] {
        mark(field, changed, event, at, pending);
    }
}

fn coordinate(
    row: usize,
    column: usize,
    owner: TeletextPageAddress,
    event: TeletextSpatialEvent,
) -> Result<(u16, u16), TeletextAttributeExecutionError> {
    let fail = || event_error(owner, event, TeletextAttributeExecutionFailure::WorkBudget);
    Ok((
        u16::try_from(row).map_err(|_| fail())?,
        u16::try_from(column).map_err(|_| fail())?,
    ))
}
