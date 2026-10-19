//! Atomic encoding of one owned, declaration-authenticated authored service.
//!
//! Authored lifecycle values remain caller supplied. Generated DRCS baselines
//! are compiler-owned and share the checked per-response lifecycle authorization.
//! This candidate is not carousel, publication or client acceptance.

use std::collections::{btree_map::Entry, BTreeMap, BTreeSet};
use std::time::Instant;

mod compiler_drcs;
mod resource_namespace;
pub use compiler_drcs::TeletextCompilerDrcsResource;

use ersatzrs_domain::{
    TeletextDeclaredService, TeletextDefaultObjectSelection, TeletextMotDefaultObject,
    TeletextMotLevel25Declaration, TeletextMotPublicLinkIndex, TeletextPageAddress,
    TeletextResourceHeaderMetadata, TeletextResourceIdentity, TeletextResourceKind,
    TeletextResourceUpdateCounter,
};

use super::compile_context::TeletextCompileContext;
use super::compiled_display_page::TeletextCompiledDisplayPageEncodingFailure;
use super::main_composition::compose_teletext_main_program_with_context;
use super::resource_transmission::{
    settle_native_with_context, ResourceTransmissionContext, TeletextResourceTransmissionError,
};
use super::{
    allocate_teletext_resource_objects, encode_teletext_drcs_page, encode_teletext_mot_page,
    encode_teletext_native_default_page, encode_teletext_object_page, layout, packets,
    presentation, validate_teletext_physical_page_inventory, EncodedTeletextCompiledDisplayPage,
    EncodedTeletextNativeDefaultPage, EncodedTeletextObjectPage, EncodedTeletextResourcePage,
    TeletextCompiledDisplayPageEncodingError, TeletextEncodingError, TeletextEncodingRegion,
    TeletextMainComposition, TeletextMainCompositionError, TeletextMotDefaultAllocations,
    TeletextNativeDefaultPageEncodingError, TeletextNormalizedMainProgramBudget,
    TeletextObjectAllocationError, TeletextObjectPageEncodingError, TeletextObjectPageTransmission,
    TeletextPhysicalPageInventoryError, TeletextPhysicalPageInventoryRecord,
    TeletextResourceFullUpdate, TeletextResourceObjectAllocations,
    TeletextResourcePageEncodingError, TeletextResourceTransmissionMode,
};

/// One explicit full-page lifecycle for every authored resource, including empty pages.
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
    /// Optional cumulative admitted display-resolution charges across all
    /// original/physical/settled displays. Zero is an exhausted quota; None
    /// retains the existing independent stage limits. No production default
    /// is inferred, and this meter is not total compiler CPU or RSS.
    pub max_display_updates: Option<usize>,
    /// Optional cumulative admitted glyph-comparison lookup work. This is a
    /// separate unit from display updates, not a CPU-time conversion.
    pub max_glyph_comparison_work: Option<usize>,
    /// Optional cumulative admitted selected-display comparison updates,
    /// including its conservative bulk reservations.
    pub max_display_comparison_updates: Option<usize>,
    /// Optional cumulative admitted compatibility-planning visits.
    pub max_realization_updates: Option<usize>,
    /// Optional cumulative admitted carrier-placement visits.
    pub max_placement_work: Option<usize>,
    /// Optional cumulative assembly visits and conservative clone reservations,
    /// including original/prefix/physical object-header normalization. These
    /// scans may precede realization; carrier identity search is bounded below.
    pub max_assembly_work: Option<usize>,
    /// Optional cumulative admitted invocation-correspondence visits/reservations.
    pub max_ancestry_work: Option<usize>,
    /// Optional cumulative admitted command visits in original and supplied-program collection.
    pub max_collected_commands: Option<usize>,
    /// Optional cumulative admitted DRCS reference occurrences in those collections.
    pub max_collected_drcs_references: Option<usize>,
    /// Optional cumulative admitted event visits across count and emit spatial passes.
    pub max_spatial_event_visits: Option<usize>,
    /// Optional cumulative admitted invocation visits across both spatial passes.
    pub max_spatial_invocation_visits: Option<usize>,
    /// Optional cumulative admitted catalogue scans and carrier-ID candidate probes.
    pub max_identity_checks: Option<usize>,
    /// Optional cumulative admitted attribute cell projections/writes; event
    /// scans, private state, snapshots and observations remain separate work.
    pub max_attribute_cell_updates: Option<usize>,
    /// Independent stage bounds applied to each display candidate.
    pub normalization: TeletextNormalizedMainProgramBudget,
}

