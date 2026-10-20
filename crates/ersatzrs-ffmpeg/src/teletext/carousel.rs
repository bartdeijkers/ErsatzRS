use std::{collections::BTreeMap, sync::Arc};

use ersatzrs_domain::{TeletextResourceHeaderMetadata, TeletextResourceKind};

use super::{
    packets, EncodedTeletextAuthoredService, EncodedTeletextPageSet, TeletextTransportError,
};

// 31 units plus the data identifier fit within the 1,504-byte Bttx buffer.
pub(super) const MAX_BATCH_UNITS: usize = 31;
// EN 300 706 B.1/B.9: at least 20 ms after erase, at most 100 ms between packets.
const STEP_TICKS: u64 = 1_800;
// EN 300 706 B.6: resource and magazine acquisition repetition is at most 20 s.
const MAX_RESOURCE_CYCLE_BATCHES: usize = 1_000;
// Null-slot placement may differ within each 20 ms output window.
// Reserve one window so physical packet positions retain the B.6 bound.
const RESOURCE_WINDOW_MARGIN: usize = 1;

/// Purpose of a bounded serial-mode transmission batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextCarouselBatchKind {
    /// Service-wide magazine declarations, preceding resource acquisition.
    Magazine,
    /// A page header, sent independently of its remaining packets.
    Header,
    /// Ordered links, designation, enhancements and display rows.
    Page,
    /// Reserved mFF header closing the final page of a complete cycle.
    Terminator,
}

/// Host-authored units with a relative 90 kHz presentation time.
///
/// This is a transmission plan, not evidence of delivery. The output adapter
/// must reserve capacity, pace packets on the programme clock, preserve the
/// erase interval and satisfy the DVB decoder buffer model. It must never
/// catch up overdue batches by emitting a burst or silently dropping units.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextCarouselBatch {
    ticks: u64,
    kind: TeletextCarouselBatchKind,
    units: Vec<[u8; 46]>,
}

impl TeletextCarouselBatch {
    pub(super) fn retirement(
        ticks: u64,
        kind: TeletextCarouselBatchKind,
        units: Vec<[u8; 46]>,
    ) -> Self {
        Self { ticks, kind, units }
    }
    /// Presentation offset in 90 kHz ticks from the carousel's programme anchor.
    #[must_use]
    pub const fn presentation_ticks(&self) -> u64 {
        self.ticks
    }

    /// Whether these packets open, extend or close a page cycle.
    #[must_use]
    pub const fn kind(&self) -> TeletextCarouselBatchKind {
        self.kind
    }

    /// At most 31 immutable EN 300 472 units in their required wire order.
    #[must_use]
    pub fn data_units(&self) -> &[[u8; 46]] {
        &self.units
    }

    /// Exact number of payload-only TS packets required by this bounded batch.
    #[must_use]
    pub fn transport_packet_count(&self) -> usize {
        (self.units.len() + 1).div_ceil(4)
    }
}

/// Bounded, synchronous page/subpage carousel over an entirely encoded set.
///
/// Legacy pages retain producer order and exact subpage identities. Authored
/// services acquire magazines, MOT, DRCS and object pages before displays.
/// There is no task,
/// timer, input-media access or background queue here. Publication ownership,
/// generation fencing, cancellation and actual multiplex pacing belong to the
/// host adapter. Replacement is permitted only after the cycle's terminator.
#[derive(Debug)]
pub struct TeletextCarousel {
    content: CarouselContent,
    page: usize,
    unit: usize,
    ticks: u64,
    boundary: bool,
    first_cycle: bool,
    magazine_pending: bool,
    resource_start_ticks: u64,
    resource_resume: Option<usize>,
    resource_history: super::service_update::ResourceUpdateHistory,
}

#[derive(Debug)]
enum CarouselContent {
    Legacy(EncodedTeletextPageSet),
    Authored {
        service: Arc<EncodedTeletextAuthoredService>,
        resource_order: Vec<usize>,
        selected_headers: super::resource_lifecycle::SelectedResourceHeaders,
    },
}

