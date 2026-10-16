//! Exact-original local object transformations, sharing the complete service gates.
use std::collections::BTreeMap;

use super::{
    TeletextEnhancementCommand, TeletextEnhancementProgram, TeletextMagazinePresentationDraft,
    TeletextObjectIdentity, TeletextResolvedServiceDraft, TeletextSemanticService,
    TeletextServiceBudget, TeletextServiceError,
};
use crate::teletext::{TeletextLocalObjectOperandFailure, TeletextLocalObjectOperandOverlay};

impl TeletextSemanticService {
    /// Replace G0/G1 operands in existing local object definitions only.
    ///
    /// Preserves object identities, types, command order, all other commands and
    /// every other semantic input. Raw work and existing service budgets are
    /// checked before allocation; reconstruction reruns the full collector.
    /// Hosts must authenticate original compiler provenance and check all shared
    /// consumer effects. This neither grants edit permission nor publishes.
    /// # Errors
    /// Rejects empty/repeated changes, missing objects, mismatched originals and
    /// any service or collector budget failure.
    pub fn with_local_object_operand_overlays(
        self,
        overlays: &[TeletextLocalObjectOperandOverlay],
        budget: TeletextServiceBudget,
    ) -> Result<Self, TeletextServiceError> {
        self.check_local_operand_overlay_budgets(overlays.len(), budget)?;
        if overlays.is_empty() {
            return Err(TeletextServiceError::EmptyLocalObjectOperandOverlays);
        }
        let indexed = self.validate_local_object_operand_overlays(overlays)?;
        let mut objects = self.objects.definitions().to_vec();
        for object in &mut objects {
            let mut changes = indexed
                .range((object.identity, 0)..=(object.identity, usize::MAX))
                .peekable();
            if changes.peek().is_none() {
                continue;
            }
            let mut commands = object.program.commands().to_vec();
            for (_, overlay) in changes {
                commands[overlay.command()] = TeletextEnhancementCommand::AtColumn {
                    column: overlay.column(),
                    operation: overlay.replacement(),
                };
            }
            object.program = TeletextEnhancementProgram::new(commands)?;
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
            objects,
            drcs: self.drcs.subtables().to_vec(),
        };
        Self::new_with_resolved_mot_fallbacks(
            draft,
            self.object_companions,
            self.mot_fallbacks,
            budget,
        )?
        .with_physical_base_overlays(self.physical_base_overlays.into_values().collect(), budget)
    }

    fn validate_local_object_operand_overlays<'a>(
        &self,
        overlays: &'a [TeletextLocalObjectOperandOverlay],
    ) -> Result<
        BTreeMap<(TeletextObjectIdentity, usize), &'a TeletextLocalObjectOperandOverlay>,
        TeletextServiceError,
    > {
        let mut indexed = BTreeMap::new();
        for overlay in overlays {
            let failure = |reason| TeletextServiceError::LocalObjectOperandOverlay {
                identity: overlay.identity(),
                command: overlay.command(),
                reason,
            };
            if indexed
                .insert((overlay.identity(), overlay.command()), overlay)
                .is_some()
            {
                return Err(failure(TeletextLocalObjectOperandFailure::Duplicate));
            }
            let object = self
                .objects
                .definition(overlay.identity())
                .ok_or_else(|| failure(TeletextLocalObjectOperandFailure::MissingObject))?;
            let expected = TeletextEnhancementCommand::AtColumn {
                column: overlay.column(),
                operation: overlay.expected(),
            };
            if object.program.commands().get(overlay.command()) != Some(&expected) {
                return Err(failure(TeletextLocalObjectOperandFailure::CommandMismatch));
            }
        }
        Ok(indexed)
    }
}
