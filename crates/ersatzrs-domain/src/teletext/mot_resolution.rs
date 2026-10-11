//! Bounded, per-display MOT/X27 declaration derivation, EN 300 706 clauses
//! 9.6.2 and 10.6. This is neither resource acquisition nor decoder readiness.
//!
//! Conflicting declarations remain unresolved with their original candidates.
//! Default ownership and conditional POP fallback remain independent of link
//! selection. Local X26 and X28/M29 presence are not inputs, so no effective
//! default suppression or presentation is decided here. The immutable authored
//! MOT declaration remains authoritative; this result is not its roundtrip form.

use super::{
    TeletextMotDefaultObject, TeletextMotFallback, TeletextMotLevel25Declaration,
    TeletextMotLevel25Links, TeletextMotLinkRole, TeletextMotObjectLink,
    TeletextMotPageAssociation, TeletextMotSubpageExtent, TeletextPageAddress,
    TeletextRawDataPageNumber, TeletextResourceIdentity, TeletextSubpageRequirementMask,
    TeletextX27Level25Links, TeletextX27LinkDeclaration, TeletextX27LinkRole,
    TeletextX27LinkTarget,
};

/// Only an explicitly selected MOT belonging to the display magazine is accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextMotResolutionError {
    #[error("teletext display {display:?} is outside selected MOT {mot:?} magazine")]
    MagazineOwner {
        display: TeletextPageAddress,
        mot: TeletextResourceIdentity,
    },
}

/// Exact source of selected declarations, including the selected MOT logical S1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextMotLinkProvenance {
    Mot {
        identity: TeletextResourceIdentity,
        role: TeletextMotLinkRole,
    },
    X27 {
        display: TeletextPageAddress,
        role: TeletextX27LinkRole,
    },
    /// A non-supersedable MOT page remains the owner; X27 supplies the exact mask.
    MotPageWithX27Mask {
        identity: TeletextResourceIdentity,
        role: TeletextMotLinkRole,
        display: TeletextPageAddress,
    },
}

/// These distinct declarations are never implicitly intersected or expanded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextMotResolutionRequirements {
    MotExtent(TeletextMotSubpageExtent),
    X27Mask(TeletextSubpageRequirementMask),
}

/// Original active associated MOT candidate, retained even after X27 selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextMotResolutionCandidate {
    pub target: TeletextRawDataPageNumber,
    pub extent: TeletextMotSubpageExtent,
    pub supersedable: bool,
    pub provenance: TeletextMotLinkProvenance,
}

/// Legal authored combinations for which no precedence is silently invented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextMotUnresolvedReason {
    NonSupersedablePageConflict,
    UnspecifiedOverAssociatedMot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextMotRoleResolution {
    Unselected,
    Selected {
        target: TeletextRawDataPageNumber,
        requirements: TeletextMotResolutionRequirements,
        provenance: TeletextMotLinkProvenance,
        original_mot: Option<TeletextMotResolutionCandidate>,
    },
    Unresolved {
        reason: TeletextMotUnresolvedReason,
        mot: TeletextMotResolutionCandidate,
        x27: TeletextX27LinkDeclaration,
    },
}

/// Original ordered slots, without asserting that any default is effective.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextMotDefaultSource {
    pub provenance: TeletextMotLinkProvenance,
    pub link: TeletextMotObjectLink,
}

/// POP flags whose effect still depends on presentation packets not supplied here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextMotConditionalPopFallback {
    pub provenance: TeletextMotLinkProvenance,
    pub flags: TeletextMotFallback,
}

/// An active original GPOP default conflicts with cleared global association.
/// The zero-based slot and declaration remain visible; no default is applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextMotGlobalDefaultConflict {
    pub slot: u8,
    pub provenance: TeletextMotLinkProvenance,
    pub declaration: TeletextMotDefaultObject,
}

/// Trusted derivation with bounded, copied declaration context; deliberately no serde.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextMotLevel25Resolution {
    selected_mot: TeletextResourceIdentity,
    display: TeletextPageAddress,
    association: Option<TeletextMotPageAssociation>,
    mot_links: TeletextMotLevel25Links,
    x27: Option<TeletextX27Level25Links>,
    roles: [TeletextMotRoleResolution; 4],
    default_sources: [Option<TeletextMotDefaultSource>; 2],
    conditional_pop_fallback: Option<TeletextMotConditionalPopFallback>,
    default_conflicts: [Option<TeletextMotGlobalDefaultConflict>; 2],
}

