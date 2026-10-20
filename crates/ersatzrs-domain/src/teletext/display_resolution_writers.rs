//! Original compact writer replay and separate stage coverage.

use super::display_resolution::{
    event_error, TeletextDisplayResolutionBudget, TeletextDisplayResolutionError,
    TeletextDisplayResolutionFailure, TeletextDisplayResolutionInputs,
};
use super::display_resolution_values::{
    Cell, Selection, State, TeletextDisplayFragment, TeletextDisplayOrigin, TeletextDisplayPending,
    TeletextDisplayPendingReason, TeletextDisplayResolution, TeletextDisplaySize,
    TeletextDisplayWriter, Value, BG, BOXED, CONCEAL, D5, DH, DW, FG, FIELDS, FLASH, INVERT,
    SPACING,
};
use super::{
    TeletextAttributeSource, TeletextBaseAttributeSource, TeletextBaseDisplayCell,
    TeletextBaseDisplayCellKind, TeletextBaseSpacingControl, TeletextCandidatePlacement,
    TeletextColourReference, TeletextColumnOperation, TeletextDisplayGeometry,
    TeletextEnhancementCommand, TeletextEnhancementSource, TeletextExecutedFlash,
    TeletextFlashMode, TeletextFlashRate, TeletextGlyphDisposition, TeletextGlyphPriority,
    TeletextObjectType, TeletextSpacingTiming, TeletextSpatialEvent, TeletextSpatialInvocation,
    TeletextSpatialInvocationId, TeletextSpatialPosition, TeletextSpatialRootSource,
};

#[derive(Clone, Copy)]
struct Action {
    at: usize,
    writer: usize,
}

struct ObjectExecution {
    private: bool,
    state: State,
    changed: [Option<Selection>; FIELDS],
    coverage: Vec<Option<usize>>,
    deferred: Vec<usize>,
    current_end: usize,
    action_index: usize,
    candidate_index: usize,
    extents: Vec<(usize, usize)>,
    tails: Vec<Option<TeletextSpatialEvent>>,
}
#[derive(Clone, Copy)]
struct ObjectCell<'a> {
    at: usize,
    hidden: bool,
    group: &'a [(usize, usize)],
}

pub(super) struct Builder<'a> {
    input: TeletextDisplayResolutionInputs<'a>,
    budget: TeletextDisplayResolutionBudget,
    output: TeletextDisplayResolution,
    defaults: [usize; 25],
    panels: [usize; 25],
    event_writers: Vec<Option<usize>>,
    base_writers: Vec<Action>,
    updates: usize,
    shared: Vec<Option<Cell>>,
    private_states: Vec<Option<State>>,
    adaptive_states: Vec<Option<[Option<Selection>; FIELDS]>>,
}

