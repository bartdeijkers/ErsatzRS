//! Producer-owned augmented candidate and emitter lineage, not wire admission.
//! Original commands and object bodies are retained. Generated calls are ordered
//! only within their placement component; packing and whole-display comparison
//! (including original pending and actual invocation ancestry) remain required.
use super::national_drcs_carrier::{self, NationalDrcsCarrier};
use super::origin_carrier::build_with_national_drcs_and_usage;
use super::{
    TeletextMainCompositionSource, TeletextOriginCarrierBudget, TeletextOriginCarrierCommandRole,
    TeletextOriginCarrierError, TeletextOriginCarrierPlacement,
    TeletextOriginCarrierPlacementPending,
};
use ersatzrs_domain::{
    TeletextEnhancementCommand, TeletextEnhancementObjectCatalog,
    TeletextEnhancementObjectDefinition, TeletextEnhancementProgram,
    TeletextEnhancementProgramError, TeletextEnhancementSource, TeletextGlyphCandidate,
    TeletextObjectCatalogBudget, TeletextObjectCatalogError, TeletextObjectIdentity,
    TeletextObjectType, TeletextPageAddress, TeletextSemanticService, TeletextSpatialInvocationId,
    TeletextSpatialRootSource,
};

mod foreign_defaults;
use foreign_defaults::ForeignDefault;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextAugmentedOriginProgramBudget {
    pub max_input_commands: usize,
    pub max_output_commands: usize,
    pub max_carriers: usize,
    /// BOTH derivation passes: catalogue entries plus lowest-free ID probes.
    pub max_identity_checks: usize,
    /// Assembly scans and catalogue entry/command visits, not a CPU-time guarantee.
    pub max_work: usize,
    pub recipe: TeletextOriginCarrierBudget,
}

