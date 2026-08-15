//! Adapter for provider-neutral media-list add-ons.

use std::collections::{BTreeMap, HashSet};
use std::time::Duration;

use async_trait::async_trait;
use ersatzrs_addon_contract::{
    AddonMediaListContentKind as WireContentKind,
    AddonMediaListItemAvailability as WireAvailability, AddonMediaListItemKind as WireItemKind,
    AddonMediaListRecord, CAPABILITY_MEDIA_LIST_LIST_V1, CAPABILITY_MEDIA_LIST_LIST_V2,
    MAX_AVAILABILITY_REASON_BYTES, MAX_LIST_ITEMS, MAX_MEDIA_LIST_OUTPUT_BYTES,
    MAX_PROVIDER_ID_BYTES,
};
use ersatzrs_core::addon_media_list::{
    AddonMediaListProvider, EnumeratedAddonMediaList, EnumeratedAddonMediaListItem,
};
use ersatzrs_core::repositories::{RepositoryError, Result};

use crate::addons::{
    invoke_enabled_addon, load_installed_addon, AddonInvocation, AddonRuntimeContext,
};
use crate::db::DbPool;

const SOURCE_URL_ENVIRONMENT: &str = "ERSATZRS_MEDIA_LIST_URL";

/// Invokes installed add-ons that implement media-list list v1 or v2.
#[derive(Clone)]
pub struct InstalledAddonMediaListProvider {
    pool: DbPool,
}

