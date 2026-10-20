//! Private semantic rebuild after declared selected-page announcement checks.
use super::{
    BTreeMap, TeletextMagazinePresentationDraft, TeletextPageAddress, TeletextResolvedServiceDraft,
    TeletextSemanticService, TeletextServiceBudget, TeletextServiceError, TeletextX27Level25Links,
};

impl TeletextSemanticService {
    pub(in crate::teletext) fn with_announced_object_links(
        self,
        updates: BTreeMap<TeletextPageAddress, TeletextX27Level25Links>,
        budget: TeletextServiceBudget,
    ) -> Result<Self, TeletextServiceError> {
        self.check_existing_service_budgets(budget)?;
        // Only existing metadata slots are replaced. The shared preflight above
        // includes physical overlays and spellings; no new structure is added.
        let mut pages = self.pages;
        for (display, links) in updates {
            let page = pages
                .get_mut(&display)
                .ok_or(TeletextServiceError::MissingPage { address: display })?;
            page.compositional_links = Some(links);
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
            objects: self.objects.definitions().to_vec(),
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