/// Fallible work sealed for one synchronous replacement of the same carousel.
/// Its fields are private; this carries no delivery or generation authority.
pub(super) struct PreparedCarouselReplacement {
    content: CarouselContent,
}

impl CarouselContent {
    fn authored(
        service: Arc<EncodedTeletextAuthoredService>,
    ) -> Result<Self, TeletextTransportError> {
        Self::validate_authored(&service)?;
        Ok(Self::authored_prevalidated(service, BTreeMap::default()))
    }

    fn validate_authored(
        service: &EncodedTeletextAuthoredService,
    ) -> Result<(), TeletextTransportError> {
        Self::validate_authored_with_prefix(service, 0)
    }

    fn validate_authored_with_prefix(
        service: &EncodedTeletextAuthoredService,
        prefix_batches: usize,
    ) -> Result<(), TeletextTransportError> {
        let magazine_batch = usize::from(!service.magazine_units().is_empty());
        let displays = service.declared().semantic().base().pages.len();
        let resources = &service.pages()[displays..];
        let resource_batches = resources.iter().try_fold(magazine_batch, |sum, page| {
            sum.checked_add(page_batches(page.data_units()))
                .ok_or(TeletextTransportError::ClockOverflow)
        })?;
        let largest_display = service.pages()[..displays]
            .iter()
            .map(|page| page_batches(page.data_units()))
            .max()
            .unwrap_or(0);
        let batches = resource_batches
            .checked_add(largest_display)
            .and_then(|sum| sum.checked_add(1 + RESOURCE_WINDOW_MARGIN))
            .and_then(|sum| sum.checked_add(prefix_batches))
            .ok_or(TeletextTransportError::ClockOverflow)?;
        if (resource_batches != 0 || prefix_batches != 0) && batches > MAX_RESOURCE_CYCLE_BATCHES {
            return Err(TeletextTransportError::CycleTiming {
                batches,
                maximum: MAX_RESOURCE_CYCLE_BATCHES,
            });
        }
        Ok(())
    }

    fn authored_prevalidated(
        service: Arc<EncodedTeletextAuthoredService>,
        selected_headers: super::resource_lifecycle::SelectedResourceHeaders,
    ) -> Self {
        let displays = service.declared().semantic().base().pages.len();
        let mut resource_order: Vec<_> = (displays..service.pages().len()).collect();
        resource_order.sort_unstable_by_key(|&index| {
            // Physical headers also cover compiler-owned banks absent from the
            // original authored inventory. Encoded resources follow displays.
            let kind = service.pages()[index]
                .resource_header()
                .expect("encoded resource pages retain their physical identity")
                .identity()
                .kind();
            let rank = match kind {
                TeletextResourceKind::Mot => 0,
                TeletextResourceKind::Drcs | TeletextResourceKind::Gdrcs => 1,
                TeletextResourceKind::Pop | TeletextResourceKind::Gpop => 2,
                // The authored producer currently excludes MIP pages.
                TeletextResourceKind::Mip => 3,
            };
            (rank, index)
        });
        Self::Authored {
            service,
            resource_order,
            selected_headers,
        }
    }

    fn page_count(&self) -> usize {
        match self {
            Self::Legacy(set) => set.pages().len(),
            Self::Authored { service, .. } => service.pages().len(),
        }
    }

    fn resource_count(&self) -> usize {
        match self {
            Self::Legacy(_) => 0,
            Self::Authored { resource_order, .. } => resource_order.len(),
        }
    }

    fn has_acquisition(&self) -> bool {
        self.resource_count() != 0 || !self.magazines().is_empty()
    }

    fn magazines(&self) -> &[[u8; 46]] {
        match self {
            Self::Legacy(_) => &[],
            Self::Authored { service, .. } => service.magazine_units(),
        }
    }

