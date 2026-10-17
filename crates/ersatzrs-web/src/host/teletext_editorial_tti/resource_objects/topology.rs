//! Resource call topology and structure retain sealed identities and anchors.
use super::{project, Failure, Mapping, Result};
use ersatzrs_domain::{
    TeletextColumnOperation, TeletextEnhancementCommand as Command, TeletextEnhancementProgram,
    TeletextResourceObjectProgramOverlay,
};
use std::{collections::BTreeMap, time::Instant};

pub(super) use super::super::structural::resource::CallAtom as Call;
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
    let mut seen = 0;
    for (ordinal, command) in mapped.definition.program.commands().iter().enumerate() {
        mapped.location.check_deadline(deadline)?;
        let mut spans = body.spans.iter().filter(|span| span.ordinal == ordinal);
        let span = match (spans.next(), spans.next()) {
            (None, None) if matches!(command, Command::SetPosition { position } if position.row() == 0) => {
                continue
            }
            (Some(span), None) => span,
            _ => return Err(fail()),
        };
        seen += 1;
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
    if next != body.before.len() || seen != body.spans.len() {
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

fn noncall_commands(
    mapped: &Mapping<'_>,
    body: &project::Body,
    old: &Layout,
    new: &Layout,
    limits: project::Limits,
    deadline: Instant,
) -> Result<Vec<Command>> {
    let fail = || mapped.location.error(Failure::Structure);
    let original = mapped.definition.program.commands();
    if old.words == new.words {
        return Ok(original.to_vec());
    }
    if old.words.len() != new.words.len() {
        return Err(fail());
    }
    // Only existing G0/G1 semantic words may differ. Compound prefixes, styles,
    // selectors and positions stay byte-exact, including their call boundaries.
    let mut normalized = body.before.clone();
    for (&word, &offset) in &old.offsets {
        mapped.location.check_deadline(deadline)?;
        let Some(after) = new.words.get(offset) else {
            continue; // The authenticated end boundary carries no word.
        };
        if old.words[offset] != *after {
            let span = body
                .spans
                .iter()
                .find(|span| span.word == word)
                .ok_or_else(fail)?;
            if !matches!(
                original[span.ordinal],
                Command::AtColumn {
                    operation: TeletextColumnOperation::G0 { .. }
                        | TeletextColumnOperation::G1 { .. },
                    ..
                }
            ) {
                return Err(fail());
            }
        }
        normalized[word] = *after;
    }
    // Restore original call atoms only for validation by the existing parser.
    // Actual calls still come from the authenticated uploaded alias catalogue.
    let rebuilt = super::super::structural::resource::parse(
        mapped.definition,
        &body.spans,
        (&body.before, &normalized),
        limits.2.max_commands,
        mapped.location,
        deadline,
    )?;
    if rebuilt.replacement.commands().len() != original.len()
        || rebuilt.retained.len() != original.len()
        || rebuilt
            .retained
            .iter()
            .enumerate()
            .any(|(ordinal, retained)| *retained != Some(ordinal))
        || original
            .iter()
            .zip(rebuilt.replacement.commands())
            .any(|(before, after)| match (before, after) {
                (Command::AtColumn { column: a, .. }, Command::AtColumn { column: b, .. }) => {
                    a != b
                }
                _ => before != after,
            })
    {
        return Err(fail());
    }
    Ok(rebuilt.replacement.commands().to_vec())
}

fn fixed_program(
    mapped: &Mapping<'_>,
    body: &project::Body,
    after: &[Call],
    limits: project::Limits,
    deadline: Instant,
) -> Result<TeletextResourceObjectProgramOverlay> {
    let fail = || mapped.location.error(Failure::Structure);
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
    let noncalls = noncall_commands(mapped, body, &old, &new, limits, deadline)?;
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
        commands.push(noncalls[ordinal]);
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

pub(super) fn program(
    mapped: &Mapping<'_>,
    body: &project::Body,
    limits: project::Limits,
    deadline: Instant,
) -> Result<TeletextResourceObjectProgramOverlay> {
    let native_calls;
    let calls = if let Some(calls) = body.context_calls.as_deref() {
        calls
    } else {
        native_calls = super::invocations::topology_calls(mapped, body, limits, deadline)?;
        &native_calls
    };
    match fixed_program(mapped, body, calls, limits, deadline) {
        Ok(program) => return Ok(program),
        Err(error) if error.failure == Failure::TimedOut => return Err(error),
        Err(_) => {}
    }
    mapped.location.check_deadline(deadline)?;
    let fail = || mapped.location.error(Failure::Structure);
    // Equal-count retained-call edits retain their earlier policy in each view.
    if original_calls(mapped, body, deadline)?.len() == calls.len() {
        return Err(fail());
    }
    let rebuilt = super::super::structural::resource::parse_with_calls(
        mapped.definition,
        &body.spans,
        (&body.before, &body.after),
        calls,
        limits.2.max_commands,
        mapped.location,
        deadline,
    )?;
    TeletextResourceObjectProgramOverlay::new_with_invocation_structure(
        mapped.identity,
        mapped.definition.program.clone(),
        rebuilt.replacement,
        rebuilt.retained,
        limits.2,
    )
    .map_err(|_| fail())
}
