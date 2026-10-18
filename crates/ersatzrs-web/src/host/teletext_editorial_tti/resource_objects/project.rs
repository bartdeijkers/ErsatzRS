//! Complete original resource groups, joined before semantic reconstruction.
use super::super::structural::resource as structural;
use super::PositionScope;
use super::{
    authorize, compare, frame, invalid, mapping, mapping_resource, plan_fragments, reconcile,
    EncodedTeletextAuthoredPage, EncodedTeletextAuthoredService, Failure, Instant, Location,
    Mapping, Page, Result, TeletextExportAssetMapping, TeletextExportAssetScope,
    TeletextExportProject, TeletextResourceObjectOperandOverlay, TeletextTtiBudget,
};
use ersatzrs_app::teletext_export_project::TeletextExportFile;
use ersatzrs_domain::{
    TeletextEnhancementCommand, TeletextResourceObjectProgramOverlay, TeletextServiceBudget,
};
use ersatzrs_ffmpeg::teletext::project_encoded_teletext_object;
use std::collections::{BTreeMap, BTreeSet};

pub(super) type Limits = (TeletextTtiBudget, usize, TeletextServiceBudget);
type Files<'a> = &'a [TeletextExportFile];

pub(in crate::host) struct ResourceEdits {
    pub operands: Vec<TeletextResourceObjectOperandOverlay>,
    pub programs: Vec<TeletextResourceObjectProgramOverlay>,
}
pub(super) struct Body {
    pub(super) before: Vec<u32>,
    pub(super) after: Vec<u32>,
    pub(super) spans: Vec<structural::Span>,
    pub(super) context_calls: Option<Vec<super::topology::Call>>,
}
fn groups<'a>(
    project: &'a TeletextExportProject,
    service: &'a EncodedTeletextAuthoredService,
    limits: Limits,
) -> Result<Vec<Vec<&'a TeletextExportAssetMapping>>> {
    if project
        .assets
        .len()
        .checked_add(project.pages.len())
        .is_none_or(|count| count > limits.2.max_structure_items)
    {
        return Err(invalid());
    }
    let mut groups = BTreeMap::<_, Vec<_>>::new();
    for asset in &project.assets {
        if matches!(
            asset.scope,
            TeletextExportAssetScope::ObjectTtiV1 { .. }
                | TeletextExportAssetScope::ObjectTtiFragmentV1 { .. }
                | TeletextExportAssetScope::ObjectContextTtiV1 { .. }
        ) {
            let mapped = mapping(asset, project, service)?;
            groups.entry(mapped.identity).or_default().push(asset);
        }
    }
    Ok(groups.into_values().collect())
}
fn inventory(project: &TeletextExportProject, files: Files<'_>) -> Result<()> {
    if project.pages.len().checked_add(project.assets.len()) != Some(files.len()) {
        return Err(invalid());
    }
    let mut names = BTreeSet::new();
    for file in files {
        if !names.insert(&file.name)
            || project
                .pages
                .iter()
                .filter(|page| page.name == file.name)
                .count()
                + project
                    .assets
                    .iter()
                    .filter(|asset| asset.name == file.name)
                    .count()
                != 1
        {
            return Err(invalid());
        }
    }
    Ok(())
}
fn file<'a>(files: Files<'a>, asset: &TeletextExportAssetMapping) -> Result<&'a [u8]> {
    let mut values = files.iter().filter(|file| file.name == asset.name);
    let value = values
        .next()
        .filter(|_| values.next().is_none())
        .ok_or_else(invalid)?;
    Ok(&value.bytes)
}
pub(super) fn without_x26<'a>(page: &Page<'a>) -> Page<'a> {
    let mut packets = page.packets;
    packets[0] = [None; 16];
    Page {
        description: page.description,
        status: page.status,
        region: page.region,
        cycle: page.cycle,
        links: page.links,
        rows: page.rows,
        packets,
    }
}
fn append(target: &mut Vec<u32>, words: &[u32], maximum: usize) -> Result<()> {
    if target
        .len()
        .checked_add(words.len())
        .is_none_or(|count| count > maximum)
    {
        return Err(invalid());
    }
    target
        .try_reserve_exact(words.len())
        .map_err(|_| invalid())?;
    target.extend_from_slice(words);
    Ok(())
}
fn member(
    mapped: &Mapping<'_>,
    before: Option<Files<'_>>,
    after: Files<'_>,
    limits: Limits,
    deadline: Instant,
) -> Result<(Vec<u32>, Vec<u32>)> {
    let frame = frame(mapped, (limits.0, limits.1), deadline)?;
    let canonical = Page::index_identity(&frame.document, 256, 0, mapped.location, deadline)?;
    if let Some(files) = before {
        let document = mapped
            .location
            .parse(file(files, mapped.asset)?, limits.0, deadline)?;
        let baseline = Page::index_identity(&document, 256, 0, mapped.location, deadline)?;
        compare(&canonical, &baseline, mapped.location, false, deadline)?;
    }
    let document = mapped
        .location
        .parse(file(after, mapped.asset)?, limits.0, deadline)?;
    let edited = Page::index_identity(&document, 256, 0, mapped.location, deadline)?;
    compare(
        &without_x26(&canonical),
        &without_x26(&edited),
        mapped.location,
        false,
        deadline,
    )?;
    let (prefix, original) = structural::image(&canonical, mapped.location, deadline)?;
    let (actual_prefix, actual) = structural::image(&edited, mapped.location, deadline)?;
    if prefix != actual_prefix {
        return Err(mapped.location.error(Failure::Structure));
    }
    Ok((original, actual))
}
fn body(
    group: &[&TeletextExportAssetMapping],
    project: &TeletextExportProject,
    service: &EncodedTeletextAuthoredService,
    files: (Option<Files<'_>>, Files<'_>),
    limits: Limits,
    deadline: Instant,
) -> Result<Body> {
    let first = group.first().ok_or_else(invalid)?;
    let mapped = mapping(first, project, service)?;
    if matches!(
        first.scope,
        TeletextExportAssetScope::ObjectContextTtiV1 { .. }
    ) {
        if group.len() != 1 {
            return Err(invalid());
        }
        return super::context::body(
            &mapped,
            files.0.map(|files| file(files, first)).transpose()?,
            file(files.1, first)?,
            limits,
            deadline,
        );
    }
    let mut output = Body {
        before: vec![],
        after: vec![],
        spans: vec![],
        context_calls: None,
    };
    let maximum = limits
        .2
        .max_commands
        .checked_mul(2)
        .ok_or_else(invalid)?
        .min(507);
    for asset in group {
        let mapped = mapping(asset, project, service)?;
        let (before, after) = member(&mapped, files.0, files.1, limits, deadline)?;
        append(&mut output.before, &before, maximum)?;
        append(&mut output.after, &after, maximum)?;
    }
    output.spans = native_spans(&mapped, limits, deadline)?;
    Ok(output)
}
pub(super) fn native_spans(
    mapped: &Mapping<'_>,
    limits: Limits,
    deadline: Instant,
) -> Result<Vec<structural::Span>> {
    let plans = plan_fragments(mapped.page, mapped.definition, limits.1, deadline)
        .map_err(|_| invalid())?;
    let start = usize::from(plans.first().ok_or_else(invalid)?.native_word_start);
    let EncodedTeletextAuthoredPage::Object(page) = mapped.page else {
        return Err(invalid());
    };
    let mut spans = Vec::new();
    for position in page
        .command_positions()
        .iter()
        .filter(|position| position.source().object == Some(mapped.identity))
    {
        mapped.location.check_deadline(deadline)?;
        if spans.len() >= limits.2.max_commands {
            return Err(invalid());
        }
        spans.push(structural::Span {
            ordinal: position.source().command,
            first: position
                .first_word()
                .checked_sub(start)
                .ok_or_else(invalid)?,
            word: position
                .semantic_word()
                .checked_sub(start)
                .ok_or_else(invalid)?,
        });
    }
    Ok(spans)
}
fn program(
    mapped: &Mapping<'_>,
    body: &Body,
    original: &EncodedTeletextAuthoredService,
    limits: Limits,
    deadline: Instant,
) -> Result<TeletextResourceObjectProgramOverlay> {
    let legacy = (|| {
        let rebuilt = structural::parse(
            mapped.definition,
            &body.spans,
            (&body.before, &body.after),
            limits.2.max_commands,
            mapped.location,
            deadline,
        )?;
        TeletextResourceObjectProgramOverlay::new(
            mapped.identity,
            mapped.definition.program.clone(),
            rebuilt.replacement,
            rebuilt.retained,
            limits.2,
        )
        .map_err(|_| mapped.location.error(Failure::Structure))
    })();
    legacy
        .or_else(|_| super::invocations::program(mapped, body, original, limits, deadline))
        .or_else(|_| super::topology::program(mapped, body, limits, deadline))
}

