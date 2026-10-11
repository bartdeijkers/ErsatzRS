//! Checked Level 2.5 MOT declarations, EN 300 706 clause 10.6/tables 20–23.
//!
//! The selected MOT identity retains its logical S1 and magazine. These values
//! do not select an MOT, acquire resources, resolve X27 precedence, calculate
//! service counts, allocate physical object pointers or establish readiness.

use std::collections::BTreeSet;
use std::fmt;
use std::marker::PhantomData;

use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Serialize};

use super::{
    TeletextObjectIdentity, TeletextObjectScope, TeletextObjectType, TeletextResourceIdentity,
    TeletextResourceIdentityError, TeletextResourceKind, TeletextResourceSubpageIndex,
    TeletextSubpageRequirementMask, TeletextX27LinkTarget, TeletextX27Magazine,
};

/// Numeric and owned declaration failures, without resource or programme data.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TeletextMotError {
    #[error("teletext MOT public link index {index} must be between one and seven")]
    PublicIndex { index: u8 },
    #[error("teletext MOT fallback flags {flags} must be a four-bit value")]
    Fallback { flags: u8 },
    #[error("teletext MOT declaration requires MOT identity, received {identity:?}")]
    IdentityKind { identity: TeletextResourceIdentity },
    #[error("teletext MOT association count {count} exceeds 256")]
    AssociationCount { count: usize },
    #[error("teletext MOT repeats raw page association {page:02X}")]
    DuplicateAssociation { page: u8 },
    #[error("teletext MOT {role:?} target: {error}")]
    LinkTarget {
        role: TeletextMotLinkRole,
        error: TeletextResourceIdentityError,
    },
    #[error("teletext MOT {role:?}, default slot {slot}: selection form is incompatible with target usage {target_used}")]
    DefaultForm {
        role: TeletextMotLinkRole,
        slot: u8,
        target_used: bool,
    },
    #[error("teletext MOT {role:?}, default slot {slot}: object {target:?} is not owned by linked page {page}")]
    DefaultOwner {
        role: TeletextMotLinkRole,
        slot: u8,
        target: TeletextObjectIdentity,
        page: super::TeletextRawDataPageNumber,
    },
    #[error("teletext MOT {role:?}, default slot {slot}: object {target:?} exceeds maximum logical S1 {maximum:?}")]
    DefaultExtent {
        role: TeletextMotLinkRole,
        slot: u8,
        target: TeletextObjectIdentity,
        maximum: TeletextResourceSubpageIndex,
    },
}

/// Highest logical S1 transmitted; the declared range is contiguous zero through maximum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct TeletextMotSubpageExtent(TeletextResourceSubpageIndex);

