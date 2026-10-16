//! Checked additive DRCS resources for a separate effective service. This is
//! neither authored intent, resource ownership, edit permission nor publication.

use super::{
    check_resource_records, validate_compositional_inventory, validate_compositional_usage,
    validate_object_references, TeletextDrcsCatalog, TeletextDrcsPageLinks, TeletextDrcsSubtable,
    TeletextPageAddress, TeletextResolvedObjectPageLinks, TeletextResolvedPageEnhancementDraft,
    TeletextResolvedPageResourceSelection, TeletextResourceKind, TeletextSemanticService,
    TeletextServiceError, TeletextX27Level25Links, TeletextX27LinkRole, TeletextX27LinkTarget,
};
use crate::teletext::{
    TeletextDrcsResourceError, TeletextDrcsTableScope, TeletextResourceIdentity,
    TeletextSubpageRequirementMask, TeletextX27LinkDeclaration, TeletextX27LinkError,
    TeletextX27LinkValidity, TeletextX27Magazine, MAX_TELETEXT_PAGE_RECORDS,
};

/// Trusted additive bounds; the original has already passed its service budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextDrcsExtensionBudget {
    /// Complete base-page and unique resource inventory after extension.
    pub max_records: usize,
    pub max_added_resources: usize,
    pub max_added_glyphs: usize,
    pub max_page_extensions: usize,
}

/// Supplement one effective DRCS scope and its physical X27 role.
/// The chosen derivation API checks whether its role must be unused or may
/// retain an explicit existing page while adding a free logical subpage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextDrcsPageExtension {
    pub display: TeletextPageAddress,
    pub table: TeletextDrcsTableScope,
    pub resource: TeletextResourceIdentity,
}