impl TeletextMotLevel25Resolution {
    /// # Errors
    /// Rejects a display outside the explicitly selected MOT's magazine.
    /// A missing sparse association means no MOT selection in this authored model.
    pub fn resolve(
        mot: &TeletextMotLevel25Declaration,
        display: TeletextPageAddress,
        x27: Option<TeletextX27Level25Links>,
    ) -> Result<Self, TeletextMotResolutionError> {
        let identity = mot.identity();
        if identity.page().magazine() != display.page.magazine() {
            return Err(TeletextMotResolutionError::MagazineOwner {
                display,
                mot: identity,
            });
        }
        let association = mot.association(display.page.wire_page());
        let links = mot.links();
        let roles = TeletextX27LinkRole::ALL.map(|role| {
            resolve_role(
                candidate(identity, links, association, role),
                x27.map(|links| links.link(role)),
                display,
                role,
            )
        });
        let global_provenance = TeletextMotLinkProvenance::Mot {
            identity,
            role: TeletextMotLinkRole::Gpop,
        };
        let global = TeletextMotDefaultSource {
            provenance: global_provenance,
            link: links.object_link(None),
        };
        let public = association
            .and_then(|association| association.objects().public())
            .map(|index| TeletextMotDefaultSource {
                provenance: TeletextMotLinkProvenance::Mot {
                    identity,
                    role: TeletextMotLinkRole::Pop { index },
                },
                link: links.object_link(Some(index)),
            });
        let conditional_pop_fallback = public.map(|source| TeletextMotConditionalPopFallback {
            provenance: source.provenance,
            flags: source.link.raw_fallback(),
        });
        let global_associated =
            association.is_some_and(|association| association.objects().global());
        let mut default_conflicts = [None; 2];
        if !global_associated {
            for (slot, declaration) in (0_u8..2).zip(global.link.defaults()) {
                if matches!(declaration, TeletextMotDefaultObject::Object { .. }) {
                    default_conflicts[usize::from(slot)] = Some(TeletextMotGlobalDefaultConflict {
                        slot,
                        provenance: global_provenance,
                        declaration,
                    });
                }
            }
        }
        Ok(Self {
            selected_mot: identity,
            display,
            association,
            mot_links: links,
            x27,
            roles,
            default_sources: [Some(global), public],
            conditional_pop_fallback,
            default_conflicts,
        })
    }

    #[must_use]
    pub const fn selected_mot(&self) -> TeletextResourceIdentity {
        self.selected_mot
    }
    #[must_use]
    pub const fn display(&self) -> TeletextPageAddress {
        self.display
    }
    #[must_use]
    pub const fn association(&self) -> Option<TeletextMotPageAssociation> {
        self.association
    }
    /// Full original checked bank, including unassociated links and ignored fields.
    #[must_use]
    pub const fn mot_links(&self) -> TeletextMotLevel25Links {
        self.mot_links
    }
    #[must_use]
    pub const fn x27(&self) -> Option<TeletextX27Level25Links> {
        self.x27
    }
    #[must_use]
    pub const fn role(&self, role: TeletextX27LinkRole) -> &TeletextMotRoleResolution {
        &self.roles[role.wire_code() as usize]
    }
    /// Global then associated public source; None is an absent public association.
    /// An explicitly associated unused FF POP link remains Some with dormant slots.
    #[must_use]
    pub const fn default_sources(&self) -> &[Option<TeletextMotDefaultSource>; 2] {
        &self.default_sources
    }
    #[must_use]
    pub const fn conditional_pop_fallback(&self) -> Option<TeletextMotConditionalPopFallback> {
        self.conditional_pop_fallback
    }
    #[must_use]
    pub const fn default_conflicts(&self) -> &[Option<TeletextMotGlobalDefaultConflict>; 2] {
        &self.default_conflicts
    }
}

