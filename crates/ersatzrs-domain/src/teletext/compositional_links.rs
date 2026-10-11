//! Checked X27/4 Format 1 declarations, EN 300 706 clause 9.6.2/tables 15–16.
//!
//! These values retain authored links, not resolved resources or decoder readiness.
//! Packet absence belongs outside this model. An explicit FF/FFFF target does
//! not automatically remove a resolved resource or permit fetching one.

use std::fmt;
use std::marker::PhantomData;

use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Serialize};

use super::{
    TeletextRawDataPageNumber, TeletextResourceIdentity, TeletextResourceIdentityError,
    TeletextResourceKind, TeletextResourceSubpageIndex,
};

/// Every bit corresponds to its unchanged logical S1 value; zero is legal.
/// No default is supplied because an omitted mask is not an authored selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TeletextSubpageRequirementMask(u16);

impl TeletextSubpageRequirementMask {
    #[must_use]
    pub const fn new(mask: u16) -> Self {
        Self(mask)
    }
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
    #[must_use]
    pub const fn requires(self, subpage: TeletextResourceSubpageIndex) -> bool {
        self.0 & (1_u16 << subpage.get()) != 0
    }
}

/// Located authored-link failure, without programme content or fetched resources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextX27LinkError {
    #[error("teletext X27 target magazine {magazine} must be between one and eight")]
    Magazine { magazine: u8 },
    #[error("teletext X27 unspecified target requires mask FFFF, received {mask:04X}")]
    UnspecifiedMask { mask: u16 },
    #[error("teletext X27 {role:?} target: {error}")]
    TargetPurpose {
        role: TeletextX27LinkRole,
        error: TeletextResourceIdentityError,
    },
}

/// Explicit target magazine, including for an unspecified FF page.
/// Relative wire bits are derived from this address and the packet owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct TeletextX27Magazine(u8);

impl TeletextX27Magazine {
    /// # Errors
    /// Rejects magazines outside the logical range 1–8.
    pub fn new(magazine: u8) -> Result<Self, TeletextX27LinkError> {
        if (1..=8).contains(&magazine) {
            Ok(Self(magazine))
        } else {
            Err(TeletextX27LinkError::Magazine { magazine })
        }
    }
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
    #[must_use]
    pub const fn wire_magazine(self) -> u8 {
        self.0 & 7
    }
}

impl TryFrom<u8> for TeletextX27Magazine {
    type Error = TeletextX27LinkError;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<TeletextX27Magazine> for u8 {
    fn from(value: TeletextX27Magazine) -> Self {
        value.get()
    }
}

/// Table 15 validity bits; Level 3.5 metadata is preserved without rendering it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TeletextX27LinkValidity {
    Level25Only,
    Level35Only,
    Level25And35,
}

impl TeletextX27LinkValidity {
    #[must_use]
    pub const fn wire_code(self) -> u8 {
        match self {
            Self::Level25Only => 1,
            Self::Level35Only => 2,
            Self::Level25And35 => 3,
        }
    }
    #[must_use]
    pub const fn applies_at_level25(self) -> bool {
        !matches!(self, Self::Level35Only)
    }
}

impl<'de> Deserialize<'de> for TeletextX27LinkValidity {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ValidityVisitor;
        impl Visitor<'_> for ValidityVisitor {
            type Value = TeletextX27LinkValidity;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("an assigned teletext X27 validity string")
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                match value {
                    "level25_only" => Ok(TeletextX27LinkValidity::Level25Only),
                    "level35_only" => Ok(TeletextX27LinkValidity::Level35Only),
                    "level25_and35" => Ok(TeletextX27LinkValidity::Level25And35),
                    _ => Err(E::custom("unknown teletext X27 validity")),
                }
            }
        }
        deserializer.deserialize_str(ValidityVisitor)
    }
}

/// One explicit address selection; Unspecified retains the authored magazine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TeletextX27LinkTarget {
    Unspecified { magazine: TeletextX27Magazine },
    Page { page: TeletextRawDataPageNumber },
}

