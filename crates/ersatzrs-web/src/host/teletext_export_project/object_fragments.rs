//! Versioned per-fragment files; immutable resources remain complete and unchanged.
use super::{
    check_deadline, resource_objects, sha256, write_teletext_tti, EncodedTeletextAuthoredPage,
    EncodedTeletextAuthoredService, Files, Instant, NativeTeletextExportProjectError as Error,
    Result, TeletextExportAssetMapping, TeletextExportAssetScope, TeletextTtiBudget,
};

/// Count complete fragment membership before allocating any editor file body.
pub(super) fn count(
    encoded: &EncodedTeletextAuthoredService,
    max_units: usize,
    deadline: Instant,
) -> Result<usize> {
    let catalog = encoded.declared().semantic().objects();
    let mut total = 0usize;
    for page in encoded.pages() {
        check_deadline(deadline)?;
        let EncodedTeletextAuthoredPage::Object(object) = page else {
            continue;
        };
        for position in object.definition_positions() {
            check_deadline(deadline)?;
            let definition = catalog
                .definition(position.target())
                .ok_or(Error::Association)?;
            total = total
                .checked_add(
                    resource_objects::plan_fragments(page, definition, max_units, deadline)?.len(),
                )
                .ok_or(Error::Budget)?;
        }
    }
    Ok(total)
}

/// Append one frame at a time under the current remaining aggregate file/byte quota.
pub(super) fn append(
    encoded: &EncodedTeletextAuthoredService,
    page: &EncodedTeletextAuthoredPage,
    resource_name: &str,
    files: &mut Files,
    assets: &mut Vec<TeletextExportAssetMapping>,
    tti: TeletextTtiBudget,
    deadline: Instant,
) -> Result<()> {
    let EncodedTeletextAuthoredPage::Object(object) = page else {
        return Ok(());
    };
    let catalog = encoded.declared().semantic().objects();
    for position in object.definition_positions() {
        check_deadline(deadline)?;
        let definition = catalog
            .definition(position.target())
            .ok_or(Error::Association)?;
        let plans =
            resource_objects::plan_fragments(page, definition, files.budget.max_units, deadline)?;
        if files
            .values
            .len()
            .checked_add(plans.len())
            .is_none_or(|count| count > files.budget.max_files)
        {
            return Err(Error::Budget);
        }
        for plan in plans {
            check_deadline(deadline)?;
            let budget = TeletextTtiBudget {
                max_bytes: tti
                    .max_bytes
                    .min(files.budget.max_file_bytes)
                    .min(files.remaining),
                max_records: tti.max_records.min(files.budget.max_units),
            };
            let frame = resource_objects::build_fragment(
                page,
                definition,
                &plan,
                files.budget.max_units,
                budget,
                deadline,
            )?;
            let bytes = write_teletext_tti(&frame.document, budget)?;
            let object_id = position.target().object_id().get();
            let name = format!(
                "{}-object-{object_id:05}-fragment-{:03}.tti",
                resource_name.trim_end_matches(".dvb"),
                plan.fragment_index
            );
            let mapping = TeletextExportAssetMapping {
                name: name.clone(),
                scope: TeletextExportAssetScope::ObjectTtiFragmentV1 {
                    resource_name: resource_name.to_owned(),
                    object_id,
                    fragment_index: plan.fragment_index,
                    fragment_count: plan.fragment_count,
                    command_start: plan.command_start,
                    command_end: plan.command_end,
                    native_word_start: plan.native_word_start,
                    native_word_end: plan.native_word_end,
                    editor_page: 0x100,
                    editor_ordinal: 0,
                },
                unit_count: 0,
                bytes: bytes.len(),
                sha256: sha256(&bytes),
            };
            files.append(name, bytes)?;
            assets.push(mapping);
        }
    }
    Ok(())
}
