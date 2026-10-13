//! Bounded interchange syntax for the pinned external-editor project.
//!
//! Parsed projects and sidecars are untrusted claims. Only the native host can
//! establish hashes, original-document ownership and supported rendered effects.
//! The private receipt body retains exact files; the public sidecar never does.

use crate::{
    teletext_design_documents::TeletextSavedV1Region,
    teletext_tti::{parse_teletext_tti, TeletextTtiBudget, TeletextTtiRecord},
};
use ersatzrs_addon_contract::{
    TeletextAuthoringMetadataWire, TeletextNavigationLinkWire, TeletextPageAddressWire,
    TeletextPageDraftWire, TeletextPageKindWire, TeletextResourceIdentityWire,
};
use serde::{
    de::{DeserializeOwned, DeserializeSeed, MapAccess, SeqAccess, Visitor},
    Deserialize, Deserializer, Serialize,
};
use std::{cell::Cell, collections::BTreeSet, fmt, io};

/// Explicit caller limits, checked before allocating or encoding document bodies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextExportProjectBudget {
    pub max_bytes: usize,
    pub max_files: usize,
    pub max_producers: usize,
    pub max_file_bytes: usize,
    pub max_units: usize,
}

/// Static rejection category; no authored text, paths or hashes are exposed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextExportProjectFailure {
    Budget,
    Bytes,
    Shape,
    Depth,
    Duplicate,
    Limit,
    Schema,
    Identity,
    Mapping,
    Metadata,
    Files,
    Tti,
}

/// Optional zero-based producer, page, asset or file position in a rejection.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("teletext editor project {kind:?} at item {item:?}")]
pub struct TeletextExportProjectError {
    pub kind: TeletextExportProjectFailure,
    pub item: Option<usize>,
}
fn failure(kind: TeletextExportProjectFailure) -> TeletextExportProjectError {
    TeletextExportProjectError { kind, item: None }
}
fn indexed(kind: TeletextExportProjectFailure, item: usize) -> TeletextExportProjectError {
    TeletextExportProjectError {
        kind,
        item: Some(item),
    }
}

type Result<T> = std::result::Result<T, TeletextExportProjectError>;
fn map_only<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> std::result::Result<T, D::Error> {
    struct Object<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> Visitor<'de> for Object<T> {
        type Value = T;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("closed object")
        }
        fn visit_map<A: MapAccess<'de>>(self, map: A) -> std::result::Result<T, A::Error> {
            T::deserialize(serde::de::value::MapAccessDeserializer::new(map))
        }
    }
    deserializer.deserialize_map(Object(std::marker::PhantomData))
}
fn nullable<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> std::result::Result<Option<T>, D::Error> {
    Option::<T>::deserialize(d)
}
fn page_kind<'de, D: Deserializer<'de>>(
    d: D,
) -> std::result::Result<TeletextPageKindWire, D::Error> {
    let value = String::deserialize(d)?;
    TeletextPageKindWire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))
}
fn navigation_role<'de, D: Deserializer<'de>>(
    d: D,
) -> std::result::Result<ersatzrs_addon_contract::TeletextNavigationRoleWire, D::Error> {
    let value = String::deserialize(d)?;
    ersatzrs_addon_contract::TeletextNavigationRoleWire::deserialize(
        serde::de::value::StringDeserializer::<D::Error>::new(value),
    )
}
fn navigation<'de, D: Deserializer<'de>>(
    d: D,
) -> std::result::Result<Vec<TeletextNavigationLinkWire>, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Link {
        #[serde(deserialize_with = "navigation_role")]
        role: ersatzrs_addon_contract::TeletextNavigationRoleWire,
        #[serde(deserialize_with = "map_only")]
        target: ersatzrs_addon_contract::TeletextNavigationTargetWire,
    }
    struct Links;
    impl<'de> Visitor<'de> for Links {
        type Value = Vec<TeletextNavigationLinkWire>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("navigation array")
        }
        fn visit_seq<A: SeqAccess<'de>>(
            self,
            mut a: A,
        ) -> std::result::Result<Self::Value, A::Error> {
            #[derive(Deserialize)]
            struct Wrapper(#[serde(deserialize_with = "map_only")] Link);
            let mut result = Vec::new();
            while let Some(Wrapper(link)) = a.next_element()? {
                result.push(TeletextNavigationLinkWire {
                    role: link.role,
                    target: link.target,
                });
            }
            Ok(result)
        }
    }
    d.deserialize_seq(Links)
}

