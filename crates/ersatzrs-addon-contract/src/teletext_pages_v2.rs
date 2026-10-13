//! Preparatory `teletext.pages.v2` authored Level 2.5 wire document (ADR 0123).
//!
//! The unchanged v1 fallback is mandatory. Enhanced declarations and resolved
//! assertions remain producer input: only the host may authenticate their
//! correspondence, apply work budgets, encode resources or authorize publication.
//! This module has no domain, application, storage or transport dependency.

use std::{fmt, marker::PhantomData};

use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer};

use crate::{
    TeletextPageAddressWire, TeletextPageDraftWire, TeletextPagesError,
    MAX_TELETEXT_PAGES_GENERATION_BYTES, MAX_TELETEXT_PAGES_OUTPUT_BYTES,
    MAX_TELETEXT_PAGES_RECORDS,
};

// Closed map-only DTOs avoid Serde's positional-array struct representation.
// The local Fields type is not public and cannot bypass this wire boundary.
macro_rules! wire_object {
    ($(#[$meta:meta])* pub struct $name:ident {
        $($(#[$field_meta:meta])* pub $field:ident: $ty:ty),* $(,)?
    }) => {
        #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
        $(#[$meta])*
        pub struct $name { $($(#[$field_meta])* pub $field: $ty),* }

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                #[derive(serde::Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Fields { $($(#[$field_meta])* $field: $ty),* }
                let fields: Fields = crate::teletext_pages_v2::deserialize_object(deserializer)?;
                Ok(Self { $($field: fields.$field),* })
            }
        }
    };
}

// Internally tagged Serde enums also accept positional sequences by default.
// Restrict enhanced variants to maps without changing the legacy v1 parser.
macro_rules! wire_tagged_enum {
    ($(#[$meta:meta])* pub enum $name:ident {
        $($variant:ident { $($(#[$field_meta:meta])* $field:ident: $ty:ty),* $(,)? }),* $(,)?
    }) => {
        #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
        $(#[$meta])*
        pub enum $name { $($variant { $($(#[$field_meta])* $field: $ty),* }),* }
        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                #[derive(serde::Deserialize)]
                $(#[$meta])*
                enum Fields { $($variant { $($(#[$field_meta])* $field: $ty),* }),* }
                let fields: Fields = crate::teletext_pages_v2::deserialize_object(deserializer)?;
                Ok(match fields { $(Fields::$variant { $($field),* } => Self::$variant { $($field),* }),* })
            }
        }
    };
}

// Unit enums are strings on the wire, not externally tagged one-entry maps.
macro_rules! wire_string_enum {
    ($(#[$meta:meta])* pub enum $name:ident { $($variant:ident),* $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
        $(#[$meta])*
        pub enum $name { $($variant),* }
        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                #[derive(serde::Deserialize)]
                $(#[$meta])*
                enum Fields { $($variant),* }
                let value = <String as serde::Deserialize>::deserialize(deserializer)?;
                let fields = <Fields as serde::Deserialize>::deserialize(
                    serde::de::value::StringDeserializer::<D::Error>::new(value))?;
                Ok(match fields { $(Fields::$variant => Self::$variant),* })
            }
        }
    };
}

mod presentation;
mod program;
mod resources;
pub use presentation::*;
pub use program::*;
pub use resources::*;

/// Version of the enhanced response, independent of v1 and request versions.
pub const TELETEXT_PAGES_V2_RESPONSE_SCHEMA_VERSION: u32 = 2;

/// Static wire rejection without authored text, keys or correlation values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextPagesV2Error {
    #[error(transparent)]
    Contract(#[from] TeletextPagesError),
    #[error("teletext enhanced response exceeds a raw collection record limit")]
    RecordCount,
}

wire_object! {
    /// Complete unpublished contribution; every collection is required, even empty.
    pub struct TeletextPagesV2Response {
        pub schema_version: u32,
        pub generation: String,
        pub language: String,
        pub initial_page: u16,
        /// Mandatory ordinary-client fallback, retaining the exact v1 shape.
        pub pages: Vec<TeletextPageDraftWire>,
        pub page_enhancements: Vec<TeletextPageEnhancementWire>,
        pub magazines: Vec<TeletextMagazinePresentationWire>,
        pub objects: Vec<TeletextEnhancementObjectDefinitionWire>,
        pub drcs: Vec<TeletextDrcsSubtableWire>,
        pub mots: Vec<TeletextMotLevel25DeclarationWire>,
        pub mot_selections: Vec<TeletextDisplayMotSelectionWire>,
        /// Explicit empty POP/GPOP companions; not invented from incomplete input.
        pub empty_object_pages: Vec<TeletextResourceIdentityWire>,
    }
}

impl TeletextPagesV2Response {
    /// Parse strictly v2 with an exact host correlation echo and existing raw cap.
    ///
    /// Duplicate keys are rejected before constructing DTOs, including in tagged
    /// commands. The default JSON recursion limit remains enabled. Each raw
    /// collection is fenced before domain conversion; this does not bound total
    /// parser allocations, RSS, command expansion or encoded output. The host
    /// must separately apply its semantic/declaration/compiler budgets.
    ///
    /// # Errors
    /// Rejects oversized/non-UTF-8/duplicate/malformed input, wrong schemas,
    /// invalid/mismatched correlation and record overflow. No v1 retry occurs.
    pub fn from_json(
        bytes: &[u8],
        expected_generation: &str,
    ) -> Result<Self, TeletextPagesV2Error> {
        if bytes.len() > MAX_TELETEXT_PAGES_OUTPUT_BYTES {
            return Err(TeletextPagesError::OutputTooLarge.into());
        }
        std::str::from_utf8(bytes).map_err(|_| TeletextPagesError::InvalidUtf8)?;
        crate::teletext_pages::reject_duplicate_keys(bytes)?;
        let response: Self =
            serde_json::from_slice(bytes).map_err(|_| TeletextPagesError::InvalidJson)?;
        response.validate_wire_bounds(expected_generation)?;
        Ok(response)
    }

    /// Apply the unchanged v2 bounds inside a separately negotiated envelope.
    pub(crate) fn validate_wire_bounds(
        &self,
        expected_generation: &str,
    ) -> Result<(), TeletextPagesV2Error> {
        if self.schema_version != TELETEXT_PAGES_V2_RESPONSE_SCHEMA_VERSION {
            return Err(TeletextPagesError::UnsupportedSchema.into());
        }
        for generation in [expected_generation, self.generation.as_str()] {
            if generation.is_empty() || generation.len() > MAX_TELETEXT_PAGES_GENERATION_BYTES {
                return Err(TeletextPagesError::InvalidGeneration.into());
            }
        }
        if self.generation != expected_generation {
            return Err(TeletextPagesError::GenerationMismatch.into());
        }
        if !(1..=MAX_TELETEXT_PAGES_RECORDS).contains(&self.pages.len()) {
            return Err(TeletextPagesError::PageCount.into());
        }
        // Bound raw collections without charging page metadata as additional
        // pages. Repeated identities still count; host semantic/declaration
        // budgets independently bound aggregate work after wire parsing.
        for count in [
            self.page_enhancements.len(),
            self.magazines.len(),
            self.objects.len(),
            self.drcs.len(),
            self.mots.len(),
            self.mot_selections.len(),
            self.empty_object_pages.len(),
        ] {
            if count > MAX_TELETEXT_PAGES_RECORDS {
                return Err(TeletextPagesV2Error::RecordCount);
            }
        }
        Ok(())
    }
}

wire_object! {
    /// Declared packet presence and resolved assertions for one exact display.
    pub struct TeletextPageEnhancementWire {
        #[serde(deserialize_with = "deserialize_object")]
        pub address: TeletextPageAddressWire,
        #[serde(deserialize_with = "required_nullable")]
        pub presentation: Option<TeletextLevel25PresentationWire>,
        #[serde(deserialize_with = "required_nullable")]
        pub compositional_links: Option<TeletextX27Level25LinksWire>,
        /// Null and a present empty program have different default-object semantics.
        #[serde(deserialize_with = "required_nullable")]
        pub local_program: Option<TeletextEnhancementProgramWire>,
        pub resources: TeletextResolvedPageResourceSelectionWire,
    }
}

wire_object! {
    /// One explicitly authored magazine presentation; absence is no declaration.
    pub struct TeletextMagazinePresentationWire {
        pub magazine: u8,
        pub presentation: TeletextLevel25PresentationWire,
    }
}

fn required_nullable<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::deserialize(deserializer)
}

pub(crate) fn deserialize_object<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct ObjectVisitor<T>(PhantomData<T>);
    impl<'de, T: Deserialize<'de>> Visitor<'de> for ObjectVisitor<T> {
        type Value = T;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a teletext enhanced wire object")
        }
        fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<T, A::Error> {
            T::deserialize(serde::de::value::MapAccessDeserializer::new(map))
        }
    }
    deserializer.deserialize_map(ObjectVisitor(PhantomData))
}

#[cfg(test)]
mod tests;
