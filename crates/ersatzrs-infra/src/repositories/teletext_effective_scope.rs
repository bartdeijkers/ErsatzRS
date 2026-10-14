//! Compare one bounded immutable original with its exact supplied scope.
//! This is association, not native qualification or fresh current authority.

use chrono::{DateTime, Utc};
use ersatzrs_domain::{
    TeletextEffectiveBindingSnapshot as Binding,
    TeletextEffectiveChannelSelectionSnapshot as Selection, TeletextInstallationSnapshot,
    TeletextOverrideState, TeletextProfileSetting, TeletextProfileValue,
    TeletextSecretReferenceKind,
};
use serde::de::{self, DeserializeSeed, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde_json::{json, Map, Value};
use std::fmt;

fn installation(value: &TeletextInstallationSnapshot) -> Value {
    json!({"addon_id":value.addon_id,"state_revision":value.state_revision,"version":value.version,
        "package_sha256":value.package_sha256,"content_sha256":value.content_sha256,
        "manifest_sha256":value.manifest_sha256})
}

fn settings(values: &[TeletextProfileSetting]) -> Value {
    Value::Array(
        values
            .iter()
            .map(|setting| {
                // Compare unresolved reference identity; never read environment/file contents.
                let (kind, value) = match &setting.value {
                    TeletextProfileValue::Literal(value) => ("literal", value),
                    TeletextProfileValue::SecretReference {
                        kind: TeletextSecretReferenceKind::Environment,
                        reference,
                    } => ("environment", reference),
                    TeletextProfileValue::SecretReference {
                        kind: TeletextSecretReferenceKind::File,
                        reference,
                    } => ("file", reference),
                };
                json!({"key":setting.key,"value":{"kind":kind,"value":value}})
            })
            .collect(),
    )
}

fn expected_scope(selection: &Selection) -> Value {
    let bindings = selection.bindings.iter().map(|binding| match binding {
        Binding::Base(base) => json!({"origin":"base","scripted":null,"base":{
            "contract":base.contract.schema_version(),"channel_id":base.channel_id,"addon_id":base.addon_id,
            "binding_revision":base.binding_revision,"profile_id":base.profile_id,"profile_revision":base.profile_revision,
            "installation":installation(&base.installation),"effective_settings":settings(&base.effective_settings)}}),
        Binding::Scripted(published) => {
            let state = &published.interval.state;
            let candidate = match state {
                TeletextOverrideState::Off { .. } => Value::Null,
                TeletextOverrideState::Enabled(candidate) => json!({"contract":candidate.contract.schema_version(),
                    "channel_id":candidate.channel_id,"addon_id":candidate.addon_id,"profile_id":candidate.profile_id,
                    "profile_revision":candidate.profile_revision,"installation":installation(&candidate.installation),
                    "effective_settings":settings(&candidate.effective_settings)}),
            };
            json!({"origin":"scripted","base":null,"scripted":{"playout_id":published.playout_id,
                "start":published.interval.start,"finish":published.interval.finish,"state":{
                    "kind":if binding.enabled(){"enabled"}else{"off"},"channel_id":state.channel_id(),
                    "addon_id":state.addon_id(),"candidate":candidate}}})
        }
    }).collect::<Vec<_>>();
    json!({"channel_id":selection.channel_id,"bindings":bindings,"valid_until":selection.valid_until})
}

#[cfg(test)]
pub(in crate::repositories) fn test_scope(selection: &Selection) -> Value {
    expected_scope(selection)
}

pub(in crate::repositories) fn matches(bytes: &[u8], selection: &Selection) -> bool {
    if !expected_fits(selection, bytes.len()) {
        return false;
    }
    let expected = expected_scope(selection);
    let mut decoder = serde_json::Deserializer::from_slice(bytes);
    Root(&expected).deserialize(&mut decoder).is_ok() && decoder.end().is_ok()
}

fn charge(remaining: &mut usize, size: usize) -> bool {
    let Some(next) = remaining.checked_sub(size) else {
        return false;
    };
    *remaining = next;
    true
}

fn profile_fits(
    remaining: &mut usize,
    addon: &str,
    install: &TeletextInstallationSnapshot,
    values: &[TeletextProfileSetting],
) -> bool {
    // Every encoded setting has more than 35 fixed JSON bytes, even before its
    // key/value text. Admit expected nodes before allocating their JSON tree.
    if !charge(remaining, values.len().saturating_mul(35)) {
        return false;
    }
    for text in [
        addon,
        &install.addon_id,
        &install.state_revision,
        &install.version,
        &install.package_sha256,
        &install.content_sha256,
        &install.manifest_sha256,
    ] {
        if !charge(remaining, text.len()) {
            return false;
        }
    }
    for setting in values {
        let text = match &setting.value {
            TeletextProfileValue::Literal(text) => text,
            TeletextProfileValue::SecretReference { reference, .. } => reference,
        };
        if !charge(remaining, setting.key.len()) || !charge(remaining, text.len()) {
            return false;
        }
    }
    true
}

fn expected_fits(selection: &Selection, maximum: usize) -> bool {
    let mut remaining = maximum;
    if !charge(&mut remaining, selection.bindings.len().saturating_mul(3)) {
        return false;
    }
    selection.bindings.iter().all(|binding| match binding {
        Binding::Base(base) => profile_fits(
            &mut remaining,
            &base.addon_id,
            &base.installation,
            &base.effective_settings,
        ),
        Binding::Scripted(published) => {
            charge(&mut remaining, published.interval.state.addon_id().len())
                && match &published.interval.state {
                    TeletextOverrideState::Off { .. } => true,
                    TeletextOverrideState::Enabled(candidate) => profile_fits(
                        &mut remaining,
                        &candidate.addon_id,
                        &candidate.installation,
                        &candidate.effective_settings,
                    ),
                }
        }
    })
}

fn mismatch<E: de::Error>() -> E {
    E::custom("effective original scope does not match")
}

struct Compare<'a> {
    expected: &'a Value,
    date: bool,
}
impl<'de> DeserializeSeed<'de> for Compare<'_> {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, decoder: D) -> Result<(), D::Error> {
        decoder.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Compare<'_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the exact effective original field")
    }
    fn visit_unit<E: de::Error>(self) -> Result<(), E> {
        if self.expected.is_null() {
            Ok(())
        } else {
            Err(mismatch())
        }
    }
    fn visit_bool<E: de::Error>(self, value: bool) -> Result<(), E> {
        if self.expected.as_bool() == Some(value) {
            Ok(())
        } else {
            Err(mismatch())
        }
    }
    fn visit_i64<E: de::Error>(self, value: i64) -> Result<(), E> {
        if self.expected.as_i64() == Some(value) {
            Ok(())
        } else {
            Err(mismatch())
        }
    }
    fn visit_u64<E: de::Error>(self, value: u64) -> Result<(), E> {
        if self.expected.as_u64() == Some(value) {
            Ok(())
        } else {
            Err(mismatch())
        }
    }
    fn visit_str<E: de::Error>(self, value: &str) -> Result<(), E> {
        let Some(expected) = self.expected.as_str() else {
            return Err(mismatch());
        };
        if expected == value
            || self.date
                && expected
                    .parse::<DateTime<Utc>>()
                    .ok()
                    .zip(value.parse::<DateTime<Utc>>().ok())
                    .is_some_and(|(a, b)| a == b)
        {
            Ok(())
        } else {
            Err(mismatch())
        }
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        let Some(expected) = self.expected.as_array() else {
            return Err(mismatch());
        };
        for item in expected {
            seq.next_element_seed(Compare {
                expected: item,
                date: false,
            })?
            .ok_or_else(mismatch)?;
        }
        let _ = seq.next_element_seed(Reject)?;
        Ok(())
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        let Some(expected) = self.expected.as_object() else {
            return Err(mismatch());
        };
        let mut seen = vec![false; expected.len()];
        while let Some(index) = map.next_key_seed(Key(expected))? {
            if seen[index] {
                return Err(mismatch());
            }
            seen[index] = true;
            let (key, value) = expected.iter().nth(index).ok_or_else(mismatch)?;
            map.next_value_seed(Compare {
                expected: value,
                date: matches!(key.as_str(), "start" | "finish" | "valid_until"),
            })?;
        }
        if seen.into_iter().all(|present| present) {
            Ok(())
        } else {
            Err(mismatch())
        }
    }
}