impl<'a> Builder<'a> {
    pub(super) fn new(
        input: TeletextDisplayResolutionInputs<'a>,
        budget: TeletextDisplayResolutionBudget,
    ) -> Result<Self, TeletextDisplayResolutionError> {
        let count = writer_count(input);
        if count > budget.max_writers {
            return Err(input.error(TeletextDisplayResolutionFailure::WriterBudget {
                count,
                maximum: budget.max_writers,
            }));
        }
        let width = input.geometry.total_width();
        let mut output = TeletextDisplayResolution {
            owner: input.base.owner(),
            width,
            cells: vec![None; 25 * usize::from(width)],
            states: Vec::new(),
            origins: Vec::with_capacity(input.layers.candidates().len()),
            writers: Vec::with_capacity(count),
            pending: Vec::new(),
        };
        let mut defaults = [0; 25];
        let mut panels = [0; 25];
        for row in 0..25u8 {
            let attrs = input
                .base_footprints
                .row(row)
                .ok_or_else(|| {
                    input.error(TeletextDisplayResolutionFailure::BaseFootprintMismatch)
                })?
                .baseline();
            let source = TeletextAttributeSource::Base {
                position: (u16::from(row), if row == 0 { 8 } else { 0 }),
                source: TeletextBaseAttributeSource::RowDefault {
                    row,
                    start_column: if row == 0 { 8 } else { 0 },
                },
            };
            defaults[usize::from(row)] = output.writers.len();
            output.writers.push(default_writer(
                source,
                attrs.values().foreground,
                attrs.values().background,
            ));
            panels[usize::from(row)] = output.writers.len();
            output.writers.push(default_writer(
                TeletextAttributeSource::PanelDefault { row },
                TeletextColourReference::WHITE,
                input.row_colours[usize::from(row)],
            ));
        }
        let event_writers = vec![None; input.original.events().len()];
        let mut result = Self {
            input,
            budget,
            output,
            defaults,
            panels,
            event_writers,
            base_writers: Vec::new(),
            updates: 0,
            shared: Vec::new(),
            private_states: vec![None; input.layers.candidates().len()],
            adaptive_states: vec![None; input.layers.candidates().len()],
        };
        result.build_writers();
        for (index, record) in input.geometry.records().iter().enumerate() {
            let raw = record.candidate().position();
            let default = if raw.1 < 40 {
                result.defaults[usize::from(raw.0.min(24))]
            } else {
                result.panels[usize::from(raw.0.min(24))]
            };
            let state = result.store(
                [Selection {
                    writer: default,
                    pending: None,
                }; FIELDS],
            );
            let visual = match record.placement() {
                TeletextCandidatePlacement::Placed { row, column } => Some((row, column)),
                TeletextCandidatePlacement::Unplaced(_) => None,
            };
            result.output.origins.push(TeletextDisplayOrigin {
                candidate: index,
                visual,
                requested: None,
                effective: None,
                attributes: state,
                pending: None,
                obscured_by: None,
            });
            if visual.is_none() {
                result.origin_pending(index, TeletextDisplayPendingReason::Unaddressable);
            }
        }
        Ok(result)
    }
    pub(super) fn finish(self) -> TeletextDisplayResolution {
        self.output
    }
    pub(super) fn admitted_updates(&self) -> usize {
        self.updates
    }
    fn store(&mut self, state: State) -> usize {
        let index = self.output.states.len();
        self.output.states.push(state);
        index
    }
    fn charge(&mut self) -> Result<(), TeletextDisplayResolutionError> {
        if self.updates >= self.budget.max_updates {
            return Err(self
                .input
                .error(TeletextDisplayResolutionFailure::UpdateBudget {
                    maximum: self.budget.max_updates,
                }));
        }
        self.updates += 1;
        Ok(())
    }
    fn build_writers(&mut self) {
        for row in 0..25u8 {
            if self
                .input
                .base_footprints
                .row(row)
                .is_some_and(|r| r.suppressed_by().is_some())
            {
                continue;
            }
            for column in if row == 0 { 8 } else { 0 }..40u8 {
                let cell = self.input.base.cell(row, column).copied();
                let Some(cell) = cell else { continue };
                let TeletextBaseDisplayCellKind::SpacingControl { control, timing } = cell.kind()
                else {
                    continue;
                };
                if control == TeletextBaseSpacingControl::StartBox
                    && self
                        .input
                        .base
                        .cell(row, column + 1)
                        .is_none_or(|next| next.code() != 0x0b)
                {
                    continue;
                }
                let start = column + u8::from(timing == TeletextSpacingTiming::SetAfter);
                let source = TeletextAttributeSource::Base {
                    position: (u16::from(row), u16::from(column)),
                    source: TeletextBaseAttributeSource::Spacing {
                        position: cell.position(),
                        control,
                        timing,
                    },
                };
                let values = spacing_values(self.input, cell, control);
                let writer = self.output.writers.len();
                self.output.writers.push(TeletextDisplayWriter {
                    source,
                    requested: (u16::from(row), u16::from(start)),
                    effective: (u16::from(row), u16::from(start)),
                    placed: (u16::from(row), u16::from(column)),
                    values,
                    explicit: true,
                });
                if let Some(at) = self.index((u16::from(row), u16::from(start))) {
                    self.base_writers.push(Action { at, writer });
                }
            }
        }
        for (ordinal, event) in self.input.original.events().iter().enumerate() {
            let TeletextEnhancementCommand::AtColumn { operation, .. } = event.command() else {
                continue;
            };
            let values = operation_values(operation);
            if values.iter().all(Option::is_none) {
                continue;
            }
            let position = position(event.absolute_after());
            let source = TeletextAttributeSource::Enhancement {
                event_ordinal: ordinal,
                location: event.source(),
                root_source: event.root_source(),
                invocation: event.invocation(),
                effective_start: position,
                placed_position: position,
            };
            let index = self.output.writers.len();
            self.event_writers[ordinal] = Some(index);
            self.output.writers.push(TeletextDisplayWriter {
                source,
                requested: position,
                effective: position,
                placed: position,
                values,
                explicit: true,
            });
        }
    }
    fn index(&self, raw: (u16, u16)) -> Option<usize> {
        visual(self.input.geometry, raw)
            .map(|(row, col)| usize::from(row) * usize::from(self.output.width) + usize::from(col))
    }
    fn raw(&self, at: usize) -> (u16, u16) {
        let width = usize::from(self.output.width);
        let row = at / width;
        let col = at % width;
        let left = usize::from(self.input.geometry.left_width());
        let raw = if col < left {
            72 - left + col
        } else if col < left + 40 {
            col - left
        } else {
            40 + col - left - 40
        };
        (
            u16::try_from(row).unwrap_or(25),
            u16::try_from(raw).unwrap_or(72),
        )
    }
    fn area_end(&self, at: usize) -> usize {
        let width = usize::from(self.output.width);
        let column = at % width;
        let left = usize::from(self.input.geometry.left_width());
        let end = if column < left {
            left
        } else if column < left + 40 {
            left + 40
        } else {
            width
        };
        at - column + end
    }
    fn baseline(&self, at: usize) -> State {
        let raw = self.raw(at);
        let writer = if raw.1 < 40 {
            self.defaults[usize::from(raw.0)]
        } else {
            self.panels[usize::from(raw.0)]
        };
        [Selection {
            writer,
            pending: None,
        }; FIELDS]
    }
    fn origin_pending(&mut self, index: usize, reason: TeletextDisplayPendingReason) -> usize {
        let candidate_index = self.output.origins[index].candidate;
        let candidate = &self.input.layers.candidates()[candidate_index];
        let pending = self.output.pending.len();
        self.output.pending.push(TeletextDisplayPending {
            candidate: Some(candidate_index),
            position: candidate.position(),
            location: candidate.source(),
            root_source: candidate.root_source(),
            invocation: candidate.invocation(),
            reason,
            first: None,
            second: None,
            previous: self.output.origins[index].pending,
            incoming_previous: None,
        });
        self.output.origins[index].pending = Some(pending);
        pending
    }
    fn merge(
        &mut self,
        state: &mut State,
        writer: usize,
        at: usize,
        private: bool,
    ) -> Result<(), TeletextDisplayResolutionError> {
        for (field, selection) in state.iter_mut().enumerate() {
            if self.output.writers[writer].values[field].is_none() {
                continue;
            }
            self.charge()?;
            let previous = *selection;
            let incoming = self.output.writers[writer];
            let current = self.output.writers[previous.writer];
            if private || incoming.key() > current.key() {
                *selection = Selection {
                    writer,
                    pending: None,
                };
            } else if incoming.key() == current.key()
                && writer != previous.writer
                && incoming.explicit
            {
                // Only final positions from this exact original execution can
                // settle C.2 r3. An older unknown branch cannot be discarded by
                // comparing merely the newest pair of contenders.
                if previous.pending.is_none() {
                    if let Some(order) = self
                        .input
                        .attributes
                        .transmission_evidence()
                        .and_then(|evidence| evidence.compare(incoming.source, current.source))
                    {
                        if order.is_gt() {
                            *selection = Selection {
                                writer,
                                pending: None,
                            };
                        }
                        continue;
                    }
                }
                let (location, root_source, invocation) = source_location(incoming.source);
                let index = self.output.pending.len();
                self.output.pending.push(TeletextDisplayPending {
                    candidate: None,
                    position: self.raw(at),
                    location,
                    root_source,
                    invocation,
                    reason: TeletextDisplayPendingReason::TransmissionOrder,
                    first: Some(previous.writer),
                    second: Some(writer),
                    previous: previous.pending,
                    incoming_previous: None,
                });
                *selection = Selection {
                    writer,
                    pending: Some(index),
                };
            }
        }
        Ok(())
    }
    fn actions(&self, instance: Option<TeletextSpatialInvocationId>, shared: bool) -> Vec<Action> {
        let mut result = if shared {
            self.base_writers.clone()
        } else {
            Vec::new()
        };
        for (ordinal, event) in self.input.original.events().iter().enumerate() {
            let matches = if shared {
                event.invocation().is_none_or(|id| {
                    self.input.original.invocations()[id.get()].object_type()
                        == TeletextObjectType::Active
                })
            } else {
                event.invocation() == instance
            };
            if matches {
                if let (Some(writer), Some(at)) = (
                    self.event_writers[ordinal],
                    self.index(position(event.absolute_after())),
                ) {
                    result.push(Action { at, writer });
                }
            }
        }
        result.sort_by_key(|action| (action.at, action.writer));
        result
    }
    fn selected_candidates(
        &self,
        instance: Option<TeletextSpatialInvocationId>,
        shared: bool,
    ) -> Vec<(usize, usize)> {
        let mut result = Vec::new();
        for (index, record) in self.input.geometry.records().iter().enumerate() {
            let candidate = record.candidate();
            let belongs = if shared {
                matches!(
                    candidate.priority(),
                    TeletextGlyphPriority::Base
                        | TeletextGlyphPriority::Active
                        | TeletextGlyphPriority::Local
                )
            } else {
                candidate.invocation() == instance
            };
            if belongs {
                if let TeletextCandidatePlacement::Placed { row, column } = record.placement() {
                    result.push((
                        usize::from(row) * usize::from(self.output.width) + usize::from(column),
                        index,
                    ));
                }
            }
        }
        result.sort_by_key(|&(at, index)| {
            (
                at,
                self.input.layers.candidates()[index].priority(),
                self.input.layers.candidates()[index]
                    .invocation()
                    .map(TeletextSpatialInvocationId::get),
                self.input.layers.candidates()[index].event_ordinal(),
            )
        });
        result
    }
    pub(super) fn shared(&mut self) -> Result<(), TeletextDisplayResolutionError> {
        let actions = self.actions(None, true);
        let candidates = self.selected_candidates(None, true);
        let mut coverage: Vec<Option<usize>> = vec![None; self.output.cells.len()];
        let mut action_index = 0;
        let mut candidate_index = 0;
        let mut deferred = Vec::new();
        let mut state = self.baseline(0);
        let mut current_end = 0;
        for at in 0..self.output.cells.len() {
            if self.index(self.raw(at)).is_none() {
                continue;
            }
            if at >= current_end {
                state = self.baseline(at);
                current_end = self.area_end(at);
                deferred.clear();
            }
            while action_index < actions.len() && actions[action_index].at <= at {
                let action = actions[action_index];
                action_index += 1;
                if action.at == at {
                    deferred.push(action.writer);
                }
            }
            let hidden = coverage[at].is_some();
            if !hidden {
                for writer in deferred.drain(..) {
                    self.output.writers[writer].effective = self.raw(at);
                    self.merge(&mut state, writer, at, false)?;
                }
            }
            let start = candidate_index;
            while candidate_index < candidates.len() && candidates[candidate_index].0 == at {
                candidate_index += 1;
            }
            let group = &candidates[start..candidate_index];
            for &(_, index) in group {
                let pool = self.store(state);
                self.output.origins[index].attributes = pool;
                self.output.origins[index].requested = self.size(state, index)?;
            }
            if hidden {
                for &(_, index) in group {
                    self.output.origins[index].obscured_by = coverage[at];
                }
                continue;
            }
            let raw = self.raw(at);
            let suppressed = raw.1 < 40
                && self
                    .input
                    .base_footprints
                    .row(u8::try_from(raw.0).unwrap_or(25))
                    .is_some_and(|row| row.suppressed_by().is_some());
            let chosen = group
                .iter()
                .rev()
                .find(|&&(_, index)| {
                    !suppressed
                        || self.input.layers.candidates()[index].priority()
                            != TeletextGlyphPriority::Base
                })
                .map(|&(_, index)| index);
            if let Some(index) = chosen {
                self.paint(index, at, state, &mut coverage)?;
            } else {
                let mut blank = state;
                if suppressed && at >= usize::from(self.output.width) {
                    if let Some(upper) = self.output.cells[at - usize::from(self.output.width)] {
                        blank[BG] = self.output.states[upper.attributes][BG];
                    }
                }
                let attributes = self.store(blank);
                self.output.cells[at] = Some(Cell {
                    origin: None,
                    attributes,
                    fragment: TeletextDisplayFragment::BackgroundOnly,
                });
            }
        }
        self.shared = self.output.cells.clone();
        Ok(())
    }
    pub(super) fn objects(
        &mut self,
        kind: TeletextObjectType,
    ) -> Result<(), TeletextDisplayResolutionError> {
        let instances: Vec<_> = self
            .input
            .original
            .invocations()
            .iter()
            .copied()
            .filter(|instance| instance.object_type() == kind)
            .collect();
        for instance in instances {
            self.object(instance, kind)?;
        }
        Ok(())
    }
    fn object(
        &mut self,
        instance: TeletextSpatialInvocation,
        kind: TeletextObjectType,
    ) -> Result<(), TeletextDisplayResolutionError> {
        let scans = self
            .input
            .original
            .events()
            .len()
            .checked_mul(80)
            .and_then(|work| work.checked_add(self.input.layers.candidates().len().checked_mul(4)?))
            .and_then(|work| work.checked_add(self.output.cells.len().checked_mul(80)?))
            .ok_or_else(|| {
                self.input
                    .error(TeletextDisplayResolutionFailure::AllocationSize)
            })?;
        self.charge_work(scans)?;
        let actions = self.actions(Some(instance.id()), false);
        let candidates = self.selected_candidates(Some(instance.id()), false);
        let mut execution = self.prepare_object(instance, kind)?;
        for at in 0..self.output.cells.len() {
            if self.index(self.raw(at)).is_none() {
                continue;
            }
            let range = self.advance_object(at, &actions, &candidates, &mut execution);
            let cell = ObjectCell {
                at,
                hidden: execution.coverage[at].is_some(),
                group: &candidates[range],
            };
            self.apply_object_writers(cell, &mut execution)?;
            self.paint_object_glyphs(cell, &mut execution)?;
            self.repaint_shared(cell, &mut execution)?;
            self.project_adaptive_tail(cell, &execution)?;
        }
        Ok(())
    }
    fn prepare_object(
        &mut self,
        instance: TeletextSpatialInvocation,
        kind: TeletextObjectType,
    ) -> Result<ObjectExecution, TeletextDisplayResolutionError> {
        let coverage: Vec<Option<usize>> = vec![None; self.output.cells.len()];
        let action_index = 0;
        let candidate_index = 0;
        let deferred = Vec::new();
        let private = kind == TeletextObjectType::Passive;
        let default = self.output.writers.len();
        if private {
            self.output.writers.push(default_writer(
                TeletextAttributeSource::PassiveDefault {
                    invocation: instance.id(),
                },
                TeletextColourReference::WHITE,
                TeletextColourReference::BLACK,
            ));
        }
        if private {
            self.capture_private(instance.id(), default)?;
        } else {
            self.capture_adaptive(instance.id())?;
        }
        let changed: [Option<Selection>; FIELDS] = [None; FIELDS];
        let state = [Selection {
            writer: default,
            pending: None,
        }; FIELDS];
        let current_end = 0;
        let extents = self.extents(instance.id());
        let tails = if private {
            vec![None; 75]
        } else {
            self.adaptive_tails(instance.id(), &extents)
        };
        Ok(ObjectExecution {
            private,
            state,
            changed,
            coverage,
            deferred,
            current_end,
            action_index,
            candidate_index,
            extents,
            tails,
        })
    }
    fn advance_object(
        &self,
        at: usize,
        actions: &[Action],
        candidates: &[(usize, usize)],
        execution: &mut ObjectExecution,
    ) -> std::ops::Range<usize> {
        if at >= execution.current_end {
            execution.current_end = self.area_end(at);
            if !execution.private {
                execution.changed = [None; FIELDS];
            }
            execution.deferred.clear();
        }
        let start = execution.candidate_index;
        while execution.candidate_index < candidates.len()
            && candidates[execution.candidate_index].0 == at
        {
            execution.candidate_index += 1;
        }

        while execution.action_index < actions.len() && actions[execution.action_index].at <= at {
            let action = actions[execution.action_index];
            execution.action_index += 1;
            if action.at == at {
                execution.deferred.push(action.writer);
            }
        }
        start..execution.candidate_index
    }
    fn apply_object_writers(
        &mut self,
        cell: ObjectCell<'_>,
        execution: &mut ObjectExecution,
    ) -> Result<(), TeletextDisplayResolutionError> {
        let lower_hidden = execution.coverage[cell.at].is_some_and(|origin| {
            self.input.layers.candidates()[self.output.origins[origin].candidate].priority()
                < TeletextGlyphPriority::Adaptive
        });
        let opens_lower = !execution.private
            && lower_hidden
            && cell.group.iter().any(|&(_, index)| {
                self.adaptive_states[index].is_some_and(|fields| {
                    fields[DH].is_some_and(|s| s.pending.is_none())
                        && fields[DW].is_some_and(|s| s.pending.is_none())
                })
            });
        if !cell.hidden || opens_lower {
            if !execution.private {
                execution.state = self.shared[cell.at].map_or_else(
                    || self.baseline(cell.at),
                    |cell| self.output.states[cell.attributes],
                );
                for (field, value) in execution.changed.iter().enumerate() {
                    if let Some(value) = value {
                        execution.state[field] = *value;
                    }
                }
            }
            for writer in execution.deferred.drain(..) {
                self.output.writers[writer].effective = self.raw(cell.at);
                if !execution.private {
                    for (field, value) in self.output.writers[writer].values.iter().enumerate() {
                        if value.is_some() && execution.changed[field].is_none() {
                            execution.state[field] = Selection {
                                writer,
                                pending: None,
                            };
                        }
                    }
                }
                self.merge(&mut execution.state, writer, cell.at, execution.private)?;
                if !execution.private {
                    for (field, value) in self.output.writers[writer].values.iter().enumerate() {
                        if value.is_some() {
                            execution.changed[field] = Some(execution.state[field]);
                        }
                    }
                }
            }
        }
        Ok(())
    }
    fn paint_object_glyphs(
        &mut self,
        cell: ObjectCell<'_>,
        execution: &mut ObjectExecution,
    ) -> Result<(), TeletextDisplayResolutionError> {
        for &(_, index) in cell.group {
            let mut candidate_state = if execution.private {
                self.private_states[index].unwrap_or(execution.state)
            } else {
                self.shared[cell.at].map_or_else(
                    || self.baseline(cell.at),
                    |cell| self.output.states[cell.attributes],
                )
            };
            let own_override = self.adaptive_states[index];
            let explicit_display = own_override.is_some_and(|fields| {
                fields[DH].is_some_and(|s| s.pending.is_none())
                    && fields[DW].is_some_and(|s| s.pending.is_none())
            });
            let lower_hidden = !execution.private
                && execution.coverage[cell.at].is_some_and(|origin| {
                    self.input.layers.candidates()[self.output.origins[origin].candidate].priority()
                        < TeletextGlyphPriority::Adaptive
                });
            if let Some(fields) = own_override {
                for (field, value) in fields.into_iter().enumerate() {
                    if let Some(value) = value {
                        candidate_state[field] = value;
                    }
                }
            }
            let pool = self.store(candidate_state);
            self.output.origins[index].attributes = pool;
            self.output.origins[index].requested = self.size(candidate_state, index)?;
            if cell.hidden && !(lower_hidden && explicit_display) {
                if !execution.private
                    && execution.coverage[cell.at].is_some_and(|origin| {
                        self.input.layers.candidates()[self.output.origins[origin].candidate]
                            .priority()
                            < TeletextGlyphPriority::Adaptive
                    })
                {
                    self.origin_pending(
                        index,
                        TeletextDisplayPendingReason::InheritedAdaptiveNonOrigin,
                    );
                } else {
                    self.output.origins[index].obscured_by = execution.coverage[cell.at];
                }
                continue;
            }
            if !execution.private
                && !explicit_display
                && execution.changed[DH].is_none()
                && execution.changed[DW].is_none()
                && self.inherited_non_origin(cell.at)
            {
                self.origin_pending(
                    index,
                    TeletextDisplayPendingReason::InheritedAdaptiveNonOrigin,
                );
                continue;
            }
            self.paint(index, cell.at, candidate_state, &mut execution.coverage)?;
        }
        Ok(())
    }
    fn inherited_non_origin(&self, at: usize) -> bool {
        let enlarged = self.shared[at].is_some_and(|cell| {
            cell.fragment != TeletextDisplayFragment::Normal
                && cell.fragment != TeletextDisplayFragment::BackgroundOnly
        });
        let origin = self.shared[at].and_then(|cell| cell.origin);
        let is_origin = origin.is_some_and(|index| {
            self.output.origins[index].visual
                == Some((
                    u8::try_from(at / usize::from(self.output.width)).unwrap_or(25),
                    u8::try_from(at % usize::from(self.output.width)).unwrap_or(72),
                ))
        });
        enlarged && !is_origin
    }
    fn repaint_shared(
        &mut self,
        cell: ObjectCell<'_>,
        execution: &mut ObjectExecution,
    ) -> Result<(), TeletextDisplayResolutionError> {
        if !execution.private && !cell.hidden && cell.group.is_empty() {
            let in_extent = execution
                .extents
                .iter()
                .any(|&(first, last)| cell.at >= first && cell.at <= last);
            if let Some(lower) = self.shared[cell.at]
                .filter(|cell| cell.fragment != TeletextDisplayFragment::BackgroundOnly)
            {
                if let Some(origin) = lower.origin.filter(|&origin| {
                    self.output.origins[origin].visual
                        == Some((
                            u8::try_from(cell.at / usize::from(self.output.width)).unwrap_or(25),
                            u8::try_from(cell.at % usize::from(self.output.width)).unwrap_or(72),
                        ))
                }) {
                    let visible_shared = self.output.cells[cell.at]
                        .and_then(|cell| cell.origin)
                        .is_none_or(|current| {
                            self.input.layers.candidates()[self.output.origins[current].candidate]
                                .priority()
                                < TeletextGlyphPriority::Adaptive
                        });
                    if in_extent && execution.changed.iter().any(Option::is_some) && visible_shared
                    {
                        let repaint = self.repaint_origin(origin, execution.state)?;
                        self.paint(repaint, cell.at, execution.state, &mut execution.coverage)?;
                    } else {
                        self.seed_coverage(origin, cell.at, &mut execution.coverage)?;
                    }
                }
            }
        }
        Ok(())
    }
    fn project_adaptive_tail(
        &mut self,
        cell: ObjectCell<'_>,
        execution: &ObjectExecution,
    ) -> Result<(), TeletextDisplayResolutionError> {
        if !execution.private
            && !cell.hidden
            && cell.group.is_empty()
            && execution.changed.iter().any(Option::is_some)
        {
            if let Some(event) = execution.tails[area_index(self.raw(cell.at))] {
                if let Some(lower) = self.output.cells[cell.at].filter(|cell| {
                    cell.fragment != TeletextDisplayFragment::BackgroundOnly
                        && cell.origin.is_none_or(|origin| {
                            self.input.layers.candidates()[self.output.origins[origin].candidate]
                                .priority()
                                < TeletextGlyphPriority::Adaptive
                        })
                }) {
                    let lower_state = self.output.states[lower.attributes];
                    let pending =
                        self.pending_tail(lower_state, execution.changed, event, cell.at)?;
                    let attributes = self.store(pending);
                    self.output.cells[cell.at] = Some(Cell {
                        attributes,
                        ..lower
                    });
                }
            }
        }
        Ok(())
    }
    fn size(
        &mut self,
        state: State,
        origin: usize,
    ) -> Result<Option<TeletextDisplaySize>, TeletextDisplayResolutionError> {
        let Some(width) = self.size_flag(state[DW], DW, origin)? else {
            return Ok(None);
        };
        let Some(height) = self.size_flag(state[DH], DH, origin)? else {
            return Ok(None);
        };
        Ok(Some(match (width, height) {
            (false, false) => TeletextDisplaySize::Normal,
            (true, false) => TeletextDisplaySize::DoubleWidth,
            (false, true) => TeletextDisplaySize::DoubleHeight,
            (true, true) => TeletextDisplaySize::DoubleSize,
        }))
    }
    fn size_flag(
        &mut self,
        selection: Selection,
        field: usize,
        origin: usize,
    ) -> Result<Option<bool>, TeletextDisplayResolutionError> {
        let Some(value) = writer_flag(&self.output, selection.writer, field) else {
            return Ok(None);
        };
        let Some(root) = selection.pending else {
            return Ok(Some(value));
        };
        let candidate = self.output.origins[origin].candidate;
        let count = self.output.pending.len();
        if root >= count {
            return Ok(None);
        }
        let bytes = count.checked_mul(std::mem::size_of::<bool>() + std::mem::size_of::<usize>());
        if bytes.is_none_or(|bytes| bytes > isize::MAX as usize) {
            return Err(self
                .input
                .candidate_error(candidate, TeletextDisplayResolutionFailure::AllocationSize));
        }
        // A winner can remain unknown even when every contender has the same
        // size flag. Inspect both retained chains without changing provenance.
        // Charge scratch initialization before allocation; visit shared DAG
        // predecessors once, with each inspected node charged as update work.
        self.charge_work(count)
            .map_err(|error| self.input.candidate_error(candidate, error.reason))?;
        let mut visited = vec![false; count];
        let mut pending = Vec::with_capacity(count);
        visited[root] = true;
        pending.push(root);
        while let Some(index) = pending.pop() {
            self.charge()
                .map_err(|error| self.input.candidate_error(candidate, error.reason))?;
            let node = self.output.pending[index];
            for writer in [node.first, node.second] {
                if writer.and_then(|writer| writer_flag(&self.output, writer, field)) != Some(value)
                {
                    return Ok(None);
                }
            }
            for predecessor in [node.previous, node.incoming_previous]
                .into_iter()
                .flatten()
            {
                // Constructed diagnostics refer only to earlier retained nodes.
                // Missing or inconsistent evidence cannot establish consensus.
                if predecessor >= index {
                    return Ok(None);
                }
                if !visited[predecessor] {
                    visited[predecessor] = true;
                    pending.push(predecessor);
                }
            }
        }
        Ok(Some(value))
    }
    fn charge_work(&mut self, count: usize) -> Result<(), TeletextDisplayResolutionError> {
        let Some(total) = self.updates.checked_add(count) else {
            return Err(self
                .input
                .error(TeletextDisplayResolutionFailure::AllocationSize));
        };
        if total > self.budget.max_updates {
            return Err(self
                .input
                .error(TeletextDisplayResolutionFailure::UpdateBudget {
                    maximum: self.budget.max_updates,
                }));
        }
        self.updates = total;
        Ok(())
    }
    fn capture_private(
        &mut self,
        id: TeletextSpatialInvocationId,
        default: usize,
    ) -> Result<(), TeletextDisplayResolutionError> {
        let mut state = [Selection {
            writer: default,
            pending: None,
        }; FIELDS];
        let mut glyphs: Vec<_> = self
            .input
            .layers
            .candidates()
            .iter()
            .enumerate()
            .filter(|(_, candidate)| candidate.invocation() == Some(id))
            .filter_map(|(index, candidate)| {
                candidate.event_ordinal().map(|ordinal| (ordinal, index))
            })
            .collect();
        glyphs.sort_unstable();
        let mut next = 0;
        for (ordinal, event) in self.input.original.events().iter().enumerate() {
            if event.invocation() != Some(id) {
                continue;
            }
            if let Some(writer) = self.event_writers[ordinal] {
                self.merge(&mut state, writer, 0, true)?;
            }
            if let Some(&(event_index, index)) = glyphs.get(next) {
                if event_index == ordinal {
                    let attributes = self.store(state);
                    self.output.origins[index].attributes = attributes;
                    self.output.origins[index].requested = self.size(state, index)?;
                    self.private_states[index] = Some(state);
                    next += 1;
                }
            }
        }
        Ok(())
    }
    fn capture_adaptive(
        &mut self,
        id: TeletextSpatialInvocationId,
    ) -> Result<(), TeletextDisplayResolutionError> {
        let mut state = [Selection {
            writer: self.defaults[0],
            pending: None,
        }; FIELDS];
        let mut changed = [None; FIELDS];
        let mut current_area = None;
        let mut glyphs: Vec<_> = self
            .input
            .layers
            .candidates()
            .iter()
            .enumerate()
            .filter(|(_, candidate)| candidate.invocation() == Some(id))
            .filter_map(|(index, candidate)| {
                candidate.event_ordinal().map(|ordinal| (ordinal, index))
            })
            .collect();
        glyphs.sort_unstable();
        let mut next = 0;
        for (ordinal, event) in self.input.original.events().iter().enumerate() {
            if event.invocation() != Some(id) {
                continue;
            }
            let raw = position(event.absolute_after());
            let area = (
                raw.0,
                if raw.1 < 40 {
                    0
                } else if raw.1 < 56 {
                    1
                } else {
                    2
                },
            );
            if current_area != Some(area) {
                current_area = Some(area);
                changed = [None; FIELDS];
                state = [Selection {
                    writer: self.defaults[usize::from(raw.0.min(24))],
                    pending: None,
                }; FIELDS];
            }
            if let Some(writer) = self.event_writers[ordinal] {
                for (field, value) in self.output.writers[writer].values.iter().enumerate() {
                    if value.is_some() && changed[field].is_none() {
                        state[field] = Selection {
                            writer,
                            pending: None,
                        };
                    }
                }
                self.merge(&mut state, writer, self.index(raw).unwrap_or(0), false)?;
                for (field, value) in self.output.writers[writer].values.iter().enumerate() {
                    if value.is_some() {
                        changed[field] = Some(state[field]);
                    }
                }
            }
            if let Some(&(event_index, index)) = glyphs.get(next) {
                if event_index == ordinal {
                    self.adaptive_states[index] = Some(changed);
                    next += 1;
                }
            }
        }
        Ok(())
    }
    fn adaptive_tails(
        &self,
        id: TeletextSpatialInvocationId,
        extents: &[(usize, usize)],
    ) -> Vec<Option<TeletextSpatialEvent>> {
        let mut result = vec![None; 75];
        for event in self
            .input
            .original
            .events()
            .iter()
            .copied()
            .filter(|event| event.invocation() == Some(id))
        {
            if !matches!(
                event.command(),
                TeletextEnhancementCommand::SetPosition { .. }
                    | TeletextEnhancementCommand::InvokeObject { .. }
            ) {
                continue;
            }
            if let Some(endpoint) = self.index(position(event.absolute_after())) {
                let firm = extents
                    .iter()
                    .find(|&&(first, _)| self.area_end(first) == self.area_end(endpoint))
                    .map(|&(_, last)| last);
                if firm.is_none_or(|last| endpoint > last) {
                    result[area_index(self.raw(endpoint))] = Some(event);
                }
            }
        }
        result
    }
    fn pending_tail(
        &mut self,
        state: State,
        changed: [Option<Selection>; FIELDS],
        event: TeletextSpatialEvent,
        at: usize,
    ) -> Result<State, TeletextDisplayResolutionError> {
        let mut result = state;
        for (field, selected) in changed.into_iter().enumerate() {
            if let Some(writer) = selected {
                self.charge()?;
                let index = self.output.pending.len();
                self.output.pending.push(TeletextDisplayPending {
                    candidate: None,
                    position: self.raw(at),
                    location: Some(event.source()),
                    root_source: Some(event.root_source()),
                    invocation: event.invocation(),
                    reason: TeletextDisplayPendingReason::AdaptiveExtent,
                    first: Some(writer.writer),
                    second: Some(state[field].writer),
                    previous: writer.pending,
                    incoming_previous: state[field].pending,
                });
                result[field] = Selection {
                    writer: writer.writer,
                    pending: Some(index),
                };
            }
        }
        Ok(result)
    }
    fn extents(&self, id: TeletextSpatialInvocationId) -> Vec<(usize, usize)> {
        let mut points: Vec<_> = self
            .input
            .original
            .events()
            .iter()
            .filter(|event| {
                event.invocation() == Some(id)
                    && matches!(event.command(), TeletextEnhancementCommand::AtColumn { .. })
            })
            .filter_map(|event| self.index(position(event.absolute_after())))
            .collect();
        points.sort_unstable();
        let mut result: Vec<(usize, usize)> = Vec::new();
        for point in points {
            if let Some(last) = result.last_mut() {
                if self.area_end(last.0) == self.area_end(point) {
                    last.1 = point;
                    continue;
                }
            }
            result.push((point, point));
        }
        result
    }
    fn repaint_origin(
        &mut self,
        original: usize,
        state: State,
    ) -> Result<usize, TeletextDisplayResolutionError> {
        let count = self.output.origins.len().checked_add(1).ok_or_else(|| {
            self.input
                .error(TeletextDisplayResolutionFailure::AllocationSize)
        })?;
        if count > self.budget.max_origins {
            return Err(self.input.candidate_error(
                self.output.origins[original].candidate,
                TeletextDisplayResolutionFailure::OriginBudget {
                    count,
                    maximum: self.budget.max_origins,
                },
            ));
        }
        let index = self.output.origins.len();
        let attributes = self.store(state);
        self.output.origins.push(TeletextDisplayOrigin {
            attributes,
            requested: None,
            effective: None,
            pending: None,
            obscured_by: None,
            ..self.output.origins[original]
        });
        Ok(index)
    }
    fn seed_coverage(
        &mut self,
        origin: usize,
        at: usize,
        coverage: &mut [Option<usize>],
    ) -> Result<(), TeletextDisplayResolutionError> {
        if let Some(size) = self.output.origins[origin].effective {
            let (width, height) = size.dimensions();
            let stride = usize::from(self.output.width);
            for dy in 0..height {
                for dx in 0..width {
                    let part = at + dy * stride + dx;
                    if part < coverage.len() {
                        self.charge()?;
                        coverage[part] = Some(origin);
                    }
                }
            }
        }
        Ok(())
    }
    fn paint(
        &mut self,
        index: usize,
        at: usize,
        state: State,
        coverage: &mut [Option<usize>],
    ) -> Result<(), TeletextDisplayResolutionError> {
        let pool = self.store(state);
        self.output.origins[index].attributes = pool;
        let requested = self.size(state, index)?;
        self.output.origins[index].requested = requested;
        let Some(requested) = requested else {
            self.origin_pending(index, TeletextDisplayPendingReason::UnknownSize);
            return Ok(());
        };
        let raw = self.raw(at);
        let (mut width, mut height) = requested.dimensions();
        let row = at / usize::from(self.output.width);
        if (width == 2 && at + 1 >= self.area_end(at)) || (height == 2 && (row == 0 || row >= 23)) {
            self.origin_pending(index, TeletextDisplayPendingReason::IllegalSizeBoundary);
            return Ok(());
        }
        let stride = usize::from(self.output.width);
        let priority =
            self.input.layers.candidates()[self.output.origins[index].candidate].priority();
        let blocks = |origin: Option<usize>| {
            origin.is_some_and(|other| {
                self.input.layers.candidates()[self.output.origins[other].candidate].priority()
                    >= priority
            })
        };
        let blocked_width = width == 2 && blocks(coverage[at + 1]);
        let blocked_height = height == 2 && blocks(coverage[at + stride]);
        if blocked_width {
            width = 1;
        }
        if blocked_height {
            height = 1;
        }
        if width == 2 && height == 2 && blocks(coverage[at + stride + 1]) {
            self.origin_pending(index, TeletextDisplayPendingReason::ConflictingSizeAxes);
            return Ok(());
        }
        let effective = match (width, height) {
            (2, 2) => TeletextDisplaySize::DoubleSize,
            (2, _) => TeletextDisplaySize::DoubleWidth,
            (_, 2) => TeletextDisplaySize::DoubleHeight,
            _ => TeletextDisplaySize::Normal,
        };
        self.output.origins[index].effective = Some(effective);
        if self.input.intent.records()[self.output.origins[index].candidate].disposition()
            != TeletextGlyphDisposition::Assigned
        {
            self.origin_pending(index, TeletextDisplayPendingReason::GlyphAssignment);
        }
        if raw.1 < 40
            && self
                .input
                .base_footprints
                .cell(
                    u8::try_from(raw.0).unwrap_or(25),
                    u8::try_from(raw.1).unwrap_or(40),
                )
                .is_some_and(|cell| cell.pending().is_some())
        {
            self.origin_pending(index, TeletextDisplayPendingReason::BaseHeightBoundary);
        }
        for dy in 0..height {
            for dx in 0..width {
                self.charge()?;
                let part = at + dy * stride + dx;
                coverage[part] = Some(index);
                let inherited_repaint = index >= self.input.layers.candidates().len();
                if inherited_repaint
                    && self.output.cells[part]
                        .and_then(|cell| cell.origin)
                        .is_some_and(|origin| {
                            self.input.layers.candidates()[self.output.origins[origin].candidate]
                                .priority()
                                >= TeletextGlyphPriority::Adaptive
                        })
                {
                    continue;
                }
                self.output.cells[part] = Some(Cell {
                    origin: Some(index),
                    attributes: pool,
                    fragment: fragment(effective, dx, dy),
                });
            }
        }
        Ok(())
    }
}