/// Admitted work in separately bounded service-wide buckets. A disabled bucket
/// reports None, not zero. These counters are neither model/wire identity nor
/// complete compiler CPU/RSS costs; earlier charges survive later failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextAuthoredServiceEncodingUsage {
    pub display_updates: Option<usize>,
    pub glyph_comparison_work: Option<usize>,
    pub display_comparison_updates: Option<usize>,
    pub realization_updates: Option<usize>,
    pub placement_work: Option<usize>,
    pub assembly_work: Option<usize>,
    pub ancestry_work: Option<usize>,
    pub collected_commands: Option<usize>,
    pub collected_drcs_references: Option<usize>,
    pub spatial_event_visits: Option<usize>,
    pub spatial_invocation_visits: Option<usize>,
    pub identity_checks: Option<usize>,
    pub attribute_cell_updates: Option<usize>,
}

impl TeletextAuthoredServiceEncodingUsage {
    fn empty(budget: &TeletextAuthoredServiceEncodingBudget) -> Self {
        Self {
            display_updates: budget.max_display_updates.map(|_| 0),
            glyph_comparison_work: budget.max_glyph_comparison_work.map(|_| 0),
            display_comparison_updates: budget.max_display_comparison_updates.map(|_| 0),
            realization_updates: budget.max_realization_updates.map(|_| 0),
            placement_work: budget.max_placement_work.map(|_| 0),
            assembly_work: budget.max_assembly_work.map(|_| 0),
            ancestry_work: budget.max_ancestry_work.map(|_| 0),
            collected_commands: budget.max_collected_commands.map(|_| 0),
            collected_drcs_references: budget.max_collected_drcs_references.map(|_| 0),
            spatial_event_visits: budget.max_spatial_event_visits.map(|_| 0),
            spatial_invocation_visits: budget.max_spatial_invocation_visits.map(|_| 0),
            identity_checks: budget.max_identity_checks.map(|_| 0),
            attribute_cell_updates: budget.max_attribute_cell_updates.map(|_| 0),
        }
    }
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
    compiler_drcs_resources: Vec<TeletextCompilerDrcsResource>,
    pages: Vec<EncodedTeletextAuthoredPage>,
    magazine_units: Vec<[u8; 46]>,
    inventory: Vec<TeletextPhysicalPageInventoryRecord>,
    unit_count: usize,
    resource_lifecycle_variants: Option<super::TeletextResourceLifecyclePlan>,
}
impl EncodedTeletextAuthoredService {
    #[must_use]
    pub const fn declared(&self) -> &TeletextDeclaredService {
        &self.declared
    }
    /// Engine-owned physical resources, separate from immutable addon intent.
    /// These definitions and their generated links confer no editor authority.
    #[must_use]
    pub fn compiler_drcs_resources(&self) -> &[TeletextCompilerDrcsResource] {
        &self.compiler_drcs_resources
    }
    /// Original display order, followed by authored then compiler-owned resources.
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

    /// Derive finite checked lifecycle headers bound to this exact service.
    /// Baseline pages, declarations, inventory and first-start bytes are unchanged.
    /// Warm outputs may select only these alternatives using completed delivery.
    /// # Errors
    /// Rejects expired derivation, unsupported resource states, noncanonical
    /// native headers or storage beyond the finite resource-owner namespace.
    pub fn with_resource_lifecycle_variants(
        mut self,
        deadline: std::time::Instant,
    ) -> Result<Self, super::TeletextResourceLifecycleError> {
        self.resource_lifecycle_variants = Some(super::TeletextResourceLifecyclePlan::derive(
            &self, deadline,
        )?);
        Ok(self)
    }

    /// Checked alternatives retained with the immutable baseline service.
    /// Absence preserves the original strict authored lifecycle contract.
    #[must_use]
    pub const fn resource_lifecycle_variants(
        &self,
    ) -> Option<&super::TeletextResourceLifecyclePlan> {
        self.resource_lifecycle_variants.as_ref()
    }

