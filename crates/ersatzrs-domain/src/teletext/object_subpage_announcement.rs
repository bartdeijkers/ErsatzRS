//! Explicit additive S1 requirements on the already selected POP/GPOP raw page.
use super::{fence, preflight};
use crate::teletext::{
    TeletextDeclaredService, TeletextDeclaredServiceBudget, TeletextDeclaredServiceError,
    TeletextDrcsExtensionError, TeletextMotRoleResolution, TeletextPageAddress,
    TeletextResourceIdentity, TeletextResourceKind, TeletextServiceBudget, TeletextServiceError,
    TeletextSubpageRequirementMask, TeletextX27Level25Links, TeletextX27LinkDeclaration,
    TeletextX27LinkError, TeletextX27LinkRole, TeletextX27LinkTarget,
};
use std::collections::BTreeMap;

/// One intentional missing S1 announcement; never implicit bank creation or selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextObjectSubpageAnnouncement {
    display: TeletextPageAddress,
    resource: TeletextResourceIdentity,
}

/// Rejection leaves no partially announced service available to the caller.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TeletextObjectSubpageAnnouncementError {
    #[error("object subpage announcements must not be empty")]
    Empty,
    #[error("object subpage announcement requires POP/GPOP, received {resource:?}")]
    Kind { resource: TeletextResourceIdentity },
    #[error("object subpage announcement repeats {display:?}, {resource:?}")]
    Duplicate {
        display: TeletextPageAddress,
        resource: TeletextResourceIdentity,
    },
    #[error("object subpage announcement display {display:?} is absent")]
    MissingDisplay { display: TeletextPageAddress },
    #[error("object subpage announcement resource {resource:?} is absent")]
    MissingResource { resource: TeletextResourceIdentity },
    #[error("object subpage announcement {display:?} does not select {resource:?}")]
    Owner {
        display: TeletextPageAddress,
        resource: TeletextResourceIdentity,
    },
    #[error("object subpage announcement {display:?}, {resource:?} is already required")]
    AlreadyAnnounced {
        display: TeletextPageAddress,
        resource: TeletextResourceIdentity,
    },
    #[error("object subpage announcement {display:?} lacks authenticated four-role context")]
    UnprovenRoles { display: TeletextPageAddress },
    #[error("object subpage announcement {display:?} uses a Level 3.5-only role")]
    Level35Only { display: TeletextPageAddress },
    #[error(transparent)]
    Service(#[from] TeletextServiceError),
    #[error(transparent)]
    Declaration(#[from] TeletextDeclaredServiceError),
    #[error(transparent)]
    Link(#[from] TeletextX27LinkError),
}

impl TeletextObjectSubpageAnnouncement {
    /// Bind an exact display and object-bank identity without granting edit authority.
    /// # Errors
    /// Rejects every non-POP/GPOP purpose. Service association is checked on application.
    pub fn new(
        display: TeletextPageAddress,
        resource: TeletextResourceIdentity,
    ) -> Result<Self, TeletextObjectSubpageAnnouncementError> {
        if !matches!(
            resource.kind(),
            TeletextResourceKind::Pop | TeletextResourceKind::Gpop
        ) {
            return Err(TeletextObjectSubpageAnnouncementError::Kind { resource });
        }
        Ok(Self { display, resource })
    }
    #[must_use]
    pub const fn display(&self) -> TeletextPageAddress {
        self.display
    }
    #[must_use]
    pub const fn resource(&self) -> TeletextResourceIdentity {
        self.resource
    }
}

impl TeletextDeclaredService {
    /// Announce existing S1 resources on each display's already selected raw page.
    ///
    /// Only the missing X27 mask bits change. Absent X27 may be materialized from
    /// authenticated MOT roles; all original MOTs, selections, defaults and source
    /// programs remain intact. Complete semantic collection and declaration
    /// authentication are repeated. This grants no host authoring/publication authority.
    /// # Errors
    /// Rejects empty/repeated/already-present intent, missing owners/resources,
    /// different selected raw pages, Level 3.5-only links and complete service limits.
    pub fn with_object_subpage_announcements(
        self,
        announcements: &[TeletextObjectSubpageAnnouncement],
        service_budget: TeletextServiceBudget,
        declaration_budget: TeletextDeclaredServiceBudget,
    ) -> Result<Self, TeletextObjectSubpageAnnouncementError> {
        use TeletextObjectSubpageAnnouncementError as Error;
        if announcements.is_empty() {
            return Err(Error::Empty);
        }
        self.check_announcement_budgets(announcements.len(), service_budget, declaration_budget)?;
        let mut updates = BTreeMap::new();
        for (ordinal, announcement) in announcements.iter().enumerate() {
            let display = announcement.display;
            let resource = announcement.resource;
            if announcements[..ordinal].contains(announcement) {
                return Err(Error::Duplicate { display, resource });
            }
            let resolution = self
                .resolutions
                .iter()
                .find(|resolution| resolution.display() == display)
                .ok_or(Error::MissingDisplay { display })?;
            if !self.resource_inventory.contains(&resource) {
                return Err(Error::MissingResource { resource });
            }
            let role = if resource.kind() == TeletextResourceKind::Pop {
                TeletextX27LinkRole::Pop
            } else {
                TeletextX27LinkRole::Gpop
            };
            let original = self
                .semantic
                .page_enhancements()
                .get(&display)
                .and_then(|page| page.compositional_links);
            if original.is_some_and(|links| !links.link(role).validity().applies_at_level25()) {
                return Err(Error::Level35Only { display });
            }
            if !matches!(resolution.role(role), TeletextMotRoleResolution::Selected {target,..} if *target == resource.page())
            {
                return Err(Error::Owner { display, resource });
            }
            let links = match updates.get(&display).copied().or(original) {
                Some(links) => links,
                None if resolution.mot().is_some() => resolution
                    .materialized_level25_links()
                    .map_err(|error| match error {
                        TeletextDrcsExtensionError::Link(error) => Error::Link(error),
                        _ => Error::UnprovenRoles { display },
                    })?,
                None => return Err(Error::UnprovenRoles { display }),
            };
            let link = links.link(role);
            // A real explicit link must select this raw owner too, even when a
            // non-supersedable MOT supplied the effective selection.
            if link.target()
                != (TeletextX27LinkTarget::Page {
                    page: resource.page(),
                })
            {
                return Err(Error::Owner { display, resource });
            }
            let bit = 1_u16 << resource.logical_subpage().get();
            if link.required_subpages().get() & bit != 0 {
                return Err(Error::AlreadyAnnounced { display, resource });
            }
            let changed = TeletextX27LinkDeclaration::new(
                link.target(),
                link.validity(),
                TeletextSubpageRequirementMask::new(link.required_subpages().get() | bit),
            )?;
            let mut roles = TeletextX27LinkRole::ALL.map(|role| links.link(role));
            roles[usize::from(role.wire_code())] = changed;
            updates.insert(
                display,
                TeletextX27Level25Links::new(roles[0], roles[1], roles[2], roles[3])?,
            );
        }
        let semantic = self
            .semantic
            .with_announced_object_links(updates, service_budget)?;
        Ok(Self::new(
            semantic,
            self.mots,
            self.selections,
            self.empty_object_pages,
            declaration_budget,
        )?)
    }
    fn check_announcement_budgets(
        &self,
        count: usize,
        service_budget: TeletextServiceBudget,
        declaration_budget: TeletextDeclaredServiceBudget,
    ) -> Result<(), TeletextObjectSubpageAnnouncementError> {
        // Fence raw intent and the complete immutable namespace before any clone/index.
        let mut remaining = service_budget.max_structure_items;
        super::super::service::charge(
            &mut remaining,
            count,
            "structure item",
            service_budget.max_structure_items,
        )?;
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
        let mut records = service_budget.max_records;
        super::super::service::charge(
            &mut records,
            self.semantic.base().pages.len(),
            "record",
            service_budget.max_records,
        )?;
        super::super::service::charge(
            &mut records,
            self.resource_inventory.len(),
            "record",
            service_budget.max_records,
        )?;
        Ok(())
    }
}