fn position(position: TeletextSpatialPosition) -> (u16, u16) {
    (position.row(), position.column())
}
pub(super) fn visual(
    geometry: &TeletextDisplayGeometry,
    (row, col): (u16, u16),
) -> Option<(u8, u8)> {
    if row > 24 {
        return None;
    }
    let left = u16::from(geometry.left_width());
    let right = u16::from(geometry.right_width());
    let visual = if col < 40 {
        if row == 0 && col < 8 {
            return None;
        }
        left + col
    } else if row != 0 && col >= 72 - left && col < 72 {
        col - (72 - left)
    } else if row != 0 && col >= 40 && col < 40 + right {
        left + col
    } else {
        return None;
    };
    Some((u8::try_from(row).ok()?, u8::try_from(visual).ok()?))
}
fn default_writer(
    source: TeletextAttributeSource,
    fg: TeletextColourReference,
    bg: TeletextColourReference,
) -> TeletextDisplayWriter {
    let mut values = [Some(Value::Flag(false)); FIELDS];
    values[FG] = Some(Value::Colour(fg));
    values[BG] = Some(Value::Colour(bg));
    values[FLASH] = Some(Value::Flash(TeletextExecutedFlash {
        mode: TeletextFlashMode::Steady,
        rate: TeletextFlashRate::Slow1Hz,
    }));
    TeletextDisplayWriter {
        source,
        requested: (0, 0),
        effective: (0, 0),
        placed: (0, 0),
        values,
        explicit: false,
    }
}
fn operation_values(operation: TeletextColumnOperation) -> [Option<Value>; FIELDS] {
    let mut values = [None; FIELDS];
    match operation {
        TeletextColumnOperation::Foreground { colour } => values[FG] = Some(Value::Colour(colour)),
        TeletextColumnOperation::Background { colour } => values[BG] = Some(Value::Colour(colour)),
        TeletextColumnOperation::Flash { mode, rate } => {
            values[FLASH] = Some(Value::Flash(TeletextExecutedFlash { mode, rate }));
        }
        TeletextColumnOperation::Attributes {
            double_height,
            double_width,
            boxed,
            conceal,
            invert,
            underline_or_separated_mosaics,
        } => {
            for (field, value) in [
                (DH, double_height),
                (DW, double_width),
                (BOXED, boxed),
                (CONCEAL, conceal),
                (INVERT, invert),
                (D5, underline_or_separated_mosaics),
            ] {
                values[field] = Some(Value::Flag(value));
            }
            // Table 29 D5=0 also cancels an earlier Level 1 Separated Mosaics
            // attribute. Retain this explicit writer's C.2/C.3 scope and source;
            // a row/default D5=0 is not a cancellation command. Later spacing
            // separation may supersede this write, while D5=1 remains separate.
            if !underline_or_separated_mosaics {
                values[SPACING] = Some(Value::Flag(false));
            }
        }
        _ => {}
    }
    values
}
fn spacing_values(
    input: TeletextDisplayResolutionInputs<'_>,
    cell: TeletextBaseDisplayCell,
    control: TeletextBaseSpacingControl,
) -> [Option<Value>; FIELDS] {
    let mut values = [None; FIELDS];
    let row = cell.position().row();
    match control {
        // ESC changes Base G0 only, not any presentation attribute writer.
        TeletextBaseSpacingControl::Escape => {}
        TeletextBaseSpacingControl::AlphaColour { .. }
        | TeletextBaseSpacingControl::MosaicColour { .. } => {
            let clut = input
                .base
                .presentation()
                .map_or(0, |p| p.remapping.cluts().0);
            values[FG] = TeletextColourReference::new(clut * 8 + (control.code() & 7))
                .ok()
                .map(Value::Colour);
            values[CONCEAL] = Some(Value::Flag(false));
        }
        TeletextBaseSpacingControl::Flash | TeletextBaseSpacingControl::Steady => {
            values[FLASH] = Some(Value::Flash(TeletextExecutedFlash {
                mode: if control == TeletextBaseSpacingControl::Flash {
                    TeletextFlashMode::Normal
                } else {
                    TeletextFlashMode::Steady
                },
                rate: TeletextFlashRate::Slow1Hz,
            }));
        }
        TeletextBaseSpacingControl::StartBox | TeletextBaseSpacingControl::EndBox => {
            values[BOXED] = Some(Value::Flag(control == TeletextBaseSpacingControl::StartBox));
        }
        TeletextBaseSpacingControl::NormalHeight | TeletextBaseSpacingControl::DoubleHeight => {
            values[DH] = Some(Value::Flag(
                control == TeletextBaseSpacingControl::DoubleHeight,
            ));
            values[DW] = Some(Value::Flag(false));
        }
        TeletextBaseSpacingControl::Conceal => values[CONCEAL] = Some(Value::Flag(true)),
        TeletextBaseSpacingControl::ContiguousMosaics
        | TeletextBaseSpacingControl::SeparatedMosaics => {
            values[SPACING] = Some(Value::Flag(
                control == TeletextBaseSpacingControl::SeparatedMosaics,
            ));
        }
        TeletextBaseSpacingControl::BlackBackground => {
            values[BG] = input
                .base_footprints
                .row(row)
                .map(|row| Value::Colour(row.baseline().background()));
        }
        TeletextBaseSpacingControl::NewBackground => {
            values[BG] = Some(Value::Colour(cell.attributes().background()));
        }
    }
    values
}
pub(super) fn writer_count(input: TeletextDisplayResolutionInputs<'_>) -> usize {
    let controls=input.base.cells().iter().enumerate().filter(|(row,_)|input.base_footprints.row(u8::try_from(*row).unwrap_or(25)).is_none_or(|r|r.suppressed_by().is_none())).flat_map(|(_,row)|row).filter(|cell|matches!(cell.kind(),TeletextBaseDisplayCellKind::SpacingControl{control,..}if control!=TeletextBaseSpacingControl::StartBox||input.base.cell(cell.position().row(),cell.position().column()+1).is_some_and(|next|next.code()==0x0b))).count();
    let commands=input.original.events().iter().filter(|event|matches!(event.command(),TeletextEnhancementCommand::AtColumn{operation,..}if operation_values(operation).iter().any(Option::is_some))).count();
    50 + controls
        + commands
        + input
            .original
            .invocations()
            .iter()
            .filter(|instance| instance.object_type() == TeletextObjectType::Passive)
            .count()
}
fn source_location(
    source: TeletextAttributeSource,
) -> (
    Option<TeletextEnhancementSource>,
    Option<TeletextSpatialRootSource>,
    Option<TeletextSpatialInvocationId>,
) {
    if let TeletextAttributeSource::Enhancement {
        location,
        root_source,
        invocation,
        ..
    } = source
    {
        (Some(location), Some(root_source), invocation)
    } else {
        (None, None, None)
    }
}
fn writer_flag(output: &TeletextDisplayResolution, writer: usize, field: usize) -> Option<bool> {
    match output
        .writers
        .get(writer)?
        .values
        .get(field)
        .copied()
        .flatten()?
    {
        Value::Flag(value) => Some(value),
        _ => None,
    }
}
fn fragment(size: TeletextDisplaySize, x: usize, y: usize) -> TeletextDisplayFragment {
    match (size, x, y) {
        (TeletextDisplaySize::Normal, _, _) => TeletextDisplayFragment::Normal,
        (TeletextDisplaySize::DoubleWidth, 0, _) => TeletextDisplayFragment::DoubleWidthLeft,
        (TeletextDisplaySize::DoubleWidth, _, _) => TeletextDisplayFragment::DoubleWidthRight,
        (TeletextDisplaySize::DoubleHeight, _, 0) => TeletextDisplayFragment::DoubleHeightTop,
        (TeletextDisplaySize::DoubleHeight, _, _) => TeletextDisplayFragment::DoubleHeightBottom,
        (TeletextDisplaySize::DoubleSize, 0, 0) => TeletextDisplayFragment::DoubleSizeTopLeft,
        (TeletextDisplaySize::DoubleSize, _, 0) => TeletextDisplayFragment::DoubleSizeTopRight,
        (TeletextDisplaySize::DoubleSize, 0, _) => TeletextDisplayFragment::DoubleSizeBottomLeft,
        _ => TeletextDisplayFragment::DoubleSizeBottomRight,
    }
}

