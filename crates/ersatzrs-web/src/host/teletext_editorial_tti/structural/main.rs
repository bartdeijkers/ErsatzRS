//! Main call structure retains anchors and reuses existing authored scalar checks.
use super::{fail, program, stream, Location, Result};
use ersatzrs_domain::{
    TeletextColumnOperation, TeletextEnhancementCommand as Command,
    TeletextEnhancementObjectCatalog, TeletextEnhancementProgram,
    TeletextMainLocalInvocationProgramOverlay, TeletextObjectScope, TeletextPageAddress,
};
use ersatzrs_ffmpeg::teletext::{
    EncodedTeletextAuthoredService, EncodedTeletextLocalObjectProgram,
};
use std::time::Instant;

mod correspondence;
mod header;
#[cfg(test)]
mod tests;

struct Unit {
    first: usize,
    last: usize,
    original: Option<usize>,
    call: Option<Command>,
    editable: bool,
    scalar: Option<(u8, TeletextColumnOperation)>,
}

pub(super) fn reconcile(
    service: &EncodedTeletextAuthoredService,
    owner: TeletextPageAddress,
    native: &EncodedTeletextLocalObjectProgram,
    images: (&stream::Image, &stream::Image),
    aligned: &mut [u32],
    location: Location,
    deadline: Instant,
) -> Result<Option<TeletextMainLocalInvocationProgramOverlay>> {
    location.check_deadline(deadline)?;
    if aligned.len() > 208 {
        return Err(fail(location));
    }
    let mut scalar = aligned.to_vec();
    if images.0.markers.first() == images.1.markers.first()
        && super::main_words(
            service,
            owner,
            native,
            images,
            &mut scalar,
            location,
            deadline,
        )
        .is_ok()
    {
        aligned.copy_from_slice(&scalar);
        return Ok(None);
    }
    location.check_deadline(deadline)?;
    let semantic = service.declared().semantic();
    let original = semantic
        .page_enhancements()
        .get(&owner)
        .and_then(|page| page.local_program.as_ref())
        .ok_or_else(|| fail(location))?;
    parse(
        original,
        semantic.objects(),
        native,
        images,
        aligned,
        location,
        deadline,
    )
    .map(Some)
}

fn parse(
    original: &TeletextEnhancementProgram,
    catalog: &TeletextEnhancementObjectCatalog,
    native: &EncodedTeletextLocalObjectProgram,
    images: (&stream::Image, &stream::Image),
    aligned: &mut [u32],
    location: Location,
    deadline: Instant,
) -> Result<TeletextMainLocalInvocationProgramOverlay> {
    location.check_deadline(deadline)?;
    if original.commands().len() > 208 || images.0.words.len() > 208 || images.1.words.len() > 208 {
        return Err(fail(location));
    }
    let mut old = units(catalog, native, images.0, location, deadline)?;
    let mut new = units(catalog, native, images.1, location, deadline)?;
    sources(original, native, &mut old, location, deadline)?;
    let carrier = header::prepare(
        original,
        native,
        (&mut old, &mut new),
        images,
        location,
        deadline,
    )?;
    let steps = correspondence::align((&old, &new), images, location, deadline)?;
    let scalars = correspondence::scalars(
        original,
        (&old, &new),
        &steps,
        images,
        native.owner(),
        location,
        deadline,
    )?;
    let (replacement, retained) =
        correspondence::assemble(&scalars.expected, (&old, &new), &steps, location, deadline)?;
    if let Some(carrier) = carrier {
        carrier.check_candidate(original, &replacement, &retained, native.owner(), location)?;
    }
    let intent = TeletextMainLocalInvocationProgramOverlay::new(
        native.owner(),
        scalars.expected,
        TeletextEnhancementProgram::new(replacement).map_err(|_| fail(location))?,
        retained,
    )
    .map_err(|_| fail(location))?;
    // Commit only validated scalar words at their original packed positions.
    // The existing scalar reconciler derives intents from this old-size image.
    if scalars
        .words
        .iter()
        .any(|(index, _)| *index >= aligned.len())
    {
        return Err(fail(location));
    }
    for (index, word) in scalars.words {
        aligned[index] = word;
    }
    Ok(intent)
}