    fn initial_magazine(&self) -> u8 {
        match self {
            Self::Legacy(set) => set.initial_page().wire_magazine(),
            Self::Authored { service, .. } => service
                .declared()
                .semantic()
                .base()
                .initial_page
                .wire_magazine(),
        }
    }

    fn page(&self, logical: usize) -> (&[[u8; 46]], u16, Option<TeletextResourceHeaderMetadata>) {
        match self {
            Self::Legacy(set) => {
                let page = &set.pages()[logical];
                (page.data_units(), page.address().subcode.get(), None)
            }
            Self::Authored {
                service,
                resource_order,
                ..
            } => {
                let physical = if logical < resource_order.len() {
                    resource_order[logical]
                } else {
                    logical - resource_order.len()
                };
                let page = &service.pages()[physical];
                let subcode = match page.inventory_record() {
                    super::TeletextPhysicalPageInventoryRecord::Display(address) => {
                        address.subcode.get()
                    }
                    super::TeletextPhysicalPageInventoryRecord::Resource(header) => {
                        header.physical_subcode()
                    }
                };
                (page.data_units(), subcode, page.resource_header())
            }
        }
    }
    fn selected_header(&self, logical: usize, repeat: bool) -> Option<[u8; 46]> {
        let Self::Authored {
            resource_order,
            selected_headers,
            ..
        } = self
        else {
            return None;
        };
        let physical = *resource_order.get(logical)?;
        selected_headers.get(&physical).map(|selected| {
            if repeat {
                selected.repeat
            } else {
                selected.first
            }
        })
    }
}

fn page_batches(units: &[[u8; 46]]) -> usize {
    1 + (units.len() - 1).div_ceil(MAX_BATCH_UNITS)
}

impl TeletextCarousel {
    pub(super) fn validate_retirement_prefix(
        service: &EncodedTeletextAuthoredService,
        batches: usize,
    ) -> Result<(), TeletextTransportError> {
        CarouselContent::validate_authored_with_prefix(service, batches)
    }
    pub(super) fn authored_service_shared(&self) -> Option<&Arc<EncodedTeletextAuthoredService>> {
        match &self.content {
            CarouselContent::Authored { service, .. } => Some(service),
            CarouselContent::Legacy(_) => None,
        }
    }
    /// Start a plan at relative time zero from a validated, fully encoded set.
    #[must_use]
    pub fn new(set: EncodedTeletextPageSet) -> Self {
        Self {
            content: CarouselContent::Legacy(set),
            page: 0,
            unit: 0,
            ticks: 0,
            boundary: true,
            first_cycle: true,
            magazine_pending: false,
            resource_start_ticks: 0,
            resource_resume: None,
            resource_history: super::service_update::ResourceUpdateHistory::default(),
        }
    }

    /// Start one complete authored service, acquiring resources before displays.
    /// Refresh complete resources/M29 between complete displays within 20 seconds.
    /// Display cycles may be longer; refreshes do not permit generation replacement.
    /// # Errors
    /// Rejects an acquisition sweep plus one whole display/terminator that cannot
    /// fit the fixed 20 ms cadence's resource repetition bound, including one
    /// window of physical null-slot placement margin.
    pub fn new_authored(
        service: EncodedTeletextAuthoredService,
    ) -> Result<Self, TeletextTransportError> {
        Self::new_authored_shared(Arc::new(service))
    }

    /// Start a fresh cursor over the same immutable complete authored service.
    /// No display composition or encoded resource body is cloned.
    /// # Errors
    /// Enforces the same complete acquisition cadence as the owned constructor.
    pub fn new_authored_shared(
        service: Arc<EncodedTeletextAuthoredService>,
    ) -> Result<Self, TeletextTransportError> {
        let content = CarouselContent::authored(service)?;
        let magazine_pending = !content.magazines().is_empty();
        Ok(Self {
            content,
            page: 0,
            unit: 0,
            ticks: 0,
            boundary: true,
            first_cycle: true,
            magazine_pending,
            resource_start_ticks: 0,
            resource_resume: None,
            resource_history: super::service_update::ResourceUpdateHistory::default(),
        })
    }

