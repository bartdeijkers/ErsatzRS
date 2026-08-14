//! Stable, dependency-light contracts shared by the ErsatzRS host and add-ons.
//!
//! Add-ons are separate processes. This crate defines package metadata,
//! repository catalogs, settings, declarative panels, and bounded invocation
//! results; it is not a native plugin ABI.

use std::collections::{BTreeMap, BTreeSet};

use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};

/// Current package-manifest schema. Schema v1 remains readable.
pub const ADDON_MANIFEST_VERSION: u32 = 3;
/// Current repository-index schema. Schema v1 remains readable.
pub const ADDON_REPOSITORY_SCHEMA_VERSION: u32 = 2;
/// Health capability implemented by every executable add-on.
pub const CAPABILITY_ADDON_CHECK_V1: &str = "addon.check.v1";
/// Enumerate a Remote Stream playlist as bounded `NDJSON`.
pub const CAPABILITY_REMOTE_STREAM_LIST_V1: &str = "remote-stream.list.v1";
/// Produce Remote Stream bytes for playback.
pub const CAPABILITY_REMOTE_STREAM_PLAY_V1: &str = "remote-stream.play.v1";
/// Enumerate a provider list whose entries are matched to local media.
pub const CAPABILITY_MEDIA_LIST_LIST_V1: &str = "media-list.list.v1";
/// Host-rendered declarative add-on panel schema.
pub const CAPABILITY_DECLARATIVE_PANEL_V1: &str = "declarative-panel.v1";
const MAX_PACKAGE_VERSION_BYTES: usize = 128;

/// Manifest validation failure.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ContractError {
    /// A required or constrained field is invalid.
    #[error("invalid {field}: {message}")]
    Invalid {
        /// Field or section name.
        field: &'static str,
        /// Safe validation detail.
        message: String,
    },
}

/// A package manifest stored at the package root.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddonManifest {
    /// Manifest schema version.
    pub manifest_version: u32,
    /// Stable reverse-DNS add-on identifier.
    pub id: String,
    /// `SemVer` package version.
    pub version: Version,
    /// Localized display names, requiring en-US.
    pub name: BTreeMap<String, String>,
    /// Localized short summaries, requiring en-US.
    pub summary: BTreeMap<String, String>,
    /// Optional package-local icon rendered by the host.
    #[serde(default)]
    pub icon: Option<AddonPackageIcon>,
    /// SPDX license expression or identifier.
    pub license: String,
    /// Public source-code URL.
    pub source_url: String,
    /// Supported ErsatzRS host versions.
    pub host_version: VersionReq,
    /// Capabilities implemented by this package.
    #[serde(default)]
    pub capabilities: Vec<AddonCapability>,
    /// Other add-ons required or optionally integrated by this package.
    #[serde(default)]
    pub dependencies: Vec<AddonDependency>,
    /// Entrypoints selected by exact `RID` or the portable `any` fallback.
    pub entrypoints: BTreeMap<String, AddonEntrypoint>,
    /// Operator-visible privilege declaration. This is not an OS sandbox.
    #[serde(default)]
    pub permissions: AddonPermissions,
    /// Host-rendered configuration fields.
    #[serde(default)]
    pub settings: Vec<AddonSettingDefinition>,
    /// Optional host-managed storage for file-backed media-list libraries.
    #[serde(default)]
    pub media_list_storage: Option<AddonMediaListStorage>,
    /// Host-rendered declarative panels.
    #[serde(default)]
    pub panels: Vec<AddonPanel>,
}

