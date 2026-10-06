use super::{
    MediaEnumerator, PlaylistEnumerator, PlayoutBuildMode, SchedulingError, YamlContext,
    YamlInstruction, YamlPlayoutDefinition,
};

mod content;
mod skipping;
mod state;
mod timing;

use content::{handle_all, handle_count};
use skipping::{handle_skip_items, handle_skip_to_item};
use state::{
    enabled_sequence, handle_epg_group, handle_graphics_off, handle_graphics_on, handle_watermark,
};
use timing::{
    handle_duration, handle_pad_to_next, handle_pad_until, handle_rewind, handle_wait_until,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum YamlInstructionKind {
    WaitUntil,
    Rewind,
    EpgGroup,
    Watermark,
    GraphicsOn,
    GraphicsOff,
    PreRoll,
    PostRoll,
    MidRoll,
    Repeat,
    Sequence,
    ShuffleSequence,
    SkipItems,
    SkipToItem,
    All,
    Count,
    Duration,
    PadToNext,
    PadUntil,
    Unsupported,
}

impl YamlInstruction {
    pub(super) fn kind(&self) -> YamlInstructionKind {
        if self.wait_until.is_some() {
            YamlInstructionKind::WaitUntil
        } else if self.rewind.is_some() {
            YamlInstructionKind::Rewind
        } else if self.epg_group.is_some() {
            YamlInstructionKind::EpgGroup
        } else if self.watermark.is_some() {
            YamlInstructionKind::Watermark
        } else if self.graphics_on.is_some() {
            YamlInstructionKind::GraphicsOn
        } else if self.graphics_off.is_some() {
            YamlInstructionKind::GraphicsOff
        } else if self.pre_roll.is_some() {
            YamlInstructionKind::PreRoll
        } else if self.post_roll.is_some() {
            YamlInstructionKind::PostRoll
        } else if self.mid_roll.is_some() {
            YamlInstructionKind::MidRoll
        } else if self.repeat.is_some_and(super::YamlRepeat::is_control) {
            YamlInstructionKind::Repeat
        } else if self.sequence.is_some() {
            YamlInstructionKind::Sequence
        } else if self.shuffle_sequence.is_some() {
            YamlInstructionKind::ShuffleSequence
        } else if self.skip_items.is_some() {
            YamlInstructionKind::SkipItems
        } else if self.skip_to_item.is_some() {
            YamlInstructionKind::SkipToItem
        } else if self.all.is_some() {
            YamlInstructionKind::All
        } else if self.count.is_some() {
            YamlInstructionKind::Count
        } else if self.duration.is_some() {
            YamlInstructionKind::Duration
        } else if self.pad_to_next.is_some() {
            YamlInstructionKind::PadToNext
        } else if self.pad_until.is_some() {
            YamlInstructionKind::PadUntil
        } else {
            YamlInstructionKind::Unsupported
        }
    }
}

pub(super) fn dispatch_instruction(
    context: &mut YamlContext,
    instruction: &YamlInstruction,
    mode: PlayoutBuildMode,
    definition: &YamlPlayoutDefinition,
) -> Result<(), SchedulingError> {
    context
        .visited_instructions
        .insert(context.instruction_index);
    match instruction.kind() {
        YamlInstructionKind::WaitUntil => handle_wait_until(context, instruction, mode),
        YamlInstructionKind::Rewind => handle_rewind(context, instruction),
        YamlInstructionKind::EpgGroup => {
            handle_epg_group(context, instruction);
            Ok(())
        }
        YamlInstructionKind::Watermark => {
            handle_watermark(context, instruction);
            Ok(())
        }
        YamlInstructionKind::GraphicsOn => {
            handle_graphics_on(context, instruction)?;
            Ok(())
        }
        YamlInstructionKind::GraphicsOff => {
            handle_graphics_off(context, instruction);
            Ok(())
        }
        YamlInstructionKind::PreRoll => {
            context.pre_roll_sequence =
                enabled_sequence(instruction.pre_roll, instruction.sequence.as_ref());
            Ok(())
        }
        YamlInstructionKind::PostRoll => {
            context.post_roll_sequence =
                enabled_sequence(instruction.post_roll, instruction.sequence.as_ref());
            Ok(())
        }
        YamlInstructionKind::MidRoll => {
            context.mid_roll_sequence =
                enabled_sequence(instruction.mid_roll, instruction.sequence.as_ref())
                    .map(|sequence| (sequence, instruction.expression.clone()));
            Ok(())
        }
        YamlInstructionKind::Repeat => {
            context.instruction_index = 0;
            Ok(())
        }
        YamlInstructionKind::Sequence => execute_sequence(
            context,
            definition,
            instruction.sequence.as_deref().unwrap_or_default(),
        ),
        YamlInstructionKind::ShuffleSequence => {
            if let Some(key) = instruction
                .shuffle_sequence
                .as_deref()
                .filter(|key| !key.trim().is_empty())
            {
                context.pending_shuffles.push(key.to_owned());
            }
            Ok(())
        }
        YamlInstructionKind::SkipItems => {
            handle_skip_items(context, instruction);
            Ok(())
        }
        YamlInstructionKind::SkipToItem => {
            handle_skip_to_item(context, instruction);
            Ok(())
        }
        YamlInstructionKind::All => handle_all(context, instruction, definition),
        YamlInstructionKind::Count => handle_count(context, instruction, definition),
        YamlInstructionKind::Duration => handle_duration(context, instruction, definition),
        YamlInstructionKind::PadToNext => handle_pad_to_next(context, instruction, definition),
        YamlInstructionKind::PadUntil => handle_pad_until(context, instruction, definition),
        YamlInstructionKind::Unsupported => Err(SchedulingError::UnsupportedYamlInstruction),
    }
}

pub(super) fn resume_instruction(
    context: &mut YamlContext,
    instruction: &YamlInstruction,
    definition: &YamlPlayoutDefinition,
    pending: super::checkpoints::PendingInstruction,
) -> Result<(), SchedulingError> {
    match pending {
        super::checkpoints::PendingInstruction::Count(count) => {
            let content = instruction
                .content
                .as_deref()
                .or(instruction.all.as_deref())
                .ok_or(SchedulingError::UnsupportedYamlInstruction)?;
            content::schedule_count(context, instruction, definition, content, count)
        }
        super::checkpoints::PendingInstruction::Until(target) => {
            timing::schedule_until(context, instruction, definition, target)
        }
    }
}

fn execute_sequence(
    context: &mut YamlContext,
    definition: &YamlPlayoutDefinition,
    sequence: &str,
) -> Result<(), SchedulingError> {
    let Some(sequence) = definition.sequence.iter().find(|item| item.key == sequence) else {
        return Ok(());
    };
    for instruction in &sequence.items {
        dispatch_instruction(context, instruction, PlayoutBuildMode::Continue, definition)?;
    }
    Ok(())
}
