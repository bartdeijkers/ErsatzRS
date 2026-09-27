use super::{
    BTreeMap, BTreeSet, SchedulingError, YamlInstruction, YamlPlayoutDefinition,
    YamlPlayoutSequenceItem,
};

pub(super) fn detect_cycle(definition: &YamlPlayoutDefinition) -> Result<(), SchedulingError> {
    fn visit<'a>(
        key: &'a str,
        graph: &'a BTreeMap<&'a str, Vec<&'a str>>,
        visiting: &mut BTreeSet<&'a str>,
        visited: &mut BTreeSet<&'a str>,
    ) -> bool {
        if visiting.contains(key) {
            return true;
        }
        if !visited.insert(key) {
            return false;
        }
        visiting.insert(key);
        for child in graph.get(key).into_iter().flatten() {
            if visit(child, graph, visiting, visited) {
                return true;
            }
        }
        visiting.remove(key);
        false
    }

    let mut graph = BTreeMap::<&str, Vec<&str>>::new();
    for sequence in &definition.sequence {
        let children = sequence
            .items
            .iter()
            .filter_map(|instruction| instruction.sequence.as_deref())
            .collect::<Vec<_>>();
        graph.insert(&sequence.key, children);
    }
    let mut visited = BTreeSet::new();
    for key in graph.keys() {
        if visit(key, &graph, &mut BTreeSet::new(), &mut visited) {
            return Err(SchedulingError::SequenceCycle);
        }
    }
    Ok(())
}

pub(super) fn flatten_sequences(
    definition: &mut YamlPlayoutDefinition,
) -> Result<(), SchedulingError> {
    flatten_list(&mut definition.playout, &definition.sequence)?;
    for schedule in &mut definition.schedules {
        flatten_list(&mut schedule.playout, &definition.sequence)?;
    }
    Ok(())
}

fn flatten_list(
    playout: &mut Vec<YamlInstruction>,
    sequences: &[YamlPlayoutSequenceItem],
) -> Result<(), SchedulingError> {
    let mut depth = 0;
    let mut sequence_id = 0_u64;
    while playout.iter().any(is_sequence_instruction) {
        if depth > 100 {
            return Err(SchedulingError::SequenceNestingTooDeep);
        }
        let original = std::mem::take(playout);
        for instruction in original {
            if is_sequence_instruction(&instruction) {
                let sequence_key = instruction.sequence.as_ref().expect("sequence exists");
                let repeat = instruction
                    .repeat
                    .map_or(1, super::YamlRepeat::sequence_count);
                if let Some(sequence) = sequences
                    .iter()
                    .find(|sequence| &sequence.key == sequence_key)
                {
                    sequence_id += 1;
                    let sequence_guid = format!("{sequence_key}:{sequence_id}");
                    let fingerprint = super::sequence_state::fingerprint(&sequence.items)?;
                    for repetition in 0..repeat {
                        for (index, item) in sequence.items.iter().enumerate() {
                            let mut item = item.clone();
                            item.sequence_key = Some(sequence_key.clone());
                            item.sequence_guid = Some(sequence_guid.clone());
                            item.sequence_index = repetition * sequence.items.len() + index;
                            item.sequence_shuffled = false;
                            item.sequence_fingerprint.clone_from(&fingerprint);
                            if instruction
                                .custom_title
                                .as_ref()
                                .is_some_and(|title| !title.trim().is_empty())
                            {
                                item.custom_title.clone_from(&instruction.custom_title);
                            }
                            playout.push(item);
                        }
                    }
                }
            } else {
                playout.push(instruction);
            }
        }
        depth += 1;
    }
    Ok(())
}

fn is_sequence_instruction(instruction: &YamlInstruction) -> bool {
    instruction.sequence.is_some()
        && instruction.pre_roll.is_none()
        && instruction.post_roll.is_none()
        && instruction.mid_roll.is_none()
}
