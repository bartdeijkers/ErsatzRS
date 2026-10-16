//! Private ownership of exact national-glyph realizations. This supplements the
//! physical service only; it never rewrites the authenticated authored input.

use super::{bounded, error, Error, Failure, TeletextAuthoredServiceEncodingBudget};
use crate::teletext::{
    compile_context::TeletextCompileContext, layout, TeletextCompiledDisplayPageEncodingError,
    TeletextCompiledDisplayPageEncodingFailure, TeletextMainComposition,
};
use crate::teletext::{national_drcs_carrier, normalized_main_program};
use ersatzrs_domain::{
    TeletextDeclaredService, TeletextDefaultObjectSelection, TeletextDrcsBitmap,
    TeletextDrcsExtensionBudget, TeletextDrcsGlyphDefinition, TeletextDrcsGlyphSlot,
    TeletextDrcsPageExtension, TeletextDrcsSubtable, TeletextDrcsTableScope, TeletextPageAddress,
    TeletextSemanticService, TeletextX27Level25Links, TeletextX27LinkRole, TeletextX27LinkTarget,
};
use std::collections::BTreeMap;

/// A factory-owned resource and the exact validated physical link supplements.
/// Getters expose proof for export/import, never mutable authoring permissions.
#[derive(Debug)]
pub struct TeletextCompilerDrcsResource {
    table: TeletextDrcsSubtable,
    consumers: Vec<TeletextPageAddress>,
    links: BTreeMap<TeletextPageAddress, TeletextX27Level25Links>,
}

impl TeletextCompilerDrcsResource {
    /// Exact generated definitions; all their slots are engine-owned.
    #[must_use]
    pub const fn table(&self) -> &TeletextDrcsSubtable {
        &self.table
    }
    /// Exact display owners whose physical program consumes this resource.
    #[must_use]
    pub fn consumers(&self) -> &[TeletextPageAddress] {
        &self.consumers
    }
    /// Complete effective X27 declaration, including unchanged authored roles.
    #[must_use]
    pub fn compositional_links(
        &self,
        display: TeletextPageAddress,
    ) -> Option<&TeletextX27Level25Links> {
        self.links.get(&display)
    }
}

pub(super) struct Prepared {
    pub originals: BTreeMap<TeletextPageAddress, TeletextMainComposition>,
    pub effective: Option<TeletextSemanticService>,
    pub resources: Vec<TeletextCompilerDrcsResource>,
}

type Demands = BTreeMap<TeletextDrcsTableScope, (TeletextDrcsBitmap, Vec<TeletextPageAddress>)>;

pub(super) fn prepare(
    declared: &TeletextDeclaredService,
    defaults: &[TeletextDefaultObjectSelection],
    budget: &TeletextAuthoredServiceEncodingBudget,
    context: &mut TeletextCompileContext<'_, '_>,
) -> Result<Prepared, Error> {
    let service = declared.semantic();
    let mut originals = BTreeMap::new();
    let mut demands = Demands::new();
    for page in &service.base().pages {
        if !may_need_carrier(service, page.address)? {
            continue;
        }
        let original = normalized_main_program::compose_normalization_original(
            service,
            page.address,
            defaults,
            &budget.normalization,
            context,
        )
        .map_err(normalization_error)?;
        if let Some(bitmap) = missing_bitmap(service, &original, budget, context)? {
            if let Some(scope) = unused_scope(service, page.address) {
                demands
                    .entry(scope)
                    .or_insert_with(|| (bitmap, Vec::new()))
                    .1
                    .push(page.address);
            }
        }
        originals.insert(page.address, original);
    }
    if demands.is_empty() {
        return Ok(Prepared {
            originals,
            effective: None,
            resources: Vec::new(),
        });
    }
    derive(declared, originals, demands, budget)
}

