//! Immutable original-attribute execution values; no renderer/readiness authority.

use super::{
    TeletextBaseAttributeSource, TeletextColourReference, TeletextEnhancementSource,
    TeletextFlashMode, TeletextFlashRate, TeletextSpatialInvocationId, TeletextSpatialRootSource,
};

/// Attribute origin; logical event ordinal is never a physical transmission time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextAttributeSource {
    Base {
        position: (u16, u16),
        source: TeletextBaseAttributeSource,
    },
    PanelDefault {
        row: u8,
    },
    PassiveDefault {
        invocation: TeletextSpatialInvocationId,
    },
    Enhancement {
        event_ordinal: usize,
        location: TeletextEnhancementSource,
        root_source: TeletextSpatialRootSource,
        invocation: Option<TeletextSpatialInvocationId>,
        effective_start: (u16, u16),
        placed_position: (u16, u16),
    },
}
impl TeletextAttributeSource {
    pub(super) fn key(self) -> Option<((u16, u16), (u16, u16))> {
        match self {
            Self::Base {
                source: source @ TeletextBaseAttributeSource::Spacing { position, .. },
                ..
            } => Some((
                (
                    u16::from(position.row()),
                    u16::from(source.effective_start_column()),
                ),
                (u16::from(position.row()), u16::from(position.column())),
            )),
            Self::Enhancement {
                effective_start,
                placed_position,
                ..
            } => Some((effective_start, placed_position)),
            _ => None,
        }
    }
}