/// Separate admitted assembly reservations and catalogue/ID checks across both
/// recipe passes, including failed recipes. These are not total CPU/RSS costs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextAugmentedOriginProgramUsage {
    pub work: usize,
    pub identity_checks: usize,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextAugmentedOriginSource {
    RetainedMain {
        composed_command: usize,
        provenance: TeletextMainCompositionSource,
    },
    RetainedObject {
        identity: TeletextObjectIdentity,
        command: usize,
    },
    CarrierCall {
        carrier: usize,
    },
    CarrierBody {
        carrier: usize,
        role: TeletextOriginCarrierCommandRole,
    },
}
#[derive(Debug)]
pub struct TeletextAugmentedOriginCarrier {
    origin: usize,
    candidate: usize,
    component: usize,
    call: usize,
    identity: TeletextObjectIdentity,
    roles: Vec<TeletextOriginCarrierCommandRole>,
    location: Option<TeletextEnhancementSource>,
    root_source: Option<TeletextSpatialRootSource>,
    invocation: Option<TeletextSpatialInvocationId>,
    national_drcs: Option<NationalDrcsCarrier>,
}
impl TeletextAugmentedOriginCarrier {
    #[must_use]
    pub const fn original_origin_index(&self) -> usize {
        self.origin
    }
    #[must_use]
    pub const fn original_candidate_index(&self) -> usize {
        self.candidate
    }
    #[must_use]
    pub const fn component_index(&self) -> usize {
        self.component
    }
    #[must_use]
    pub const fn call_index(&self) -> usize {
        self.call
    }
    #[must_use]
    pub const fn identity(&self) -> TeletextObjectIdentity {
        self.identity
    }
    #[must_use]
    pub fn command_roles(&self) -> &[TeletextOriginCarrierCommandRole] {
        &self.roles
    }
    #[must_use]
    pub const fn original_source(&self) -> Option<TeletextEnhancementSource> {
        self.location
    }
    #[must_use]
    pub const fn original_root_source(&self) -> Option<TeletextSpatialRootSource> {
        self.root_source
    }
    /// Original semantic instance, NOT the augmented physical invocation ID.
    #[must_use]
    pub const fn original_invocation(&self) -> Option<TeletextSpatialInvocationId> {
        self.invocation
    }
    pub(super) const fn national_drcs(&self) -> Option<NationalDrcsCarrier> {
        self.national_drcs
    }
}
/// Exact borrowed original placement plus owned candidate. The source map is
/// derived internally; it maps emitters, never instance ancestry or generation.
#[derive(Debug)]
pub struct TeletextAugmentedOriginProgram<'a> {
    placement: &'a TeletextOriginCarrierPlacement<'a>,
    program: TeletextEnhancementProgram,
    catalog: TeletextEnhancementObjectCatalog,
    object_sources: Option<super::object_header_positions::ObjectHeaderSources>,
    main_sources: Vec<TeletextAugmentedOriginSource>,
    carriers: Vec<TeletextAugmentedOriginCarrier>,
    foreign_defaults: Vec<ForeignDefault>,
}
impl TeletextAugmentedOriginProgram<'_> {
    /// Preserve retained object ordinals and distinguish the derived AP writer.
    /// Neither role identifies an original physical instance or grants edits.
    pub(super) fn original_object_source(
        &self,
        source: TeletextEnhancementSource,
    ) -> Option<TeletextMainCompositionSource> {
        let identity = source.object?;
        if self
            .foreign_defaults
            .iter()
            .any(|wrapper| wrapper.identity() == identity)
        {
            return None;
        }
        if let Some(TeletextAugmentedOriginSource::RetainedObject { identity, command }) =
            self.emitter_source(source)
        {
            return Some(TeletextMainCompositionSource::Object { identity, command });
        }
        self.object_sources
            .as_ref()?
            .materialized_position(source)
            .map(|command| TeletextMainCompositionSource::HeaderPosition { command })
    }

    #[must_use]
    pub const fn placement(&self) -> &TeletextOriginCarrierPlacement<'_> {
        self.placement
    }
    #[must_use]
    pub const fn program(&self) -> &TeletextEnhancementProgram {
        &self.program
    }
    #[must_use]
    pub const fn catalog(&self) -> &TeletextEnhancementObjectCatalog {
        &self.catalog
    }
    #[must_use]
    pub fn main_sources(&self) -> &[TeletextAugmentedOriginSource] {
        &self.main_sources
    }
    #[must_use]
    pub fn carriers(&self) -> &[TeletextAugmentedOriginCarrier] {
        &self.carriers
    }
    /// Private comparison/ancestry mapping, never authored editor provenance.
    pub(super) fn foreign_default_target(
        &self,
        physical: TeletextObjectIdentity,
        original: TeletextSpatialInvocationId,
    ) -> Option<TeletextObjectIdentity> {
        self.foreign_defaults
            .iter()
            .find(|wrapper| wrapper.identity() == physical)?
            .target(original)
    }
    pub(super) fn foreign_default_count(&self) -> usize {
        self.foreign_defaults.len()
    }

    /// Only the normalized factory may derive retained definition bodies. Raw
    /// assembly keeps its existing original-only behaviour and source mapping.
    pub(super) fn materialize_object_headers(
        &mut self,
        budget: TeletextObjectCatalogBudget,
        maximum_work: usize,
        context: &mut super::compile_context::TeletextCompileContext<'_, '_>,
    ) -> Result<(), super::TeletextMainCompositionError> {
        let derived = context.object_headers(
            self.placement.plan().composition(),
            &self.catalog,
            budget,
            maximum_work,
        )?;
        let (catalog, sources) = derived.into_parts();
        self.catalog = catalog;
        self.object_sources = Some(sources);
        Ok(())
    }
    /// Validates main/body ordinal ranges against the exact retained or generated
    /// definition. A repeated body emitter does not identify its physical instance.
    #[must_use]
    pub fn emitter_source(
        &self,
        source: TeletextEnhancementSource,
    ) -> Option<TeletextAugmentedOriginSource> {
        let Some(identity) = source.object else {
            return self.main_sources.get(source.command).copied();
        };
        if let Some((carrier, value)) = self
            .carriers
            .iter()
            .enumerate()
            .find(|(_, value)| value.identity == identity)
        {
            return value
                .roles
                .get(source.command)
                .copied()
                .map(|role| TeletextAugmentedOriginSource::CarrierBody { carrier, role });
        }
        let source = match &self.object_sources {
            Some(sources) => sources.retained_source(source)?,
            None => source,
        };
        let source = if let Some(wrapper) = self
            .foreign_defaults
            .iter()
            .find(|wrapper| wrapper.identity() == identity)
        {
            wrapper.retained_source(source)?
        } else {
            source
        };
        let identity = source.object?;
        self.placement
            .catalog()
            .definition(identity)?
            .program
            .commands()
            .get(source.command)?;
        Some(TeletextAugmentedOriginSource::RetainedObject {
            identity,
            command: source.command,
        })
    }
}
#[derive(Debug, thiserror::Error)]
pub enum TeletextAugmentedOriginProgramFailure {
    #[error("{kind} count {count} exceeds trusted maximum {maximum}")]
    InputBudget {
        kind: &'static str,
        count: usize,
        maximum: usize,
    },
    #[error("assembly work exceeds trusted maximum {maximum}")]
    WorkBudget { maximum: usize },
    #[error("bounded assembly storage/work arithmetic exceeds supported range")]
    AllocationSize,
    #[error("component {component} has unresolved placement {pending:?}")]
    Placement {
        component: usize,
        pending: TeletextOriginCarrierPlacementPending,
    },
    #[error("exact placement/source correspondence failed")]
    Correspondence,
    #[error("original origin recipe cannot be derived: {0}")]
    Carrier(Box<TeletextOriginCarrierError>),
    #[error("augmented catalogue is invalid: {0}")]
    Catalog(Box<TeletextObjectCatalogError>),
    #[error("augmented main is invalid: {0}")]
    Program(TeletextEnhancementProgramError),
}
#[derive(Debug, thiserror::Error)]
#[error("teletext augmented candidate for {owner:?}, origin {origin:?}: {reason}")]
pub struct TeletextAugmentedOriginProgramError {
    pub owner: TeletextPageAddress,
    pub origin: Option<usize>,
    pub location: Option<TeletextEnhancementSource>,
    pub root_source: Option<TeletextSpatialRootSource>,
    pub invocation: Option<TeletextSpatialInvocationId>,
    pub reason: TeletextAugmentedOriginProgramFailure,
}
/// Two bounded recipe passes establish exact final command counts before cloning
/// original inputs. Both passes charge ID scans/probes; emission derives against
/// the growing checked catalogue, so each lowest-free exact-owner ID is distinct.
/// # Errors
/// Rejects budgets, pending components or located recipe uncertainty. No partial
/// result or input mutation escapes. Even no carriers retains original pending
/// via `placement()` and is not a readiness or whole-plane equivalence claim.
pub fn build_teletext_augmented_origin_program<'a>(
    placement: &'a TeletextOriginCarrierPlacement<'a>,
    budget: TeletextAugmentedOriginProgramBudget,
) -> Result<TeletextAugmentedOriginProgram<'a>, TeletextAugmentedOriginProgramError> {
    build_teletext_augmented_origin_program_with_usage(placement, budget).0
}

