//! Bounded overlap planning, not physical lowering or receiver/readiness authority.
//!
//! Final effective rectangles and final fragment owners are retained. Historical
//! paint/skip chronology is not available and is never inferred from origin order.

use super::{TeletextBaseCellCommandKind, TeletextMainComposition, TeletextMainCompositionSource};
use ersatzrs_domain::{
    TeletextDisplayFragment, TeletextDisplayOrigin, TeletextDisplayPending, TeletextDisplaySize,
    TeletextEnhancementSource, TeletextGlyphCandidate, TeletextGlyphPriority, TeletextPageAddress,
    TeletextSpatialInvocationId, TeletextSpatialRootSource,
};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextBaseCompatibilityRealizationBudget {
    pub max_commands: usize,
    pub max_candidates: usize,
    pub max_origins: usize,
    pub max_seeds: usize,
    pub max_edges: usize,
    pub max_updates: usize,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextBaseCompatibilitySeed {
    pub base_position: (u8, u8),
    pub visual_position: (u8, u8),
    pub composed_command: usize,
    pub origin: usize,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextBaseCompatibilityRectangle {
    pub row: u8,
    pub column: u8,
    pub height: u8,
    pub width: u8,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextBaseCompatibilityOrigin {
    pub origin_index: usize,
    pub original: TeletextDisplayOrigin,
    pub candidate: TeletextGlyphCandidate,
    pub rectangle: Option<TeletextBaseCompatibilityRectangle>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextBaseCompatibilityFragment {
    pub visual_position: (u8, u8),
    pub fragment: TeletextDisplayFragment,
    pub origin: Option<usize>,
}
/// Whole-origin replay would need `before` below `after` at this final cell.
/// This necessary constraint is not a sufficient proof of replay correctness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextBaseCompatibilityPrecedence {
    pub before: usize,
    pub after: usize,
    pub visual_position: (u8, u8),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextBaseCompatibilityPendingReason {
    UnknownRectangle,
    RectangleBoundary,
    OriginalOrigin {
        pending: usize,
    },
    FinalFragment,
    /// On a cycle, or blocked downstream by a cycle; not a replay order.
    CyclicPrecedence,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextBaseCompatibilityPending {
    pub origin: usize,
    /// Visual display coordinates, including a boundary outside that plane.
    pub position: Option<(u16, u16)>,
    pub source: Option<TeletextEnhancementSource>,
    pub root_source: Option<TeletextSpatialRootSource>,
    pub invocation: Option<TeletextSpatialInvocationId>,
    pub reason: TeletextBaseCompatibilityPendingReason,
}
/// Indices stay associated with this exact borrowed immutable composition.
/// Even zero seeds/zero pending is not a lowering or readiness certificate.
#[derive(Debug)]
pub struct TeletextBaseCompatibilityRealizationPlan<'a> {
    composition: &'a TeletextMainComposition,
    seeds: Vec<TeletextBaseCompatibilitySeed>,
    origins: Vec<TeletextBaseCompatibilityOrigin>,
    fragments: Vec<TeletextBaseCompatibilityFragment>,
    precedence: Vec<TeletextBaseCompatibilityPrecedence>,
    pending: Vec<TeletextBaseCompatibilityPending>,
}
impl TeletextBaseCompatibilityRealizationPlan<'_> {
    #[must_use]
    pub const fn composition(&self) -> &TeletextMainComposition {
        self.composition
    }
    #[must_use]
    pub fn seeds(&self) -> &[TeletextBaseCompatibilitySeed] {
        &self.seeds
    }
    #[must_use]
    pub fn origins(&self) -> &[TeletextBaseCompatibilityOrigin] {
        &self.origins
    }
    #[must_use]
    pub fn fragments(&self) -> &[TeletextBaseCompatibilityFragment] {
        &self.fragments
    }
    #[must_use]
    pub fn precedence(&self) -> &[TeletextBaseCompatibilityPrecedence] {
        &self.precedence
    }
    #[must_use]
    pub fn pending(&self) -> &[TeletextBaseCompatibilityPending] {
        &self.pending
    }
    /// Global original diagnostics survive even without painted Active seeds.
    /// Attributes/charset pending and exact per-origin values remain accessible
    /// through `composition()`, rather than cloned or guessed per-cell snapshots.
    #[must_use]
    pub fn original_pending(&self) -> &[TeletextDisplayPending] {
        self.composition.display_resolution().pending()
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextBaseCompatibilityRealizationFailure {
    #[error("{kind} count {count} exceeds trusted budget {maximum}")]
    InputBudget {
        kind: &'static str,
        count: usize,
        maximum: usize,
    },
    #[error("seed count exceeds trusted budget {maximum}")]
    SeedBudget { maximum: usize },
    #[error("precedence count exceeds trusted budget {maximum}")]
    EdgeBudget { maximum: usize },
    #[error("closure work exceeds trusted budget {maximum}")]
    WorkBudget { maximum: usize },
    #[error("original origin/candidate correspondence failed")]
    Correspondence,
    #[error("bounded planning storage arithmetic exceeds supported range")]
    AllocationSize,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("teletext compatibility plan for {owner:?}, origin {origin:?}: {reason}")]
pub struct TeletextBaseCompatibilityRealizationError {
    pub owner: TeletextPageAddress,
    pub origin: Option<usize>,
    pub location: Option<TeletextEnhancementSource>,
    pub root_source: Option<TeletextSpatialRootSource>,
    pub invocation: Option<TeletextSpatialInvocationId>,
    pub reason: TeletextBaseCompatibilityRealizationFailure,
}

/// Plan collisions from actual emitted `BaseCell` glyph provenance. Each reached
/// origin is expanded once, through its whole effective rectangle (at most four
/// cells), including every final owner there. Unknown boundaries are not clipped
/// into a supposedly safe footprint; original global pending always survives.
/// # Errors
/// Rejects raw budgets/storage before allocation, bounded closure/constraint work
/// and inconsistent immutable origin/candidate references. No partial plan escapes.
pub fn plan_teletext_base_compatibility_realization(
    composition: &TeletextMainComposition,
    budget: TeletextBaseCompatibilityRealizationBudget,
) -> Result<TeletextBaseCompatibilityRealizationPlan<'_>, TeletextBaseCompatibilityRealizationError>
{
    let mut planner = Planner {
        composition,
        budget,
        remaining: budget.max_updates,
    };
    planner.preflight()?;
    let mut count = 0usize;
    for command in 0..composition.provenance().len() {
        planner.charge(None)?;
        if let Some(seed) = planner.seed(command)? {
            if count == budget.max_seeds {
                return Err(planner.error(
                    Some(seed.origin),
                    Failure::SeedBudget {
                        maximum: budget.max_seeds,
                    },
                ));
            }
            count += 1;
        }
    }
    let display = composition.display_resolution();
    let mut result = TeletextBaseCompatibilityRealizationPlan {
        composition,
        seeds: Vec::with_capacity(count),
        origins: Vec::new(),
        fragments: Vec::new(),
        precedence: Vec::new(),
        pending: Vec::new(),
    };
    let mut closure = Closure {
        included: vec![false; display.origins().len()],
        cells: vec![false; 25 * usize::from(display.width())],
        queue: Vec::new(),
        constraints: BTreeMap::new(),
    };
    for command in 0..composition.provenance().len() {
        planner.charge(None)?;
        if let Some(seed) = planner.seed(command)? {
            result.seeds.push(seed);
            enqueue(seed.origin, &mut closure.included, &mut closure.queue);
        }
    }
    let mut cursor = 0;
    while let Some(&index) = closure.queue.get(cursor) {
        cursor += 1;
        planner.charge(Some(index))?;
        let original = *display
            .origins()
            .get(index)
            .ok_or_else(|| planner.error(Some(index), Failure::Correspondence))?;
        let candidate = planner.candidate(index)?;
        let rectangle = rectangle(original);
        result.origins.push(TeletextBaseCompatibilityOrigin {
            origin_index: index,
            original,
            candidate,
            rectangle,
        });
        if let Some(pending) = original.pending() {
            result.pending.push(planner.pending(
                index,
                TeletextBaseCompatibilityPendingReason::OriginalOrigin { pending },
                None,
            )?);
        }
        if let Some(rectangle) = rectangle {
            planner.expand(index, rectangle, &mut result, &mut closure)?;
        } else {
            result.pending.push(planner.pending(
                index,
                TeletextBaseCompatibilityPendingReason::UnknownRectangle,
                None,
            )?);
        }
    }
    result.precedence = closure.constraints.into_values().collect();
    planner.cycles(&mut result, &closure.included)?;
    Ok(result)
}

type PlanningError = TeletextBaseCompatibilityRealizationError;
type Failure = TeletextBaseCompatibilityRealizationFailure;

struct Closure {
    included: Vec<bool>,
    cells: Vec<bool>,
    queue: Vec<usize>,
    constraints: BTreeMap<(usize, usize), TeletextBaseCompatibilityPrecedence>,
}
struct Planner<'a> {
    composition: &'a TeletextMainComposition,
    budget: TeletextBaseCompatibilityRealizationBudget,
    remaining: usize,
}
impl Planner<'_> {
    fn error(&self, origin: Option<usize>, reason: Failure) -> PlanningError {
        let candidate = origin
            .and_then(|index| self.composition.display_resolution().origins().get(index))
            .and_then(|value| {
                self.composition
                    .glyph_layers()
                    .candidates()
                    .get(value.candidate_index())
            });
        PlanningError {
            owner: self.composition.owner(),
            origin,
            location: candidate.and_then(TeletextGlyphCandidate::source),
            root_source: candidate.and_then(TeletextGlyphCandidate::root_source),
            invocation: candidate.and_then(TeletextGlyphCandidate::invocation),
            reason,
        }
    }
    fn candidate(&self, index: usize) -> Result<TeletextGlyphCandidate, PlanningError> {
        self.composition
            .display_resolution()
            .origins()
            .get(index)
            .and_then(|origin| {
                self.composition
                    .glyph_layers()
                    .candidates()
                    .get(origin.candidate_index())
            })
            .copied()
            .ok_or_else(|| self.error(Some(index), Failure::Correspondence))
    }
    fn charge(&mut self, origin: Option<usize>) -> Result<(), PlanningError> {
        self.remaining = self.remaining.checked_sub(1).ok_or_else(|| {
            self.error(
                origin,
                Failure::WorkBudget {
                    maximum: self.budget.max_updates,
                },
            )
        })?;
        Ok(())
    }
    fn preflight(&self) -> Result<(), PlanningError> {
        let composition = self.composition;
        let display = composition.display_resolution();
        for (kind, count, maximum) in [
            (
                "commands",
                composition
                    .program()
                    .commands()
                    .len()
                    .max(composition.spatial_trace().events().len()),
                self.budget.max_commands,
            ),
            (
                "candidates",
                composition.glyph_layers().candidates().len(),
                self.budget.max_candidates,
            ),
            ("origins", display.origins().len(), self.budget.max_origins),
        ] {
            if count > maximum {
                return Err(self.error(
                    (kind == "origins").then_some(maximum),
                    Failure::InputBudget {
                        kind,
                        count,
                        maximum,
                    },
                ));
            }
        }
        let origin_count = display.origins().len();
        let cells = 25 * usize::from(display.width());
        // Every expanded rectangle has <=4 points/edges/pending cell witnesses.
        for (count, size) in [
            (
                origin_count,
                std::mem::size_of::<TeletextBaseCompatibilityOrigin>()
                    + 10 * std::mem::size_of::<TeletextBaseCompatibilityPending>()
                    + 6 * std::mem::size_of::<usize>()
                    + std::mem::size_of::<Vec<usize>>(),
            ),
            (
                origin_count,
                8 * std::mem::size_of::<TeletextBaseCompatibilityPrecedence>(),
            ),
            (
                cells,
                std::mem::size_of::<TeletextBaseCompatibilityFragment>() + 1,
            ),
            (
                composition.provenance().len(),
                std::mem::size_of::<TeletextBaseCompatibilitySeed>(),
            ),
        ] {
            if count
                .checked_mul(size)
                .is_none_or(|bytes| isize::try_from(bytes).is_err())
            {
                return Err(self.error(None, Failure::AllocationSize));
            }
        }
        if composition.provenance().len() != composition.program().commands().len() {
            return Err(self.error(None, Failure::Correspondence));
        }
        Ok(())
    }
    fn seed(&self, command: usize) -> Result<Option<TeletextBaseCompatibilitySeed>, PlanningError> {
        let TeletextMainCompositionSource::BaseCell {
            position,
            kind: TeletextBaseCellCommandKind::Glyph,
        } = self.composition.provenance()[command]
        else {
            return Ok(None);
        };
        let visual = (
            position.row(),
            position.column() + self.composition.display_geometry().left_width(),
        );
        let Some(index) = self
            .composition
            .display_resolution()
            .cell(visual.0, visual.1)
            .and_then(ersatzrs_domain::TeletextDisplayCell::origin)
        else {
            return Ok(None);
        };
        if self.candidate(index)?.priority() != TeletextGlyphPriority::Active {
            return Ok(None);
        }
        Ok(Some(TeletextBaseCompatibilitySeed {
            base_position: (position.row(), position.column()),
            visual_position: visual,
            composed_command: command,
            origin: index,
        }))
    }
    fn pending(
        &self,
        origin: usize,
        reason: TeletextBaseCompatibilityPendingReason,
        position: Option<(u16, u16)>,
    ) -> Result<TeletextBaseCompatibilityPending, PlanningError> {
        let candidate = self.candidate(origin)?;
        Ok(TeletextBaseCompatibilityPending {
            origin,
            position,
            source: candidate.source(),
            root_source: candidate.root_source(),
            invocation: candidate.invocation(),
            reason,
        })
    }
    fn expand(
        &mut self,
        origin: usize,
        rectangle: TeletextBaseCompatibilityRectangle,
        result: &mut TeletextBaseCompatibilityRealizationPlan<'_>,
        closure: &mut Closure,
    ) -> Result<(), PlanningError> {
        let display = self.composition.display_resolution();
        let area = |column| {
            if column < self.composition.display_geometry().left_width() {
                0
            } else if column < self.composition.display_geometry().left_width() + 40 {
                1
            } else {
                2
            }
        };
        for dr in 0..rectangle.height {
            for dc in 0..rectangle.width {
                self.charge(Some(origin))?;
                let (visual_row, visual_column) = (
                    u16::from(rectangle.row) + u16::from(dr),
                    u16::from(rectangle.column) + u16::from(dc),
                );
                let position = Some((visual_row, visual_column));
                if visual_row >= 25 || visual_column >= u16::from(display.width()) {
                    result.pending.push(self.pending(
                        origin,
                        TeletextBaseCompatibilityPendingReason::RectangleBoundary,
                        position,
                    )?);
                    continue;
                }
                let (row, column) = (
                    u8::try_from(visual_row)
                        .map_err(|_| self.error(Some(origin), Failure::Correspondence))?,
                    u8::try_from(visual_column)
                        .map_err(|_| self.error(Some(origin), Failure::Correspondence))?,
                );
                if area(column) != area(rectangle.column) || (row == 0 && area(column) != 1) {
                    result.pending.push(self.pending(
                        origin,
                        TeletextBaseCompatibilityPendingReason::RectangleBoundary,
                        position,
                    )?);
                    // Retain in-display owners even across an uncertain area boundary.
                }
                let Some(_) = display.cell(row, column) else {
                    result.pending.push(self.pending(
                        origin,
                        TeletextBaseCompatibilityPendingReason::FinalFragment,
                        Some((u16::from(row), u16::from(column))),
                    )?);
                    continue;
                };
                self.retain_cell(origin, (row, column), result, closure)?;
            }
        }
        Ok(())
    }
    fn retain_cell(
        &self,
        origin: usize,
        position: (u8, u8),
        result: &mut TeletextBaseCompatibilityRealizationPlan<'_>,
        closure: &mut Closure,
    ) -> Result<(), PlanningError> {
        let (row, column) = position;
        let display = self.composition.display_resolution();
        let Some(cell) = display.cell(row, column) else {
            return Ok(());
        };
        let slot = usize::from(row) * usize::from(display.width()) + usize::from(column);
        if !closure.cells[slot] {
            closure.cells[slot] = true;
            result.fragments.push(TeletextBaseCompatibilityFragment {
                visual_position: (row, column),
                fragment: cell.fragment(),
                origin: cell.origin(),
            });
        }
        if cell.fragment() == TeletextDisplayFragment::Pending {
            result.pending.push(self.pending(
                origin,
                TeletextBaseCompatibilityPendingReason::FinalFragment,
                Some((u16::from(row), u16::from(column))),
            )?);
        }
        if let Some(owner) = cell.origin() {
            self.candidate(owner)?;
            enqueue(owner, &mut closure.included, &mut closure.queue);
            if owner != origin && !closure.constraints.contains_key(&(origin, owner)) {
                if closure.constraints.len() == self.budget.max_edges {
                    return Err(self.error(
                        Some(origin),
                        Failure::EdgeBudget {
                            maximum: self.budget.max_edges,
                        },
                    ));
                }
                closure.constraints.insert(
                    (origin, owner),
                    TeletextBaseCompatibilityPrecedence {
                        before: origin,
                        after: owner,
                        visual_position: (row, column),
                    },
                );
            }
        }
        Ok(())
    }
    fn cycles(
        &mut self,
        result: &mut TeletextBaseCompatibilityRealizationPlan<'_>,
        included: &[bool],
    ) -> Result<(), PlanningError> {
        let mut incoming = vec![0usize; included.len()];
        let mut outgoing = vec![Vec::new(); included.len()];
        for edge in &result.precedence {
            self.charge(Some(edge.before))?;
            incoming[edge.after] += 1;
            outgoing[edge.before].push(edge.after);
        }
        let mut ready: Vec<_> = result
            .origins
            .iter()
            .map(|origin| origin.origin_index)
            .filter(|&index| incoming[index] == 0)
            .collect();
        while let Some(index) = ready.pop() {
            self.charge(Some(index))?;
            for &next in &outgoing[index] {
                self.charge(Some(next))?;
                incoming[next] -= 1;
                if incoming[next] == 0 {
                    ready.push(next);
                }
            }
        }
        for (index, &count) in incoming.iter().enumerate() {
            self.charge(None)?;
            if count != 0 {
                result.pending.push(self.pending(
                    index,
                    TeletextBaseCompatibilityPendingReason::CyclicPrecedence,
                    None,
                )?);
            }
        }
        Ok(())
    }
}
fn enqueue(origin: usize, included: &mut [bool], queue: &mut Vec<usize>) {
    if !included[origin] {
        included[origin] = true;
        queue.push(origin);
    }
}
fn rectangle(origin: TeletextDisplayOrigin) -> Option<TeletextBaseCompatibilityRectangle> {
    let (row, column) = origin.visual_position()?;
    let (height, width) = match origin.effective_size()? {
        TeletextDisplaySize::Normal => (1, 1),
        TeletextDisplaySize::DoubleWidth => (1, 2),
        TeletextDisplaySize::DoubleHeight => (2, 1),
        TeletextDisplaySize::DoubleSize => (2, 2),
    };
    Some(TeletextBaseCompatibilityRectangle {
        row,
        column,
        height,
        width,
    })
}

#[cfg(test)]
#[path = "base_compatibility_realization_tests.rs"]
mod tests;