/// Typed known value or an index into the immutable pending diagnostic chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextExecutedAttribute<T> {
    pub(super) value: Option<T>,
    pub(super) source: TeletextAttributeSource,
    pub(super) pending: Option<usize>,
}
impl<T: Copy> TeletextExecutedAttribute<T> {
    /// None is honest execution uncertainty, not a default or guessed winner.
    #[must_use]
    pub const fn value(self) -> Option<T> {
        self.value
    }
    /// A known origin; pending contenders remain in the execution's pending log.
    #[must_use]
    pub const fn source(self) -> Option<TeletextAttributeSource> {
        if self.pending.is_none() {
            Some(self.source)
        } else {
            None
        }
    }
    #[must_use]
    pub const fn pending(self) -> Option<usize> {
        self.pending
    }
    pub(super) const fn known(value: T, source: TeletextAttributeSource) -> Self {
        Self {
            value: Some(value),
            source,
            pending: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextExecutedFlash {
    pub(super) mode: TeletextFlashMode,
    pub(super) rate: TeletextFlashRate,
}
impl TeletextExecutedFlash {
    #[must_use]
    pub const fn mode(self) -> TeletextFlashMode {
        self.mode
    }
    #[must_use]
    pub const fn rate(self) -> TeletextFlashRate {
        self.rate
    }
}

/// All four groups, retaining each Display Attributes field's own origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextExecutedAttributes {
    pub(super) foreground: TeletextExecutedAttribute<TeletextColourReference>,
    pub(super) background: TeletextExecutedAttribute<TeletextColourReference>,
    pub(super) flash: TeletextExecutedAttribute<TeletextExecutedFlash>,
    pub(super) double_height: TeletextExecutedAttribute<bool>,
    pub(super) double_width: TeletextExecutedAttribute<bool>,
    pub(super) boxed: TeletextExecutedAttribute<bool>,
    pub(super) conceal: TeletextExecutedAttribute<bool>,
    pub(super) invert: TeletextExecutedAttribute<bool>,
    pub(super) underline_or_separated_mosaics: TeletextExecutedAttribute<bool>,
    pub(super) spacing_separated: TeletextExecutedAttribute<bool>,
}
impl TeletextExecutedAttributes {
    #[must_use]
    pub const fn foreground(self) -> TeletextExecutedAttribute<TeletextColourReference> {
        self.foreground
    }
    #[must_use]
    pub const fn background(self) -> TeletextExecutedAttribute<TeletextColourReference> {
        self.background
    }
    #[must_use]
    pub const fn flash(self) -> TeletextExecutedAttribute<TeletextExecutedFlash> {
        self.flash
    }
    #[must_use]
    pub const fn double_height(self) -> TeletextExecutedAttribute<bool> {
        self.double_height
    }
    #[must_use]
    pub const fn double_width(self) -> TeletextExecutedAttribute<bool> {
        self.double_width
    }
    #[must_use]
    pub const fn boxed(self) -> TeletextExecutedAttribute<bool> {
        self.boxed
    }
    #[must_use]
    pub const fn conceal(self) -> TeletextExecutedAttribute<bool> {
        self.conceal
    }
    #[must_use]
    pub const fn invert(self) -> TeletextExecutedAttribute<bool> {
        self.invert
    }
    #[must_use]
    pub const fn underline_or_separated_mosaics(self) -> TeletextExecutedAttribute<bool> {
        self.underline_or_separated_mosaics
    }
    #[must_use]
    pub const fn spacing_separated(self) -> TeletextExecutedAttribute<bool> {
        self.spacing_separated
    }
    /// Base/Level 1 mosaic separation only; enhanced G1 uses D5 alone.
    /// Nonspacing D5 is separate from Level 1 spacing separation. Contiguous
    /// spacing cannot clear true D5; false D5 clears earlier spacing separation.
    #[must_use]
    pub fn separated_mosaics(self) -> Option<bool> {
        let latch = self.underline_or_separated_mosaics.value?;
        let spacing = self.spacing_separated.value?;
        if latch {
            Some(true)
        } else if self.underline_or_separated_mosaics.source.key().is_some()
            && self.spacing_separated.source.key()
                <= self.underline_or_separated_mosaics.source.key()
        {
            Some(false)
        } else {
            Some(spacing)
        }
    }
    /// Derived inversion only; logical colours and both origins remain unchanged.
    #[must_use]
    pub fn rendered_colours(self) -> Option<(TeletextColourReference, TeletextColourReference)> {
        let pair = (self.foreground.value?, self.background.value?);
        Some(if self.invert.value? {
            (pair.1, pair.0)
        } else {
            pair
        })
    }
}

/// Uncertainty is retained rather than being resolved with logical event order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextAttributePendingReason {
    TransmissionOrder,
    AdaptiveExtent,
    Unaddressable,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextAttributePending {
    pub(super) position: (u16, u16),
    pub(super) location: TeletextEnhancementSource,
    pub(super) root_source: TeletextSpatialRootSource,
    pub(super) invocation: Option<TeletextSpatialInvocationId>,
    pub(super) reason: TeletextAttributePendingReason,
    pub(super) first_source: Option<TeletextAttributeSource>,
    pub(super) second_source: Option<TeletextAttributeSource>,
    pub(super) previous: Option<usize>,
    pub(super) incoming_previous: Option<usize>,
}
impl TeletextAttributePending {
    #[must_use]
    pub const fn position(self) -> (u16, u16) {
        self.position
    }
    #[must_use]
    pub const fn location(self) -> TeletextEnhancementSource {
        self.location
    }
    #[must_use]
    pub const fn root_source(self) -> TeletextSpatialRootSource {
        self.root_source
    }
    #[must_use]
    pub const fn invocation(self) -> Option<TeletextSpatialInvocationId> {
        self.invocation
    }
    #[must_use]
    pub const fn reason(self) -> TeletextAttributePendingReason {
        self.reason
    }
    #[must_use]
    pub const fn first_source(self) -> Option<TeletextAttributeSource> {
        self.first_source
    }
    #[must_use]
    pub const fn second_source(self) -> Option<TeletextAttributeSource> {
        self.second_source
    }
    /// Earlier ambiguity record, retaining every original contender without per-cell Vecs.
    #[must_use]
    pub const fn previous(self) -> Option<usize> {
        self.previous
    }
    /// Incoming uncertainty chain when two already-pending contenders meet.
    #[must_use]
    pub const fn incoming_previous(self) -> Option<usize> {
        self.incoming_previous
    }
}
