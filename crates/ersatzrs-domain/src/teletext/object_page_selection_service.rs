//! One complete semantic rebuild for role changes and checked resource programs.
use super::{
    BTreeMap, TeletextMagazinePresentationDraft, TeletextObjectIdentity, TeletextPageAddress,
    TeletextResolvedObjectPageLinks, TeletextResolvedServiceDraft, TeletextSemanticService,
    TeletextServiceBudget, TeletextServiceError, TeletextX27Level25Links,
};
use crate::teletext::TeletextResourceObjectProgramOverlay;

impl TeletextSemanticService {
    pub(in crate::teletext) fn with_object_page_selection_draft(
        self,
        updates: BTreeMap<
            TeletextPageAddress,
            (TeletextX27Level25Links, TeletextResolvedObjectPageLinks),
        >,
        overlays: &BTreeMap<TeletextObjectIdentity, &TeletextResourceObjectProgramOverlay>,
        budget: TeletextServiceBudget,
    ) -> Result<Self, TeletextServiceError> {
        // The declared entry point preflights old/final budgets and exact bodies
        // before reaching this sole clone/reconstruction boundary.
        let mut pages = self.pages;
        for (display, (links, objects)) in updates {
            let page = pages
                .get_mut(&display)
                .ok_or(TeletextServiceError::MissingPage { address: display })?;
            page.compositional_links = Some(links);
            page.resources.objects = objects;
        }
        let mut objects = self.objects.definitions().to_vec();
        for object in &mut objects {
            if let Some(overlay) = overlays.get(&object.identity) {
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
}
