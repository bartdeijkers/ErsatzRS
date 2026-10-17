//! Compact original display selections and C.3 provenance; no raster authority.

use super::{
    TeletextAttributeSource, TeletextColourReference, TeletextEnhancementSource,
    TeletextExecutedFlash, TeletextSpatialInvocationId, TeletextSpatialRootSource,
};

pub(super) const FIELDS: usize = 10;
pub(super) const FG: usize = 0;
pub(super) const BG: usize = 1;
pub(super) const FLASH: usize = 2;
pub(super) const DH: usize = 3;
pub(super) const DW: usize = 4;
pub(super) const BOXED: usize = 5;
pub(super) const CONCEAL: usize = 6;
pub(super) const INVERT: usize = 7;
pub(super) const D5: usize = 8;
pub(super) const SPACING: usize = 9;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextDisplaySize {
    Normal,
    DoubleWidth,
    DoubleHeight,
    DoubleSize,
}
impl TeletextDisplaySize {
    pub(super) const fn dimensions(self) -> (usize, usize) {
        match self {
            Self::Normal => (1, 1),
            Self::DoubleWidth => (2, 1),
            Self::DoubleHeight => (1, 2),
            Self::DoubleSize => (2, 2),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextDisplayFragment {
    Normal,
    DoubleWidthLeft,
    DoubleWidthRight,
    DoubleHeightTop,
    DoubleHeightBottom,
    DoubleSizeTopLeft,
    DoubleSizeTopRight,
    DoubleSizeBottomLeft,
    DoubleSizeBottomRight,
    BackgroundOnly,
    Pending,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextDisplayPendingReason {
    TransmissionOrder,
    OriginalAttribute { pending: usize },
    IllegalSizeBoundary,
    ConflictingSizeAxes,
    InheritedAdaptiveNonOrigin,
    UnknownSize,
    GlyphAssignment,
    Unaddressable,
    AdaptiveExtent,
    ObjectAddressEvidence,
    BaseHeightBoundary,
}
/// Original source is retained; effective start can move after obscuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextDisplayWriter {
    pub(super) source: TeletextAttributeSource,
    pub(super) requested: (u16, u16),
    pub(super) effective: (u16, u16),
    pub(super) placed: (u16, u16),
    pub(super) values: [Option<Value>; FIELDS],
    pub(super) explicit: bool,
}
impl TeletextDisplayWriter {
    #[must_use]
    pub const fn source(&self) -> TeletextAttributeSource {
        self.source
    }
    #[must_use]
    pub const fn requested_start(&self) -> (u16, u16) {
        self.requested
    }
    #[must_use]
    pub const fn effective_start(&self) -> (u16, u16) {
        self.effective
    }
    #[must_use]
    pub const fn placed_position(&self) -> (u16, u16) {
        self.placed
    }
    /// Compare every writer payload field while excluding only source identity.
    /// Callers must independently authenticate any source correspondence. This
    /// preserves hidden values and explicitness as well as visible coordinates.
    #[must_use]
    pub fn same_payload(&self, other: &Self) -> bool {
        Self {
            source: other.source,
            ..*self
        } == *other
    }

    pub(super) const fn key(&self) -> Option<((u16, u16), (u16, u16))> {
        if self.explicit {
            Some((self.effective, self.placed))
        } else {
            None
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextDisplayPending {
    pub(super) candidate: Option<usize>,
    pub(super) position: (u16, u16),
    pub(super) location: Option<TeletextEnhancementSource>,
    pub(super) root_source: Option<TeletextSpatialRootSource>,
    pub(super) invocation: Option<TeletextSpatialInvocationId>,
    pub(super) reason: TeletextDisplayPendingReason,
    pub(super) first: Option<usize>,
    pub(super) second: Option<usize>,
    pub(super) previous: Option<usize>,
    pub(super) incoming_previous: Option<usize>,
}
impl TeletextDisplayPending {
    #[must_use]
    pub const fn candidate_index(&self) -> Option<usize> {
        self.candidate
    }
    #[must_use]
    pub const fn position(&self) -> (u16, u16) {
        self.position
    }
    #[must_use]
    pub const fn location(&self) -> Option<TeletextEnhancementSource> {
        self.location
    }
    #[must_use]
    pub const fn root_source(&self) -> Option<TeletextSpatialRootSource> {
        self.root_source
    }
    #[must_use]
    pub const fn invocation(&self) -> Option<TeletextSpatialInvocationId> {
        self.invocation
    }
    #[must_use]
    pub const fn reason(&self) -> TeletextDisplayPendingReason {
        self.reason
    }
    #[must_use]
    pub const fn first_writer(&self) -> Option<usize> {
        self.first
    }
    #[must_use]
    pub const fn second_writer(&self) -> Option<usize> {
        self.second
    }
    #[must_use]
    pub const fn previous(&self) -> Option<usize> {
        self.previous
    }
    #[must_use]
    pub const fn incoming_previous(&self) -> Option<usize> {
        self.incoming_previous
    }
}
/// The prefix contains one record per original candidate, including losers.
/// Later records re-realize the same candidate under an Adaptive writer layer;
/// earlier visible parts keep their own origin and attributes.
/// Requested attribute flags remain original intent. Occupancy uses only the
/// effective size/fragment, including a reduction of one conflicting axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextDisplayOrigin {
    pub(super) candidate: usize,
    pub(super) visual: Option<(u8, u8)>,
    pub(super) requested: Option<TeletextDisplaySize>,
    pub(super) effective: Option<TeletextDisplaySize>,
    pub(super) attributes: usize,
    pub(super) pending: Option<usize>,
    pub(super) obscured_by: Option<usize>,
}
impl TeletextDisplayOrigin {
    #[must_use]
    pub const fn candidate_index(&self) -> usize {
        self.candidate
    }
    #[must_use]
    pub const fn visual_position(&self) -> Option<(u8, u8)> {
        self.visual
    }
    #[must_use]
    pub const fn requested_size(&self) -> Option<TeletextDisplaySize> {
        self.requested
    }
    #[must_use]
    pub const fn effective_size(&self) -> Option<TeletextDisplaySize> {
        self.effective
    }
    #[must_use]
    pub const fn pending(&self) -> Option<usize> {
        self.pending
    }
    #[must_use]
    pub const fn obscured_by(&self) -> Option<usize> {
        self.obscured_by
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Value {
    Colour(TeletextColourReference),
    Flash(TeletextExecutedFlash),
    Flag(bool),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Selection {
    pub writer: usize,
    pub pending: Option<usize>,
}
pub(super) type State = [Selection; FIELDS];
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Cell {
    pub origin: Option<usize>,
    pub attributes: usize,
    pub fragment: TeletextDisplayFragment,
}
/// Compact cells refer to shared immutable attribute/origin/source pools.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextDisplayResolution {
    pub(super) owner: super::TeletextPageAddress,
    pub(super) width: u8,
    pub(super) cells: Vec<Option<Cell>>,
    pub(super) states: Vec<State>,
    pub(super) origins: Vec<TeletextDisplayOrigin>,
    pub(super) writers: Vec<TeletextDisplayWriter>,
    pub(super) pending: Vec<TeletextDisplayPending>,
}
impl TeletextDisplayResolution {
    #[must_use]
    pub const fn owner(&self) -> super::TeletextPageAddress {
        self.owner
    }
    #[must_use]
    pub const fn width(&self) -> u8 {
        self.width
    }
    #[must_use]
    pub fn cell(&self, row: u8, column: u8) -> Option<TeletextDisplayCell<'_>> {
        if row >= 25 || column >= self.width {
            return None;
        }
        self.cells[usize::from(row) * usize::from(self.width) + usize::from(column)].map(|cell| {
            TeletextDisplayCell {
                resolution: self,
                cell,
            }
        })
    }
    #[must_use]
    pub fn origins(&self) -> &[TeletextDisplayOrigin] {
        &self.origins
    }
    #[must_use]
    pub fn writers(&self) -> &[TeletextDisplayWriter] {
        &self.writers
    }
    #[must_use]
    pub fn pending(&self) -> &[TeletextDisplayPending] {
        &self.pending
    }
    #[must_use]
    pub fn origin_attributes(&self, index: usize) -> Option<TeletextDisplayAttributes<'_>> {
        self.origins
            .get(index)
            .map(|origin| TeletextDisplayAttributes {
                resolution: self,
                index: origin.attributes,
            })
    }
}
#[derive(Debug, Clone, Copy)]
pub struct TeletextDisplayCell<'a> {
    resolution: &'a TeletextDisplayResolution,
    cell: Cell,
}
impl<'a> TeletextDisplayCell<'a> {
    #[must_use]
    pub const fn fragment(self) -> TeletextDisplayFragment {
        self.cell.fragment
    }
    #[must_use]
    pub const fn origin(self) -> Option<usize> {
        self.cell.origin
    }
    #[must_use]
    pub const fn attributes(self) -> TeletextDisplayAttributes<'a> {
        TeletextDisplayAttributes {
            resolution: self.resolution,
            index: self.cell.attributes,
        }
    }
}
#[derive(Debug, Clone, Copy)]
pub struct TeletextDisplayAttributes<'a> {
    resolution: &'a TeletextDisplayResolution,
    index: usize,
}
impl<'a> TeletextDisplayAttributes<'a> {
    fn selection(self, field: usize) -> Selection {
        self.resolution.states[self.index][field]
    }
    fn colour(self, field: usize) -> TeletextDisplayAttribute<'a, TeletextColourReference> {
        let selection = self.selection(field);
        let value = match self.resolution.writers[selection.writer].values[field] {
            Some(Value::Colour(c)) if selection.pending.is_none() => Some(c),
            _ => None,
        };
        TeletextDisplayAttribute {
            resolution: self.resolution,
            selection,
            value,
        }
    }
    fn flag(self, field: usize) -> TeletextDisplayAttribute<'a, bool> {
        let selection = self.selection(field);
        let value = match self.resolution.writers[selection.writer].values[field] {
            Some(Value::Flag(c)) if selection.pending.is_none() => Some(c),
            _ => None,
        };
        TeletextDisplayAttribute {
            resolution: self.resolution,
            selection,
            value,
        }
    }
    #[must_use]
    pub fn foreground(self) -> TeletextDisplayAttribute<'a, TeletextColourReference> {
        self.colour(FG)
    }
    #[must_use]
    pub fn background(self) -> TeletextDisplayAttribute<'a, TeletextColourReference> {
        self.colour(BG)
    }
    #[must_use]
    pub fn flash(self) -> TeletextDisplayAttribute<'a, TeletextExecutedFlash> {
        let selection = self.selection(FLASH);
        let value = match self.resolution.writers[selection.writer].values[FLASH] {
            Some(Value::Flash(c)) if selection.pending.is_none() => Some(c),
            _ => None,
        };
        TeletextDisplayAttribute {
            resolution: self.resolution,
            selection,
            value,
        }
    }
    #[must_use]
    pub fn double_height(self) -> TeletextDisplayAttribute<'a, bool> {
        self.flag(DH)
    }
    #[must_use]
    pub fn double_width(self) -> TeletextDisplayAttribute<'a, bool> {
        self.flag(DW)
    }
    #[must_use]
    pub fn boxed(self) -> TeletextDisplayAttribute<'a, bool> {
        self.flag(BOXED)
    }
    #[must_use]
    pub fn conceal(self) -> TeletextDisplayAttribute<'a, bool> {
        self.flag(CONCEAL)
    }
    #[must_use]
    pub fn invert(self) -> TeletextDisplayAttribute<'a, bool> {
        self.flag(INVERT)
    }
    #[must_use]
    pub fn underline_or_separated_mosaics(self) -> TeletextDisplayAttribute<'a, bool> {
        self.flag(D5)
    }
    #[must_use]
    pub fn spacing_separated(self) -> TeletextDisplayAttribute<'a, bool> {
        self.flag(SPACING)
    }
    #[must_use]
    pub fn rendered_colours(self) -> Option<(TeletextColourReference, TeletextColourReference)> {
        let (fg, bg) = (self.foreground().value()?, self.background().value()?);
        Some(if self.invert().value()? {
            (bg, fg)
        } else {
            (fg, bg)
        })
    }
}
#[derive(Debug, Clone, Copy)]
pub struct TeletextDisplayAttribute<'a, T> {
    resolution: &'a TeletextDisplayResolution,
    selection: Selection,
    value: Option<T>,
}
impl<'a, T: Copy> TeletextDisplayAttribute<'a, T> {
    #[must_use]
    pub const fn value(self) -> Option<T> {
        self.value
    }
    #[must_use]
    pub fn writer(self) -> Option<&'a TeletextDisplayWriter> {
        self.selection
            .pending
            .is_none()
            .then(|| &self.resolution.writers[self.selection.writer])
    }
    #[must_use]
    pub fn pending(self) -> Option<&'a TeletextDisplayPending> {
        self.selection
            .pending
            .map(|index| &self.resolution.pending[index])
    }
}

#[cfg(test)]
#[path = "display_writer_payload_tests.rs"]
mod writer_payload_tests;
