//! Immutable display placement values; no raster or publication authority.

use super::{
    TeletextGlyphCandidate, TeletextObjectOpenCount, TeletextObjectSuccessiveCount,
    TeletextPageAddress, TeletextSidePanels, TeletextSpatialEvent, TeletextSpatialInvocation,
    TeletextSpatialInvocationId,
};

/// Raw address areas in EN 300 706 Annex A.4 scan order.
/// This differs from the visual order: left panel, main, right panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextRawDisplayArea {
    Main,
    RightPanel,
    LeftPanel,
    Outside,
}

/// Why a retained raw contender has no selected visual cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextCandidateNonplacement {
    HeaderOwned,
    HeaderPanel,
    DisabledPanel,
    OutsideActiveWidth,
    OutsideDisplay,
}

/// Point placement only; size footprints, overlap and visibility are separate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextCandidatePlacement {
    Placed { row: u8, column: u8 },
    Unplaced(TeletextCandidateNonplacement),
}

/// One unchanged original candidate, including losing/out-of-display contenders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextPlacedGlyphCandidate {
    pub(super) candidate_index: usize,
    pub(super) candidate: TeletextGlyphCandidate,
    pub(super) area: TeletextRawDisplayArea,
    pub(super) placement: TeletextCandidatePlacement,
}

impl TeletextPlacedGlyphCandidate {
    #[must_use]
    pub const fn candidate_index(&self) -> usize {
        self.candidate_index
    }
    #[must_use]
    pub const fn candidate(&self) -> &TeletextGlyphCandidate {
        &self.candidate
    }
    #[must_use]
    pub const fn area(&self) -> TeletextRawDisplayArea {
        self.area
    }
    #[must_use]
    pub const fn placement(&self) -> TeletextCandidatePlacement {
        self.placement
    }
}

/// Shared geometry model for the exact original display, retaining raw metadata.
/// Point placement alone does not validate occupied footprints, object overlap,
/// final attributes, row-zero normalization or publication readiness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextDisplayGeometry {
    pub(super) owner: TeletextPageAddress,
    pub(super) panels: Option<TeletextSidePanels>,
    pub(super) left_width: u8,
    pub(super) right_width: u8,
    pub(super) records: Vec<TeletextPlacedGlyphCandidate>,
    pub(super) object_regions: Vec<TeletextObjectDisplayRegion>,
    pub(super) addresses: Vec<TeletextObjectGeometryFact>,
    pub(super) writes: Vec<TeletextObjectGeometryFact>,
    pub(super) object_open_count: TeletextObjectOpenCount,
    pub(super) successive_open_count: TeletextObjectSuccessiveCount,
}

impl TeletextDisplayGeometry {
    #[must_use]
    pub const fn owner(&self) -> TeletextPageAddress {
        self.owner
    }
    /// Original flags/status/width, distinct from effective Level 2.5 widths.
    #[must_use]
    pub const fn panels(&self) -> Option<TeletextSidePanels> {
        self.panels
    }
    #[must_use]
    pub const fn left_width(&self) -> u8 {
        self.left_width
    }
    #[must_use]
    pub const fn right_width(&self) -> u8 {
        self.right_width
    }
    #[must_use]
    pub const fn total_width(&self) -> u8 {
        self.left_width + 40 + self.right_width
    }
    /// Exact candidate order/cardinality from the supplied original layers.
    #[must_use]
    pub fn records(&self) -> &[TeletextPlacedGlyphCandidate] {
        &self.records
    }
    /// Distinct actual instances, never collapsed by full semantic object ID.
    #[must_use]
    pub fn object_regions(&self) -> &[TeletextObjectDisplayRegion] {
        &self.object_regions
    }
    /// Inclusive known-open lower bound across ALL object types. Pending
    /// intervals and uncertain successive opens/A.7/C.3 still require final
    /// validation.
    #[must_use]
    pub const fn object_open_count(&self) -> &TeletextObjectOpenCount {
        &self.object_open_count
    }
    /// Known positioned successive runs; pending resets and cross-emitter
    /// ordering remain distinct from complete A.8/page admission.
    #[must_use]
    pub const fn successive_open_count(&self) -> &TeletextObjectSuccessiveCount {
        &self.successive_open_count
    }
    /// Direct own addressing events only, in original command order; child
    /// endpoints and written regions are never folded into the caller.
    #[must_use]
    pub fn direct_addresses(
        &self,
        id: TeletextSpatialInvocationId,
    ) -> &[TeletextObjectGeometryFact] {
        self.object_regions.get(id.get()).map_or(&[], |region| {
            &self.addresses[region.address_start..region.address_finish]
        })
    }
    /// Definite own glyph/four-attribute-group operations, retaining even events
    /// without selected visual placement. Cursor/charset/plane writes differ.
    #[must_use]
    pub fn direct_writes(&self, id: TeletextSpatialInvocationId) -> &[TeletextObjectGeometryFact] {
        self.object_regions.get(id.get()).map_or(&[], |region| {
            &self.writes[region.write_start..region.write_finish]
        })
    }
}