/// Build the same candidate and report admitted assembly work on either outcome.
///
/// The meter is the existing `max_work` reservation counter, including both
/// recipe passes. A rejected reservation consumes no credit; earlier admitted
/// work is retained after failure. Identity probes have a separate budget and
/// are not included in this assembly-work count.
/// # Errors
/// Returns the ordinary assembly result with unchanged diagnostics and no partial
/// candidate. The usage count does not establish total CPU or memory consumption.
pub fn build_teletext_augmented_origin_program_with_usage<'a>(
    placement: &'a TeletextOriginCarrierPlacement<'a>,
    budget: TeletextAugmentedOriginProgramBudget,
) -> (
    Result<TeletextAugmentedOriginProgram<'a>, TeletextAugmentedOriginProgramError>,
    usize,
) {
    let (result, usage) =
        build_teletext_augmented_origin_program_with_work_usage(placement, budget);
    (result, usage.work)
}

/// Build the ordinary candidate with independent admitted assembly and ID usage.
/// Both derivation passes debit successful checks before propagating any recipe
/// failure. Denied reservations consume no credit; previous usage is retained.
/// # Errors
/// Preserves the original located failure and never returns a partial candidate.
pub fn build_teletext_augmented_origin_program_with_work_usage<'a>(
    placement: &'a TeletextOriginCarrierPlacement<'a>,
    budget: TeletextAugmentedOriginProgramBudget,
) -> (
    Result<TeletextAugmentedOriginProgram<'a>, TeletextAugmentedOriginProgramError>,
    TeletextAugmentedOriginProgramUsage,
) {
    build_with_optional_service_and_work_usage(placement, None, budget, false)
}