    /// True before the first header or after the complete cycle's terminator.
    #[must_use]
    pub const fn is_cycle_boundary(&self) -> bool {
        self.boundary
    }

    /// Replace a complete set without mixing pages within a carousel cycle.
    ///
    /// The owner must first confirm the previous batch was actually delivered
    /// and apply its own generation fence. This method neither publishes nor
    /// acknowledges delivery; the programme-relative clock is retained.
    /// # Errors
    /// Returns an error between the opening header and closing terminator.
    pub fn replace_at_boundary(
        &mut self,
        set: EncodedTeletextPageSet,
    ) -> Result<(), TeletextTransportError> {
        if !self.boundary {
            return Err(TeletextTransportError::MidCycleReplacement);
        }
        self.replace_content(CarouselContent::Legacy(set));
        Ok(())
    }

    /// Replace with a complete authored service while retaining the programme clock.
    /// # Errors
    /// Rejects mid-cycle replacement, excessive acquisition cost or stale resource
    /// lifecycle metadata under the strict baseline contract. A service-bound
    /// checked plan permits only its prevalidated headers against this response's
    /// completed history. Cold startup retains the explicit baseline metadata.
    pub fn replace_authored_at_boundary(
        &mut self,
        service: EncodedTeletextAuthoredService,
    ) -> Result<(), TeletextTransportError> {
        self.replace_authored_shared_at_boundary(Arc::new(service))
    }

    /// Replace from the exact shared service without cloning its compiled model.
    /// The owner must acknowledge the old complete cycle and fence its generation.
    /// # Errors
    /// Rejects the same boundary, acquisition and resource-history failures as
    /// the owned replacement. Every failure leaves the carousel unchanged.
    pub fn replace_authored_shared_at_boundary(
        &mut self,
        service: Arc<EncodedTeletextAuthoredService>,
    ) -> Result<(), TeletextTransportError> {
        let prepared = self.prepare_authored_shared_replacement_at_boundary(service)?;
        self.commit_prepared_replacement(prepared);
        Ok(())
    }

    /// Check boundary, acquisition and lifecycle, then build the resource order
    /// without modifying the current carousel. The returned value must be
    /// committed only to this carousel without intervening state mutation.
    pub(super) fn prepare_authored_shared_replacement_at_boundary(
        &self,
        service: Arc<EncodedTeletextAuthoredService>,
    ) -> Result<PreparedCarouselReplacement, TeletextTransportError> {
        if !self.boundary {
            return Err(TeletextTransportError::MidCycleReplacement);
        }
        CarouselContent::validate_authored(&service)?;
        let selected = self.select_resource_headers(&service)?;
        Ok(PreparedCarouselReplacement {
            content: CarouselContent::authored_prevalidated(service, selected),
        })
    }

    /// Commit already checked content after the installer's fallible preflight.
    /// The synchronous caller must retain the same carousel and must not advance
    /// or replace its cursor, content or history between preparation and commit.
    pub(super) fn commit_prepared_replacement(&mut self, prepared: PreparedCarouselReplacement) {
        self.replace_content(prepared.content);
    }

    /// Nonmutating preflight for a synchronous installer. This is not delivery
    /// evidence; state must not advance between this check and replacement.
    #[cfg(test)]
    pub(super) fn validate_shared_replacement_at_boundary(
        &self,
        service: &Arc<EncodedTeletextAuthoredService>,
    ) -> Result<(), TeletextTransportError> {
        if !self.boundary {
            return Err(TeletextTransportError::MidCycleReplacement);
        }
        CarouselContent::validate_authored(service)?;
        self.select_resource_headers(service).map(|_| ())
    }