impl TeletextMotSubpageExtent {
    /// # Errors
    /// Rejects highest S1 values outside 0–15; zero means one subpage.
    pub fn new(maximum: u8) -> Result<Self, TeletextResourceIdentityError> {
        TeletextResourceSubpageIndex::new(maximum).map(Self)
    }
    #[must_use]
    pub const fn maximum(self) -> TeletextResourceSubpageIndex {
        self.0
    }
    #[must_use]
    pub const fn requires(self, subpage: TeletextResourceSubpageIndex) -> bool {
        subpage.get() <= self.0.get()
    }
    #[must_use]
    pub const fn required_mask(self) -> TeletextSubpageRequirementMask {
        TeletextSubpageRequirementMask::new(if self.0.get() == 15 {
            u16::MAX
        } else {
            (1_u16 << (self.0.get() + 1)) - 1
        })
    }
}
impl TryFrom<u8> for TeletextMotSubpageExtent {
    type Error = TeletextResourceIdentityError;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<TeletextMotSubpageExtent> for u8 {
    fn from(value: TeletextMotSubpageExtent) -> Self {
        value.maximum().get()
    }
}

/// One of seven public link slots; selector zero is represented explicitly by None.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct TeletextMotPublicLinkIndex(u8);
impl TeletextMotPublicLinkIndex {
    pub const ALL: [Self; 7] = [
        Self(1),
        Self(2),
        Self(3),
        Self(4),
        Self(5),
        Self(6),
        Self(7),
    ];
    /// # Errors
    /// Rejects zero and values greater than seven, without masking them.
    pub fn new(index: u8) -> Result<Self, TeletextMotError> {
        if (1..=7).contains(&index) {
            Ok(Self(index))
        } else {
            Err(TeletextMotError::PublicIndex { index })
        }
    }
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}
impl TryFrom<u8> for TeletextMotPublicLinkIndex {
    type Error = TeletextMotError;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<TeletextMotPublicLinkIndex> for u8 {
    fn from(value: TeletextMotPublicLinkIndex) -> Self {
        value.get()
    }
}

/// Raw fallback nibble; all bits are reserved in GPOP links, and some POP bits
/// are ignored when protocol bit one is set. Both cases retain the full nibble.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct TeletextMotFallback(u8);
impl TeletextMotFallback {
    /// # Errors
    /// Rejects values beyond the authored four-bit field.
    pub fn new(flags: u8) -> Result<Self, TeletextMotError> {
        if flags <= 15 {
            Ok(Self(flags))
        } else {
            Err(TeletextMotError::Fallback { flags })
        }
    }
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
    /// True selects the no-panel/no-substitution protocol; other bits remain stored.
    #[must_use]
    pub const fn protocol_no_panels(self) -> bool {
        self.0 & 1 != 0
    }
    /// Raw table 21 side-panel bits: none, left16, right16, or both8.
    #[must_use]
    pub const fn panel_code(self) -> u8 {
        (self.0 >> 1) & 3
    }
    /// Raw substitution flag, even when the protocol bit suppresses its effect.
    #[must_use]
    pub const fn black_background_substitution(self) -> bool {
        self.0 & 8 != 0
    }
}
impl TryFrom<u8> for TeletextMotFallback {
    type Error = TeletextMotError;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<TeletextMotFallback> for u8 {
    fn from(value: TeletextMotFallback) -> Self {
        value.get()
    }
}

/// One association nibble: an independent global bit and optional public slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TeletextMotAssociationSelection {
    global: bool,
    public: Option<TeletextMotPublicLinkIndex>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SelectionFields {
    global: bool,
    #[serde(deserialize_with = "required_option")]
    public: Option<TeletextMotPublicLinkIndex>,
}
impl TeletextMotAssociationSelection {
    #[must_use]
    pub const fn new(global: bool, public: Option<TeletextMotPublicLinkIndex>) -> Self {
        Self { global, public }
    }
    #[must_use]
    pub const fn global(self) -> bool {
        self.global
    }
    #[must_use]
    pub const fn public(self) -> Option<TeletextMotPublicLinkIndex> {
        self.public
    }
}
impl<'de> Deserialize<'de> for TeletextMotAssociationSelection {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let fields = object::<D, SelectionFields>(deserializer)?;
        Ok(Self::new(fields.global, fields.public))
    }
}

/// Association for one raw page byte in the selected MOT's own magazine.
/// Every byte, including hexadecimal addresses and FF, is retained as authored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TeletextMotPageAssociation {
    page: u8,
    objects: TeletextMotAssociationSelection,
    drcs: TeletextMotAssociationSelection,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AssociationFields {
    page: u8,
    objects: TeletextMotAssociationSelection,
    drcs: TeletextMotAssociationSelection,
}
impl TeletextMotPageAssociation {
    #[must_use]
    pub const fn new(
        page: u8,
        objects: TeletextMotAssociationSelection,
        drcs: TeletextMotAssociationSelection,
    ) -> Self {
        Self {
            page,
            objects,
            drcs,
        }
    }
    #[must_use]
    pub const fn page(self) -> u8 {
        self.page
    }
    #[must_use]
    pub const fn objects(self) -> TeletextMotAssociationSelection {
        self.objects
    }
    #[must_use]
    pub const fn drcs(self) -> TeletextMotAssociationSelection {
        self.drcs
    }
}
impl<'de> Deserialize<'de> for TeletextMotPageAssociation {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let fields = object::<D, AssociationFields>(deserializer)?;
        Ok(Self::new(fields.page, fields.objects, fields.drcs))
    }
}

/// Explicit coverage and totals from clause 10.6.6; raw declaration, not computed evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TeletextMotEnhancementCounts {
    magazine_mask: u8,
    object_pages: u8,
    drcs_pages: u8,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CountFields {
    magazine_mask: u8,
    object_pages: u8,
    drcs_pages: u8,
}
impl TeletextMotEnhancementCounts {
    #[must_use]
    pub const fn new(magazine_mask: u8, object_pages: u8, drcs_pages: u8) -> Self {
        Self {
            magazine_mask,
            object_pages,
            drcs_pages,
        }
    }
    #[must_use]
    pub const fn magazine_mask(self) -> u8 {
        self.magazine_mask
    }
    #[must_use]
    pub const fn object_pages(self) -> u8 {
        self.object_pages
    }
    #[must_use]
    pub const fn drcs_pages(self) -> u8 {
        self.drcs_pages
    }
    /// Mask bit zero describes magazine eight, then bits one through seven describe 1–7.
    #[must_use]
    pub const fn includes_magazine(self, magazine: TeletextX27Magazine) -> bool {
        self.magazine_mask & (1_u8 << magazine.wire_magazine()) != 0
    }
}
impl<'de> Deserialize<'de> for TeletextMotEnhancementCounts {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let fields = object::<D, CountFields>(deserializer)?;
        Ok(Self::new(
            fields.magazine_mask,
            fields.object_pages,
            fields.drcs_pages,
        ))
    }
}