fn program_with_positions(
    mapped: &Mapping<'_>,
    body: &Body,
    original: &EncodedTeletextAuthoredService,
    scope: PositionScope<'_>,
    limits: Limits,
    deadline: Instant,
) -> Result<TeletextResourceObjectProgramOverlay> {
    if let Some(restoration) = scope.restore(mapped, body, limits, deadline)? {
        return Ok(restoration);
    }
    let added = scope
        .additions
        .iter()
        .find(|added| added.identity() == mapped.identity);
    if added.is_some_and(|added| added.definition() != mapped.definition) {
        return Err(invalid());
    }
    let rebuilt = if mapped.definition.program.commands().is_empty() {
        structural::parse_first_body(
            mapped.definition,
            &body.spans,
            (&body.before, &body.after),
            limits.2.max_commands,
            mapped.location,
            deadline,
        )?
    } else if let Some(appended) = structural::parse_appended_body(
        mapped.definition,
        &body.spans,
        (&body.before, &body.after),
        limits.2.max_commands,
        mapped.location,
        deadline,
    )? {
        appended
    } else if let Some(inserted) = structural::parse_inserted_rows(
        mapped.definition,
        &body.spans,
        (&body.before, &body.after),
        limits.2.max_commands,
        mapped.location,
        deadline,
    )? {
        inserted
    } else {
        return program(mapped, body, original, limits, deadline);
    };
    let constructor = if rebuilt
        .replacement
        .commands()
        .iter()
        .zip(&rebuilt.retained)
        .any(|(command, original)| {
            original.is_none() && matches!(command, TeletextEnhancementCommand::SetPosition { .. })
        }) {
        if added.is_some() {
            TeletextResourceObjectProgramOverlay::new_with_inserted_positions
        } else {
            TeletextResourceObjectProgramOverlay::new_with_original_positions
        }
    } else {
        TeletextResourceObjectProgramOverlay::new
    };
    constructor(
        mapped.identity,
        mapped.definition.program.clone(),
        rebuilt.replacement,
        rebuilt.retained,
        limits.2,
    )
    .map_err(|_| mapped.location.error(Failure::Structure))
}

