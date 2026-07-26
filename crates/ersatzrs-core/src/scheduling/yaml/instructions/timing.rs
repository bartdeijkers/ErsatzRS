use super::super::{
    parse_duration, parse_hhmm, DateTime, Duration, PlayoutBuildMode, SchedulingError, Timelike,
    Utc, YamlContext, YamlInstruction, YamlPlayoutDefinition,
};
use super::content::{add_media_item, schedule_next_with_rolls};

pub(super) fn handle_wait_until(
    context: &mut YamlContext,
    instruction: &YamlInstruction,
    mode: PlayoutBuildMode,
) -> Result<(), SchedulingError> {
    let Some(wait_until) = &instruction.wait_until else {
        return Ok(());
    };
    let (hour, minute, second) = parse_hhmm(wait_until)?;
    let current_time = context.current_time;
    let current_seconds = current_time.num_seconds_from_midnight();
    let target_seconds = hour * 3600 + minute * 60 + second;
    let mut date = current_time.date_naive();
    if current_seconds > target_seconds {
        if instruction.tomorrow == Some("true".to_owned()) {
            date = date
                .succ_opt()
                .ok_or_else(|| SchedulingError::InvalidTime(wait_until.clone()))?;
        } else if !(instruction.rewind_on_reset && mode == PlayoutBuildMode::Reset) {
            return Ok(());
        }
    }
    let target = date
        .and_hms_opt(hour, minute, second)
        .ok_or_else(|| SchedulingError::InvalidTime(wait_until.clone()))?;
    context.current_time = DateTime::from_naive_utc_and_offset(target, Utc);
    Ok(())
}

pub(super) fn handle_rewind(
    context: &mut YamlContext,
    instruction: &YamlInstruction,
) -> Result<(), SchedulingError> {
    if let Some(rewind) = &instruction.rewind {
        context.current_time -= parse_duration(rewind)?;
    }
    Ok(())
}

pub(super) fn handle_duration(
    context: &mut YamlContext,
    instruction: &YamlInstruction,
    definition: &YamlPlayoutDefinition,
) -> Result<(), SchedulingError> {
    let duration = parse_duration(instruction.duration.as_deref().unwrap_or_default())?;
    let target = context.current_time + duration;
    schedule_until(context, instruction, definition, target)
}

pub(super) fn handle_pad_to_next(
    context: &mut YamlContext,
    instruction: &YamlInstruction,
    definition: &YamlPlayoutDefinition,
) -> Result<(), SchedulingError> {
    let interval = instruction.pad_to_next.unwrap_or(1).max(1);
    let current_minute = i64::from(context.current_time.minute());
    let target_minute = ((current_minute + interval - 1) / interval) * interval;
    let base = context.current_time
        - Duration::minutes(current_minute)
        - Duration::seconds(i64::from(context.current_time.second()));
    let mut target = base + Duration::minutes(target_minute);
    if target <= context.current_time {
        target += Duration::minutes(interval);
    }
    schedule_until(context, instruction, definition, target)
}

pub(super) fn handle_pad_until(
    context: &mut YamlContext,
    instruction: &YamlInstruction,
    definition: &YamlPlayoutDefinition,
) -> Result<(), SchedulingError> {
    let value = instruction.pad_until.as_deref().unwrap_or_default();
    let (hour, minute, second) = parse_hhmm(value)?;
    let mut date = context.current_time.date_naive();
    let target_seconds = hour * 3600 + minute * 60 + second;
    if context.current_time.num_seconds_from_midnight() > target_seconds
        && instruction.tomorrow.as_deref().unwrap_or("false") == "true"
    {
        date = date
            .succ_opt()
            .ok_or_else(|| SchedulingError::InvalidTime(value.to_owned()))?;
    }
    let target = date
        .and_hms_opt(hour, minute, second)
        .ok_or_else(|| SchedulingError::InvalidTime(value.to_owned()))?;
    schedule_until(
        context,
        instruction,
        definition,
        DateTime::from_naive_utc_and_offset(target, Utc),
    )
}

fn schedule_until(
    context: &mut YamlContext,
    instruction: &YamlInstruction,
    definition: &YamlPlayoutDefinition,
    target: DateTime<Utc>,
) -> Result<(), SchedulingError> {
    let Some(content) = instruction.content.as_deref() else {
        return Ok(());
    };
    while context.current_time < target {
        let Some((media_item, history_context)) = context.current_media_for_content(content) else {
            if schedule_fallback_item(context, instruction, target) {
                continue;
            }
            break;
        };
        let remaining = target - context.current_time;
        if media_item.duration <= remaining || !instruction.stop_before_end {
            schedule_next_with_rolls(context, instruction, definition, content)?;
        } else if instruction.discard_attempts > 0 {
            context.advance_content(content);
        } else if instruction.trim {
            add_media_item(
                context,
                instruction,
                content,
                &media_item,
                remaining,
                history_context,
            );
            context.advance_content(content);
        } else if !schedule_fallback_item(context, instruction, target) {
            let gap = ersatzrs_domain::PlayoutGap {
                playout_id: context.playout.id,
                start: context.current_time,
                finish: target,
                ..ersatzrs_domain::PlayoutGap::default()
            };
            context.gaps.push(gap);
            context.current_time = target;
        }
    }
    Ok(())
}

fn schedule_fallback_item(
    context: &mut YamlContext,
    instruction: &YamlInstruction,
    target: DateTime<Utc>,
) -> bool {
    let Some(fallback) = instruction.fallback.as_deref() else {
        return false;
    };
    let remaining = target - context.current_time;
    let Some((fallback_item, fallback_history_context)) =
        context.current_media_for_content(fallback)
    else {
        return false;
    };

    let mut fallback_instruction = instruction.clone();
    fallback_instruction.filler_kind = Some("Fallback".to_owned());
    add_media_item(
        context,
        &fallback_instruction,
        fallback,
        &fallback_item,
        fallback_item.duration.min(remaining),
        fallback_history_context,
    );
    context.advance_content(fallback);
    context.current_time = target;
    true
}