    /// Move only completed resource bodies into private carousel history.
    pub(super) fn into_resource_pages(self) -> Vec<EncodedTeletextAuthoredPage> {
        let mut pages = self.pages;
        pages.retain(|page| page.resource_header().is_some());
        pages
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
    #[error("authored service compilation exceeded its original deadline")]
    TimedOut,
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
    #[error("resource transmission correspondence failed: {0}")]
    ResourceTransmission(Box<TeletextResourceTransmissionError>),
    #[error("resource page encoding failed: {0}")]
    Resource(Box<TeletextResourcePageEncodingError>),
    #[error("compiler DRCS derivation failed: {0}")]
    CompilerDrcs(Box<ersatzrs_domain::TeletextDrcsExtensionError>),
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

struct EncodingClock<'a, C> {
    deadline: Option<Instant>,
    now: &'a mut C,
}

impl<C: FnMut() -> Instant> EncodingClock<'_, C> {
    fn check(
        &mut self,
        display: Option<TeletextPageAddress>,
        resource: Option<TeletextResourceIdentity>,
    ) -> Result<(), Error> {
        if self
            .deadline
            .is_some_and(|deadline| (self.now)() >= deadline)
        {
            return Err(Error {
                display,
                resource,
                reason: Failure::TimedOut,
            });
        }
        Ok(())
    }
}

/// Compile every display and resource atomically from one authenticated input.
///
/// Exact authored lifecycle correspondence is checked before encoding. Generated
/// resources need no caller transmission entry; their baseline and checked warm
/// alternatives remain engine-owned. Global original MOT reservations drive both
/// resource writers and display normalization. The
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
    encode_teletext_authored_service_with_clock(
        declared,
        transmissions,
        budget,
        None,
        &mut Instant::now,
    )
}

/// Encode with cooperative checkpoints against one absolute native deadline.
/// The existing model, lifecycle, packing and validation rules are unchanged.
/// # Errors
/// Returns the existing located errors or a located timeout, never a partial
/// service. A running bounded stage is not preempted; this is neither an
/// aggregate operation quota nor an immediate CPU-cancellation guarantee.
pub fn encode_teletext_authored_service_before(
    declared: TeletextDeclaredService,
    transmissions: &[TeletextServiceResourceTransmission],
    budget: &TeletextAuthoredServiceEncodingBudget,
    deadline: Instant,
) -> Result<EncodedTeletextAuthoredService, TeletextAuthoredServiceEncodingError> {
    encode_teletext_authored_service_with_clock(
        declared,
        transmissions,
        budget,
        Some(deadline),
        &mut Instant::now,
    )
}

// A local clock seam makes absolute-deadline boundaries deterministic in tests.
// Production uses Instant::now; the untimed public API performs no clock reads.
pub(super) fn encode_teletext_authored_service_with_clock(
    declared: TeletextDeclaredService,
    transmissions: &[TeletextServiceResourceTransmission],
    budget: &TeletextAuthoredServiceEncodingBudget,
    deadline: Option<Instant>,
    now: &mut impl FnMut() -> Instant,
) -> Result<EncodedTeletextAuthoredService, TeletextAuthoredServiceEncodingError> {
    encode_with_clock_and_usage(declared, transmissions, budget, deadline, now).0
}

/// Compile with the optional shared update quota and report admitted charges.
/// Usage is kept outside the model/wire/artifact identity and survives failure.
/// `None` usage means aggregate metering was disabled, not that work was absent.
/// # Errors
/// Returns the same located result as the existing encoder. Quota exhaustion
/// returns no partial candidate. Other independent stage limits still apply;
/// conservative reservations are admitted charges, not exact CPU/RSS work.
pub fn encode_teletext_authored_service_with_usage(
    declared: TeletextDeclaredService,
    transmissions: &[TeletextServiceResourceTransmission],
    budget: &TeletextAuthoredServiceEncodingBudget,
    deadline: Option<Instant>,
) -> (
    Result<EncodedTeletextAuthoredService, TeletextAuthoredServiceEncodingError>,
    Option<usize>,
) {
    let (result, usage) =
        encode_with_clock_and_usage(declared, transmissions, budget, deadline, &mut Instant::now);
    (result, usage.display_updates)
}

/// Compile with separate optional service quotas and report their admitted work.
/// Every enabled bucket is shared across displays. No charge is refunded on
/// failure, and disabled buckets remain None. The compatibility usage API still
/// reports only display-resolution updates; all encoders honor every supplied cap.
/// # Errors
/// Returns the original located result without a partial candidate. These
/// buckets preserve independent stage limits and do not certify total CPU/RSS.
pub fn encode_teletext_authored_service_with_work_usage(
    declared: TeletextDeclaredService,
    transmissions: &[TeletextServiceResourceTransmission],
    budget: &TeletextAuthoredServiceEncodingBudget,
    deadline: Option<Instant>,
) -> (
    Result<EncodedTeletextAuthoredService, TeletextAuthoredServiceEncodingError>,
    TeletextAuthoredServiceEncodingUsage,
) {
    encode_with_clock_and_usage(declared, transmissions, budget, deadline, &mut Instant::now)
}