impl InstalledAddonMediaListProvider {
    #[must_use]
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AddonMediaListProvider for InstalledAddonMediaListProvider {
    async fn enumerate(
        &self,
        addon_id: &str,
        source_url: &str,
    ) -> Result<EnumeratedAddonMediaList> {
        let mut environment = BTreeMap::new();
        environment.insert(SOURCE_URL_ENVIRONMENT.to_owned(), source_url.to_owned());
        // Every other invocation path supplies the managed runtime; a list
        // enumeration that omitted it could not run an add-on that declares
        // `managed_ffmpeg`, because resolving its environment requires both
        // paths.
        let runtime = AddonRuntimeContext {
            ffmpeg_path: Some(ersatzrs_ffmpeg::ffmpeg_path()),
            ffprobe_path: Some(ersatzrs_ffmpeg::ffprobe_path()),
        };
        let installation = load_installed_addon(&self.pool, addon_id)
            .await
            .map_err(|error| {
                RepositoryError::unavailable("load media-list add-on", error.to_string())
            })?
            .ok_or_else(|| {
                RepositoryError::unavailable("load media-list add-on", "add-on is not installed")
            })?;
        let capability = if installation
            .manifest
            .has_capability(CAPABILITY_MEDIA_LIST_LIST_V2)
        {
            CAPABILITY_MEDIA_LIST_LIST_V2
        } else {
            CAPABILITY_MEDIA_LIST_LIST_V1
        };
        let output = invoke_enabled_addon(
            &self.pool,
            addon_id,
            AddonInvocation {
                capability,
                operation: "list",
                runtime: &runtime,
                extra_environment: &environment,
                timeout: Duration::from_mins(1),
                max_output_bytes: MAX_MEDIA_LIST_OUTPUT_BYTES,
            },
        )
        .await
        .map_err(|error| {
            RepositoryError::unavailable("invoke media-list add-on", error.to_string())
        })?;

        decode_media_list_output(&output)
    }
}

#[allow(clippy::too_many_lines)]
fn decode_media_list_output(output: &[u8]) -> Result<EnumeratedAddonMediaList> {
    let text = std::str::from_utf8(output).map_err(|_| RepositoryError::Decode {
        kind: "media-list add-on output",
        value: "output is not UTF-8".to_owned(),
    })?;
    let mut records = text.lines().filter(|line| !line.trim().is_empty());
    let header = records.next().ok_or_else(|| RepositoryError::Decode {
        kind: "media-list add-on output",
        value: "missing list record".to_owned(),
    })?;
    let AddonMediaListRecord::List {
        provider_id,
        name,
        description,
    } = decode_record(header)?
    else {
        return Err(RepositoryError::Decode {
            kind: "media-list add-on output",
            value: "first record must describe the list".to_owned(),
        });
    };

    if provider_id.trim().is_empty()
        || provider_id.len() > MAX_PROVIDER_ID_BYTES
        || name.trim().is_empty()
    {
        return Err(RepositoryError::Decode {
            kind: "media-list add-on output",
            value: "list provider_id must be non-empty and bounded; name must not be empty"
                .to_owned(),
        });
    }

    let mut items = Vec::new();
    let mut provider_ids = HashSet::new();
    let mut ranks = HashSet::new();
    for record in records {
        if items.len() == MAX_LIST_ITEMS {
            return Err(RepositoryError::Decode {
                kind: "media-list add-on output",
                value: format!("list exceeds the {MAX_LIST_ITEMS}-item limit"),
            });
        }
        let AddonMediaListRecord::Item {
            provider_id,
            rank,
            display_title,
            title,
            year,
            season,
            episode,
            kind,
            guids,
            source_url,
            availability,
            availability_reason,
            content_kind,
        } = decode_record(record)?
        else {
            return Err(RepositoryError::Decode {
                kind: "media-list add-on output",
                value: "list record may only appear once".to_owned(),
            });
        };
        if provider_id.trim().is_empty()
            || provider_id.len() > MAX_PROVIDER_ID_BYTES
            || display_title.trim().is_empty()
            || title.trim().is_empty()
            || rank < 0
            || availability_reason
                .as_ref()
                .is_some_and(|reason| reason.len() > MAX_AVAILABILITY_REASON_BYTES)
        {
            return Err(RepositoryError::Decode {
                kind: "media-list add-on output",
                value:
                    "item provider_id/display_title/title must be set and bounded; rank must be \
                        non-negative and availability_reason must be bounded"
                        .to_owned(),
            });
        }
        if kind == WireItemKind::RemoteStream && source_url.as_deref().is_none_or(str::is_empty) {
            return Err(RepositoryError::Decode {
                kind: "media-list add-on output",
                value: "remote_stream items require source_url".to_owned(),
            });
        }
        if !provider_ids.insert(provider_id.clone()) || !ranks.insert(rank) {
            return Err(RepositoryError::Decode {
                kind: "media-list add-on output",
                value: "item provider_id and rank must be unique".to_owned(),
            });
        }
        items.push(EnumeratedAddonMediaListItem {
            provider_id,
            rank,
            display_title,
            title,
            year,
            season,
            episode,
            kind: domain_item_kind(kind),
            guids,
            source_url,
            availability: domain_availability(availability),
            availability_reason,
            content_kind: domain_content_kind(content_kind),
        });
    }
    items.sort_by_key(|item| item.rank);

    Ok(EnumeratedAddonMediaList {
        provider_id,
        name,
        description,
        items,
    })
}

fn decode_record(line: &str) -> Result<AddonMediaListRecord> {
    let record = serde_json::from_str::<AddonMediaListRecord>(line).map_err(|_| {
        RepositoryError::Decode {
            kind: "media-list add-on output",
            value: "record violates the bounded media-list JSON contract".to_owned(),
        }
    })?;
    record.validate().map_err(|_| RepositoryError::Decode {
        kind: "media-list add-on output",
        value: "record violates the bounded media-list JSON contract".to_owned(),
    })?;
    Ok(record)
}

const fn domain_item_kind(kind: WireItemKind) -> ersatzrs_domain::AddonMediaListItemKind {
    match kind {
        WireItemKind::Movie => ersatzrs_domain::AddonMediaListItemKind::Movie,
        WireItemKind::Show => ersatzrs_domain::AddonMediaListItemKind::Show,
        WireItemKind::Season => ersatzrs_domain::AddonMediaListItemKind::Season,
        WireItemKind::Episode => ersatzrs_domain::AddonMediaListItemKind::Episode,
        WireItemKind::RemoteStream => ersatzrs_domain::AddonMediaListItemKind::RemoteStream,
    }
}

const fn domain_availability(
    availability: WireAvailability,
) -> ersatzrs_domain::AddonMediaListItemAvailability {
    match availability {
        WireAvailability::Available => ersatzrs_domain::AddonMediaListItemAvailability::Available,
        WireAvailability::Unavailable => {
            ersatzrs_domain::AddonMediaListItemAvailability::Unavailable
        }
        WireAvailability::Unknown => ersatzrs_domain::AddonMediaListItemAvailability::Unknown,
    }
}

const fn domain_content_kind(kind: WireContentKind) -> ersatzrs_domain::AddonMediaListContentKind {
    match kind {
        WireContentKind::Auto => ersatzrs_domain::AddonMediaListContentKind::Auto,
        WireContentKind::TelevisionEpisode => {
            ersatzrs_domain::AddonMediaListContentKind::TelevisionEpisode
        }
        WireContentKind::Movie => ersatzrs_domain::AddonMediaListContentKind::Movie,
        WireContentKind::MusicVideo => ersatzrs_domain::AddonMediaListContentKind::MusicVideo,
        WireContentKind::OtherVideo => ersatzrs_domain::AddonMediaListContentKind::OtherVideo,
    }
}

#[cfg(test)]
mod tests {
    use super::decode_media_list_output;

