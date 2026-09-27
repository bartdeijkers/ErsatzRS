//! Persist flattened sequence permutations, matching `YamlPlayoutContext`'s
//! list fingerprint and all-or-nothing restoration contract.
use std::collections::VecDeque;
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use super::{
    BTreeMap, BTreeSet, SchedulingError, YamlContext, YamlInstruction, YamlPlayoutDefinition,
};

#[derive(Debug, Clone, Default)]
pub(super) struct SequenceState {
    pub orders: Option<serde_json::Value>,
    pub fingerprints: Option<BTreeMap<String, String>>,
}

#[derive(Serialize, Deserialize)]
struct SavedOrder {
    #[serde(alias = "Sequence")]
    sequence: String,
    #[serde(alias = "Order")]
    order: Vec<usize>,
}

struct Group {
    sequence: String,
    positions: Vec<usize>,
}

pub(super) fn fingerprint(value: &impl Serialize) -> Result<String, SchedulingError> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| SchedulingError::InvalidYaml(error.to_string()))?;
    let mut digest = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(&mut digest, "{byte:02X}")
            .map_err(|error| SchedulingError::InvalidYaml(error.to_string()))?;
    }
    Ok(digest)
}

fn groups(instructions: &[YamlInstruction]) -> Vec<Group> {
    let mut groups = Vec::<(Option<String>, Group)>::new();
    for (position, instruction) in instructions.iter().enumerate() {
        let Some(key) = instruction
            .sequence_key
            .as_ref()
            .filter(|key| !key.trim().is_empty())
        else {
            continue;
        };
        if let Some((_, group)) = groups
            .iter_mut()
            .find(|(id, _)| *id == instruction.sequence_guid)
        {
            group.positions.push(position);
        } else {
            groups.push((
                instruction.sequence_guid.clone(),
                Group {
                    sequence: key.clone(),
                    positions: vec![position],
                },
            ));
        }
    }
    groups.into_iter().map(|(_, group)| group).collect()
}

fn list_fingerprint(instructions: &[YamlInstruction]) -> Result<String, SchedulingError> {
    let mut normalized = instructions.to_vec();
    for group in groups(instructions) {
        let mut declaration = group
            .positions
            .iter()
            .map(|&index| instructions[index].clone())
            .collect::<Vec<_>>();
        declaration.sort_by_key(|instruction| instruction.sequence_index);
        for (position, instruction) in group.positions.into_iter().zip(declaration) {
            normalized[position] = instruction;
        }
    }
    for instruction in &mut normalized {
        instruction.sequence_guid = None;
    }
    let state = normalized
        .iter()
        .map(|instruction| {
            (
                instruction,
                instruction.sequence_index,
                &instruction.sequence_fingerprint,
            )
        })
        .collect::<Vec<_>>();
    fingerprint(&state)
}

impl SequenceState {
    pub fn capture_orders(
        &mut self,
        definition: &YamlPlayoutDefinition,
    ) -> Result<(), SchedulingError> {
        let mut lists = BTreeMap::new();
        let mut seen = BTreeSet::new();
        let named = definition
            .schedules
            .iter()
            .filter(|schedule| {
                !schedule.name.trim().is_empty() && seen.insert(schedule.name.as_str())
            })
            .map(|schedule| (schedule.name.as_str(), schedule.playout.as_slice()));
        for (name, instructions) in
            std::iter::once(("", definition.playout.as_slice())).chain(named)
        {
            let orders = groups(instructions)
                .into_iter()
                .filter(|group| {
                    group
                        .positions
                        .iter()
                        .any(|&position| instructions[position].sequence_shuffled)
                })
                .map(|group| SavedOrder {
                    sequence: group.sequence,
                    order: group
                        .positions
                        .iter()
                        .map(|&position| instructions[position].sequence_index)
                        .collect(),
                })
                .collect::<Vec<_>>();
            if !orders.is_empty() {
                lists.insert(name.to_owned(), orders);
            }
        }
        self.orders = if lists.is_empty() {
            None
        } else {
            Some(
                serde_json::to_value(lists)
                    .map_err(|error| SchedulingError::InvalidYaml(error.to_string()))?,
            )
        };
        Ok(())
    }
}

