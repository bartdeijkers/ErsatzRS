//! DRCS definitions and display dependency checks, EN 300 706 14.1/14.4.1.
//!
//! This model preserves monochrome authored resources. It does not compile PTUs,
//! allocate a complete service, expand object invocations or establish readiness.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::{
    TeletextDrcsBitmap, TeletextDrcsGlyphSlot, TeletextDrcsSubtableIndex, TeletextPageAddress,
    TeletextRawDataPageNumber, TeletextResourceIdentity, TeletextResourceIdentityError,
    TeletextResourceKind, TeletextResourceSubpageIndex, MAX_TELETEXT_PAGE_RECORDS,
};

/// Located failure in an authored DRCS resource or display dependency.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TeletextDrcsResourceError {
    #[error("teletext DRCS subtable {identity:?} must have DRCS or GDRCS purpose")]
    Kind { identity: TeletextResourceIdentity },
    #[error("teletext DRCS subtable {identity:?} contains {count} glyphs; maximum is 48")]
    GlyphCount {
        identity: TeletextResourceIdentity,
        count: usize,
    },
    #[error("teletext DRCS subtable {identity:?} repeats glyph slot {slot:?}")]
    DuplicateGlyph {
        identity: TeletextResourceIdentity,
        slot: TeletextDrcsGlyphSlot,
    },
    #[error("teletext DRCS catalog contains {count} records; host budget is {MAX_TELETEXT_PAGE_RECORDS}")]
    ResourceCount { count: usize },
    #[error("teletext DRCS resources collide at {page}/{subpage:?}")]
    DuplicateResource {
        page: TeletextRawDataPageNumber,
        subpage: TeletextResourceSubpageIndex,
    },
    #[error(transparent)]
    Identity(#[from] TeletextResourceIdentityError),
    #[error("teletext display {display:?} links missing {table:?} DRCS page {page}")]
    MissingPage {
        display: TeletextPageAddress,
        table: TeletextDrcsTableScope,
        page: TeletextRawDataPageNumber,
    },
    #[error(
        "teletext display {display:?}, DRCS reference {reference}: {table:?} table has no link"
    )]
    MissingLink {
        display: TeletextPageAddress,
        reference: usize,
        table: TeletextDrcsTableScope,
    },
    #[error(
        "teletext display {display:?}, DRCS reference {reference}: missing subtable {identity:?}"
    )]
    MissingSubtable {
        display: TeletextPageAddress,
        reference: usize,
        identity: TeletextResourceIdentity,
    },
    #[error("teletext display {display:?}, DRCS reference {reference}: subtable {identity:?} lacks slot {slot:?}")]
    MissingGlyph {
        display: TeletextPageAddress,
        reference: usize,
        identity: TeletextResourceIdentity,
        slot: TeletextDrcsGlyphSlot,
    },
    #[error("teletext display {display:?}, DRCS reference {reference}: Level 2.5 permits at most two subtables")]
    DisplaySubtables {
        display: TeletextPageAddress,
        reference: usize,
    },
    #[error("teletext display {display:?}, DRCS reference {reference}: Level 2.5 permits at most 24 distinct glyphs")]
    DisplayGlyphs {
        display: TeletextPageAddress,
        reference: usize,
    },
}

/// One defined glyph, with a checked slot and twelve-by-ten monochrome bitmap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeletextDrcsGlyphDefinition {
    pub slot: TeletextDrcsGlyphSlot,
    pub bitmap: TeletextDrcsBitmap,
}