/// Fixed link role, retaining public/normal slot ordinals in diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextMotLinkRole {
    Gpop,
    Pop { index: TeletextMotPublicLinkIndex },
    Gdrcs,
    Drcs { index: TeletextMotPublicLinkIndex },
}
impl TeletextMotLinkRole {
    #[must_use]
    pub const fn kind(self) -> TeletextResourceKind {
        match self {
            Self::Gpop => TeletextResourceKind::Gpop,
            Self::Pop { .. } => TeletextResourceKind::Pop,
            Self::Gdrcs => TeletextResourceKind::Gdrcs,
            Self::Drcs { .. } => TeletextResourceKind::Drcs,
        }
    }
}

/// Ordered default slot, distinguishing ignored pointer bytes from semantic IDs.
/// Active physical selector mapping belongs to the compiler/importer; semantic
/// object IDs are not restricted to physical pointer-table ordinals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TeletextMotDefaultObject {
    /// Type bits zero; ignored object-number bytes are retained.
    NoObject { raw_number: u8 },
    /// Nonzero type under unused FF link; selector remains uninterpreted.
    Dormant {
        object_type: TeletextObjectType,
        raw_number: u8,
    },
    /// Active authored target retains scope, resource page, logical S1 and host ID.
    Object {
        target: TeletextObjectIdentity,
        object_type: TeletextObjectType,
    },
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum DefaultFields {
    NoObject {
        raw_number: u8,
    },
    Dormant {
        #[serde(deserialize_with = "object_type")]
        object_type: TeletextObjectType,
        raw_number: u8,
    },
    Object {
        target: TeletextObjectIdentity,
        #[serde(deserialize_with = "object_type")]
        object_type: TeletextObjectType,
    },
}
impl<'de> Deserialize<'de> for TeletextMotDefaultObject {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match object::<D, DefaultFields>(deserializer)? {
            DefaultFields::NoObject { raw_number } => Self::NoObject { raw_number },
            DefaultFields::Dormant {
                object_type,
                raw_number,
            } => Self::Dormant {
                object_type,
                raw_number,
            },
            DefaultFields::Object {
                target,
                object_type,
            } => Self::Object {
                target,
                object_type,
            },
        })
    }
}