impl AddonManifest {
    /// Validate cross-field invariants that serde alone cannot express.
    ///
    /// # Errors
    ///
    /// Returns a safe validation error when the package cannot be installed.
    pub fn validate(&self) -> Result<(), ContractError> {
        if !(1..=ADDON_MANIFEST_VERSION).contains(&self.manifest_version) {
            return invalid("manifest_version", "unsupported schema version");
        }
        validate_addon_id(&self.id)?;
        validate_package_version(&self.version)?;
        require_english(&self.name, "name")?;
        require_english(&self.summary, "summary")?;
        if self.manifest_version == 1 && self.icon.is_some() {
            return invalid("icon", "requires manifest schema version 2");
        }
        if let Some(icon) = &self.icon {
            icon.validate()?;
        }
        if self.license.trim().is_empty() {
            return invalid("license", "must not be empty");
        }
        if !is_https_url(&self.source_url) {
            return invalid("source_url", "must use HTTPS");
        }
        if self.capabilities.is_empty() {
            return invalid("capabilities", "at least one capability is required");
        }
        if !self.has_capability(CAPABILITY_ADDON_CHECK_V1) {
            return invalid(
                "capabilities",
                "executable add-ons must implement addon.check.v1",
            );
        }
        if self.entrypoints.is_empty() {
            return invalid("entrypoints", "at least one entrypoint is required");
        }

        let mut capability_ids = BTreeSet::new();
        for capability in &self.capabilities {
            validate_capability_id(&capability.id)?;
            if !capability_ids.insert(capability.id.as_str()) {
                return invalid("capabilities", "duplicate capability id");
            }
        }
        validate_dependencies(&self.id, &self.dependencies)?;
        for (rid, entrypoint) in &self.entrypoints {
            if rid != "any" && !is_supported_rid(rid) {
                return invalid("entrypoints", "unknown ErsatzRS RID");
            }
            entrypoint.validate()?;
        }
        self.permissions.validate()?;

        let mut setting_keys = BTreeSet::new();
        for setting in &self.settings {
            setting.validate(self.manifest_version)?;
            if !setting_keys.insert(setting.key.as_str()) {
                return invalid("settings", "duplicate setting key");
            }
        }
        if let Some(storage) = &self.media_list_storage {
            if self.manifest_version < 3 {
                return invalid("media_list_storage", "requires manifest schema version 3");
            }
            storage.validate(&self.settings)?;
            for capability in [
                CAPABILITY_MEDIA_LIST_LIST_V1,
                CAPABILITY_REMOTE_STREAM_LIST_V1,
                CAPABILITY_REMOTE_STREAM_PLAY_V1,
            ] {
                if !self.has_capability(capability) {
                    return invalid("media_list_storage", format!("requires {capability}"));
                }
            }
        }

        let mut panel_ids = BTreeSet::new();
        if !self.panels.is_empty() && !self.has_capability(CAPABILITY_DECLARATIVE_PANEL_V1) {
            return invalid("panels", "declarative panels require declarative-panel.v1");
        }
        for panel in &self.panels {
            panel.validate()?;
            if !panel_ids.insert(panel.id.as_str()) {
                return invalid("panels", "duplicate panel id");
            }
        }
        Ok(())
    }

    /// Return the entrypoint for an exact `RID`, then the portable fallback.
    #[must_use]
    pub fn entrypoint_for(&self, rid: &str) -> Option<&AddonEntrypoint> {
        self.entrypoints
            .get(rid)
            .or_else(|| self.entrypoints.get("any"))
    }

    /// Whether the package declares a capability.
    #[must_use]
    pub fn has_capability(&self, capability: &str) -> bool {
        self.capabilities.iter().any(|item| item.id == capability)
    }
}

/// Host-owned file storage used to materialize add-on lists as local Remote
/// Stream libraries.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddonMediaListStorage {
    /// Manifest setting that allows the operator to override the root path.
    pub path_setting: String,
    /// Safe folder below the ErsatzRS profile used when no override is stored.
    pub default_subdirectory: String,
    /// Stable operator-facing name for the managed local media source.
    pub source_name: String,
}

