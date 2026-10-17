//! Sealed original callee context never becomes editable native resource input.
use super::super::structural::stream;
use super::{
    compare, invalid, project, Failure, Location, Mapping, ObjectEditorFrame, Page, Result,
    TeletextTtiBudget,
};
use crate::host::teletext_export_project::resource_objects::{
    build_context, build_context_with_root, ObjectEditorCommandPosition, ObjectEditorContextFrame,
};
use ersatzrs_domain::{
    TeletextEnhancementCommand, TeletextObjectIdentity, TeletextObjectOffset, TeletextObjectScope,
    TeletextObjectType,
};
use ersatzrs_ffmpeg::teletext::allocate_teletext_resource_objects;
use std::{collections::BTreeMap, time::Instant};

fn build(
    mapped: &Mapping<'_>,
    limits: (TeletextTtiBudget, usize),
    deadline: Instant,
) -> Result<ObjectEditorContextFrame> {
    build_context(
        mapped.service,
        mapped.identity,
        limits.1,
        limits.0,
        deadline,
    )
    .map_err(|error| export_error(&error, mapped.location))
}
fn export_error(
    error: &crate::host::teletext_export_project::NativeTeletextExportProjectError,
    location: Location,
) -> super::NativeTeletextEditorialTtiError {
    location.error(match error {
        crate::host::teletext_export_project::NativeTeletextExportProjectError::TimedOut => {
            Failure::TimedOut
        }
        _ => Failure::Structure,
    })
}
pub(super) fn frame(
    mapped: &Mapping<'_>,
    limits: (TeletextTtiBudget, usize),
    deadline: Instant,
) -> Result<ObjectEditorFrame> {
    let frame = build(mapped, limits, deadline)?;
    let root = frame
        .encoded
        .definition_positions()
        .first()
        .ok_or_else(invalid)?
        .target();
    let command_positions = frame
        .encoded
        .command_positions()
        .iter()
        .filter(|position| position.source().object == Some(root))
        .map(|position| ObjectEditorCommandPosition {
            command: position.source().command,
            editor_word: position.semantic_word(),
        })
        .collect();
    Ok(ObjectEditorFrame {
        identity: frame.identity,
        object_type: frame.object_type,
        command_positions,
        document: frame.document,
    })
}

struct Images {
    frame: ObjectEditorContextFrame,
    before: stream::Image,
    after: stream::Image,
}
fn images(
    mapped: &Mapping<'_>,
    baseline: Option<&[u8]>,
    uploaded: &[u8],
    limits: (TeletextTtiBudget, usize),
    deadline: Instant,
) -> Result<Images> {
    let frame = build(mapped, limits, deadline)?;
    let canonical = Page::index_identity(&frame.document, 256, 0, mapped.location, deadline)?;
    if let Some(baseline) = baseline {
        let document = mapped.location.parse(baseline, limits.0, deadline)?;
        let page = Page::index_identity(&document, 256, 0, mapped.location, deadline)?;
        compare(&canonical, &page, mapped.location, false, deadline)?;
    }
    let document = mapped.location.parse(uploaded, limits.0, deadline)?;
    let page = Page::index_identity(&document, 256, 0, mapped.location, deadline)?;
    compare(
        &project::without_x26(&canonical),
        &project::without_x26(&page),
        mapped.location,
        false,
        deadline,
    )?;
    let result = Images {
        before: stream::Image::read(&canonical, mapped.location, deadline)?,
        after: stream::Image::read(&page, mapped.location, deadline)?,
        frame,
    };
    protect(&result, mapped.location, deadline)?;
    Ok(result)
}
fn protect(images: &Images, location: Location, deadline: Instant) -> Result<()> {
    let (before, after) = (&images.before, &images.after);
    let positions = images.frame.encoded.definition_positions();
    if before.markers.len() != positions.len()
        || after.markers.len() != positions.len()
        || images.frame.aliases.len() != positions.len()
        || before.markers.first() != after.markers.first()
    {
        return Err(location.error(Failure::Structure));
    }
    let first = *before.markers.first().ok_or_else(invalid)?;
    protect_words(
        &before.words[..first],
        &after.words[..first],
        before,
        after,
        location,
        deadline,
    )?;
    for (index, position) in positions.iter().enumerate() {
        location.check_deadline(deadline)?;
        let (a, b) = (before.markers[index], after.markers[index]);
        if usize::from(position.absolute_index()) != a
            || stream::marker(before.words[a], b, location)? != after.words[b]
        {
            return Err(location.error(Failure::Structure));
        }
        if index != 0 {
            protect_words(
                before.body(index),
                after.body(index),
                before,
                after,
                location,
                deadline,
            )?;
        }
    }
    Ok(())
}
fn protect_words(
    old: &[u32],
    new: &[u32],
    before: &stream::Image,
    after: &stream::Image,
    location: Location,
    deadline: Instant,
) -> Result<()> {
    if old.len() != new.len() {
        return Err(location.error(Failure::Structure));
    }
    for (old, new) in old.iter().zip(new) {
        location.check_deadline(deadline)?;
        if stream::relocate(*old, before, after, location)? != *new {
            return Err(location.error(Failure::Structure));
        }
    }
    Ok(())
}

