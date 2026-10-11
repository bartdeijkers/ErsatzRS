//! Independent authored resource declarations and checked-selection assertions.
//!
//! Logical IDs and S1 values never identify allocated pointer slots, update
//! counters, encoded packet lengths, publication generations or ready output.
//! Scalar ranges, ownership, dependency closure and host budgets are checked by
//! the application/domain conversion after the strict wire parser.

use super::{required_nullable, TeletextEnhancementProgramWire, TeletextPageAddressWire};

wire_string_enum! {
/// Declared page purpose, checked against its raw address during conversion.
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum TeletextResourceKindWire {
    Pop,
    Gpop,
    Drcs,
    Gdrcs,
    Mot,
    Mip,
}
}

wire_object! {
    /// Logical magazine and unchanged hexadecimal resource-page byte.
    pub struct TeletextRawDataPageNumberWire {
        pub magazine: u8,
        pub page: u8,
    }
}

wire_object! {
    /// Authored purpose/address/logical S1; `subpage` excludes physical nn/S2.
    pub struct TeletextResourceIdentityWire {
        pub kind: TeletextResourceKindWire,
        pub page: TeletextRawDataPageNumberWire,
        pub subpage: u8,
    }
}

wire_string_enum! {
/// Object behavior is independent of Local/Public/Global definition scope.
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum TeletextObjectTypeWire {
    Active,
    Adaptive,
    Passive,
}
}

wire_tagged_enum! {
/// Logical object ID within an exact display owner or resource, never a pointer.
#[serde(tag = "scope", rename_all = "snake_case", deny_unknown_fields)]
pub enum TeletextObjectIdentityWire {
    Local {
        #[serde(deserialize_with = "crate::teletext_pages_v2::deserialize_object")]
        owner: TeletextPageAddressWire,
        object_id: u16,
    },
    Public {
        resource: TeletextResourceIdentityWire,
        object_id: u16,
    },
    Global {
        resource: TeletextResourceIdentityWire,
        object_id: u16,
    },
}
}

wire_object! {
    /// Complete ordered definition; its identity and invocation graph need validation.
    pub struct TeletextEnhancementObjectDefinitionWire {
        pub identity: TeletextObjectIdentityWire,
        pub object_type: TeletextObjectTypeWire,
        pub program: TeletextEnhancementProgramWire,
    }
}

wire_string_enum! {
/// Explicit enhanced DRCS table scope, independent of mutable receiver state.
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum TeletextDrcsTableScopeWire {
    Normal,
    Global,
}
}

wire_object! {
    /// Logical S1/subtable and glyph slot, not a Unicode or physical PTU address.
    pub struct TeletextDrcsGlyphReferenceWire {
        pub table: TeletextDrcsTableScopeWire,
        pub subtable: u8,
        pub slot: u8,
    }
}

wire_object! {
    /// Ten top-to-bottom monochrome rows; bit 11 is the leftmost of twelve pixels.
    pub struct TeletextDrcsGlyphDefinitionWire {
        pub slot: u8,
        pub bitmap: [u16; 10],
    }
}

wire_object! {
    /// Authored DRCS/GDRCS definitions; an explicit empty table defines no glyphs.
    pub struct TeletextDrcsSubtableWire {
        pub identity: TeletextResourceIdentityWire,
        pub glyphs: Vec<TeletextDrcsGlyphDefinitionWire>,
    }
}

wire_string_enum! {
/// Preserve declared applicability without claiming Level 3.5 rendering.
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum TeletextX27LinkValidityWire {
    Level25Only,
    Level35Only,
    Level25And35,
}
}

wire_tagged_enum! {
/// An explicit FF target retains its magazine and is distinct from packet absence.
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TeletextX27LinkTargetWire {
    Unspecified { magazine: u8 },
    Page { page: TeletextRawDataPageNumberWire },
}
}

wire_object! {
    /// Complete X27 declaration; each mask bit retains its exact logical S1.
    pub struct TeletextX27LinkDeclarationWire {
        pub target: TeletextX27LinkTargetWire,
        pub validity: TeletextX27LinkValidityWire,
        pub required_subpages: u16,
    }
}

wire_object! {
    /// Fixed first four X27/4 roles; packet presence belongs to the page record.
    pub struct TeletextX27Level25LinksWire {
        pub gpop: TeletextX27LinkDeclarationWire,
        pub pop: TeletextX27LinkDeclarationWire,
        pub gdrcs: TeletextX27LinkDeclarationWire,
        pub drcs: TeletextX27LinkDeclarationWire,
    }
}