impl AddonMediaListStorage {
    fn validate(&self, settings: &[AddonSettingDefinition]) -> Result<(), ContractError> {
        let setting = settings
            .iter()
            .find(|setting| setting.key == self.path_setting)
            .ok_or_else(|| ContractError::Invalid {
                field: "media_list_storage.path_setting",
                message: "must reference a declared setting".to_owned(),
            })?;
        if setting.kind != AddonSettingKind::Path {
            return invalid(
                "media_list_storage.path_setting",
                "must reference a path setting",
            );
        }
        if setting.default.is_some() {
            return invalid(
                "media_list_storage.path_setting",
                "host-managed path defaults must not declare a static default",
            );
        }
        if setting.required {
            return invalid(
                "media_list_storage.path_setting",
                "must be optional so the host default can be used",
            );
        }
        if !is_safe_single_component(&self.default_subdirectory) {
            return invalid(
                "media_list_storage.default_subdirectory",
                "must be one safe folder name",
            );
        }
        if self.source_name.trim().is_empty()
            || self.source_name.len() > 128
            || self.source_name.chars().any(char::is_control)
        {
            return invalid(
                "media_list_storage.source_name",
                "must be a non-empty display-safe name",
            );
        }
        Ok(())
    }
}

/// Optional package-local add-on icon.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddonPackageIcon {
    /// Relative path inside the extracted package.
    pub path: String,
    /// Exact supported media type.
    pub media_type: AddonIconMediaType,
}

impl AddonPackageIcon {
    fn validate(&self) -> Result<(), ContractError> {
        if !is_safe_relative_path(&self.path) {
            return invalid("icon.path", "must stay inside the package");
        }
        let expected_extension = match self.media_type {
            AddonIconMediaType::Png => ".png",
            AddonIconMediaType::Webp => ".webp",
        };
        if !self.path.to_ascii_lowercase().ends_with(expected_extension) {
            return invalid("icon.path", "extension does not match media type");
        }
        Ok(())
    }
}

/// Image formats accepted for add-on icons.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonIconMediaType {
    Png,
    Webp,
}

impl AddonIconMediaType {
    #[must_use]
    pub const fn as_mime_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Webp => "image/webp",
        }
    }
}

/// One versioned executable capability.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddonCapability {
    /// Capability identifier ending in .v<major>.
    pub id: String,
}

/// One version-constrained add-on dependency.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddonDependency {
    /// Stable reverse-DNS add-on identity.
    pub id: String,
    /// Accepted installed versions.
    pub version: VersionReq,
    /// Optional integrations do not block readiness when absent.
    #[serde(default)]
    pub optional: bool,
}

/// How the host starts one package-local entrypoint.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddonEntrypoint {
    /// Relative package-local executable or script path.
    pub path: String,
    /// Host-controlled interpreter kind.
    pub kind: AddonEntrypointKind,
}

impl AddonEntrypoint {
    fn validate(&self) -> Result<(), ContractError> {
        if !is_safe_relative_path(&self.path) {
            return invalid("entrypoint.path", "must stay inside the package");
        }
        Ok(())
    }
}

/// Supported entrypoint launchers. Arbitrary interpreters are deliberately absent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonEntrypointKind {
    /// Native executable contained in the package.
    Native,
    /// POSIX sh script.
    PosixShell,
    /// Windows batch script.
    WindowsBatch,
}

/// Operator-visible permissions requested by an add-on.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AddonPermissions {
    /// Network host patterns the add-on expects to contact.
    pub network_hosts: Vec<String>,
    /// Whether media input paths are supplied to the process.
    pub media_read: bool,
    /// Whether the host supplies its managed `FFmpeg`/`ffprobe` paths.
    pub managed_ffmpeg: bool,
    /// External executable names expected by the package.
    pub external_commands: Vec<String>,
}

impl AddonPermissions {
    /// Validate bounded, display-safe permission declarations.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid host patterns or executable names.
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.network_hosts.len() > 100 || self.external_commands.len() > 100 {
            return invalid("permissions", "too many declared permissions");
        }
        if self.network_hosts.iter().any(|host| {
            host.is_empty()
                || host.len() > 255
                || host.contains("//")
                || host.contains('/')
                || host.contains(char::is_whitespace)
                || host.chars().any(char::is_control)
        }) {
            return invalid(
                "permissions.network_hosts",
                "contains an invalid host pattern",
            );
        }
        if self.external_commands.iter().any(|command| {
            command.is_empty()
                || command.len() > 128
                || command.contains(['/', '\\', ':'])
                || command.contains(char::is_whitespace)
                || command.chars().any(char::is_control)
        }) {
            return invalid(
                "permissions.external_commands",
                "contains an invalid executable name",
            );
        }
        Ok(())
    }
}

