//! Lossless command-aligned editor members; native full-service preview owns context.
use super::{
    body_range, check_deadline, document, native_word, project_encoded_teletext_object, wrapper,
    EncodedTeletextAuthoredPage, EncodedTeletextObjectPage, Error, Instant,
    ObjectEditorCommandPosition, ObjectEditorFrame, Range, Result, TeletextEnhancementCommand,
    TeletextEnhancementObjectDefinition, TeletextObjectIdentity, TeletextObjectType,
    TeletextTtiBudget, TeletextTtiDocument, TeletextTtiRecord,
};

/// Regenerated from the native page and its original catalogue definition.
/// Word coordinates are absolute FIRST-X3 coordinates, never editor aliases.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::host) struct ObjectEditorFragmentPlan {
    pub identity: TeletextObjectIdentity,
    pub object_type: TeletextObjectType,
    pub fragment_index: u16,
    pub fragment_count: u16,
    pub command_start: u64,
    pub command_end: u64,
    pub native_word_start: u16,
    pub native_word_end: u16,
}

fn original<'a>(
    page: &'a EncodedTeletextAuthoredPage,
    definition: &TeletextEnhancementObjectDefinition,
    max_units: usize,
    deadline: Instant,
) -> Result<(&'a EncodedTeletextObjectPage, Range<usize>)> {
    check_deadline(deadline)?;
    if max_units == 0 || page.data_units().len() > max_units {
        return Err(Error::Budget);
    }
    let EncodedTeletextAuthoredPage::Object(resource) = page else {
        return Err(Error::Association);
    };
    let (kind, body) = body_range(resource, definition.identity, deadline)?;
    if kind != definition.object_type || body.len() > 507 {
        return Err(Error::Association);
    }
    let mut positions = resource
        .command_positions()
        .iter()
        .filter(|position| position.source().object == Some(definition.identity));
    let mut next = positions.next();
    let mut end = body.start;
    for (ordinal, command) in definition.program.commands().iter().enumerate() {
        check_deadline(deadline)?;
        if next.is_some_and(|position| position.source().command == ordinal) {
            let position = next.unwrap();
            if position.first_word() != end
                || position.semantic_word() < end
                || position.semantic_word() >= body.end
            {
                return Err(Error::Association);
            }
            end = position
                .semantic_word()
                .checked_add(1)
                .ok_or(Error::Budget)?;
            next = positions.next();
        } else if !matches!(command, TeletextEnhancementCommand::SetPosition { position }
            if position.row() == 0)
        {
            // The current native lowerer omits only checked forward header AP.
            return Err(Error::Association);
        }
    }
    if next.is_some() || end != body.end {
        return Err(Error::Association);
    }
    Ok((resource, body))
}

fn plan(
    definition: &TeletextEnhancementObjectDefinition,
    command: Range<usize>,
    body: Range<usize>,
) -> Result<ObjectEditorFragmentPlan> {
    Ok(ObjectEditorFragmentPlan {
        identity: definition.identity,
        object_type: definition.object_type,
        fragment_index: 0,
        fragment_count: 0,
        command_start: u64::try_from(command.start).map_err(|_| Error::Budget)?,
        command_end: u64::try_from(command.end).map_err(|_| Error::Budget)?,
        native_word_start: u16::try_from(body.start).map_err(|_| Error::Budget)?,
        native_word_end: u16::try_from(body.end).map_err(|_| Error::Budget)?,
    })
}

/// Partition without copying body words. The exact source definition is supplied
/// by the host's authenticated declared catalogue, not uploaded metadata.
pub(in crate::host) fn plan_fragments(
    page: &EncodedTeletextAuthoredPage,
    definition: &TeletextEnhancementObjectDefinition,
    max_units: usize,
    deadline: Instant,
) -> Result<Vec<ObjectEditorFragmentPlan>> {
    let (resource, body) = original(page, definition, max_units, deadline)?;
    let (marker, _) = wrapper(definition.object_type)?;
    let capacity = 208_usize.checked_sub(marker + 2).ok_or(Error::Budget)?;
    let mut plans = Vec::new();
    // A nonempty whole-command span consumes at least one of <=507 native words.
    plans
        .try_reserve_exact(body.len().max(1))
        .map_err(|_| Error::Budget)?;
    let mut start = body.start;
    let mut command_start = 0;
    for position in resource
        .command_positions()
        .iter()
        .filter(|position| position.source().object == Some(definition.identity))
    {
        check_deadline(deadline)?;
        let end = position.semantic_word() + 1;
        if end - position.first_word() > capacity {
            return Err(Error::Budget);
        }
        if end - start > capacity {
            // Invisible ordinals before this emitting command remain in the
            // previous member. Every semantic ordinal belongs to exactly one.
            plans.push(plan(
                definition,
                command_start..position.source().command,
                start..position.first_word(),
            )?);
            command_start = position.source().command;
            start = position.first_word();
        }
    }
    plans.push(plan(
        definition,
        command_start..definition.program.commands().len(),
        start..body.end,
    )?);
    let count = u16::try_from(plans.len()).map_err(|_| Error::Budget)?;
    for (index, plan) in plans.iter_mut().enumerate() {
        check_deadline(deadline)?;
        plan.fragment_index = u16::try_from(index).map_err(|_| Error::Budget)?;
        plan.fragment_count = count;
    }
    Ok(plans)
}