    /// Refuses to enumerate unless the host supplied its managed `FFmpeg`
    /// paths, the way a manifest declaring `managed_ffmpeg` expects.
    #[cfg(unix)]
    const FIXTURE_SCRIPT: &str = r#"#!/bin/sh
set -eu
: "${FFMPEG_BIN:?managed ffmpeg is required}"
: "${FFPROBE_BIN:?managed ffprobe is required}"
printf '%s\n' '{"record_type":"list","provider_id":"fixture/list","name":"Fixture"}'
printf '%s\n' '{"record_type":"item","provider_id":"movie:1","rank":0,"display_title":"Arrival","title":"Arrival","kind":"movie","guids":[]}'
"#;

    #[cfg(unix)]
    const INVALID_AVAILABILITY_FIXTURE_SCRIPT: &str = r#"#!/bin/sh
set -eu
printf '%s\n' '{"record_type":"list","provider_id":"fixture/list","name":"Fixture"}'
printf '%s\n' '{"record_type":"item","provider_id":"movie:1","rank":0,"display_title":"Arrival","title":"Arrival","kind":"movie","guids":[],"availability":"maybe_later"}'
"#;

    #[cfg(unix)]
    async fn fixture_media_list_addon_with_script(
        root: &std::path::Path,
        source: &str,
    ) -> crate::db::DbPool {
        use std::os::unix::fs::PermissionsExt as _;

        tokio::fs::create_dir_all(root)
            .await
            .expect("fixture package folder");
        let script = root.join("fixture.sh");
        tokio::fs::write(&script, source)
            .await
            .expect("fixture script");
        let mut permissions = tokio::fs::metadata(&script)
            .await
            .expect("fixture metadata")
            .permissions();
        permissions.set_mode(0o755);
        tokio::fs::set_permissions(&script, permissions)
            .await
            .expect("fixture permissions");

        let database_url = format!(
            "sqlite:{}",
            root.parent()
                .expect("fixture package has a parent")
                .join("addons.db")
                .display()
        );
        let pool = crate::db::init(&database_url)
            .await
            .expect("fixture database");
        let manifest = serde_json::json!({
            "manifest_version": 1,
            "id": "org.ersatzrs.addon.fixture",
            "version": "1.0.0",
            "name": {"en-US": "Fixture"},
            "summary": {"en-US": "Fixture media-list add-on"},
            "license": "GPL-3.0-or-later",
            "source_url": "https://example.test/source",
            "host_version": "*",
            "capabilities": [
                {"id": "addon.check.v1"},
                {"id": "media-list.list.v1"}
            ],
            "entrypoints": {
                "any": {"path": "fixture.sh", "kind": "posix_shell"}
            },
            "permissions": {
                "network_hosts": [],
                "media_read": false,
                "managed_ffmpeg": true,
                "external_commands": []
            },
            "settings": [],
            "panels": []
        });
        let now = chrono::Utc::now().to_rfc3339();
        let content_sha256 = crate::addons::calculate_addon_content_sha256(root)
            .await
            .expect("hash fixture package");
        sqlx::query(
            r#"INSERT INTO "AddonInstallation" (
                   "AddonId", "RepositoryId", "Version", "Enabled", "PackageSha256", "ContentSha256",
                   "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt"
               ) VALUES (?, NULL, '1.0.0', 1, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind("org.ersatzrs.addon.fixture")
        .bind("a".repeat(64))
        .bind(content_sha256)
        .bind(manifest.to_string())
        .bind(root.to_string_lossy().into_owned())
        .bind(&now)
        .bind(&now)
        .execute(&pool)
        .await
        .expect("fixture add-on installation");
        pool
    }

    #[cfg(unix)]
    async fn fixture_media_list_addon(root: &std::path::Path) -> crate::db::DbPool {
        fixture_media_list_addon_with_script(root, FIXTURE_SCRIPT).await
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn list_invocations_supply_the_managed_ffmpeg_runtime() {
        use ersatzrs_core::addon_media_list::AddonMediaListProvider as _;

        let temporary = tempfile::tempdir().expect("temporary folder");
        let root = temporary.path().join("package");
        let pool = fixture_media_list_addon(&root).await;

        let list = super::InstalledAddonMediaListProvider::new(pool)
            .enumerate("org.ersatzrs.addon.fixture", "https://example.test/list")
            .await
            .expect("enumerate fixture list");

        assert_eq!(list.provider_id, "fixture/list");
        assert_eq!(list.items.len(), 1);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn provider_enumeration_rejects_unknown_availability_without_echoing_it() {
        use ersatzrs_core::addon_media_list::AddonMediaListProvider as _;

        let temporary = tempfile::tempdir().expect("temporary folder");
        let root = temporary.path().join("package");
        let pool =
            fixture_media_list_addon_with_script(&root, INVALID_AVAILABILITY_FIXTURE_SCRIPT).await;

        let error = super::InstalledAddonMediaListProvider::new(pool)
            .enumerate("org.ersatzrs.addon.fixture", "https://example.test/list")
            .await
            .expect_err("unknown availability must reject the enumeration");
        let message = error.to_string();
        assert!(message.contains("bounded media-list JSON contract"));
        assert!(!message.contains("maybe_later"));
        assert!(message.len() < 512);
    }

    #[test]
    fn decodes_bounded_ndjson_contract() {
        let output = br#"{"record_type":"list","provider_id":"alice/watchlist","name":"Watchlist"}
{"record_type":"item","provider_id":"movie:1","rank":0,"display_title":"Arrival (2016)","title":"Arrival","year":2016,"kind":"movie","guids":["imdb://tt2543164"]}
"#;
        let list = decode_media_list_output(output).expect("valid output");
        assert_eq!(list.provider_id, "alice/watchlist");
        assert_eq!(list.items.len(), 1);
    }

    #[test]
    fn availability_contract_accepts_canonical_and_omitted_values() {
        let output = br#"{"record_type":"list","provider_id":"x","name":"X"}
{"record_type":"item","provider_id":"one","rank":0,"display_title":"One","title":"One","kind":"movie","availability":"available"}
{"record_type":"item","provider_id":"two","rank":1,"display_title":"Two","title":"Two","kind":"movie","availability":"unavailable"}
{"record_type":"item","provider_id":"three","rank":2,"display_title":"Three","title":"Three","kind":"movie","availability":"unknown"}
{"record_type":"item","provider_id":"legacy","rank":3,"display_title":"Legacy","title":"Legacy","kind":"movie"}
"#;
        let list = decode_media_list_output(output).expect("canonical availability");
        assert_eq!(
            list.items
                .iter()
                .map(|item| item.availability)
                .collect::<Vec<_>>(),
            [
                ersatzrs_domain::AddonMediaListItemAvailability::Available,
                ersatzrs_domain::AddonMediaListItemAvailability::Unavailable,
                ersatzrs_domain::AddonMediaListItemAvailability::Unknown,
                ersatzrs_domain::AddonMediaListItemAvailability::Available,
            ]
        );
    }

    #[test]
    fn rejects_duplicate_provider_ids() {
        let output = br#"{"record_type":"list","provider_id":"x","name":"X"}
{"record_type":"item","provider_id":"same","rank":0,"display_title":"A","title":"A","kind":"movie"}
{"record_type":"item","provider_id":"same","rank":1,"display_title":"B","title":"B","kind":"movie"}
"#;
        assert!(decode_media_list_output(output).is_err());
    }

    #[test]
    fn rejects_items_without_a_display_title() {
        let output = br#"{"record_type":"list","provider_id":"x","name":"X"}
{"record_type":"item","provider_id":"movie:1","rank":0,"display_title":"","title":"Arrival","kind":"movie"}
"#;

        assert!(decode_media_list_output(output).is_err());
    }

    #[test]
    fn rejects_provider_values_that_exceed_cross_database_bounds() {
        let provider_id = "x".repeat(ersatzrs_addon_contract::MAX_PROVIDER_ID_BYTES + 1);
        let output = format!(
            "{{\"record_type\":\"list\",\"provider_id\":\"list\",\"name\":\"List\"}}\n\
             {{\"record_type\":\"item\",\"provider_id\":\"{provider_id}\",\"rank\":0,\"display_title\":\"Item\",\"title\":\"Item\",\"kind\":\"movie\"}}\n"
        );
        assert!(decode_media_list_output(output.as_bytes()).is_err());
    }
}
