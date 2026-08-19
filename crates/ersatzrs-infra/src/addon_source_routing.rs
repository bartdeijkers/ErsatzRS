//! Route one operator-supplied source URL to the add-on that should process it.
//!
//! ErsatzTV has no equivalent: it has no add-on platform, so every provider is
//! a built-in client. ADR 0072 owns the deliberate extension.
//!
//! The host stays provider-neutral. A package declares the hosts it offers to
//! handle in `default_source_hosts`, and this module only compares that
//! declaration against the URL. It performs no provider protocol work and never
//! starts an add-on process, so it is cheap enough to call while the operator
//! is still typing.

use ersatzrs_addon_contract::{
    match_declared_source_host, network_host_pattern_matches, SourceAddonMatch,
};

use crate::addons::{default_remote_stream_addon, list_installed_addons, AddonError};
use crate::db::DbPool;

/// Which add-on should process one operator source URL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolvedSourceAddon {
    /// A package declares this host as one it handles.
    Declared(String),
    /// No package claims the host; the configured default accepted it.
    Default(String),
    /// Nothing claims the host and no default applies.
    None,
    /// Several packages claim the host equally well.
    ///
    /// The host deliberately does not guess: the operator picks.
    Ambiguous(Vec<String>),
}

impl ResolvedSourceAddon {
    /// The selected add-on, if exactly one applies.
    #[must_use]
    pub fn addon_id(&self) -> Option<&str> {
        match self {
            Self::Declared(addon_id) | Self::Default(addon_id) => Some(addon_id.as_str()),
            Self::None | Self::Ambiguous(_) => None,
        }
    }
}

/// Resolve the add-on that should process `url`.
///
/// `required_capabilities` is a family: a package qualifies when it declares
/// any one of them, because contract versions coexist (`media-list.list.v1`
/// through `v3` all enumerate a list).
///
/// A declared handler always wins. The configured default is a fallback for
/// bounded HTTP(S) URLs only: applying it to another scheme would hand a source
/// `FFmpeg` can open directly to a package that cannot fetch it. The default is
/// also skipped when its own `permissions.network_hosts` exclude the host,
/// because enumeration would then fail at
/// `ersatzrs_addon_contract::validate_operator_source_url` anyway.
///
/// # Errors
///
/// Returns an error when installed packages cannot be read.
pub async fn resolve_source_addon(
    pool: &DbPool,
    url: &str,
    required_capabilities: &[&str],
) -> Result<ResolvedSourceAddon, AddonError> {
    let Some((host, is_web)) = parse_source_host(url) else {
        return Ok(ResolvedSourceAddon::None);
    };
    let installed = list_installed_addons(pool)
        .await?
        .into_iter()
        .filter(|item| {
            item.enabled
                && required_capabilities
                    .iter()
                    .any(|capability| item.manifest.has_capability(capability))
        })
        .collect::<Vec<_>>();

    let candidates = installed
        .iter()
        .map(|item| {
            (
                item.addon_id.as_str(),
                item.manifest.default_source_hosts.as_slice(),
            )
        })
        .collect::<Vec<_>>();
    match match_declared_source_host(&host, &candidates) {
        SourceAddonMatch::Declared(addon_id) => return Ok(ResolvedSourceAddon::Declared(addon_id)),
        SourceAddonMatch::Ambiguous(addon_ids) => {
            return Ok(ResolvedSourceAddon::Ambiguous(addon_ids))
        }
        SourceAddonMatch::None => {}
    }

    if !is_web {
        return Ok(ResolvedSourceAddon::None);
    }
    let Some(default_addon) = default_remote_stream_addon(pool).await? else {
        return Ok(ResolvedSourceAddon::None);
    };
    let permitted = installed
        .iter()
        .find(|item| item.addon_id == default_addon)
        .is_some_and(|item| {
            let hosts = &item.manifest.permissions.network_hosts;
            hosts.is_empty()
                || hosts
                    .iter()
                    .any(|pattern| network_host_pattern_matches(pattern, &host))
        });
    if permitted {
        Ok(ResolvedSourceAddon::Default(default_addon))
    } else {
        Ok(ResolvedSourceAddon::None)
    }
}

