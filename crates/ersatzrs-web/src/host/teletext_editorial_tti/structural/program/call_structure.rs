//! Local call atoms use only sealed definitions; generated writers stay derived.
use super::{
    calls, fail, header_ordinal, original_ordinal, raw, stream, Candidate, Location, Result,
};
use ersatzrs_domain::{
    TeletextColumnOperation, TeletextEnhancementCommand as Command,
    TeletextEnhancementObjectCatalog, TeletextEnhancementObjectDefinition,
    TeletextEnhancementPosition, TeletextObjectIdentity, TeletextObjectScope, TeletextObjectType,
};
use ersatzrs_ffmpeg::teletext::{
    EncodedTeletextAuthoredService, EncodedTeletextLocalObjectProgram,
    TeletextMainCompositionSource,
};
use std::time::Instant;

#[cfg(test)]
mod tests;

struct Atom {
    first: usize,
    last: usize,
    command: Command,
}

/// An untrusted carrier hypothesis, never an authored style or edit authority.
/// Only the completed native candidate can discharge this exact producer claim.
pub(in super::super) struct ProspectiveCarrier {
    identity: TeletextObjectIdentity,
    position: TeletextEnhancementPosition,
    uploaded_word: usize,
    word: u32,
}
impl ProspectiveCarrier {
    pub(super) fn check_candidate(&self, candidate: &Candidate, location: Location) -> Result<()> {
        let position = Command::SetPosition {
            position: self.position,
        };
        if candidate.identity != self.identity
            || candidate.expected.commands().first() != Some(&position)
            || candidate.replacement.first() != Some(&position)
            || candidate.retained.first() != Some(&Some(0))
            || !matches!(candidate.replacement.get(1), Some(Command::InvokeObject { target, .. })
                if target.scope() == TeletextObjectScope::Local && target.owner() == self.identity.owner())
        {
            return Err(fail(location));
        }
        Ok(())
    }

    pub(in super::super) fn qualify(
        &self,
        service: &EncodedTeletextAuthoredService,
        program: &EncodedTeletextLocalObjectProgram,
        images: (&stream::Image, &stream::Image),
        changes: &super::super::TeletextEditorialChanges,
        location: Location,
    ) -> Result<()> {
        let mut intents = changes
            .object_programs
            .iter()
            .filter(|intent| intent.identity() == self.identity);
        let intent = intents.next().ok_or_else(|| fail(location))?;
        if intents.next().is_some()
            || !intent.has_invocation_structure_changes()
            || intent.expected().commands().first()
                != Some(&Command::SetPosition {
                    position: self.position,
                })
            || intent.replacement().commands().first() != intent.expected().commands().first()
            || intent.original_command(0) != Some(0)
        {
            return Err(fail(location));
        }
        let definition = service
            .declared()
            .semantic()
            .objects()
            .definition(self.identity)
            .ok_or_else(|| fail(location))?;
        if definition.program != *intent.replacement()
            || self.identity.owner() != Some(program.owner())
        {
            return Err(fail(location));
        }
        let index = program
            .definition_positions()
            .iter()
            .position(|position| position.target() == self.identity)
            .ok_or_else(|| fail(location))?;
        let start = images.1.markers.get(index).ok_or_else(|| fail(location))? + 1;
        let mut producers = program.command_positions().iter().filter(|position| {
            position.source().object == Some(self.identity)
                && position.original_object_source()
                    == Some(TeletextMainCompositionSource::HeaderPosition { command: 0 })
        });
        let producer = producers.next().ok_or_else(|| fail(location))?;
        if producers.next().is_some()
            || producer.first_word() != start
            || producer.semantic_word() != start
            || images.0.words.get(self.uploaded_word) != Some(&self.word)
            || images.1.words.get(start) != Some(&self.word)
        {
            return Err(fail(location));
        }
        Ok(())
    }
}