/// Object link; fixed-purpose/default checks belong to the containing bank.
/// In this MOT context, an X27 Unspecified target represents an unused XFF link.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TeletextMotObjectLink {
    target: TeletextX27LinkTarget,
    extent: TeletextMotSubpageExtent,
    supersedable: bool,
    raw_fallback: TeletextMotFallback,
    defaults: [TeletextMotDefaultObject; 2],
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ObjectLinkFields {
    target: TeletextX27LinkTarget,
    extent: TeletextMotSubpageExtent,
    supersedable: bool,
    raw_fallback: TeletextMotFallback,
    defaults: [TeletextMotDefaultObject; 2],
}
impl TeletextMotObjectLink {
    #[must_use]
    pub const fn new(
        target: TeletextX27LinkTarget,
        extent: TeletextMotSubpageExtent,
        supersedable: bool,
        raw_fallback: TeletextMotFallback,
        defaults: [TeletextMotDefaultObject; 2],
    ) -> Self {
        Self {
            target,
            extent,
            supersedable,
            raw_fallback,
            defaults,
        }
    }
    #[must_use]
    pub const fn target(self) -> TeletextX27LinkTarget {
        self.target
    }
    #[must_use]
    pub const fn extent(self) -> TeletextMotSubpageExtent {
        self.extent
    }
    #[must_use]
    pub const fn supersedable(self) -> bool {
        self.supersedable
    }
    #[must_use]
    pub const fn raw_fallback(self) -> TeletextMotFallback {
        self.raw_fallback
    }
    #[must_use]
    pub const fn defaults(self) -> [TeletextMotDefaultObject; 2] {
        self.defaults
    }
}
impl<'de> Deserialize<'de> for TeletextMotObjectLink {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let fields = object::<D, ObjectLinkFields>(deserializer)?;
        Ok(Self::new(
            fields.target,
            fields.extent,
            fields.supersedable,
            fields.raw_fallback,
            fields.defaults,
        ))
    }
}

/// DRCS link retains unused extent/flags without acquiring resource pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TeletextMotDrcsLink {
    target: TeletextX27LinkTarget,
    extent: TeletextMotSubpageExtent,
    supersedable: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DrcsLinkFields {
    target: TeletextX27LinkTarget,
    extent: TeletextMotSubpageExtent,
    supersedable: bool,
}
impl TeletextMotDrcsLink {
    #[must_use]
    pub const fn new(
        target: TeletextX27LinkTarget,
        extent: TeletextMotSubpageExtent,
        supersedable: bool,
    ) -> Self {
        Self {
            target,
            extent,
            supersedable,
        }
    }
    #[must_use]
    pub const fn target(self) -> TeletextX27LinkTarget {
        self.target
    }
    #[must_use]
    pub const fn extent(self) -> TeletextMotSubpageExtent {
        self.extent
    }
    #[must_use]
    pub const fn supersedable(self) -> bool {
        self.supersedable
    }
}
impl<'de> Deserialize<'de> for TeletextMotDrcsLink {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let fields = object::<D, DrcsLinkFields>(deserializer)?;
        Ok(Self::new(fields.target, fields.extent, fields.supersedable))
    }
}

