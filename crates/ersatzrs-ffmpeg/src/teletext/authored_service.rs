//! Atomic encoding of one owned, declaration-authenticated authored service.
//!
//! Full-page lifecycle values remain caller supplied. This is a bounded owned
//! candidate, not lifecycle-history, carousel, publication or client acceptance.

use std::collections::{btree_map::Entry, BTreeMap, BTreeSet};

use ersatzrs_domain::{
    TeletextDeclaredService, TeletextDefaultObjectSelection, TeletextMotDefaultObject,
    TeletextMotLevel25Declaration, TeletextMotPublicLinkIndex, TeletextPageAddress,
    TeletextResourceHeaderMetadata, TeletextResourceIdentity, TeletextResourceKind,
    TeletextResourceUpdateCounter,
};

use super::compiled_display_page::encode_teletext_compiled_display_page;
use super::{
    allocate_teletext_resource_objects, compose_teletext_main_program, encode_teletext_drcs_page,
    encode_teletext_mot_page, encode_teletext_native_default_page, encode_teletext_object_page,
    layout, packets, presentation, validate_teletext_physical_page_inventory,
    EncodedTeletextCompiledDisplayPage, EncodedTeletextNativeDefaultPage,
    EncodedTeletextObjectPage, EncodedTeletextResourcePage,
    TeletextCompiledDisplayPageEncodingError, TeletextEncodingError, TeletextEncodingRegion,
    TeletextMainComposition, TeletextMainCompositionError, TeletextMotDefaultAllocations,
    TeletextNativeDefaultPageEncodingError, TeletextNormalizedMainProgramBudget,
    TeletextObjectAllocationError, TeletextObjectPageEncodingError, TeletextObjectPageTransmission,
    TeletextPhysicalPageInventoryError, TeletextPhysicalPageInventoryRecord,
    TeletextResourceFullUpdate, TeletextResourceObjectAllocations,
    TeletextResourcePageEncodingError, TeletextResourceTransmissionMode,
};

/// One explicit full-page lifecycle for every resource, including empty pages.
/// Serial transmission is fixed to match the existing display-header C11 bit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextServiceResourceTransmission {
    pub resource: TeletextResourceIdentity,
    pub counter: TeletextResourceUpdateCounter,
    pub update: TeletextResourceFullUpdate,
    /// POP/GPOP termination data only; MOT/DRCS have no object terminator.
    pub last_subpage: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextAuthoredServiceEncodingBudget {
    /// Unique display/resource pages, also bounding raw lifecycle records.
    pub max_pages: usize,
    /// All page units plus each actual magazine declaration exactly once.
    pub max_units: usize,
    /// Raw Local main commands and all catalog bodies, counted once each.
    pub max_commands: usize,
    /// Independent stage bounds applied to each display candidate.
    pub normalization: TeletextNormalizedMainProgramBudget,
}

/// Native absent-X26 display and its original semantic preview composition.
/// Foreign native default owners are composed without manufacturing wire calls.
#[derive(Debug)]
pub struct EncodedTeletextAuthoredNativeDisplay {
    encoded: EncodedTeletextNativeDefaultPage,
    composition: TeletextMainComposition,
}
impl EncodedTeletextAuthoredNativeDisplay {
    #[must_use]
    pub const fn encoded(&self) -> &EncodedTeletextNativeDefaultPage {
        &self.encoded
    }
    #[must_use]
    pub const fn composition(&self) -> &TeletextMainComposition {
        &self.composition
    }
}

/// Exactly one unique compiled page; no closing next-page/FF header is appended.
#[derive(Debug)]
pub enum EncodedTeletextAuthoredPage {
    NativeDisplay(Box<EncodedTeletextAuthoredNativeDisplay>),
    CompiledDisplay(Box<EncodedTeletextCompiledDisplayPage>),
    Object(EncodedTeletextObjectPage),
    Resource(EncodedTeletextResourcePage),
}
impl EncodedTeletextAuthoredPage {
    #[must_use]
    pub fn data_units(&self) -> &[[u8; 46]] {
        match self {
            Self::NativeDisplay(page) => page.encoded().page().data_units(),
            Self::CompiledDisplay(page) => page.page().data_units(),
            Self::Object(page) => page.data_units(),
            Self::Resource(page) => page.data_units(),
        }
    }
    #[must_use]
    pub fn inventory_record(&self) -> TeletextPhysicalPageInventoryRecord {
        match self {
            Self::NativeDisplay(page) => {
                TeletextPhysicalPageInventoryRecord::Display(page.encoded().page().address())
            }
            Self::CompiledDisplay(page) => {
                TeletextPhysicalPageInventoryRecord::Display(page.page().address())
            }
            Self::Object(page) => TeletextPhysicalPageInventoryRecord::Resource(page.header()),
            Self::Resource(page) => TeletextPhysicalPageInventoryRecord::Resource(page.header()),
        }
    }
    #[must_use]
    pub const fn resource_header(&self) -> Option<TeletextResourceHeaderMetadata> {
        match self {
            Self::Object(page) => Some(page.header()),
            Self::Resource(page) => Some(page.header()),
            Self::NativeDisplay(_) | Self::CompiledDisplay(_) => None,
        }
    }
}

