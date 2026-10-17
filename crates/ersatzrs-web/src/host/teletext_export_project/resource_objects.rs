//! Receipt-owned object views; Local aliases exist only in the editor wrapper.
use super::{
    check_deadline, Instant, NativeTeletextExportProjectError as Error, Result, TeletextTtiBudget,
    TeletextTtiDocument, TeletextTtiRecord,
};
use ersatzrs_domain::{
    TeletextEnhancementCommand, TeletextEnhancementObjectCatalog,
    TeletextEnhancementObjectDefinition, TeletextEnhancementProgram, TeletextObjectCatalogBudget,
    TeletextObjectIdentity, TeletextObjectType, TeletextPageAddress, TeletextPageNumber,
    TeletextResolvedObjectPageLinks, TeletextSubcode,
};
use ersatzrs_ffmpeg::teletext::{
    allocate_teletext_resource_objects, compile_teletext_local_program_with_objects,
    project_encoded_teletext_local_objects, project_encoded_teletext_object,
    EncodedTeletextAuthoredPage, EncodedTeletextObjectPage, TeletextEncodedDisplayRecord,
    TeletextEncodedObjectProjection,
};
use std::ops::Range;

mod fragments;
pub(in super::super) use fragments::{build_fragment, plan_fragments, ObjectEditorFragmentPlan};
mod context;
pub(in super::super) use context::{
    build_context, build_context_with_root, ObjectEditorContextFrame,
};