/// Complete authored Level 2.5 bank and raw coverage/count declarations.
/// Level 3.5 MOT packets and declared resource availability are not represented.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TeletextMotLevel25Links {
    global_object: TeletextMotObjectLink,
    public_objects: [TeletextMotObjectLink; 7],
    global_drcs: TeletextMotDrcsLink,
    normal_drcs: [TeletextMotDrcsLink; 7],
    counts: TeletextMotEnhancementCounts,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BankFields {
    global_object: TeletextMotObjectLink,
    public_objects: [TeletextMotObjectLink; 7],
    global_drcs: TeletextMotDrcsLink,
    normal_drcs: [TeletextMotDrcsLink; 7],
    counts: TeletextMotEnhancementCounts,
}
impl TeletextMotLevel25Links {
    /// # Errors
    /// Checks fixed targets and active default ownership/extents; fallback bits are retained.
    /// Definition existence/type agreement and physical pointer mapping remain separate.
    pub fn new(
        global_object: TeletextMotObjectLink,
        public_objects: [TeletextMotObjectLink; 7],
        global_drcs: TeletextMotDrcsLink,
        normal_drcs: [TeletextMotDrcsLink; 7],
        counts: TeletextMotEnhancementCounts,
    ) -> Result<Self, TeletextMotError> {
        let bank = Self {
            global_object,
            public_objects,
            global_drcs,
            normal_drcs,
            counts,
        };
        validate_object(bank.global_object, TeletextMotLinkRole::Gpop)?;
        validate_target(bank.global_drcs.target(), TeletextMotLinkRole::Gdrcs)?;
        for index in TeletextMotPublicLinkIndex::ALL {
            validate_object(
                bank.object_link(Some(index)),
                TeletextMotLinkRole::Pop { index },
            )?;
            validate_target(
                bank.drcs_link(Some(index)).target(),
                TeletextMotLinkRole::Drcs { index },
            )?;
        }
        Ok(bank)
    }
    #[must_use]
    pub const fn object_link(
        self,
        index: Option<TeletextMotPublicLinkIndex>,
    ) -> TeletextMotObjectLink {
        match index {
            None => self.global_object,
            Some(index) => self.public_objects[index.get() as usize - 1],
        }
    }
    #[must_use]
    pub const fn drcs_link(self, index: Option<TeletextMotPublicLinkIndex>) -> TeletextMotDrcsLink {
        match index {
            None => self.global_drcs,
            Some(index) => self.normal_drcs[index.get() as usize - 1],
        }
    }
    #[must_use]
    pub const fn counts(self) -> TeletextMotEnhancementCounts {
        self.counts
    }
    /// Semantic POP fallback only; reserved GPOP fallback bits have no such accessor.
    #[must_use]
    pub const fn public_fallback(self, index: TeletextMotPublicLinkIndex) -> TeletextMotFallback {
        self.object_link(Some(index)).raw_fallback()
    }
}
impl<'de> Deserialize<'de> for TeletextMotLevel25Links {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let fields = object::<D, BankFields>(deserializer)?;
        Self::new(
            fields.global_object,
            fields.public_objects,
            fields.global_drcs,
            fields.normal_drcs,
            fields.counts,
        )
        .map_err(serde::de::Error::custom)
    }
}

/// Explicit selected MOT identity and full Level 2.5 authoring context.
/// Raw FF associations are reserved slots, never usable display owners.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TeletextMotLevel25Declaration {
    identity: TeletextResourceIdentity,
    associations: Vec<TeletextMotPageAssociation>,
    links: TeletextMotLevel25Links,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MotFields {
    identity: TeletextResourceIdentity,
    associations: Vec<TeletextMotPageAssociation>,
    links: TeletextMotLevel25Links,
}
impl TeletextMotLevel25Declaration {
    /// # Errors
    /// Rejects non-MOT identity, over256 associations or duplicate raw bytes.
    /// Nonzero selected MOT S1 is retained; no first-subpage policy is imposed.
    pub fn new(
        identity: TeletextResourceIdentity,
        associations: Vec<TeletextMotPageAssociation>,
        links: TeletextMotLevel25Links,
    ) -> Result<Self, TeletextMotError> {
        if identity.kind() != TeletextResourceKind::Mot {
            return Err(TeletextMotError::IdentityKind { identity });
        }
        if associations.len() > 256 {
            return Err(TeletextMotError::AssociationCount {
                count: associations.len(),
            });
        }
        let mut pages = BTreeSet::new();
        for association in &associations {
            if !pages.insert(association.page()) {
                return Err(TeletextMotError::DuplicateAssociation {
                    page: association.page(),
                });
            }
        }
        Ok(Self {
            identity,
            associations,
            links,
        })
    }
    #[must_use]
    pub const fn identity(&self) -> TeletextResourceIdentity {
        self.identity
    }
    /// Raw slots include reserved FF and retain original authored order.
    #[must_use]
    pub fn associations(&self) -> &[TeletextMotPageAssociation] {
        &self.associations
    }
    /// Returns no usable association for reserved FF.
    #[must_use]
    pub fn association(&self, page: u8) -> Option<TeletextMotPageAssociation> {
        if page == 0xff {
            None
        } else {
            self.associations
                .iter()
                .find(|association| association.page() == page)
                .copied()
        }
    }
    #[must_use]
    pub const fn links(&self) -> TeletextMotLevel25Links {
        self.links
    }
}
impl<'de> Deserialize<'de> for TeletextMotLevel25Declaration {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let fields = object::<D, MotFields>(deserializer)?;
        Self::new(fields.identity, fields.associations, fields.links)
            .map_err(serde::de::Error::custom)
    }
}