struct NativeTarget {
    word: u32,
    identity: TeletextObjectIdentity,
    object_type: TeletextObjectType,
}
fn native_targets(
    mapped: &Mapping<'_>,
    images: &Images,
    limits: project::Limits,
    deadline: Instant,
) -> Result<BTreeMap<u32, NativeTarget>> {
    let fail = || mapped.location.error(Failure::Structure);
    let declared = mapped.service.declared();
    let defaults = declared
        .defaults()
        .iter()
        .map(ersatzrs_domain::TeletextDeclaredDefaultReservation::selection)
        .collect::<Vec<_>>();
    let allocations = allocate_teletext_resource_objects(
        declared.semantic().objects(),
        &defaults,
        limits.2.max_objects,
    )
    .map_err(|_| fail())?;
    let positions = images.frame.encoded.definition_positions();
    let mut result = BTreeMap::new();
    for (index, position) in positions.iter().enumerate() {
        mapped.location.check_deadline(deadline)?;
        let target = *images.frame.aliases.get(index).ok_or_else(fail)?;
        let object_type = position.object_type();
        let allocation = allocations.lookup(target, object_type).ok_or_else(fail)?;
        let scope = match target.scope() {
            TeletextObjectScope::Public => 0x30,
            TeletextObjectScope::Global => 0x38,
            TeletextObjectScope::Local => return Err(fail()),
        };
        let mode = match object_type {
            TeletextObjectType::Active => 17,
            TeletextObjectType::Adaptive => 18,
            TeletextObjectType::Passive => 19,
        };
        let number = u32::from(allocation.object_number());
        let native = scope | (number >> 7) | (mode << 6) | ((number & 127) << 11);
        let marker = *images
            .before
            .words
            .get(usize::from(position.absolute_index()))
            .ok_or_else(fail)?;
        if (marker >> 6) & 31 != mode + 4 {
            return Err(fail());
        }
        // The checked native Local compiler owns every alias marker/pointer.
        // Invocation modes are exactly four less than definition modes.
        let editor = marker.checked_sub(4 << 6).ok_or_else(fail)?;
        if result
            .insert(
                editor,
                NativeTarget {
                    word: native,
                    identity: target,
                    object_type,
                },
            )
            .is_some()
        {
            return Err(fail());
        }
    }
    Ok(result)
}
fn translate(
    words: &[u32],
    images: (&stream::Image, &stream::Image),
    targets: &BTreeMap<u32, NativeTarget>,
    location: Location,
    deadline: Instant,
) -> Result<(Vec<u32>, Vec<super::topology::Call>)> {
    let mut result = Vec::new();
    let mut calls = Vec::new();
    result
        .try_reserve_exact(words.len())
        .map_err(|_| invalid())?;
    for (index, &word) in words.iter().enumerate() {
        location.check_deadline(deadline)?;
        let value = if word & 63 >= 40 && (17..=19).contains(&((word >> 6) & 31)) {
            let original = stream::relocate(word, images.1, images.0, location)?;
            if stream::relocate(original, images.0, images.1, location)? != word {
                return Err(location.error(Failure::Structure));
            }
            let target = targets
                .get(&original)
                .ok_or_else(|| location.error(Failure::Structure))?;
            let first =
                if index > 0 && words[index - 1] & 63 >= 40 && (words[index - 1] >> 6) & 31 == 16 {
                    index - 1
                } else {
                    index
                };
            let offset = if first == index {
                None
            } else {
                let row = u8::try_from((words[first] & 63) - 40)
                    .map_err(|_| location.error(Failure::Structure))?;
                let column = u8::try_from(words[first] >> 11)
                    .map_err(|_| location.error(Failure::Structure))?;
                Some(
                    TeletextObjectOffset::new(row, column)
                        .map_err(|_| location.error(Failure::Structure))?,
                )
            };
            calls.push(super::topology::Call {
                first,
                word: index,
                command: TeletextEnhancementCommand::InvokeObject {
                    target: target.identity,
                    object_type: target.object_type,
                    offset,
                },
            });
            target.word
        } else {
            word
        };
        result.push(value);
    }
    Ok((result, calls))
}
pub(super) fn body(
    mapped: &Mapping<'_>,
    baseline: Option<&[u8]>,
    uploaded: &[u8],
    limits: project::Limits,
    deadline: Instant,
) -> Result<project::Body> {
    let images = images(mapped, baseline, uploaded, (limits.0, limits.1), deadline)?;
    let targets = native_targets(mapped, &images, limits, deadline)?;
    let before = project::native_body(mapped, limits, deadline)?;
    let (original, _) = translate(
        images.before.body(0),
        (&images.before, &images.before),
        &targets,
        mapped.location,
        deadline,
    )?;
    if before != original {
        return Err(mapped.location.error(Failure::Structure));
    }
    let (after, context_calls) = translate(
        images.after.body(0),
        (&images.before, &images.after),
        &targets,
        mapped.location,
        deadline,
    )?;
    // Distinct sealed aliases may share one native resource selector. Never
    // discard that authored identity edit as an unchanged native word stream.
    if before == after && images.before.words != images.after.words {
        return Err(mapped.location.error(Failure::Structure));
    }
    // The same original full resource spans feed the existing V16 native parser.
    let spans = project::native_spans(mapped, limits, deadline)?;
    Ok(project::Body {
        before,
        after,
        spans,
        context_calls: Some(context_calls),
    })
}
pub(super) fn validate_applied(
    original: &Mapping<'_>,
    candidate: &Mapping<'_>,
    uploaded: &[u8],
    limits: project::Limits,
    deadline: Instant,
) -> Result<()> {
    let frame = build_context_with_root(
        original.service,
        original.identity,
        &candidate.definition.program,
        limits.1,
        limits.0,
        deadline,
    )
    .map_err(|error| export_error(&error, original.location))?;
    let expected = Page::index_identity(&frame.document, 256, 0, original.location, deadline)?;
    let document = original.location.parse(uploaded, limits.0, deadline)?;
    let actual = Page::index_identity(&document, 256, 0, original.location, deadline)?;
    compare(&expected, &actual, original.location, false, deadline)
}

#[cfg(test)]
pub(super) mod tests;
