//! Derived AP0 carriers have producer provenance, never authored scalar authority.
use super::{fail, stream, Command, Location, Result, TeletextEnhancementProgram, Unit};
use ersatzrs_domain::{
    TeletextEnhancementPosition, TeletextMainLocalInvocationProgramOverlay, TeletextObjectScope,
    TeletextPageAddress,
};
use ersatzrs_ffmpeg::teletext::{EncodedTeletextLocalObjectProgram, TeletextMainCompositionSource};
use std::time::Instant;

#[cfg(test)]
mod tests;

pub(super) struct ProspectiveCarrier {
    position: TeletextEnhancementPosition,
}
impl ProspectiveCarrier {
    pub(super) fn check_candidate(
        &self,
        original: &TeletextEnhancementProgram,
        replacement: &[Command],
        retained: &[Option<usize>],
        owner: TeletextPageAddress,
        location: Location,
    ) -> Result<()> {
        let ap = Command::SetPosition {
            position: self.position,
        };
        if original.commands().first() != Some(&ap)
            || replacement.first() != Some(&ap)
            || retained.first() != Some(&Some(0))
            || !local_call(replacement.get(1), owner)
        {
            return Err(fail(location));
        }
        Ok(())
    }
}

pub(super) fn prepare(
    original: &TeletextEnhancementProgram,
    native: &EncodedTeletextLocalObjectProgram,
    units: (&mut Vec<Unit>, &mut Vec<Unit>),
    images: (&stream::Image, &stream::Image),
    location: Location,
    deadline: Instant,
) -> Result<Option<ProspectiveCarrier>> {
    location.check_deadline(deadline)?;
    let (old, new) = units;
    let observed = observed(original, native, images.0, location, deadline)?;
    if let Some(span) = observed {
        if !prefix_units(old, span) {
            // Unqualified writers retain the established mandatory alignment.
            return Ok(None);
        }
        old.drain(..span);
    }
    if observed.is_none() && !ap_only(original, native) {
        return Ok(None);
    }
    prospective(original, native.owner(), new, images, observed, location)
}

fn observed(
    original: &TeletextEnhancementProgram,
    native: &EncodedTeletextLocalObjectProgram,
    image: &stream::Image,
    location: Location,
    deadline: Instant,
) -> Result<Option<usize>> {
    location.check_deadline(deadline)?;
    let Some(position) = leading_position(original) else {
        return Ok(None);
    };
    let mut observed = None;
    for producer in native.command_positions() {
        location.check_deadline(deadline)?;
        if producer.source().object.is_some()
            || producer.original_main_source()
                != Some(TeletextMainCompositionSource::HeaderPosition { command: 0 })
            || producer.first_word() != 0
            || producer.semantic_word().checked_add(1) != carrier_span(image, position.column())
            || producer.original_local_command().is_some()
            || producer.original_base_glyph().is_some()
        {
            continue;
        }
        if observed.is_some() {
            return Err(fail(location));
        }
        observed = carrier_span(image, position.column());
    }
    Ok(observed)
}

fn ap_only(
    original: &TeletextEnhancementProgram,
    native: &EncodedTeletextLocalObjectProgram,
) -> bool {
    original.commands().len() == 1 && native.main_triplet_count() == 0
}

fn prospective(
    original: &TeletextEnhancementProgram,
    owner: TeletextPageAddress,
    new: &mut Vec<Unit>,
    images: (&stream::Image, &stream::Image),
    observed: Option<usize>,
    location: Location,
) -> Result<Option<ProspectiveCarrier>> {
    let Some(position) = leading_position(original) else {
        return Ok(None);
    };
    let Some(span) = carrier_span(images.1, position.column()) else {
        return Ok(None);
    };
    // The complete bounded span may include the generated row-zero compatibility
    // selector. None of its words is an authored command or scalar permission.
    if !prefix_units(new, span)
        || new
            .get(span)
            .is_none_or(|unit| unit.first != span || !local_call(unit.call.as_ref(), owner))
    {
        return Ok(None);
    }
    if let Some(before) = observed {
        if images.0.words.get(..before) != images.1.words.get(..span) {
            return Err(fail(location));
        }
    }
    new.drain(..span);
    Ok(Some(ProspectiveCarrier { position }))
}