    fn select_resource_headers(
        &self,
        service: &EncodedTeletextAuthoredService,
    ) -> Result<super::resource_lifecycle::SelectedResourceHeaders, TeletextTransportError> {
        let completed = match &self.content {
            CarouselContent::Authored {
                service,
                selected_headers,
                ..
            } if !self.first_cycle => Some(super::service_update::CompletedResourceService {
                service,
                selected: Some(selected_headers),
            }),
            CarouselContent::Legacy(_) | CarouselContent::Authored { .. } => None,
        };
        if service.resource_lifecycle_variants().is_some() {
            Ok(self.resource_history.select_headers(service, completed))
        } else {
            if completed.as_ref().is_some_and(|completed| {
                completed
                    .selected
                    .is_some_and(|selected| !selected.is_empty())
            }) {
                self.resource_history
                    .validate_with_selected(service, completed)?;
            } else {
                self.resource_history
                    .validate(service, completed.map(|completed| completed.service))?;
            }
            Ok(BTreeMap::default())
        }
    }

    pub(super) fn mot_retirement_counter(
        &self,
        identity: ersatzrs_domain::TeletextResourceIdentity,
    ) -> Option<ersatzrs_domain::TeletextResourceUpdateCounter> {
        if let CarouselContent::Authored {
            service,
            selected_headers,
            ..
        } = &self.content
        {
            if !self.first_cycle {
                if let Some((index, header)) =
                    service
                        .pages()
                        .iter()
                        .enumerate()
                        .find_map(|(index, page)| {
                            page.resource_header()
                                .filter(|header| header.identity() == identity)
                                .map(|header| (index, header))
                        })
                {
                    return Some(
                        selected_headers
                            .get(&index)
                            .map_or(header.update_counter(), |selected| {
                                selected.metadata.update_counter()
                            }),
                    );
                }
            }
        }
        self.resource_history.completed_counter(identity)
    }

    pub(super) fn record_mot_retirement(&mut self, pages: Vec<super::EncodedTeletextResourcePage>) {
        for page in pages {
            self.resource_history
                .record_completed_resource(super::EncodedTeletextAuthoredPage::Resource(page));
        }
    }

    /// Advance withdrawn empty intervals without emitting or resetting content.
    /// Only the programme-relative clock changes after all checks succeed.
    pub(super) fn advance_empty_at_boundary(
        &mut self,
        delta_ticks: u64,
    ) -> Result<(), TeletextTransportError> {
        if !self.boundary {
            return Err(TeletextTransportError::MidCycleReplacement);
        }
        let next = self
            .ticks
            .checked_add(delta_ticks)
            .ok_or(TeletextTransportError::ClockOverflow)?;
        self.ticks = next;
        Ok(())
    }

    fn replace_content(&mut self, content: CarouselContent) {
        self.magazine_pending = !content.magazines().is_empty();
        let previous = std::mem::replace(&mut self.content, content);
        if !self.first_cycle {
            if let CarouselContent::Authored {
                service,
                selected_headers,
                ..
            } = previous
            {
                match Arc::try_unwrap(service) {
                    Ok(service) if selected_headers.is_empty() => {
                        self.resource_history.record_completed(service);
                    }
                    Ok(service) => self
                        .resource_history
                        .record_completed_with_selected(service, &selected_headers),
                    Err(service) if selected_headers.is_empty() => {
                        self.resource_history.record_completed_shared(&service);
                    }
                    Err(service) => self
                        .resource_history
                        .record_completed_shared_with_selected(&service, &selected_headers),
                }
            }
        }
        self.page = 0;
        self.unit = 0;
        self.first_cycle = true;
        self.resource_start_ticks = self.ticks;
        self.resource_resume = None;
    }

