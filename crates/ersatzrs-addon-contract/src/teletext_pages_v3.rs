//! Explicitly negotiated authoring metadata for an unchanged authored v2 service.
//!
//! Metadata describes the addon's template, theme and edit boundaries. It grants
//! no editing or publication authority. The host must retain its original receipt
//! and validate rendered/resource effects before accepting any imported change.

use std::{collections::BTreeSet, fmt};

use crate::{
    TeletextPageAddressWire, TeletextPageDraftWire, TeletextPagesError, TeletextPagesV2Error,
    TeletextPagesV2Response, MAX_TELETEXT_PAGES_OUTPUT_BYTES, MAX_TELETEXT_PAGES_RECORDS,
};

/// Response version selected by the host, without fallback after a v3 failure.
pub const TELETEXT_PAGES_V3_RESPONSE_SCHEMA_VERSION: u32 = 3;
/// Metadata version independent of response and saved-design envelope versions.
pub const TELETEXT_AUTHORING_METADATA_SCHEMA_VERSION: u32 = 1;
/// Maximum symbolic identifier bytes; paths and control characters are forbidden.
pub const MAX_TELETEXT_AUTHORING_ID_BYTES: usize = 128;
/// Maximum theme overrides or per-page regions and dynamic fields.
pub const MAX_TELETEXT_AUTHORING_PAGE_RECORDS: usize = 256;
/// Maximum regions or dynamic fields across the complete contribution.
pub const MAX_TELETEXT_AUTHORING_TOTAL_RECORDS: usize = 4096;

// Every metadata object is map-only and closed. Debug never exposes arbitrary
// addon identifiers, bindings, override values or authored page contents.
macro_rules! authoring_object {
    ($(#[$meta:meta])* pub struct $name:ident {
        $($(#[$field_meta:meta])* pub $field:ident: $ty:ty),* $(,)?
    }) => {
        #[derive(Clone, PartialEq, Eq, serde::Serialize)]
        $(#[$meta])*
        pub struct $name { $($(#[$field_meta])* pub $field: $ty),* }
        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(concat!(stringify!($name), "(<redacted>)"))
            }
        }
        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self,D::Error> {
                #[derive(serde::Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Fields { $($(#[$field_meta])* $field: $ty),* }
                let fields: Fields = crate::teletext_pages_v2::deserialize_object(deserializer)?;
                Ok(Self { $($field: fields.$field),* })
            }
        }
    };
}

macro_rules! authoring_string_enum {
    ($(#[$meta:meta])* pub enum $name:ident { $($variant:ident),* $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
        #[serde(rename_all = "snake_case")]
        $(#[$meta])*
        pub enum $name { $($variant),* }
        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self,D::Error> {
                #[derive(serde::Deserialize)]
                #[serde(rename_all = "snake_case")]
                enum Fields { $($variant),* }
                let value = <String as serde::Deserialize>::deserialize(deserializer)?;
                let fields = <Fields as serde::Deserialize>::deserialize(
                    serde::de::value::StringDeserializer::<D::Error>::new(value))?;
                Ok(match fields { $(Fields::$variant => Self::$variant),* })
            }
        }
    };
}

authoring_object! {
    /// Complete unpublished authored service and addon-owned metadata.
    pub struct TeletextPagesV3Response {
        pub schema_version: u32,
        pub service: TeletextPagesV2Response,
        pub authoring: TeletextAuthoringMetadataWire,
    }
}

authoring_object! {
    /// Descriptive edit boundaries captured with the original producer response.
    pub struct TeletextAuthoringMetadataWire {
        pub schema_version: u32,
        pub template: TeletextAuthoringTemplateWire,
        pub theme: TeletextAuthoringThemeWire,
        pub pages: Vec<TeletextAuthoringPageWire>,
    }
}
authoring_object! {
    /// Template identity within the installed addon version, not a filesystem path.
    pub struct TeletextAuthoringTemplateWire { pub id: String, pub revision: String }
}
authoring_object! {
    /// Selected named theme and explicit typed overrides.
    pub struct TeletextAuthoringThemeWire {
        pub id: String,
        pub overrides: Vec<TeletextAuthoringThemeOverrideWire>,
    }
}
authoring_object! {
    /// One uniquely named presentation override, without executable settings.
    pub struct TeletextAuthoringThemeOverrideWire {
        pub key: String,
        pub value: TeletextAuthoringThemeOverrideValueWire,
    }
}

/// Closed typed presentation value. Colours use the Level 2.5 four-bit components.
#[derive(Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TeletextAuthoringThemeOverrideValueWire {
    Colour { red: u8, green: u8, blue: u8 },
    Layout { id: String },
    Number { value: i64 },
    Boolean { value: bool },
}
impl fmt::Debug for TeletextAuthoringThemeOverrideValueWire {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TeletextAuthoringThemeOverrideValueWire(<redacted>)")
    }
}
impl<'de> serde::Deserialize<'de> for TeletextAuthoringThemeOverrideValueWire {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Colour { red: u8, green: u8, blue: u8 },
            Layout { id: String },
            Number { value: i64 },
            Boolean { value: bool },
        }
        let fields: Fields = crate::teletext_pages_v2::deserialize_object(deserializer)?;
        Ok(match fields {
            Fields::Colour { red, green, blue } => Self::Colour { red, green, blue },
            Fields::Layout { id } => Self::Layout { id },
            Fields::Number { value } => Self::Number { value },
            Fields::Boolean { value } => Self::Boolean { value },
        })
    }
}
authoring_object! {
    /// Boundaries for one exact display page and subpage; omission protects cells.
    pub struct TeletextAuthoringPageWire {
        #[serde(deserialize_with = "crate::teletext_pages_v2::deserialize_object")]
        pub page: TeletextPageAddressWire,
        pub regions: Vec<TeletextAuthoringRegionWire>,
        pub dynamic_fields: Vec<TeletextAuthoringDynamicFieldWire>,
    }
}
authoring_object! {
    /// Nonoverlapping rectangle in the local coordinates of its display area.
    pub struct TeletextAuthoringRegionWire {
        pub id: String,
        pub kind: TeletextAuthoringRegionKindWire,
        pub area: TeletextAuthoringRegionAreaWire,
        pub row: u8,
        pub column: u8,
        pub width: u8,
        pub height: u8,
    }
}
authoring_string_enum! {
    /// Declared editorial ownership; the original receipt remains authoritative.
    pub enum TeletextAuthoringRegionKindWire { Free, Zone, Generated }
}
authoring_string_enum! {
    /// Main display or enhanced side panel with local rectangle coordinates.
    pub enum TeletextAuthoringRegionAreaWire { Main, LeftPanel, RightPanel }
}
authoring_object! {
    /// Symbolic generated-field binding; never a command or uploaded expression.
    pub struct TeletextAuthoringDynamicFieldWire {
        pub id: String,
        pub region: String,
        pub binding: String,
    }
}

