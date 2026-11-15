//! Response-local rotation on the existing programme clock, without a timer.

use std::sync::Arc;

use ersatzrs_domain::{TeletextPageAddress, TeletextSubpageCycle};

use super::{
    page_batches, CarouselContent, TeletextTransportError, MAX_RESOURCE_CYCLE_BATCHES, STEP_TICKS,
};

#[derive(Debug)]
struct Group {
    cycle: TeletextSubpageCycle,
    members: Vec<usize>,
    addresses: Vec<TeletextPageAddress>,
}

#[derive(Debug)]
struct Plan {
    groups: Vec<Group>,
    membership: Vec<Option<(usize, usize)>>,
}

#[derive(Debug, Clone, Copy, Default)]
struct Position {
    active: usize,
    started: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct SubpageRotation {
    plan: Option<Arc<Plan>>,
    positions: Vec<Position>,
    seen: Vec<bool>,
    pending_first_header: Option<usize>,
}

impl SubpageRotation {
    pub(super) fn new(
        content: &CarouselContent,
        previous: Option<&Self>,
    ) -> Result<Self, TeletextTransportError> {
        let CarouselContent::Authored { service, .. } = content else {
            return Ok(Self::default());
        };
        Self::from_service(service, previous)
    }

    pub(super) fn validate(
        service: &super::EncodedTeletextAuthoredService,
    ) -> Result<(), TeletextTransportError> {
        Self::from_service(service, None).map(|_| ())
    }

    fn from_service(
        service: &super::EncodedTeletextAuthoredService,
        previous: Option<&Self>,
    ) -> Result<Self, TeletextTransportError> {
        if service.subpage_cycles().is_empty() {
            return Ok(Self::default());
        }
        let displays = &service.declared().semantic().base().pages;
        let offset = service
            .pages()
            .len()
            .checked_sub(displays.len())
            .ok_or(TeletextTransportError::ClockOverflow)?;
        let mut membership = vec![None; service.pages().len()];
        let mut groups = Vec::with_capacity(service.subpage_cycles().len());
        let mut positions = Vec::with_capacity(service.subpage_cycles().len());
        for cycle in service.subpage_cycles() {
            let mut members = Vec::new();
            let mut addresses = Vec::new();
            for (index, page) in displays
                .iter()
                .enumerate()
                .filter(|(_, page)| page.address.page == cycle.page())
            {
                membership[index + offset] = Some((groups.len(), members.len()));
                members.push(index + offset);
                addresses.push(page.address);
            }
            if members.is_empty() {
                return Err(TeletextTransportError::ClockOverflow);
            }
            let prior = previous
                .and_then(|old| {
                    old.plan.as_ref().and_then(|plan| {
                        plan.groups
                            .iter()
                            .position(|group| group.cycle == *cycle && group.addresses == addresses)
                            .map(|index| old.positions[index])
                    })
                })
                .unwrap_or_default();
            positions.push(prior);
            groups.push(Group {
                cycle: *cycle,
                members,
                addresses,
            });
        }
        let rotation = Self {
            plan: Some(Arc::new(Plan { groups, membership })),
            positions,
            seen: vec![false; service.pages().len()],
            pending_first_header: None,
        };
        rotation.validate_cadence(service)?;
        Ok(rotation)
    }

    // Timed updates consume the existing 50 intervals/s; acquisition retains its
    // existing 20 s bound. They must leave positive capacity for ordinary sweep
    // progress and its closing FF, rather than limiting the whole sweep to the
    // shortest page's dwell. Floating-point rounding is bounded conservatively.
    fn validate_cadence(
        &self,
        service: &super::EncodedTeletextAuthoredService,
    ) -> Result<(), TeletextTransportError> {
        let Some(plan) = &self.plan else {
            return Ok(());
        };
        if plan.groups.iter().all(|group| group.members.len() == 1) {
            return Ok(());
        }
        let displays = service.declared().semantic().base().pages.len();
        let resources = service
            .pages()
            .len()
            .checked_sub(displays)
            .ok_or(TeletextTransportError::ClockOverflow)?;
        let prefix = service.pages()[displays..].iter().try_fold(
            usize::from(!service.magazine_units().is_empty()),
            |sum, page| {
                sum.checked_add(page_batches(page.data_units()))
                    .ok_or(TeletextTransportError::ClockOverflow)
            },
        )?;
        let mut load =
            f64::from(u32::try_from(prefix).map_err(|_| TeletextTransportError::ClockOverflow)?)
                / f64::from(
                    u32::try_from(MAX_RESOURCE_CYCLE_BATCHES)
                        .map_err(|_| TeletextTransportError::ClockOverflow)?,
                );
        for group in &plan.groups {
            if group.members.len() == 1 {
                continue;
            }
            let cost = group
                .members
                .iter()
                .map(|index| page_batches(service.pages()[*index - resources].data_units()))
                .max()
                .unwrap_or(0);
            load +=
                f64::from(u32::try_from(cost).map_err(|_| TeletextTransportError::ClockOverflow)?)
                    / (f64::from(group.cycle.seconds()) * 50.0);
        }
        let terms = f64::from(
            u16::try_from(plan.groups.len()).map_err(|_| TeletextTransportError::ClockOverflow)?,
        ) + 1.0;
        if load + f64::EPSILON * terms >= 1.0 {
            return Err(TeletextTransportError::SubpageCapacity);
        }
        Ok(())
    }

