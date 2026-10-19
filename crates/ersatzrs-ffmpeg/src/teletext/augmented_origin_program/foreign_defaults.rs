//! Private physical bodies for exact foreign MOT roots, never semantic aliases.
use super::{Builder, Error, Failure};
use ersatzrs_domain::{
    TeletextEffectiveProgramSource, TeletextEnhancementCommand as Command,
    TeletextEnhancementObjectCatalog, TeletextEnhancementObjectDefinition,
    TeletextEnhancementSource, TeletextObjectIdentity, TeletextObjectScope, TeletextObjectType,
    TeletextSpatialInvocationId, TeletextSpatialRootSource,
};

#[derive(Debug)]
pub(super) struct ForeignDefault {
    physical: TeletextObjectIdentity,
    target: TeletextObjectIdentity,
    original: TeletextSpatialInvocationId,
    composed: usize,
    commands: usize,
    object_type: TeletextObjectType,
}
impl ForeignDefault {
    pub(super) const fn identity(&self) -> TeletextObjectIdentity {
        self.physical
    }
    pub(super) fn target(
        &self,
        original: TeletextSpatialInvocationId,
    ) -> Option<TeletextObjectIdentity> {
        (self.original == original).then_some(self.target)
    }
    pub(super) fn retained_source(
        &self,
        source: TeletextEnhancementSource,
    ) -> Option<TeletextEnhancementSource> {
        (source.object == Some(self.physical) && source.command < self.commands).then_some(
            TeletextEnhancementSource {
                object: Some(self.target),
                command: source.command,
            },
        )
    }
    pub(super) fn replace(&self, composed: usize, command: Command) -> Option<Command> {
        if composed != self.composed {
            return None;
        }
        match command {
            Command::InvokeObject {
                target,
                object_type,
                offset,
            } if target == self.target && object_type == self.object_type && offset.is_none() => {
                Some(Command::InvokeObject {
                    target: self.physical,
                    object_type,
                    offset,
                })
            }
            _ => None,
        }
    }
}

pub(super) struct Plan {
    composed: usize,
    slot: u8,
    original: TeletextSpatialInvocationId,
    target: TeletextObjectIdentity,
    object_type: TeletextObjectType,
    commands: usize,
}
impl Builder<'_, '_> {
    /// The opt-in caller already owns complete declared/resource admission. Keep
    /// that capability private; a bare semantic model cannot enable this path.
    pub(super) fn foreign_default_plan(&mut self) -> Result<Vec<Plan>, Error> {
        let composition = self.placement.plan().composition();
        let Some(service) = self.service else {
            return Err(self.error(None, Failure::Correspondence));
        };
        if service
            .page_enhancements()
            .get(&composition.owner())
            .is_some_and(|page| page.local_program.is_some())
        {
            return Ok(Vec::new());
        }
        let TeletextEffectiveProgramSource::Defaults { slots } =
            composition.original_program_source()
        else {
            return Ok(Vec::new());
        };
        if slots.len() > 4 {
            return Err(self.error(None, Failure::Correspondence));
        }
        let mut plan = Vec::new();
        for (composed, provenance) in composition.provenance().iter().enumerate() {
            self.charge(1, None)?;
            let super::TeletextMainCompositionSource::DefaultSlot { slot } = *provenance else {
                continue;
            };
            if plan.iter().any(|existing: &Plan| existing.slot == slot) {
                return Err(self.error(None, Failure::Correspondence));
            }
            if let Some(root) = self.foreign_root(composed, slot, slots)? {
                plan.push(root);
            }
        }
        Ok(plan)
    }

    fn foreign_root(
        &mut self,
        composed: usize,
        slot: u8,
        slots: &[u8],
    ) -> Result<Option<Plan>, Error> {
        let composition = self.placement.plan().composition();
        let service = self
            .service
            .ok_or_else(|| self.error(None, Failure::Correspondence))?;
        let Some(Command::InvokeObject {
            target,
            object_type,
            offset,
        }) = composition.program().commands().get(composed).copied()
        else {
            return Err(self.error(None, Failure::Correspondence));
        };
        let resource = target
            .resource()
            .ok_or_else(|| self.error(None, Failure::Correspondence))?;
        let page = service
            .page_enhancements()
            .get(&composition.owner())
            .ok_or_else(|| self.error(None, Failure::Correspondence))?;
        let selection = match target.scope() {
            TeletextObjectScope::Public => page.resources.objects.public,
            TeletextObjectScope::Global => page.resources.objects.global,
            TeletextObjectScope::Local => return Err(self.error(None, Failure::Correspondence)),
        };
        if selection == Some(resource.page()) {
            return Ok(None);
        }
        let captured = page
            .resources
            .defaults
            .get(usize::from(slot))
            .copied()
            .flatten()
            .ok_or_else(|| self.error(None, Failure::Correspondence))?;
        if !slots.contains(&slot)
            || captured.target != target
            || captured.object_type != object_type
            || offset.is_some()
        {
            return Err(self.error(None, Failure::Correspondence));
        }
        let mut original = None;
        for call in composition.spatial_trace().invocations() {
            self.charge(1, None)?;
            if call.parent().is_none()
                && call.root_source() == (TeletextSpatialRootSource::DefaultSlot { slot })
                && original.replace(*call).is_some()
            {
                return Err(self.error(None, Failure::Correspondence));
            }
        }
        let original = original.ok_or_else(|| self.error(None, Failure::Correspondence))?;
        if original.target() != target
            || original.object_type() != object_type
            || original.offset() != offset
            || original.source().object.is_some()
        {
            return Err(self.error(None, Failure::Correspondence));
        }
        self.charge(self.placement.catalog().definitions().len(), None)?;
        let definition = self
            .placement
            .catalog()
            .definition(target)
            .ok_or_else(|| self.error(None, Failure::Correspondence))?;
        if definition.object_type != object_type {
            return Err(self.error(None, Failure::Correspondence));
        }
        Ok(Some(Plan {
            composed,
            slot,
            original: original.id(),
            target,
            object_type,
            commands: definition.program.commands().len(),
        }))
    }