/// One immutable input generation and its complete bounded encoded candidate.
/// Page-local magazine getters are diagnostic copies; only `magazine_units()`
/// is the service-wide emission list, with each actual M29/0 exactly once.
#[derive(Debug)]
pub struct EncodedTeletextAuthoredService {
    declared: TeletextDeclaredService,
    pages: Vec<EncodedTeletextAuthoredPage>,
    magazine_units: Vec<[u8; 46]>,
    inventory: Vec<TeletextPhysicalPageInventoryRecord>,
    unit_count: usize,
}
impl EncodedTeletextAuthoredService {
    #[must_use]
    pub const fn declared(&self) -> &TeletextDeclaredService {
        &self.declared
    }
    /// Original display order, followed by the declaration's logical resources.
    #[must_use]
    pub fn pages(&self) -> &[EncodedTeletextAuthoredPage] {
        &self.pages
    }
    #[must_use]
    pub fn magazine_units(&self) -> &[[u8; 46]] {
        &self.magazine_units
    }
    /// Actual body-derived headers, in the same order as `pages()`.
    #[must_use]
    pub fn inventory(&self) -> &[TeletextPhysicalPageInventoryRecord] {
        &self.inventory
    }
    #[must_use]
    pub const fn unit_count(&self) -> usize {
        self.unit_count
    }
}