/// One host-rendered configuration field.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddonSettingDefinition {
    /// Stable uppercase snake-case key used for process environment projection.
    pub key: String,
    /// Localized label map, requiring en-US.
    pub label: BTreeMap<String, String>,
    /// Value kind and rendering behavior.
    pub kind: AddonSettingKind,
    /// Whether enabling requires a configured value.
    #[serde(default)]
    pub required: bool,
    /// Optional non-secret default.
    pub default: Option<String>,
    /// Allowed values for enum settings.
    #[serde(default)]
    pub options: Vec<String>,
    /// Safe host-rendered default for a secret *reference*, never its value.
    #[serde(default)]
    pub suggested_reference: Option<AddonSuggestedSecretReference>,
}

impl AddonSettingDefinition {
    fn validate(&self, manifest_version: u32) -> Result<(), ContractError> {
        if !is_setting_key(&self.key) {
            return invalid("settings.key", "must be uppercase snake case");
        }
        require_english(&self.label, "settings.label")?;
        if self.kind == AddonSettingKind::SecretReference && self.default.is_some() {
            return invalid("settings.default", "secret references cannot have defaults");
        }
        if let Some(reference) = &self.suggested_reference {
            if manifest_version < 3 {
                return invalid(
                    "settings.suggested_reference",
                    "requires manifest schema version 3",
                );
            }
            if self.kind != AddonSettingKind::SecretReference {
                return invalid(
                    "settings.suggested_reference",
                    "only secret references can have a suggested reference",
                );
            }
            reference.validate()?;
        }
        if self.kind == AddonSettingKind::Enum && self.options.is_empty() {
            return invalid("settings.options", "enum settings require options");
        }
        Ok(())
    }
}

/// Non-sensitive location suggested by an add-on for a protected setting.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddonSuggestedSecretReference {
    /// Secret-store location type.
    pub kind: AddonSuggestedSecretReferenceKind,
    /// Environment-variable name or file path, never the resolved value.
    pub reference: String,
}

impl AddonSuggestedSecretReference {
    fn validate(&self) -> Result<(), ContractError> {
        match self.kind {
            AddonSuggestedSecretReferenceKind::Environment
                if !is_environment_reference_name(&self.reference) =>
            {
                invalid(
                    "settings.suggested_reference.reference",
                    "environment reference must be uppercase snake case",
                )
            }
            AddonSuggestedSecretReferenceKind::File if self.reference.trim().is_empty() => invalid(
                "settings.suggested_reference.reference",
                "file reference must not be empty",
            ),
            _ => Ok(()),
        }
    }
}

/// Supported safe locations for an add-on secret.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonSuggestedSecretReferenceKind {
    Environment,
    File,
}

/// Supported host-rendered setting types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonSettingKind {
    String,
    Boolean,
    Integer,
    Enum,
    Path,
    Executable,
    /// Reference to an environment variable or file, never a raw secret.
    SecretReference,
}

/// One declarative page rendered entirely by the host.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddonPanel {
    /// Stable route-local identifier.
    pub id: String,
    /// Localized panel title.
    pub title: BTreeMap<String, String>,
    /// Ordered safe blocks.
    pub blocks: Vec<AddonPanelBlock>,
}

impl AddonPanel {
    fn validate(&self) -> Result<(), ContractError> {
        if !is_lower_identifier(&self.id) {
            return invalid("panels.id", "must be lowercase ASCII with dashes");
        }
        require_english(&self.title, "panels.title")?;
        if self.blocks.len() > 100 {
            return invalid("panels.blocks", "too many blocks");
        }
        for block in &self.blocks {
            block.validate()?;
        }
        Ok(())
    }
}