pub(super) fn qualify(
    baseline: (&EncodedTeletextLocalObjectProgram, &stream::Image),
    native: &EncodedTeletextLocalObjectProgram,
    images: (&stream::Image, &stream::Image),
    intent: &TeletextMainLocalInvocationProgramOverlay,
    location: Location,
    deadline: Instant,
) -> Result<()> {
    location.check_deadline(deadline)?;
    if observed(
        intent.expected(),
        baseline.0,
        baseline.1,
        location,
        deadline,
    )?
    .is_none()
        && !ap_only(intent.expected(), baseline.0)
    {
        return Ok(());
    }
    let Some(position) = leading_position(intent.expected()) else {
        return Ok(());
    };
    let Some(span) = carrier_span(images.0, position.column()) else {
        return Ok(());
    };
    if !local_call(intent.replacement().commands().get(1), native.owner()) {
        return Ok(());
    }
    ProspectiveCarrier { position }.check_candidate(
        intent.expected(),
        intent.replacement().commands(),
        intent.retained_commands(),
        native.owner(),
        location,
    )?;
    if baseline.0.owner() != native.owner() {
        return Err(fail(location));
    }
    let mut found = false;
    for producer in native.command_positions() {
        location.check_deadline(deadline)?;
        if producer.source().object.is_some()
            || producer.original_main_source()
                != Some(TeletextMainCompositionSource::HeaderPosition { command: 0 })
        {
            continue;
        }
        if found
            || producer.first_word() != 0
            || producer.semantic_word().checked_add(1) != Some(span)
            || producer.original_local_command().is_some()
            || producer.original_base_glyph().is_some()
        {
            return Err(fail(location));
        }
        found = true;
    }
    if !found || images.0.words.get(..span) != images.1.words.get(..span) {
        return Err(fail(location));
    }
    Ok(())
}

fn prefix_units(units: &[Unit], span: usize) -> bool {
    units.get(..span).is_some_and(|units| {
        units.iter().enumerate().all(|(index, unit)| {
            unit.first == index
                && unit.last == index
                && unit.original.is_none()
                && unit.scalar.is_none()
                && unit.call.is_none()
        })
    })
}

fn carrier_span(image: &stream::Image, column: u8) -> Option<usize> {
    let first = *image.words.first()?;
    if carrier_colour(first, column) {
        return Some(1);
    }
    (first <= 0x3ffff
        && first & 63 == 63
        && (first >> 6) & 31 == 7
        && (first >> 11) & 127 <= 31
        && image
            .words
            .get(1)
            .is_some_and(|word| carrier_colour(*word, column)))
    .then_some(2)
}

fn leading_position(original: &TeletextEnhancementProgram) -> Option<TeletextEnhancementPosition> {
    match original.commands().first()? {
        Command::SetPosition { position } if position.row() == 0 && position.column() >= 8 => {
            Some(*position)
        }
        _ => None,
    }
}

// The materializer chooses only a known foreground, or its authenticated
// Base-background fallback. Actual colour/context is sealed by native replay.
fn carrier_colour(word: u32, column: u8) -> bool {
    word <= 0x3ffff
        && word & 63 == u32::from(column)
        && matches!((word >> 6) & 31, 0 | 3)
        && (word >> 11) & 127 <= 31
}

fn local_call(command: Option<&Command>, owner: TeletextPageAddress) -> bool {
    matches!(command, Some(Command::InvokeObject { target, .. })
        if target.scope() == TeletextObjectScope::Local && target.owner() == Some(owner))
}
