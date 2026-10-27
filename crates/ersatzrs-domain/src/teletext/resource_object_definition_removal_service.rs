//! Rebuild the complete service after an explicit, bounded unused-object removal.
use super::{
    charge, TeletextMagazinePresentationDraft, TeletextResolvedServiceDraft,
    TeletextSemanticService, TeletextServiceBudget,
};
use crate::teletext::{
    TeletextEnhancementCommand, TeletextObjectIdentity,
    TeletextResourceObjectDefinitionRemovalError as Error,
};
use std::collections::BTreeSet;

impl TeletextSemanticService {
    /// Remove unused Public/Global definitions while retaining their resource owners.
    ///
    /// A batch may remove a closed unused subtree, but never a target of a
    /// surviving definition, a Main program or any supplied default (including
    /// suppressed defaults). Empty resources become explicit companions. Every
    /// surviving definition, display, bitmap, context and native spelling passes
    /// through the ordinary full service constructor again.
    ///
    /// This grants no original/copy distinction, edit permission or publication
    /// authority. The host must separately authenticate the selected added copy,
    /// protected original catalogue, current revision and unchanged effects.
    /// # Errors
    /// Rejects empty, duplicate, Local, missing or referenced targets and ordinary
    /// service budget, graph, charset, geometry or dependency failures.
    pub fn without_unused_resource_object_definitions(
        self,
        removals: &[TeletextObjectIdentity],
        budget: TeletextServiceBudget,
    ) -> Result<Self, Error> {
        if removals.is_empty() {
            return Err(Error::Empty);
        }
        self.check_existing_service_budgets(budget)?;
        let mut remaining = budget.max_objects;
        charge(&mut remaining, removals.len(), "object", budget.max_objects)?;
        let mut items = budget.max_structure_items;
        charge(
            &mut items,
            removals.len(),
            "structure item",
            budget.max_structure_items,
        )?;
        let mut removed = BTreeSet::new();
        let mut resources = BTreeSet::new();
        for &identity in removals {
            let resource = identity.resource().ok_or(Error::Scope { identity })?;
            if !removed.insert(identity) {
                return Err(Error::Duplicate { identity });
            }
            if self.objects.definition(identity).is_none() {
                return Err(Error::Missing { identity });
            }
            resources.insert(resource);
        }
        let check_commands = |commands: &[TeletextEnhancementCommand]| {
            for command in commands {
                if let TeletextEnhancementCommand::InvokeObject { target, .. } = command {
                    if removed.contains(target) {
                        return Err(Error::Referenced { identity: *target });
                    }
                }
            }
            Ok(())
        };
        for page in self.pages.values() {
            for selection in page.resources.defaults.iter().flatten() {
                if removed.contains(&selection.target) {
                    return Err(Error::Referenced {
                        identity: selection.target,
                    });
                }
            }
            if let Some(program) = &page.local_program {
                check_commands(program.commands())?;
            }
        }
        for definition in self.objects.definitions() {
            if removed.contains(&definition.identity) {
                continue;
            }
            check_commands(definition.program.commands())?;
        }
        let definitions: Vec<_> = self
            .objects
            .definitions()
            .iter()
            .filter(|definition| !removed.contains(&definition.identity))
            .cloned()
            .collect();
        let mut companions = self.object_companions;
        for resource in resources {
            if !definitions
                .iter()
                .any(|definition| definition.identity.resource() == Some(resource))
            {
                companions.push(resource);
            }
        }
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
            .map_err(Error::from)
    }
}