fn scalar_group(
    group: &[&TeletextExportAssetMapping],
    project: &TeletextExportProject,
    original: &EncodedTeletextAuthoredService,
    files: (Option<Files<'_>>, Files<'_>),
    limits: Limits,
    deadline: Instant,
) -> Result<Vec<TeletextResourceObjectOperandOverlay>> {
    let mut output = Vec::new();
    for asset in group {
        let canonical;
        let before = if let Some(files) = files.0 {
            file(files, asset)?
        } else {
            let frame = frame(
                &mapping(asset, project, original)?,
                (limits.0, limits.1),
                deadline,
            )?;
            canonical = ersatzrs_app::teletext_tti::write_teletext_tti(&frame.document, limits.0)
                .map_err(|_| invalid())?;
            &canonical
        };
        output.extend(reconcile(
            asset,
            project,
            original,
            before,
            file(files.1, asset)?,
            (limits.0, limits.1),
            deadline,
        )?);
    }
    Ok(output)
}

pub(in crate::host) fn reconcile_project(
    project: &TeletextExportProject,
    original: &EncodedTeletextAuthoredService,
    before_files: Files<'_>,
    after_files: Files<'_>,
    limits: Limits,
    deadline: Instant,
) -> Result<ResourceEdits> {
    reconcile_project_with_position_scope(
        project,
        original,
        before_files,
        after_files,
        PositionScope::default(),
        limits,
        deadline,
    )
}

/// Added identities come from the authenticated saved overlay, never the upload.
pub(in crate::host) fn reconcile_project_with_position_scope(
    project: &TeletextExportProject,
    original: &EncodedTeletextAuthoredService,
    before_files: Files<'_>,
    after_files: Files<'_>,
    scope: PositionScope<'_>,
    limits: Limits,
    deadline: Instant,
) -> Result<ResourceEdits> {
    Location {
        page: 0,
        subcode: 0,
        resource: None,
    }
    .check_deadline(deadline)?;
    scope.check(limits)?;
    let groups = groups(project, original, limits)?;
    inventory(project, before_files)?;
    inventory(project, after_files)?;
    let mut result = ResourceEdits {
        operands: vec![],
        programs: vec![],
    };
    for group in groups {
        let body = body(
            &group,
            project,
            original,
            (Some(before_files), after_files),
            limits,
            deadline,
        )?;
        if body.before == body.after {
            continue;
        }
        let scalar = scalar_group(
            &group,
            project,
            original,
            (Some(before_files), after_files),
            limits,
            deadline,
        );
        if let Ok(scalar) = scalar {
            result.operands.extend(scalar);
        } else {
            let mapped = mapping(group[0], project, original)?;
            result.programs.push(program_with_positions(
                &mapped, &body, original, scope, limits, deadline,
            )?);
        }
    }
    Ok(result)
}

