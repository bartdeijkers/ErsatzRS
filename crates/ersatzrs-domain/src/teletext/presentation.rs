//! Authored display defaults, EN 300 706 tables 4/11 and clause 12.5 (ADR 0123).
//!
//! A complete declaration is shared by page X/28/0 and magazine M/29/0.
//! Packet absence and precedence belong to the containing page/service model.
//! These values do not resolve object placement or establish rendering readiness.

use std::{fmt, marker::PhantomData};

use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Serialize};

use super::{TeletextColourReference, TeletextLevel25Palette, TeletextPageCharsetDesignation};

/// Invalid presentation scalar, preserving the supplied value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextPresentationValueError {
    /// The raw width field is four bits; zero has context-dependent meaning.
    #[error("teletext side-panel width {width} must be between zero and 15")]
    PanelWidth { width: u8 },
}

/// Eight encodable foreground/background spacing-colour remappings from table 4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", try_from = "String")]
pub enum TeletextColourRemapping {
    FgClut0BgClut0,
    FgClut0BgClut1,
    FgClut0BgClut2,
    FgClut1BgClut1,
    FgClut1BgClut2,
    FgClut2BgClut1,
    FgClut2BgClut2,
    FgClut2BgClut3,
}

impl TryFrom<String> for TeletextColourRemapping {
    type Error = &'static str;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "fg_clut0_bg_clut0" => Ok(Self::FgClut0BgClut0),
            "fg_clut0_bg_clut1" => Ok(Self::FgClut0BgClut1),
            "fg_clut0_bg_clut2" => Ok(Self::FgClut0BgClut2),
            "fg_clut1_bg_clut1" => Ok(Self::FgClut1BgClut1),
            "fg_clut1_bg_clut2" => Ok(Self::FgClut1BgClut2),
            "fg_clut2_bg_clut1" => Ok(Self::FgClut2BgClut1),
            "fg_clut2_bg_clut2" => Ok(Self::FgClut2BgClut2),
            "fg_clut2_bg_clut3" => Ok(Self::FgClut2BgClut3),
            _ => Err("teletext spacing-colour remapping is not assigned"),
        }
    }
}

impl TeletextColourRemapping {
    /// Foreground and background CLUT numbers, in that order.
    #[must_use]
    pub const fn cluts(self) -> (u8, u8) {
        match self {
            Self::FgClut0BgClut0 => (0, 0),
            Self::FgClut0BgClut1 => (0, 1),
            Self::FgClut0BgClut2 => (0, 2),
            Self::FgClut1BgClut1 => (1, 1),
            Self::FgClut1BgClut2 => (1, 2),
            Self::FgClut2BgClut1 => (2, 1),
            Self::FgClut2BgClut2 => (2, 2),
            Self::FgClut2BgClut3 => (2, 3),
        }
    }
}

/// Preserved four-bit panel-width metadata, not a resolved display width.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct TeletextPanelWidth(u8);

impl TeletextPanelWidth {
    /// Eight-column field for balanced left and right side panels.
    pub const EIGHT: Self = Self(8);
    /// Preserved zero width field; its meaning still depends on the panel flags.
    pub const ZERO: Self = Self(0);

    /// Check the raw field without canonicalizing zero or disabled panels.
    /// # Errors
    /// Returns `PanelWidth` when the input exceeds 15.
    pub fn new(width: u8) -> Result<Self, TeletextPresentationValueError> {
        if width <= 15 {
            Ok(Self(width))
        } else {
            Err(TeletextPresentationValueError::PanelWidth { width })
        }
    }

    /// Original raw four-bit field.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for TeletextPanelWidth {
    type Error = TeletextPresentationValueError;

    fn try_from(width: u8) -> Result<Self, Self::Error> {
        Self::new(width)
    }
}

impl From<TeletextPanelWidth> for u8 {
    fn from(width: TeletextPanelWidth) -> Self {
        width.get()
    }
}

/// Which enhanced presentation levels the panel metadata addresses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", try_from = "String")]
pub enum TeletextPanelStatus {
    /// Valid metadata whose panels are suppressed at Level 2.5.
    Level35Only,
    /// Panels apply at both enhanced levels.
    Level25And35,
}

impl TryFrom<String> for TeletextPanelStatus {
    type Error = &'static str;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "level35_only" => Ok(Self::Level35Only),
            "level25_and35" => Ok(Self::Level25And35),
            _ => Err("teletext side-panel level status is not assigned"),
        }
    }
}

