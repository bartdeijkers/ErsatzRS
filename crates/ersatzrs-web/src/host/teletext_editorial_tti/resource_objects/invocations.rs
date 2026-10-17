//! Resolve changed call words only through the authenticated original catalogue.
use super::{invalid, Failure, Location, Result};
use ersatzrs_domain::{
    TeletextEnhancementCommand as Command, TeletextEnhancementProgram, TeletextObjectOffset,
    TeletextObjectScope, TeletextObjectType, TeletextResourceObjectProgramOverlay,
};
use ersatzrs_ffmpeg::teletext::{
    allocate_teletext_resource_objects, EncodedTeletextAuthoredService,
    TeletextResourceObjectAllocation, TeletextResourceObjectAllocations,
};
use std::{collections::BTreeMap, ops::Range, time::Instant};

fn calls(words: &[u32], location: Location, deadline: Instant) -> Result<Vec<Range<usize>>> {
    let mut result = Vec::new();
    for (index, word) in words.iter().enumerate() {
        location.check_deadline(deadline)?;
        if *word > 0x3ffff {
            return Err(location.error(Failure::Structure));
        }
        if word & 63 >= 40 && (17..=19).contains(&((word >> 6) & 31)) {
            let first =
                if index > 0 && words[index - 1] & 63 >= 40 && (words[index - 1] >> 6) & 31 == 16 {
                    index - 1
                } else {
                    index
                };
            result.push(first..index + 1);
        }
    }
    Ok(result)
}

type Targets = BTreeMap<u32, Option<(ersatzrs_domain::TeletextObjectIdentity, TeletextObjectType)>>;

fn allocations(
    original: &EncodedTeletextAuthoredService,
    maximum: usize,
    location: Location,
) -> Result<TeletextResourceObjectAllocations> {
    let fail = || location.error(Failure::Structure);
    let declared = original.declared();
    let defaults = declared
        .defaults()
        .iter()
        .map(ersatzrs_domain::TeletextDeclaredDefaultReservation::selection)
        .collect::<Vec<_>>();
    allocate_teletext_resource_objects(declared.semantic().objects(), &defaults, maximum)
        .map_err(|_| fail())
}

fn target_word(entry: &TeletextResourceObjectAllocation, location: Location) -> Result<u32> {
    let scope = match entry.target().scope() {
        TeletextObjectScope::Public => 0x30,
        TeletextObjectScope::Global => 0x38,
        TeletextObjectScope::Local => return Err(location.error(Failure::Structure)),
    };
    let mode = match entry.object_type() {
        TeletextObjectType::Active => 17,
        TeletextObjectType::Adaptive => 18,
        TeletextObjectType::Passive => 19,
    };
    let number = u32::from(entry.object_number());
    Ok((scope | (number >> 7)) | (mode << 6) | ((number & 127) << 11))
}

fn allocated_targets(
    allocations: &TeletextResourceObjectAllocations,
    location: Location,
    deadline: Instant,
) -> Result<Targets> {
    let mut targets = BTreeMap::new();
    for entry in allocations.entries() {
        location.check_deadline(deadline)?;
        let word = target_word(entry, location)?;
        // Different complete resources can have the same physical selector.
        // Uploaded words cannot choose between them or invent a resource page.
        targets
            .entry(word)
            .and_modify(|v| *v = None)
            .or_insert(Some((entry.target(), entry.object_type())));
    }
    Ok(targets)
}

fn targets(
    original: &EncodedTeletextAuthoredService,
    maximum: usize,
    location: Location,
    deadline: Instant,
) -> Result<Targets> {
    allocated_targets(
        &allocations(original, maximum, location)?,
        location,
        deadline,
    )
}

fn offset(words: &[u32], location: Location) -> Result<Option<TeletextObjectOffset>> {
    let fail = || location.error(Failure::Structure);
    match words {
        [_, _] if words[0] & 63 >= 40 && (words[0] >> 6) & 31 == 16 => {
            let row = u8::try_from((words[0] & 63) - 40).map_err(|_| fail())?;
            let column = u8::try_from(words[0] >> 11).map_err(|_| fail())?;
            Ok(Some(
                TeletextObjectOffset::new(row, column).map_err(|_| fail())?,
            ))
        }
        [_] => Ok(None),
        _ => Err(fail()),
    }
}

fn known_targets(
    mapped: &super::Mapping<'_>,
    body: &super::project::Body,
    before: &[Range<usize>],
    allocations: &TeletextResourceObjectAllocations,
    deadline: Instant,
) -> Result<Targets> {
    let fail = || mapped.location.error(Failure::Structure);
    let mut known = Targets::new();
    let mut physical = before.iter();
    for (ordinal, command) in mapped.definition.program.commands().iter().enumerate() {
        mapped.location.check_deadline(deadline)?;
        let Command::InvokeObject {
            target,
            object_type,
            offset: expected_offset,
        } = command
        else {
            continue;
        };
        let range = physical.next().ok_or_else(fail)?;
        let mut matching = body.spans.iter().filter(|span| span.ordinal == ordinal);
        let (Some(span), None) = (matching.next(), matching.next()) else {
            return Err(fail());
        };
        if span.first != range.start || span.word.checked_add(1) != Some(range.end) {
            return Err(fail());
        }
        let entry = allocations.lookup(*target, *object_type).ok_or_else(fail)?;
        let selector = target_word(entry, mapped.location)?;
        if body.before.get(span.word) != Some(&selector)
            || offset(&body.before[range.clone()], mapped.location)? != *expected_offset
        {
            return Err(fail());
        }
        let identity = (*target, *object_type);
        known
            .entry(selector)
            .and_modify(|value| {
                if *value != Some(identity) {
                    *value = None;
                }
            })
            .or_insert(Some(identity));
    }
    if physical.next().is_some() {
        return Err(fail());
    }
    Ok(known)
}