fn candidate(
    identity: TeletextResourceIdentity,
    links: TeletextMotLevel25Links,
    association: Option<TeletextMotPageAssociation>,
    role: TeletextX27LinkRole,
) -> Option<TeletextMotResolutionCandidate> {
    let association = association?;
    let (target, extent, supersedable, mot_role) = match role {
        TeletextX27LinkRole::Gpop if association.objects().global() => {
            let link = links.object_link(None);
            (
                link.target(),
                link.extent(),
                link.supersedable(),
                TeletextMotLinkRole::Gpop,
            )
        }
        TeletextX27LinkRole::Pop => {
            let index = association.objects().public()?;
            let link = links.object_link(Some(index));
            (
                link.target(),
                link.extent(),
                link.supersedable(),
                TeletextMotLinkRole::Pop { index },
            )
        }
        TeletextX27LinkRole::Gdrcs if association.drcs().global() => {
            let link = links.drcs_link(None);
            (
                link.target(),
                link.extent(),
                link.supersedable(),
                TeletextMotLinkRole::Gdrcs,
            )
        }
        TeletextX27LinkRole::Drcs => {
            let index = association.drcs().public()?;
            let link = links.drcs_link(Some(index));
            (
                link.target(),
                link.extent(),
                link.supersedable(),
                TeletextMotLinkRole::Drcs { index },
            )
        }
        _ => return None,
    };
    let TeletextX27LinkTarget::Page { page } = target else {
        return None;
    };
    Some(TeletextMotResolutionCandidate {
        target: page,
        extent,
        supersedable,
        provenance: TeletextMotLinkProvenance::Mot {
            identity,
            role: mot_role,
        },
    })
}

fn resolve_role(
    mot: Option<TeletextMotResolutionCandidate>,
    x27: Option<TeletextX27LinkDeclaration>,
    display: TeletextPageAddress,
    role: TeletextX27LinkRole,
) -> TeletextMotRoleResolution {
    let applicable = x27.filter(|link| link.validity().applies_at_level25());
    match (mot, applicable) {
        (None, Some(link)) => match link.target() {
            TeletextX27LinkTarget::Page { page } => TeletextMotRoleResolution::Selected {
                target: page,
                requirements: TeletextMotResolutionRequirements::X27Mask(link.required_subpages()),
                provenance: TeletextMotLinkProvenance::X27 { display, role },
                original_mot: None,
            },
            TeletextX27LinkTarget::Unspecified { .. } => TeletextMotRoleResolution::Unselected,
        },
        (None, None) => TeletextMotRoleResolution::Unselected,
        (Some(mot), None) => TeletextMotRoleResolution::Selected {
            target: mot.target,
            requirements: TeletextMotResolutionRequirements::MotExtent(mot.extent),
            provenance: mot.provenance,
            original_mot: Some(mot),
        },
        (Some(mot), Some(link)) => match link.target() {
            TeletextX27LinkTarget::Unspecified { .. } => TeletextMotRoleResolution::Unresolved {
                reason: TeletextMotUnresolvedReason::UnspecifiedOverAssociatedMot,
                mot,
                x27: link,
            },
            TeletextX27LinkTarget::Page { page } if mot.supersedable => {
                TeletextMotRoleResolution::Selected {
                    target: page,
                    requirements: TeletextMotResolutionRequirements::X27Mask(
                        link.required_subpages(),
                    ),
                    provenance: TeletextMotLinkProvenance::X27 { display, role },
                    original_mot: Some(mot),
                }
            }
            TeletextX27LinkTarget::Page { page } if page == mot.target => {
                let TeletextMotLinkProvenance::Mot {
                    identity,
                    role: mot_role,
                } = mot.provenance
                else {
                    // Candidate construction above always records an original MOT source.
                    return TeletextMotRoleResolution::Unresolved {
                        reason: TeletextMotUnresolvedReason::NonSupersedablePageConflict,
                        mot,
                        x27: link,
                    };
                };
                TeletextMotRoleResolution::Selected {
                    target: page,
                    requirements: TeletextMotResolutionRequirements::X27Mask(
                        link.required_subpages(),
                    ),
                    provenance: TeletextMotLinkProvenance::MotPageWithX27Mask {
                        identity,
                        role: mot_role,
                        display,
                    },
                    original_mot: Some(mot),
                }
            }
            TeletextX27LinkTarget::Page { .. } => TeletextMotRoleResolution::Unresolved {
                reason: TeletextMotUnresolvedReason::NonSupersedablePageConflict,
                mot,
                x27: link,
            },
        },
    }
}
