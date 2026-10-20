//! Atomic native bank creation; retained-bank Add retains its stricter contract.
use super::{
    charge, check_physical_overlay_budgets, check_resource_records,
    TeletextMagazinePresentationDraft, TeletextResolvedServiceDraft, TeletextSemanticService,
    TeletextServiceBudget, TeletextServiceError,
};
use crate::teletext::{
    TeletextObjectResourceAddition, TeletextObjectResourceAdditionApplyError,
    TeletextResourceObjectDefinitionAddition,
};

impl TeletextSemanticService {
    /// Extend the complete catalogue with distinct, previously absent POP/GPOP banks.
    ///
    /// All definitions are admitted together so forward and cross-bank graph
    /// dependencies use the ordinary complete service validator. Existing pages,
    /// definitions, defaults, resolved MOT fallbacks, DRCS, physical Base overlays
    /// and native text spellings are preserved. No existing program or selection
    /// is implicitly rewired to the new resources. This grants no authoring or
    /// publication authority and does not establish physical encoding readiness.
    /// # Errors
    /// Rejects empty batches, occupied/repeated logical bank owners, aggregate
    /// limits and all ordinary service graph, type and collection failures.
    pub fn with_object_resource_additions(
        self,
        additions: &[TeletextObjectResourceAddition],
        budget: TeletextServiceBudget,
    ) -> Result<Self, TeletextObjectResourceAdditionApplyError> {
        self.with_object_resource_and_definition_additions(additions, &[], budget)
    }

    /// Add complete new banks and definitions in original banks as one graph.
    ///
    /// Forward references across the two explicit intent classes are validated
    /// together. Definition additions still require an originally present bank;
    /// this does not widen existing-bank Add to implicitly create resources.
    /// All original content and collection provenance remain intact. Host edit,
    /// lifecycle, effect and publication authority remain separate requirements.
    /// # Errors
    /// Rejects an empty bank batch, invalid original-bank additions, occupied
    /// bank owners, combined bounds and ordinary complete-service failures.
    pub fn with_object_resource_and_definition_additions(
        self,
        additions: &[TeletextObjectResourceAddition],
        definitions: &[TeletextResourceObjectDefinitionAddition],
        budget: TeletextServiceBudget,
    ) -> Result<Self, TeletextObjectResourceAdditionApplyError> {
        let populated = if definitions.is_empty() {
            std::collections::BTreeSet::new()
        } else {
            self.check_resource_definition_additions(definitions, budget)?
                .1
        };
        let populated_companions = self
            .object_companions
            .iter()
            .filter(|identity| populated.contains(identity))
            .count();
        let capacity = self.check_object_resource_additions(
            additions,
            definitions,
            populated_companions,
            budget,
        )?;
        let mut objects = Vec::with_capacity(capacity);
        objects.extend_from_slice(self.objects.definitions());
        let mut companions = self
            .object_companions
            .into_iter()
            .filter(|identity| !populated.contains(identity))
            .collect::<Vec<_>>();
        for addition in additions {
            objects.extend_from_slice(addition.definitions());
            if addition.definitions().is_empty() {
                companions.push(addition.resource());
            }
        }
        objects.extend(
            definitions
                .iter()
                .map(|addition| addition.definition().clone()),
        );
        let draft = TeletextResolvedServiceDraft {
            base: self.base,
            pages: self.pages.into_values().collect(),
            magazines: self
                .magazines
                .into_iter()
                .map(
                    |(magazine, presentation)| TeletextMagazinePresentationDraft {
                        magazine,
                        presentation,
                    },
                )
                .collect(),
            objects,
            drcs: self.drcs.subtables().to_vec(),
        };
        Self::new_with_resolved_mot_fallbacks(draft, companions, self.mot_fallbacks, budget)?
            .with_physical_base_overlays(
                self.physical_base_overlays.into_values().collect(),
                budget,
            )?
            .with_base_text_spellings(self.base_text_spellings.into_values().collect(), budget)
            .map_err(TeletextObjectResourceAdditionApplyError::from)
    }

    fn check_object_resource_additions(
        &self,
        additions: &[TeletextObjectResourceAddition],
        definitions: &[TeletextResourceObjectDefinitionAddition],
        populated_companions: usize,
        budget: TeletextServiceBudget,
    ) -> Result<usize, TeletextObjectResourceAdditionApplyError> {
        use TeletextObjectResourceAdditionApplyError as Error;
        if additions.is_empty() {
            return Err(Error::Empty);
        }
        // Bound raw input before indexing new identities or cloning definitions.
        let mut records = budget.max_records;
        charge(&mut records, additions.len(), "record", budget.max_records)?;
        self.check_existing_service_budgets(budget)?;
        let mut inventory = check_resource_records(
            &self.base,
            self.objects.definitions(),
            self.drcs.subtables(),
            &self.object_companions,
            budget.max_records,
        )?;
        charge(
            &mut records,
            self.base.pages.len(),
            "record",
            budget.max_records,
        )?;
        charge(&mut records, inventory.len(), "record", budget.max_records)?;
        let mut objects = budget.max_objects;
        charge(
            &mut objects,
            self.objects.definitions().len(),
            "object",
            budget.max_objects,
        )?;
        charge(
            &mut objects,
            definitions.len(),
            "object",
            budget.max_objects,
        )?;
        let mut commands = self.remaining_object_resource_commands(budget)?;
        for addition in definitions {
            charge(
                &mut commands,
                addition.definition().program.commands().len(),
                "authored command",
                budget.max_commands,
            )?;
        }
        let mut items = budget.max_structure_items;
        charge(
            &mut items,
            // Only original companion sentinels actually removed above are
            // released; definitions still consume object/command budgets.
            self.original_structure_items - populated_companions,
            "structure item",
            budget.max_structure_items,
        )?;
        charge(
            &mut items,
            self.base_text_spellings.len(),
            "structure item",
            budget.max_structure_items,
        )?;
        for addition in additions {
            charge(
                &mut objects,
                addition.definitions().len(),
                "object",
                budget.max_objects,
            )?;
            charge(
                &mut items,
                usize::from(addition.definitions().is_empty()),
                "structure item",
                budget.max_structure_items,
            )?;
            for definition in addition.definitions() {
                charge(
                    &mut commands,
                    definition.program.commands().len(),
                    "authored command",
                    budget.max_commands,
                )?;
            }
            let resource = addition.resource();
            let key = (resource.page(), resource.logical_subpage());
            if inventory.insert(key, resource.kind()).is_some() {
                return Err(Error::Occupied { resource });
            }
        }
        check_physical_overlay_budgets(
            budget.max_structure_items - items,
            self.original_base_content_bytes,
            self.physical_base_overlays.values(),
            budget,
        )?;
        Ok(budget.max_objects - objects)
    }

    fn remaining_object_resource_commands(
        &self,
        budget: TeletextServiceBudget,
    ) -> Result<usize, TeletextServiceError> {
        let mut commands = budget.max_commands;
        for page in self.pages.values() {
            for count in [
                page.local_program
                    .as_ref()
                    .map_or(0, |p| p.commands().len()),
                page.resources.defaults.iter().flatten().count(),
            ] {
                charge(
                    &mut commands,
                    count,
                    "authored command",
                    budget.max_commands,
                )?;
            }
        }
        for definition in self.objects.definitions() {
            charge(
                &mut commands,
                definition.program.commands().len(),
                "authored command",
                budget.max_commands,
            )?;
        }
        charge(
            &mut commands,
            self.base_text_spellings.len(),
            "authored command",
            budget.max_commands,
        )?;
        Ok(commands)
    }
}
