//! Adapter for provider-neutral media-list add-ons.

use std::collections::{BTreeMap, HashSet};
use std::time::Duration;

use async_trait::async_trait;
use ersatzrs_addon_contract::CAPABILITY_MEDIA_LIST_LIST_V1;
use ersatzrs_core::addon_media_list::{
    AddonMediaListProvider, AddonMediaListRecord, EnumeratedAddonMediaList,
    EnumeratedAddonMediaListItem,
};
use ersatzrs_core::repositories::{RepositoryError, Result};

use crate::addons::{invoke_enabled_addon, AddonInvocation, AddonRuntimeContext};
use crate::db::DbPool;

const SOURCE_URL_ENVIRONMENT: &str = "ERSATZRS_MEDIA_LIST_URL";
const MAX_OUTPUT_BYTES: usize = 16 * 1024 * 1024;
const MAX_ITEMS: usize = 10_000;

/// Invokes installed add-ons that implement `media-list.list.v1`.
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
        let output = invoke_enabled_addon(
            &self.pool,
            addon_id,
            AddonInvocation {
                capability: CAPABILITY_MEDIA_LIST_LIST_V1,
                operation: "list",
                runtime: &AddonRuntimeContext::default(),
                extra_environment: &environment,
                timeout: Duration::from_mins(1),
                max_output_bytes: MAX_OUTPUT_BYTES,
            },
        )
        .await
        .map_err(|error| {
            RepositoryError::unavailable("invoke media-list add-on", error.to_string())
        })?;

        decode_media_list_output(&output)
    }
}

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

    if provider_id.trim().is_empty() || name.trim().is_empty() {
        return Err(RepositoryError::Decode {
            kind: "media-list add-on output",
            value: "list provider_id and name must not be empty".to_owned(),
        });
    }

    let mut items = Vec::new();
    let mut provider_ids = HashSet::new();
    let mut ranks = HashSet::new();
    for record in records {
        if items.len() == MAX_ITEMS {
            return Err(RepositoryError::Decode {
                kind: "media-list add-on output",
                value: format!("list exceeds the {MAX_ITEMS}-item limit"),
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
        } = decode_record(record)?
        else {
            return Err(RepositoryError::Decode {
                kind: "media-list add-on output",
                value: "list record may only appear once".to_owned(),
            });
        };
        if provider_id.trim().is_empty()
            || display_title.trim().is_empty()
            || title.trim().is_empty()
            || rank < 0
        {
            return Err(RepositoryError::Decode {
                kind: "media-list add-on output",
                value: "item provider_id/display_title/title must be set and rank must be \
                        non-negative"
                    .to_owned(),
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
            kind,
            guids,
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
    serde_json::from_str(line).map_err(|error| RepositoryError::Decode {
        kind: "media-list add-on output",
        value: error.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::decode_media_list_output;

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
}
