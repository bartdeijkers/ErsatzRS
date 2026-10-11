//! Checked authored resource `nnXs` and lifecycle metadata, EN 300 706 Annex A.1,
//! Annex B.6/table B.1 and clauses 10.5.1, 10.5.2, 10.6.1 and 11.3.2.
//!
//! Logical identity is independent of the last-packet declaration and update
//! counter. These values do not prove actual body extent, prior update history,
//! fragment continuity, acquisition or encoding readiness. Header text, service
//! C11 and non-lifecycle controls are deliberately outside this type. Reserved
//! C14 is not a national-option bit for resources and is not authored here.

use std::fmt;

use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Serialize};

use super::{TeletextResourceIdentity, TeletextResourceKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextResourceHeaderError {
    #[error("teletext resource last-packet value {last} must be between one and 41")]
    LastPacket { last: u8 },
    #[error("teletext resource update counter {counter} must be between zero and seven")]
    UpdateCounter { counter: u8 },
    #[error(
        "teletext resource {identity:?} last-packet value {last} exceeds kind maximum {maximum}"
    )]
    KindLastPacket {
        identity: TeletextResourceIdentity,
        last: u8,
        maximum: u8,
    },
}

/// Last data packet in prescribed transmission order, not the highest packet address.
/// Values 1–25 identify X/1–X/25; 26–41 identify X/26/0–X/26/15.
/// Preceding X/27 and X/28 packets do not increase this value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct TeletextResourceLastPacket(u8);

impl TeletextResourceLastPacket {
    /// # Errors
    /// Rejects values outside 1–41 without masking or substituting a packet.
    pub fn new(last: u8) -> Result<Self, TeletextResourceHeaderError> {
        if (1..=41).contains(&last) {
            Ok(Self(last))
        } else {
            Err(TeletextResourceHeaderError::LastPacket { last })
        }
    }
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}
impl TryFrom<u8> for TeletextResourceLastPacket {
    type Error = TeletextResourceHeaderError;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<TeletextResourceLastPacket> for u8 {
    fn from(value: TeletextResourceLastPacket) -> Self {
        value.get()
    }
}

/// Authored S2 indicator, independent of logical S1 and previous transmission state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct TeletextResourceUpdateCounter(u8);

impl TeletextResourceUpdateCounter {
    /// # Errors
    /// Rejects values outside 0–7; omitted counters have no implicit value.
    pub fn new(counter: u8) -> Result<Self, TeletextResourceHeaderError> {
        if counter <= 7 {
            Ok(Self(counter))
        } else {
            Err(TeletextResourceHeaderError::UpdateCounter { counter })
        }
    }
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
    /// Pure explicit increment; choosing when to advance belongs to the update owner.
    #[must_use]
    pub const fn next_modulo_eight(self) -> Self {
        Self((self.0 + 1) & 7)
    }
}
impl TryFrom<u8> for TeletextResourceUpdateCounter {
    type Error = TeletextResourceHeaderError;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<TeletextResourceUpdateCounter> for u8 {
    fn from(value: TeletextResourceUpdateCounter) -> Self {
        value.get()
    }
}

/// Six table B.1 lifecycle declarations. First changed/partial transmissions
/// increment S2; continuations retain it. This type does not validate that history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TeletextResourceTransmission {
    FullUnchangedFirst,
    FullUnchangedContinuation,
    FullChangedFirst,
    FullChangedContinuation,
    PartialFirst,
    PartialContinuation,
}

impl TeletextResourceTransmission {
    /// Resource C4, set only for a changed full page's first transmission.
    #[must_use]
    pub const fn erase_page(self) -> bool {
        matches!(self, Self::FullChangedFirst)
    }
    /// Resource C8; unchanged full transmissions are the only clear states.
    #[must_use]
    pub const fn update_indicator(self) -> bool {
        !matches!(
            self,
            Self::FullUnchangedFirst | Self::FullUnchangedContinuation
        )
    }
    /// Resource C12; a continuation never implies a fresh counter increment.
    #[must_use]
    pub const fn continuation(self) -> bool {
        matches!(
            self,
            Self::FullUnchangedContinuation
                | Self::FullChangedContinuation
                | Self::PartialContinuation
        )
    }
    /// Resource C13; partial transmissions are changed without page erasure.
    #[must_use]
    pub const fn partial(self) -> bool {
        matches!(self, Self::PartialFirst | Self::PartialContinuation)
    }
}

impl<'de> Deserialize<'de> for TeletextResourceTransmission {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct TransmissionVisitor;
        impl Visitor<'_> for TransmissionVisitor {
            type Value = TeletextResourceTransmission;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("an assigned teletext resource transmission string")
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                match value {
                    "full_unchanged_first" => Ok(TeletextResourceTransmission::FullUnchangedFirst),
                    "full_unchanged_continuation" => {
                        Ok(TeletextResourceTransmission::FullUnchangedContinuation)
                    }
                    "full_changed_first" => Ok(TeletextResourceTransmission::FullChangedFirst),
                    "full_changed_continuation" => {
                        Ok(TeletextResourceTransmission::FullChangedContinuation)
                    }
                    "partial_first" => Ok(TeletextResourceTransmission::PartialFirst),
                    "partial_continuation" => Ok(TeletextResourceTransmission::PartialContinuation),
                    _ => Err(E::custom("unknown teletext resource transmission")),
                }
            }
        }
        deserializer.deserialize_str(TransmissionVisitor)
    }
}