#[derive(Debug, thiserror::Error)]
#[error("authored service at display {display:?}, resource {resource:?}: {reason}")]
pub struct TeletextAuthoredServiceEncodingError {
    pub display: Option<TeletextPageAddress>,
    pub resource: Option<TeletextResourceIdentity>,
    pub reason: TeletextAuthoredServiceEncodingFailure,
}
#[derive(Debug, thiserror::Error)]
pub enum TeletextAuthoredServiceEncodingFailure {
    #[error("{category} count {count} exceeds trusted budget {maximum}")]
    Budget {
        category: &'static str,
        count: usize,
        maximum: usize,
    },
    #[error("checked size arithmetic failed for {category}")]
    Size { category: &'static str },
    #[error("lifecycle records {first} and {second} repeat the same resource")]
    DuplicateTransmission { first: usize, second: usize },
    #[error("required resource has no explicit lifecycle record")]
    MissingTransmission,
    #[error("lifecycle record {ordinal} refers to an undeclared resource")]
    UnexpectedTransmission { ordinal: usize },
    #[error("authenticated resource has no supported body")]
    MissingResourceBody,
    #[error("resource allocation failed: {0}")]
    Allocation(Box<TeletextObjectAllocationError>),
    #[error("display layout failed: {0}")]
    Layout(Box<TeletextEncodingError>),
    #[error("native semantic composition failed: {0}")]
    Composition(Box<TeletextMainCompositionError>),
    #[error("native display encoding failed: {0}")]
    Native(Box<TeletextNativeDefaultPageEncodingError>),
    #[error("normalized display encoding failed: {0}")]
    Compiled(Box<TeletextCompiledDisplayPageEncodingError>),
    #[error("object page encoding failed: {0}")]
    Object(Box<TeletextObjectPageEncodingError>),
    #[error("resource page encoding failed: {0}")]
    Resource(Box<TeletextResourcePageEncodingError>),
    #[error("actual physical inventory failed: {0}")]
    Inventory(Box<TeletextPhysicalPageInventoryError>),
}
type Failure = TeletextAuthoredServiceEncodingFailure;
type Error = TeletextAuthoredServiceEncodingError;
fn error(reason: Failure) -> Error {
    Error {
        display: None,
        resource: None,
        reason,
    }
}
fn bounded(category: &'static str, count: usize, maximum: usize) -> Result<(), Error> {
    if count > maximum {
        return Err(error(Failure::Budget {
            category,
            count,
            maximum,
        }));
    }
    Ok(())
}
fn sized<T>(category: &'static str, count: usize) -> Result<(), Error> {
    count
        .checked_mul(std::mem::size_of::<T>())
        .filter(|&bytes| isize::try_from(bytes).is_ok())
        .ok_or_else(|| error(Failure::Size { category }))?;
    Ok(())
}

/// Compile every display and declared resource atomically from one owned input.
///
/// Exact lifecycle correspondence is checked before encoding. Global original
/// MOT reservations drive both resource writers and display normalization. The
/// native path is selected only by absent Local X26 and no derived Base glyph;
/// encoding/normalization failures never trigger fallback or owner exemptions.
/// All output uses serial mode and preserves actual declarations and headers.
/// # Errors
/// Returns located lifecycle, budget, existing encoder/model or final inventory
/// diagnostics. No partial service or detached allocation escapes on failure.
/// Full resource C2 settlement, lifecycle history, transport and clients remain
/// separate acceptance obligations; a candidate is not publication authority.
pub fn encode_teletext_authored_service(
    declared: TeletextDeclaredService,
    transmissions: &[TeletextServiceResourceTransmission],
    budget: &TeletextAuthoredServiceEncodingBudget,
) -> Result<EncodedTeletextAuthoredService, TeletextAuthoredServiceEncodingError> {
    let count = preflight(&declared, transmissions, budget)?;
    let transmissions = lifecycle_index(&declared, transmissions)?;
    let defaults = declared
        .defaults()
        .iter()
        .map(ersatzrs_domain::TeletextDeclaredDefaultReservation::selection)
        .collect::<Vec<_>>();
    let allocations = allocate_teletext_resource_objects(
        declared.semantic().objects(),
        &defaults,
        budget.normalization.max_resource_objects,
    )
    .map_err(|reason| error(Failure::Allocation(Box::new(reason))))?;
    let mut pages = Vec::with_capacity(count);
    let mut inventory = Vec::with_capacity(count);
    let mut units = declared.semantic().magazine_presentations().len();
    for page in &declared.semantic().base().pages {
        let encoded = encode_display(&declared, page.address, &defaults, budget)?;
        append_page(encoded, &mut pages, &mut inventory, &mut units, budget)?;
    }
    for &resource in declared.resource_inventory() {
        let tx = transmissions[&resource].1;
        let encoded = encode_resource(&declared, resource, &allocations, tx, budget.max_commands)?;
        append_page(encoded, &mut pages, &mut inventory, &mut units, budget)?;
    }
    validate_teletext_physical_page_inventory(&inventory, budget.max_pages)
        .map_err(|reason| error(Failure::Inventory(Box::new(reason))))?;
    let magazine_units = declared
        .semantic()
        .magazine_presentations()
        .iter()
        .map(|(&magazine, declaration)| {
            packets::data_unit(
                magazine % 8,
                29,
                &presentation::encode_presentation(declaration),
                false,
            )
        })
        .collect();
    Ok(EncodedTeletextAuthoredService {
        declared,
        pages,
        magazine_units,
        inventory,
        unit_count: units,
    })
}

fn preflight(
    declared: &TeletextDeclaredService,
    tx: &[TeletextServiceResourceTransmission],
    budget: &TeletextAuthoredServiceEncodingBudget,
) -> Result<usize, Error> {
    let count = declared
        .semantic()
        .base()
        .pages
        .len()
        .checked_add(declared.resource_inventory().len())
        .ok_or_else(|| error(Failure::Size { category: "pages" }))?;
    bounded("pages", count, budget.max_pages)?;
    bounded("transmissions", tx.len(), budget.max_pages)?;
    bounded(
        "units",
        declared.semantic().magazine_presentations().len(),
        budget.max_units,
    )?;
    let objects = declared.semantic().objects().definitions();
    let maximum = budget.normalization.max_resource_objects;
    if objects.len() > maximum {
        return Err(error(Failure::Allocation(Box::new(
            TeletextObjectAllocationError::ObjectBudget {
                count: objects.len(),
                maximum,
            },
        ))));
    }
    if declared.defaults().len() > maximum {
        return Err(error(Failure::Allocation(Box::new(
            TeletextObjectAllocationError::DefaultBudget {
                count: declared.defaults().len(),
                maximum,
            },
        ))));
    }
    sized::<EncodedTeletextAuthoredPage>("pages", count)?;
    sized::<TeletextPhysicalPageInventoryRecord>("inventory", count)?;
    sized::<(
        TeletextResourceIdentity,
        (usize, TeletextServiceResourceTransmission),
    )>("transmissions", tx.len())?;
    sized::<TeletextResourceIdentity>("resources", declared.resource_inventory().len())?;
    sized::<TeletextDefaultObjectSelection>("defaults", declared.defaults().len())?;
    // Existing page writers return at most 44 units. Fence actual derived
    // storage rather than treating a large trusted limit as an allocation size.
    let unit_storage = count
        .checked_mul(44)
        .and_then(|pages| pages.checked_add(declared.semantic().magazine_presentations().len()))
        .ok_or_else(|| error(Failure::Size { category: "units" }))?;
    sized::<[u8; 46]>("units", unit_storage)?;
    let mut commands = 0_usize;
    for count in objects
        .iter()
        .map(|object| object.program.commands().len())
        .chain(
            declared
                .semantic()
                .page_enhancements()
                .values()
                .filter_map(|page| {
                    page.local_program
                        .as_ref()
                        .map(|program| program.commands().len())
                }),
        )
    {
        commands = commands.checked_add(count).ok_or_else(|| {
            error(Failure::Size {
                category: "commands",
            })
        })?;
        bounded("commands", commands, budget.max_commands)?;
    }
    Ok(count)
}

fn lifecycle_index(
    declared: &TeletextDeclaredService,
    tx: &[TeletextServiceResourceTransmission],
) -> Result<BTreeMap<TeletextResourceIdentity, (usize, TeletextServiceResourceTransmission)>, Error>
{
    let expected = declared
        .resource_inventory()
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let mut index: BTreeMap<
        TeletextResourceIdentity,
        (usize, TeletextServiceResourceTransmission),
    > = BTreeMap::new();
    for (second, &record) in tx.iter().enumerate() {
        let located = |reason| Error {
            resource: Some(record.resource),
            ..error(reason)
        };
        if !expected.contains(&record.resource) {
            return Err(located(Failure::UnexpectedTransmission { ordinal: second }));
        }
        match index.entry(record.resource) {
            Entry::Occupied(previous) => {
                return Err(located(Failure::DuplicateTransmission {
                    first: previous.get().0,
                    second,
                }))
            }
            Entry::Vacant(entry) => {
                entry.insert((second, record));
            }
        }
    }
    for resource in expected {
        if !index.contains_key(&resource) {
            return Err(Error {
                resource: Some(resource),
                ..error(Failure::MissingTransmission)
            });
        }
    }
    Ok(index)
}

fn encode_display(
    declared: &TeletextDeclaredService,
    owner: TeletextPageAddress,
    defaults: &[TeletextDefaultObjectSelection],
    budget: &TeletextAuthoredServiceEncodingBudget,
) -> Result<EncodedTeletextAuthoredPage, Error> {
    let service = declared.semantic();
    let located = |reason| Error {
        display: Some(owner),
        ..error(reason)
    };
    let page = service
        .base()
        .pages
        .iter()
        .find(|page| page.address == owner)
        .ok_or_else(|| located(Failure::MissingResourceBody))?;
    let layout = layout::compile_for_composition(page)
        .map_err(|reason| located(Failure::Layout(Box::new(reason))))?;
    let local = service
        .page_enhancements()
        .get(&owner)
        .is_some_and(|page| page.local_program.is_some());
    if !local && layout.base_glyphs().is_empty() {
        let composition =
            compose_teletext_main_program(service, owner, budget.normalization.composition)
                .map_err(|reason| located(Failure::Composition(Box::new(reason))))?;
        let encoded = encode_teletext_native_default_page(
            service,
            owner,
            TeletextEncodingRegion::WesternLatin,
        )
        .map_err(|reason| located(Failure::Native(Box::new(reason))))?;
        Ok(EncodedTeletextAuthoredPage::NativeDisplay(Box::new(
            EncodedTeletextAuthoredNativeDisplay {
                encoded,
                composition,
            },
        )))
    } else {
        let encoded = encode_teletext_compiled_display_page(
            service,
            owner,
            TeletextEncodingRegion::WesternLatin,
            defaults,
            &budget.normalization,
        )
        .map_err(|reason| located(Failure::Compiled(Box::new(reason))))?;
        Ok(EncodedTeletextAuthoredPage::CompiledDisplay(Box::new(
            encoded,
        )))
    }
}

fn encode_resource(
    declared: &TeletextDeclaredService,
    resource: TeletextResourceIdentity,
    allocations: &TeletextResourceObjectAllocations,
    tx: TeletextServiceResourceTransmission,
    max_commands: usize,
) -> Result<EncodedTeletextAuthoredPage, Error> {
    let located = |reason| Error {
        resource: Some(resource),
        ..error(reason)
    };
    match resource.kind() {
        TeletextResourceKind::Pop | TeletextResourceKind::Gpop => encode_teletext_object_page(
            resource,
            declared.semantic().objects(),
            allocations,
            TeletextObjectPageTransmission {
                counter: tx.counter,
                update: tx.update,
                mode: TeletextResourceTransmissionMode::Serial,
                last_subpage: tx.last_subpage,
            },
            max_commands,
        )
        .map(EncodedTeletextAuthoredPage::Object)
        .map_err(|reason| located(Failure::Object(Box::new(reason)))),
        TeletextResourceKind::Mot => {
            let declaration = declared
                .mots()
                .iter()
                .find(|mot| mot.identity() == resource)
                .ok_or_else(|| located(Failure::MissingResourceBody))?;
            let defaults = mot_allocations(declaration, allocations);
            encode_teletext_mot_page(
                declaration,
                &defaults,
                tx.counter,
                tx.update,
                TeletextResourceTransmissionMode::Serial,
            )
            .map(EncodedTeletextAuthoredPage::Resource)
            .map_err(|reason| located(Failure::Resource(Box::new(reason))))
        }
        TeletextResourceKind::Drcs | TeletextResourceKind::Gdrcs => {
            let table = declared
                .semantic()
                .drcs()
                .subtables()
                .iter()
                .find(|table| table.identity() == resource)
                .ok_or_else(|| located(Failure::MissingResourceBody))?;
            encode_teletext_drcs_page(
                table,
                tx.counter,
                tx.update,
                TeletextResourceTransmissionMode::Serial,
            )
            .map(EncodedTeletextAuthoredPage::Resource)
            .map_err(|reason| located(Failure::Resource(Box::new(reason))))
        }
        TeletextResourceKind::Mip => Err(located(Failure::MissingResourceBody)),
    }
}

fn mot_allocations(
    declaration: &TeletextMotLevel25Declaration,
    allocations: &TeletextResourceObjectAllocations,
) -> TeletextMotDefaultAllocations {
    let mut result = [[None; 2]; 8];
    for (link, role) in std::iter::once(None)
        .chain(TeletextMotPublicLinkIndex::ALL.map(Some))
        .enumerate()
    {
        for (slot, default) in declaration
            .links()
            .object_link(role)
            .defaults()
            .into_iter()
            .enumerate()
        {
            if let TeletextMotDefaultObject::Object {
                target,
                object_type,
            } = default
            {
                result[link][slot] = allocations
                    .lookup(target, object_type)
                    .and_then(super::TeletextResourceObjectAllocation::mot_default_allocation);
            }
        }
    }
    result
}

fn append_page(
    page: EncodedTeletextAuthoredPage,
    pages: &mut Vec<EncodedTeletextAuthoredPage>,
    inventory: &mut Vec<TeletextPhysicalPageInventoryRecord>,
    units: &mut usize,
    budget: &TeletextAuthoredServiceEncodingBudget,
) -> Result<(), Error> {
    let record = page.inventory_record();
    let located = |reason| match record {
        TeletextPhysicalPageInventoryRecord::Display(display) => Error {
            display: Some(display),
            ..error(reason)
        },
        TeletextPhysicalPageInventoryRecord::Resource(header) => Error {
            resource: Some(header.identity()),
            ..error(reason)
        },
    };
    let count = units
        .checked_add(page.data_units().len())
        .ok_or_else(|| located(Failure::Size { category: "units" }))?;
    bounded("units", count, budget.max_units).map_err(|error| located(error.reason))?;
    *units = count;
    inventory.push(record);
    pages.push(page);
    Ok(())
}