    pub(super) fn due(&self, ticks: u64) -> Result<Option<usize>, TeletextTransportError> {
        let Some(plan) = &self.plan else {
            return Ok(None);
        };
        let mut earliest: Option<(u64, usize)> = None;
        for (index, group) in plan.groups.iter().enumerate() {
            if group.members.len() == 1 {
                continue;
            }
            let Some(start) = self.positions[index].started else {
                continue;
            };
            let elapsed = ticks
                .checked_sub(start)
                .ok_or(TeletextTransportError::ClockOverflow)?;
            let period = u64::from(group.cycle.seconds()) * 90_000;
            if elapsed >= period && earliest.is_none_or(|(overdue, _)| elapsed - period > overdue) {
                earliest = Some((elapsed - period, group.members[0]));
            }
        }
        Ok(earliest.map(|(_, page)| page))
    }

    fn member(&self, page: usize) -> Option<(usize, usize)> {
        self.plan.as_ref()?.membership.get(page).copied().flatten()
    }

    pub(super) fn emits(&self, page: usize) -> bool {
        match (self.plan.as_ref(), self.member(page)) {
            (Some(plan), Some((group, _))) => plan.groups[group].members[0] == page,
            _ => true,
        }
    }

    pub(super) fn current(&self, page: usize) -> usize {
        match (self.plan.as_ref(), self.member(page)) {
            (Some(plan), Some((group, _))) => {
                plan.groups[group].members[self.positions[group].active]
            }
            _ => page,
        }
    }

    pub(super) fn choose(&self, page: usize, ticks: u64) -> Result<usize, TeletextTransportError> {
        let (Some(plan), Some((group, _))) = (&self.plan, self.member(page)) else {
            return Ok(page);
        };
        let position = self.positions[group];
        let definition = &plan.groups[group];
        let due = position
            .started
            .map(|start| {
                ticks
                    .checked_sub(start)
                    .ok_or(TeletextTransportError::ClockOverflow)
            })
            .transpose()?
            .is_some_and(|elapsed| elapsed >= u64::from(definition.cycle.seconds()) * 90_000);
        let active = if due {
            (position.active + 1) % definition.members.len()
        } else {
            position.active
        };
        Ok(definition.members[active])
    }

    pub(super) fn was_transmitted(&self, page: usize) -> bool {
        self.member(page).is_none() || self.seen[page]
    }

    pub(super) fn repeat(&self, page: usize, ordinary: bool, inherited: bool) -> bool {
        if self.member(page).is_some() {
            self.seen[page] || inherited
        } else {
            ordinary || inherited
        }
    }

    pub(super) fn begin_batch(&mut self) {
        self.pending_first_header = None;
    }

    pub(super) fn record_header(&mut self, page: usize, ticks: u64) {
        if let Some((group, active)) = self.member(page) {
            let position = &mut self.positions[group];
            if position.active != active || position.started.is_none() {
                *position = Position {
                    active,
                    started: Some(ticks),
                };
                self.pending_first_header = Some(group);
            }
            self.seen[page] = true;
        }
    }

    // A first header awaiting null slots has not started its physical dwell yet.
    pub(super) fn defer_pending_header(&mut self) -> Result<(), TeletextTransportError> {
        if let Some(group) = self.pending_first_header {
            let start = self.positions[group]
                .started
                .ok_or(TeletextTransportError::ClockOverflow)?;
            let next = start
                .checked_add(STEP_TICKS)
                .ok_or(TeletextTransportError::ClockOverflow)?;
            self.positions[group].started = Some(next);
        }
        Ok(())
    }
}
