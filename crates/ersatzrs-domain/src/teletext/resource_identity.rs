//! Authored resource identities, EN 300 706 Annex A/clauses 10.5–11.3 (ADR 0123).
//!
//! Resource identity retains logical S1, not physical `nnXs` update/last-packet
//! metadata. Checked identities do not establish content, dependencies, service
//! allocation uniqueness, encoding feasibility or publication readiness.

use std::fmt;
use std::marker::PhantomData;

use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Serialize};

/// Failure to construct a resource identity, retaining its numeric location.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextResourceIdentityError {
    #[error("teletext resource magazine {magazine} must be between one and eight")]
    Magazine { magazine: u8 },
    #[error("teletext data page {magazine}{page:02X} cannot use terminator/null page FF")]
    PageByte { magazine: u8, page: u8 },
    #[error("teletext resource logical subpage {subpage} must be between zero and 15")]
    Subpage { subpage: u8 },
    #[error("teletext resource {kind:?} cannot use reserved-purpose page {page}")]
    ReservedPurpose {
        kind: TeletextResourceKind,
        page: TeletextRawDataPageNumber,
    },
    #[error("teletext resource {kind:?} at {page} requires page byte {required_page:02X}")]
    RequiredPage {
        kind: TeletextResourceKind,
        page: TeletextRawDataPageNumber,
        required_page: u8,
    },
    #[error("teletext MIP resource {page} requires logical subpage zero, received {subpage}")]
    MipSubpage {
        page: TeletextRawDataPageNumber,
        subpage: u8,
    },
}

/// Raw data-page address, separate from ordinary decimal display identities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(into = "RawDataPageFields")]
pub struct TeletextRawDataPageNumber {
    magazine: u8,
    page: u8,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDataPageFields {
    magazine: u8,
    page: u8,
}

impl TeletextRawDataPageNumber {
    /// Preserve a magazine and raw hexadecimal page byte without BCD conversion.
    /// # Errors
    /// Rejects magazines outside 1–8 and page FF, which carries no acquired data.
    pub fn new(magazine: u8, page: u8) -> Result<Self, TeletextResourceIdentityError> {
        if !(1..=8).contains(&magazine) {
            return Err(TeletextResourceIdentityError::Magazine { magazine });
        }
        if page == 0xff {
            return Err(TeletextResourceIdentityError::PageByte { magazine, page });
        }
        Ok(Self { magazine, page })
    }

    /// Logical magazine number, including eight rather than its wire value zero.
    #[must_use]
    pub const fn magazine(self) -> u8 {
        self.magazine
    }

    /// Original page byte, not a decimal display-page suffix.
    #[must_use]
    pub const fn page(self) -> u8 {
        self.page
    }

    /// Three packet-address magazine bits; magazine eight is encoded as zero.
    #[must_use]
    pub const fn wire_magazine(self) -> u8 {
        self.magazine & 7
    }
}

impl TryFrom<RawDataPageFields> for TeletextRawDataPageNumber {
    type Error = TeletextResourceIdentityError;

    fn try_from(value: RawDataPageFields) -> Result<Self, Self::Error> {
        Self::new(value.magazine, value.page)
    }
}

impl<'de> Deserialize<'de> for TeletextRawDataPageNumber {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::try_from(deserialize_object::<D, RawDataPageFields>(deserializer)?)
            .map_err(serde::de::Error::custom)
    }
}

impl From<TeletextRawDataPageNumber> for RawDataPageFields {
    fn from(value: TeletextRawDataPageNumber) -> Self {
        Self {
            magazine: value.magazine,
            page: value.page,
        }
    }
}

impl fmt::Display for TeletextRawDataPageNumber {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}{:02X}", self.magazine, self.page)
    }
}

/// Logical S1 resource subpage, independent of full physical header subcodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct TeletextResourceSubpageIndex(u8);

impl TeletextResourceSubpageIndex {
    /// Explicit logical S1 zero, without any physical update/header bits.
    pub const ZERO: Self = Self(0);

    /// All sixteen assigned logical S1 values, in unchanged numeric order.
    pub const ALL: [Self; 16] = [
        Self(0),
        Self(1),
        Self(2),
        Self(3),
        Self(4),
        Self(5),
        Self(6),
        Self(7),
        Self(8),
        Self(9),
        Self(10),
        Self(11),
        Self(12),
        Self(13),
        Self(14),
        Self(15),
    ];

    /// Validate a logical subpage without masking physical metadata bits.
    /// # Errors
    /// Returns `Subpage` for values above 15.
    pub fn new(subpage: u8) -> Result<Self, TeletextResourceIdentityError> {
        if subpage <= 15 {
            Ok(Self(subpage))
        } else {
            Err(TeletextResourceIdentityError::Subpage { subpage })
        }
    }

    /// Original zero-based logical S1 value.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for TeletextResourceSubpageIndex {
    type Error = TeletextResourceIdentityError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<TeletextResourceSubpageIndex> for u8 {
    fn from(value: TeletextResourceSubpageIndex) -> Self {
        value.get()
    }
}

/// Page purpose; this declaration supplies no resource payload implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TeletextResourceKind {
    /// Public object definition page.
    Pop,
    /// Global public object definition page.
    Gpop,
    /// Normal DRCS downloading page.
    Drcs,
    /// Global DRCS downloading page.
    Gdrcs,
    /// Magazine organization table.
    Mot,
    /// Magazine inventory page.
    Mip,
}