pub(super) fn build_augmented_with_service_and_work_usage<'a>(
    placement: &'a TeletextOriginCarrierPlacement<'a>,
    service: &TeletextSemanticService,
    budget: TeletextAugmentedOriginProgramBudget,
) -> (
    Result<TeletextAugmentedOriginProgram<'a>, TeletextAugmentedOriginProgramError>,
    TeletextAugmentedOriginProgramUsage,
) {
    build_with_optional_service_and_work_usage(placement, Some(service), budget, false)
}

/// Only a fully declared resource-bound factory may request this physical lowering.
pub(super) fn build_augmented_with_foreign_defaults_and_work_usage<'a>(
    placement: &'a TeletextOriginCarrierPlacement<'a>,
    service: &TeletextSemanticService,
    budget: TeletextAugmentedOriginProgramBudget,
) -> (
    Result<TeletextAugmentedOriginProgram<'a>, TeletextAugmentedOriginProgramError>,
    TeletextAugmentedOriginProgramUsage,
) {
    build_with_optional_service_and_work_usage(placement, Some(service), budget, true)
}

fn build_with_optional_service_and_work_usage<'a>(
    placement: &'a TeletextOriginCarrierPlacement<'a>,
    service: Option<&TeletextSemanticService>,
    budget: TeletextAugmentedOriginProgramBudget,
    realize_foreign_defaults: bool,
) -> (
    Result<TeletextAugmentedOriginProgram<'a>, TeletextAugmentedOriginProgramError>,
    TeletextAugmentedOriginProgramUsage,
) {
    let mut builder = Builder {
        placement,
        service,
        budget,
        work: budget.max_work,
        ids: budget.max_identity_checks,
    };
    let result = (|| {
        let (count, original_commands) = builder.preflight()?;
        let defaults = if realize_foreign_defaults {
            builder.foreign_default_plan()?
        } else {
            Vec::new()
        };
        if !defaults.is_empty() {
            builder.foreign_default_bounds(&defaults, count, original_commands)?;
        }
        let mut commands = original_commands;
        for component in placement.components() {
            for call in component.calls() {
                let recipe = builder.recipe(call.original_origin_index(), placement.catalog())?;
                commands = commands
                    .checked_add(recipe.definition().program.commands().len())
                    .ok_or_else(|| {
                        builder.error(Some(call.original_origin_index()), Failure::AllocationSize)
                    })?;
            }
        }
        builder.bound(
            "catalog commands",
            commands,
            budget.recipe.catalog.max_commands,
            None,
        )?;
        builder.storage(count, commands)?;
        if !defaults.is_empty() {
            builder.foreign_default_bounds(&defaults, count, commands)?;
        }
        let (catalog, carriers) = builder.emit_catalog(count, original_commands)?;
        let (catalog, foreign_defaults) = if defaults.is_empty() {
            (catalog, Vec::new())
        } else {
            builder.emit_foreign_defaults(catalog, &defaults)?
        };
        let (program, sources) = builder.emit_main(&carriers, count, &foreign_defaults)?;
        Ok(TeletextAugmentedOriginProgram {
            placement,
            program,
            catalog,
            object_sources: None,
            main_sources: sources,
            carriers,
            foreign_defaults,
        })
    })();
    (
        result,
        TeletextAugmentedOriginProgramUsage {
            work: budget.max_work - builder.work,
            identity_checks: budget.max_identity_checks - builder.ids,
        },
    )
}
type Failure = TeletextAugmentedOriginProgramFailure;
type Error = TeletextAugmentedOriginProgramError;
struct Builder<'a, 's> {
    placement: &'a TeletextOriginCarrierPlacement<'a>,
    service: Option<&'s TeletextSemanticService>,
    budget: TeletextAugmentedOriginProgramBudget,
    work: usize,
    ids: usize,
}
impl<'a> Builder<'a, '_> {
    fn emit_catalog(
        &mut self,
        count: usize,
        original_commands: usize,
    ) -> Result<
        (
            TeletextEnhancementObjectCatalog,
            Vec<TeletextAugmentedOriginCarrier>,
        ),
        Error,
    > {
        // All final raw counts, exact body counts and sidecar/storage fences above.
        self.catalog_work(
            self.placement.catalog().definitions().len(),
            original_commands,
            1,
            None,
        )?;
        let mut catalog = self.placement.catalog().clone();
        let mut carriers = Vec::with_capacity(count);
        for (component_index, component) in self.placement.components().iter().enumerate() {
            for (call_index, call) in component.calls().iter().enumerate() {
                let origin = call.original_origin_index();
                let recipe = self.recipe(origin, &catalog)?;
                let definition = recipe.definition().clone();
                let carrier = TeletextAugmentedOriginCarrier {
                    origin,
                    candidate: recipe.candidate_index(),
                    component: component_index,
                    call: call_index,
                    identity: definition.identity,
                    roles: recipe.command_roles().to_vec(),
                    location: recipe.location(),
                    root_source: recipe.root_source(),
                    invocation: recipe.invocation(),
                    national_drcs: recipe.national_drcs(),
                };
                drop(recipe);
                let previous = self.catalog_commands(&catalog, Some(origin))?;
                // Definition clone/index/closure-validation visits are bounded here;
                // checked catalogue validation remains the authority for its graph.
                let updated = previous
                    .checked_add(definition.program.commands().len())
                    .ok_or_else(|| self.error(Some(origin), Failure::AllocationSize))?;
                self.catalog_work(catalog.definitions().len() + 1, updated, 4, Some(origin))?;
                let mut definitions = catalog.definitions().to_vec();
                definitions.push(definition);
                catalog =
                    TeletextEnhancementObjectCatalog::new(definitions, self.budget.recipe.catalog)
                        .map_err(|error| {
                            self.error(Some(origin), Failure::Catalog(Box::new(error)))
                        })?;
                carriers.push(carrier);
            }
        }
        Ok((catalog, carriers))
    }
    fn emit_main(
        &mut self,
        carriers: &[TeletextAugmentedOriginCarrier],
        count: usize,
        foreign_defaults: &[ForeignDefault],
    ) -> Result<
        (
            TeletextEnhancementProgram,
            Vec<TeletextAugmentedOriginSource>,
        ),
        Error,
    > {
        let main = self.placement.plan().composition().program().commands();
        let output_count = main.len() + count;
        let mut output = Vec::with_capacity(output_count);
        let mut sources = Vec::with_capacity(output_count);
        // gap0 is initial; gap n+1 is after retained composed main command n.
        for gap in 0..=main.len() {
            self.charge(1, None)?;
            for (component_index, component) in self.placement.components().iter().enumerate() {
                self.charge(1, None)?;
                let anchor = component.gap().ok_or_else(|| {
                    self.error(
                        component.origin_indices().first().copied(),
                        Failure::Correspondence,
                    )
                })?;
                if anchor.after_command().map_or(0, |index| index + 1) != gap {
                    continue;
                }
                for (carrier_index, carrier) in carriers.iter().enumerate() {
                    self.charge(1, Some(carrier.origin))?;
                    if carrier.component != component_index {
                        continue;
                    }
                    let call = component.calls()[carrier.call];
                    output.push(TeletextEnhancementCommand::InvokeObject {
                        target: carrier.identity,
                        object_type: TeletextObjectType::Passive,
                        offset: Some(call.offset()),
                    });
                    sources.push(TeletextAugmentedOriginSource::CarrierCall {
                        carrier: carrier_index,
                    });
                }
            }
            if let Some(&command) = main.get(gap) {
                self.charge(foreign_defaults.len(), None)?;
                let command = foreign_defaults
                    .iter()
                    .find_map(|wrapper| wrapper.replace(gap, command))
                    .unwrap_or(command);
                output.push(command);
                sources.push(TeletextAugmentedOriginSource::RetainedMain {
                    composed_command: gap,
                    provenance: self.placement.plan().composition().provenance()[gap],
                });
            }
        }
        if output.len() != output_count {
            return Err(self.error(None, Failure::Correspondence));
        }
        self.charge(output_count, None)?;
        let program = TeletextEnhancementProgram::new(output)
            .map_err(|error| self.error(None, Failure::Program(error)))?;
        Ok((program, sources))
    }
    fn error(&self, origin: Option<usize>, reason: Failure) -> Error {
        let composition = self.placement.plan().composition();
        let candidate = origin
            .and_then(|index| composition.display_resolution().origins().get(index))
            .and_then(|o| {
                composition
                    .glyph_layers()
                    .candidates()
                    .get(o.candidate_index())
            });
        Error {
            owner: composition.owner(),
            origin,
            location: candidate.and_then(TeletextGlyphCandidate::source),
            root_source: candidate.and_then(TeletextGlyphCandidate::root_source),
            invocation: candidate.and_then(TeletextGlyphCandidate::invocation),
            reason,
        }
    }
    fn bound(
        &self,
        kind: &'static str,
        count: usize,
        maximum: usize,
        origin: Option<usize>,
    ) -> Result<(), Error> {
        if count > maximum {
            Err(self.error(
                origin,
                Failure::InputBudget {
                    kind,
                    count,
                    maximum,
                },
            ))
        } else {
            Ok(())
        }
    }
    fn charge(&mut self, count: usize, origin: Option<usize>) -> Result<(), Error> {
        self.work = self.work.checked_sub(count).ok_or_else(|| {
            self.error(
                origin,
                Failure::WorkBudget {
                    maximum: self.budget.max_work,
                },
            )
        })?;
        Ok(())
    }
    fn catalog_work(
        &mut self,
        objects: usize,
        commands: usize,
        factor: usize,
        origin: Option<usize>,
    ) -> Result<(), Error> {
        let work = objects
            .checked_add(commands)
            .and_then(|n| n.checked_mul(factor))
            .ok_or_else(|| self.error(origin, Failure::AllocationSize))?;
        self.charge(work, origin)
    }
    fn catalog_commands(
        &mut self,
        catalog: &TeletextEnhancementObjectCatalog,
        origin: Option<usize>,
    ) -> Result<usize, Error> {
        self.charge(catalog.definitions().len(), origin)?;
        catalog.definitions().iter().try_fold(0usize, |n, d| {
            n.checked_add(d.program.commands().len())
                .ok_or_else(|| self.error(origin, Failure::AllocationSize))
        })
    }
    fn recipe<'b>(
        &mut self,
        origin: usize,
        catalog: &'b TeletextEnhancementObjectCatalog,
    ) -> Result<super::TeletextOriginCarrier<'b>, Error>
    where
        'a: 'b,
    {
        let commands = self.catalog_commands(catalog, Some(origin))?;
        self.catalog_work(catalog.definitions().len(), commands, 1, Some(origin))?;
        let mut budget = self.budget.recipe;
        budget.max_id_checks = budget.max_id_checks.min(self.ids);
        let national_drcs = if let Some(service) = self.service.filter(|_| {
            national_drcs_carrier::needs_binding(self.placement.plan().composition(), origin)
        }) {
            self.charge(service.drcs().subtables().len(), Some(origin))?;
            let work = national_drcs_carrier::scan_work(service)
                .ok_or_else(|| self.error(Some(origin), Failure::AllocationSize))?;
            self.charge(work, Some(origin))?;
            national_drcs_carrier::resolve(service, self.placement.plan().composition(), origin)
        } else {
            None
        };
        let (result, admitted) = build_with_national_drcs_and_usage(
            self.placement.plan().composition(),
            origin,
            catalog,
            budget,
            national_drcs,
        );
        self.ids = self
            .ids
            .checked_sub(admitted)
            .ok_or_else(|| self.error(Some(origin), Failure::AllocationSize))?;
        result.map_err(|error| self.error(Some(origin), Failure::Carrier(Box::new(error))))
    }
    fn preflight(&mut self) -> Result<(usize, usize), Error> {
        let composition = self.placement.plan().composition();
        let main = composition.program().commands().len();
        let catalog = self.placement.catalog();
        for (kind, count, maximum) in [
            ("input commands", main, self.budget.max_input_commands),
            (
                "input catalog objects",
                catalog.definitions().len(),
                self.budget.recipe.catalog.max_objects,
            ),
            (
                "candidates",
                composition.glyph_layers().candidates().len(),
                self.budget.recipe.max_candidates,
            ),
            (
                "origins",
                composition.display_resolution().origins().len(),
                self.budget.recipe.max_origins,
            ),
        ] {
            self.bound(kind, count, maximum, None)?;
        }
        self.charge(self.placement.components().len(), None)?;
        let count = self
            .placement
            .components()
            .iter()
            .try_fold(0usize, |n, c| {
                n.checked_add(c.origin_indices().len())
                    .ok_or_else(|| self.error(None, Failure::AllocationSize))
            })?;
        self.bound("carriers", count, self.budget.max_carriers, None)?;
        self.bound(
            "output commands",
            main.checked_add(count)
                .ok_or_else(|| self.error(None, Failure::AllocationSize))?,
            self.budget.max_output_commands,
            None,
        )?;
        self.bound(
            "catalog objects",
            catalog
                .definitions()
                .len()
                .checked_add(count)
                .ok_or_else(|| self.error(None, Failure::AllocationSize))?,
            self.budget.recipe.catalog.max_objects,
            None,
        )?;
        self.validate_components(main)?;
        let commands = self.catalog_commands(catalog, None)?;
        self.bound(
            "input catalog commands",
            commands,
            self.budget.recipe.catalog.max_commands,
            None,
        )?;
        self.storage(
            count,
            commands
                .checked_add(
                    count
                        .checked_mul(6)
                        .ok_or_else(|| self.error(None, Failure::AllocationSize))?,
                )
                .ok_or_else(|| self.error(None, Failure::AllocationSize))?,
        )?;
        Ok((count, commands))
    }
    fn validate_components(&mut self, main: usize) -> Result<(), Error> {
        let composition = self.placement.plan().composition();
        if main != composition.provenance().len() {
            return Err(self.error(None, Failure::Correspondence));
        }
        for (index, component) in self.placement.components().iter().enumerate() {
            self.charge(1, component.origin_indices().first().copied())?;
            if let Some(pending) = component.pending() {
                let origin = match pending {
                    TeletextOriginCarrierPlacementPending::OriginalPlan { origin }
                    | TeletextOriginCarrierPlacementPending::UnknownRectangle { origin } => {
                        Some(origin)
                    }
                    TeletextOriginCarrierPlacementPending::CyclicPrecedence
                    | TeletextOriginCarrierPlacementPending::NoUnchangedGap => {
                        component.origin_indices().first().copied()
                    }
                };
                return Err(self.error(
                    origin,
                    Failure::Placement {
                        component: index,
                        pending,
                    },
                ));
            }
            if component
                .gap()
                .is_none_or(|g| g.after_command().is_some_and(|n| n >= main))
                || component.calls().len() != component.origin_indices().len()
            {
                return Err(self.error(None, Failure::Correspondence));
            }
            for (call, &origin) in component.calls().iter().zip(component.origin_indices()) {
                self.charge(1, Some(origin))?;
                if call.original_origin_index() != origin {
                    return Err(self.error(Some(origin), Failure::Correspondence));
                }
            }
        }
        Ok(())
    }
    fn storage(&self, count: usize, commands: usize) -> Result<(), Error> {
        let main = self
            .placement
            .plan()
            .composition()
            .program()
            .commands()
            .len()
            .checked_add(count)
            .ok_or_else(|| self.error(None, Failure::AllocationSize))?;
        let objects = self
            .placement
            .catalog()
            .definitions()
            .len()
            .checked_add(count)
            .ok_or_else(|| self.error(None, Failure::AllocationSize))?;
        let mut total = 0usize;
        for (count, size) in [
            (main, std::mem::size_of::<TeletextEnhancementCommand>()),
            (main, std::mem::size_of::<TeletextAugmentedOriginSource>()),
            (count, std::mem::size_of::<TeletextAugmentedOriginCarrier>()),
            (
                objects,
                3 * (std::mem::size_of::<TeletextEnhancementObjectDefinition>()
                    + std::mem::size_of::<(TeletextObjectIdentity, usize)>()),
            ),
            (
                commands,
                3 * std::mem::size_of::<TeletextEnhancementCommand>(),
            ),
            (
                count,
                6 * std::mem::size_of::<TeletextOriginCarrierCommandRole>(),
            ),
        ] {
            total = total
                .checked_add(
                    count
                        .checked_mul(size)
                        .ok_or_else(|| self.error(None, Failure::AllocationSize))?,
                )
                .ok_or_else(|| self.error(None, Failure::AllocationSize))?;
        }
        if isize::try_from(total).is_err() {
            return Err(self.error(None, Failure::AllocationSize));
        }
        main.checked_add(1)
            .and_then(|m| m.checked_mul(self.placement.components().len().max(1)))
            .and_then(|n| n.checked_mul(count.max(1)))
            .and_then(|_| {
                objects
                    .checked_add(commands)
                    .and_then(|n| n.checked_mul(count.max(1)).and_then(|n| n.checked_mul(8)))
            })
            .ok_or_else(|| self.error(None, Failure::AllocationSize))?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "augmented_origin_program/foreign_defaults_tests.rs"]
mod foreign_defaults_tests;