pub(in crate::host) fn authorize_programs(
    project: &TeletextExportProject,
    original: &EncodedTeletextAuthoredService,
    operands: &[TeletextResourceObjectOperandOverlay],
    programs: &[TeletextResourceObjectProgramOverlay],
    limits: Limits,
    deadline: Instant,
) -> Result<()> {
    Location {
        page: 0,
        subcode: 0,
        resource: None,
    }
    .check_deadline(deadline)?;
    authorize(project, original, operands, (limits.0, limits.1), deadline)?;
    let groups = groups(project, original, limits)?;
    let mut seen = BTreeSet::new();
    if programs.len() > limits.2.max_objects {
        return Err(invalid());
    }
    for program in programs {
        let group = groups
            .iter()
            .find(|group| {
                mapping(group[0], project, original)
                    .is_ok_and(|mapped| mapped.identity == program.identity())
            })
            .ok_or_else(invalid)?;
        let mapped = mapping(group[0], project, original)?;
        let mut expected = mapped.definition.program.commands().to_vec();
        for operand in operands
            .iter()
            .filter(|operand| operand.identity() == program.identity())
        {
            let command = expected.get_mut(operand.command()).ok_or_else(invalid)?;
            *command = TeletextEnhancementCommand::AtColumn {
                column: operand.column(),
                operation: operand.replacement(),
            };
        }
        if program.is_position_restoration()
            || (program.has_inserted_positions() && !program.is_original_position_intent())
            || !seen.insert(program.identity())
            || program.expected().commands() != expected
            || program.replacement().commands().len() > limits.2.max_commands
        {
            return Err(invalid());
        }
        for asset in group {
            frame(
                &mapping(asset, project, original)?,
                (limits.0, limits.1),
                deadline,
            )?;
        }
    }
    Ok(())
}
pub(super) fn native_body(
    mapped: &Mapping<'_>,
    limits: Limits,
    deadline: Instant,
) -> Result<Vec<u32>> {
    let plans = plan_fragments(mapped.page, mapped.definition, limits.1, deadline)
        .map_err(|_| invalid())?;
    let start = plans.first().ok_or_else(invalid)?.native_word_start;
    let end = plans.last().ok_or_else(invalid)?.native_word_end;
    let projection =
        project_encoded_teletext_object(mapped.page, limits.1).map_err(|_| invalid())?;
    (usize::from(start)..usize::from(end))
        .map(|index| {
            projection
                .packets
                .get(2 + index / 13)
                .map(|packet| packet.words[index % 13])
                .ok_or_else(invalid)
        })
        .collect()
}
pub(in crate::host) fn validate_applied_project(
    project: &TeletextExportProject,
    original: &EncodedTeletextAuthoredService,
    candidate: &EncodedTeletextAuthoredService,
    edited_files: Files<'_>,
    limits: Limits,
    deadline: Instant,
) -> Result<()> {
    validate_applied_project_with_position_scope(
        project,
        original,
        candidate,
        edited_files,
        PositionScope::default(),
        limits,
        deadline,
    )
}

pub(in crate::host) fn validate_applied_project_with_position_scope(
    project: &TeletextExportProject,
    original: &EncodedTeletextAuthoredService,
    candidate: &EncodedTeletextAuthoredService,
    edited_files: Files<'_>,
    scope: PositionScope<'_>,
    limits: Limits,
    deadline: Instant,
) -> Result<()> {
    Location {
        page: 0,
        subcode: 0,
        resource: None,
    }
    .check_deadline(deadline)?;
    scope.check(limits)?;
    let groups = groups(project, original, limits)?;
    inventory(project, edited_files)?;
    for group in groups {
        let body = body(
            &group,
            project,
            original,
            (None, edited_files),
            limits,
            deadline,
        )?;
        let before = mapping(group[0], project, original)?;
        if body.before != body.after
            && scalar_group(
                &group,
                project,
                original,
                (None, edited_files),
                limits,
                deadline,
            )
            .is_err()
        {
            let _ = program_with_positions(&before, &body, original, scope, limits, deadline)?;
        }
        let after = mapping_resource(
            group[0],
            project,
            candidate,
            before.page.resource_header().ok_or_else(invalid)?,
        )?;
        if before.identity != after.identity
            || before.object_type != after.object_type
            || !structural::equivalent(
                &body.after,
                &native_body(&after, limits, deadline)?,
                limits.2.max_commands.checked_mul(2).ok_or_else(invalid)?,
                before.location,
                deadline,
            )?
        {
            return Err(invalid());
        }
        if matches!(
            group[0].scope,
            TeletextExportAssetScope::ObjectContextTtiV1 { .. }
        ) {
            super::context::validate_applied(
                &before,
                &after,
                file(edited_files, group[0])?,
                limits,
                deadline,
            )?;
        }
    }
    Ok(())
}