/// Exact original own-instance fact, not a raster-cell or bounding rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextObjectGeometryFact {
    pub(super) event_ordinal: usize,
    pub(super) event: TeletextSpatialEvent,
}
impl TeletextObjectGeometryFact {
    #[must_use]
    pub const fn event_ordinal(&self) -> usize {
        self.event_ordinal
    }
    #[must_use]
    pub const fn event(&self) -> &TeletextSpatialEvent {
        &self.event
    }
}

/// Address-only or unplaced evidence cannot certify an occupied written area.
/// Exact underlying facts remain available even when more than one issue exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextObjectAreaPending {
    NoDefiniteWrites,
    AddressCrossesAreas,
    UnplacedAddress,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextObjectIntervalPending {
    NoDirectAddress,
    OutsideDisplay,
    EndBeforeStart,
}

/// Inclusive Annex A.8 RAW scan interval, not a visual rectangle or proof of
/// overlap admissibility. The start is caller AP BEFORE invocation offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextObjectAddressInterval {
    Known {
        start: (u16, u16),
        finish: (u16, u16),
    },
    Pending {
        reason: TeletextObjectIntervalPending,
        endpoint: Option<(u16, u16)>,
    },
}

/// Direct own-instance topology after raw A.4 addressed-area admission.
/// A.7 addressed-cell admission, complete A.8 interval admission and C.3
/// occupied footprints/final attributes remain distinct validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextObjectDisplayRegion {
    pub(super) instance: TeletextSpatialInvocation,
    pub(super) written_area: Option<TeletextRawDisplayArea>,
    pub(super) area_pending: Option<TeletextObjectAreaPending>,
    pub(super) interval: TeletextObjectAddressInterval,
    pub(super) address_start: usize,
    pub(super) address_finish: usize,
    pub(super) write_start: usize,
    pub(super) write_finish: usize,
}
impl TeletextObjectDisplayRegion {
    /// Full target/type, origin, caller AP, source/root and actual instance ID.
    #[must_use]
    pub const fn instance(&self) -> &TeletextSpatialInvocation {
        &self.instance
    }
    /// Area proved by definite placed own writes only, not setup or descendants.
    /// This metadata is separate from A.4 admission, which checks all original
    /// own AP-addressing cells, including charset/setup and inactive panels.
    #[must_use]
    pub const fn written_area(&self) -> Option<TeletextRawDisplayArea> {
        self.written_area
    }
    #[must_use]
    pub const fn area_pending(&self) -> Option<TeletextObjectAreaPending> {
        self.area_pending
    }
    /// Endpoint uses the last explicit direct addressing fact, even if the
    /// instance has no glyph or attribute write; children cannot supply it.
    #[must_use]
    pub const fn interval(&self) -> TeletextObjectAddressInterval {
        self.interval
    }
}
