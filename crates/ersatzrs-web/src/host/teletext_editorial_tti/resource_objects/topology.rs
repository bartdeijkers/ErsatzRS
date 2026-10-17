//! V8 call topology from sealed aliases, with the non-call native skeleton fixed.
use super::{project, Failure, Mapping, Result};
use ersatzrs_domain::{
    TeletextEnhancementCommand as Command, TeletextEnhancementProgram,
    TeletextResourceObjectProgramOverlay,
};
use std::{collections::BTreeMap, time::Instant};

pub(super) struct Call {
    pub first: usize,
    pub word: usize,
    pub command: Command,
}
struct Layout {
    words: Vec<u32>,
    calls: BTreeMap<usize, Vec<(usize, Command)>>,
    offsets: BTreeMap<usize, usize>,
}
fn layout(
    words: &[u32],
    calls: &[(usize, &Call)],
    location: super::Location,
    deadline: Instant,
) -> Result<Layout> {
    let fail = || location.error(Failure::Structure);
    if words.len() > 507 || calls.len() > 507 {
        return Err(fail());
    }
    let mut result = Layout {
        words: vec![],
        calls: BTreeMap::new(),
        offsets: BTreeMap::new(),
    };
    let (mut index, mut next_call) = (0, 0);
    while index < words.len() {
        location.check_deadline(deadline)?;
        if let Some((ordinal, call)) = calls.get(next_call) {
            if call.first < index || call.first > call.word || call.word >= words.len() {
                return Err(fail());
            }
            if call.first == index {
                if !matches!(call.command, Command::InvokeObject { .. }) {
                    return Err(fail());
                }
                result
                    .calls
                    .entry(result.words.len())
                    .or_default()
                    .push((*ordinal, call.command));
                index = call.word + 1;
                next_call += 1;
                continue;
            }
        }
        result.offsets.insert(index, result.words.len());
        result.words.push(words[index]);
        index += 1;
    }
    if next_call != calls.len() {
        return Err(fail());
    }
    result.offsets.insert(index, result.words.len());
    Ok(result)
}
fn original_calls(
    mapped: &Mapping<'_>,
    body: &project::Body,
    deadline: Instant,
) -> Result<Vec<(usize, Call)>> {
    let fail = || mapped.location.error(Failure::Structure);
    let mut result = Vec::new();
    let mut next = 0;
    for (ordinal, command) in mapped.definition.program.commands().iter().enumerate() {
        mapped.location.check_deadline(deadline)?;
        // Zero-word semantic positions have no editor-owned insertion boundary.
        // Keep that ambiguity closed instead of guessing which side owns a call.
        let span = body
            .spans
            .iter()
            .find(|span| span.ordinal == ordinal)
            .ok_or_else(fail)?;
        if span.first != next || span.word < span.first || span.word >= body.before.len() {
            return Err(fail());
        }
        if matches!(command, Command::InvokeObject { .. }) {
            result.push((
                ordinal,
                Call {
                    first: span.first,
                    word: span.word,
                    command: *command,
                },
            ));
        }
        next = span.word + 1;
    }
    if next != body.before.len() {
        return Err(fail());
    }
    Ok(result)
}
fn append_calls(
    boundary: usize,
    old: &mut Layout,
    new: &mut Layout,
    commands: &mut Vec<Command>,
    retained: &mut Vec<Option<usize>>,
    limits: (usize, super::Location, Instant),
) -> Result<()> {
    let (maximum, location, deadline) = limits;
    let before = old.calls.remove(&boundary).unwrap_or_default();
    let after = new.calls.remove(&boundary).unwrap_or_default();
    let after = after
        .iter()
        .map(|(_, command)| *command)
        .collect::<Vec<_>>();
    let aligned = super::super::structural::resource::align_calls(
        &before, &after, maximum, location, deadline,
    )?;
    commands.extend_from_slice(aligned.replacement.commands());
    retained.extend(aligned.retained);
    if commands.len() > maximum {
        return Err(location.error(Failure::Structure));
    }
    Ok(())
}

pub(super) fn program(
    mapped: &Mapping<'_>,
    body: &project::Body,
    limits: project::Limits,
    deadline: Instant,
) -> Result<TeletextResourceObjectProgramOverlay> {
    let fail = || mapped.location.error(Failure::Structure);
    let after = body.context_calls.as_ref().ok_or_else(fail)?;
    let before = original_calls(mapped, body, deadline)?;
    if before.len() == after.len() {
        return Err(fail());
    }
    let old = before
        .iter()
        .map(|(ordinal, call)| (*ordinal, call))
        .collect::<Vec<_>>();
    let new = after.iter().enumerate().collect::<Vec<_>>();
    let mut old = layout(&body.before, &old, mapped.location, deadline)?;
    let mut new = layout(&body.after, &new, mapped.location, deadline)?;
    if old.words != new.words {
        return Err(fail());
    }
    let mut commands = Vec::new();
    let mut retained = Vec::new();
    for (ordinal, command) in mapped.definition.program.commands().iter().enumerate() {
        mapped.location.check_deadline(deadline)?;
        if matches!(command, Command::InvokeObject { .. }) {
            continue;
        }
        let span = body
            .spans
            .iter()
            .find(|span| span.ordinal == ordinal)
            .ok_or_else(fail)?;
        let boundary = *old.offsets.get(&span.first).ok_or_else(fail)?;
        append_calls(
            boundary,
            &mut old,
            &mut new,
            &mut commands,
            &mut retained,
            (limits.2.max_commands, mapped.location, deadline),
        )?;
        commands.push(*command);
        retained.push(Some(ordinal));
    }
    append_calls(
        old.words.len(),
        &mut old,
        &mut new,
        &mut commands,
        &mut retained,
        (limits.2.max_commands, mapped.location, deadline),
    )?;
    if !old.calls.is_empty() || !new.calls.is_empty() {
        return Err(fail());
    }
    TeletextResourceObjectProgramOverlay::new_with_invocation_structure(
        mapped.identity,
        mapped.definition.program.clone(),
        TeletextEnhancementProgram::new(commands).map_err(|_| fail())?,
        retained,
        limits.2,
    )
    .map_err(|_| fail())
}
