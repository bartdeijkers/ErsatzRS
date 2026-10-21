//! Atomic raw X27 role selection and checked resource-consumer changes.
use super::{fence, no_mot_role, preflight};
use crate::teletext::{
    TeletextDeclaredService, TeletextDeclaredServiceBudget, TeletextDeclaredServiceError,
    TeletextDrcsExtensionError, TeletextDrcsPageLinks, TeletextDrcsTableScope,
    TeletextMotLevel25Resolution, TeletextMotResolutionError, TeletextMotRoleResolution,
    TeletextPageAddress, TeletextResolvedPageResourceSelection,
    TeletextResourceObjectProgramOverlay, TeletextServiceBudget, TeletextServiceError,
    TeletextX27Level25Links, TeletextX27LinkDeclaration, TeletextX27LinkError, TeletextX27LinkRole,
    TeletextX27LinkTarget,
};
#[path = "object_page_selection_consumers.rs"]
mod consumers;
pub use consumers::TeletextObjectPageSelectionConsumers;

/// Exact-original role/link intent; never host edit or publication authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextObjectPageSelectionChange {
    display: TeletextPageAddress,
    role: TeletextX27LinkRole,
    expected_links: Option<TeletextX27Level25Links>,
    expected_role: TeletextMotRoleResolution,
    replacement: TeletextX27LinkDeclaration,
    unselected_restoration: Option<UnselectedRestorationRoot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnselectedRestorationRoot {
    Absent,
    Explicit(TeletextX27Level25Links),
}

