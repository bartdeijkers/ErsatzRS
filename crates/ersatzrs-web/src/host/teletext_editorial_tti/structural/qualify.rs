//! Uploaded structure is authenticated against old ownership before native rebuild.
use super::{fail, raw, stream, Location, Page, Result, TeletextEditorialChanges};
use ersatzrs_domain::TeletextPageAddress;
use ersatzrs_ffmpeg::teletext::{
    EncodedTeletextAuthoredService, EncodedTeletextLocalObjectProgram,
};
use std::time::Instant;

pub(in super::super) fn check(
    baseline: &EncodedTeletextAuthoredService,
    final_service: &EncodedTeletextAuthoredService,
    owner: TeletextPageAddress,
    uploaded: &Page<'_>,
    changes: &TeletextEditorialChanges,
    location: Location,
    deadline: Instant,
) -> Result<()> {
    let before = super::super::operands::packet_words(baseline, owner, location, deadline)?;
    let mut original = Page::default();
    original.packets[0] = before.each_ref().map(Option::as_ref);
    let (derived, carriers) =
        super::reconcile_with_carriers(baseline, owner, &original, uploaded, location, deadline)?;
    exact(&derived, changes, owner, location)?;
    let old =
        super::super::operands::compiled_program(baseline, owner).ok_or_else(|| fail(location))?;
    let new = super::super::operands::compiled_program(final_service, owner)
        .ok_or_else(|| fail(location))?;
    let final_packets =
        super::super::operands::packet_words(final_service, owner, location, deadline)?;
    let mut final_page = Page::default();
    final_page.packets[0] = final_packets.each_ref().map(Option::as_ref);
    let a = stream::Image::read(uploaded, location, deadline)?;
    let b = stream::Image::read(&final_page, location, deadline)?;
    if b.markers.len() != new.definition_positions().len() {
        return Err(fail(location));
    }
    for carrier in carriers {
        location.check_deadline(deadline)?;
        carrier.qualify(final_service, new, (&a, &b), changes, location)?;
    }
    for definition in baseline
        .declared()
        .semantic()
        .objects()
        .definitions()
        .iter()
        .filter(|definition| definition.identity.owner() == Some(owner))
    {
        location.check_deadline(deadline)?;
        let i = old
            .definition_positions()
            .iter()
            .position(|value| value.target() == definition.identity)
            .ok_or_else(|| fail(location))?;
        let j = new
            .definition_positions()
            .iter()
            .position(|value| value.target() == definition.identity)
            .ok_or_else(|| fail(location))?;
        if old.definition_positions()[i].object_type()
            != new.definition_positions()[j].object_type()
        {
            return Err(fail(location));
        }
        let translated = a
            .body(i)
            .iter()
            .map(|word| translate(*word, old, new, &a, &b, location))
            .collect::<Result<Vec<_>>>()?;
        if tokens(&translated, location, deadline)? != tokens(b.body(j), location, deadline)? {
            return Err(fail(location));
        }
    }
    main_sources(old, new, &a, &b, location, deadline)
}
fn exact(
    a: &TeletextEditorialChanges,
    b: &TeletextEditorialChanges,
    owner: TeletextPageAddress,
    location: Location,
) -> Result<()> {
    let object = b
        .object_operands
        .iter()
        .filter(|value| value.owner() == owner)
        .collect::<Vec<_>>();
    let pages = b
        .operands
        .iter()
        .filter(|value| value.owner() == owner)
        .collect::<Vec<_>>();
    let programs = b
        .object_programs
        .iter()
        .filter(|value| value.owner() == owner)
        .collect::<Vec<_>>();
    if pages.len() != a.operands.len()
        || object.len() != a.object_operands.len()
        || programs.len() != a.object_programs.len()
        || a.operands.iter().any(|value| {
            pages
                .iter()
                .filter(|candidate| ***candidate == *value)
                .count()
                != 1
        })
        || a.object_operands.iter().any(|value| {
            object
                .iter()
                .filter(|candidate| ***candidate == *value)
                .count()
                != 1
        })
        || a.object_programs.iter().any(|value| {
            programs
                .iter()
                .filter(|candidate| ***candidate == *value)
                .count()
                != 1
        })
    {
        return Err(fail(location));
    }
    Ok(())
}
fn translate(
    word: u32,
    old: &EncodedTeletextLocalObjectProgram,
    new: &EncodedTeletextLocalObjectProgram,
    a: &stream::Image,
    b: &stream::Image,
    location: Location,
) -> Result<u32> {
    let address = word & 63;
    let mode = (word >> 6) & 31;
    if address < 40 || !(17..=19).contains(&mode) || address & 0x18 != 8 {
        return Ok(word);
    }
    let data = (word >> 11) & 127;
    let index = usize::try_from((((address & 1) * 8 + (data >> 4)) * 13) + (data & 15))
        .map_err(|_| fail(location))?;
    let i = a
        .markers
        .iter()
        .position(|value| *value == index)
        .ok_or_else(|| fail(location))?;
    let identity = old
        .definition_positions()
        .get(i)
        .ok_or_else(|| fail(location))?
        .target();
    let j = new
        .definition_positions()
        .iter()
        .position(|value| value.target() == identity)
        .ok_or_else(|| fail(location))?;
    let index = b.markers[j];
    let data = u32::try_from(((index / 13) & 7) * 16 + index % 13).map_err(|_| fail(location))?;
    Ok((address & 0x38) | u32::from(index >= 104) | (mode << 6) | (data << 11))
}
#[derive(Debug, PartialEq, Eq)]
enum Token {
    Word(u32),
    Drcs {
        column: u32,
        scope: usize,
        subtable: u32,
        slot: u32,
    },
}
fn tokens(words: &[u32], location: Location, deadline: Instant) -> Result<Vec<Token>> {
    let body = raw::Body::read(words, location, deadline)?;
    Ok(body
        .entries
        .into_iter()
        .map(|entry| match entry.command {
            Some(ersatzrs_domain::TeletextEnhancementCommand::AtColumn {
                column,
                operation: ersatzrs_domain::TeletextColumnOperation::Drcs { reference },
            }) => Token::Drcs {
                column: u32::from(column),
                scope: usize::from(
                    reference.table == ersatzrs_domain::TeletextDrcsTableScope::Normal,
                ),
                subtable: u32::from(reference.subtable.get()),
                slot: u32::from(reference.slot.get()),
            },
            _ => Token::Word(entry.word),
        })
        .collect())
}
fn main_sources(
    old: &EncodedTeletextLocalObjectProgram,
    new: &EncodedTeletextLocalObjectProgram,
    a: &stream::Image,
    b: &stream::Image,
    location: Location,
    deadline: Instant,
) -> Result<()> {
    for position in old
        .command_positions()
        .iter()
        .filter(|value| value.source().object.is_none() && value.original_local_command().is_some())
    {
        location.check_deadline(deadline)?;
        let mut matches = new.command_positions().iter().filter(|value| {
            value.source().object.is_none()
                && value.original_local_command() == position.original_local_command()
        });
        let final_position = matches.next().ok_or_else(|| fail(location))?;
        if matches.next().is_some() {
            return Err(fail(location));
        }
        let before = *a
            .words
            .get(position.semantic_word())
            .ok_or_else(|| fail(location))?;
        let after = *b
            .words
            .get(final_position.semantic_word())
            .ok_or_else(|| fail(location))?;
        if (before >> 6) & 31 == 13 && before & 63 < 40 {
            let first = position.semantic_word();
            let last = final_position.semantic_word();
            let left = tokens(&a.words[..=first], location, deadline)?
                .pop()
                .ok_or_else(|| fail(location))?;
            let right = tokens(&b.words[..=last], location, deadline)?
                .pop()
                .ok_or_else(|| fail(location))?;
            if left != right {
                return Err(fail(location));
            }
        } else if translate(before, old, new, a, b, location)? != after {
            return Err(fail(location));
        }
    }
    Ok(())
}