macro_rules! object {
    ($(#[$meta:meta])* pub struct $name:ident { $($(#[$field_meta:meta])* pub $field:ident: $ty:ty),* $(,)? }) => {
        $(#[$meta])* #[derive(Clone, PartialEq, Eq, Serialize)]
        pub struct $name { $($(#[$field_meta])* pub $field: $ty),* }
        impl fmt::Debug for $name { fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(concat!(stringify!($name), "(<redacted>)")) } }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self,D::Error> {
                #[derive(Deserialize)] #[serde(deny_unknown_fields)]
                struct Fields { $($(#[$field_meta])* $field: $ty),* }
                let fields: Fields = map_only(d)?;
                Ok(Self { $($field: fields.$field),* })
            }
        }
    }
}
object! {
    /// Versioned public descriptive project, without profile settings or permissions.
    pub struct TeletextExportProject {
        pub schema_version: u32,
        pub editor_profile: String,
        pub original: TeletextExportOriginal,
        pub channel_id: i64,
        #[serde(deserialize_with="map_only")] pub entry: TeletextPageAddressWire,
        pub language: String,
        #[serde(deserialize_with="nullable")] pub v1_region: Option<TeletextSavedV1Region>,
        pub producers: Vec<TeletextExportProducer>,
        pub pages: Vec<TeletextExportPageMapping>,
        pub assets: Vec<TeletextExportAssetMapping>,
    }
}
object! {
    /// Claimed original immutable revision and head fence; the repository is authoritative.
    pub struct TeletextExportOriginal { pub design_id: i64, pub revision: i64, pub digest: String, pub expected_head: i64, pub document_version: u16 }
}
object! {
    /// Selected producer identity; omits effective settings and secret references.
    pub struct TeletextExportProducer {
        pub ordinal: usize, pub addon_id: String, pub contract: u32,
        pub binding_revision: i64, pub profile_id: i64, pub profile_revision: i64,
        pub installation: TeletextExportInstallation,
        #[serde(deserialize_with="nullable")] pub authoring: Option<TeletextAuthoringMetadataWire>,
    }
}
object! {
    /// Package provenance copied from the original selection, never activation authority.
    pub struct TeletextExportInstallation { pub state_revision: String, pub version: String, pub package_sha256: String, pub content_sha256: String, pub manifest_sha256: String }
}
object! {
    /// One stable editable file maps editor ordinal zero to its exact authored subcode.
    pub struct TeletextExportPageMapping {
        pub name: String,
        #[serde(deserialize_with="map_only")] pub page: TeletextPageAddressWire,
        pub producer_ordinal: usize, pub editor_page: u16, pub editor_ordinal: u64,
        pub language: String,
        #[serde(deserialize_with="page_kind")] pub kind: TeletextPageKindWire,
        pub header_controls: u16,
        #[serde(deserialize_with="navigation")] pub navigation: Vec<TeletextNavigationLinkWire>,
        #[serde(deserialize_with="nullable")] pub materialized_magazine: Option<u8>,
        pub tti_bytes: usize, pub tti_sha256: String,
    }
}
object! {
    /// Exact raw resource or magazine units stored outside the editable TTI file.
    pub struct TeletextExportAssetMapping { pub name: String, pub scope: TeletextExportAssetScope, pub unit_count: usize, pub bytes: usize, pub sha256: String }
}
/// Closed packet ownership, independent of editable page files.
#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TeletextExportAssetScope {
    Resource {
        identity: TeletextResourceIdentityWire,
        physical_subcode: u16,
    },
    Magazine {
        magazine: u8,
    },
}
impl fmt::Debug for TeletextExportAssetScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TeletextExportAssetScope(<redacted>)")
    }
}
impl<'de> Deserialize<'de> for TeletextExportAssetScope {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Resource {
                identity: TeletextResourceIdentityWire,
                physical_subcode: u16,
            },
            Magazine {
                magazine: u8,
            },
        }
        Ok(match map_only(d)? {
            Fields::Resource {
                identity,
                physical_subcode,
            } => Self::Resource {
                identity,
                physical_subcode,
            },
            Fields::Magazine { magazine } => Self::Magazine { magazine },
        })
    }
}
object! {
    /// PRIVATE receipt body: exact file bytes retained by the host, not an editor sidecar.
    pub struct TeletextExportReceiptBody { pub schema_version: u32, pub project: TeletextExportProject, pub files: Vec<TeletextExportFile> }
}
object! {
    /// One exact flat-named file; Debug never exposes its content.
    pub struct TeletextExportFile { pub name: String, pub bytes: Vec<u8> }
}
object! {
    /// Public receipt reference with descriptive metadata; contains no file bodies.
    pub struct TeletextExportSidecar { pub schema_version: u32, pub receipt_id: i64, pub receipt_digest: String, pub project: TeletextExportProject }
}

fn validate_budget(b: TeletextExportProjectBudget) -> Result<()> {
    if [
        b.max_bytes,
        b.max_files,
        b.max_producers,
        b.max_file_bytes,
        b.max_units,
    ]
    .into_iter()
    .any(|v| v == 0 || v > isize::MAX as usize || i64::try_from(v).is_err())
    {
        return Err(failure(TeletextExportProjectFailure::Budget));
    }
    Ok(())
}
fn hash(v: &str) -> bool {
    v.len() == 64
        && v.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn flat_name(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 128
        && v != "."
        && v != ".."
        && v.bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_.-".contains(&c))
}
fn text(v: &str) -> bool {
    !v.is_empty() && v.len() <= 255 && v.bytes().all(|c| c.is_ascii_graphic())
}
fn address(v: TeletextPageAddressWire) -> bool {
    page(v.page) && subcode(v.subcode)
}
fn page(v: u16) -> bool {
    (100..=899).contains(&v)
}
fn editor_page(decimal: u16) -> u16 {
    ((decimal / 100) << 8) | (((decimal / 10) % 10) << 4) | (decimal % 10)
}
fn kind_flags(kind: TeletextPageKindWire) -> u16 {
    match kind {
        TeletextPageKindWire::FullPage => 0,
        TeletextPageKindWire::Newsflash => 2,
        TeletextPageKindWire::Subtitle => 4,
    }
}
fn subcode(v: u16) -> bool {
    v & !0x3f7f == 0
}

impl TeletextExportProject {
    /// Check syntax and internal associations only; native validation remains required.
    /// # Errors
    /// Returns a static indexed error for invalid bounds, identities or mappings.
    pub fn validate(&self, b: TeletextExportProjectBudget) -> Result<()> {
        use TeletextExportProjectFailure as K;
        validate_budget(b)?;
        if self.schema_version != 1
            || !matches!(
                self.editor_profile.as_str(),
                "qteletextmaker-1.0" | "qteletextmaker-1.0-carrier-v2"
            )
            || self.original.document_version != 1
        {
            return Err(failure(K::Schema));
        }
        if self.original.design_id <= 0
            || self.original.revision <= 0
            || self.original.expected_head < self.original.revision
            || !hash(&self.original.digest)
            || self.channel_id <= 0
            || !address(self.entry)
            || ersatzrs_domain::TeletextLanguage::new(self.language.clone()).is_err()
        {
            return Err(failure(K::Identity));
        }
        if self.producers.is_empty()
            || self.producers.len() > b.max_producers
            || self.pages.is_empty()
            || self
                .pages
                .len()
                .checked_add(self.assets.len())
                .is_none_or(|v| v > b.max_files)
        {
            return Err(failure(K::Limit));
        }
        validate_producers(&self.producers)?;
        if self.producers.iter().any(|p| p.contract == 1) != self.v1_region.is_some() {
            return Err(failure(K::Mapping));
        }
        validate_pages(self, b)?;
        validate_assets(self, b)?;
        Ok(())
    }
}
fn validate_producers(producers: &[TeletextExportProducer]) -> Result<()> {
    use TeletextExportProjectFailure as K;
    let mut addons = BTreeSet::new();
    for (i, p) in producers.iter().enumerate() {
        if p.ordinal != i
            || ersatzrs_addon_contract::validate_addon_id(&p.addon_id).is_err()
            || !addons.insert(p.addon_id.as_str())
            || !(1..=3).contains(&p.contract)
            || p.binding_revision <= 0
            || p.profile_id <= 0
            || p.profile_revision <= 0
            || !text(&p.installation.state_revision)
            || !text(&p.installation.version)
            || [
                &p.installation.package_sha256,
                &p.installation.content_sha256,
                &p.installation.manifest_sha256,
            ]
            .into_iter()
            .any(|v| !hash(v))
        {
            return Err(indexed(K::Identity, i));
        }
        if (p.contract == 3) != p.authoring.is_some() {
            return Err(indexed(K::Metadata, i));
        }
        if let Some(metadata) = &p.authoring {
            // This validates metadata against its own claimed identities. The host
            // separately compares every identity with the retained producer service.
            if metadata.pages.len() > ersatzrs_addon_contract::MAX_TELETEXT_PAGES_RECORDS {
                return Err(indexed(K::Metadata, i));
            }
            let pages: Vec<_> = metadata
                .pages
                .iter()
                .map(|p| TeletextPageDraftWire {
                    address: p.page,
                    kind: TeletextPageKindWire::FullPage,
                    language: "eng".into(),
                    national_option: 0,
                    rows: Vec::new(),
                    navigation: Vec::new(),
                })
                .collect();
            if pages.iter().any(|p| !address(p.address))
                || metadata.validate_against(&pages).is_err()
            {
                return Err(indexed(K::Metadata, i));
            }
        }
    }
    Ok(())
}
fn validate_pages(project: &TeletextExportProject, b: TeletextExportProjectBudget) -> Result<()> {
    use TeletextExportProjectFailure as K;
    let mut names = BTreeSet::new();
    let mut pages = BTreeSet::new();
    for (i, p) in project.pages.iter().enumerate() {
        if !flat_name(&p.name)
            || !names.insert(p.name.as_str())
            || !address(p.page)
            || !pages.insert((p.page.page, p.page.subcode))
            || p.producer_ordinal >= project.producers.len()
            || p.editor_page != editor_page(p.page.page)
            || p.editor_ordinal != 0
            || p.header_controls > 0x7ff
            || ersatzrs_domain::TeletextLanguage::new(p.language.clone()).is_err()
            || (p.header_controls & 6) != kind_flags(p.kind)
            || p.tti_bytes == 0
            || p.tti_bytes > b.max_file_bytes
            || !hash(&p.tti_sha256)
            || p.materialized_magazine
                .is_some_and(|m| !(1..=8).contains(&m) || u16::from(m) != p.page.page / 100)
        {
            return Err(indexed(K::Mapping, i));
        }
        if project.producers[p.producer_ordinal]
            .authoring
            .as_ref()
            .is_some_and(|metadata| !metadata.pages.iter().any(|v| v.page == p.page))
        {
            return Err(indexed(K::Metadata, i));
        }
        let mut roles = BTreeSet::new();
        for link in &p.navigation {
            if !page(link.target.page)
                || link.target.subcode.is_some_and(|s| !subcode(s))
                || !roles.insert(format!("{:?}", link.role))
            {
                return Err(indexed(K::Mapping, i));
            }
        }
    }
    Ok(())
}
fn validate_assets(project: &TeletextExportProject, b: TeletextExportProjectBudget) -> Result<()> {
    use TeletextExportProjectFailure as K;
    let mut names: BTreeSet<_> = project.pages.iter().map(|p| p.name.as_str()).collect();
    let mut scopes = BTreeSet::new();
    let mut units = 0usize;
    for (i, a) in project.assets.iter().enumerate() {
        units = units
            .checked_add(a.unit_count)
            .ok_or_else(|| indexed(K::Limit, i))?;
        let identity = match &a.scope {
            TeletextExportAssetScope::Resource {
                identity,
                physical_subcode,
            } => {
                if !(1..=8).contains(&identity.page.magazine)
                    || identity.subpage > 15
                    || !subcode(*physical_subcode)
                {
                    return Err(indexed(K::Mapping, i));
                }
                format!(
                    "r:{:?}:{}:{}:{}:{}",
                    identity.kind,
                    identity.page.magazine,
                    identity.page.page,
                    identity.subpage,
                    physical_subcode
                )
            }
            TeletextExportAssetScope::Magazine { magazine } => {
                if !(1..=8).contains(magazine) {
                    return Err(indexed(K::Mapping, i));
                }
                format!("m:{magazine}")
            }
        };
        if !flat_name(&a.name)
            || !names.insert(a.name.as_str())
            || !scopes.insert(identity)
            || a.unit_count == 0
            || units > b.max_units
            || a.unit_count.checked_mul(46) != Some(a.bytes)
            || a.bytes > b.max_file_bytes
            || !hash(&a.sha256)
        {
            return Err(indexed(K::Mapping, i));
        }
    }
    Ok(())
}
impl TeletextExportReceiptBody {
    /// Validate exact file coverage and TTI identity syntax without checking hashes.
    /// # Errors
    /// Returns a static indexed error; hash and native semantics are host obligations.
    pub fn validate(&self, b: TeletextExportProjectBudget) -> Result<()> {
        use TeletextExportProjectFailure as K;
        self.project.validate(b)?;
        if self.schema_version != 1 {
            return Err(failure(K::Schema));
        }
        if self.files.len() != self.project.pages.len() + self.project.assets.len()
            || self.files.len() > b.max_files
        {
            return Err(failure(K::Files));
        }
        let mut names = BTreeSet::new();
        for (i, file) in self.files.iter().enumerate() {
            if !flat_name(&file.name)
                || !names.insert(file.name.as_str())
                || file.bytes.len() > b.max_file_bytes
            {
                return Err(indexed(K::Files, i));
            }
            if let Some(mapping) = self.project.pages.iter().find(|p| p.name == file.name) {
                if file.bytes.len() != mapping.tti_bytes {
                    return Err(indexed(K::Files, i));
                }
                let tti = parse_teletext_tti(
                    &file.bytes,
                    TeletextTtiBudget {
                        max_bytes: b.max_file_bytes,
                        max_records: b.max_units,
                    },
                )
                .map_err(|_| indexed(K::Tti, i))?;
                let mut pn = 0;
                let mut sc = 0;
                for record in tti.records() {
                    match record {
                        TeletextTtiRecord::PageNumber {
                            page,
                            editor_ordinal,
                        } => {
                            pn += 1;
                            if *page != mapping.editor_page
                                || *editor_ordinal != mapping.editor_ordinal
                            {
                                return Err(indexed(K::Tti, i));
                            }
                        }
                        TeletextTtiRecord::SubpageOrdinal(ordinal) => {
                            sc += 1;
                            if *ordinal != mapping.editor_ordinal {
                                return Err(indexed(K::Tti, i));
                            }
                        }
                        _ => {}
                    }
                }
                if pn != 1 || sc != 1 {
                    return Err(indexed(K::Tti, i));
                }
            } else if let Some(mapping) = self.project.assets.iter().find(|a| a.name == file.name) {
                if file.bytes.len() != mapping.bytes {
                    return Err(indexed(K::Files, i));
                }
            } else {
                return Err(indexed(K::Files, i));
            }
        }
        Ok(())
    }
}
impl TeletextExportSidecar {
    /// Check a public untrusted receipt reference, never granting edit authority.
    /// # Errors
    /// Returns a static error for invalid metadata, receipt identity or bounds.
    pub fn validate(&self, b: TeletextExportProjectBudget) -> Result<()> {
        self.project.validate(b)?;
        if self.schema_version != 1 {
            return Err(failure(TeletextExportProjectFailure::Schema));
        }
        if self.receipt_id <= 0 || !hash(&self.receipt_digest) {
            return Err(failure(TeletextExportProjectFailure::Identity));
        }
        Ok(())
    }
}

struct Scan<'a> {
    budget: TeletextExportProjectBudget,
    depth: usize,
    field: Option<&'a str>,
    authoring: bool,
    error: &'a Cell<Option<TeletextExportProjectFailure>>,
}
impl<'de> DeserializeSeed<'de> for Scan<'_> {
    type Value = ();
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> std::result::Result<(), D::Error> {
        if self.depth > 32 {
            self.error.set(Some(TeletextExportProjectFailure::Depth));
            return Err(serde::de::Error::custom("editor depth"));
        }
        d.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Scan<'_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bounded editor JSON")
    }
    fn visit_bool<E: serde::de::Error>(self, _: bool) -> std::result::Result<(), E> {
        Ok(())
    }
    fn visit_i64<E: serde::de::Error>(self, _: i64) -> std::result::Result<(), E> {
        Ok(())
    }
    fn visit_u64<E: serde::de::Error>(self, _: u64) -> std::result::Result<(), E> {
        Ok(())
    }
    fn visit_f64<E: serde::de::Error>(self, _: f64) -> std::result::Result<(), E> {
        Ok(())
    }
    fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<(), E> {
        Ok(())
    }
    fn visit_str<E: serde::de::Error>(self, _: &str) -> std::result::Result<(), E> {
        Ok(())
    }
    fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> std::result::Result<(), A::Error> {
        let mut keys = BTreeSet::new();
        while let Some(key) = a.next_key::<String>()? {
            if key.len() > 128 || keys.len() >= 32 {
                self.error.set(Some(TeletextExportProjectFailure::Limit));
                return Err(serde::de::Error::custom("editor object limit"));
            }
            if !keys.insert(key.clone()) {
                self.error
                    .set(Some(TeletextExportProjectFailure::Duplicate));
                return Err(serde::de::Error::custom("editor duplicate"));
            }
            a.next_value_seed(Scan {
                budget: self.budget,
                depth: self.depth + 1,
                field: Some(&key),
                authoring: self.authoring || self.field == Some("authoring"),
                error: self.error,
            })?;
        }
        Ok(())
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> std::result::Result<(), A::Error> {
        let max = match self.field {
            Some("files" | "assets") => self.budget.max_files,
            Some("pages") if !self.authoring => self.budget.max_files,
            Some("producers") => self.budget.max_producers,
            Some("bytes") => self.budget.max_file_bytes,
            _ => self.budget.max_units,
        };
        let mut count = 0;
        while a
            .next_element_seed(Counted {
                inner: Scan {
                    budget: self.budget,
                    depth: self.depth + 1,
                    field: None,
                    authoring: self.authoring,
                    error: self.error,
                },
                count,
                maximum: max,
            })?
            .is_some()
        {
            count += 1;
        }
        Ok(())
    }
}
struct Counted<'a> {
    inner: Scan<'a>,
    count: usize,
    maximum: usize,
}
impl<'de> DeserializeSeed<'de> for Counted<'_> {
    type Value = ();
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> std::result::Result<(), D::Error> {
        if self.count >= self.maximum {
            self.inner
                .error
                .set(Some(TeletextExportProjectFailure::Limit));
            return Err(serde::de::Error::custom("editor array limit"));
        }
        self.inner.deserialize(d)
    }
}
fn parse<T: DeserializeOwned>(bytes: &[u8], b: TeletextExportProjectBudget) -> Result<T> {
    validate_budget(b)?;
    if bytes.len() > b.max_bytes {
        return Err(failure(TeletextExportProjectFailure::Bytes));
    }
    scan_json(bytes, b)?;
    serde_json::from_slice(bytes).map_err(|_| failure(TeletextExportProjectFailure::Shape))
}
fn scan_json(bytes: &[u8], b: TeletextExportProjectBudget) -> Result<()> {
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let error = Cell::new(None);
    Scan {
        budget: b,
        depth: 0,
        field: None,
        authoring: false,
        error: &error,
    }
    .deserialize(&mut deserializer)
    .and_then(|()| deserializer.end())
    .map_err(|_| failure(error.get().unwrap_or(TeletextExportProjectFailure::Shape)))?;
    Ok(())
}
struct Writer {
    bytes: Vec<u8>,
    maximum: usize,
}
impl io::Write for Writer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self
            .bytes
            .len()
            .checked_add(bytes.len())
            .is_none_or(|len| len > self.maximum)
        {
            return Err(io::Error::other("editor output limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn write<T: Serialize>(value: &T, b: TeletextExportProjectBudget) -> Result<Vec<u8>> {
    let mut writer = Writer {
        bytes: Vec::new(),
        maximum: b.max_bytes,
    };
    serde_json::to_writer(&mut writer, value)
        .map_err(|_| failure(TeletextExportProjectFailure::Bytes))?;
    scan_json(&writer.bytes, b)?;
    Ok(writer.bytes)
}
/// Parse an untrusted public project after bounded duplicate/depth preflight.
/// # Errors
/// Returns static shape, bound or association errors without exposing input.
pub fn parse_teletext_export_project(
    bytes: &[u8],
    b: TeletextExportProjectBudget,
) -> Result<TeletextExportProject> {
    let p: TeletextExportProject = parse(bytes, b)?;
    p.validate(b)?;
    Ok(p)
}
/// Write a descriptive project with a bounded writer; it grants no authority.
/// # Errors
/// Returns static validation or encoded-size errors.
pub fn write_teletext_export_project(
    p: &TeletextExportProject,
    b: TeletextExportProjectBudget,
) -> Result<Vec<u8>> {
    p.validate(b)?;
    write(p, b)
}
/// Parse a PRIVATE host receipt body, including exact file and TTI syntax checks.
/// # Errors
/// Returns static indexed errors; actual hash verification remains native work.
pub fn parse_teletext_export_receipt_body(
    bytes: &[u8],
    b: TeletextExportProjectBudget,
) -> Result<TeletextExportReceiptBody> {
    let p: TeletextExportReceiptBody = parse(bytes, b)?;
    p.validate(b)?;
    Ok(p)
}
/// Write a PRIVATE exact-file receipt within the caller's serialized-size cap.
/// # Errors
/// Returns static validation or encoded-size errors.
pub fn write_teletext_export_receipt_body(
    p: &TeletextExportReceiptBody,
    b: TeletextExportProjectBudget,
) -> Result<Vec<u8>> {
    p.validate(b)?;
    write(p, b)
}
/// Parse a public sidecar; repository/native validation must establish ownership.
/// # Errors
/// Returns static metadata, reference, shape or bound errors.
pub fn parse_teletext_export_sidecar(
    bytes: &[u8],
    b: TeletextExportProjectBudget,
) -> Result<TeletextExportSidecar> {
    let p: TeletextExportSidecar = parse(bytes, b)?;
    p.validate(b)?;
    Ok(p)
}
/// Write a public sidecar without exact private files or permission flags.
/// # Errors
/// Returns static validation or encoded-size errors.
pub fn write_teletext_export_sidecar(
    p: &TeletextExportSidecar,
    b: TeletextExportProjectBudget,
) -> Result<Vec<u8>> {
    p.validate(b)?;
    write(p, b)
}

#[cfg(test)]
#[path = "teletext_export_project/tests.rs"]
mod tests;
