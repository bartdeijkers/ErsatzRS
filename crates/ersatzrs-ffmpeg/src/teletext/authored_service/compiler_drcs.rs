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
    TeletextResourceIdentity, TeletextSemanticService, TeletextX27Level25Links,
    TeletextX27LinkRole, TeletextX27LinkTarget,
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

// Fixed Polish slots 0/1 and Turkish slots 2/3 retain each identity's two painted
// responses when consumers reorder or any other variant leaves.
type Bitmaps = [Option<TeletextDrcsBitmap>; national_drcs_carrier::COMPILER_GLYPH_SLOTS];
#[derive(Default)]
struct Demand {
    bitmaps: Bitmaps,
    consumers: Vec<TeletextPageAddress>,
}
// A linked-page supplement owns a separate S1; a fresh scope retains its old
// deterministic whole-page allocation and consumer grouping.
type Target = (TeletextDrcsTableScope, Option<TeletextResourceIdentity>);
type Demands = BTreeMap<Target, Demand>;

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
        let bitmaps = missing_bitmaps(service, &original, budget, context)?;
        if bitmaps.iter().any(Option::is_some) {
            if let Some(target) = target(declared, page.address) {
                let demand = demands.entry(target).or_default();
                for (existing, bitmap) in demand.bitmaps.iter_mut().zip(bitmaps) {
                    if let Some(bitmap) = bitmap {
                        if existing.is_some_and(|existing| existing != bitmap) {
                            return Err(error(Failure::MissingResourceBody));
                        }
                        *existing = Some(bitmap);
                    }
                }
                demand.consumers.push(page.address);
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
    // Distinct occupied targets can need separate banks in the same scope.
    // Their complete page count is admitted above; each bank has only the fixed
    // supported slots. Per-display two-subtable/24-glyph checks remain separate.
    let max_added_glyphs = demands
        .len()
        .checked_mul(national_drcs_carrier::COMPILER_GLYPH_SLOTS)
        .ok_or_else(|| {
            error(Failure::Size {
                category: "compiler DRCS glyphs",
            })
        })?;
    let mut tables = Vec::with_capacity(demands.len());
    let mut extensions = Vec::new();
    for (&(scope, linked), demand) in &demands {
        let resource = match linked {
            Some(resource) => resource,
            None => super::resource_namespace::allocate(declared, scope)?,
        };
        if tables.iter().any(|table: &TeletextDrcsSubtable| {
            table.identity().page() == resource.page()
                && table.identity().logical_subpage() == resource.logical_subpage()
        }) {
            return Err(error(Failure::Budget {
                category: "compiler DRCS free resource owners",
                count: 2,
                maximum: 1,
            }));
        }
        tables.push(
            TeletextDrcsSubtable::new(resource, definitions(demand.bitmaps)?)
                .map_err(|_| error(Failure::MissingResourceBody))?,
        );
        extensions.extend(
            demand
                .consumers
                .iter()
                .map(|&display| TeletextDrcsPageExtension {
                    display,
                    table: scope,
                    resource,
                }),
        );
    }
    let effective = service
        .derive_with_drcs_subpages(
            &tables,
            &extensions,
            TeletextDrcsExtensionBudget {
                max_records: budget.max_pages,
                max_added_resources: demands.len(),
                max_added_glyphs,
                max_page_extensions: budget.max_pages,
            },
        )
        .map_err(|reason| error(Failure::CompilerDrcs(Box::new(reason))))?;
    let resources = tables
        .into_iter()
        .zip(demands.into_values())
        .map(|(table, demand)| {
            let links = demand
                .consumers
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
                consumers: demand.consumers,
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

fn definitions(bitmaps: Bitmaps) -> Result<Vec<TeletextDrcsGlyphDefinition>, Error> {
    bitmaps
        .into_iter()
        .enumerate()
        .filter_map(|(slot, bitmap)| bitmap.map(|bitmap| (slot, bitmap)))
        .map(|(slot, bitmap)| {
            Ok(TeletextDrcsGlyphDefinition {
                slot: u8::try_from(slot)
                    .ok()
                    .and_then(|slot| TeletextDrcsGlyphSlot::new(slot).ok())
                    .ok_or_else(|| error(Failure::MissingResourceBody))?,
                bitmap,
            })
        })
        .collect()
}

fn may_need_carrier(
    service: &TeletextSemanticService,
    owner: TeletextPageAddress,
) -> Result<bool, Error> {
    let supported_national = service
        .effective_presentation(owner)
        .and_then(|presentation| presentation.primary_designation().ok())
        .is_some_and(|primary| matches!(primary.get(), 0x08 | 0x16 | 0x36));
    if !supported_national {
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

fn missing_bitmaps(
    service: &TeletextSemanticService,
    original: &TeletextMainComposition,
    budget: &TeletextAuthoredServiceEncodingBudget,
    context: &mut TeletextCompileContext<'_, '_>,
) -> Result<Bitmaps, Error> {
    let mut missing = [None; national_drcs_carrier::COMPILER_GLYPH_SLOTS];
    if !(0..original.display_resolution().origins().len())
        .any(|origin| national_drcs_carrier::needs_binding(original, origin))
    {
        return Ok(missing);
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
            let slot = national_drcs_carrier::required_slot(original, origin.origin_index)
                .ok_or_else(|| error(Failure::MissingResourceBody))?;
            let entry = missing
                .get_mut(usize::from(slot.get()))
                .ok_or_else(|| error(Failure::MissingResourceBody))?;
            *entry = Some(bitmap);
        }
    }
    Ok(missing)
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

#[path = "compiler_drcs/linked.rs"]
mod linked;

use linked::target;
