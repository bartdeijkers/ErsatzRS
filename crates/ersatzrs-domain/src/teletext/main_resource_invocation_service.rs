//! Original Main command authentication before the single final reconstruction.
use super::{BTreeMap, TeletextPageAddress, TeletextSemanticService, TeletextServiceError};
use crate::teletext::{
    TeletextMainResourceInvocationFailure as Failure, TeletextMainResourceInvocationOverlay,
};

impl TeletextSemanticService {
    pub(in crate::teletext) fn index_main_resource_invocations<'a>(
        &self,
        overlays: &'a [TeletextMainResourceInvocationOverlay],
    ) -> Result<
        BTreeMap<(TeletextPageAddress, usize), &'a TeletextMainResourceInvocationOverlay>,
        TeletextServiceError,
    > {
        let mut indexed = BTreeMap::new();
        for overlay in overlays {
            let key = (overlay.owner(), overlay.command());
            let failure = |reason| TeletextServiceError::MainResourceInvocationOverlay {
                owner: key.0,
                command: key.1,
                reason,
            };
            if indexed.insert(key, overlay).is_some() {
                return Err(failure(Failure::Duplicate));
            }
            let original = self
                .pages
                .get(&overlay.owner())
                .and_then(|page| page.local_program.as_ref())
                .ok_or_else(|| failure(Failure::MissingProgram))?;
            let command = original
                .commands()
                .get(overlay.command())
                .ok_or_else(|| failure(Failure::MissingCommand))?;
            if *command != overlay.expected() {
                return Err(failure(Failure::OriginalMismatch));
            }
        }
        Ok(indexed)
    }
}
