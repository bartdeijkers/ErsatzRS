//! Private streaming duplicate-key check; never roundtrip through JSON values.

use std::{collections::BTreeSet, fmt};

use serde::de::{DeserializeSeed, Error, MapAccess, SeqAccess, Visitor};

use super::TeletextPagesError;

pub(super) fn reject_duplicate_keys(bytes: &[u8]) -> Result<(), TeletextPagesError> {
    let mut duplicate = false;
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let result = CheckedValue {
        duplicate: &mut duplicate,
    }
    .deserialize(&mut deserializer)
    .and_then(|()| deserializer.end());
    result.map_err(|_| {
        if duplicate {
            TeletextPagesError::DuplicateKey
        } else {
            TeletextPagesError::InvalidJson
        }
    })
}

struct CheckedValue<'a> {
    duplicate: &'a mut bool,
}

impl<'de> DeserializeSeed<'de> for CheckedValue<'_> {
    type Value = ();

    fn deserialize<D>(self, deserializer: D) -> Result<(), D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for CheckedValue<'_> {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("one JSON value without repeated object keys")
    }

    fn visit_bool<E>(self, _: bool) -> Result<(), E> {
        Ok(())
    }

    fn visit_i64<E>(self, _: i64) -> Result<(), E> {
        Ok(())
    }

    fn visit_u64<E>(self, _: u64) -> Result<(), E> {
        Ok(())
    }

    fn visit_f64<E>(self, _: f64) -> Result<(), E> {
        Ok(())
    }

    fn visit_str<E>(self, _: &str) -> Result<(), E> {
        Ok(())
    }

    fn visit_unit<E>(self) -> Result<(), E> {
        Ok(())
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<(), A::Error>
    where
        A: SeqAccess<'de>,
    {
        while sequence
            .next_element_seed(CheckedValue {
                duplicate: self.duplicate,
            })?
            .is_some()
        {}
        Ok(())
    }

    fn visit_map<A>(self, mut map: A) -> Result<(), A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut keys = BTreeSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if !keys.insert(key) {
                *self.duplicate = true;
                return Err(A::Error::custom("duplicate JSON object key"));
            }
            map.next_value_seed(CheckedValue {
                duplicate: self.duplicate,
            })?;
        }
        Ok(())
    }
}