fn validate_target(
    target: TeletextX27LinkTarget,
    role: TeletextMotLinkRole,
) -> Result<(), TeletextMotError> {
    if let TeletextX27LinkTarget::Page { page } = target {
        TeletextResourceIdentity::new(role.kind(), page, TeletextResourceSubpageIndex::ZERO)
            .map_err(|error| TeletextMotError::LinkTarget { role, error })?;
    }
    Ok(())
}
fn validate_object(
    link: TeletextMotObjectLink,
    role: TeletextMotLinkRole,
) -> Result<(), TeletextMotError> {
    validate_target(link.target(), role)?;
    let public = matches!(role, TeletextMotLinkRole::Pop { .. });
    for (slot, selection) in (0_u8..2).zip(link.defaults()) {
        match (link.target(), selection) {
            (_, TeletextMotDefaultObject::NoObject { .. })
            | (
                TeletextX27LinkTarget::Unspecified { .. },
                TeletextMotDefaultObject::Dormant { .. },
            ) => {}
            (
                TeletextX27LinkTarget::Page { page },
                TeletextMotDefaultObject::Object { target, .. },
            ) => {
                let scope = if public {
                    TeletextObjectScope::Public
                } else {
                    TeletextObjectScope::Global
                };
                let Some(resource) = target.resource() else {
                    return Err(TeletextMotError::DefaultOwner {
                        role,
                        slot,
                        target,
                        page,
                    });
                };
                if target.scope() != scope
                    || resource.page() != page
                    || resource.kind() != role.kind()
                {
                    return Err(TeletextMotError::DefaultOwner {
                        role,
                        slot,
                        target,
                        page,
                    });
                }
                if !link.extent().requires(resource.logical_subpage()) {
                    return Err(TeletextMotError::DefaultExtent {
                        role,
                        slot,
                        target,
                        maximum: link.extent().maximum(),
                    });
                }
            }
            (target, _) => {
                return Err(TeletextMotError::DefaultForm {
                    role,
                    slot,
                    target_used: matches!(target, TeletextX27LinkTarget::Page { .. }),
                })
            }
        }
    }
    Ok(())
}

fn object_type<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<TeletextObjectType, D::Error> {
    struct TypeVisitor;
    impl Visitor<'_> for TypeVisitor {
        type Value = TeletextObjectType;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("an assigned teletext object type string")
        }
        fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
            match value {
                "active" => Ok(TeletextObjectType::Active),
                "adaptive" => Ok(TeletextObjectType::Adaptive),
                "passive" => Ok(TeletextObjectType::Passive),
                _ => Err(E::custom("unknown teletext object type")),
            }
        }
    }
    deserializer.deserialize_str(TypeVisitor)
}

fn required_option<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    Option::<T>::deserialize(deserializer)
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
            formatter.write_str("a teletext MOT declaration object")
        }
        fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
            T::deserialize(serde::de::value::MapAccessDeserializer::new(map))
        }
    }
    deserializer.deserialize_map(ObjectVisitor(PhantomData))
}
