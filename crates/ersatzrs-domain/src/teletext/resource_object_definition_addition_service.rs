//! New definitions pass through the unchanged full semantic service collector.
use super::{
    charge, TeletextMagazinePresentationDraft, TeletextResolvedServiceDraft,
    TeletextSemanticService, TeletextServiceBudget,
};
use crate::teletext::{
    TeletextResourceIdentity, TeletextResourceObjectDefinitionAddition,
    TeletextResourceObjectDefinitionAdditionApplyError,
};
use std::collections::BTreeSet;

impl TeletextSemanticService {
    /// Add new Public/Global definitions only within existing POP/GPOP resources.
    ///
    /// Preserves every old definition, page, default, selection and bitmap. An
    /// explicitly empty companion becomes populated without creating a resource.
    /// Raw and combined budgets precede cloning; the ordinary service constructor
    /// validates all graph edges and every executing consumer. This grants no
    /// authoring, package or publication authority, nor physical packing readiness.
    /// # Errors
    /// Rejects empty, duplicate, occupied or absent-resource additions and all
    /// ordinary service dependency, selection, collection or budget failures.
    pub fn with_resource_object_definition_additions(
        self,
        additions: &[TeletextResourceObjectDefinitionAddition],
        budget: TeletextServiceBudget,
    ) -> Result<Self, TeletextResourceObjectDefinitionAdditionApplyError> {
        let (capacity, populated) = self.check_resource_definition_additions(additions, budget)?;
        let mut definitions = Vec::with_capacity(capacity);
        definitions.extend_from_slice(self.objects.definitions());
        definitions.extend(
            additions
                .iter()
                .map(|addition| addition.definition().clone()),
        );
        let companions = self
            .object_companions
            .into_iter()
            .filter(|identity| !populated.contains(identity))
            .collect();
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
            objects: definitions,
            drcs: self.drcs.subtables().to_vec(),
        };
        Self::new_with_resolved_mot_fallbacks(draft, companions, self.mot_fallbacks, budget)?
            .with_physical_base_overlays(
                self.physical_base_overlays.into_values().collect(),
                budget,
            )?
            .with_base_text_spellings(self.base_text_spellings.into_values().collect(), budget)
            .map_err(TeletextResourceObjectDefinitionAdditionApplyError::from)
    }

    fn check_resource_definition_additions(
        &self,
        additions: &[TeletextResourceObjectDefinitionAddition],
        budget: TeletextServiceBudget,
    ) -> Result<
        (usize, BTreeSet<TeletextResourceIdentity>),
        TeletextResourceObjectDefinitionAdditionApplyError,
    > {
        use TeletextResourceObjectDefinitionAdditionApplyError as Error;
        if additions.is_empty() {
            return Err(Error::Empty);
        }
        self.check_existing_service_budgets(budget)?;
        let mut objects = budget.max_objects;
        charge(
            &mut objects,
            self.objects.definitions().len(),
            "object",
            budget.max_objects,
        )?;
        charge(&mut objects, additions.len(), "object", budget.max_objects)?;
        let capacity = budget.max_objects - objects;
        let mut items = budget.max_structure_items;
        charge(
            &mut items,
            additions.len(),
            "structure item",
            budget.max_structure_items,
        )?;
        let mut commands = budget.max_commands;
        for page in self.pages.values() {
            charge(
                &mut commands,
                page.local_program
                    .as_ref()
                    .map_or(0, |p| p.commands().len()),
                "authored command",
                budget.max_commands,
            )?;
            charge(
                &mut commands,
                page.resources.defaults.iter().flatten().count(),
                "authored command",
                budget.max_commands,
            )?;
        }
        for definition in self.objects.definitions().iter().chain(
            additions
                .iter()
                .map(TeletextResourceObjectDefinitionAddition::definition),
        ) {
            charge(
                &mut commands,
                definition.program.commands().len(),
                "authored command",
                budget.max_commands,
            )?;
        }
        let mut identities = BTreeSet::new();
        let mut populated = BTreeSet::new();
        for addition in additions {
            let identity = addition.identity();
            if !identities.insert(identity) {
                return Err(Error::Duplicate { identity });
            }
            if self.objects.definition(identity).is_some() {
                return Err(Error::Occupied { identity });
            }
            let resource = addition.resource();
            if !self.object_companions.contains(&resource)
                && !self
                    .objects
                    .definitions()
                    .iter()
                    .any(|definition| definition.identity.resource() == Some(resource))
            {
                return Err(Error::MissingResource { identity: resource });
            }
            populated.insert(resource);
        }
        Ok((capacity, populated))
    }
}