/// Only physical identity/lifecycle metadata, not a complete authored header.
/// Actual bodies, opaque controls and service-wide mode remain separate inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TeletextResourceHeaderMetadata {
    identity: TeletextResourceIdentity,
    last_packet: TeletextResourceLastPacket,
    update_counter: TeletextResourceUpdateCounter,
    transmission: TeletextResourceTransmission,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HeaderFields {
    identity: TeletextResourceIdentity,
    last_packet: TeletextResourceLastPacket,
    update_counter: TeletextResourceUpdateCounter,
    transmission: TeletextResourceTransmission,
}

impl TeletextResourceHeaderMetadata {
    /// # Errors
    /// Object pages allow last values 1–41; DRCS, MOT and MIP allow 1–24.
    /// The identity preserves MOT's advisory nonzero S1 and MIP's checked fixed zero.
    /// No actual body, previous counter or transmission interval is supplied.
    pub fn new(
        identity: TeletextResourceIdentity,
        last_packet: TeletextResourceLastPacket,
        update_counter: TeletextResourceUpdateCounter,
        transmission: TeletextResourceTransmission,
    ) -> Result<Self, TeletextResourceHeaderError> {
        let maximum = match identity.kind() {
            TeletextResourceKind::Pop | TeletextResourceKind::Gpop => 41,
            TeletextResourceKind::Drcs
            | TeletextResourceKind::Gdrcs
            | TeletextResourceKind::Mot
            | TeletextResourceKind::Mip => 24,
        };
        if last_packet.get() > maximum {
            return Err(TeletextResourceHeaderError::KindLastPacket {
                identity,
                last: last_packet.get(),
                maximum,
            });
        }
        Ok(Self {
            identity,
            last_packet,
            update_counter,
            transmission,
        })
    }
    #[must_use]
    pub const fn identity(self) -> TeletextResourceIdentity {
        self.identity
    }
    #[must_use]
    pub const fn last_packet(self) -> TeletextResourceLastPacket {
        self.last_packet
    }
    #[must_use]
    pub const fn update_counter(self) -> TeletextResourceUpdateCounter {
        self.update_counter
    }
    #[must_use]
    pub const fn transmission(self) -> TeletextResourceTransmission {
        self.transmission
    }
    /// S4/S3 carry nn, S2 carries the explicit counter, S1 carries the logical identity.
    /// Header controls C4/C5/C6 are not mixed into this numeric subcode.
    #[must_use]
    pub fn physical_subcode(self) -> u16 {
        (u16::from(self.last_packet.get()) << 8)
            | (u16::from(self.update_counter.get()) << 4)
            | u16::from(self.identity.logical_subpage().get())
    }
}

impl<'de> Deserialize<'de> for TeletextResourceHeaderMetadata {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct HeaderVisitor;
        impl<'de> Visitor<'de> for HeaderVisitor {
            type Value = HeaderFields;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a teletext resource header metadata object")
            }
            fn visit_map<A: MapAccess<'de>>(self, access: A) -> Result<Self::Value, A::Error> {
                HeaderFields::deserialize(serde::de::value::MapAccessDeserializer::new(access))
            }
        }
        let fields = deserializer.deserialize_map(HeaderVisitor)?;
        Self::new(
            fields.identity,
            fields.last_packet,
            fields.update_counter,
            fields.transmission,
        )
        .map_err(serde::de::Error::custom)
    }
}
