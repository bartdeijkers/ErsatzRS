use super::super::{SchedulingError, YamlContext, YamlInstruction};

pub(super) fn handle_epg_group(context: &mut YamlContext, instruction: &YamlInstruction) {
    if instruction.epg_group.unwrap_or(false) {
        if instruction.advance.unwrap_or(true) {
            context.advance_guide_group();
        }
        context.guide_group_locked = true;
    } else {
        context.guide_group_locked = false;
    }
}

pub(super) fn handle_watermark(context: &mut YamlContext, instruction: &YamlInstruction) {
    if instruction.watermark.unwrap_or(false) {
        if let Some(name) = &instruction.name {
            if let Some(id) = context.reference_data.watermark_ids_by_name.get(name) {
                context.watermark_ids.insert(*id);
            }
        }
    } else if let Some(name) = &instruction.name {
        if let Some(id) = context.reference_data.watermark_ids_by_name.get(name) {
            context.watermark_ids.remove(id);
        }
    } else {
        context.watermark_ids.clear();
    }
}

pub(super) fn handle_graphics_on(
    context: &mut YamlContext,
    instruction: &YamlInstruction,
) -> Result<(), SchedulingError> {
    if let Some(path) = &instruction.graphics_on {
        if let Some(id) = context.reference_data.graphics_ids_by_path.get(path) {
            let variables = if instruction.variables.is_empty() {
                None
            } else {
                Some(
                    serde_json::to_string(&instruction.variables)
                        .map_err(|error| SchedulingError::InvalidYaml(error.to_string()))?,
                )
            };
            context.graphics_elements.insert(*id, variables);
        }
    }
    Ok(())
}

pub(super) fn handle_graphics_off(context: &mut YamlContext, instruction: &YamlInstruction) {
    if let Some(path) = &instruction.graphics_off {
        if path.trim().is_empty() {
            context.graphics_elements.clear();
        } else if let Some(id) = context.reference_data.graphics_ids_by_path.get(path) {
            context.graphics_elements.remove(id);
        }
    }
}

pub(super) fn enabled_sequence(enabled: Option<bool>, sequence: Option<&String>) -> Option<String> {
    if enabled.unwrap_or(false) {
        sequence.filter(|value| !value.trim().is_empty()).cloned()
    } else {
        None
    }
}
