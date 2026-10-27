//! Read-only removal choices from authenticated additions and the full graph.
use super::{check_deadline, TeletextAuthoringError};
use ersatzrs_domain::{TeletextEnhancementCommand, TeletextObjectIdentity};
use ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService;
use std::{collections::BTreeSet, time::Instant};

pub(super) fn observe(
    service: &EncodedTeletextAuthoredService,
    added: &[TeletextObjectIdentity],
    deadline: Instant,
) -> Result<Vec<TeletextObjectIdentity>, TeletextAuthoringError> {
    check_deadline(deadline)?;
    if added.is_empty() {
        return Ok(Vec::new());
    }
    let declared = service.declared();
    let semantic = declared.semantic();
    let mut used = BTreeSet::new();
    for default in declared.defaults() {
        check_deadline(deadline)?;
        used.insert(default.selection().target);
    }
    for page in semantic.page_enhancements().values() {
        check_deadline(deadline)?;
        for default in page.resources.defaults.iter().flatten() {
            used.insert(default.target);
        }
    }
    let programs = semantic
        .page_enhancements()
        .values()
        .filter_map(|page| page.local_program.as_ref())
        .chain(
            semantic
                .objects()
                .definitions()
                .iter()
                .map(|definition| &definition.program),
        );
    for program in programs {
        for command in program.commands() {
            check_deadline(deadline)?;
            if let TeletextEnhancementCommand::InvokeObject { target, .. } = command {
                used.insert(*target);
            }
        }
    }
    let mut result = Vec::new();
    for &identity in added {
        check_deadline(deadline)?;
        if identity.resource().is_some()
            && !used.contains(&identity)
            && semantic.objects().definition(identity).is_some()
        {
            result.push(identity);
        }
    }
    Ok(result)
}