/// Numeric derivation failures, without source text or allocated publication state.
#[derive(Debug, thiserror::Error)]
pub enum TeletextDrcsExtensionError {
    #[error("DRCS extension requires both resources and page owners")]
    Empty,
    #[error("DRCS extension {category} count {count} exceeds {maximum}")]
    Budget {
        category: &'static str,
        count: usize,
        maximum: usize,
    },
    #[error("DRCS extension count arithmetic overflow")]
    Size,
    #[error("DRCS extension repeats resource {resource:?}")]
    DuplicateResource { resource: TeletextResourceIdentity },
    #[error("DRCS extension resource {resource:?} overlaps an original page owner")]
    OccupiedResourcePage { resource: TeletextResourceIdentity },
    #[error("DRCS extension repeats display {display:?}")]
    DuplicatePage { display: TeletextPageAddress },
    #[error("DRCS extension display {display:?} is absent")]
    MissingPage { display: TeletextPageAddress },
    #[error("DRCS extension display {display:?} selects absent added resource {resource:?}")]
    MissingResource {
        display: TeletextPageAddress,
        resource: TeletextResourceIdentity,
    },
    #[error("DRCS extension display {display:?}, scope {table:?}, has incompatible resource {resource:?}")]
    Kind {
        display: TeletextPageAddress,
        table: TeletextDrcsTableScope,
        resource: TeletextResourceIdentity,
    },
    #[error(
        "DRCS extension display {display:?}, scope {table:?}, is already selected or declared"
    )]
    OccupiedRole {
        display: TeletextPageAddress,
        table: TeletextDrcsTableScope,
    },
    #[error("DRCS extension display {display:?} lacks proven unchanged non-DRCS X27/MOT roles")]
    UnprovenOtherRoles { display: TeletextPageAddress },
    #[error(transparent)]
    Service(#[from] TeletextServiceError),
    #[error(transparent)]
    Drcs(#[from] TeletextDrcsResourceError),
    #[error(transparent)]
    Link(#[from] TeletextX27LinkError),
}

type Result<T> = std::result::Result<T, TeletextDrcsExtensionError>;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ExtensionMode {
    FreshPages,
    FreeSubpages,
}

#[path = "drcs_extension/subpages.rs"]
mod subpages;

fn bounded(category: &'static str, count: usize, maximum: usize) -> Result<()> {
    if count > maximum {
        return Err(TeletextDrcsExtensionError::Budget {
            category,
            count,
            maximum,
        });
    }
    Ok(())
}

impl TeletextSemanticService {
    /// Derive a checked effective service without changing this original.
    ///
    /// Fresh extension banks must not share a raw page with any original owner;
    /// this allocation constraint does not reject existing authored overlaps.
    /// Existing X27 roles other than the selected unused role remain byte-exact.
    /// Without X27, non-DRCS links/defaults/MOT presentation are not guessed.
    /// The caller must separately authenticate physical declarations, retain
    /// generated-resource ownership and validate final lifecycle/transport.
    /// # Errors
    /// Bounds precede cloning/index allocation. Rejects duplicate/occupied banks,
    /// missing/repeated owners, occupied scopes, unproven absent-X27 supplements
    /// and the existing inventory/link/collected-usage validation failures.
    pub fn derive_with_drcs_resources(
        &self,
        resources: &[TeletextDrcsSubtable],
        pages: &[TeletextDrcsPageExtension],
        budget: TeletextDrcsExtensionBudget,
    ) -> Result<Self> {
        self.derive_drcs(resources, pages, budget, ExtensionMode::FreshPages)
    }

    /// Derive an effective service with fresh banks or free linked DRCS subpages.
    /// Existing definitions and the original service remain unchanged. A shared
    /// raw page requires a free logical S1 and an explicit, matching resolved
    /// and X27 Level 2.5 link. Only its required-subpage mask is supplemented;
    /// its target, validity and every other role remain exact.
    /// # Errors
    /// Rejects collisions, missing or incompatible linked consumers, and all
    /// count, inventory, collected-reference and fresh-bank derivation failures.
    pub fn derive_with_drcs_subpages(
        &self,
        resources: &[TeletextDrcsSubtable],
        pages: &[TeletextDrcsPageExtension],
        budget: TeletextDrcsExtensionBudget,
    ) -> Result<Self> {
        self.derive_drcs(resources, pages, budget, ExtensionMode::FreeSubpages)
    }

    fn derive_drcs(
        &self,
        resources: &[TeletextDrcsSubtable],
        pages: &[TeletextDrcsPageExtension],
        budget: TeletextDrcsExtensionBudget,
        mode: ExtensionMode,
    ) -> Result<Self> {
        let structure = self.preflight_drcs_extension(resources, pages, budget, mode)?;
        let mut tables = self.drcs.subtables().to_vec();
        tables.extend_from_slice(resources);
        let catalog = TeletextDrcsCatalog::new(tables)?;
        let mut derived = self.clone();
        derived.drcs = catalog;
        derived.original_structure_items = structure;
        for supplement in pages {
            let mut enhancement = derived.pages.get(&supplement.display).cloned().unwrap_or(
                TeletextResolvedPageEnhancementDraft {
                    address: supplement.display,
                    presentation: None,
                    compositional_links: None,
                    local_program: None,
                    resources: TeletextResolvedPageResourceSelection {
                        objects: TeletextResolvedObjectPageLinks {
                            public: None,
                            global: None,
                        },
                        drcs: TeletextDrcsPageLinks::unlinked(),
                        defaults: [None; 4],
                    },
                },
            );
            supplement_page(&mut enhancement, supplement, resources, mode)?;
            derived.pages.insert(supplement.display, enhancement);
        }
        let inventory = check_resource_records(
            &derived.base,
            derived.objects.definitions(),
            derived.drcs.subtables(),
            &derived.object_companions,
            budget.max_records,
        )?;
        for page in derived.pages.values() {
            validate_compositional_inventory(page, &inventory)?;
            derived.validate_links_and_defaults(page, &inventory)?;
            // Programs, default slots, primary presentation and source ordinals
            // are unchanged, so the original complete collection remains valid.
            let (source, refs) = &derived.collected[&page.address];
            validate_object_references(page.address, page.resources.objects, source, refs)?;
            refs.validate_drcs(&derived.drcs, page.resources.drcs)
                .map_err(|error| TeletextServiceError::CollectedDrcs {
                    program_source: source.clone(),
                    error,
                })?;
            validate_compositional_usage(page, source, refs)?;
        }
        Ok(derived)
    }

    fn preflight_drcs_extension(
        &self,
        resources: &[TeletextDrcsSubtable],
        pages: &[TeletextDrcsPageExtension],
        budget: TeletextDrcsExtensionBudget,
        mode: ExtensionMode,
    ) -> Result<usize> {
        let glyphs = self.bound_drcs_extension(resources, pages, budget)?;
        self.validate_drcs_extension_resources(resources, pages, mode)?;
        self.validate_drcs_extension_pages(resources, pages, mode)?;
        self.drcs_extension_structure(resources, pages, glyphs)
    }

    fn bound_drcs_extension(
        &self,
        resources: &[TeletextDrcsSubtable],
        pages: &[TeletextDrcsPageExtension],
        budget: TeletextDrcsExtensionBudget,
    ) -> Result<usize> {
        bounded(
            "added resources",
            resources.len(),
            budget.max_added_resources,
        )?;
        bounded("page extensions", pages.len(), budget.max_page_extensions)?;
        let glyphs = resources
            .iter()
            .try_fold(0_usize, |count, table| {
                count.checked_add(table.glyphs().len())
            })
            .ok_or(TeletextDrcsExtensionError::Size)?;
        bounded("added glyphs", glyphs, budget.max_added_glyphs)?;
        let tables = self
            .drcs
            .subtables()
            .len()
            .checked_add(resources.len())
            .ok_or(TeletextDrcsExtensionError::Size)?;
        bounded("DRCS subtables", tables, MAX_TELETEXT_PAGE_RECORDS)?;
        let object_resources = self
            .objects
            .definitions()
            .iter()
            .enumerate()
            .filter(|(index, definition)| {
                definition.identity.resource().is_some_and(|resource| {
                    !self.objects.definitions()[..*index]
                        .iter()
                        .any(|previous| previous.identity.resource() == Some(resource))
                })
            })
            .count();
        let count = self
            .base
            .pages
            .len()
            .checked_add(self.drcs.subtables().len())
            .and_then(|count| count.checked_add(self.object_companions.len()))
            .and_then(|count| count.checked_add(object_resources))
            .and_then(|count| count.checked_add(resources.len()))
            .ok_or(TeletextDrcsExtensionError::Size)?;
        bounded("records", count, budget.max_records)?;
        if resources.is_empty() || pages.is_empty() {
            return Err(TeletextDrcsExtensionError::Empty);
        }
        Ok(glyphs)
    }

    fn validate_drcs_extension_resources(
        &self,
        resources: &[TeletextDrcsSubtable],
        pages: &[TeletextDrcsPageExtension],
        mode: ExtensionMode,
    ) -> Result<()> {
        for (index, table) in resources.iter().enumerate() {
            let resource = table.identity();
            if resources[..index].iter().any(|previous| {
                previous.identity().page() == resource.page()
                    && previous.identity().logical_subpage() == resource.logical_subpage()
            }) {
                return Err(TeletextDrcsExtensionError::DuplicateResource { resource });
            }
            if self.base.pages.iter().any(|page| {
                page.address.page.magazine() == resource.page().magazine()
                    && page.address.page.wire_page() == resource.page().page()
            }) || self
                .object_companions
                .iter()
                .any(|identity| identity.page() == resource.page())
                || self.objects.definitions().iter().any(|definition| {
                    definition
                        .identity
                        .resource()
                        .is_some_and(|identity| identity.page() == resource.page())
                })
            {
                return Err(TeletextDrcsExtensionError::OccupiedResourcePage { resource });
            }
            if subpages::existing_page(self, resource) {
                if mode == ExtensionMode::FreshPages {
                    return Err(TeletextDrcsExtensionError::OccupiedResourcePage { resource });
                }
                subpages::validate_resource(self, resource, pages)?;
            }
        }
        Ok(())
    }

    fn validate_drcs_extension_pages(
        &self,
        resources: &[TeletextDrcsSubtable],
        pages: &[TeletextDrcsPageExtension],
        mode: ExtensionMode,
    ) -> Result<()> {
        for (index, extension) in pages.iter().enumerate() {
            let display = extension.display;
            if pages[..index]
                .iter()
                .any(|previous| previous.display == display)
            {
                return Err(TeletextDrcsExtensionError::DuplicatePage { display });
            }
            if !self.base.pages.iter().any(|page| page.address == display) {
                return Err(TeletextDrcsExtensionError::MissingPage { display });
            }
            if !resources
                .iter()
                .any(|table| table.identity() == extension.resource)
            {
                return Err(TeletextDrcsExtensionError::MissingResource {
                    display,
                    resource: extension.resource,
                });
            }
            let kind = match extension.table {
                TeletextDrcsTableScope::Normal => TeletextResourceKind::Drcs,
                TeletextDrcsTableScope::Global => TeletextResourceKind::Gdrcs,
            };
            if extension.resource.kind() != kind {
                return Err(TeletextDrcsExtensionError::Kind {
                    display,
                    table: extension.table,
                    resource: extension.resource,
                });
            }
            if mode == ExtensionMode::FreeSubpages
                && subpages::existing_page(self, extension.resource)
            {
                subpages::validate_page(self, extension)?;
                continue;
            }
            if let Some(page) = self.pages.get(&display) {
                if page.resources.drcs.page(extension.table).is_some()
                    || page.compositional_links.is_some_and(|links| {
                        matches!(
                            links.link(role(extension.table)).target(),
                            TeletextX27LinkTarget::Page { .. }
                        )
                    })
                {
                    return Err(TeletextDrcsExtensionError::OccupiedRole {
                        display,
                        table: extension.table,
                    });
                }
                if page.compositional_links.is_none()
                    && (page.resources.objects.public.is_some()
                        || page.resources.objects.global.is_some()
                        || page
                            .resources
                            .drcs
                            .page(other_scope(extension.table))
                            .is_some()
                        || page.resources.defaults.iter().any(Option::is_some)
                        || self.mot_fallbacks.contains_key(&display))
                {
                    return Err(TeletextDrcsExtensionError::UnprovenOtherRoles { display });
                }
            } else if self.mot_fallbacks.contains_key(&display) {
                return Err(TeletextDrcsExtensionError::UnprovenOtherRoles { display });
            }
        }
        Ok(())
    }

    fn drcs_extension_structure(
        &self,
        resources: &[TeletextDrcsSubtable],
        pages: &[TeletextDrcsPageExtension],
        glyphs: usize,
    ) -> Result<usize> {
        let metadata = pages
            .iter()
            .filter(|page| !self.pages.contains_key(&page.display))
            .count();
        self.original_structure_items
            .checked_add(resources.len())
            .and_then(|count| count.checked_add(glyphs))
            .and_then(|count| count.checked_add(metadata))
            .ok_or(TeletextDrcsExtensionError::Size)
    }
}

fn role(scope: TeletextDrcsTableScope) -> TeletextX27LinkRole {
    match scope {
        TeletextDrcsTableScope::Normal => TeletextX27LinkRole::Drcs,
        TeletextDrcsTableScope::Global => TeletextX27LinkRole::Gdrcs,
    }
}
fn other_scope(scope: TeletextDrcsTableScope) -> TeletextDrcsTableScope {
    match scope {
        TeletextDrcsTableScope::Normal => TeletextDrcsTableScope::Global,
        TeletextDrcsTableScope::Global => TeletextDrcsTableScope::Normal,
    }
}
fn supplement_page(
    page: &mut TeletextResolvedPageEnhancementDraft,
    extension: &TeletextDrcsPageExtension,
    resources: &[TeletextDrcsSubtable],
    mode: ExtensionMode,
) -> Result<()> {
    let unused = TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Unspecified {
            magazine: TeletextX27Magazine::new(page.address.page.magazine())?,
        },
        TeletextX27LinkValidity::Level25Only,
        TeletextSubpageRequirementMask::new(u16::MAX),
    )?;
    let mut links = TeletextX27LinkRole::ALL.map(|role| {
        page.compositional_links
            .map_or(unused, |links| links.link(role))
    });
    let mask = resources
        .iter()
        .filter(|table| {
            table.identity().page() == extension.resource.page()
                && table.identity().kind() == extension.resource.kind()
        })
        .fold(0_u16, |mask, table| {
            mask | (1 << table.identity().logical_subpage().get())
        });
    let index = usize::from(role(extension.table).wire_code());
    links[index] = if mode == ExtensionMode::FreeSubpages
        && page.resources.drcs.page(extension.table) == Some(extension.resource.page())
    {
        TeletextX27LinkDeclaration::new(
            links[index].target(),
            links[index].validity(),
            TeletextSubpageRequirementMask::new(links[index].required_subpages().get() | mask),
        )?
    } else {
        TeletextX27LinkDeclaration::new(
            TeletextX27LinkTarget::Page {
                page: extension.resource.page(),
            },
            TeletextX27LinkValidity::Level25Only,
            TeletextSubpageRequirementMask::new(mask),
        )?
    };
    page.compositional_links = Some(TeletextX27Level25Links::new(
        links[0], links[1], links[2], links[3],
    )?);
    let old = page.resources.drcs;
    page.resources.drcs = TeletextDrcsPageLinks::new(
        if extension.table == TeletextDrcsTableScope::Normal {
            Some(extension.resource.page())
        } else {
            old.page(TeletextDrcsTableScope::Normal)
        },
        if extension.table == TeletextDrcsTableScope::Global {
            Some(extension.resource.page())
        } else {
            old.page(TeletextDrcsTableScope::Global)
        },
    )
    .map_err(TeletextDrcsResourceError::from)?;
    Ok(())
}

#[cfg(test)]
#[path = "drcs_extension_tests.rs"]
mod tests;