/// A checked logical DRCS subtable; empty tables do not define any glyphs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "DrcsSubtableFields")]
pub struct TeletextDrcsSubtable {
    identity: TeletextResourceIdentity,
    glyphs: Vec<TeletextDrcsGlyphDefinition>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DrcsSubtableFields {
    identity: TeletextResourceIdentity,
    glyphs: Vec<TeletextDrcsGlyphDefinition>,
}

impl TeletextDrcsSubtable {
    /// Validate purpose, size and slot uniqueness without renumbering glyphs.
    /// # Errors
    /// Rejects non-DRCS purposes, more than 48 definitions or repeated slots.
    pub fn new(
        identity: TeletextResourceIdentity,
        glyphs: Vec<TeletextDrcsGlyphDefinition>,
    ) -> Result<Self, TeletextDrcsResourceError> {
        if !matches!(
            identity.kind(),
            TeletextResourceKind::Drcs | TeletextResourceKind::Gdrcs
        ) {
            return Err(TeletextDrcsResourceError::Kind { identity });
        }
        if glyphs.len() > 48 {
            return Err(TeletextDrcsResourceError::GlyphCount {
                identity,
                count: glyphs.len(),
            });
        }
        let mut slots = BTreeSet::new();
        for glyph in &glyphs {
            if !slots.insert(glyph.slot) {
                return Err(TeletextDrcsResourceError::DuplicateGlyph {
                    identity,
                    slot: glyph.slot,
                });
            }
        }
        Ok(Self { identity, glyphs })
    }

    #[must_use]
    pub const fn identity(&self) -> TeletextResourceIdentity {
        self.identity
    }

    /// Definitions retain their authored order and exact bitmap values.
    #[must_use]
    pub fn glyphs(&self) -> &[TeletextDrcsGlyphDefinition] {
        &self.glyphs
    }
}

impl TryFrom<DrcsSubtableFields> for TeletextDrcsSubtable {
    type Error = TeletextDrcsResourceError;

    fn try_from(value: DrcsSubtableFields) -> Result<Self, Self::Error> {
        Self::new(value.identity, value.glyphs)
    }
}

/// The two DRCS table scopes selectable by enhancement data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeletextDrcsTableScope {
    Normal,
    Global,
}

impl TeletextDrcsTableScope {
    const fn kind(self) -> TeletextResourceKind {
        match self {
            Self::Normal => TeletextResourceKind::Drcs,
            Self::Global => TeletextResourceKind::Gdrcs,
        }
    }
}

/// Resolved per-display links, after the later MOT/page-link precedence step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "DrcsLinkFields")]
pub struct TeletextDrcsPageLinks {
    normal: Option<TeletextRawDataPageNumber>,
    global: Option<TeletextRawDataPageNumber>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DrcsLinkFields {
    normal: Option<TeletextRawDataPageNumber>,
    global: Option<TeletextRawDataPageNumber>,
}

impl TeletextDrcsPageLinks {
    /// Check linked page purposes without acquiring resources or selecting S1.
    /// # Errors
    /// Rejects addresses reserved for another page purpose.
    pub fn new(
        normal: Option<TeletextRawDataPageNumber>,
        global: Option<TeletextRawDataPageNumber>,
    ) -> Result<Self, TeletextResourceIdentityError> {
        for (page, kind) in [
            (normal, TeletextResourceKind::Drcs),
            (global, TeletextResourceKind::Gdrcs),
        ] {
            if let Some(page) = page {
                TeletextResourceIdentity::new(kind, page, TeletextResourceSubpageIndex::new(0)?)?;
            }
        }
        Ok(Self { normal, global })
    }

    #[must_use]
    pub const fn page(self, table: TeletextDrcsTableScope) -> Option<TeletextRawDataPageNumber> {
        match table {
            TeletextDrcsTableScope::Normal => self.normal,
            TeletextDrcsTableScope::Global => self.global,
        }
    }
}

impl TryFrom<DrcsLinkFields> for TeletextDrcsPageLinks {
    type Error = TeletextResourceIdentityError;

    fn try_from(value: DrcsLinkFields) -> Result<Self, Self::Error> {
        Self::new(value.normal, value.global)
    }
}

/// Semantic glyph reference; physical resource headers are not editable here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeletextDrcsGlyphReference {
    pub table: TeletextDrcsTableScope,
    pub subtable: TeletextDrcsSubtableIndex,
    pub slot: TeletextDrcsGlyphSlot,
}