impl YamlContext {
    pub(super) fn restore_sequence_orders(
        &mut self,
        definition: &mut YamlPlayoutDefinition,
    ) -> Result<(), SchedulingError> {
        let saved = std::mem::take(&mut self.sequence_state);
        let mut fingerprints = BTreeMap::new();
        let mut seen = BTreeSet::new();
        let named = definition
            .schedules
            .iter_mut()
            .filter(|schedule| {
                !schedule.name.trim().is_empty() && seen.insert(schedule.name.clone())
            })
            .map(|schedule| (schedule.name.as_str(), &mut schedule.playout));
        for (name, instructions) in std::iter::once(("", &mut definition.playout)).chain(named) {
            let current = list_fingerprint(instructions)?;
            let stale = saved
                .fingerprints
                .as_ref()
                .is_some_and(|old| old.get(name) != Some(&current));
            let order = saved.orders.as_ref().and_then(|orders| orders.get(name));
            let invalid = saved
                .orders
                .as_ref()
                .is_some_and(|orders| !orders.is_object())
                || order.is_some_and(|order| {
                    saved.fingerprints.is_none() || !restore_list(instructions, order, stale)
                });
            if stale || invalid {
                if self.active_schedule.as_deref().unwrap_or_default() == name {
                    self.instruction_index = 0;
                }
                if let Some(state) = self.list_states.get_mut(name) {
                    state.instruction_index = 0;
                }
            }
            fingerprints.insert(name.to_owned(), current);
        }
        self.sequence_state.fingerprints = Some(fingerprints);
        Ok(())
    }
}

