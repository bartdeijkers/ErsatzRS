//! Receipt-authenticated original-position history; upload bytes grant no authority.
use super::project::{native_body, Body, Limits};
use super::{invalid, EncodedTeletextAuthoredService, Instant, Mapping, Result};
use ersatzrs_domain::{
    TeletextResourceObjectDefinitionAddition, TeletextResourceObjectProgramOverlay,
};

#[derive(Clone, Copy, Default)]
pub(in crate::host) struct PositionScope<'a> {
    pub additions: &'a [TeletextResourceObjectDefinitionAddition],
    pub programs: &'a [TeletextResourceObjectProgramOverlay],
    pub root: Option<&'a EncodedTeletextAuthoredService>,
}
impl PositionScope<'_> {
    pub(super) fn check(&self, limits: Limits) -> Result<()> {
        if self.additions.len() > limits.2.max_objects || self.programs.len() > limits.2.max_objects
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub(super) fn restore(
        &self,
        mapped: &Mapping<'_>,
        body: &Body,
        limits: Limits,
        deadline: Instant,
    ) -> Result<Option<TeletextResourceObjectProgramOverlay>> {
        let Some(root) = self.root else {
            return Ok(None);
        };
        let mut programs = self
            .programs
            .iter()
            .filter(|program| program.identity() == mapped.identity);
        let Some(previous) = programs.next() else {
            return Ok(None);
        };
        if programs.next().is_some() {
            return Err(invalid());
        }
        if !previous.is_original_position_intent()
            || previous.replacement() != &mapped.definition.program
        {
            return Ok(None);
        }
        let definition = root
            .declared()
            .semantic()
            .objects()
            .definition(mapped.identity)
            .ok_or_else(invalid)?;
        if previous.expected() != &definition.program
            || definition.object_type != mapped.object_type
        {
            return Ok(None);
        }
        let header_identity = mapped
            .page
            .resource_header()
            .ok_or_else(invalid)?
            .identity();
        let mut pages = root.pages().iter().filter(|page| {
            page.resource_header()
                .is_some_and(|header| header.identity() == header_identity)
        });
        let page = pages.next().ok_or_else(invalid)?;
        if pages.next().is_some() {
            return Err(invalid());
        }
        let original = Mapping {
            identity: mapped.identity,
            object_type: mapped.object_type,
            page,
            location: mapped.location,
            asset: mapped.asset,
            project: mapped.project,
            definition,
            service: root,
        };
        if body.after != native_body(&original, limits, deadline)? {
            return Ok(None);
        }
        TeletextResourceObjectProgramOverlay::restore_original_positions(previous, limits.2)
            .map(Some)
            .map_err(|_| invalid())
    }
}
