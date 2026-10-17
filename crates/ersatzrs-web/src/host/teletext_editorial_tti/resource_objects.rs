//! Exact original POP/GPOP scalars through receipt-owned editor wrappers.
use super::{
    operands, Location, NativeTeletextEditorialTtiError,
    NativeTeletextEditorialTtiFailure as Failure, Page, Result,
};
use crate::host::teletext_export_project::resource_objects::{
    build_fragment, build_frame, plan_fragments, ObjectEditorFragmentPlan, ObjectEditorFrame,
};
use ersatzrs_addon_contract::TeletextResourceKindWire;
use ersatzrs_app::{
    teletext_export_project::{
        TeletextExportAssetMapping, TeletextExportAssetScope, TeletextExportProject,
    },
    teletext_tti::TeletextTtiBudget,
};
use ersatzrs_domain::{
    TeletextEnhancementCommand, TeletextEnhancementObjectDefinition, TeletextObjectId,
    TeletextObjectIdentity, TeletextResourceHeaderMetadata, TeletextResourceKind,
    TeletextResourceObjectOperandOverlay,
};
use ersatzrs_ffmpeg::teletext::{EncodedTeletextAuthoredPage, EncodedTeletextAuthoredService};
use std::{collections::BTreeSet, time::Instant};

mod context;
mod invocations;

