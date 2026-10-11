//! Bounded unchanged-main anchors, not augmented wire or whole-plane equivalence.
//! Generated DAG order does not settle retained later paints; a subsequent
//! producer must compare its complete augmented display against the original.
use super::TeletextBaseCompatibilityRealizationPlan;
use ersatzrs_domain::{
    resolve_teletext_spatial_program, TeletextEnhancementObjectCatalog, TeletextEnhancementSource,
    TeletextObjectCatalogBudget, TeletextObjectOffset, TeletextPageAddress,
    TeletextSpatialInvocationId, TeletextSpatialRootSource, TeletextSpatialTraceBudget,
    TeletextSpatialTraceError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextOriginCarrierPlacementBudget {
    pub max_commands: usize,
    pub max_origins: usize,
    pub max_edges: usize,
    pub catalog: TeletextObjectCatalogBudget,
    pub spatial: TeletextSpatialTraceBudget,
    /// Placement scans/graph operations; spatial expansion has its own budgets.
    pub max_work: usize,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextOriginCarrierGap {
    after_command: Option<usize>,
    caller: (u16, u16),
}
impl TeletextOriginCarrierGap {
    #[must_use]
    pub const fn after_command(self) -> Option<usize> {
        self.after_command
    }
    #[must_use]
    pub const fn caller_position(self) -> (u16, u16) {
        self.caller
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextOriginCarrierPlacementCall {
    origin: usize,
    raw: (u8, u8),
    offset: TeletextObjectOffset,
}
impl TeletextOriginCarrierPlacementCall {
    #[must_use]
    pub const fn original_origin_index(self) -> usize {
        self.origin
    }
    #[must_use]
    pub const fn raw_position(self) -> (u8, u8) {
        self.raw
    }
    #[must_use]
    pub const fn offset(self) -> TeletextObjectOffset {
        self.offset
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextOriginCarrierPlacementPending {
    OriginalPlan { origin: usize },
    UnknownRectangle { origin: usize },
    CyclicPrecedence,
    NoUnchangedGap,
}
#[derive(Debug)]
pub struct TeletextOriginCarrierPlacementComponent {
    origins: Vec<usize>,
    gap: Option<TeletextOriginCarrierGap>,
    calls: Vec<TeletextOriginCarrierPlacementCall>,
    pending: Option<TeletextOriginCarrierPlacementPending>,
}
impl TeletextOriginCarrierPlacementComponent {
    /// Topological order unless cyclic; cyclic membership is never a replay order.
    #[must_use]
    pub fn origin_indices(&self) -> &[usize] {
        &self.origins
    }
    #[must_use]
    pub const fn gap(&self) -> Option<TeletextOriginCarrierGap> {
        self.gap
    }
    #[must_use]
    pub fn calls(&self) -> &[TeletextOriginCarrierPlacementCall] {
        &self.calls
    }
    #[must_use]
    pub const fn pending(&self) -> Option<TeletextOriginCarrierPlacementPending> {
        self.pending
    }
}
/// Exact borrowed identity, not detached correspondence or admission authority.
/// Zero components still retain global original pending through `plan()`.
#[derive(Debug)]
pub struct TeletextOriginCarrierPlacement<'a> {
    plan: &'a TeletextBaseCompatibilityRealizationPlan<'a>,
    catalog: &'a TeletextEnhancementObjectCatalog,
    components: Vec<TeletextOriginCarrierPlacementComponent>,
}
impl TeletextOriginCarrierPlacement<'_> {
    #[must_use]
    pub const fn plan(&self) -> &TeletextBaseCompatibilityRealizationPlan<'_> {
        self.plan
    }
    #[must_use]
    pub const fn catalog(&self) -> &TeletextEnhancementObjectCatalog {
        self.catalog
    }
    #[must_use]
    pub fn components(&self) -> &[TeletextOriginCarrierPlacementComponent] {
        &self.components
    }
}
#[derive(Debug, thiserror::Error)]
pub enum TeletextOriginCarrierPlacementFailure {
    #[error("{kind} count {count} exceeds trusted maximum {maximum}")]
    InputBudget {
        kind: &'static str,
        count: usize,
        maximum: usize,
    },
    #[error("placement work exceeds trusted maximum {maximum}")]
    WorkBudget { maximum: usize },
    #[error("placement storage/work arithmetic exceeds supported range")]
    AllocationSize,
    #[error("exact original origin or edge correspondence failed")]
    Correspondence,
    #[error("actual composed main cannot be traced: {0}")]
    Spatial(Box<TeletextSpatialTraceError>),
}
#[derive(Debug, thiserror::Error)]
#[error("teletext carrier placement for {owner:?}, origin {origin:?}: {reason}")]
pub struct TeletextOriginCarrierPlacementError {
    pub owner: TeletextPageAddress,
    pub origin: Option<usize>,
    pub location: Option<TeletextEnhancementSource>,
    pub root_source: Option<TeletextSpatialRootSource>,
    pub invocation: Option<TeletextSpatialInvocationId>,
    pub reason: TeletextOriginCarrierPlacementFailure,
}
/// Choose the latest unchanged main gap componentwise before all raw origins,
/// with checked relative offsets. Child events never become main gaps.
/// # Errors
/// Rejects input/storage/work limits and invalid actual main tracing. Unresolved
/// rectangles, cycles and absent anchors remain component pending. No mutation.
pub fn plan_teletext_origin_carrier_placement<'a>(
    plan: &'a TeletextBaseCompatibilityRealizationPlan<'a>,
    catalog: &'a TeletextEnhancementObjectCatalog,
    budget: TeletextOriginCarrierPlacementBudget,
) -> Result<TeletextOriginCarrierPlacement<'a>, TeletextOriginCarrierPlacementError> {
    let mut planner = PlacementPlanner {
        plan,
        catalog,
        budget,
        remaining: budget.max_work,
    };
    planner.preflight()?;
    let composition = plan.composition();
    let trace = resolve_teletext_spatial_program(
        composition.owner(),
        composition.program(),
        catalog,
        budget.spatial,
    )
    .map_err(|error| TeletextOriginCarrierPlacementError {
        owner: composition.owner(),
        origin: None,
        location: error.location,
        root_source: error.root_source,
        invocation: error.invocation,
        reason: Failure::Spatial(Box::new(error)),
    })?;
    let mut gaps = vec![TeletextOriginCarrierGap {
        after_command: None,
        caller: (0, 0),
    }];
    for event in trace.events() {
        planner.charge(None)?;
        if event.invocation().is_none() && event.source().object.is_none() {
            if event.source().command != gaps.len() - 1 {
                return Err(planner.error(None, Failure::Correspondence));
            }
            let ap = event.absolute_after();
            gaps.push(TeletextOriginCarrierGap {
                after_command: Some(event.source().command),
                caller: (ap.row(), ap.column()),
            });
        }
    }
    if gaps.len() - 1 != composition.program().commands().len() {
        return Err(planner.error(None, Failure::Correspondence));
    }
    let mut used = vec![false; plan.origins().len()];
    let mut components = Vec::new();
    for first in 0..used.len() {
        planner.charge(Some(first))?;
        if used[first] {
            continue;
        }
        used[first] = true;
        let mut members = vec![first];
        let mut cursor = 0;
        while cursor < members.len() {
            let current = members[cursor];
            cursor += 1;
            for (other, included) in used.iter_mut().enumerate() {
                planner.charge(Some(current))?;
                if !*included && planner.connected(current, other)? {
                    *included = true;
                    members.push(other);
                }
            }
        }
        components.push(planner.component(&members, &gaps)?);
    }
    Ok(TeletextOriginCarrierPlacement {
        plan,
        catalog,
        components,
    })
}
type Failure = TeletextOriginCarrierPlacementFailure;
type Error = TeletextOriginCarrierPlacementError;
struct PlacementPlanner<'a> {
    plan: &'a TeletextBaseCompatibilityRealizationPlan<'a>,
    catalog: &'a TeletextEnhancementObjectCatalog,
    budget: TeletextOriginCarrierPlacementBudget,
    remaining: usize,
}
impl PlacementPlanner<'_> {
    fn error(&self, index: Option<usize>, reason: Failure) -> Error {
        let origin = index.and_then(|index| self.plan.origins().get(index));
        Error {
            owner: self.plan.composition().owner(),
            origin: origin.map(|o| o.origin_index),
            location: origin.and_then(|o| o.candidate.source()),
            root_source: origin.and_then(|o| o.candidate.root_source()),
            invocation: origin.and_then(|o| o.candidate.invocation()),
            reason,
        }
    }
    fn charge(&mut self, origin: Option<usize>) -> Result<(), Error> {
        self.remaining = self.remaining.checked_sub(1).ok_or_else(|| {
            self.error(
                origin,
                Failure::WorkBudget {
                    maximum: self.budget.max_work,
                },
            )
        })?;
        Ok(())
    }
    fn preflight(&mut self) -> Result<(), Error> {
        let commands = self.plan.composition().program().commands().len();
        let n = self.plan.origins().len();
        for (kind, count, maximum) in [
            ("main commands", commands, self.budget.max_commands),
            ("origins", n, self.budget.max_origins),
            ("edges", self.plan.precedence().len(), self.budget.max_edges),
            (
                "catalog objects",
                self.catalog.definitions().len(),
                self.budget.catalog.max_objects,
            ),
        ] {
            if count > maximum {
                return Err(self.error(
                    None,
                    Failure::InputBudget {
                        kind,
                        count,
                        maximum,
                    },
                ));
            }
        }
        let mut count = 0usize;
        for definition in self.catalog.definitions() {
            self.charge(None)?;
            count = count
                .checked_add(definition.program.commands().len())
                .ok_or_else(|| self.error(None, Failure::AllocationSize))?;
        }
        if count > self.budget.catalog.max_commands {
            return Err(self.error(
                None,
                Failure::InputBudget {
                    kind: "catalog commands",
                    count,
                    maximum: self.budget.catalog.max_commands,
                },
            ));
        }
        for (count, size) in [
            (
                n,
                std::mem::size_of::<TeletextOriginCarrierPlacementComponent>(),
            ),
            (n, std::mem::size_of::<TeletextOriginCarrierPlacementCall>()),
            (n, 6 * std::mem::size_of::<usize>()),
            (
                commands
                    .checked_add(1)
                    .ok_or_else(|| self.error(None, Failure::AllocationSize))?,
                std::mem::size_of::<TeletextOriginCarrierGap>(),
            ),
        ] {
            if count
                .checked_mul(size)
                .is_none_or(|bytes| bytes > isize::MAX.unsigned_abs())
            {
                return Err(self.error(None, Failure::AllocationSize));
            }
        }
        n.checked_mul(n)
            .and_then(|count| count.checked_mul(self.plan.precedence().len().max(1)))
            .and_then(|_| {
                self.budget
                    .spatial
                    .max_events
                    .checked_mul(self.catalog.definitions().len().max(1))
            })
            .ok_or_else(|| self.error(None, Failure::AllocationSize))?;
        for (index, origin) in self.plan.origins().iter().enumerate() {
            self.charge(Some(index))?;
            if self
                .plan
                .composition()
                .display_resolution()
                .origins()
                .get(origin.origin_index)
                != Some(&origin.original)
            {
                return Err(self.error(Some(index), Failure::Correspondence));
            }
        }
        for edge in self.plan.precedence() {
            let mut before = false;
            let mut after = false;
            for origin in self.plan.origins() {
                self.charge(None)?;
                before |= origin.origin_index == edge.before;
                after |= origin.origin_index == edge.after;
            }
            if !before || !after {
                return Err(self.error(None, Failure::Correspondence));
            }
        }
        Ok(())
    }
    fn connected(&mut self, a: usize, b: usize) -> Result<bool, Error> {
        let current = a;
        let a = self.plan.origins()[a];
        let b = self.plan.origins()[b];
        for edge in self.plan.precedence() {
            self.charge(Some(current))?;
            if (edge.before == a.origin_index && edge.after == b.origin_index)
                || (edge.before == b.origin_index && edge.after == a.origin_index)
            {
                return Ok(true);
            }
        }
        Ok(match (a.rectangle, b.rectangle) {
            (Some(a), Some(b)) => {
                u16::from(a.row) < u16::from(b.row) + u16::from(b.height)
                    && u16::from(b.row) < u16::from(a.row) + u16::from(a.height)
                    && u16::from(a.column) < u16::from(b.column) + u16::from(b.width)
                    && u16::from(b.column) < u16::from(a.column) + u16::from(a.width)
            }
            _ => false,
        })
    }
    fn order(&mut self, members: &[usize]) -> Result<(Vec<usize>, bool), Error> {
        let mut remaining = members.to_vec();
        let mut ordered = Vec::with_capacity(members.len());
        while !remaining.is_empty() {
            let mut next = None;
            for (position, &index) in remaining.iter().enumerate() {
                self.charge(Some(index))?;
                let mut incoming = false;
                for edge in self.plan.precedence() {
                    self.charge(Some(index))?;
                    if edge.after == self.plan.origins()[index].origin_index {
                        for &other in &remaining {
                            self.charge(Some(index))?;
                            incoming |= self.plan.origins()[other].origin_index == edge.before;
                        }
                    }
                }
                if !incoming
                    && next.is_none_or(|old: usize| {
                        self.plan.origins()[index].origin_index
                            < self.plan.origins()[remaining[old]].origin_index
                    })
                {
                    next = Some(position);
                }
            }
            let Some(next) = next else {
                break;
            };
            ordered.push(remaining.remove(next));
        }
        let cyclic = !remaining.is_empty();
        ordered.extend(remaining);
        Ok((ordered, cyclic))
    }
    fn component(
        &mut self,
        members: &[usize],
        gaps: &[TeletextOriginCarrierGap],
    ) -> Result<TeletextOriginCarrierPlacementComponent, Error> {
        let (ordered, cyclic) = self.order(members)?;
        let mut component = TeletextOriginCarrierPlacementComponent {
            origins: ordered
                .iter()
                .map(|&index| self.plan.origins()[index].origin_index)
                .collect(),
            gap: None,
            calls: Vec::new(),
            pending: None,
        };
        if cyclic {
            component.pending = Some(TeletextOriginCarrierPlacementPending::CyclicPrecedence);
            return Ok(component);
        }
        let mut raw = Vec::with_capacity(ordered.len());
        for &index in &ordered {
            self.charge(Some(index))?;
            let origin = self.plan.origins()[index];
            for pending in self.plan.pending() {
                self.charge(Some(index))?;
                if pending.origin == origin.origin_index {
                    component.pending = Some(TeletextOriginCarrierPlacementPending::OriginalPlan {
                        origin: origin.origin_index,
                    });
                    return Ok(component);
                }
            }
            let Some(rectangle) = origin.rectangle else {
                component.pending = Some(TeletextOriginCarrierPlacementPending::UnknownRectangle {
                    origin: origin.origin_index,
                });
                return Ok(component);
            };
            let geometry = self.plan.composition().display_geometry();
            let Some(position) = inverse_visual(
                geometry.left_width(),
                geometry.right_width(),
                rectangle.row,
                rectangle.column,
            ) else {
                component.pending = Some(TeletextOriginCarrierPlacementPending::UnknownRectangle {
                    origin: origin.origin_index,
                });
                return Ok(component);
            };
            raw.push(position);
        }
        for &gap in gaps.iter().rev() {
            let mut viable = true;
            for (&index, &position) in ordered.iter().zip(&raw) {
                self.charge(Some(index))?;
                viable &= offset(gap.caller, position).is_some();
            }
            if viable {
                component.gap = Some(gap);
                for (&index, &position) in ordered.iter().zip(&raw) {
                    let offset = offset(gap.caller, position)
                        .ok_or_else(|| self.error(Some(index), Failure::Correspondence))?;
                    component.calls.push(TeletextOriginCarrierPlacementCall {
                        origin: self.plan.origins()[index].origin_index,
                        raw: position,
                        offset,
                    });
                }
                return Ok(component);
            }
        }
        component.pending = Some(TeletextOriginCarrierPlacementPending::NoUnchangedGap);
        Ok(component)
    }
}
fn inverse_visual(left: u8, right: u8, row: u8, column: u8) -> Option<(u8, u8)> {
    if row > 24 || column >= left + 40 + right {
        return None;
    }
    let raw = if column < left {
        72 - left + column
    } else {
        column - left
    };
    if row == 0 && !(8..40).contains(&raw) {
        return None;
    }
    Some((row, raw))
}
fn offset(caller: (u16, u16), raw: (u8, u8)) -> Option<TeletextObjectOffset> {
    let row = u8::try_from(u16::from(raw.0).checked_sub(caller.0)?).ok()?;
    let column = u8::try_from(u16::from(raw.1).checked_sub(caller.1)?).ok()?;
    TeletextObjectOffset::new(row, column).ok()
}

#[cfg(test)]
#[path = "origin_carrier_placement_tests.rs"]
mod tests;