    pub(super) fn foreign_default_bounds(
        &mut self,
        plan: &[Plan],
        carriers: usize,
        commands: usize,
    ) -> Result<usize, Error> {
        let count = carriers
            .checked_add(plan.len())
            .ok_or_else(|| self.error(None, Failure::AllocationSize))?;
        self.bound("carriers", count, self.budget.max_carriers, None)?;
        self.bound(
            "catalog objects",
            self.placement
                .catalog()
                .definitions()
                .len()
                .checked_add(count)
                .ok_or_else(|| self.error(None, Failure::AllocationSize))?,
            self.budget.recipe.catalog.max_objects,
            None,
        )?;
        let mut total = commands;
        for root in plan {
            self.charge(1, None)?;
            total = total
                .checked_add(root.commands)
                .ok_or_else(|| self.error(None, Failure::AllocationSize))?;
        }
        self.bound(
            "catalog commands",
            total,
            self.budget.recipe.catalog.max_commands,
            None,
        )?;
        self.storage(count, total)?;
        count
            .checked_mul(std::mem::size_of::<ForeignDefault>())
            .filter(|bytes| isize::try_from(*bytes).is_ok())
            .ok_or_else(|| self.error(None, Failure::AllocationSize))?;
        Ok(total)
    }

    pub(super) fn emit_foreign_defaults(
        &mut self,
        mut catalog: TeletextEnhancementObjectCatalog,
        plan: &[Plan],
    ) -> Result<(TeletextEnhancementObjectCatalog, Vec<ForeignDefault>), Error> {
        let mut wrappers = Vec::with_capacity(plan.len());
        for root in plan {
            let previous = self.catalog_commands(&catalog, None)?;
            self.catalog_work(catalog.definitions().len(), previous, 1, None)?;
            let mut admitted = 0;
            let result = super::super::origin_carrier::free_identity(
                self.placement.plan().composition().owner(),
                &catalog,
                self.ids.min(self.budget.recipe.max_id_checks),
                &mut admitted,
            );
            self.ids = self
                .ids
                .checked_sub(admitted)
                .ok_or_else(|| self.error(None, Failure::AllocationSize))?;
            let physical = result.map_err(|reason| {
                let reason = match reason {
                    super::super::TeletextOriginCarrierFailure::InputBudget {
                        kind,
                        count,
                        maximum,
                    } => Failure::InputBudget {
                        kind,
                        count,
                        maximum,
                    },
                    super::super::TeletextOriginCarrierFailure::AllocationSize => {
                        Failure::AllocationSize
                    }
                    _ => Failure::Correspondence,
                };
                let mut error = self.error(None, reason);
                error.root_source =
                    Some(TeletextSpatialRootSource::DefaultSlot { slot: root.slot });
                error.invocation = Some(root.original);
                error
            })?;
            let updated = previous
                .checked_add(root.commands)
                .ok_or_else(|| self.error(None, Failure::AllocationSize))?;
            self.catalog_work(catalog.definitions().len() + 1, updated, 4, None)?;
            let definition = catalog
                .definition(root.target)
                .ok_or_else(|| self.error(None, Failure::Correspondence))?;
            if definition.object_type != root.object_type
                || definition.program.commands().len() != root.commands
            {
                return Err(self.error(None, Failure::Correspondence));
            }
            let wrapper = TeletextEnhancementObjectDefinition {
                identity: physical,
                object_type: root.object_type,
                program: definition.program.clone(),
            };
            let mut definitions = catalog.definitions().to_vec();
            definitions.push(wrapper);
            catalog =
                TeletextEnhancementObjectCatalog::new(definitions, self.budget.recipe.catalog)
                    .map_err(|reason| self.error(None, Failure::Catalog(Box::new(reason))))?;
            wrappers.push(ForeignDefault {
                physical,
                target: root.target,
                original: root.original,
                composed: root.composed,
                commands: root.commands,
                object_type: root.object_type,
            });
        }
        Ok((catalog, wrappers))
    }
}