fn invalid() -> NativeTeletextEditorialTtiError {
    Location {
        page: 0,
        subcode: 0,
        resource: None,
    }
    .error(Failure::Mapping)
}
struct Mapping<'a> {
    identity: TeletextObjectIdentity,
    object_type: ersatzrs_domain::TeletextObjectType,
    page: &'a EncodedTeletextAuthoredPage,
    location: Location,
    asset: &'a TeletextExportAssetMapping,
    project: &'a TeletextExportProject,
    definition: &'a TeletextEnhancementObjectDefinition,
    service: &'a EncodedTeletextAuthoredService,
}
fn companion<'a>(
    asset: &'a TeletextExportAssetMapping,
    project: &TeletextExportProject,
) -> Result<(&'a str, u16)> {
    let (resource_name, object_id, profile) = match &asset.scope {
        TeletextExportAssetScope::ObjectTtiV1 {
            resource_name,
            object_id,
            editor_page: 0x100,
            editor_ordinal: 0,
        } => (resource_name, object_id, "qteletextmaker-1.0-carrier-v6"),
        TeletextExportAssetScope::ObjectTtiFragmentV1 {
            resource_name,
            object_id,
            editor_page: 0x100,
            editor_ordinal: 0,
            ..
        } => (resource_name, object_id, "qteletextmaker-1.0-carrier-v7"),
        TeletextExportAssetScope::ObjectContextTtiV1 {
            resource_name,
            object_id,
            editor_page: 0x100,
            editor_ordinal: 0,
        } => (resource_name, object_id, "qteletextmaker-1.0-carrier-v8"),
        _ => return Err(invalid()),
    };
    let exact = project
        .assets
        .iter()
        .filter(|candidate| *candidate == asset)
        .count();
    let targets = project
        .assets
        .iter()
        .filter(|candidate| {
            matches!(&candidate.scope, TeletextExportAssetScope::ObjectTtiV1 {
            resource_name: name, object_id: id, ..
        } | TeletextExportAssetScope::ObjectContextTtiV1 {
            resource_name: name, object_id: id, ..
        } if name == resource_name && id == object_id)
        })
        .count();
    if project.editor_profile != profile
        || asset.unit_count != 0
        || exact != 1
        || (profile != "qteletextmaker-1.0-carrier-v7" && targets != 1)
    {
        return Err(invalid());
    }
    Ok((resource_name, *object_id))
}
fn mapping<'a>(
    asset: &'a TeletextExportAssetMapping,
    project: &'a TeletextExportProject,
    service: &'a EncodedTeletextAuthoredService,
) -> Result<Mapping<'a>> {
    mapping_checked(asset, project, service, None)
}
fn mapping_resource<'a>(
    asset: &'a TeletextExportAssetMapping,
    project: &'a TeletextExportProject,
    service: &'a EncodedTeletextAuthoredService,
    original_header: TeletextResourceHeaderMetadata,
) -> Result<Mapping<'a>> {
    mapping_checked(asset, project, service, Some(original_header))
}
fn matches_header(
    header: TeletextResourceHeaderMetadata,
    original: Option<TeletextResourceHeaderMetadata>,
    physical_subcode: u16,
) -> bool {
    let Some(original) = original else {
        return header.physical_subcode() == physical_subcode;
    };
    // The original receipt remains exact; only canonical candidate body extent
    // may change. Counter, transmission and complete logical identity stay fixed.
    original.physical_subcode() == physical_subcode
        && TeletextResourceHeaderMetadata::new(
            header.identity(),
            original.last_packet(),
            header.update_counter(),
            header.transmission(),
        )
        .is_ok_and(|normalized| normalized == original)
}
fn mapping_checked<'a>(
    asset: &'a TeletextExportAssetMapping,
    project: &'a TeletextExportProject,
    service: &'a EncodedTeletextAuthoredService,
    original_header: Option<TeletextResourceHeaderMetadata>,
) -> Result<Mapping<'a>> {
    let (resource_name, object_id) = companion(asset, project)?;
    let mut raw = project
        .assets
        .iter()
        .filter(|candidate| candidate.name == resource_name);
    let raw = raw
        .next()
        .filter(|_| raw.next().is_none())
        .ok_or_else(invalid)?;
    let TeletextExportAssetScope::Resource {
        identity,
        physical_subcode,
    } = &raw.scope
    else {
        return Err(invalid());
    };
    let mut pages = service.pages().iter().filter(|page| {
        page.resource_header().is_some_and(|header| {
            let native = header.identity();
            matches!(
                (native.kind(), identity.kind),
                (TeletextResourceKind::Pop, TeletextResourceKindWire::Pop)
                    | (TeletextResourceKind::Gpop, TeletextResourceKindWire::Gpop)
            ) && native.page().magazine() == identity.page.magazine
                && native.page().page() == identity.page.page
                && native.logical_subpage().get() == identity.subpage
                && matches_header(header, original_header, *physical_subcode)
        })
    });
    let page = pages
        .next()
        .filter(|_| pages.next().is_none())
        .ok_or_else(invalid)?;
    let resource = page.resource_header().ok_or_else(invalid)?.identity();
    let location = Location {
        page: (u16::from(resource.page().magazine()) << 8) | u16::from(resource.page().page()),
        subcode: u16::from(resource.logical_subpage().get()),
        resource: Some(resource),
    };
    let identity = match resource.kind() {
        TeletextResourceKind::Pop => {
            TeletextObjectIdentity::public(resource, TeletextObjectId::new(object_id))
        }
        TeletextResourceKind::Gpop => {
            TeletextObjectIdentity::global(resource, TeletextObjectId::new(object_id))
        }
        _ => return Err(invalid()),
    }
    .map_err(|_| invalid())?;
    if original_header.is_none()
        && (raw.unit_count != page.data_units().len()
            || raw.unit_count.checked_mul(46) != Some(raw.bytes))
    {
        return Err(location.error(Failure::Mapping));
    }
    let definition = service
        .declared()
        .semantic()
        .objects()
        .definition(identity)
        .ok_or_else(|| location.error(Failure::Mapping))?;
    Ok(Mapping {
        identity,
        object_type: definition.object_type,
        page,
        location,
        asset,
        project,
        definition,
        service,
    })
}
fn frame(
    mapping: &Mapping<'_>,
    limits: (TeletextTtiBudget, usize),
    deadline: Instant,
) -> Result<ObjectEditorFrame> {
    mapping.location.check_deadline(deadline)?;
    if matches!(
        mapping.asset.scope,
        TeletextExportAssetScope::ObjectContextTtiV1 { .. }
    ) {
        return context::frame(mapping, limits, deadline);
    }
    let result = if matches!(
        mapping.asset.scope,
        TeletextExportAssetScope::ObjectTtiFragmentV1 { .. }
    ) {
        let plan = fragment_plan(mapping, limits.1, deadline)?;
        build_fragment(
            mapping.page,
            mapping.definition,
            &plan,
            limits.1,
            limits.0,
            deadline,
        )
    } else {
        build_frame(mapping.page, mapping.identity, limits.1, limits.0, deadline)
    };
    let frame = result.map_err(|error| {
        mapping.location.error(match error {
            crate::host::teletext_export_project::NativeTeletextExportProjectError::TimedOut => {
                Failure::TimedOut
            }
            _ => Failure::Structure,
        })
    })?;
    if frame.identity != mapping.identity || frame.object_type != mapping.object_type {
        return Err(mapping.location.error(Failure::Structure));
    }
    Ok(frame)
}