pub(super) fn read(
    definitions: (
        &TeletextEnhancementObjectDefinition,
        &TeletextEnhancementObjectCatalog,
    ),
    program: &EncodedTeletextLocalObjectProgram,
    images: (&stream::Image, &stream::Image),
    index: usize,
    location: Location,
    deadline: Instant,
) -> Result<(raw::Body, raw::Body, Option<ProspectiveCarrier>)> {
    location.check_deadline(deadline)?;
    let (definition, _) = definitions;
    let start = images.0.markers[index] + 1;
    let old = images.0.body(index);
    let new = images.1.body(index);
    if old.len() > 208 || new.len() > 208 {
        return Err(fail(location));
    }
    let originals = original_atoms(
        definitions,
        program,
        (images.0, old, start),
        location,
        deadline,
    )?;
    let uploaded = uploaded_atoms(definitions, program, (images.1, new), location, deadline)?;
    let mut before = collapse(
        raw::Body::read(old, location, deadline)?,
        &originals,
        location,
        deadline,
    )?;
    let mut after = collapse(
        raw::Body::read(new, location, deadline)?,
        &uploaded,
        location,
        deadline,
    )?;
    let mut observed_carrier = false;
    for position in program.command_positions() {
        location.check_deadline(deadline)?;
        let Some(ordinal) = header_ordinal(position, definition) else {
            continue;
        };
        observed_carrier = true;
        // The supported normalizer inserts a single carrier for the leading
        // authored AP0. It is not an authored editable style command. A changed
        // call topology can remove that carrier; final native equality decides
        // whether the replacement still needs it.
        if ordinal != 0 || position.first_word() != start || position.semantic_word() != start {
            return Err(fail(location));
        }
        let original = before
            .entries
            .first()
            .copied()
            .ok_or_else(|| fail(location))?;
        if original.index != 0 || original.word != old[0] || original.style.is_none() {
            return Err(fail(location));
        }
        before.entries.remove(0);
        if let Some(entry) = after.entries.first() {
            if entry.index == 0 && entry.word == original.word {
                after.entries.remove(0);
                if after.entries.first().is_some_and(|entry| {
                    entry.style.as_ref().and_then(super::style::kind)
                        == original.style.as_ref().and_then(super::style::kind)
                }) {
                    return Err(fail(location));
                }
            } else if entry.style.is_some_and(|command| {
                super::style::kind(&command) == original.style.as_ref().and_then(super::style::kind)
            }) {
                return Err(fail(location));
            }
        }
    }
    let carrier = if observed_carrier {
        None
    } else {
        prospective(
            definition,
            &mut after,
            &uploaded,
            images.1.markers[index] + 1,
            location,
        )?
    };
    // Call insertion must not disguise a same-position glyph-family mutation.
    // Retain the existing check after removing the authenticated whole atoms.
    let noncalls = |body: &raw::Body| raw::Body {
        entries: body
            .entries
            .iter()
            .filter(|entry| !matches!(entry.command, Some(Command::InvokeObject { .. })))
            .copied()
            .collect(),
        selectors: Vec::new(),
    };
    raw::protect_fixed_families(&noncalls(&before), &noncalls(&after), location)?;
    Ok((before, after, carrier))
}

fn original_atoms(
    definitions: (
        &TeletextEnhancementObjectDefinition,
        &TeletextEnhancementObjectCatalog,
    ),
    program: &EncodedTeletextLocalObjectProgram,
    input: (&stream::Image, &[u32], usize),
    location: Location,
    deadline: Instant,
) -> Result<Vec<Atom>> {
    let (definition, _) = definitions;
    let (image, old, start) = input;
    let mut originals = Vec::new();
    for (ordinal, command) in definition.program.commands().iter().enumerate() {
        location.check_deadline(deadline)?;
        if !matches!(command, Command::InvokeObject { target, .. } if target.scope() == TeletextObjectScope::Local)
        {
            continue;
        }
        let mut matching = program
            .command_positions()
            .iter()
            .filter(|position| original_ordinal(position, definition) == Some(ordinal));
        let position = matching.next().ok_or_else(|| fail(location))?;
        if matching.next().is_some() {
            return Err(fail(location));
        }
        let first = position
            .first_word()
            .checked_sub(start)
            .ok_or_else(|| fail(location))?;
        let last = position
            .semantic_word()
            .checked_sub(start)
            .ok_or_else(|| fail(location))?;
        let words = old.get(first..=last).ok_or_else(|| fail(location))?;
        if calls::decode(words, definitions, program, image, location)? != *command {
            return Err(fail(location));
        }
        originals.push(Atom {
            first,
            last,
            command: *command,
        });
    }
    Ok(originals)
}

