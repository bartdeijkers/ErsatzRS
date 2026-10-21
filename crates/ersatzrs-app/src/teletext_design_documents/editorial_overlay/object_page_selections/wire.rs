//! Closed selected-role/attachment wire, separate from trusted domain results.
use super::super::super::Entry;
use super::super::map_only;
use super::{invalid, TeletextDesignDocumentError, TeletextSavedObjectPageSelectionChange};
use ersatzrs_domain::{
    TeletextMotLinkProvenance, TeletextMotLinkRole, TeletextMotPublicLinkIndex,
    TeletextMotResolutionCandidate, TeletextMotResolutionRequirements, TeletextMotRoleResolution,
    TeletextMotSubpageExtent, TeletextObjectPageSelectionChange, TeletextPageAddress,
    TeletextRawDataPageNumber, TeletextResourceIdentity, TeletextSubpageRequirementMask,
    TeletextX27Level25Links, TeletextX27LinkDeclaration, TeletextX27LinkRole,
};
use serde::{Deserialize, Deserializer, Serialize};
macro_rules! record {
    ($name:ident { $($field:ident: $ty:ty),* $(,)? }) => {
        #[derive(Serialize)]
        pub(in super::super::super) struct $name { $($field: $ty),* }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                #[derive(Deserialize)] #[serde(deny_unknown_fields)]
                struct Fields { $($field: $ty),* }
                let value: Fields = map_only(d)?;
                Ok(Self { $($field: value.$field),* })
            }
        }
    };
}
record!(StoredSelection {
    document_index: usize,
    display: Entry,
    role: X27Role,
    expected_links: Vec<TeletextX27Level25Links>,
    expected_role: ExpectedRole,
    replacement: TeletextX27LinkDeclaration,
});
// Untagged preserves the historical selected-role object exactly; the new
// branch accepts only its sealed marker, never a trusted resolution enum.
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum ExpectedRole {
    Selected(SelectedRole),
    Unselected(UnselectedRole),
}
record!(UnselectedRole { unselected: bool });
record!(SelectedRole {
    target: TeletextRawDataPageNumber,
    requirements: Requirements,
    provenance: Provenance,
    original_mot: Vec<Candidate>,
});
record!(Candidate {
    target: TeletextRawDataPageNumber,
    extent: TeletextMotSubpageExtent,
    supersedable: bool,
    provenance: Provenance,
});
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum X27Role {
    Gpop,
    Pop,
    Gdrcs,
    Drcs,
}
impl From<TeletextX27LinkRole> for X27Role {
    fn from(value: TeletextX27LinkRole) -> Self {
        match value {
            TeletextX27LinkRole::Gpop => Self::Gpop,
            TeletextX27LinkRole::Pop => Self::Pop,
            TeletextX27LinkRole::Gdrcs => Self::Gdrcs,
            TeletextX27LinkRole::Drcs => Self::Drcs,
        }
    }
}
impl From<X27Role> for TeletextX27LinkRole {
    fn from(value: X27Role) -> Self {
        match value {
            X27Role::Gpop => Self::Gpop,
            X27Role::Pop => Self::Pop,
            X27Role::Gdrcs => Self::Gdrcs,
            X27Role::Drcs => Self::Drcs,
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Requirements {
    MotExtent {
        value: TeletextMotSubpageExtent,
    },
    X27Mask {
        value: TeletextSubpageRequirementMask,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum MotRole {
    Gpop,
    Pop { index: TeletextMotPublicLinkIndex },
    Gdrcs,
    Drcs { index: TeletextMotPublicLinkIndex },
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Provenance {
    Mot {
        identity: TeletextResourceIdentity,
        role: MotRole,
    },
    X27 {
        display: Entry,
        role: X27Role,
    },
    MotPageWithX27Mask {
        identity: TeletextResourceIdentity,
        role: MotRole,
        display: Entry,
    },
}
fn entry(display: TeletextPageAddress) -> Entry {
    Entry {
        page: display.page,
        subcode: display.subcode,
    }
}
fn address(display: &Entry) -> TeletextPageAddress {
    TeletextPageAddress {
        page: display.page,
        subcode: display.subcode,
    }
}
impl From<TeletextMotLinkRole> for MotRole {
    fn from(value: TeletextMotLinkRole) -> Self {
        match value {
            TeletextMotLinkRole::Gpop => Self::Gpop,
            TeletextMotLinkRole::Pop { index } => Self::Pop { index },
            TeletextMotLinkRole::Gdrcs => Self::Gdrcs,
            TeletextMotLinkRole::Drcs { index } => Self::Drcs { index },
        }
    }
}
impl From<MotRole> for TeletextMotLinkRole {
    fn from(value: MotRole) -> Self {
        match value {
            MotRole::Gpop => Self::Gpop,
            MotRole::Pop { index } => Self::Pop { index },
            MotRole::Gdrcs => Self::Gdrcs,
            MotRole::Drcs { index } => Self::Drcs { index },
        }
    }
}
impl From<TeletextMotLinkProvenance> for Provenance {
    fn from(value: TeletextMotLinkProvenance) -> Self {
        match value {
            TeletextMotLinkProvenance::Mot { identity, role } => Self::Mot {
                identity,
                role: role.into(),
            },
            TeletextMotLinkProvenance::X27 { display, role } => Self::X27 {
                display: entry(display),
                role: role.into(),
            },
            TeletextMotLinkProvenance::MotPageWithX27Mask {
                identity,
                role,
                display,
            } => Self::MotPageWithX27Mask {
                identity,
                role: role.into(),
                display: entry(display),
            },
        }
    }
}
impl From<Provenance> for TeletextMotLinkProvenance {
    fn from(value: Provenance) -> Self {
        match value {
            Provenance::Mot { identity, role } => Self::Mot {
                identity,
                role: role.into(),
            },
            Provenance::X27 { display, role } => Self::X27 {
                display: address(&display),
                role: role.into(),
            },
            Provenance::MotPageWithX27Mask {
                identity,
                role,
                display,
            } => Self::MotPageWithX27Mask {
                identity,
                role: role.into(),
                display: address(&display),
            },
        }
    }
}
impl From<TeletextMotResolutionCandidate> for Candidate {
    fn from(value: TeletextMotResolutionCandidate) -> Self {
        Self {
            target: value.target,
            extent: value.extent,
            supersedable: value.supersedable,
            provenance: value.provenance.into(),
        }
    }
}
impl From<Candidate> for TeletextMotResolutionCandidate {
    fn from(value: Candidate) -> Self {
        Self {
            target: value.target,
            extent: value.extent,
            supersedable: value.supersedable,
            provenance: value.provenance.into(),
        }
    }
}
impl StoredSelection {
    pub(super) const fn is_attachment(&self) -> bool {
        matches!(self.expected_role, ExpectedRole::Unselected(_))
    }
    pub(super) const fn is_drcs(&self) -> bool {
        matches!(self.role, X27Role::Drcs | X27Role::Gdrcs)
    }
}
impl From<TeletextSavedObjectPageSelectionChange> for StoredSelection {
    fn from(value: TeletextSavedObjectPageSelectionChange) -> Self {
        let change = value.change();
        let expected_role = match change.expected_role() {
            TeletextMotRoleResolution::Unselected => {
                ExpectedRole::Unselected(UnselectedRole { unselected: true })
            }
            TeletextMotRoleResolution::Selected {
                target,
                requirements,
                provenance,
                original_mot,
            } => ExpectedRole::Selected(SelectedRole {
                target,
                requirements: match requirements {
                    TeletextMotResolutionRequirements::MotExtent(value) => {
                        Requirements::MotExtent { value }
                    }
                    TeletextMotResolutionRequirements::X27Mask(value) => {
                        Requirements::X27Mask { value }
                    }
                },
                provenance: provenance.into(),
                original_mot: original_mot.into_iter().map(Into::into).collect(),
            }),
            TeletextMotRoleResolution::Unresolved { .. } => {
                unreachable!("typed saved change requires a selected or unselected original")
            }
        };
        Self {
            document_index: value.document_index(),
            display: entry(change.display()),
            role: change.role().into(),
            expected_links: change.expected_links().into_iter().collect(),
            expected_role,
            replacement: change.replacement(),
        }
    }
}
impl TryFrom<StoredSelection> for TeletextSavedObjectPageSelectionChange {
    type Error = TeletextDesignDocumentError;
    fn try_from(value: StoredSelection) -> Result<Self, Self::Error> {
        if value.expected_links.len() > 1 {
            return Err(invalid());
        }
        let display = address(&value.display);
        let role = value.role.into();
        let links = value.expected_links.into_iter().next();
        let change = match value.expected_role {
            ExpectedRole::Unselected(marker) => {
                if !marker.unselected {
                    return Err(invalid());
                }
                TeletextObjectPageSelectionChange::new_attachment(
                    display,
                    role,
                    links,
                    value.replacement,
                )
            }
            ExpectedRole::Selected(original) => {
                if original.original_mot.len() > 1 {
                    return Err(invalid());
                }
                let expected = TeletextMotRoleResolution::Selected {
                    target: original.target,
                    requirements: match original.requirements {
                        Requirements::MotExtent { value } => {
                            TeletextMotResolutionRequirements::MotExtent(value)
                        }
                        Requirements::X27Mask { value } => {
                            TeletextMotResolutionRequirements::X27Mask(value)
                        }
                    },
                    provenance: original.provenance.into(),
                    original_mot: original.original_mot.into_iter().next().map(Into::into),
                };
                TeletextObjectPageSelectionChange::new(
                    display,
                    role,
                    links,
                    expected,
                    value.replacement,
                )
            }
        }
        .map_err(|_| invalid())?;
        Self::new(value.document_index, change)
    }
}