fn fragment_plan(
    mapping: &Mapping<'_>,
    max_units: usize,
    deadline: Instant,
) -> Result<ObjectEditorFragmentPlan> {
    let plans = plan_fragments(mapping.page, mapping.definition, max_units, deadline)
        .map_err(|error| mapping.location.error(match error {
            crate::host::teletext_export_project::NativeTeletextExportProjectError::TimedOut => Failure::TimedOut,
            _ => Failure::Structure,
        }))?;
    let TeletextExportAssetScope::ObjectTtiFragmentV1 {
        resource_name,
        object_id,
        fragment_index,
        ..
    } = &mapping.asset.scope
    else {
        return Err(invalid());
    };
    let mut matched = BTreeSet::new();
    for asset in &mapping.project.assets {
        mapping.location.check_deadline(deadline)?;
        match &asset.scope {
            TeletextExportAssetScope::ObjectTtiV1 {
                resource_name: name,
                object_id: id,
                ..
            } if name == resource_name && id == object_id => return Err(invalid()),
            TeletextExportAssetScope::ObjectTtiFragmentV1 {
                resource_name: name,
                object_id: id,
                fragment_index: index,
                ..
            } if name == resource_name && id == object_id => {
                let plan = plans.get(usize::from(*index)).ok_or_else(invalid)?;
                if usize::from(*index) != matched.len()
                    || !matched.insert(*index)
                    || asset.unit_count != 0
                    || !matches_plan(&asset.scope, plan)
                {
                    return Err(invalid());
                }
            }
            _ => {}
        }
    }
    if matched.len() != plans.len() {
        return Err(invalid());
    }
    plans
        .into_iter()
        .nth(usize::from(*fragment_index))
        .ok_or_else(invalid)
}