    fn cursor(&self) -> CarouselCursor {
        CarouselCursor {
            page: self.page,
            unit: self.unit,
            ticks: self.ticks,
            boundary: self.boundary,
            first_cycle: self.first_cycle,
            magazine_pending: self.magazine_pending,
            resource_start_ticks: self.resource_start_ticks,
            resource_resume: self.resource_resume,
        }
    }

    fn apply_cursor(&mut self, cursor: CarouselCursor) {
        self.page = cursor.page;
        self.unit = cursor.unit;
        self.ticks = cursor.ticks;
        self.boundary = cursor.boundary;
        self.first_cycle = cursor.first_cycle;
        self.magazine_pending = cursor.magazine_pending;
        self.resource_start_ticks = cursor.resource_start_ticks;
        self.resource_resume = cursor.resource_resume;
    }

    /// Predict the first closing FF within a bounded prefix using the same cursor
    /// rules as actual planning. Even an initial boundary scans to the next FF.
    pub(super) fn batches_until_boundary(
        &self,
        maximum: usize,
    ) -> Result<Option<usize>, TeletextTransportError> {
        let mut cursor = self.cursor();
        for count in 0..maximum {
            if matches!(cursor.next_plan(&self.content)?, BatchPlan::Terminator) {
                return Ok(Some(count + 1));
            }
        }
        Ok(None)
    }

    /// Plan the next bounded batch, leaving a 20 ms presentation interval.
    /// # Errors
    /// Clock overflow leaves the cursor and encoded set unchanged.
    pub fn next_batch(&mut self) -> Result<TeletextCarouselBatch, TeletextTransportError> {
        let mut cursor = self.cursor();
        let plan = cursor.next_plan(&self.content)?;
        let (kind, units) = self.units_for_plan(plan);
        let ticks = self.ticks;
        self.apply_cursor(cursor);
        Ok(TeletextCarouselBatch { ticks, kind, units })
    }

    fn units_for_plan(&mut self, plan: BatchPlan) -> (TeletextCarouselBatchKind, Vec<[u8; 46]>) {
        match plan {
            BatchPlan::Magazine => (
                TeletextCarouselBatchKind::Magazine,
                self.content.magazines().to_vec(),
            ),
            BatchPlan::Terminator => {
                if let CarouselContent::Legacy(set) = &self.content {
                    // Only actual planning retires completed legacy display owners;
                    // prediction does not acknowledge or change receiver history.
                    self.resource_history.retire_display_owners(set);
                }
                (
                    TeletextCarouselBatchKind::Terminator,
                    vec![packets::terminator(self.content.initial_magazine())],
                )
            }
            BatchPlan::Header { page, repeat } => {
                if let Some(header) = self.content.selected_header(page, repeat) {
                    return (TeletextCarouselBatchKind::Header, vec![header]);
                }
                let (units, subcode, resource_header) = self.content.page(page);
                let mut header = units[0];
                if repeat {
                    // Clear C4 without disturbing subcode, and resource C8 only.
                    header[9] = packets::hamming_8_4(subcode.to_le_bytes()[0] >> 4).reverse_bits();
                    if resource_header
                        .is_some_and(|metadata| metadata.transmission().update_indicator())
                    {
                        header[12] ^=
                            (packets::hamming_8_4(0) ^ packets::hamming_8_4(2)).reverse_bits();
                    }
                }
                (TeletextCarouselBatchKind::Header, vec![header])
            }
            BatchPlan::Page { page, start, end } => (
                TeletextCarouselBatchKind::Page,
                self.content.page(page).0[start..end].to_vec(),
            ),
        }
    }
}

#[derive(Clone, Copy)]
struct CarouselCursor {
    page: usize,
    unit: usize,
    ticks: u64,
    boundary: bool,
    first_cycle: bool,
    magazine_pending: bool,
    resource_start_ticks: u64,
    resource_resume: Option<usize>,
}

#[derive(Clone, Copy)]
enum BatchPlan {
    Magazine,
    Terminator,
    Header {
        page: usize,
        repeat: bool,
    },
    Page {
        page: usize,
        start: usize,
        end: usize,
    },
}