/// Explicit panel flags and raw width, including currently unused metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TeletextSidePanels {
    /// Whether a left panel is requested at the selected level.
    pub left: bool,
    /// Whether a right panel is requested at the selected level.
    pub right: bool,
    /// Level applicability, retained even when both panels are disabled.
    pub status: TeletextPanelStatus,
    /// Raw table 4 width; interpretation depends on both flags.
    pub width: TeletextPanelWidth,
}

impl TeletextSidePanels {
    /// Resolve left/right widths at Level 2.5 without altering the raw metadata.
    #[must_use]
    pub const fn level25_widths(self) -> (u8, u8) {
        match self.status {
            TeletextPanelStatus::Level35Only => (0, 0),
            TeletextPanelStatus::Level25And35 => self.level35_widths(),
        }
    }

    /// Resolve table 4 widths when panels apply; this promises no Level 3.5 engine.
    #[must_use]
    pub const fn level35_widths(self) -> (u8, u8) {
        let width = self.width.get();
        let left = if !self.left {
            0
        } else if width == 0 && !self.right {
            16
        } else {
            width
        };
        let right = if self.right { 16 - width } else { 0 };
        (left, right)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PanelFields {
    left: bool,
    right: bool,
    status: TeletextPanelStatus,
    width: TeletextPanelWidth,
}

impl<'de> Deserialize<'de> for TeletextSidePanels {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let fields: PanelFields = deserialize_object(deserializer)?;
        Ok(Self {
            left: fields.left,
            right: fields.right,
            status: fields.status,
            width: fields.width,
        })
    }
}

/// Complete explicitly supplied X/28/0 or M/29/0 presentation declaration.
///
/// No `Default` is supplied: absence of a packet is not a declaration with
/// guessed charset, palette or receiver fallback values. `secondary: None`
/// explicitly means table 33's no-second-G0 selection, not a missing packet.
/// The secondary selection never changes the primary G2. Page function/coding
/// and magazine packet function are fixed by the encoder, not editable here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TeletextLevel25PresentationDefaults {
    /// Primary G0/G2 and national-option designation.
    pub primary: TeletextPageCharsetDesignation,
    /// Explicit second G0/national selection, or no second G0.
    pub secondary: Option<TeletextPageCharsetDesignation>,
    /// Complete editable CLUT2/3 declaration; CLUT0/1 remain fixed at Level 2.5.
    pub palette: TeletextLevel25Palette,
    /// Default full screen colour, subject to later enhancement precedence.
    pub screen_colour: TeletextColourReference,
    /// Default full row colour, subject to later enhancement precedence.
    pub row_colour: TeletextColourReference,
    /// Replace implicit or Black Background spacing black by the full row colour.
    /// Explicit black foreground followed by New Background is exempt.
    pub black_background_substitution: bool,
    /// Encodable spacing-attribute foreground/background table selection.
    pub remapping: TeletextColourRemapping,
    /// Complete panel declaration, with unused metadata preserved.
    pub panels: TeletextSidePanels,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PresentationFields {
    primary: TeletextPageCharsetDesignation,
    #[serde(deserialize_with = "required_secondary")]
    secondary: Option<TeletextPageCharsetDesignation>,
    #[serde(deserialize_with = "palette_object")]
    palette: TeletextLevel25Palette,
    screen_colour: TeletextColourReference,
    row_colour: TeletextColourReference,
    black_background_substitution: bool,
    remapping: TeletextColourRemapping,
    panels: TeletextSidePanels,
}

impl<'de> Deserialize<'de> for TeletextLevel25PresentationDefaults {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let fields: PresentationFields = deserialize_object(deserializer)?;
        Ok(Self {
            primary: fields.primary,
            secondary: fields.secondary,
            palette: fields.palette,
            screen_colour: fields.screen_colour,
            row_colour: fields.row_colour,
            black_background_substitution: fields.black_background_substitution,
            remapping: fields.remapping,
            panels: fields.panels,
        })
    }
}

fn required_secondary<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<TeletextPageCharsetDesignation>, D::Error> {
    Option::deserialize(deserializer)
}

fn palette_object<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<TeletextLevel25Palette, D::Error> {
    deserialize_object(deserializer)
}

/// Restrict the local closed field DTOs to maps rather than positional arrays.
fn deserialize_object<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct ObjectVisitor<T>(PhantomData<T>);

    impl<'de, T: Deserialize<'de>> Visitor<'de> for ObjectVisitor<T> {
        type Value = T;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a teletext presentation metadata object")
        }

        fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
            T::deserialize(serde::de::value::MapAccessDeserializer::new(map))
        }
    }

    deserializer.deserialize_map(ObjectVisitor(PhantomData))
}