impl<'de> Deserialize<'de> for TeletextResourceKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct KindVisitor;

        impl Visitor<'_> for KindVisitor {
            type Value = TeletextResourceKind;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a lowercase teletext resource kind string")
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                match value {
                    "pop" => Ok(TeletextResourceKind::Pop),
                    "gpop" => Ok(TeletextResourceKind::Gpop),
                    "drcs" => Ok(TeletextResourceKind::Drcs),
                    "gdrcs" => Ok(TeletextResourceKind::Gdrcs),
                    "mot" => Ok(TeletextResourceKind::Mot),
                    "mip" => Ok(TeletextResourceKind::Mip),
                    _ => Err(E::custom("unknown teletext resource kind")),
                }
            }
        }

        deserializer.deserialize_str(KindVisitor)
    }
}

impl TeletextResourceKind {
    /// Page-function bits from EN 300 706 clause 9.4.2.1, table 3.
    #[must_use]
    pub const fn function_code(self) -> u8 {
        match self {
            Self::Gpop => 2,
            Self::Pop => 3,
            Self::Gdrcs => 4,
            Self::Drcs => 5,
            Self::Mot => 6,
            Self::Mip => 7,
        }
    }

    /// Page-coding bits for this purpose; independently editable codes are absent.
    #[must_use]
    pub const fn coding_code(self) -> u8 {
        match self {
            Self::Pop | Self::Gpop => 2,
            Self::Drcs | Self::Gdrcs => 0,
            Self::Mot | Self::Mip => 3,
        }
    }
}

/// Checked resource purpose/address/subpage, not a service allocation guarantee.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(into = "ResourceIdentityFields")]
pub struct TeletextResourceIdentity {
    kind: TeletextResourceKind,
    page: TeletextRawDataPageNumber,
    logical_subpage: TeletextResourceSubpageIndex,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ResourceIdentityFields {
    kind: TeletextResourceKind,
    page: TeletextRawDataPageNumber,
    logical_subpage: TeletextResourceSubpageIndex,
}

impl TeletextResourceIdentity {
    /// Validate mandatory reserved purposes and MIP's fixed logical S1 value.
    ///
    /// Decimal-looking object/DRCS addresses are accepted. MOT subpage zero is a
    /// recommended authoring convention, not a constructor restriction.
    /// # Errors
    /// Rejects an incompatible reserved address, wrong MOT/MIP page, or nonzero MIP S1.
    pub fn new(
        kind: TeletextResourceKind,
        page: TeletextRawDataPageNumber,
        logical_subpage: TeletextResourceSubpageIndex,
    ) -> Result<Self, TeletextResourceIdentityError> {
        let required_page = match kind {
            TeletextResourceKind::Mot => Some(0xfe),
            TeletextResourceKind::Mip => Some(0xfd),
            _ => None,
        };
        if let Some(required_page) = required_page {
            if page.page() != required_page {
                return Err(TeletextResourceIdentityError::RequiredPage {
                    kind,
                    page,
                    required_page,
                });
            }
        } else if matches!(page.page(), 0xfd | 0xfe)
            || (page.magazine() == 1 && matches!(page.page(), 0xbe | 0xf0))
        {
            return Err(TeletextResourceIdentityError::ReservedPurpose { kind, page });
        }
        if kind == TeletextResourceKind::Mip && logical_subpage.get() != 0 {
            return Err(TeletextResourceIdentityError::MipSubpage {
                page,
                subpage: logical_subpage.get(),
            });
        }
        Ok(Self {
            kind,
            page,
            logical_subpage,
        })
    }

    /// Resource purpose, not inferred from address or contents.
    #[must_use]
    pub const fn kind(self) -> TeletextResourceKind {
        self.kind
    }

    /// Original raw data-page address.
    #[must_use]
    pub const fn page(self) -> TeletextRawDataPageNumber {
        self.page
    }

    /// Stable logical subpage, excluding update and last-packet fields.
    #[must_use]
    pub const fn logical_subpage(self) -> TeletextResourceSubpageIndex {
        self.logical_subpage
    }
}

impl TryFrom<ResourceIdentityFields> for TeletextResourceIdentity {
    type Error = TeletextResourceIdentityError;

    fn try_from(value: ResourceIdentityFields) -> Result<Self, Self::Error> {
        Self::new(value.kind, value.page, value.logical_subpage)
    }
}

impl<'de> Deserialize<'de> for TeletextResourceIdentity {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::try_from(deserialize_object::<D, ResourceIdentityFields>(
            deserializer,
        )?)
        .map_err(serde::de::Error::custom)
    }
}

impl From<TeletextResourceIdentity> for ResourceIdentityFields {
    fn from(value: TeletextResourceIdentity) -> Self {
        Self {
            kind: value.kind,
            page: value.page,
            logical_subpage: value.logical_subpage,
        }
    }
}

/// Unlike derived struct visitors, this authored object boundary accepts no tuple arrays.
fn deserialize_object<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct ObjectVisitor<T>(PhantomData<T>);

    impl<'de, T: Deserialize<'de>> Visitor<'de> for ObjectVisitor<T> {
        type Value = T;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a teletext resource identity object")
        }

        fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
            T::deserialize(serde::de::value::MapAccessDeserializer::new(map))
        }
    }

    deserializer.deserialize_map(ObjectVisitor(PhantomData))
}