/// A fragment is a protected command-edit view, not an independently executing
/// resource. Earlier AP/charset/colour/DRCS and nested context remain native.
pub(in crate::host) fn build_fragment(
    page: &EncodedTeletextAuthoredPage,
    definition: &TeletextEnhancementObjectDefinition,
    selected: &ObjectEditorFragmentPlan,
    max_units: usize,
    tti: TeletextTtiBudget,
    deadline: Instant,
) -> Result<ObjectEditorFrame> {
    let plans = plan_fragments(page, definition, max_units, deadline)?;
    if plans.get(usize::from(selected.fragment_index)) != Some(selected) {
        return Err(Error::Association);
    }
    let body = usize::from(selected.native_word_start)..usize::from(selected.native_word_end);
    let (marker, skeleton) = wrapper(definition.object_type)?;
    let padded = (marker + 2 + body.len()).div_ceil(13) * 13;
    if padded > 208 || padded / 13 + 7 > tti.max_records {
        return Err(Error::Budget);
    }
    let projection = project_encoded_teletext_object(page, max_units)?;
    let mut words = Vec::new();
    words.try_reserve_exact(padded).map_err(|_| Error::Budget)?;
    words.extend_from_slice(&skeleton[..=marker]);
    for index in body.clone() {
        check_deadline(deadline)?;
        words.push(native_word(&projection, index)?);
    }
    words.resize(padded, skeleton[marker + 1]);
    let EncodedTeletextAuthoredPage::Object(resource) = page else {
        return Err(Error::Association);
    };
    let command_positions =
        fragment_positions(resource, definition, selected, marker + 1, deadline)?;
    let base = document(&words, tti)?;
    let mut records = Vec::new();
    records
        .try_reserve_exact(base.records().len() + 1)
        .map_err(|_| Error::Budget)?;
    records.push(TeletextTtiRecord::Description(format!(
        "Object command fragment {}/{}; use native preview for full context.",
        selected.fragment_index + 1,
        selected.fragment_count,
    )));
    records.extend_from_slice(base.records());
    let document = TeletextTtiDocument::checked_from_records(records, tti)?;
    check_deadline(deadline)?;
    Ok(ObjectEditorFrame {
        identity: definition.identity,
        object_type: definition.object_type,
        command_positions,
        document,
    })
}

fn fragment_positions(
    resource: &EncodedTeletextObjectPage,
    definition: &TeletextEnhancementObjectDefinition,
    selected: &ObjectEditorFragmentPlan,
    editor_start: usize,
    deadline: Instant,
) -> Result<Vec<ObjectEditorCommandPosition>> {
    let mut positions = Vec::new();
    positions
        .try_reserve_exact(usize::from(
            selected.native_word_end - selected.native_word_start,
        ))
        .map_err(|_| Error::Budget)?;
    for position in resource
        .command_positions()
        .iter()
        .filter(|position| position.source().object == Some(definition.identity))
    {
        check_deadline(deadline)?;
        let ordinal = u64::try_from(position.source().command).map_err(|_| Error::Budget)?;
        if (selected.command_start..selected.command_end).contains(&ordinal) {
            if position.first_word() < usize::from(selected.native_word_start)
                || position.semantic_word() >= usize::from(selected.native_word_end)
            {
                return Err(Error::Association);
            }
            positions.push(ObjectEditorCommandPosition {
                command: position.source().command,
                editor_word: editor_start + position.semantic_word()
                    - usize::from(selected.native_word_start),
            });
        }
    }
    Ok(positions)
}