/// Safe panel elements. No executable markup or styling is accepted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AddonPanelBlock {
    Text {
        text: BTreeMap<String, String>,
    },
    Status {
        label: BTreeMap<String, String>,
        code: String,
    },
    KeyValue {
        label: BTreeMap<String, String>,
        value: String,
    },
    Link {
        label: BTreeMap<String, String>,
        url: String,
    },
}

impl AddonPanelBlock {
    fn validate(&self) -> Result<(), ContractError> {
        match self {
            Self::Text { text } => require_english(text, "panels.blocks.text"),
            Self::Status { label, code } => {
                require_english(label, "panels.blocks.label")?;
                if !is_lower_identifier(code) {
                    return invalid("panels.blocks.code", "must be a lowercase identifier");
                }
                Ok(())
            }
            Self::KeyValue { label, value } => {
                require_english(label, "panels.blocks.label")?;
                if value.len() > 4_096 || value.chars().any(char::is_control) {
                    return invalid("panels.blocks.value", "contains unsafe or excessive text");
                }
                Ok(())
            }
            Self::Link { label, url } => {
                require_english(label, "panels.blocks.label")?;
                if !is_https_url(url) {
                    return invalid("panels.blocks.url", "must use HTTPS without userinfo");
                }
                Ok(())
            }
        }
    }
}

/// One signed repository catalog.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddonRepositoryIndex {
    pub schema_version: u32,
    pub repository_id: String,
    pub repository_name: String,
    pub sequence: u64,
    pub generated_at: String,
    pub expires_at: String,
    #[serde(default)]
    pub addons: Vec<CatalogAddon>,
}

/// Add-on metadata shown before package download.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogAddon {
    pub id: String,
    pub version: Version,
    pub name: BTreeMap<String, String>,
    pub summary: BTreeMap<String, String>,
    #[serde(default)]
    pub icon: Option<CatalogAddonIcon>,
    pub license: String,
    pub source_url: String,
    pub host_version: VersionReq,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<AddonDependency>,
    #[serde(default)]
    pub permissions: AddonPermissions,
    pub packages: BTreeMap<String, CatalogPackage>,
}

impl CatalogAddon {
    /// Validate signed metadata shown before a package is downloaded.
    ///
    /// # Errors
    ///
    /// Returns an error when catalog metadata violates the v1 contract.
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_addon_id(&self.id)?;
        validate_package_version(&self.version)?;
        require_english(&self.name, "name")?;
        require_english(&self.summary, "summary")?;
        if let Some(icon) = &self.icon {
            icon.validate()?;
        }
        if self.license.trim().is_empty() {
            return invalid("license", "must not be empty");
        }
        if !is_https_url(&self.source_url) {
            return invalid("source_url", "must use HTTPS without userinfo");
        }
        let mut capability_ids = BTreeSet::new();
        for capability in &self.capabilities {
            validate_capability_id(capability)?;
            if !capability_ids.insert(capability.as_str()) {
                return invalid("capabilities", "duplicate capability id");
            }
        }
        validate_dependencies(&self.id, &self.dependencies)?;
        self.permissions.validate()?;
        if self.packages.is_empty() {
            return invalid("packages", "at least one package is required");
        }
        for (rid, package) in &self.packages {
            if rid != "any" && !is_supported_rid(rid) {
                return invalid("packages", "unknown ErsatzRS RID");
            }
            if !is_package_url(&package.url) {
                return invalid(
                    "packages.url",
                    "must use HTTPS without userinfo, query, or fragment",
                );
            }
            if package.size == 0 {
                return invalid("packages.size", "must be positive");
            }
            if !is_sha256_hex(&package.sha256) {
                return invalid("packages.sha256", "must be a hexadecimal SHA-256 digest");
            }
        }
        Ok(())
    }
}

/// Signed remote icon metadata for a catalog entry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogAddonIcon {
    pub url: String,
    pub media_type: AddonIconMediaType,
    pub size: u64,
    pub sha256: String,
}

