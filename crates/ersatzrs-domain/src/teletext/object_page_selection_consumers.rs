//! One final selected-role draft for original Main, Local and resource consumers.
use super::{TeletextObjectPageSelectionChange, TeletextObjectPageSelectionError};
use crate::teletext::{
    TeletextDeclaredService, TeletextDeclaredServiceBudget, TeletextLocalObjectGlyphProgramOverlay,
    TeletextMainResourceInvocationOverlay, TeletextResourceObjectProgramOverlay,
    TeletextServiceBudget,
};
use std::collections::BTreeMap;

/// Borrowed checked consumers; no uploaded source or publication authority.
///
/// Main entries only retarget retained calls. Local and POP/GPOP entries keep
/// their existing independently checked program correspondence contracts.
#[derive(Debug, Clone, Copy, Default)]
pub struct TeletextObjectPageSelectionConsumers<'a> {
    pub main: &'a [TeletextMainResourceInvocationOverlay],
    pub local: &'a [TeletextLocalObjectGlyphProgramOverlay],
    pub resources: &'a [TeletextResourceObjectProgramOverlay],
}
impl TeletextDeclaredService {
    /// Change selected POP/GPOP roles and every checked consumer in one draft.
    ///
    /// Existing Main/Local/resource originals are authenticated before cloning;
    /// no incomplete intermediate service escapes. Main ordering, type, offset,
    /// noncalls and explicit-program presence remain exact. Complete final graph,
    /// declarations, defaults, resource usage and budgets are revalidated. Hosts
    /// still prove source ownership, protected effects, native packing and publish.
    /// # Errors
    /// Rejects entirely empty intent, duplicate/stale/missing original consumers,
    /// invalid role changes, incomplete final dependencies and trusted limits.
    pub fn with_object_page_selections_and_consumers(
        self,
        changes: &[TeletextObjectPageSelectionChange],
        consumers: TeletextObjectPageSelectionConsumers<'_>,
        service_budget: TeletextServiceBudget,
        declaration_budget: TeletextDeclaredServiceBudget,
    ) -> Result<Self, TeletextObjectPageSelectionError> {
        use TeletextObjectPageSelectionError as Error;
        if changes.is_empty()
            && consumers.main.is_empty()
            && consumers.local.is_empty()
            && consumers.resources.is_empty()
        {
            return Err(Error::EmptyConsumers);
        }
        check_raw_consumers(changes.len(), consumers, service_budget)?;
        self.check_object_page_selection_budgets(
            changes,
            consumers.resources,
            service_budget,
            declaration_budget,
        )?;
        let main = self
            .semantic
            .index_main_resource_invocations(consumers.main)?;
        let local = self
            .semantic
            .validate_local_object_glyph_program_overlays(consumers.local)?;
        let resources = self.semantic.index_resource_programs(consumers.resources)?;
        self.semantic.check_object_page_selection_consumer_result(
            &local,
            &resources,
            service_budget,
        )?;
        let mut updates = BTreeMap::new();
        for (ordinal, change) in changes.iter().enumerate() {
            let display = change.display();
            let role = change.role();
            if changes[..ordinal]
                .iter()
                .any(|other| other.display() == display && other.role() == role)
            {
                return Err(Error::Duplicate { display, role });
            }
            let update =
                self.object_page_selection_update(*change, updates.get(&display).copied())?;
            updates.insert(display, update);
        }
        let semantic = self.semantic.with_object_page_selection_draft(
            updates,
            &main,
            &local,
            &resources,
            service_budget,
        )?;
        Ok(Self::new(
            semantic,
            self.mots,
            self.selections,
            self.empty_object_pages,
            declaration_budget,
        )?)
    }
}

fn check_raw_consumers(
    selections: usize,
    consumers: TeletextObjectPageSelectionConsumers<'_>,
    budget: TeletextServiceBudget,
) -> Result<(), TeletextObjectPageSelectionError> {
    use super::super::super::service::charge;
    // Combined raw counts, both program bodies and correspondence maps precede
    // original indexing and catalogue/body clones, even for repeated intents.
    let mut objects = budget.max_objects;
    charge(
        &mut objects,
        consumers.local.len(),
        "object",
        budget.max_objects,
    )?;
    charge(
        &mut objects,
        consumers.resources.len(),
        "object",
        budget.max_objects,
    )?;
    let mut commands = budget.max_commands;
    let mut items = budget.max_structure_items;
    charge(
        &mut items,
        selections,
        "structure item",
        budget.max_structure_items,
    )?;
    charge(
        &mut items,
        consumers.main.len(),
        "structure item",
        budget.max_structure_items,
    )?;
    for _ in consumers.main {
        charge(&mut commands, 2, "authored command", budget.max_commands)?;
    }
    for overlay in consumers.local {
        for program in [overlay.expected(), overlay.replacement()] {
            charge(
                &mut commands,
                program.commands().len(),
                "authored command",
                budget.max_commands,
            )?;
        }
        charge(
            &mut items,
            overlay.retained_commands().len(),
            "structure item",
            budget.max_structure_items,
        )?;
    }
    for overlay in consumers.resources {
        for program in [overlay.expected(), overlay.replacement()] {
            charge(
                &mut commands,
                program.commands().len(),
                "authored command",
                budget.max_commands,
            )?;
        }
        charge(
            &mut items,
            overlay.retained_commands().len(),
            "structure item",
            budget.max_structure_items,
        )?;
    }
    Ok(())
}
