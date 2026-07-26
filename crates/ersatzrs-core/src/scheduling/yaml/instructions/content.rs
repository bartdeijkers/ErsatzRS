use super::super::{
    eval_count_expression, history, history_details_for_media_item, playout_item,
    CurrentHistoryContext, Datelike, Duration, FillerKind, PlaybackOrder, PlayoutItem,
    SchedulingError, SchedulingMediaItem, YamlContext, YamlInstruction, YamlPlayoutDefinition,
};
use super::execute_sequence;
use crate::scheduling::{
    is_randomized_playback_order, SchedulingSmartShuffleRecordContext, SchedulingSmartShuffleUnit,
};

pub(super) fn handle_all(
    context: &mut YamlContext,
    instruction: &YamlInstruction,
    definition: &YamlPlayoutDefinition,
) -> Result<(), SchedulingError> {
    let Some(content) = instruction
        .content
        .as_deref()
        .or(instruction.all.as_deref())
    else {
        return Ok(());
    };
    let count = context.content_len(content);
    for _ in 0..count {
        schedule_next_with_rolls(context, instruction, definition, content)?;
    }
    Ok(())
}

pub(super) fn handle_count(
    context: &mut YamlContext,
    instruction: &YamlInstruction,
    definition: &YamlPlayoutDefinition,
) -> Result<(), SchedulingError> {
    let Some(content) = instruction.content.as_deref() else {
        return Ok(());
    };
    let enumerator_len = context.count_expression_len(content);
    let seed = context
        .playout
        .id
        .cast_unsigned()
        .wrapping_add(context.instruction_index as u64)
        .wrapping_add(u64::from(context.current_time.ordinal()));
    let count = instruction
        .count
        .as_deref()
        .map(|expr| eval_count_expression(expr, enumerator_len, seed))
        .unwrap_or_default();
    for _ in 0..count {
        schedule_next_with_rolls(context, instruction, definition, content)?;
    }
    Ok(())
}

pub(super) fn schedule_next_with_rolls(
    context: &mut YamlContext,
    instruction: &YamlInstruction,
    definition: &YamlPlayoutDefinition,
    content: &str,
) -> Result<(), SchedulingError> {
    if context.filler_stack.is_empty() {
        if let Some(sequence) = context.pre_roll_sequence.clone() {
            context.filler_stack.push(FillerKind::PreRoll);
            execute_sequence(context, definition, &sequence)?;
            context.filler_stack.pop();
        }
    }

    let media_item = context.current_media_for_content(content);
    if let Some((media_item, history_context)) = media_item {
        add_media_item(
            context,
            instruction,
            content,
            &media_item,
            media_item.duration,
            history_context,
        );
        context.advance_content(content);
    }

    if context.filler_stack.is_empty() {
        if let Some((sequence, expression)) = context.mid_roll_sequence.clone() {
            if expression.trim().eq_ignore_ascii_case("true") {
                context.filler_stack.push(FillerKind::MidRoll);
                execute_sequence(context, definition, &sequence)?;
                context.filler_stack.pop();
            }
        }
    }

    if context.filler_stack.is_empty() {
        if let Some(sequence) = context.post_roll_sequence.clone() {
            context.filler_stack.push(FillerKind::PostRoll);
            execute_sequence(context, definition, &sequence)?;
            context.filler_stack.pop();
        }
    }
    Ok(())
}

pub(super) fn add_media_item(
    context: &mut YamlContext,
    instruction: &YamlInstruction,
    content: &str,
    media_item: &SchedulingMediaItem,
    duration: Duration,
    history_context: CurrentHistoryContext,
) {
    let graphics_element_ids = context
        .graphics_elements
        .keys()
        .copied()
        .collect::<Vec<_>>();
    let mut item = playout_item(
        context.playout.id,
        media_item,
        context.current_time,
        duration,
        context.peek_guide_group(),
        context.current_filler_kind(instruction),
        instruction.custom_title.clone(),
        instruction.disable_watermarks,
        context.watermark_ids.iter().copied().collect(),
        graphics_element_ids,
        Some(format!(
            r#"{{"content":"{}","instructionIndex":{}}}"#,
            content, context.instruction_index
        )),
    );
    record_smart_shuffle_unit(context, content, media_item, duration, &item);
    item.graphics_elements = context
        .graphics_elements
        .iter()
        .map(
            |(&graphics_element_id, variables)| ersatzrs_domain::PlayoutItemGraphicsElement {
                playout_item_id: 0,
                graphics_element_id: Some(graphics_element_id),
                variables: variables.clone(),
            },
        )
        .collect();
    let history_key = context.history_key(content);
    match history_context {
        CurrentHistoryContext::Single { index } => {
            context.added_history.push(history(
                context.playout.id,
                history_key,
                &item,
                None,
                index,
                history_details_for_media_item(media_item),
                false,
            ));
        }
        CurrentHistoryContext::Playlist {
            index,
            child_key,
            child_index,
            child_details,
        } => {
            context.added_history.push(history(
                context.playout.id,
                history_key.clone(),
                &item,
                None,
                index,
                history_details_for_media_item(media_item),
                false,
            ));
            context.added_history.push(history(
                context.playout.id,
                history_key,
                &item,
                Some(child_key),
                child_index,
                child_details,
                true,
            ));
        }
    }
    context.current_time = item.finish;
    context.added_items.push(item);
    context.advance_guide_group();
}

fn record_smart_shuffle_unit(
    context: &mut YamlContext,
    content: &str,
    media_item: &SchedulingMediaItem,
    duration: Duration,
    item: &PlayoutItem,
) {
    let playback_order = context
        .playlist_enumerators
        .get(content)
        .map(super::PlaylistEnumerator::current_playback_order)
        .or_else(|| {
            context
                .enumerators
                .get(content)
                .map(super::MediaEnumerator::playback_order)
        })
        .or_else(|| {
            context
                .content_sources
                .get(content)
                .map(|resolved| resolved.playback_order)
        })
        .unwrap_or(PlaybackOrder::None);
    let policy = context
        .reference_data
        .smart_shuffle_policy_for_unit(content)
        .clone();
    let scope_key = policy.scope_key(context.playout.id, None, Some(content));
    let tags = context
        .reference_data
        .smart_shuffle_tags_for_unit(content)
        .clone();
    let unit = context.playlist_enumerator(content).map_or_else(
        || {
            Some(SchedulingSmartShuffleUnit::single(
                media_item.id,
                tags.get(&media_item.id).cloned().unwrap_or_default(),
                duration,
            ))
        },
        |playlist| playlist.smart_shuffle_unit(&tags),
    );
    if let Some(unit) = unit {
        context.smart_shuffle_state.record_unit(
            &policy,
            context.playout.id,
            &scope_key,
            item.start,
            &unit,
            SchedulingSmartShuffleRecordContext::new(
                is_randomized_playback_order(playback_order),
                item.filler_kind != FillerKind::None,
            ),
        );
    }
}