pub(super) fn validate_objects(
    input: TeletextDisplayResolutionInputs<'_>,
) -> Result<(), TeletextDisplayResolutionError> {
    // A.7 membership follows the original A.4 AP-addressing facts, including
    // charset/setup cells and inactive panels. Never use occupied-size
    // rectangles, visual placement, implicit initial AP or child addresses.
    // Attribute masks remain whole-instance types, with compound Display as
    // ONE type. Unresolved wide facts remain in the geometry's pending regions.
    let mut facts = Vec::with_capacity(input.original.events().len());
    let mut masks = vec![0u8; input.original.invocations().len()];
    for (ordinal, event) in input.original.events().iter().enumerate() {
        let Some(id) = event.invocation() else {
            continue;
        };
        if let TeletextEnhancementCommand::AtColumn { operation, .. } = event.command() {
            masks[id.get()] |= match operation {
                TeletextColumnOperation::Foreground { .. } => 1,
                TeletextColumnOperation::Background { .. } => 2,
                TeletextColumnOperation::Flash { .. } => 4,
                TeletextColumnOperation::Attributes { .. } => 8,
                _ => 0,
            };
        }
        if let Some(position) = super::display_geometry::known_address_position(*event) {
            facts.push((position, id.get(), ordinal));
        }
    }
    facts.sort_unstable();
    facts.dedup_by_key(|fact| (fact.0, fact.1));
    validate_adaptive_interleaving(input, &facts, &masks)?;
    let mut start = 0;
    while start < facts.len() {
        let mut finish = start + 1;
        while finish < facts.len() && facts[finish].0 == facts[start].0 {
            finish += 1;
        }
        let mut adaptive = None;
        let mut groups = [None; 4];
        for &(_, instance, ordinal) in &facts[start..finish] {
            let current = input.original.invocations()[instance];
            let conflicting = match current.object_type() {
                TeletextObjectType::Adaptive => {
                    let previous = adaptive;
                    adaptive = Some(current.id());
                    previous
                }
                TeletextObjectType::Active => {
                    let mut previous = None;
                    for (bit, writer) in groups.iter_mut().enumerate() {
                        if masks[instance] & (1 << bit) != 0 {
                            if previous.is_none() {
                                previous = *writer;
                            }
                            *writer = Some(current.id());
                        }
                    }
                    previous
                }
                TeletextObjectType::Passive => None,
            };
            if let Some(first) = conflicting {
                return Err(event_error(
                    input,
                    &input.original.events()[ordinal],
                    TeletextDisplayResolutionFailure::ObjectOverlap {
                        first,
                        second: current.id(),
                    },
                ));
            }
        }
        start = finish;
    }
    Ok(())
}

