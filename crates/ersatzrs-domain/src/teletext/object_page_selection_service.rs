//! One complete semantic rebuild for role changes and checked resource programs.
use super::{
    BTreeMap, TeletextMagazinePresentationDraft, TeletextObjectIdentity, TeletextPageAddress,
    TeletextResolvedPageResourceSelection, TeletextResolvedServiceDraft, TeletextSemanticService,
    TeletextServiceBudget, TeletextServiceError, TeletextX27Level25Links,
};
use crate::teletext::{
    TeletextEnhancementProgram, TeletextLocalObjectGlyphProgramOverlay,
    TeletextMainResourceInvocationFailure, TeletextMainResourceInvocationOverlay,
    TeletextResourceObjectProgramOverlay,
};

impl TeletextSemanticService {
    pub(in crate::teletext) fn with_object_page_selection_draft(
        self,
        updates: BTreeMap<
            TeletextPageAddress,
            (
                TeletextX27Level25Links,
                TeletextResolvedPageResourceSelection,
            ),
        >,
        main: &BTreeMap<(TeletextPageAddress, usize), &TeletextMainResourceInvocationOverlay>,
        local: &BTreeMap<TeletextObjectIdentity, &TeletextLocalObjectGlyphProgramOverlay>,
        resources: &BTreeMap<TeletextObjectIdentity, &TeletextResourceObjectProgramOverlay>,
        budget: TeletextServiceBudget,
    ) -> Result<Self, TeletextServiceError> {
        // The declared entry point preflights old/final budgets and exact bodies
        // before reaching this sole clone/reconstruction boundary.
        let mut pages = self.pages;
        for (display, (links, resources)) in updates {
            let page = pages
                .get_mut(&display)
                .ok_or(TeletextServiceError::MissingPage { address: display })?;
            page.compositional_links = Some(links);
            page.resources = resources;
        }
        for page in pages.values_mut() {
            let mut changes = main
                .range((page.address, 0)..=(page.address, usize::MAX))
                .peekable();
            if changes.peek().is_none() {
                continue;
            }
            let failure = |command, reason| TeletextServiceError::MainResourceInvocationOverlay {
                owner: page.address,
                command,
                reason,
            };
            let program = page
                .local_program
                .as_ref()
                .ok_or_else(|| failure(0, TeletextMainResourceInvocationFailure::MissingProgram))?;
            let mut commands = program.commands().to_vec();
            for ((_, ordinal), overlay) in changes {
                *commands.get_mut(*ordinal).ok_or_else(|| {
                    failure(
                        *ordinal,
                        TeletextMainResourceInvocationFailure::MissingCommand,
                    )
                })? = overlay.replacement();
            }
            page.local_program = Some(TeletextEnhancementProgram::new(commands)?);
        }
        let mut objects = self.objects.definitions().to_vec();
        for object in &mut objects {
            if let Some(overlay) = resources.get(&object.identity) {
                object.program = overlay.replacement().clone();
            } else if let Some(overlay) = local.get(&object.identity) {
                object.program = overlay.replacement().clone();
            }
        }
        let draft = TeletextResolvedServiceDraft {
            base: self.base,
            pages: pages.into_values().collect(),
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
        Self::new_with_resolved_mot_fallbacks(
            draft,
            self.object_companions,
            self.mot_fallbacks,
            budget,
        )?
        .with_physical_base_overlays(self.physical_base_overlays.into_values().collect(), budget)?
        .with_base_text_spellings(self.base_text_spellings.into_values().collect(), budget)
    }
    pub(in crate::teletext) fn check_object_page_selection_consumer_result(
        &self,
        local: &BTreeMap<TeletextObjectIdentity, &TeletextLocalObjectGlyphProgramOverlay>,
        resources: &BTreeMap<TeletextObjectIdentity, &TeletextResourceObjectProgramOverlay>,
        budget: TeletextServiceBudget,
    ) -> Result<(), TeletextServiceError> {
        let mut commands = budget.max_commands;
        super::charge(
            &mut commands,
            self.base_text_spellings.len(),
            "authored command",
            budget.max_commands,
        )?;
        for page in self.pages.values() {
            // Main retargets cannot change command count or default suppression.
            super::charge(
                &mut commands,
                page.local_program
                    .as_ref()
                    .map_or(0, |program| program.commands().len()),
                "authored command",
                budget.max_commands,
            )?;
            super::charge(
                &mut commands,
                page.resources.defaults.iter().flatten().count(),
                "authored command",
                budget.max_commands,
            )?;
        }
        for object in self.objects.definitions() {
            let program = if let Some(overlay) = resources.get(&object.identity) {
                overlay.replacement()
            } else if let Some(overlay) = local.get(&object.identity) {
                overlay.replacement()
            } else {
                &object.program
            };
            super::charge(
                &mut commands,
                program.commands().len(),
                "authored command",
                budget.max_commands,
            )?;
        }
        Ok(())
    }
}
