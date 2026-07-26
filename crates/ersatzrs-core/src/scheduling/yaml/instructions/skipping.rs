use super::super::{eval_count_expression, Datelike, YamlContext, YamlInstruction};

pub(super) fn handle_skip_items(context: &mut YamlContext, instruction: &YamlInstruction) {
    let Some(content) = instruction.content.as_deref() else {
        return;
    };
    let enumerator_len = context.content_len(content);
    if enumerator_len == 0 {
        return;
    }
    let seed = context
        .playout
        .id
        .cast_unsigned()
        .wrapping_add(context.instruction_index as u64)
        .wrapping_add(u64::from(context.current_time.ordinal()));
    let skip = instruction
        .skip_items
        .as_deref()
        .map(|expr| eval_count_expression(expr, enumerator_len, seed) % enumerator_len)
        .unwrap_or_default();
    context.skip_content(content, skip);
}

pub(super) fn handle_skip_to_item(context: &mut YamlContext, instruction: &YamlInstruction) {
    let Some(content) = instruction.content.as_deref() else {
        return;
    };
    let season = instruction.season.unwrap_or_default();
    let episode = instruction.episode.unwrap_or_default();
    let enumerator = context.enumerator(content);
    for _ in 0..enumerator.len() {
        if enumerator
            .current()
            .is_some_and(|item| item.season == Some(season) && item.episode == Some(episode))
        {
            break;
        }
        enumerator.advance();
    }
}