/// Static metadata rejection with optional zero-based collection positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{kind} (page index {page:?}, record index {record:?})")]
pub struct TeletextAuthoringMetadataError {
    pub kind: TeletextAuthoringMetadataErrorKind,
    pub page: Option<usize>,
    pub record: Option<usize>,
}
/// Content-free metadata rejection reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextAuthoringMetadataErrorKind {
    #[error("unsupported authoring metadata schema")]
    UnsupportedSchema,
    #[error("invalid authoring symbolic identifier")]
    Identifier,
    #[error("authoring metadata collection exceeds its limit")]
    RecordCount,
    #[error("duplicate authoring identifier")]
    DuplicateIdentifier,
    #[error("theme colour component exceeds four bits")]
    Colour,
    #[error("authoring pages do not exactly cover original display addresses")]
    PageCoverage,
    #[error("authoring rectangle is empty or outside its display area")]
    Rectangle,
    #[error("editable authoring rectangle covers receiver-owned header cells")]
    Header,
    #[error("authoring rectangles overlap within one display area")]
    Overlap,
    #[error("dynamic field does not reference a generated region")]
    DynamicBinding,
}
/// Static envelope rejection; authored identifiers and content are never retained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextPagesV3Error {
    #[error(transparent)]
    Contract(#[from] TeletextPagesError),
    #[error(transparent)]
    Service(#[from] TeletextPagesV2Error),
    #[error(transparent)]
    Authoring(#[from] TeletextAuthoringMetadataError),
}

impl TeletextPagesV3Response {
    /// Parse a closed bounded v3 envelope, preserving all existing v2 fences.
    ///
    /// Raw bytes are capped before both duplicate-key and typed parsing passes.
    /// Semantic compilation and imported-change authorization remain host duties.
    /// # Errors
    /// Rejects malformed, oversized, duplicate, unsupported or mismatched input,
    /// and invalid metadata coverage, identifiers, rectangles or bindings.
    pub fn from_json(
        bytes: &[u8],
        expected_generation: &str,
    ) -> Result<Self, TeletextPagesV3Error> {
        if bytes.len() > MAX_TELETEXT_PAGES_OUTPUT_BYTES {
            return Err(TeletextPagesError::OutputTooLarge.into());
        }
        std::str::from_utf8(bytes).map_err(|_| TeletextPagesError::InvalidUtf8)?;
        crate::teletext_pages::reject_duplicate_keys(bytes)?;
        let response: Self =
            serde_json::from_slice(bytes).map_err(|_| TeletextPagesError::InvalidJson)?;
        if response.schema_version != TELETEXT_PAGES_V3_RESPONSE_SCHEMA_VERSION {
            return Err(TeletextPagesError::UnsupportedSchema.into());
        }
        response.service.validate_wire_bounds(expected_generation)?;
        response
            .authoring
            .validate_against(&response.service.pages)?;
        Ok(response)
    }
}

fn symbolic_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_TELETEXT_AUTHORING_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}
fn rejection(
    kind: TeletextAuthoringMetadataErrorKind,
    page: Option<usize>,
    record: Option<usize>,
) -> TeletextAuthoringMetadataError {
    TeletextAuthoringMetadataError { kind, page, record }
}

impl TeletextAuthoringMetadataWire {
    /// Validate descriptive metadata against all exact original display identities.
    ///
    /// Side-panel geometry is local; actual panel availability and indirect object,
    /// DRCS, palette or packet effects require the host's compiled-model validation.
    /// # Errors
    /// Returns static indexed errors for schema, identifiers, collection limits,
    /// coverage, overlapping/out-of-range rectangles or generated-field bindings.
    pub fn validate_against(
        &self,
        original: &[TeletextPageDraftWire],
    ) -> Result<(), TeletextAuthoringMetadataError> {
        use TeletextAuthoringMetadataErrorKind as K;
        if self.schema_version != TELETEXT_AUTHORING_METADATA_SCHEMA_VERSION {
            return Err(rejection(K::UnsupportedSchema, None, None));
        }
        if !symbolic_id(&self.template.id)
            || !symbolic_id(&self.template.revision)
            || !symbolic_id(&self.theme.id)
        {
            return Err(rejection(K::Identifier, None, None));
        }
        validate_theme_overrides(&self.theme)?;
        if self.pages.is_empty()
            || self.pages.len() > MAX_TELETEXT_PAGES_RECORDS
            || self.pages.len() != original.len()
        {
            return Err(rejection(K::PageCoverage, None, None));
        }
        let original_addresses: BTreeSet<_> = original
            .iter()
            .map(|page| (page.address.page, page.address.subcode))
            .collect();
        if original_addresses.len() != original.len() {
            return Err(rejection(K::PageCoverage, None, None));
        }
        let mut addresses = BTreeSet::new();
        let mut total_regions = 0usize;
        let mut total_fields = 0usize;
        for (page_index, page) in self.pages.iter().enumerate() {
            let position = Some(page_index);
            let address = (page.page.page, page.page.subcode);
            if !original_addresses.contains(&address) || !addresses.insert(address) {
                return Err(rejection(K::PageCoverage, position, None));
            }
            if page.regions.len() > MAX_TELETEXT_AUTHORING_PAGE_RECORDS
                || page.dynamic_fields.len() > MAX_TELETEXT_AUTHORING_PAGE_RECORDS
            {
                return Err(rejection(K::RecordCount, position, None));
            }
            // Each addend is at most 256, with at most 1024 display pages;
            // these additions are bounded on every supported native target.
            total_regions += page.regions.len();
            total_fields += page.dynamic_fields.len();
            if total_regions > MAX_TELETEXT_AUTHORING_TOTAL_RECORDS
                || total_fields > MAX_TELETEXT_AUTHORING_TOTAL_RECORDS
            {
                return Err(rejection(K::RecordCount, position, None));
            }
            validate_regions(page, position)?;
            validate_dynamic_fields(page, position)?;
        }
        Ok(())
    }
}

fn validate_theme_overrides(
    theme: &TeletextAuthoringThemeWire,
) -> Result<(), TeletextAuthoringMetadataError> {
    use TeletextAuthoringMetadataErrorKind as K;
    if theme.overrides.len() > MAX_TELETEXT_AUTHORING_PAGE_RECORDS {
        return Err(rejection(K::RecordCount, None, None));
    }
    let mut keys = BTreeSet::new();
    for (index, item) in theme.overrides.iter().enumerate() {
        if !symbolic_id(&item.key) {
            return Err(rejection(K::Identifier, None, Some(index)));
        }
        if !keys.insert(item.key.as_str()) {
            return Err(rejection(K::DuplicateIdentifier, None, Some(index)));
        }
        match &item.value {
            TeletextAuthoringThemeOverrideValueWire::Colour { red, green, blue }
                if [red, green, blue]
                    .into_iter()
                    .any(|component| *component > 15) =>
            {
                return Err(rejection(K::Colour, None, Some(index)))
            }
            TeletextAuthoringThemeOverrideValueWire::Layout { id } if !symbolic_id(id) => {
                return Err(rejection(K::Identifier, None, Some(index)))
            }
            _ => {}
        }
    }
    Ok(())
}

fn validate_regions(
    page: &TeletextAuthoringPageWire,
    position: Option<usize>,
) -> Result<(), TeletextAuthoringMetadataError> {
    use TeletextAuthoringMetadataErrorKind as K;
    let mut region_ids = BTreeSet::new();
    for (index, region) in page.regions.iter().enumerate() {
        let record = Some(index);
        if !symbolic_id(&region.id) {
            return Err(rejection(K::Identifier, position, record));
        }
        if !region_ids.insert(region.id.as_str()) {
            return Err(rejection(K::DuplicateIdentifier, position, record));
        }
        let columns = if region.area == TeletextAuthoringRegionAreaWire::Main {
            40u16
        } else {
            16
        };
        if region.width == 0
            || region.height == 0
            || u16::from(region.column) + u16::from(region.width) > columns
            || u16::from(region.row) + u16::from(region.height) > 25
        {
            return Err(rejection(K::Rectangle, position, record));
        }
        if region.kind != TeletextAuthoringRegionKindWire::Generated
            && region.area == TeletextAuthoringRegionAreaWire::Main
            && region.row == 0
            && region.column < 8
        {
            return Err(rejection(K::Header, position, record));
        }
        if page.regions[..index]
            .iter()
            .any(|previous| regions_overlap(previous, region))
        {
            return Err(rejection(K::Overlap, position, record));
        }
    }
    Ok(())
}

fn validate_dynamic_fields(
    page: &TeletextAuthoringPageWire,
    position: Option<usize>,
) -> Result<(), TeletextAuthoringMetadataError> {
    use TeletextAuthoringMetadataErrorKind as K;
    let mut field_ids = BTreeSet::new();
    for (index, field) in page.dynamic_fields.iter().enumerate() {
        let record = Some(index);
        if !symbolic_id(&field.id) || !symbolic_id(&field.region) || !symbolic_id(&field.binding) {
            return Err(rejection(K::Identifier, position, record));
        }
        if !field_ids.insert(field.id.as_str()) {
            return Err(rejection(K::DuplicateIdentifier, position, record));
        }
        if !page.regions.iter().any(|region| {
            region.id == field.region && region.kind == TeletextAuthoringRegionKindWire::Generated
        }) {
            return Err(rejection(K::DynamicBinding, position, record));
        }
    }
    Ok(())
}

fn regions_overlap(
    first: &TeletextAuthoringRegionWire,
    second: &TeletextAuthoringRegionWire,
) -> bool {
    first.area == second.area
        && u16::from(first.column) < u16::from(second.column) + u16::from(second.width)
        && u16::from(second.column) < u16::from(first.column) + u16::from(first.width)
        && u16::from(first.row) < u16::from(second.row) + u16::from(second.height)
        && u16::from(second.row) < u16::from(first.row) + u16::from(first.height)
}

#[cfg(test)]
mod tests;