impl TeletextX27LinkTarget {
    #[must_use]
    pub const fn magazine(self) -> u8 {
        match self {
            Self::Unspecified { magazine } => magazine.get(),
            Self::Page { page } => page.magazine(),
        }
    }
    #[must_use]
    pub const fn wire_magazine(self) -> u8 {
        self.magazine() & 7
    }
    #[must_use]
    pub const fn page_byte(self) -> u8 {
        match self {
            Self::Unspecified { .. } => 0xff,
            Self::Page { page } => page.page(),
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum TargetFields {
    Unspecified { magazine: TeletextX27Magazine },
    Page { page: TeletextRawDataPageNumber },
}

impl<'de> Deserialize<'de> for TeletextX27LinkTarget {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match object::<D, TargetFields>(deserializer)? {
            TargetFields::Unspecified { magazine } => Self::Unspecified { magazine },
            TargetFields::Page { page } => Self::Page { page },
        })
    }
}

/// A required complete link declaration; private fields prevent null-mask bypass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TeletextX27LinkDeclaration {
    target: TeletextX27LinkTarget,
    validity: TeletextX27LinkValidity,
    required_subpages: TeletextSubpageRequirementMask,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DeclarationFields {
    target: TeletextX27LinkTarget,
    validity: TeletextX27LinkValidity,
    required_subpages: TeletextSubpageRequirementMask,
}

impl TeletextX27LinkDeclaration {
    /// # Errors
    /// Explicit unspecified FF targets require all sixteen flags set.
    /// Purpose is checked by the containing fixed-role declaration.
    pub fn new(
        target: TeletextX27LinkTarget,
        validity: TeletextX27LinkValidity,
        required_subpages: TeletextSubpageRequirementMask,
    ) -> Result<Self, TeletextX27LinkError> {
        if matches!(target, TeletextX27LinkTarget::Unspecified { .. })
            && required_subpages.get() != u16::MAX
        {
            return Err(TeletextX27LinkError::UnspecifiedMask {
                mask: required_subpages.get(),
            });
        }
        Ok(Self {
            target,
            validity,
            required_subpages,
        })
    }
    #[must_use]
    pub const fn target(self) -> TeletextX27LinkTarget {
        self.target
    }
    #[must_use]
    pub const fn validity(self) -> TeletextX27LinkValidity {
        self.validity
    }
    #[must_use]
    pub const fn required_subpages(self) -> TeletextSubpageRequirementMask {
        self.required_subpages
    }
}

impl<'de> Deserialize<'de> for TeletextX27LinkDeclaration {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let fields = object::<D, DeclarationFields>(deserializer)?;
        Self::new(fields.target, fields.validity, fields.required_subpages)
            .map_err(serde::de::Error::custom)
    }
}

/// Four directional fixed roles, never an arbitrary ordering or duplicate role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextX27LinkRole {
    Gpop,
    Pop,
    Gdrcs,
    Drcs,
}

impl TeletextX27LinkRole {
    pub const ALL: [Self; 4] = [Self::Gpop, Self::Pop, Self::Gdrcs, Self::Drcs];
    #[must_use]
    pub const fn kind(self) -> TeletextResourceKind {
        match self {
            Self::Gpop => TeletextResourceKind::Gpop,
            Self::Pop => TeletextResourceKind::Pop,
            Self::Gdrcs => TeletextResourceKind::Gdrcs,
            Self::Drcs => TeletextResourceKind::Drcs,
        }
    }
    /// Table 15 link-function bits and table 16 fixed slot ordinal.
    #[must_use]
    pub const fn wire_code(self) -> u8 {
        match self {
            Self::Gpop => 0,
            Self::Pop => 1,
            Self::Gdrcs => 2,
            Self::Drcs => 3,
        }
    }
}

/// Complete first four links of X27/4; packet absence remains Option outside.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TeletextX27Level25Links {
    gpop: TeletextX27LinkDeclaration,
    pop: TeletextX27LinkDeclaration,
    gdrcs: TeletextX27LinkDeclaration,
    drcs: TeletextX27LinkDeclaration,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LinksFields {
    gpop: TeletextX27LinkDeclaration,
    pop: TeletextX27LinkDeclaration,
    gdrcs: TeletextX27LinkDeclaration,
    drcs: TeletextX27LinkDeclaration,
}

impl TeletextX27Level25Links {
    /// # Errors
    /// Rejects real targets reserved for another page purpose, with their role.
    /// Zero masks and `Level35Only` declarations remain valid authored metadata.
    pub fn new(
        gpop: TeletextX27LinkDeclaration,
        pop: TeletextX27LinkDeclaration,
        gdrcs: TeletextX27LinkDeclaration,
        drcs: TeletextX27LinkDeclaration,
    ) -> Result<Self, TeletextX27LinkError> {
        let links = Self {
            gpop,
            pop,
            gdrcs,
            drcs,
        };
        for role in [
            TeletextX27LinkRole::Gpop,
            TeletextX27LinkRole::Pop,
            TeletextX27LinkRole::Gdrcs,
            TeletextX27LinkRole::Drcs,
        ] {
            if let TeletextX27LinkTarget::Page { page } = links.link(role).target() {
                // Logical S1 zero is only an address-purpose validation input;
                // the authored mask retains its complete independent selection.
                TeletextResourceIdentity::new(
                    role.kind(),
                    page,
                    TeletextResourceSubpageIndex::ZERO,
                )
                .map_err(|error| TeletextX27LinkError::TargetPurpose { role, error })?;
            }
        }
        Ok(links)
    }
    #[must_use]
    pub const fn link(self, role: TeletextX27LinkRole) -> TeletextX27LinkDeclaration {
        match role {
            TeletextX27LinkRole::Gpop => self.gpop,
            TeletextX27LinkRole::Pop => self.pop,
            TeletextX27LinkRole::Gdrcs => self.gdrcs,
            TeletextX27LinkRole::Drcs => self.drcs,
        }
    }
}

impl<'de> Deserialize<'de> for TeletextX27Level25Links {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let fields = object::<D, LinksFields>(deserializer)?;
        Self::new(fields.gpop, fields.pop, fields.gdrcs, fields.drcs)
            .map_err(serde::de::Error::custom)
    }
}

fn object<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct ObjectVisitor<T>(PhantomData<T>);
    impl<'de, T: Deserialize<'de>> Visitor<'de> for ObjectVisitor<T> {
        type Value = T;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a teletext compositional link object")
        }
        fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
            T::deserialize(serde::de::value::MapAccessDeserializer::new(map))
        }
    }
    deserializer.deserialize_map(ObjectVisitor(PhantomData))
}