fn encode_with_clock_and_usage(
    declared: TeletextDeclaredService,
    transmissions: &[TeletextServiceResourceTransmission],
    budget: &TeletextAuthoredServiceEncodingBudget,
    deadline: Option<Instant>,
    now: &mut impl FnMut() -> Instant,
) -> (
    Result<EncodedTeletextAuthoredService, TeletextAuthoredServiceEncodingError>,
    TeletextAuthoredServiceEncodingUsage,
) {
    let mut clock = EncodingClock { deadline, now };
    let mut usage = TeletextAuthoredServiceEncodingUsage::empty(budget);
    let result = encode_owned_service(declared, transmissions, budget, &mut clock, &mut usage);
    (result, usage)
}

fn encode_owned_service(
    declared: TeletextDeclaredService,
    transmissions: &[TeletextServiceResourceTransmission],
    budget: &TeletextAuthoredServiceEncodingBudget,
    clock: &mut EncodingClock<'_, impl FnMut() -> Instant>,
    usage: &mut TeletextAuthoredServiceEncodingUsage,
) -> Result<EncodedTeletextAuthoredService, TeletextAuthoredServiceEncodingError> {
    clock.check(None, None)?;
    let count = preflight(&declared, transmissions, budget)?;
    clock.check(None, None)?;
    let transmissions = lifecycle_index(&declared, transmissions)?;
    clock.check(None, None)?;
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
    clock.check(None, None)?;
    // The same final resource bodies supply C2 evidence before either display
    // pass. Keep them private until every display and final inventory succeeds.
    let resources =
        encode_original_resources(&declared, &transmissions, &allocations, budget, clock)?;
    let resource_context = ResourceTransmissionContext::new(
        declared.semantic().objects(),
        &allocations,
        &resources,
        budget.max_commands,
        budget.normalization.max_resource_objects,
        budget.max_pages,
    )
    .map_err(|reason| Error {
        resource: reason.resource,
        ..error(Failure::ResourceTransmission(Box::new(reason)))
    })?;
    clock.check(None, None)?;
    let mut context =
        TeletextCompileContext::for_service(Some(&resource_context), clock.deadline, budget);
    context.declared_foreign_defaults = true;
    let prepared = compiler_drcs::prepare(&declared, &defaults, budget, &mut context);
    *usage = context.used_work();
    let compiler_drcs::Prepared {
        mut originals,
        effective,
        resources: compiler_drcs_resources,
    } = prepared?;
    clock.check(None, None)?;
    let effective = effective.as_ref().unwrap_or_else(|| declared.semantic());
    let count = count
        .checked_add(compiler_drcs_resources.len())
        .ok_or_else(|| error(Failure::Size { category: "pages" }))?;
    bounded("pages", count, budget.max_pages)?;
    let mut pages = Vec::with_capacity(count);
    let mut inventory = Vec::with_capacity(count);
    let mut units = declared.semantic().magazine_presentations().len();
    for page in &declared.semantic().base().pages {
        clock.check(Some(page.address), None)?;
        let result = encode_display(
            effective,
            page.address,
            &defaults,
            &mut context,
            budget,
            clock,
            originals.remove(&page.address),
        );
        *usage = context.used_work();
        let encoded = result?;
        clock.check(Some(page.address), None)?;
        append_page(encoded, &mut pages, &mut inventory, &mut units, budget)?;
        clock.check(Some(page.address), None)?;
    }
    for encoded in resources {
        let resource = encoded
            .resource_header()
            .map(TeletextResourceHeaderMetadata::identity);
        clock.check(None, resource)?;
        append_page(encoded, &mut pages, &mut inventory, &mut units, budget)?;
        clock.check(None, resource)?;
    }
    append_compiler_resources(
        &compiler_drcs_resources,
        &mut pages,
        &mut inventory,
        &mut units,
        budget,
        clock,
    )?;
    clock.check(None, None)?;
    validate_teletext_physical_page_inventory(&inventory, budget.max_pages)
        .map_err(|reason| error(Failure::Inventory(Box::new(reason))))?;
    clock.check(None, None)?;
    let magazine_units = encode_magazines(&declared);
    clock.check(None, None)?;
    Ok(EncodedTeletextAuthoredService {
        declared,
        compiler_drcs_resources,
        pages,
        magazine_units,
        inventory,
        unit_count: units,
        resource_lifecycle_variants: None,
    })
}