/// Atomic selection rejection retains no partially reconstructed service.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TeletextObjectPageSelectionError {
    #[error("object page selection requires at least one role change")]
    Empty,
    #[error("object page selection requires a role or consumer change")]
    EmptyConsumers,
    #[error("object page selection requires a changed Level 2.5 page target")]
    InvalidChange,
    #[error("object page selection repeats {display:?}, {role:?}")]
    Duplicate {
        display: TeletextPageAddress,
        role: TeletextX27LinkRole,
    },
    #[error("object page selection display {display:?} is absent")]
    MissingDisplay { display: TeletextPageAddress },
    #[error("object page selection original declarations differ at {display:?}, {role:?}")]
    OriginalMismatch {
        display: TeletextPageAddress,
        role: TeletextX27LinkRole,
    },
    #[error("object page selection resource is absent at {display:?}, {role:?}")]
    MissingResource {
        display: TeletextPageAddress,
        role: TeletextX27LinkRole,
    },
    #[error("object page selection lacks four-role context at {display:?}")]
    UnprovenRoles { display: TeletextPageAddress },
    #[error("object page selection does not resolve the replacement at {display:?}, {role:?}")]
    Unresolved {
        display: TeletextPageAddress,
        role: TeletextX27LinkRole,
    },
    #[error(transparent)]
    Service(#[from] TeletextServiceError),
    #[error(transparent)]
    Declaration(#[from] TeletextDeclaredServiceError),
    #[error(transparent)]
    Link(#[from] TeletextX27LinkError),
    #[error(transparent)]
    Resolution(#[from] TeletextMotResolutionError),
}

impl TeletextObjectPageSelectionChange {
    /// Bind an exact original X27 body/presence and selected-role provenance.
    ///
    /// Replacement selects a different raw page with its explicit required S1
    /// mask. Mask-only additions retain the separate announcement operation.
    /// # Errors
    /// Rejects unselected/unresolved originals, same-page targets,
    /// unspecified replacements and original/replacement Level 3.5-only links.
    pub fn new(
        display: TeletextPageAddress,
        role: TeletextX27LinkRole,
        expected_links: Option<TeletextX27Level25Links>,
        expected_role: TeletextMotRoleResolution,
        replacement: TeletextX27LinkDeclaration,
    ) -> Result<Self, TeletextObjectPageSelectionError> {
        if !replacement.validity().applies_at_level25()
            || expected_links.is_some_and(|links| {
                !links.link(role).validity().applies_at_level25()
                    || links.link(role).validity() != replacement.validity()
            })
            || !matches!((expected_role, replacement.target()),
                (TeletextMotRoleResolution::Selected { target: old, .. }, TeletextX27LinkTarget::Page { page: new }) if old != new)
        {
            return Err(TeletextObjectPageSelectionError::InvalidChange);
        }
        Ok(Self {
            display,
            role,
            expected_links,
            expected_role,
            replacement,
            unselected_restoration: None,
        })
    }

    /// Select an existing retained bank for an exactly unselected display role.
    ///
    /// This is separate from selected-page retargeting. The complete service
    /// still authenticates the original resolution, bank kind and final graph.
    /// # Errors
    /// Rejects selected explicit originals, inapplicable declarations and changes
    /// to explicit link validity. Absent originals use the canonical Level 2.5
    /// materialization validity.
    pub fn new_attachment(
        display: TeletextPageAddress,
        role: TeletextX27LinkRole,
        expected_links: Option<TeletextX27Level25Links>,
        replacement: TeletextX27LinkDeclaration,
    ) -> Result<Self, TeletextObjectPageSelectionError> {
        let original = expected_links.map(|links| links.link(role));
        if !replacement.validity().applies_at_level25()
            || !matches!(replacement.target(), TeletextX27LinkTarget::Page { .. })
            || original.is_some_and(|link| {
                !link.validity().applies_at_level25()
                    || link.validity() != replacement.validity()
                    || !matches!(link.target(), TeletextX27LinkTarget::Unspecified { .. })
            })
            || original.is_none()
                && replacement.validity() != super::super::TeletextX27LinkValidity::Level25Only
        {
            return Err(TeletextObjectPageSelectionError::InvalidChange);
        }
        Ok(Self {
            display,
            role,
            expected_links,
            expected_role: TeletextMotRoleResolution::Unselected,
            replacement,
            unselected_restoration: None,
        })
    }

    /// Describe native cumulative undo of a first attachment to its exact root.
    ///
    /// This transient marker is not domain rebuild or saved-edit authority. The
    /// native fold must match its retained original attachment before removing
    /// that cumulative intent and reconstructing the complete root generation.
    /// # Errors
    /// Rejects nonattachment originals, nested restoration and stale or malformed
    /// selected declarations/provenance. The current role must exactly describe
    /// the attachment's current replacement, including its mask and validity.
    pub fn new_unselected_restoration(
        original_attachment: Self,
        current_links: Option<TeletextX27Level25Links>,
        current_selected_role: TeletextMotRoleResolution,
    ) -> Result<Self, TeletextObjectPageSelectionError> {
        use super::super::{
            TeletextMotLinkProvenance, TeletextMotResolutionRequirements,
            TeletextSubpageRequirementMask, TeletextX27LinkValidity, TeletextX27Magazine,
        };
        let display = original_attachment.display;
        let role = original_attachment.role;
        let Some(current) = current_links.map(|links| links.link(role)) else {
            return Err(TeletextObjectPageSelectionError::InvalidChange);
        };
        let TeletextX27LinkTarget::Page { page: target } = current.target() else {
            return Err(TeletextObjectPageSelectionError::InvalidChange);
        };
        if original_attachment.is_unselected_restoration()
            || original_attachment.expected_role != TeletextMotRoleResolution::Unselected
            || current != original_attachment.replacement
            || current_selected_role
                != (TeletextMotRoleResolution::Selected {
                    target,
                    requirements: TeletextMotResolutionRequirements::X27Mask(
                        current.required_subpages(),
                    ),
                    provenance: TeletextMotLinkProvenance::X27 { display, role },
                    original_mot: None,
                })
        {
            return Err(TeletextObjectPageSelectionError::InvalidChange);
        }
        let replacement = match original_attachment.expected_links {
            Some(links) => links.link(role),
            None => TeletextX27LinkDeclaration::new(
                TeletextX27LinkTarget::Unspecified {
                    magazine: TeletextX27Magazine::new(display.page.magazine())?,
                },
                TeletextX27LinkValidity::Level25Only,
                TeletextSubpageRequirementMask::new(u16::MAX),
            )?,
        };
        Ok(Self {
            display,
            role,
            expected_links: current_links,
            expected_role: current_selected_role,
            replacement,
            unselected_restoration: Some(match original_attachment.expected_links {
                Some(links) => UnselectedRestorationRoot::Explicit(links),
                None => UnselectedRestorationRoot::Absent,
            }),
        })
    }

    /// Whether this is transient native root-undo intent, never a direct rebuild.
    #[must_use]
    pub const fn is_unselected_restoration(&self) -> bool {
        self.unselected_restoration.is_some()
    }

    /// Exact root X27 presence/body bound by transient native restoration intent.
    #[must_use]
    pub const fn restored_unselected_links(&self) -> Option<Option<TeletextX27Level25Links>> {
        match self.unselected_restoration {
            Some(UnselectedRestorationRoot::Absent) => Some(None),
            Some(UnselectedRestorationRoot::Explicit(links)) => Some(Some(links)),
            None => None,
        }
    }
    #[must_use]
    pub const fn display(&self) -> TeletextPageAddress {
        self.display
    }
    #[must_use]
    pub const fn role(&self) -> TeletextX27LinkRole {
        self.role
    }
    #[must_use]
    pub const fn expected_links(&self) -> Option<TeletextX27Level25Links> {
        self.expected_links
    }
    #[must_use]
    pub const fn expected_role(&self) -> TeletextMotRoleResolution {
        self.expected_role
    }
    #[must_use]
    pub const fn replacement(&self) -> TeletextX27LinkDeclaration {
        self.replacement
    }
}

impl TeletextDeclaredService {
    /// Change raw object/DRCS-page roles and checked resource consumers atomically.
    ///
    /// The final draft alone undergoes complete semantic collection and declared
    /// authentication. Original catalogue identities/types, MOTs, defaults and
    /// selections remain intact. Every shared consumer must work under the final
    /// roles; no intermediate incomplete graph escapes. Hosts still authenticate
    /// source ownership, protected effects, physical feasibility and publication.
    /// # Errors
    /// Rejects empty/repeated/stale role intent, absent targets, Level 3.5-only or
    /// non-supersedable roles, incomplete final dependencies and all trusted bounds.
    pub fn with_object_page_selections_and_program_overlays(
        self,
        changes: &[TeletextObjectPageSelectionChange],
        overlays: &[TeletextResourceObjectProgramOverlay],
        service_budget: TeletextServiceBudget,
        declaration_budget: TeletextDeclaredServiceBudget,
    ) -> Result<Self, TeletextObjectPageSelectionError> {
        if changes.is_empty() {
            return Err(TeletextObjectPageSelectionError::Empty);
        }
        self.with_object_page_selections_and_consumers(
            changes,
            TeletextObjectPageSelectionConsumers {
                resources: overlays,
                ..Default::default()
            },
            service_budget,
            declaration_budget,
        )
    }

    fn object_page_selection_update(
        &self,
        change: TeletextObjectPageSelectionChange,
        prior: Option<(
            TeletextX27Level25Links,
            TeletextResolvedPageResourceSelection,
        )>,
    ) -> Result<
        (
            TeletextX27Level25Links,
            TeletextResolvedPageResourceSelection,
        ),
        TeletextObjectPageSelectionError,
    > {
        use TeletextObjectPageSelectionError as Error;
        let display = change.display;
        let role = change.role;
        let resolution = self
            .resolutions
            .iter()
            .find(|value| value.display() == display)
            .ok_or(Error::MissingDisplay { display })?;
        let metadata = self
            .semantic
            .page_enhancements()
            .get(&display)
            .ok_or(Error::MissingDisplay { display })?;
        if metadata.compositional_links != change.expected_links
            || *resolution.role(role) != change.expected_role
        {
            return Err(Error::OriginalMismatch { display, role });
        }
        let TeletextX27LinkTarget::Page { page: target } = change.replacement.target() else {
            return Err(Error::InvalidChange);
        };
        let kind = role.kind();
        if !self
            .resource_inventory
            .iter()
            .any(|resource| resource.kind() == kind && resource.page() == target)
        {
            return Err(Error::MissingResource { display, role });
        }
        let (links, mut resources) = match prior {
            Some(update) => update,
            None => (
                match metadata.compositional_links {
                    Some(links) => links,
                    None => (if change.expected_role == TeletextMotRoleResolution::Unselected {
                        resolution.materialized_level25_attachment_links()
                    } else {
                        resolution.materialized_level25_links()
                    })
                    .map_err(|error| match error {
                        TeletextDrcsExtensionError::Link(error) => Error::Link(error),
                        _ => Error::UnprovenRoles { display },
                    })?,
                },
                metadata.resources,
            ),
        };
        if links.link(role).validity() != change.replacement.validity() {
            return Err(Error::InvalidChange);
        }
        let mut roles = TeletextX27LinkRole::ALL.map(|role| links.link(role));
        roles[usize::from(role.wire_code())] = change.replacement;
        let links = TeletextX27Level25Links::new(roles[0], roles[1], roles[2], roles[3])?;
        let effective = if let Some(original_mot) = resolution.mot() {
            let mot = self
                .mots
                .iter()
                .find(|mot| mot.identity() == original_mot.selected_mot())
                .ok_or(Error::UnprovenRoles { display })?;
            *TeletextMotLevel25Resolution::resolve(mot, display, Some(links))?.role(role)
        } else {
            no_mot_role(Some(links), display, role)
        };
        if !matches!(effective, TeletextMotRoleResolution::Selected { target: actual, .. } if actual == target)
        {
            return Err(Error::Unresolved { display, role });
        }
        match role {
            TeletextX27LinkRole::Pop => resources.objects.public = Some(target),
            TeletextX27LinkRole::Gpop => resources.objects.global = Some(target),
            TeletextX27LinkRole::Drcs | TeletextX27LinkRole::Gdrcs => {
                let normal = if role == TeletextX27LinkRole::Drcs {
                    Some(target)
                } else {
                    resources.drcs.page(TeletextDrcsTableScope::Normal)
                };
                let global = if role == TeletextX27LinkRole::Gdrcs {
                    Some(target)
                } else {
                    resources.drcs.page(TeletextDrcsTableScope::Global)
                };
                resources.drcs = TeletextDrcsPageLinks::new(normal, global)
                    .map_err(|error| TeletextServiceError::Drcs(error.into()))?;
            }
        }
        Ok((links, resources))
    }

    fn check_object_page_selection_budgets(
        &self,
        changes: &[TeletextObjectPageSelectionChange],
        overlays: &[TeletextResourceObjectProgramOverlay],
        service_budget: TeletextServiceBudget,
        declaration_budget: TeletextDeclaredServiceBudget,
    ) -> Result<(), TeletextObjectPageSelectionError> {
        // Native undo must first remove the retained cumulative attachment;
        // transient restoration never authorizes a direct partial rebuild.
        if changes
            .iter()
            .any(TeletextObjectPageSelectionChange::is_unselected_restoration)
        {
            return Err(TeletextObjectPageSelectionError::InvalidChange);
        }
        // Bound all raw intents/maps and both program bodies before indexing or cloning.
        let mut items = service_budget.max_structure_items;
        super::super::service::charge(
            &mut items,
            changes.len(),
            "structure item",
            service_budget.max_structure_items,
        )?;
        for overlay in overlays {
            super::super::service::charge(
                &mut items,
                overlay.retained_commands().len(),
                "structure item",
                service_budget.max_structure_items,
            )?;
        }
        self.semantic
            .check_resource_program_inputs(overlays, service_budget)?;
        preflight(
            &self.semantic,
            &self.mots,
            &self.selections,
            &self.empty_object_pages,
            declaration_budget,
        )?;
        fence(
            "resource",
            self.resource_inventory.len(),
            declaration_budget.max_resources,
        )?;
        Ok(())
    }
}