fn restore_list(
    instructions: &mut [YamlInstruction],
    value: &serde_json::Value,
    stale: bool,
) -> bool {
    if stale {
        return false;
    }
    let Ok(orders) = serde_json::from_value::<Vec<SavedOrder>>(value.clone()) else {
        return false;
    };
    if orders.is_empty() || orders.iter().any(|order| order.sequence.trim().is_empty()) {
        return false;
    }
    let mut remaining = BTreeMap::<String, VecDeque<Group>>::new();
    for group in groups(instructions) {
        remaining
            .entry(group.sequence.clone())
            .or_default()
            .push_back(group);
    }
    let mut restored = Vec::new();
    let mut restored_sequences = BTreeSet::new();
    for order in orders {
        let Some(group) = remaining
            .get_mut(&order.sequence)
            .and_then(VecDeque::pop_front)
        else {
            return false;
        };
        if order.order.len() != group.positions.len() {
            return false;
        }
        let mut by_index = BTreeMap::<usize, VecDeque<YamlInstruction>>::new();
        for &position in &group.positions {
            let instruction = instructions[position].clone();
            by_index
                .entry(instruction.sequence_index)
                .or_default()
                .push_back(instruction);
        }
        for (position, index) in group.positions.into_iter().zip(order.order) {
            let Some(mut instruction) = by_index.get_mut(&index).and_then(VecDeque::pop_front)
            else {
                return false;
            };
            instruction.sequence_shuffled = true;
            restored.push((position, instruction));
        }
        if by_index.values().any(|queue| !queue.is_empty()) {
            return false;
        }
        restored_sequences.insert(order.sequence);
    }
    if restored_sequences
        .iter()
        .any(|name| remaining.get(name).is_some_and(|queue| !queue.is_empty()))
    {
        return false;
    }
    for (position, instruction) in restored {
        instructions[position] = instruction;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduling::SchedulingReferenceData;
    use ersatzrs_domain::{Playout, PlayoutAnchor};

    fn context() -> YamlContext {
        YamlContext::new(
            Playout::default(),
            SchedulingReferenceData::default(),
            chrono::Utc::now(),
            BTreeMap::new(),
        )
    }

    fn definition() -> YamlPlayoutDefinition {
        YamlPlayoutDefinition {
            playout: (0..4)
                .map(|index| YamlInstruction {
                    content: Some(format!("show-{index}")),
                    sequence_key: Some("shows".to_owned()),
                    sequence_guid: Some("shows:1".to_owned()),
                    sequence_index: index,
                    sequence_fingerprint: "synthetic-sequence".to_owned(),
                    ..YamlInstruction::default()
                })
                .collect(),
            ..YamlPlayoutDefinition::default()
        }
    }

    fn saved_anchor() -> PlayoutAnchor {
        let mut definition = definition();
        let mut context = context();
        context.restore_sequence_orders(&mut definition).unwrap();
        definition.playout.rotate_left(2);
        for instruction in &mut definition.playout {
            instruction.sequence_shuffled = true;
        }
        context.instruction_index = 3;
        context.sequence_state.capture_orders(&definition).unwrap();
        PlayoutAnchor {
            context: Some(context.serialize().unwrap()),
            ..PlayoutAnchor::default()
        }
    }

    #[test]
    fn saved_sequence_order_and_position_restore_together() {
        let mut definition = definition();
        let mut context = context();
        context.restore(&saved_anchor()).unwrap();
        context.restore_sequence_orders(&mut definition).unwrap();
        assert_eq!(context.instruction_index, 3);
        assert_eq!(
            definition
                .playout
                .iter()
                .map(|item| item.sequence_index)
                .collect::<Vec<_>>(),
            [2, 3, 0, 1]
        );
    }

    #[test]
    fn partial_invocation_orders_leave_every_slot_unchanged() {
        let mut instructions = definition().playout;
        let mut second = instructions.clone();
        for instruction in &mut second {
            instruction.sequence_guid = Some("shows:2".to_owned());
        }
        instructions.extend(second);
        let original = instructions.clone();
        assert!(!restore_list(
            &mut instructions,
            &serde_json::json!([{"sequence":"shows","order":[2,3,0,1]}]),
            false,
        ));
        assert_eq!(instructions, original);
    }

    #[test]
    fn stale_named_list_resets_only_its_position() {
        use super::super::{YamlListState, YamlPlayoutScheduleItem};

        let mut original = definition();
        original.schedules = ["Holiday", "Weekend"]
            .into_iter()
            .map(|name| YamlPlayoutScheduleItem {
                name: name.to_owned(),
                playout: definition().playout,
                ..YamlPlayoutScheduleItem::default()
            })
            .collect();
        let mut shuffled = original.clone();
        let mut source = context();
        source.restore_sequence_orders(&mut shuffled).unwrap();
        for (index, instructions) in std::iter::once(&mut shuffled.playout)
            .chain(
                shuffled
                    .schedules
                    .iter_mut()
                    .map(|schedule| &mut schedule.playout),
            )
            .enumerate()
        {
            instructions.rotate_left(index + 1);
            for instruction in instructions {
                instruction.sequence_shuffled = true;
            }
        }
        source.instruction_index = 3;
        source
            .list_states
            .insert("Holiday".to_owned(), YamlListState::at_index(2));
        source
            .list_states
            .insert("Weekend".to_owned(), YamlListState::at_index(1));
        source.sequence_state.capture_orders(&shuffled).unwrap();
        let anchor = PlayoutAnchor {
            context: Some(source.serialize().unwrap()),
            ..PlayoutAnchor::default()
        };

        original.schedules[0].playout[0].sequence_fingerprint = "changed declaration".to_owned();
        let expected_holiday = original.schedules[0].playout.clone();
        let mut restored = context();
        restored.restore(&anchor).unwrap();
        restored.restore_sequence_orders(&mut original).unwrap();
        assert_eq!(original.playout, shuffled.playout);
        assert_eq!(original.schedules[0].playout, expected_holiday);
        assert_eq!(original.schedules[1].playout, shuffled.schedules[1].playout);
        assert_eq!(restored.instruction_index, 3);
        assert_eq!(restored.list_states["Holiday"].instruction_index, 0);
        assert_eq!(restored.list_states["Weekend"].instruction_index, 1);
    }

    #[test]
    fn malformed_orders_never_partially_restore_a_list() {
        for orders in [
            serde_json::json!(null),
            serde_json::json!([]),
            serde_json::json!([null]),
            serde_json::json!([{"sequence":"shows","order":[2,3,0,0]}]),
            serde_json::json!([{"sequence":"shows","order":[2,3]}]),
            serde_json::json!([{"sequence":"shows","order":[2,3,0,1]}, {"sequence":"missing","order":[0]}]),
        ] {
            let mut anchor = saved_anchor();
            let mut value: serde_json::Value =
                serde_json::from_str(anchor.context.as_deref().unwrap()).unwrap();
            value["sequence_orders"][""] = orders;
            anchor.context = Some(value.to_string());
            let mut actual = definition();
            let mut context = context();
            context.restore(&anchor).unwrap();
            context.restore_sequence_orders(&mut actual).unwrap();
            assert_eq!(context.instruction_index, 0);
            assert_eq!(actual.playout, definition().playout);
        }
    }

    #[test]
    fn duplicate_and_unnamed_schedules_do_not_replace_saved_lists() {
        use super::super::YamlPlayoutScheduleItem;

        let mut actual = definition();
        for instruction in &mut actual.playout {
            instruction.sequence_shuffled = true;
        }
        actual.schedules = ["Holiday", "Holiday", "", " "]
            .into_iter()
            .enumerate()
            .map(|(index, name)| {
                let mut playout = actual.playout.clone();
                playout.rotate_left(index);
                YamlPlayoutScheduleItem {
                    name: name.to_owned(),
                    playout,
                    ..YamlPlayoutScheduleItem::default()
                }
            })
            .collect();
        let mut source = context();
        source.restore_sequence_orders(&mut actual).unwrap();
        source.sequence_state.capture_orders(&actual).unwrap();
        let orders = source.sequence_state.orders.as_ref().unwrap();
        assert_eq!(orders.as_object().unwrap().len(), 2);
        assert_eq!(orders[""], orders["Holiday"]);
        assert_eq!(
            source.sequence_state.fingerprints.as_ref().unwrap().len(),
            2
        );
    }

    #[test]
    fn changed_layout_and_unfingerprinted_orders_reset_position() {
        for missing_fingerprint in [false, true] {
            let mut anchor = saved_anchor();
            let mut actual = definition();
            if missing_fingerprint {
                let mut value: serde_json::Value =
                    serde_json::from_str(anchor.context.as_deref().unwrap()).unwrap();
                value.as_object_mut().unwrap().remove("list_fingerprints");
                anchor.context = Some(value.to_string());
            } else {
                actual.playout[0].custom_title = Some("Changed title".to_owned());
            }
            let expected = actual.playout.clone();
            let mut context = context();
            context.restore(&anchor).unwrap();
            context.restore_sequence_orders(&mut actual).unwrap();
            assert_eq!(context.instruction_index, 0);
            assert_eq!(actual.playout, expected);
        }
    }

    #[test]
    fn legacy_anchor_without_sequence_state_keeps_position() {
        let mut anchor = saved_anchor();
        let mut value: serde_json::Value =
            serde_json::from_str(anchor.context.as_deref().unwrap()).unwrap();
        let object = value.as_object_mut().unwrap();
        object.remove("list_fingerprints");
        object.remove("sequence_orders");
        anchor.context = Some(value.to_string());
        let mut actual = definition();
        let mut context = context();
        context.restore(&anchor).unwrap();
        context.restore_sequence_orders(&mut actual).unwrap();
        assert_eq!(context.instruction_index, 3);
        assert_eq!(actual.playout, definition().playout);
    }
}