fn derive(
    declared: &TeletextDeclaredService,
    originals: BTreeMap<TeletextPageAddress, TeletextMainComposition>,
    demands: Demands,
    budget: &TeletextAuthoredServiceEncodingBudget,
) -> Result<Prepared, Error> {
    let service = declared.semantic();
    let count = service
        .base()
        .pages
        .len()
        .checked_add(declared.resource_inventory().len())
        .and_then(|count| count.checked_add(demands.len()))
        .ok_or_else(|| {
            error(Failure::Size {
                category: "compiler DRCS pages",
            })
        })?;
    bounded("pages", count, budget.max_pages)?;
    let mut tables = Vec::with_capacity(demands.len());
    let mut extensions = Vec::new();
    for (&scope, &(bitmap, ref consumers)) in &demands {
        let resource = super::resource_namespace::allocate(declared, scope)?;
        if tables
            .iter()
            .any(|table: &TeletextDrcsSubtable| table.identity().page() == resource.page())
        {
            return Err(error(Failure::Budget {
                category: "compiler DRCS free resource owners",
                count: 2,
                maximum: 1,
            }));
        }
        let slot =
            TeletextDrcsGlyphSlot::new(0).map_err(|_| error(Failure::MissingResourceBody))?;
        tables.push(
            TeletextDrcsSubtable::new(resource, vec![TeletextDrcsGlyphDefinition { slot, bitmap }])
                .map_err(|_| error(Failure::MissingResourceBody))?,
        );
        extensions.extend(consumers.iter().map(|&display| TeletextDrcsPageExtension {
            display,
            table: scope,
            resource,
        }));
    }
    let effective = service
        .derive_with_drcs_resources(
            &tables,
            &extensions,
            TeletextDrcsExtensionBudget {
                max_records: budget.max_pages,
                max_added_resources: 2,
                max_added_glyphs: 2,
                max_page_extensions: budget.max_pages,
            },
        )
        .map_err(|reason| error(Failure::CompilerDrcs(Box::new(reason))))?;
    let resources = tables
        .into_iter()
        .zip(demands.into_values())
        .map(|(table, (_, consumers))| {
            let links = consumers
                .iter()
                .map(|&owner| {
                    effective
                        .page_enhancements()
                        .get(&owner)
                        .and_then(|page| page.compositional_links)
                        .map(|links| (owner, links))
                        .ok_or_else(|| Error {
                            display: Some(owner),
                            ..error(Failure::MissingResourceBody)
                        })
                })
                .collect::<Result<BTreeMap<_, _>, Error>>()?;
            Ok(TeletextCompilerDrcsResource {
                table,
                consumers,
                links,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(Prepared {
        originals,
        effective: Some(effective),
        resources,
    })
}

fn may_need_carrier(
    service: &TeletextSemanticService,
    owner: TeletextPageAddress,
) -> Result<bool, Error> {
    let polish = service
        .effective_presentation(owner)
        .and_then(|presentation| presentation.primary_designation().ok())
        .is_some_and(|primary| primary.get() == 0x08);
    if !polish {
        return Ok(false);
    }
    let local = service
        .page_enhancements()
        .get(&owner)
        .is_some_and(|page| page.local_program.is_some());
    let page = service
        .base()
        .pages
        .iter()
        .find(|page| page.address == owner)
        .ok_or_else(|| error(Failure::MissingResourceBody))?;
    let layout = layout::compile_for_composition(page).map_err(|reason| Error {
        display: Some(owner),
        ..error(Failure::Layout(Box::new(reason)))
    })?;
    Ok(local || !layout.base_glyphs().is_empty())
}

fn missing_bitmap(
    service: &TeletextSemanticService,
    original: &TeletextMainComposition,
    budget: &TeletextAuthoredServiceEncodingBudget,
    context: &mut TeletextCompileContext<'_, '_>,
) -> Result<Option<TeletextDrcsBitmap>, Error> {
    if !(0..original.display_resolution().origins().len())
        .any(|origin| national_drcs_carrier::needs_binding(original, origin))
    {
        return Ok(None);
    }
    let plan = context
        .realization(original, budget.normalization.realization)
        .map_err(|reason| {
            normalization_error(super::super::TeletextNormalizedMainProgramError {
                owner: original.owner(),
                reason: super::super::TeletextNormalizedMainProgramFailure::Realization(Box::new(
                    reason,
                )),
            })
        })?;
    let mut remaining_scan = budget.normalization.assembly.max_work;
    for origin in plan.origins() {
        let Some(bitmap) = national_drcs_carrier::required_bitmap(original, origin.origin_index)
        else {
            continue;
        };
        let admitted = context
            .admit_national_drcs_scan(original.owner(), service, remaining_scan)
            .map_err(|reason| {
                normalization_error(super::super::TeletextNormalizedMainProgramError {
                    owner: original.owner(),
                    reason: super::super::TeletextNormalizedMainProgramFailure::Assembly(Box::new(
                        reason,
                    )),
                })
            })?;
        remaining_scan -= admitted;
        if national_drcs_carrier::resolve(service, original, origin.origin_index).is_none() {
            return Ok(Some(bitmap));
        }
    }
    Ok(None)
}

fn unused_scope(
    service: &TeletextSemanticService,
    owner: TeletextPageAddress,
) -> Option<TeletextDrcsTableScope> {
    let Some(page) = service.page_enhancements().get(&owner) else {
        return Some(TeletextDrcsTableScope::Normal);
    };
    for (scope, role) in [
        (TeletextDrcsTableScope::Normal, TeletextX27LinkRole::Drcs),
        (TeletextDrcsTableScope::Global, TeletextX27LinkRole::Gdrcs),
    ] {
        if page.resources.drcs.page(scope).is_none()
            && page.compositional_links.is_none_or(|links| {
                matches!(
                    links.link(role).target(),
                    TeletextX27LinkTarget::Unspecified { .. }
                )
            })
        {
            return Some(scope);
        }
    }
    None
}

fn normalization_error(reason: super::super::TeletextNormalizedMainProgramError) -> Error {
    if matches!(
        reason.reason,
        super::super::TeletextNormalizedMainProgramFailure::TimedOut
    ) {
        return Error {
            display: Some(reason.owner),
            ..error(Failure::TimedOut)
        };
    }
    Error {
        display: Some(reason.owner),
        ..error(Failure::Compiled(Box::new(
            TeletextCompiledDisplayPageEncodingError {
                owner: reason.owner,
                reason: TeletextCompiledDisplayPageEncodingFailure::Normalization(Box::new(reason)),
            },
        )))
    }
}