struct Reject;
impl<'de> DeserializeSeed<'de> for Reject {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, _: D) -> Result<(), D::Error> {
        Err(mismatch())
    }
}
struct Key<'a>(&'a Map<String, Value>);
impl<'de> DeserializeSeed<'de> for Key<'_> {
    type Value = usize;
    fn deserialize<D: de::Deserializer<'de>>(self, decoder: D) -> Result<usize, D::Error> {
        decoder.deserialize_str(self)
    }
}
impl Visitor<'_> for Key<'_> {
    type Value = usize;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a known effective original field")
    }
    fn visit_str<E: de::Error>(self, value: &str) -> Result<usize, E> {
        self.0
            .keys()
            .position(|key| key == value)
            .ok_or_else(mismatch)
    }
}

const ROOT_KEYS: [&str; 12] = [
    "format_version",
    "effective_selection",
    "producer_bindings",
    "generation",
    "entry_addon_id",
    "entry",
    "v1_region",
    "edit_policy",
    "resource_transmissions",
    "documents",
    "guide_inputs",
    "guide_source",
];
struct Root<'a>(&'a Value);
impl<'de> DeserializeSeed<'de> for Root<'_> {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, decoder: D) -> Result<(), D::Error> {
        decoder.deserialize_map(self)
    }
}
impl<'de> Visitor<'de> for Root<'_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an effective original envelope")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        let mut seen = [false; 12];
        while let Some(index) = map.next_key_seed(RootKey)? {
            if seen[index] {
                return Err(mismatch());
            }
            seen[index] = true;
            match index {
                0 => map.next_value_seed(Compare {
                    expected: &json!(2),
                    date: false,
                })?,
                1 => map.next_value_seed(Compare {
                    expected: self.0,
                    date: false,
                })?,
                _ => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        if seen[0] && seen[1] {
            Ok(())
        } else {
            Err(mismatch())
        }
    }
}
struct RootKey;
impl<'de> DeserializeSeed<'de> for RootKey {
    type Value = usize;
    fn deserialize<D: de::Deserializer<'de>>(self, decoder: D) -> Result<usize, D::Error> {
        decoder.deserialize_str(self)
    }
}
impl Visitor<'_> for RootKey {
    type Value = usize;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a known effective envelope field")
    }
    fn visit_str<E: de::Error>(self, value: &str) -> Result<usize, E> {
        ROOT_KEYS
            .iter()
            .position(|key| *key == value)
            .ok_or_else(mismatch)
    }
}
