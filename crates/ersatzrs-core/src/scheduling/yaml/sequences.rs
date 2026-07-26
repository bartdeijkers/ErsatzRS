use super::{BTreeMap, BTreeSet, SchedulingError, YamlInstruction, YamlPlayoutDefinition};

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
    let mut depth = 0;
    while definition.playout.iter().any(is_sequence_instruction) {
        if depth > 100 {
            return Err(SchedulingError::SequenceNestingTooDeep);
        }
        let original = definition.playout.clone();
        definition.playout.clear();
        for instruction in original {
            if is_sequence_instruction(&instruction) {
                let sequence_key = instruction.sequence.as_ref().expect("sequence exists");
                let repeat = instruction
                    .count
                    .as_deref()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(1);
                if let Some(sequence) = definition
                    .sequence
                    .iter()
                    .find(|sequence| &sequence.key == sequence_key)
                {
                    for _ in 0..repeat {
                        definition.playout.extend(sequence.items.clone());
                    }
                }
            } else {
                definition.playout.push(instruction);
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