/// Bounded checked DRCS catalog, not a complete validated teletext service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "DrcsCatalogFields")]
pub struct TeletextDrcsCatalog {
    subtables: Vec<TeletextDrcsSubtable>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DrcsCatalogFields {
    subtables: Vec<TeletextDrcsSubtable>,
}

impl TeletextDrcsCatalog {
    /// Bound records before indexing and reject exact logical address collisions.
    /// # Errors
    /// Rejects the host record budget or duplicate page/S1, even across kinds.
    /// JSON callers must separately bound input bytes before deserialization.
    pub fn new(subtables: Vec<TeletextDrcsSubtable>) -> Result<Self, TeletextDrcsResourceError> {
        if subtables.len() > MAX_TELETEXT_PAGE_RECORDS {
            return Err(TeletextDrcsResourceError::ResourceCount {
                count: subtables.len(),
            });
        }
        let mut identities = BTreeSet::new();
        for subtable in &subtables {
            let identity = subtable.identity();
            if !identities.insert((identity.page(), identity.logical_subpage())) {
                return Err(TeletextDrcsResourceError::DuplicateResource {
                    page: identity.page(),
                    subpage: identity.logical_subpage(),
                });
            }
        }
        Ok(Self { subtables })
    }

    #[must_use]
    pub fn subtables(&self) -> &[TeletextDrcsSubtable] {
        &self.subtables
    }

    /// Resolve supplied display references, including references expanded from objects.
    /// Repeated uses of the same glyph do not consume additional distinct slots.
    /// # Errors
    /// Rejects unresolved links/subtables/glyphs, more than two selected subtables
    /// or more than 24 distinct glyphs, retaining display and reference locations.
    /// The caller must supply the complete reference sequence; object expansion,
    /// display/resource collisions and aggregate byte/work budgets are separate.
    pub fn validate_display_usage(
        &self,
        display: TeletextPageAddress,
        links: TeletextDrcsPageLinks,
        references: &[TeletextDrcsGlyphReference],
    ) -> Result<(), TeletextDrcsResourceError> {
        for table in [
            TeletextDrcsTableScope::Normal,
            TeletextDrcsTableScope::Global,
        ] {
            if let Some(page) = links.page(table) {
                if !self.subtables.iter().any(|subtable| {
                    subtable.identity.kind() == table.kind() && subtable.identity.page() == page
                }) {
                    return Err(TeletextDrcsResourceError::MissingPage {
                        display,
                        table,
                        page,
                    });
                }
            }
        }
        let mut selected = BTreeSet::new();
        let mut glyphs = BTreeSet::new();
        for (reference, value) in references.iter().enumerate() {
            let page = links
                .page(value.table)
                .ok_or(TeletextDrcsResourceError::MissingLink {
                    display,
                    reference,
                    table: value.table,
                })?;
            let identity = TeletextResourceIdentity::new(
                value.table.kind(),
                page,
                TeletextResourceSubpageIndex::new(value.subtable.get())?,
            )?;
            let subtable = self
                .subtables
                .iter()
                .find(|subtable| subtable.identity == identity)
                .ok_or(TeletextDrcsResourceError::MissingSubtable {
                    display,
                    reference,
                    identity,
                })?;
            if !subtable.glyphs.iter().any(|glyph| glyph.slot == value.slot) {
                return Err(TeletextDrcsResourceError::MissingGlyph {
                    display,
                    reference,
                    identity,
                    slot: value.slot,
                });
            }
            selected.insert(identity);
            if selected.len() > 2 {
                return Err(TeletextDrcsResourceError::DisplaySubtables { display, reference });
            }
            glyphs.insert((identity, value.slot));
            if glyphs.len() > 24 {
                return Err(TeletextDrcsResourceError::DisplayGlyphs { display, reference });
            }
        }
        Ok(())
    }
}

impl TryFrom<DrcsCatalogFields> for TeletextDrcsCatalog {
    type Error = TeletextDrcsResourceError;

    fn try_from(value: DrcsCatalogFields) -> Result<Self, Self::Error> {
        Self::new(value.subtables)
    }
}