impl CatalogAddonIcon {
    fn validate(&self) -> Result<(), ContractError> {
        if !is_https_url(&self.url) {
            return invalid("icon.url", "must use HTTPS without userinfo");
        }
        if self.size == 0 || self.size > 2 * 1024 * 1024 {
            return invalid("icon.size", "must be between 1 byte and 2 MiB");
        }
        if !is_sha256_hex(&self.sha256) {
            return invalid("icon.sha256", "must be lowercase SHA-256 hex");
        }
        Ok(())
    }
}

/// Persisted provenance assigned when an add-on is installed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonTrustTier {
    Verified,
    Unverified,
    Sideloaded,
}

impl AddonTrustTier {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::Unverified => "unverified",
            Self::Sideloaded => "sideloaded",
        }
    }
}

/// Strict manifest carried by a repository installation ZIP.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryManifest {
    pub id: String,
    pub name: String,
    pub index_url: String,
    #[serde(default)]
    pub public_key: Option<String>,
    #[serde(default)]
    pub description: BTreeMap<String, String>,
}

impl RepositoryManifest {
    /// Validate a repository-package manifest before it becomes a trust record.
    ///
    /// # Errors
    ///
    /// Returns an error when the identity, display metadata, index URL, or
    /// localized description violates the repository-package contract.
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_addon_id(&self.id)?;
        if self.name.trim().is_empty() || self.name.len() > 200 {
            return invalid("name", "must be between 1 and 200 characters");
        }
        if !is_https_url(&self.index_url) {
            return invalid("index_url", "must use HTTPS without userinfo");
        }
        if !self.description.is_empty() {
            require_english(&self.description, "description")?;
        }
        Ok(())
    }
}

/// One immutable package asset selected by `RID` or `any`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogPackage {
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

/// Result of addon.check.v1.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddonCheckResult {
    pub status: AddonCheckStatus,
    pub code: String,
    pub message: String,
}

/// Stable readiness state returned by an add-on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonCheckStatus {
    Ready,
    Warning,
    Unavailable,
}

/// Supported release `RID`s shared with the host runtime matrix.
#[must_use]
pub fn is_supported_rid(value: &str) -> bool {
    matches!(
        value,
        "linux-x64" | "linux-arm64" | "linux-arm" | "osx-x64" | "osx-arm64" | "win-x64"
    )
}

/// Validate a reverse-DNS add-on identifier.
///
/// # Errors
///
/// Returns a safe validation error when the identifier is not stable/safe.
pub fn validate_addon_id(value: &str) -> Result<(), ContractError> {
    let parts = value.split('.').collect::<Vec<_>>();
    if value.len() > 255
        || parts.len() < 3
        || parts.iter().any(|part| {
            part.is_empty()
                || part.starts_with('-')
                || part.ends_with('-')
                || !part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
    {
        return invalid("id", "must be a lowercase reverse-DNS identifier");
    }
    Ok(())
}

fn validate_package_version(version: &Version) -> Result<(), ContractError> {
    if version.to_string().len() > MAX_PACKAGE_VERSION_BYTES {
        return invalid("version", "serialized SemVer is too long");
    }
    Ok(())
}

fn validate_dependencies(
    addon_id: &str,
    dependencies: &[AddonDependency],
) -> Result<(), ContractError> {
    if dependencies.len() > 100 {
        return invalid("dependencies", "too many dependencies");
    }
    let mut ids = BTreeSet::new();
    for dependency in dependencies {
        validate_addon_id(&dependency.id)?;
        if dependency.id == addon_id {
            return invalid("dependencies", "an add-on cannot depend on itself");
        }
        if !ids.insert(dependency.id.as_str()) {
            return invalid("dependencies", "duplicate dependency id");
        }
    }
    Ok(())
}

fn validate_capability_id(value: &str) -> Result<(), ContractError> {
    if value.len() > 128 {
        return invalid("capability.id", "is too long");
    }
    let Some((name, version)) = value.rsplit_once(".v") else {
        return invalid("capability.id", "must end in .v<major>");
    };
    if name.is_empty()
        || !name.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'.')
        })
        || version.parse::<u32>().is_err()
    {
        return invalid(
            "capability.id",
            "contains unsupported characters or version",
        );
    }
    Ok(())
}

