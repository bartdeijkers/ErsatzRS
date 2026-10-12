//! Additional modeled allocations, cumulatively reserved without refunds.
//! SQL source strings moved into content are already source-bounded. These
//! guards count model slots and new buffers, not allocator overhead or peak RSS.

use serde::de::{DeserializeSeed, SeqAccess, Visitor};
use serde::Deserializer as _;

use super::Read;

fn arithmetic() -> sqlx::Error {
    sqlx::Error::Decode("guide metadata expansion arithmetic overflow".into())
}

pub(super) fn reserve_slots<T>(reader: &mut Read<'_>, count: usize) -> Result<(), sqlx::Error> {
    if reader.expansion_enabled() {
        let bytes = count
            .checked_mul(std::mem::size_of::<T>())
            .ok_or_else(arithmetic)?;
        reader.reserve_expansion(count, bytes)?;
    }
    Ok(())
}

pub(super) fn reserve_buffer(reader: &mut Read<'_>, bytes: usize) -> Result<(), sqlx::Error> {
    reader.reserve_expansion(1, bytes)
}

pub(super) fn reserve_strings(reader: &mut Read<'_>, values: &[String]) -> Result<(), sqlx::Error> {
    if reader.expansion_enabled() {
        let bytes = values.iter().try_fold(
            values
                .len()
                .checked_mul(std::mem::size_of::<String>())
                .ok_or_else(arithmetic)?,
            |sum, value| sum.checked_add(value.len()).ok_or_else(arithmetic),
        )?;
        reader.reserve_expansion(values.len(), bytes)?;
    }
    Ok(())
}

pub(super) fn reserve_join(reader: &mut Read<'_>, values: &[String]) -> Result<(), sqlx::Error> {
    if reader.expansion_enabled() && !values.is_empty() {
        let separators = (values.len() - 1).checked_mul(2).ok_or_else(arithmetic)?;
        let bytes = values.iter().try_fold(separators, |sum, value| {
            sum.checked_add(value.len()).ok_or_else(arithmetic)
        })?;
        reserve_buffer(reader, bytes)?;
    }
    Ok(())
}

pub(super) fn track(
    reader: &mut Read<'_>,
    value: Option<i32>,
) -> Result<Option<String>, sqlx::Error> {
    value
        .map(|value| {
            let mut digits = value.unsigned_abs();
            let mut bytes = 1 + usize::from(value < 0);
            while digits >= 10 {
                digits /= 10;
                bytes += 1;
            }
            reserve_buffer(reader, bytes)?;
            Ok(value.to_string())
        })
        .transpose()
}

pub(super) fn decode_list(
    reader: &mut Read<'_>,
    value: Option<String>,
) -> Result<Vec<String>, sqlx::Error> {
    if !reader.expansion_enabled() {
        return Ok(super::decode_string_list(value));
    }
    let Some(value) = super::nonempty(value) else {
        return Ok(Vec::new());
    };
    if let Some(cost) = list_cost(&value)? {
        let slots = cost
            .nodes
            .checked_mul(std::mem::size_of::<String>())
            .ok_or_else(arithmetic)?;
        let bytes = slots.checked_add(cost.bytes).ok_or_else(arithmetic)?;
        reader.reserve_expansion(cost.nodes, bytes)?;
    } else {
        // Malformed JSON follows the existing literal fallback; its payload moves.
        // Do not run an allocating Vec parser after validating a malformed prefix.
        reserve_slots::<String>(reader, 1)?;
        return Ok(vec![value]);
    }
    Ok(super::decode_string_list(Some(value)))
}

#[derive(Default)]
struct ListCost {
    nodes: usize,
    bytes: usize,
    overflow: bool,
}

fn list_cost(value: &str) -> Result<Option<ListCost>, sqlx::Error> {
    let mut cost = ListCost::default();
    let mut deserializer = serde_json::Deserializer::from_str(value);
    let valid =
        deserializer.deserialize_seq(ListVisitor(&mut cost)).is_ok() && deserializer.end().is_ok();
    if cost.overflow {
        return Err(arithmetic());
    }
    Ok(valid.then_some(cost))
}

struct ListVisitor<'a>(&'a mut ListCost);
impl<'de> Visitor<'de> for ListVisitor<'_> {
    type Value = ();
    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a string list")
    }
    fn visit_seq<S: SeqAccess<'de>>(self, mut sequence: S) -> Result<(), S::Error> {
        while sequence
            .next_element_seed(StringCost(&mut *self.0))?
            .is_some()
        {}
        Ok(())
    }
}

struct StringCost<'a>(&'a mut ListCost);
impl<'de> DeserializeSeed<'de> for StringCost<'_> {
    type Value = ();
    fn deserialize<D: serde::Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_str(self)
    }
}
impl Visitor<'_> for StringCost<'_> {
    type Value = ();
    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a string")
    }
    fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<(), E> {
        if let (Some(nodes), Some(bytes)) = (
            self.0.nodes.checked_add(1),
            self.0.bytes.checked_add(value.len()),
        ) {
            self.0.nodes = nodes;
            self.0.bytes = bytes;
            Ok(())
        } else {
            self.0.overflow = true;
            Err(E::custom("list size overflow"))
        }
    }
}