fn units(
    catalog: &TeletextEnhancementObjectCatalog,
    native: &EncodedTeletextLocalObjectProgram,
    image: &stream::Image,
    location: Location,
    deadline: Instant,
) -> Result<Vec<Unit>> {
    let end = *image.markers.first().ok_or_else(|| fail(location))?;
    let words = image.words.get(..end).ok_or_else(|| fail(location))?;
    if end > 208 || words.iter().any(|word| *word > 0x3ffff) {
        return Err(fail(location));
    }
    let mut result = Vec::new();
    let mut cursor = 0;
    while cursor < end {
        location.check_deadline(deadline)?;
        let word = words[cursor];
        let mode = (word >> 6) & 31;
        let origin = word & 63 >= 40 && mode == 16;
        let last = cursor + usize::from(origin);
        let atom = words.get(cursor..=last).ok_or_else(|| fail(location))?;
        if origin
            && !atom
                .last()
                .is_some_and(|word| word & 63 >= 40 && (17..=19).contains(&((word >> 6) & 31)))
        {
            return Err(fail(location));
        }
        let call =
            program::decode_main(atom, native.owner(), catalog, native, image, location).ok();
        result.push(Unit {
            first: cursor,
            last,
            original: None,
            call,
            editable: false,
            scalar: None,
        });
        cursor = last + 1;
    }
    Ok(result)
}

fn sources(
    original: &TeletextEnhancementProgram,
    native: &EncodedTeletextLocalObjectProgram,
    units: &mut [Unit],
    location: Location,
    deadline: Instant,
) -> Result<()> {
    for (ordinal, command) in original.commands().iter().enumerate() {
        location.check_deadline(deadline)?;
        let mut count = 0;
        for position in native.command_positions().iter().filter(|position| {
            position.source().object.is_none() && position.original_local_command() == Some(ordinal)
        }) {
            let unit = units
                .iter_mut()
                .find(|unit| unit.last == position.semantic_word())
                .ok_or_else(|| fail(location))?;
            if unit.original.is_some_and(|previous| previous != ordinal) {
                return Err(fail(location));
            }
            unit.original = Some(ordinal);
            if matches!(command, Command::InvokeObject { target, .. } if target.scope() == TeletextObjectScope::Local)
            {
                if unit.first != position.first_word() || unit.call != Some(*command) {
                    return Err(fail(location));
                }
                unit.editable = true;
            }
            count += 1;
        }
        if (count == 0
            && !matches!(command, Command::SetPosition { position } if position.row() == 0))
            || (count != 1 && matches!(command, Command::InvokeObject { .. }))
        {
            return Err(fail(location));
        }
        if count == 1 {
            if let Command::AtColumn { column, operation } = *command {
                let position = native
                    .command_positions()
                    .iter()
                    .find(|position| {
                        position.source().object.is_none()
                            && position.original_local_command() == Some(ordinal)
                    })
                    .ok_or_else(|| fail(location))?;
                if position.first_word() == position.semantic_word() {
                    let unit = units
                        .iter_mut()
                        .find(|unit| {
                            unit.first == position.first_word()
                                && unit.last == position.semantic_word()
                        })
                        .ok_or_else(|| fail(location))?;
                    unit.scalar = Some((column, operation));
                }
            }
        }
    }
    if units
        .iter()
        .filter_map(|unit| unit.original)
        .collect::<Vec<_>>()
        .windows(2)
        .any(|pair| pair[0] > pair[1])
    {
        return Err(fail(location));
    }
    Ok(())
}

pub(super) fn qualify(
    baseline: (&EncodedTeletextLocalObjectProgram, &stream::Image),
    service: &EncodedTeletextAuthoredService,
    native: &EncodedTeletextLocalObjectProgram,
    images: (&stream::Image, &stream::Image),
    intent: &TeletextMainLocalInvocationProgramOverlay,
    location: Location,
    deadline: Instant,
) -> Result<()> {
    location.check_deadline(deadline)?;
    let semantic = service.declared().semantic();
    let actual = semantic
        .page_enhancements()
        .get(&intent.owner())
        .and_then(|page| page.local_program.as_ref())
        .ok_or_else(|| fail(location))?;
    if native.owner() != intent.owner() || actual != intent.replacement() {
        return Err(fail(location));
    }
    header::qualify(baseline, native, images, intent, location, deadline)?;
    let image = images.1;
    for (ordinal, command) in actual.commands().iter().enumerate() {
        location.check_deadline(deadline)?;
        if !matches!(command, Command::InvokeObject { target, .. } if target.scope() == TeletextObjectScope::Local)
        {
            continue;
        }
        let mut positions = native.command_positions().iter().filter(|position| {
            position.source().object.is_none() && position.original_local_command() == Some(ordinal)
        });
        let position = positions.next().ok_or_else(|| fail(location))?;
        if positions.next().is_some() || position.semantic_word() >= image.markers[0] {
            return Err(fail(location));
        }
        let words = image
            .words
            .get(position.first_word()..=position.semantic_word())
            .ok_or_else(|| fail(location))?;
        if program::decode_main(
            words,
            native.owner(),
            semantic.objects(),
            native,
            image,
            location,
        )? != *command
        {
            return Err(fail(location));
        }
    }
    Ok(())
}