fn matches_plan(scope: &TeletextExportAssetScope, plan: &ObjectEditorFragmentPlan) -> bool {
    matches!(scope, TeletextExportAssetScope::ObjectTtiFragmentV1 {
        fragment_index, fragment_count, command_start, command_end, native_word_start, native_word_end,
        editor_page: 0x100, editor_ordinal: 0, ..
    } if *fragment_index == plan.fragment_index && *fragment_count == plan.fragment_count
        && *command_start == plan.command_start
        && *command_end == plan.command_end
        && *native_word_start == plan.native_word_start
        && *native_word_end == plan.native_word_end)
}
fn word_error(location: Location, word: usize) -> NativeTeletextEditorialTtiError {
    NativeTeletextEditorialTtiError {
        row: Some(26),
        designation: u8::try_from(word / 13).ok(),
        column: Some(word % 13),
        ..location.error(Failure::Packet)
    }
}
fn compare(
    before: &Page<'_>,
    after: &Page<'_>,
    location: Location,
    allow_operands: bool,
    deadline: Instant,
) -> Result<()> {
    for (same, failure) in [
        (before.status == after.status, Failure::Status),
        (before.region == after.region, Failure::Region),
        (before.links == after.links, Failure::Navigation),
        (
            before.description == after.description,
            Failure::Description,
        ),
        (before.cycle == after.cycle, Failure::Cycle),
    ] {
        if !same {
            return Err(location.error(failure));
        }
    }
    for row in 0..26 {
        location.check_deadline(deadline)?;
        if before.rows[row] != after.rows[row] {
            return Err(NativeTeletextEditorialTtiError {
                row: u8::try_from(row).ok(),
                column: before.rows[row]
                    .zip(after.rows[row])
                    .and_then(|(a, b)| a.iter().zip(b).position(|(a, b)| a != b)),
                ..location.error(Failure::ProtectedRow)
            });
        }
    }
    for row in 0..4 {
        for designation in 0..16 {
            location.check_deadline(deadline)?;
            let old = before.packets[row][designation];
            let new = after.packets[row][designation];
            if old != new && !(allow_operands && row == 0 && old.is_some() && new.is_some()) {
                return Err(NativeTeletextEditorialTtiError {
                    row: u8::try_from(row + 26).ok(),
                    designation: u8::try_from(designation).ok(),
                    column: old
                        .zip(new)
                        .and_then(|(a, b)| a.iter().zip(b).position(|(a, b)| a != b)),
                    ..location.error(Failure::Packet)
                });
            }
        }
    }
    Ok(())
}

/// Uploaded words may change only authenticated original semantic `AtColumn` words.
/// Wrapper invocations, definition markers, hidden commands and prefixes stay exact.
pub(in crate::host) fn reconcile(
    asset: &TeletextExportAssetMapping,
    project: &TeletextExportProject,
    service: &EncodedTeletextAuthoredService,
    baseline_bytes: &[u8],
    edited_bytes: &[u8],
    limits: (TeletextTtiBudget, usize),
    deadline: Instant,
) -> Result<Vec<TeletextResourceObjectOperandOverlay>> {
    let mapping = mapping(asset, project, service)?;
    let frame = frame(&mapping, limits, deadline)?;
    let location = mapping.location;
    let canonical = Page::index_identity(&frame.document, 0x100, 0, location, deadline)?;
    let before = location.parse(baseline_bytes, limits.0, deadline)?;
    let after = location.parse(edited_bytes, limits.0, deadline)?;
    let before = Page::index_identity(&before, 0x100, 0, location, deadline)?;
    let after = Page::index_identity(&after, 0x100, 0, location, deadline)?;
    compare(&canonical, &before, location, false, deadline)?;
    compare(&before, &after, location, true, deadline)?;
    let definition = service
        .declared()
        .semantic()
        .objects()
        .definition(mapping.identity)
        .ok_or_else(|| location.error(Failure::Mapping))?;
    let mut changes = Vec::new();
    for designation in 0..16 {
        let (Some(before), Some(after)) = (
            before.packets[0][designation],
            after.packets[0][designation],
        ) else {
            continue;
        };
        for slot in 0..13 {
            location.check_deadline(deadline)?;
            if before[slot] == after[slot] {
                continue;
            }
            let word = designation * 13 + slot;
            let mut positions = frame
                .command_positions
                .iter()
                .filter(|position| position.editor_word == word);
            let position = positions
                .next()
                .filter(|_| positions.next().is_none())
                .ok_or_else(|| word_error(location, word))?;
            let Some(TeletextEnhancementCommand::AtColumn { column, operation }) =
                definition.program.commands().get(position.command)
            else {
                return Err(word_error(location, word));
            };
            let replacement = operands::operand(*column, *operation, before[slot], after[slot])
                .ok_or_else(|| word_error(location, word))?;
            changes.push(
                TeletextResourceObjectOperandOverlay::new(
                    mapping.identity,
                    position.command,
                    *column,
                    *operation,
                    replacement,
                )
                .map_err(|_| word_error(location, word))?,
            );
        }
    }
    Ok(changes)
}