wire_object! {
    /// Independent global association flag and optional public slot, one through seven.
    pub struct TeletextMotAssociationSelectionWire {
        pub global: bool,
        #[serde(deserialize_with = "required_nullable")]
        pub public: Option<u8>,
    }
}

wire_object! {
    /// Raw association byte, including reserved FF, with independent object/DRCS selection.
    pub struct TeletextMotPageAssociationWire {
        pub page: u8,
        pub objects: TeletextMotAssociationSelectionWire,
        pub drcs: TeletextMotAssociationSelectionWire,
    }
}

wire_object! {
    /// Original coverage/count declarations, not evidence that resources are available.
    pub struct TeletextMotEnhancementCountsWire {
        pub magazine_mask: u8,
        pub object_pages: u8,
        pub drcs_pages: u8,
    }
}

wire_tagged_enum! {
/// Ordered MOT default slot, retaining inactive selector bytes separately from IDs.
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TeletextMotDefaultObjectWire {
    NoObject {
        raw_number: u8,
    },
    Dormant {
        object_type: TeletextObjectTypeWire,
        raw_number: u8,
    },
    Object {
        target: TeletextObjectIdentityWire,
        object_type: TeletextObjectTypeWire,
    },
}
}

wire_object! {
    /// MOT object link with contiguous zero-through-extent S1 and two ordered defaults.
    pub struct TeletextMotObjectLinkWire {
        pub target: TeletextX27LinkTargetWire,
        pub extent: u8,
        pub supersedable: bool,
        pub raw_fallback: u8,
        pub defaults: [TeletextMotDefaultObjectWire; 2],
    }
}

wire_object! {
    /// MOT DRCS link, retaining extent/flags even for an unused FF target.
    pub struct TeletextMotDrcsLinkWire {
        pub target: TeletextX27LinkTargetWire,
        pub extent: u8,
        pub supersedable: bool,
    }
}

wire_object! {
    /// Complete Level 2.5 MOT bank, preserving every fixed slot and raw count.
    pub struct TeletextMotLevel25LinksWire {
        pub global_object: TeletextMotObjectLinkWire,
        pub public_objects: [TeletextMotObjectLinkWire; 7],
        pub global_drcs: TeletextMotDrcsLinkWire,
        pub normal_drcs: [TeletextMotDrcsLinkWire; 7],
        pub counts: TeletextMotEnhancementCountsWire,
    }
}

wire_object! {
    /// Exact MOT identity and original ordered sparse associations; no inferred policy.
    pub struct TeletextMotLevel25DeclarationWire {
        pub identity: TeletextResourceIdentityWire,
        pub associations: Vec<TeletextMotPageAssociationWire>,
        pub links: TeletextMotLevel25LinksWire,
    }
}

wire_object! {
    /// Selection assertion checked against the actual unique same-magazine MOT.
    pub struct TeletextDisplayMotSelectionWire {
        #[serde(deserialize_with = "crate::teletext_pages_v2::deserialize_object")]
        pub display: TeletextPageAddressWire,
        #[serde(deserialize_with = "required_nullable")]
        pub mot: Option<TeletextResourceIdentityWire>,
    }
}

wire_object! {
    /// Supplied resolved object-link assertions, authenticated against MOT/X27 declarations.
    pub struct TeletextResolvedObjectPageLinksWire {
        #[serde(deserialize_with = "required_nullable")]
        pub public: Option<TeletextRawDataPageNumberWire>,
        #[serde(deserialize_with = "required_nullable")]
        pub global: Option<TeletextRawDataPageNumberWire>,
    }
}

wire_object! {
    /// Supplied resolved DRCS-link assertions; explicit null never grants acquisition.
    pub struct TeletextDrcsPageLinksWire {
        #[serde(deserialize_with = "required_nullable")]
        pub normal: Option<TeletextRawDataPageNumberWire>,
        #[serde(deserialize_with = "required_nullable")]
        pub global: Option<TeletextRawDataPageNumberWire>,
    }
}

wire_object! {
    /// Default assertion retaining the original resource owner and object behavior.
    pub struct TeletextDefaultObjectSelectionWire {
        pub target: TeletextObjectIdentityWire,
        pub object_type: TeletextObjectTypeWire,
    }
}

wire_object! {
    /// Authenticated selection assertions; defaults remain GPOP1/GPOP2/POP1/POP2.
    pub struct TeletextResolvedPageResourceSelectionWire {
        pub objects: TeletextResolvedObjectPageLinksWire,
        pub drcs: TeletextDrcsPageLinksWire,
        pub defaults: [Option<TeletextDefaultObjectSelectionWire>; 4],
    }
}