fn uploaded_atoms(
    definitions: (
        &TeletextEnhancementObjectDefinition,
        &TeletextEnhancementObjectCatalog,
    ),
    program: &EncodedTeletextLocalObjectProgram,
    input: (&stream::Image, &[u32]),
    location: Location,
    deadline: Instant,
) -> Result<Vec<Atom>> {
    let (image, new) = input;
    let mut uploaded = Vec::new();
    let mut cursor = 0;
    while cursor < new.len() {
        location.check_deadline(deadline)?;
        let word = new[cursor];
        let mode = (word >> 6) & 31;
        if word & 63 >= 40 && (16..=19).contains(&mode) {
            let count = if mode == 16 { 2 } else { 1 };
            let end = cursor.checked_add(count).ok_or_else(|| fail(location))?;
            let words = new.get(cursor..end).ok_or_else(|| fail(location))?;
            // Existing Public/Global calls remain physical context anchors;
            // this path grants no resource-call decoding or insertion authority.
            if words.last().is_some_and(|call| call & 0x30 == 0x30) {
                cursor = end;
                continue;
            }
            let command = calls::decode(words, definitions, program, image, location)?;
            uploaded.push(Atom {
                first: cursor,
                last: end - 1,
                command,
            });
            cursor = end;
        } else {
            cursor += 1;
        }
    }
    Ok(uploaded)
}

fn prospective(
    definition: &TeletextEnhancementObjectDefinition,
    after: &mut raw::Body,
    calls: &[Atom],
    start: usize,
    location: Location,
) -> Result<Option<ProspectiveCarrier>> {
    let Some(Command::SetPosition { position }) = definition.program.commands().first() else {
        return Ok(None);
    };
    if position.row() != 0
        || position.column() < 8
        || !matches!(
            definition.object_type,
            TeletextObjectType::Active | TeletextObjectType::Adaptive
        )
    {
        return Ok(None);
    }
    let Some(entry) = after.entries.first() else {
        return Ok(None);
    };
    if !matches!(
        entry.style,
        Some(Command::AtColumn {
            operation: TeletextColumnOperation::Background { .. },
            ..
        })
    ) {
        return Ok(None);
    }
    if entry.index != 0
        || entry.word & 63 != u32::from(position.column())
        || calls.first().is_none_or(|call| call.first != 1)
    {
        return Err(fail(location));
    }
    let carrier = ProspectiveCarrier {
        identity: definition.identity,
        position: *position,
        uploaded_word: start,
        word: entry.word,
    };
    after.entries.remove(0);
    Ok(Some(carrier))
}

fn collapse(
    mut body: raw::Body,
    atoms: &[Atom],
    location: Location,
    deadline: Instant,
) -> Result<raw::Body> {
    let mut entries = Vec::with_capacity(body.entries.len());
    let mut cursor = 0;
    for atom in atoms {
        location.check_deadline(deadline)?;
        if atom.first > atom.last
            || body
                .selectors
                .iter()
                .any(|selector| (atom.first..=atom.last).contains(&selector.index))
        {
            return Err(fail(location));
        }
        while body
            .entries
            .get(cursor)
            .is_some_and(|entry| entry.index < atom.first)
        {
            entries.push(body.entries[cursor]);
            cursor += 1;
        }
        for index in atom.first..=atom.last {
            let entry = *body.entries.get(cursor).ok_or_else(|| fail(location))?;
            if entry.index != index
                || entry.command.is_some()
                || entry.style.is_some()
                || entry.selector.is_some()
            {
                return Err(fail(location));
            }
            if index == atom.last {
                entries.push(raw::Entry {
                    command: Some(atom.command),
                    ..entry
                });
            }
            cursor += 1;
        }
    }
    entries.extend_from_slice(&body.entries[cursor..]);
    body.entries = entries;
    Ok(body)
}

pub(super) fn changed(candidate: &Candidate) -> bool {
    candidate
        .expected
        .commands()
        .iter()
        .enumerate()
        .any(|(ordinal, command)| {
            matches!(command, Command::InvokeObject { .. })
                && !candidate.retained.contains(&Some(ordinal))
        })
        || candidate
            .replacement
            .iter()
            .zip(&candidate.retained)
            .any(|(command, source)| {
                source.is_none() && matches!(command, Command::InvokeObject { .. })
            })
}