fn require_english(
    values: &BTreeMap<String, String>,
    field: &'static str,
) -> Result<(), ContractError> {
    if values
        .get("en-US")
        .is_none_or(|value| value.trim().is_empty())
    {
        return invalid(field, "requires a non-empty en-US value");
    }
    Ok(())
}

fn is_safe_relative_path(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 1_024
        && !value.starts_with(['/', '\\'])
        && !value.contains(':')
        && value
            .split(['/', '\\'])
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

fn is_safe_single_component(value: &str) -> bool {
    is_safe_relative_path(value) && !value.contains(['/', '\\'])
}

fn is_setting_key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && !value.starts_with('_')
        && !value.ends_with('_')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
}

fn is_environment_reference_name(value: &str) -> bool {
    is_setting_key(value) && !value.starts_with(|character: char| character.is_ascii_digit())
}

fn is_lower_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_https_url(value: &str) -> bool {
    value.len() <= 2_048
        && url::Url::parse(value).is_ok_and(|url| {
            url.scheme() == "https"
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
        })
}

fn is_package_url(value: &str) -> bool {
    is_https_url(value)
        && url::Url::parse(value).is_ok_and(|url| url.query().is_none() && url.fragment().is_none())
}

fn invalid<T>(field: &'static str, message: impl Into<String>) -> Result<T, ContractError> {
    Err(ContractError::Invalid {
        field,
        message: message.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn localized(value: &str) -> BTreeMap<String, String> {
        BTreeMap::from([("en-US".to_owned(), value.to_owned())])
    }

    fn valid_manifest() -> AddonManifest {
        AddonManifest {
            manifest_version: 1,
            id: "org.ersatzrs.addon.fixture".to_owned(),
            version: Version::new(1, 0, 0),
            name: localized("Fixture"),
            summary: localized("Fixture add-on"),
            icon: None,
            license: "GPL-3.0-or-later".to_owned(),
            source_url: "https://example.test/source".to_owned(),
            host_version: VersionReq::STAR,
            capabilities: vec![AddonCapability {
                id: CAPABILITY_ADDON_CHECK_V1.to_owned(),
            }],
            dependencies: Vec::new(),
            entrypoints: BTreeMap::from([(
                "any".to_owned(),
                AddonEntrypoint {
                    path: "bin/fixture".to_owned(),
                    kind: AddonEntrypointKind::Native,
                },
            )]),
            permissions: AddonPermissions::default(),
            settings: Vec::new(),
            media_list_storage: None,
            panels: Vec::new(),
        }
    }

    #[test]
    fn manifest_rejects_paths_outside_the_package() {
        let mut manifest = valid_manifest();
        manifest.entrypoints.get_mut("any").unwrap().path = "../escape".to_owned();
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn schema_v1_remains_readable_but_cannot_declare_an_icon() {
        let mut manifest = valid_manifest();
        manifest.icon = Some(AddonPackageIcon {
            path: "icon.png".to_owned(),
            media_type: AddonIconMediaType::Png,
        });
        assert!(manifest.validate().is_err());

        manifest.manifest_version = 2;
        manifest.validate().expect("schema v2 icon");
    }

    #[test]
    fn schema_v3_secret_setting_can_suggest_only_a_reference_location() {
        let mut manifest = valid_manifest();
        manifest.manifest_version = 3;
        manifest.settings.push(AddonSettingDefinition {
            key: "CLIENT_ID".to_owned(),
            label: localized("Client ID"),
            kind: AddonSettingKind::SecretReference,
            required: true,
            default: None,
            options: Vec::new(),
            suggested_reference: Some(AddonSuggestedSecretReference {
                kind: AddonSuggestedSecretReferenceKind::Environment,
                reference: "TRAKT__CLIENTID".to_owned(),
            }),
        });
        manifest.validate().expect("safe suggested reference");

        manifest.manifest_version = 1;
        assert!(manifest.validate().is_err());

        manifest.manifest_version = 2;
        assert!(manifest.validate().is_err());

        manifest.manifest_version = 3;

        manifest.settings[0]
            .suggested_reference
            .as_mut()
            .unwrap()
            .reference = "literal value".to_owned();
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn schema_v3_media_list_storage_requires_the_complete_stream_contract() {
        let mut manifest = valid_manifest();
        manifest.manifest_version = 3;
        manifest.settings.push(AddonSettingDefinition {
            key: "MEDIA_STORAGE_PATH".to_owned(),
            label: localized("Media storage location"),
            kind: AddonSettingKind::Path,
            required: false,
            default: None,
            options: Vec::new(),
            suggested_reference: None,
        });
        manifest.media_list_storage = Some(AddonMediaListStorage {
            path_setting: "MEDIA_STORAGE_PATH".to_owned(),
            default_subdirectory: "fixture_media".to_owned(),
            source_name: "fixture".to_owned(),
        });
        assert!(manifest.validate().is_err());

        manifest.capabilities.extend(
            [
                CAPABILITY_MEDIA_LIST_LIST_V1,
                CAPABILITY_REMOTE_STREAM_LIST_V1,
                CAPABILITY_REMOTE_STREAM_PLAY_V1,
            ]
            .into_iter()
            .map(|id| AddonCapability { id: id.to_owned() }),
        );
        manifest
            .validate()
            .expect("complete managed storage contract");

        manifest.manifest_version = 2;
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn repository_package_manifest_accepts_an_optional_key() {
        let manifest = RepositoryManifest {
            id: "org.example.addons".to_owned(),
            name: "Example add-ons".to_owned(),
            index_url: "https://example.test/index-v2.json".to_owned(),
            public_key: None,
            description: localized("Example repository"),
        };
        manifest.validate().expect("valid repository package");
    }

    #[test]
    fn manifest_rejects_self_and_duplicate_dependencies() {
        let dependency = AddonDependency {
            id: "org.ersatzrs.addon.fixture.dependency".to_owned(),
            version: VersionReq::STAR,
            optional: false,
        };
        let mut self_dependent = valid_manifest();
        self_dependent.dependencies.push(AddonDependency {
            id: self_dependent.id.clone(),
            version: VersionReq::STAR,
            optional: false,
        });
        assert!(self_dependent.validate().is_err());

        let mut duplicated = valid_manifest();
        duplicated.dependencies = vec![dependency.clone(), dependency];
        assert!(duplicated.validate().is_err());
    }

    #[test]
    fn package_version_is_bounded_for_database_and_path_portability() {
        let version = Version::parse(&format!("1.0.0-{}", "a".repeat(129)))
            .expect("syntactically valid long SemVer");

        assert!(validate_package_version(&version).is_err());
    }

    #[test]
    fn signed_package_url_cannot_persist_query_credentials() {
        assert!(is_package_url("https://example.test/addon.zip"));
        assert!(!is_package_url(
            "https://example.test/addon.zip?token=secret"
        ));
        assert!(!is_package_url("https://example.test/addon.zip#download"));
    }

    #[test]
    fn supported_rids_match_the_release_matrix() {
        for rid in [
            "linux-x64",
            "linux-arm64",
            "linux-arm",
            "osx-x64",
            "osx-arm64",
            "win-x64",
        ] {
            assert!(is_supported_rid(rid));
        }
        assert!(!is_supported_rid("freebsd-x64"));
    }

    #[test]
    fn declarative_panel_rejects_executable_links() {
        let panel = AddonPanel {
            id: "unsafe".to_owned(),
            title: localized("Unsafe"),
            blocks: vec![AddonPanelBlock::Link {
                label: localized("Open"),
                url: "javascript:alert(1)".to_owned(),
            }],
        };
        assert!(panel.validate().is_err());
    }
}