fn validate_adaptive_interleaving(
    input: TeletextDisplayResolutionInputs<'_>,
    facts: &[((u16, u16), usize, usize)],
    masks: &[u8],
) -> Result<(), TeletextDisplayResolutionError> {
    let mut last = vec![None; input.original.invocations().len()];
    let mut row = None;
    let mut counter = 0usize;
    let mut changed = 0usize;
    for (index, &(position, instance, ordinal)) in facts.iter().enumerate() {
        if row != Some(position.0) {
            row = Some(position.0);
            last.fill(None);
            counter = 0;
            changed = 0;
        }
        if input.original.invocations()[instance].object_type() != TeletextObjectType::Adaptive {
            continue;
        }
        if let Some((previous_counter, previous_changed, previous_index)) = last[instance] {
            if counter > previous_counter && (masks[instance] != 0 || changed > previous_changed) {
                let witness = facts[previous_index + 1..index].iter().find(|fact| {
                    input.original.invocations()[fact.1].object_type()
                        == TeletextObjectType::Adaptive
                        && fact.1 != instance
                        && (masks[instance] != 0 || masks[fact.1] != 0)
                });
                if let Some(&(_, other, other_ordinal)) = witness {
                    let (first, second, blame) = if instance < other {
                        (instance, other, other_ordinal)
                    } else {
                        (other, instance, ordinal)
                    };
                    return Err(event_error(
                        input,
                        &input.original.events()[blame],
                        TeletextDisplayResolutionFailure::AdaptiveInterleaving {
                            first: input.original.invocations()[first].id(),
                            second: input.original.invocations()[second].id(),
                        },
                    ));
                }
            }
        }
        counter += 1;
        if masks[instance] != 0 {
            changed += 1;
        }
        last[instance] = Some((counter, changed, index));
    }
    Ok(())
}

fn area_index((row, column): (u16, u16)) -> usize {
    usize::from(row) * 3
        + if column < 40 {
            0
        } else if column < 56 {
            1
        } else {
            2
        }
}