/// Confirm receipt capability and exact original semantic-word correspondence
/// before a host applies typed changes. This grants no shared-region permission.
pub(in crate::host) fn authorize(
    project: &TeletextExportProject,
    service: &EncodedTeletextAuthoredService,
    changes: &[TeletextResourceObjectOperandOverlay],
    limits: (TeletextTtiBudget, usize),
    deadline: Instant,
) -> Result<()> {
    Location {
        page: 0,
        subcode: 0,
        resource: None,
    }
    .check_deadline(deadline)?;
    if changes.is_empty() {
        return Ok(());
    }
    if !matches!(
        project.editor_profile.as_str(),
        "qteletextmaker-1.0-carrier-v6"
            | "qteletextmaker-1.0-carrier-v7"
            | "qteletextmaker-1.0-carrier-v8"
    ) || project
        .assets
        .len()
        .checked_mul(208)
        .is_none_or(|maximum| changes.len() > maximum)
    {
        return Err(invalid());
    }
    let mut targets = BTreeSet::new();
    let mut matched = 0usize;
    for asset in &project.assets {
        if !matches!(
            asset.scope,
            TeletextExportAssetScope::ObjectTtiV1 { .. }
                | TeletextExportAssetScope::ObjectTtiFragmentV1 { .. }
                | TeletextExportAssetScope::ObjectContextTtiV1 { .. }
        ) {
            continue;
        }
        let mapping = mapping(asset, project, service)?;
        let selected = || {
            changes
                .iter()
                .filter(|change| change.identity() == mapping.identity)
        };
        if selected().next().is_none() {
            continue;
        }
        let frame = frame(&mapping, limits, deadline)?;
        let definition = service
            .declared()
            .semantic()
            .objects()
            .definition(mapping.identity)
            .ok_or_else(invalid)?;
        for change in selected().filter(|change| owns_command(&asset.scope, change.command())) {
            mapping.location.check_deadline(deadline)?;
            if !targets.insert((change.identity(), change.command()))
                || definition.program.commands().get(change.command())
                    != Some(&TeletextEnhancementCommand::AtColumn {
                        column: change.column(),
                        operation: change.expected(),
                    })
                || frame
                    .command_positions
                    .iter()
                    .filter(|position| position.command == change.command())
                    .count()
                    != 1
            {
                return Err(mapping.location.error(Failure::Packet));
            }
            matched += 1;
        }
    }
    if matched != changes.len() {
        return Err(invalid());
    }
    Ok(())
}

fn owns_command(scope: &TeletextExportAssetScope, command: usize) -> bool {
    match scope {
        TeletextExportAssetScope::ObjectTtiV1 { .. }
        | TeletextExportAssetScope::ObjectContextTtiV1 { .. } => true,
        TeletextExportAssetScope::ObjectTtiFragmentV1 {
            command_start,
            command_end,
            ..
        } => u64::try_from(command)
            .is_ok_and(|command| (*command_start..*command_end).contains(&command)),
        _ => false,
    }
}

/// The applied candidate must re-encode to every edited record, including all
/// protected wrapper/prefix/padding words; scalar interpretation alone is insufficient.
pub(in crate::host) fn validate_applied(
    asset: &TeletextExportAssetMapping,
    project: &TeletextExportProject,
    candidate: &EncodedTeletextAuthoredService,
    edited_bytes: &[u8],
    limits: (TeletextTtiBudget, usize),
    deadline: Instant,
) -> Result<()> {
    let mapping = mapping(asset, project, candidate)?;
    let frame = frame(&mapping, limits, deadline)?;
    let expected = Page::index_identity(&frame.document, 0x100, 0, mapping.location, deadline)?;
    let edited = mapping.location.parse(edited_bytes, limits.0, deadline)?;
    let edited = Page::index_identity(&edited, 0x100, 0, mapping.location, deadline)?;
    compare(&expected, &edited, mapping.location, false, deadline)
}

mod project;
pub(in crate::host) use project::{
    authorize_programs, reconcile_project, validate_applied_project, ResourceEdits,
};

#[cfg(test)]
mod tests;