impl CarouselCursor {
    fn prepare_acquisition(&mut self, content: &CarouselContent) {
        if !content.has_acquisition() {
            return;
        }
        if !self.magazine_pending
            && self.unit == 0
            && self.page >= content.resource_count()
            && self.page < content.page_count()
        {
            let elapsed = (self.ticks - self.resource_start_ticks) / STEP_TICKS;
            // Reserve the sole closing FF as well as the next whole display.
            // Preflight guarantees a complete sweep plus any display can fit.
            let following = page_batches(content.page(self.page).0) + 1 + RESOURCE_WINDOW_MARGIN;
            if elapsed + following as u64 > MAX_RESOURCE_CYCLE_BATCHES as u64 {
                self.resource_resume = Some(self.page);
                self.page = 0;
                self.magazine_pending = !content.magazines().is_empty();
            }
        }
        if self.magazine_pending
            || (content.magazines().is_empty() && self.page == 0 && self.unit == 0)
        {
            self.resource_start_ticks = self.ticks;
        }
    }

    fn resume_display(&mut self, content: &CarouselContent) {
        if self.page == content.resource_count() {
            if let Some(resume) = self.resource_resume.take() {
                self.page = resume;
            }
        }
    }

    fn next_plan(
        &mut self,
        content: &CarouselContent,
    ) -> Result<BatchPlan, TeletextTransportError> {
        let next_ticks = self
            .ticks
            .checked_add(STEP_TICKS)
            .ok_or(TeletextTransportError::ClockOverflow)?;
        self.prepare_acquisition(content);
        let plan = if self.magazine_pending {
            self.magazine_pending = false;
            self.boundary = false;
            self.resume_display(content);
            BatchPlan::Magazine
        } else if self.page == content.page_count() {
            self.page = 0;
            self.unit = 0;
            self.boundary = true;
            self.first_cycle = false;
            self.magazine_pending = !content.magazines().is_empty();
            BatchPlan::Terminator
        } else {
            let (units, _, resource_header) = content.page(self.page);
            self.boundary = false;
            if self.unit == 0 {
                self.unit = 1;
                BatchPlan::Header {
                    page: self.page,
                    repeat: !self.first_cycle
                        || (resource_header.is_some() && self.resource_resume.is_some()),
                }
            } else {
                let page = self.page;
                let start = self.unit;
                let end = (self.unit + MAX_BATCH_UNITS).min(units.len());
                self.unit = end;
                if end == units.len() {
                    self.page += 1;
                    self.unit = 0;
                    self.resume_display(content);
                }
                BatchPlan::Page { page, start, end }
            }
        };
        self.ticks = next_ticks;
        Ok(plan)
    }
}

#[cfg(test)]
#[path = "authored_service_carousel_tests.rs"]
pub(super) mod authored_service_carousel_tests;

#[cfg(test)]
#[path = "authored_service_interleave_tests.rs"]
mod authored_service_interleave_tests;

#[cfg(test)]
#[path = "authored_service_update_tests.rs"]
mod authored_service_update_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::teletext::transport_tests::{draft, set};

    #[test]
    fn repeated_header_planning_preserves_the_carousels_exact_encoded_page_set() {
        let encoded = set(vec![draft(200, 0, false), draft(200, 0x3f7e, false)]);
        let original = encoded.clone();
        let mut carousel = TeletextCarousel::new(encoded);
        for _ in 0..4 {
            loop {
                let batch = carousel.next_batch().unwrap();
                let CarouselContent::Legacy(set) = &carousel.content else {
                    panic!("legacy content");
                };
                assert_eq!(set, &original);
                if batch.kind() == TeletextCarouselBatchKind::Terminator {
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "authored_service_shared_replacement_tests.rs"]
mod authored_service_shared_replacement_tests;