fn encode_original_resources(
    declared: &TeletextDeclaredService,
    transmissions: &BTreeMap<
        TeletextResourceIdentity,
        (usize, TeletextServiceResourceTransmission),
    >,
    allocations: &TeletextResourceObjectAllocations,
    budget: &TeletextAuthoredServiceEncodingBudget,
    clock: &mut EncodingClock<'_, impl FnMut() -> Instant>,
) -> Result<Vec<EncodedTeletextAuthoredPage>, Error> {
    let mut resources = Vec::with_capacity(declared.resource_inventory().len());
    for &resource in declared.resource_inventory() {
        clock.check(None, Some(resource))?;
        let tx = transmissions[&resource].1;
        resources.push(encode_resource(
            declared,
            resource,
            allocations,
            tx,
            budget.max_commands,
        )?);
        clock.check(None, Some(resource))?;
    }
    Ok(resources)
}

fn append_compiler_resources(
    compiler_drcs_resources: &[TeletextCompilerDrcsResource],
    pages: &mut Vec<EncodedTeletextAuthoredPage>,
    inventory: &mut Vec<TeletextPhysicalPageInventoryRecord>,
    units: &mut usize,
    budget: &TeletextAuthoredServiceEncodingBudget,
    clock: &mut EncodingClock<'_, impl FnMut() -> Instant>,
) -> Result<(), Error> {
    for generated in compiler_drcs_resources {
        let resource = generated.table().identity();
        clock.check(None, Some(resource))?;
        let page = encode_teletext_drcs_page(
            generated.table(),
            TeletextResourceUpdateCounter::new(0)
                .map_err(|_| error(Failure::MissingResourceBody))?,
            TeletextResourceFullUpdate::Unchanged,
            TeletextResourceTransmissionMode::Serial,
        )
        .map_err(|reason| Error {
            resource: Some(resource),
            ..error(Failure::Resource(Box::new(reason)))
        })?;
        append_page(
            EncodedTeletextAuthoredPage::Resource(page),
            pages,
            inventory,
            units,
            budget,
        )?;
        clock.check(None, Some(resource))?;
    }
    Ok(())
}

fn encode_magazines(declared: &TeletextDeclaredService) -> Vec<[u8; 46]> {
    declared
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
        .collect()
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
    let stored_pages = count
        .checked_add(declared.resource_inventory().len())
        .ok_or_else(|| error(Failure::Size { category: "pages" }))?;
    sized::<EncodedTeletextAuthoredPage>("pages", stored_pages)?;
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
    service: &ersatzrs_domain::TeletextSemanticService,
    owner: TeletextPageAddress,
    defaults: &[TeletextDefaultObjectSelection],
    context: &mut TeletextCompileContext<'_, '_>,
    budget: &TeletextAuthoredServiceEncodingBudget,
    clock: &mut EncodingClock<'_, impl FnMut() -> Instant>,
    original: Option<TeletextMainComposition>,
) -> Result<EncodedTeletextAuthoredPage, Error> {
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
    clock.check(Some(owner), None)?;
    let local = service
        .page_enhancements()
        .get(&owner)
        .is_some_and(|page| page.local_program.is_some());
    if !local && layout.base_glyphs().is_empty() {
        let composition = compose_teletext_main_program_with_context(
            service,
            owner,
            budget.normalization.composition,
            context,
        )
        .map_err(|reason| located(Failure::Composition(Box::new(reason))))?;
        clock.check(Some(owner), None)?;
        let composition = settle_native_with_context(
            composition,
            service,
            budget.normalization.composition.collection.max_commands,
            context,
        )
        .map_err(|reason| located(Failure::Composition(Box::new(reason))))?;
        clock.check(Some(owner), None)?;
        let encoded = encode_teletext_native_default_page(
            service,
            owner,
            TeletextEncodingRegion::WesternLatin,
        )
        .map_err(|reason| located(Failure::Native(Box::new(reason))))?;
        clock.check(Some(owner), None)?;
        Ok(EncodedTeletextAuthoredPage::NativeDisplay(Box::new(
            EncodedTeletextAuthoredNativeDisplay {
                encoded,
                composition,
            },
        )))
    } else {
        let encoded = super::compiled_display_page::encode_with_original(
            service,
            owner,
            TeletextEncodingRegion::WesternLatin,
            defaults,
            &budget.normalization,
            context,
            original,
        )
        .map_err(|reason| {
            if matches!(
                reason.reason,
                TeletextCompiledDisplayPageEncodingFailure::TimedOut
            ) {
                located(Failure::TimedOut)
            } else {
                located(Failure::Compiled(Box::new(reason)))
            }
        })?;
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