/// Carrier selection uses the native body range and the same real wrapper as V6.
/// Full fragment validation still runs before any editor body is allocated.
pub(in super::super) fn requires_fragments(
    page: &EncodedTeletextAuthoredPage,
    deadline: Instant,
) -> Result<bool> {
    check_deadline(deadline)?;
    let EncodedTeletextAuthoredPage::Object(resource) = page else {
        return Ok(false);
    };
    for position in resource.definition_positions() {
        check_deadline(deadline)?;
        let (kind, body) = body_range(resource, position.target(), deadline)?;
        let (marker, _) = wrapper(kind)?;
        let words = marker
            .checked_add(2)
            .and_then(|value| value.checked_add(body.len()))
            .ok_or(Error::Budget)?;
        if words.div_ceil(13).checked_mul(13).ok_or(Error::Budget)? > 208 {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Correspondence is regenerated from the immutable resource, never uploaded.
pub(in super::super) struct ObjectEditorCommandPosition {
    pub command: usize,
    pub editor_word: usize,
}

pub(in super::super) struct ObjectEditorFrame {
    pub identity: TeletextObjectIdentity,
    pub object_type: TeletextObjectType,
    pub command_positions: Vec<ObjectEditorCommandPosition>,
    pub document: TeletextTtiDocument,
}

fn body_range(
    page: &EncodedTeletextObjectPage,
    target: TeletextObjectIdentity,
    deadline: Instant,
) -> Result<(TeletextObjectType, Range<usize>)> {
    let mut found = None;
    for (index, position) in page.definition_positions().iter().enumerate() {
        check_deadline(deadline)?;
        if position.target() == target {
            if found.is_some() {
                return Err(Error::Association);
            }
            found = Some((index, position));
        }
    }
    let (index, position) = found.ok_or(Error::Association)?;
    if target.resource() != Some(page.header().identity()) {
        return Err(Error::Association);
    }
    let start = usize::from(position.absolute_index()) + 1;
    let mut end = start;
    for command in page
        .command_positions()
        .iter()
        .filter(|value| value.source().object == Some(target))
    {
        check_deadline(deadline)?;
        if command.first_word() < start || command.first_word() > command.semantic_word() {
            return Err(Error::Association);
        }
        end = end.max(
            command
                .semantic_word()
                .checked_add(1)
                .ok_or(Error::Budget)?,
        );
    }
    if let Some(next) = page.definition_positions().get(index + 1) {
        if end != usize::from(next.absolute_index()) {
            return Err(Error::Association);
        }
    }
    let body_rows = page
        .header()
        .last_packet()
        .get()
        .checked_sub(2)
        .ok_or(Error::Association)?;
    if end > usize::from(body_rows) * 13 {
        return Err(Error::Association);
    }
    Ok((position.object_type(), start..end))
}

fn wrapper(kind: TeletextObjectType) -> Result<(usize, [u32; 13])> {
    let owner = TeletextPageAddress {
        page: TeletextPageNumber::new(100).map_err(|_| Error::Association)?,
        subcode: TeletextSubcode::new(0).map_err(|_| Error::Association)?,
    };
    // This is an explicit disposable alias, not a rewrite of the resource ID.
    let alias = TeletextObjectIdentity::local(owner, ersatzrs_domain::TeletextObjectId::new(0));
    let budget = TeletextObjectCatalogBudget {
        max_objects: 1,
        max_commands: 1,
    };
    let catalog = TeletextEnhancementObjectCatalog::new(
        vec![TeletextEnhancementObjectDefinition {
            identity: alias,
            object_type: kind,
            program: TeletextEnhancementProgram::new(vec![]).map_err(|_| Error::Association)?,
        }],
        budget,
    )
    .map_err(|_| Error::Association)?;
    let allocations =
        allocate_teletext_resource_objects(&catalog, &[], 1).map_err(|_| Error::Association)?;
    let main = TeletextEnhancementProgram::new(vec![TeletextEnhancementCommand::InvokeObject {
        target: alias,
        object_type: kind,
        offset: None,
    }])
    .map_err(|_| Error::Association)?;
    let encoded = compile_teletext_local_program_with_objects(
        owner,
        &main,
        &catalog,
        &allocations,
        TeletextResolvedObjectPageLinks {
            public: None,
            global: None,
        },
        budget,
    )
    .map_err(|_| Error::Association)?;
    let records = project_encoded_teletext_local_objects(&encoded, 1)?;
    let [TeletextEncodedDisplayRecord::Triplets {
        row: 26,
        designation: 0,
        words,
    }] = records.as_slice()
    else {
        return Err(Error::Association);
    };
    let [position] = encoded.definition_positions() else {
        return Err(Error::Association);
    };
    Ok((usize::from(position.absolute_index()), *words))
}

fn native_word(projection: &TeletextEncodedObjectProjection, index: usize) -> Result<u32> {
    projection
        .packets
        .get(2 + index / 13)
        .map(|packet| packet.words[index % 13])
        .ok_or(Error::Association)
}

/// One bounded original object body, with a genuinely encoded Local invocation.
/// Full native resources remain unchanged. Bodies too large for one QTM X26
/// frame fail before body allocation; larger-object continuation interchange
/// remains separate required work, never truncation or a native resource cap.
pub(in super::super) fn build_frame(
    page: &EncodedTeletextAuthoredPage,
    target: TeletextObjectIdentity,
    max_units: usize,
    tti: TeletextTtiBudget,
    deadline: Instant,
) -> Result<ObjectEditorFrame> {
    check_deadline(deadline)?;
    if max_units == 0 || page.data_units().len() > max_units {
        return Err(Error::Budget);
    }
    let EncodedTeletextAuthoredPage::Object(resource) = page else {
        return Err(Error::Association);
    };
    let (object_type, native_body) = body_range(resource, target, deadline)?;
    let (definition_index, skeleton) = wrapper(object_type)?;
    let meaningful_words = definition_index
        .checked_add(2)
        .and_then(|count| count.checked_add(native_body.len()))
        .ok_or(Error::Budget)?;
    let padded = meaningful_words
        .div_ceil(13)
        .checked_mul(13)
        .ok_or(Error::Budget)?;
    if padded > 208 || padded / 13 + 6 > tti.max_records {
        return Err(Error::Budget);
    }
    let projection = project_encoded_teletext_object(page, max_units)?;
    let mut words = Vec::new();
    words.try_reserve_exact(padded).map_err(|_| Error::Budget)?;
    words.extend_from_slice(&skeleton[..=definition_index]);
    for index in native_body.clone() {
        check_deadline(deadline)?;
        words.push(native_word(&projection, index)?);
    }
    let terminator = skeleton[definition_index + 1];
    words.resize(padded, terminator);
    let command_positions = positions(
        resource,
        target,
        &native_body,
        definition_index + 1,
        deadline,
    )?;
    let document = document(&words, tti)?;
    check_deadline(deadline)?;
    Ok(ObjectEditorFrame {
        identity: target,
        object_type,
        command_positions,
        document,
    })
}

fn positions(
    page: &EncodedTeletextObjectPage,
    target: TeletextObjectIdentity,
    body: &Range<usize>,
    editor_start: usize,
    deadline: Instant,
) -> Result<Vec<ObjectEditorCommandPosition>> {
    let count = page
        .command_positions()
        .iter()
        .filter(|value| value.source().object == Some(target))
        .count();
    if count > body.len() {
        return Err(Error::Association);
    }
    let mut result = Vec::new();
    result.try_reserve_exact(count).map_err(|_| Error::Budget)?;
    for position in page
        .command_positions()
        .iter()
        .filter(|value| value.source().object == Some(target))
    {
        check_deadline(deadline)?;
        if !body.contains(&position.first_word())
            || !body.contains(&position.semantic_word())
            || result
                .last()
                .is_some_and(|previous: &ObjectEditorCommandPosition| {
                    previous.command >= position.source().command
                })
        {
            return Err(Error::Association);
        }
        result.push(ObjectEditorCommandPosition {
            command: position.source().command,
            editor_word: editor_start + position.semantic_word() - body.start,
        });
    }
    Ok(result)
}

fn document(words: &[u32], budget: TeletextTtiBudget) -> Result<TeletextTtiDocument> {
    let mut records = Vec::new();
    records
        .try_reserve_exact(6 + words.len() / 13)
        .map_err(|_| Error::Budget)?;
    records.extend([
        TeletextTtiRecord::PageNumber {
            page: 0x100,
            editor_ordinal: 0,
        },
        TeletextTtiRecord::SubpageOrdinal(0),
        TeletextTtiRecord::PageStatus(0xc000),
        TeletextTtiRecord::Region(0),
        TeletextTtiRecord::Cycle {
            value: 20,
            unit: ersatzrs_app::teletext_tti::TeletextTtiCycleUnit::Time,
        },
        TeletextTtiRecord::BaseRow {
            row: 0,
            cells: [b' '; 40],
        },
    ]);
    for (designation, chunk) in words.chunks_exact(13).enumerate() {
        records.push(TeletextTtiRecord::Packet {
            row: 26,
            designation: u8::try_from(designation).map_err(|_| Error::Budget)?,
            words: chunk.try_into().map_err(|_| Error::Association)?,
        });
    }
    Ok(TeletextTtiDocument::checked_from_records(records, budget)?)
}

#[cfg(test)]
mod tests;