/// Extract the lowercase host, reporting whether the URL is HTTP(S).
fn parse_source_host(url: &str) -> Option<(String, bool)> {
    let parsed = url::Url::parse(url.trim()).ok()?;
    let host = parsed.host_str()?.to_ascii_lowercase();
    Some((host, matches!(parsed.scheme(), "http" | "https")))
}

#[cfg(test)]
mod tests {
    use ersatzrs_addon_contract::{
        AddonCapability, AddonEntrypoint, AddonEntrypointKind, AddonManifest, AddonPermissions,
        CAPABILITY_ADDON_CHECK_V1, CAPABILITY_MEDIA_LIST_LIST_V1,
    };
    use semver::{Version, VersionReq};
    use std::collections::{BTreeMap, BTreeSet};

    use super::*;

    fn manifest(addon_id: &str, declared: &[&str], permitted: &[&str]) -> AddonManifest {
        AddonManifest {
            manifest_version: 4,
            id: addon_id.to_owned(),
            version: Version::new(1, 0, 0),
            name: BTreeMap::from([("en-US".to_owned(), addon_id.to_owned())]),
            summary: BTreeMap::from([("en-US".to_owned(), addon_id.to_owned())]),
            icon: None,
            license: "Zlib".to_owned(),
            source_url: "https://example.test/source".to_owned(),
            host_version: VersionReq::STAR,
            capabilities: vec![
                AddonCapability {
                    id: CAPABILITY_ADDON_CHECK_V1.to_owned(),
                },
                AddonCapability {
                    id: CAPABILITY_MEDIA_LIST_LIST_V1.to_owned(),
                },
            ],
            dependencies: Vec::new(),
            entrypoints: BTreeMap::from([(
                "any".to_owned(),
                AddonEntrypoint {
                    path: "addon.sh".to_owned(),
                    kind: AddonEntrypointKind::PosixShell,
                },
            )]),
            permissions: AddonPermissions {
                network_hosts: permitted.iter().map(|host| (*host).to_owned()).collect(),
                ..AddonPermissions::default()
            },
            settings: Vec::new(),
            media_list_storage: None,
            default_source_hosts: declared.iter().map(|host| (*host).to_owned()).collect(),
            panels: Vec::new(),
        }
    }

    /// The declaration is the contract, so a fixture that would be rejected at
    /// install time must not be used to prove routing behavior.
    #[test]
    fn routing_fixtures_are_installable_manifests() {
        manifest(
            "org.example.addon.one",
            &["schatkamer.example.test"],
            &["schatkamer.example.test"],
        )
        .validate()
        .expect("declared host is permitted");
    }

    #[test]
    fn a_declared_host_outranks_the_configured_default() {
        let claimed = manifest(
            "org.example.addon.one",
            &["schatkamer.example.test"],
            &["schatkamer.example.test"],
        );
        let candidates = vec![(claimed.id.as_str(), claimed.default_source_hosts.as_slice())];
        assert_eq!(
            match_declared_source_host("schatkamer.example.test", &candidates),
            SourceAddonMatch::Declared("org.example.addon.one".to_owned()),
        );
        assert_eq!(
            match_declared_source_host("unrelated.invalid", &candidates),
            SourceAddonMatch::None,
        );
    }

    #[test]
    fn only_bounded_http_urls_can_reach_the_default_fallback() {
        assert_eq!(
            parse_source_host("https://Schatkamer.Example.test/lijst/1"),
            Some(("schatkamer.example.test".to_owned(), true)),
        );
        assert_eq!(
            parse_source_host("rtsp://camera.example.test/live"),
            Some(("camera.example.test".to_owned(), false)),
        );
        assert_eq!(parse_source_host("not a url"), None);
        assert_eq!(parse_source_host("file:///srv/media/clip.mp4"), None);
    }

    #[test]
    fn every_declared_host_stays_within_its_permission_declaration() {
        // Guards the invariant the fallback relies on: a package can only be
        // selected for a host it is allowed to contact.
        let package = manifest(
            "org.example.addon.one",
            &["media.example.test"],
            &["*.example.test"],
        );
        package.validate().expect("wildcard covers the claim");
        let permitted = package
            .permissions
            .network_hosts
            .iter()
            .any(|pattern| network_host_pattern_matches(pattern, "media.example.test"));
        assert!(permitted);

        let declared = package
            .default_source_hosts
            .iter()
            .collect::<BTreeSet<_>>()
            .len();
        assert_eq!(declared, 1);
    }
}