/// V7 is a native command-edit view; it never supplies disposable Local aliases.
pub(super) fn topology_calls(
    mapped: &super::Mapping<'_>,
    body: &super::project::Body,
    limits: super::project::Limits,
    deadline: Instant,
) -> Result<Vec<super::topology::Call>> {
    mapped.location.check_deadline(deadline)?;
    let fail = || mapped.location.error(Failure::Structure);
    if mapped.project.editor_profile != "qteletextmaker-1.0-carrier-v7"
        || !matches!(
            mapped.asset.scope,
            super::TeletextExportAssetScope::ObjectTtiFragmentV1 { .. }
        )
        || body.before.len() > 507
        || body.after.len() > 507
        || mapped.definition.program.commands().len() > limits.2.max_commands
        || body.spans.len() > limits.2.max_commands
    {
        return Err(fail());
    }
    let before = calls(&body.before, mapped.location, deadline)?;
    let after = calls(&body.after, mapped.location, deadline)?;
    // Existing equal-count V16 imports retain their original exact-target rule.
    if before.len() == after.len() || after.len() > limits.2.max_commands {
        return Err(fail());
    }
    let allocations = allocations(mapped.service, limits.2.max_objects, mapped.location)?;
    let known = known_targets(mapped, body, &before, &allocations, deadline)?;
    let targets = allocated_targets(&allocations, mapped.location, deadline)?;
    let mut result = Vec::new();
    for range in after {
        mapped.location.check_deadline(deadline)?;
        let words = &body.after[range.clone()];
        let selector = words.last().ok_or_else(fail)?;
        // A known selector carries sealed full identity; an unused collision
        // cannot relabel it. Conflicting known identities remain unresolvable.
        let (target, object_type) = known
            .get(selector)
            .or_else(|| targets.get(selector))
            .copied()
            .flatten()
            .ok_or_else(fail)?;
        result.push(super::topology::Call {
            first: range.start,
            word: range.end - 1,
            command: Command::InvokeObject {
                target,
                object_type,
                offset: offset(words, mapped.location)?,
            },
        });
    }
    Ok(result)
}

/// Normalize only call spans for the unchanged glyph/style parser, then restore
/// exact typed calls. Complete native equivalence still checks the uploaded body.
pub(super) fn program(
    mapped: &super::Mapping<'_>,
    body: &super::project::Body,
    original: &EncodedTeletextAuthoredService,
    limits: super::project::Limits,
    deadline: Instant,
) -> Result<TeletextResourceObjectProgramOverlay> {
    let location = mapped.location;
    let fail = || location.error(Failure::Structure);
    let before = calls(&body.before, location, deadline)?;
    let after = calls(&body.after, location, deadline)?;
    let originals = mapped
        .definition
        .program
        .commands()
        .iter()
        .enumerate()
        .filter(|(_, command)| matches!(command, Command::InvokeObject { .. }))
        .collect::<Vec<_>>();
    if before.is_empty() || before.len() != after.len() || before.len() != originals.len() {
        return Err(fail());
    }
    let targets = targets(original, limits.2.max_objects, location, deadline)?;
    let mut normalized = Vec::new();
    let mut replacements = BTreeMap::new();
    let mut cursor = 0;
    for ((old, new), (ordinal, original_command)) in before.iter().zip(&after).zip(originals) {
        location.check_deadline(deadline)?;
        let span = body
            .spans
            .iter()
            .find(|s| s.ordinal == ordinal)
            .ok_or_else(fail)?;
        if span.first != old.start || span.word + 1 != old.end {
            return Err(fail());
        }
        let words = &body.after[new.clone()];
        let (target, object_type) = if words.last() == body.before[old.clone()].last() {
            let Command::InvokeObject {
                target,
                object_type,
                ..
            } = original_command
            else {
                return Err(fail());
            };
            (*target, *object_type)
        } else {
            let target = targets
                .get(words.last().ok_or_else(fail)?)
                .copied()
                .flatten()
                .ok_or_else(fail)?;
            target
        };
        let offset = offset(words, location)?;
        replacements.insert(
            ordinal,
            Command::InvokeObject {
                target,
                object_type,
                offset,
            },
        );
        normalized.extend_from_slice(body.after.get(cursor..new.start).ok_or_else(fail)?);
        normalized.extend_from_slice(&body.before[old.clone()]);
        cursor = new.end;
        if normalized.len() > 507 {
            return Err(fail());
        }
    }
    normalized.extend_from_slice(body.after.get(cursor..).ok_or_else(fail)?);
    if normalized.len() > 507 {
        return Err(fail());
    }
    let rebuilt = super::super::structural::resource::parse(
        mapped.definition,
        &body.spans,
        (&body.before, &normalized),
        limits.2.max_commands,
        location,
        deadline,
    )?;
    let mut commands = rebuilt.replacement.commands().to_vec();
    for (command, retained) in commands.iter_mut().zip(&rebuilt.retained) {
        if let Some(replacement) = retained.and_then(|ordinal| replacements.get(&ordinal)) {
            *command = *replacement;
        }
    }
    TeletextResourceObjectProgramOverlay::new_with_invocation_changes(
        mapped.identity,
        mapped.definition.program.clone(),
        TeletextEnhancementProgram::new(commands).map_err(|_| invalid())?,
        rebuilt.retained,
        limits.2,
    )
    .map_err(|_| fail())
}
