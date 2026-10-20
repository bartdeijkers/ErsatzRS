//! Receipt-authenticated original-position history; upload bytes grant no authority.
use super::project::{native_body, Body, Limits};
use super::{invalid, EncodedTeletextAuthoredService, Instant, Mapping, Result};
use ersatzrs_domain::{
    TeletextEnhancementProgram, TeletextResourceObjectDefinitionAddition,
    TeletextResourceObjectProgramOverlay,
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
        if let Some(cleared) = self.clear_added_body(mapped, body, limits, deadline)? {
            return Ok(Some(cleared));
        }
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
        if definition.object_type != mapped.object_type {
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
        TeletextResourceObjectProgramOverlay::restore_original_positions_to(
            previous,
            &definition.program,
            limits.2,
        )
        .map(Some)
        .map_err(|_| invalid())
    }

    fn clear_added_body(
        &self,
        mapped: &Mapping<'_>,
        body: &Body,
        limits: Limits,
        deadline: Instant,
    ) -> Result<Option<TeletextResourceObjectProgramOverlay>> {
        mapped.location.check_deadline(deadline)?;
        if !body.after.is_empty() {
            return Ok(None);
        }
        let mut additions = self
            .additions
            .iter()
            .filter(|addition| addition.identity() == mapped.identity);
        let Some(addition) = additions.next() else {
            return Ok(None);
        };
        let root = self.root.ok_or_else(invalid)?;
        if additions.next().is_some()
            || addition.definition() != mapped.definition
            || root
                .declared()
                .semantic()
                .objects()
                .definition(mapped.identity)
                .is_some()
            || body.before != native_body(mapped, limits, deadline)?
        {
            return Err(invalid());
        }
        let empty = TeletextEnhancementProgram::new(Vec::new()).map_err(|_| invalid())?;
        TeletextResourceObjectProgramOverlay::clear_added_body(addition, empty, limits.2)
            .map(Some)
            .map_err(|_| invalid())
    }
}
