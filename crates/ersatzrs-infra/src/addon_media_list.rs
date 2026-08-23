//! Adapter for provider-neutral media-list add-ons.

use std::collections::{BTreeMap, HashSet};
use std::time::Duration;

use async_trait::async_trait;
use ersatzrs_addon_contract::{
    conform_media_list_discover_output, conform_media_list_enrich_output,
    conform_media_list_output, validate_operator_source_url, AddonArtworkRole as WireArtworkRole,
    AddonMediaListContentKind as WireContentKind,
    AddonMediaListItemAvailability as WireAvailability, AddonMediaListItemKind as WireItemKind,
    AddonMediaListItemRole as WireItemRole, AddonMediaListRecord, AddonMetadataDocument,
    AddonOperationError, MediaListDiscoverLimits, MediaListDiscoverRequest, MediaListDiscoveryPage,
    MediaListEnrichRequest, MediaListEnrichmentResult, RemoteStreamLiveness as WireLiveness,
    CAPABILITY_MEDIA_LIST_IMPORT_V1, CAPABILITY_MEDIA_LIST_LIST_V1, CAPABILITY_MEDIA_LIST_LIST_V2,
    CAPABILITY_MEDIA_LIST_LIST_V3, CAPABILITY_MEDIA_LIST_LIST_V4, CAPABILITY_MEDIA_LIST_LIST_V5,
    MAX_AVAILABILITY_REASON_BYTES, MAX_LIST_ITEMS, MAX_MEDIA_LIST_IMPORT_ITEM_OUTPUT_BYTES,
    MAX_MEDIA_LIST_IMPORT_PAGE_OUTPUT_BYTES, MAX_MEDIA_LIST_OUTPUT_BYTES, MAX_PROVIDER_ID_BYTES,
    OPERATION_CODE_UNSUPPORTED_URL,
};
use ersatzrs_core::addon_media_list::{
    AddonMediaListProvider, EnumeratedAddonMediaList, EnumeratedAddonMediaListItem,
};
use ersatzrs_core::media_chapters::parse_chapter_input;
use ersatzrs_core::repositories::{
    AddonMediaListArtwork, AddonMediaListArtworkRole, AddonMediaListNfoMetadata,
    AddonMediaListNfoPerson,
};
use ersatzrs_core::repositories::{RepositoryError, Result};

use crate::addon_chapters::{fragment_guid, fragment_stream_url};
use crate::addons::{
    invoke_enabled_addon, invoke_enabled_addon_with_input, load_installed_addon, AddonInvocation,
    AddonRuntimeContext,
};
use crate::db::DbPool;

const SOURCE_URL_ENVIRONMENT: &str = "ERSATZRS_MEDIA_LIST_URL";
const CHAPTER_INPUT_ENVIRONMENT: &str = "ERSATZRS_MEDIA_LIST_CHAPTERS";

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

    /// Resolve and validate the optional resumable import transport.
    pub(crate) async fn import_context(
        &self,
        addon_id: &str,
        source_url: &str,
    ) -> Result<Option<InstalledMediaListImportContext>> {
        let installation = load_installed_addon(&self.pool, addon_id)
            .await
            .map_err(|error| {
                RepositoryError::unavailable("load media-list add-on", error.to_string())
            })?
            .ok_or_else(|| {
                RepositoryError::unavailable("load media-list add-on", "add-on is not installed")
            })?;
        if !installation
            .manifest
            .has_capability(CAPABILITY_MEDIA_LIST_IMPORT_V1)
        {
            return Ok(None);
        }
        let record_capability = selected_record_capability(&installation.manifest);
        let source_url = validate_operator_source_url(
            source_url,
            &installation.manifest.permissions.network_hosts,
        )
        .map_err(|error| {
            RepositoryError::unavailable(
                "validate media-list source URL",
                AddonOperationError::new(OPERATION_CODE_UNSUPPORTED_URL, error.to_string())
                    .to_string(),
            )
        })?;
        Ok(Some(InstalledMediaListImportContext {
            source_url,
            record_capability: record_capability.to_owned(),
        }))
    }

    /// Fetch exactly one bounded overview page without opening item details.
    pub(crate) async fn discover(
        &self,
        addon_id: &str,
        context: &InstalledMediaListImportContext,
        cursor: Option<&str>,
    ) -> Result<MediaListDiscoveryPage> {
        let request = MediaListDiscoverRequest {
            source_url: context.source_url.clone(),
            record_capability: context.record_capability.clone(),
            cursor: cursor.map(ToOwned::to_owned),
            limits: MediaListDiscoverLimits::default(),
        };
        request
            .validate()
            .map_err(|error| RepositoryError::Decode {
                kind: "media-list discover request",
                value: error.to_string(),
            })?;
        let input = serde_json::to_vec(&request).map_err(|error| RepositoryError::Decode {
            kind: "media-list discover request",
            value: error.to_string(),
        })?;
        let runtime = managed_runtime_context();
        let output = invoke_enabled_addon_with_input(
            &self.pool,
            addon_id,
            AddonInvocation {
                capability: CAPABILITY_MEDIA_LIST_IMPORT_V1,
                operation: "discover",
                runtime: &runtime,
                extra_environment: &BTreeMap::new(),
                timeout: Duration::from_secs(45),
                max_output_bytes: MAX_MEDIA_LIST_IMPORT_PAGE_OUTPUT_BYTES,
            },
            &input,
        )
        .await
        .map_err(|error| {
            RepositoryError::unavailable("invoke media-list discovery", error.to_string())
        })?;
        conform_media_list_discover_output(&output, cursor.is_none()).map_err(|error| {
            RepositoryError::Decode {
                kind: "media-list discover output",
                value: error.to_string(),
            }
        })
    }

    /// Enrich exactly one committed overview item through the same bounded runner.
    pub(crate) async fn enrich(
        &self,
        addon_id: &str,
        context: &InstalledMediaListImportContext,
        provider_id: &str,
        overview_fingerprint: &str,
        item: AddonMediaListRecord,
    ) -> Result<MediaListEnrichmentResult> {
        let request = MediaListEnrichRequest {
            source_url: context.source_url.clone(),
            record_capability: context.record_capability.clone(),
            provider_id: provider_id.to_owned(),
            overview_fingerprint: overview_fingerprint.to_owned(),
            item,
        };
        request
            .validate()
            .map_err(|error| RepositoryError::Decode {
                kind: "media-list enrich request",
                value: error.to_string(),
            })?;
        let input = serde_json::to_vec(&request).map_err(|error| RepositoryError::Decode {
            kind: "media-list enrich request",
            value: error.to_string(),
        })?;
        let runtime = managed_runtime_context();
        let output = invoke_enabled_addon_with_input(
            &self.pool,
            addon_id,
            AddonInvocation {
                capability: CAPABILITY_MEDIA_LIST_IMPORT_V1,
                operation: "enrich",
                runtime: &runtime,
                extra_environment: &BTreeMap::new(),
                timeout: Duration::from_secs(45),
                max_output_bytes: MAX_MEDIA_LIST_IMPORT_ITEM_OUTPUT_BYTES,
            },
            &input,
        )
        .await
        .map_err(|error| {
            RepositoryError::unavailable("invoke media-list enrichment", error.to_string())
        })?;
        conform_media_list_enrich_output(&output, provider_id).map_err(|error| {
            RepositoryError::Decode {
                kind: "media-list enrich output",
                value: error.to_string(),
            }
        })
    }
}

#[derive(Clone, Debug)]
pub(crate) struct InstalledMediaListImportContext {
    pub(crate) source_url: String,
    pub(crate) record_capability: String,
}

fn managed_runtime_context() -> AddonRuntimeContext {
    AddonRuntimeContext {
        ffmpeg_path: Some(ersatzrs_ffmpeg::ffmpeg_path()),
        ffprobe_path: Some(ersatzrs_ffmpeg::ffprobe_path()),
    }
}

fn selected_record_capability(manifest: &ersatzrs_addon_contract::AddonManifest) -> &'static str {
    if manifest.has_capability(CAPABILITY_MEDIA_LIST_LIST_V5) {
        CAPABILITY_MEDIA_LIST_LIST_V5
    } else if manifest.has_capability(CAPABILITY_MEDIA_LIST_LIST_V4) {
        CAPABILITY_MEDIA_LIST_LIST_V4
    } else if manifest.has_capability(CAPABILITY_MEDIA_LIST_LIST_V3) {
        CAPABILITY_MEDIA_LIST_LIST_V3
    } else if manifest.has_capability(CAPABILITY_MEDIA_LIST_LIST_V2) {
        CAPABILITY_MEDIA_LIST_LIST_V2
    } else {
        CAPABILITY_MEDIA_LIST_LIST_V1
    }
}

#[async_trait]
impl AddonMediaListProvider for InstalledAddonMediaListProvider {
    #[allow(clippy::too_many_lines)]
    async fn enumerate(
        &self,
        addon_id: &str,
        source_url: &str,
        chapter_input: &str,
    ) -> Result<EnumeratedAddonMediaList> {
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
            .has_capability(CAPABILITY_MEDIA_LIST_LIST_V5)
        {
            CAPABILITY_MEDIA_LIST_LIST_V5
        } else if installation
            .manifest
            .has_capability(CAPABILITY_MEDIA_LIST_LIST_V4)
        {
            CAPABILITY_MEDIA_LIST_LIST_V4
        } else if installation
            .manifest
            .has_capability(CAPABILITY_MEDIA_LIST_LIST_V3)
        {
            CAPABILITY_MEDIA_LIST_LIST_V3
        } else if installation
            .manifest
            .has_capability(CAPABILITY_MEDIA_LIST_LIST_V2)
        {
            CAPABILITY_MEDIA_LIST_LIST_V2
        } else {
            CAPABILITY_MEDIA_LIST_LIST_V1
        };
        let supports_resumable_import = installation
            .manifest
            .has_capability(CAPABILITY_MEDIA_LIST_IMPORT_V1);
        require_chapter_capability(capability, chapter_input)?;
        let source_url = validate_operator_source_url(
            source_url,
            &installation.manifest.permissions.network_hosts,
        )
        .map_err(|error| {
            RepositoryError::unavailable(
                "validate media-list source URL",
                AddonOperationError::new(OPERATION_CODE_UNSUPPORTED_URL, error.to_string())
                    .to_string(),
            )
        })?;
        let environment = BTreeMap::from([
            (SOURCE_URL_ENVIRONMENT.to_owned(), source_url),
            (
                CHAPTER_INPUT_ENVIRONMENT.to_owned(),
                chapter_input.to_owned(),
            ),
        ]);
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
            RepositoryError::unavailable(
                "invoke media-list add-on",
                legacy_list_invocation_error(&error.to_string(), supports_resumable_import),
            )
        })?;

        let mut list = decode_media_list_output_for_capability(
            &output,
            matches!(
                capability,
                CAPABILITY_MEDIA_LIST_LIST_V3
                    | CAPABILITY_MEDIA_LIST_LIST_V4
                    | CAPABILITY_MEDIA_LIST_LIST_V5
            ),
            matches!(
                capability,
                CAPABILITY_MEDIA_LIST_LIST_V4 | CAPABILITY_MEDIA_LIST_LIST_V5
            ),
            capability == CAPABILITY_MEDIA_LIST_LIST_V5,
        )?;
        if !chapter_input.trim().is_empty() {
            add_chapter_fragments(&mut list, addon_id, chapter_input)?;
        }
        Ok(list)
    }
}

fn legacy_list_invocation_error(message: &str, supports_resumable_import: bool) -> String {
    if !supports_resumable_import && message.contains("entrypoint timed out") {
        return "the legacy one-shot list import timed out; update this add-on to one that supports media-list.import.v1 for resumable overview discovery".to_owned();
    }
    message.to_owned()
}

fn require_chapter_capability(capability: &str, chapter_input: &str) -> Result<()> {
    if !chapter_input.trim().is_empty()
        && !matches!(
            capability,
            CAPABILITY_MEDIA_LIST_LIST_V3
                | CAPABILITY_MEDIA_LIST_LIST_V4
                | CAPABILITY_MEDIA_LIST_LIST_V5
        )
    {
        return Err(RepositoryError::unavailable(
            "validate add-on chapters",
            "chapters require an installed add-on with media-list.list.v3 support",
        ));
    }
    Ok(())
}

fn add_chapter_fragments(
    list: &mut EnumeratedAddonMediaList,
    addon_id: &str,
    input: &str,
) -> Result<()> {
    if list.items.len() != 1 {
        return Err(RepositoryError::unavailable(
            "validate add-on chapters",
            "a chapter source must resolve to exactly one full video",
        ));
    }
    let full = list.items[0].clone();
    let duration = full.duration_seconds;
    let chapters = parse_chapter_input(input, duration).map_err(|error| {
        RepositoryError::unavailable("validate add-on chapters", error.to_string())
    })?;
    let source_url = full.source_url.as_deref().ok_or_else(|| {
        RepositoryError::unavailable(
            "create add-on chapter streams",
            "the source item has no playable URL",
        )
    })?;
    for chapter in chapters {
        let rank = i32::try_from(list.items.len()).map_err(|_| {
            RepositoryError::unavailable("create add-on chapter streams", "too many chapters")
        })?;
        let start = chapter.start_seconds;
        let end = chapter.end_seconds;
        let fragment_url =
            fragment_stream_url(source_url, chapter.start_seconds, chapter.end_seconds).map_err(
                |error| {
                    RepositoryError::unavailable("create add-on chapter streams", error.to_string())
                },
            )?;
        list.items.push(EnumeratedAddonMediaListItem {
            provider_id: format!("{}:chapter:{}", full.provider_id, chapter.start_seconds),
            rank,
            display_title: format!("{} - {}", full.display_title, chapter.title),
            title: chapter.title,
            year: full.year,
            season: full.season,
            episode: full.episode,
            kind: full.kind,
            guids: vec![fragment_guid(
                addon_id,
                &full.provider_id,
                chapter.start_seconds,
            )],
            source_url: Some(fragment_url),
            availability: full.availability,
            availability_reason: full.availability_reason.clone(),
            content_kind: full.content_kind,
            collection_role: full.collection_role,
            duration_seconds: end.map(|end| end - start),
            liveness: full.liveness,
            parent_provider_id: Some(full.provider_id.clone()),
            fragment_start_seconds: Some(start),
            fragment_end_seconds: end,
            additional_image_urls: full.additional_image_urls.clone(),
            metadata: full.metadata.clone(),
        });
    }
    Ok(())
}

#[cfg(test)]
fn decode_media_list_output(output: &[u8]) -> Result<EnumeratedAddonMediaList> {
    decode_media_list_output_for_capability(output, true, true, true)
}

#[allow(clippy::too_many_lines)]
pub(crate) fn decode_media_list_output_for_capability(
    output: &[u8],
    supports_v3_fields: bool,
    supports_v4_metadata: bool,
    supports_v5_liveness: bool,
) -> Result<EnumeratedAddonMediaList> {
    let mut records = conform_media_list_output(output)
        .map_err(|error| RepositoryError::Decode {
            kind: "media-list add-on output",
            value: error.to_string(),
        })?
        .into_iter();
    let header = records.next().ok_or_else(|| RepositoryError::Decode {
        kind: "media-list add-on output",
        value: "missing list record".to_owned(),
    })?;
    let AddonMediaListRecord::List {
        provider_id,
        name,
        description,
        metadata,
    } = header
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

    if metadata.is_some() && !supports_v4_metadata {
        return Err(RepositoryError::Decode {
            kind: "media-list add-on output",
            value: "rich metadata requires media-list.list.v4".to_owned(),
        });
    }
    let list_metadata = metadata_document(
        metadata,
        Some(name.as_str()),
        description.as_deref(),
        &[],
        &[],
    );
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
            collection_role,
            duration_seconds,
            liveness,
            parent_provider_id,
            fragment_start_seconds,
            fragment_end_seconds,
            additional_image_urls,
            metadata,
        } = record
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
        if !supports_v3_fields
            && (duration_seconds.is_some()
                || parent_provider_id.is_some()
                || fragment_start_seconds.is_some()
                || fragment_end_seconds.is_some()
                || !additional_image_urls.is_empty())
        {
            return Err(RepositoryError::Decode {
                kind: "media-list add-on output",
                value: "duration, image, and chapter-fragment fields require media-list.list.v3"
                    .to_owned(),
            });
        }
        if metadata.is_some() && !supports_v4_metadata {
            return Err(RepositoryError::Decode {
                kind: "media-list add-on output",
                value: "rich metadata requires media-list.list.v4".to_owned(),
            });
        }
        if liveness.is_some() && !supports_v5_liveness {
            return Err(RepositoryError::Decode {
                kind: "media-list add-on output",
                value: "liveness requires media-list.list.v5".to_owned(),
            });
        }
        if !supports_v4_metadata
            && (matches!(content_kind, WireContentKind::Song | WireContentKind::Image)
                || collection_role != WireItemRole::Auto)
        {
            return Err(RepositoryError::Decode {
                kind: "media-list add-on output",
                value: "song/image classification and collection roles require media-list.list.v4"
                    .to_owned(),
            });
        }
        if !provider_ids.insert(provider_id.clone()) || !ranks.insert(rank) {
            return Err(RepositoryError::Decode {
                kind: "media-list add-on output",
                value: "item provider_id and rank must be unique".to_owned(),
            });
        }
        let item_metadata = metadata_document(
            metadata,
            Some(title.as_str()),
            None,
            &guids,
            &additional_image_urls,
        );
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
            collection_role: domain_item_role(collection_role),
            duration_seconds,
            liveness: if supports_v5_liveness {
                domain_liveness(liveness.unwrap_or(WireLiveness::Unknown))
            } else {
                ersatzrs_domain::RemoteStreamLiveness::from_legacy(None, duration_seconds)
            },
            parent_provider_id,
            fragment_start_seconds,
            fragment_end_seconds,
            additional_image_urls,
            metadata: item_metadata,
        });
    }
    items.sort_by_key(|item| item.rank);

    Ok(EnumeratedAddonMediaList {
        provider_id,
        name,
        description,
        metadata: list_metadata,
        items,
    })
}

fn metadata_document(
    document: Option<AddonMetadataDocument>,
    fallback_title: Option<&str>,
    fallback_plot: Option<&str>,
    fallback_guids: &[String],
    fallback_artwork: &[String],
) -> AddonMediaListNfoMetadata {
    let document = document.unwrap_or_default();
    let artwork = document
        .artwork
        .into_iter()
        .map(|candidate| AddonMediaListArtwork {
            url: candidate.url,
            role: match candidate.role {
                WireArtworkRole::Poster => AddonMediaListArtworkRole::Poster,
                WireArtworkRole::Fanart => AddonMediaListArtworkRole::Fanart,
                WireArtworkRole::Thumb => AddonMediaListArtworkRole::Thumb,
                WireArtworkRole::Other => AddonMediaListArtworkRole::Other,
            },
            width: candidate.width,
            height: candidate.height,
            language: candidate.language,
        })
        .collect::<Vec<_>>();
    let artwork_urls = if artwork.is_empty() {
        fallback_artwork.to_vec()
    } else {
        Vec::new()
    };
    let guids = if document.guids.is_empty() {
        fallback_guids.to_vec()
    } else {
        document.guids
    };
    AddonMediaListNfoMetadata {
        title: document
            .title
            .or_else(|| fallback_title.map(ToOwned::to_owned))
            .unwrap_or_default(),
        original_title: document.original_title,
        sort_title: document.sort_title,
        outline: document.outline,
        plot: document
            .plot
            .or_else(|| fallback_plot.map(ToOwned::to_owned)),
        tagline: document.tagline,
        show_title: document.show_title,
        season: document.season,
        episode: document.episode,
        year: document.year,
        release_date: document.release_date,
        content_ratings: document.content_ratings,
        genres: document.genres,
        tags: document.tags,
        studios: document.studios,
        music_labels: document.music_labels,
        languages: document.languages,
        people: document
            .people
            .into_iter()
            .map(|person| AddonMediaListNfoPerson {
                name: person.name,
                role: person.role,
                order: person.order,
                thumb: person.thumb,
            })
            .collect(),
        writers: document.writers,
        directors: document.directors,
        artists: document.artists,
        producers: document.producers,
        original_broadcasters: document.original_broadcasters,
        broadcasters: document.broadcasters,
        collection: document.collection,
        artwork,
        artwork_urls,
        guids,
    }
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
        WireContentKind::Song => ersatzrs_domain::AddonMediaListContentKind::Song,
        WireContentKind::Image => ersatzrs_domain::AddonMediaListContentKind::Image,
    }
}

const fn domain_item_role(kind: WireItemRole) -> ersatzrs_domain::AddonMediaListItemRole {
    match kind {
        WireItemRole::Auto => ersatzrs_domain::AddonMediaListItemRole::Auto,
        WireItemRole::Primary => ersatzrs_domain::AddonMediaListItemRole::Primary,
        WireItemRole::Extra => ersatzrs_domain::AddonMediaListItemRole::Extra,
    }
}

const fn domain_liveness(kind: WireLiveness) -> ersatzrs_domain::RemoteStreamLiveness {
    match kind {
        WireLiveness::Unknown => ersatzrs_domain::RemoteStreamLiveness::Unknown,
        WireLiveness::Finite => ersatzrs_domain::RemoteStreamLiveness::Finite,
        WireLiveness::Live => ersatzrs_domain::RemoteStreamLiveness::Live,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        add_chapter_fragments, decode_media_list_output, decode_media_list_output_for_capability,
        require_chapter_capability,
    };
    use ersatzrs_addon_contract::{CAPABILITY_MEDIA_LIST_LIST_V2, CAPABILITY_MEDIA_LIST_LIST_V3};

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
    const CANONICAL_URL_FIXTURE_SCRIPT: &str = r#"#!/bin/sh
set -eu
test "$ERSATZRS_MEDIA_LIST_URL" = 'https://allowed.example.test/@evil.example/list'
printf '%s\n' '{"record_type":"list","provider_id":"fixture/list","name":"Fixture"}'
printf '%s\n' '{"record_type":"item","provider_id":"movie:1","rank":0,"display_title":"Arrival","title":"Arrival","kind":"movie","guids":[]}'
"#;

    #[cfg(unix)]
    async fn fixture_media_list_addon_with_script(
        root: &std::path::Path,
        source: &str,
        network_hosts: &[&str],
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
                "network_hosts": network_hosts,
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
        fixture_media_list_addon_with_script(root, FIXTURE_SCRIPT, &[]).await
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn list_invocations_supply_the_managed_ffmpeg_runtime() {
        use ersatzrs_core::addon_media_list::AddonMediaListProvider as _;

        let temporary = tempfile::tempdir().expect("temporary folder");
        let root = temporary.path().join("package");
        let pool = fixture_media_list_addon(&root).await;

        let list = super::InstalledAddonMediaListProvider::new(pool)
            .enumerate(
                "org.ersatzrs.addon.fixture",
                "https://example.test/list",
                "",
            )
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
            fixture_media_list_addon_with_script(&root, INVALID_AVAILABILITY_FIXTURE_SCRIPT, &[])
                .await;

        let error = super::InstalledAddonMediaListProvider::new(pool)
            .enumerate(
                "org.ersatzrs.addon.fixture",
                "https://example.test/list",
                "",
            )
            .await
            .expect_err("unknown availability must reject the enumeration");
        let message = error.to_string();
        assert!(message.contains("is not a valid record"));
        assert!(!message.contains("maybe_later"));
        assert!(message.len() < 512);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn source_url_must_match_the_installed_addon_network_hosts() {
        use ersatzrs_core::addon_media_list::AddonMediaListProvider as _;

        let temporary = tempfile::tempdir().expect("temporary folder");
        let root = temporary.path().join("package");
        let pool =
            fixture_media_list_addon_with_script(&root, FIXTURE_SCRIPT, &["allowed.example.test"])
                .await;

        let error = super::InstalledAddonMediaListProvider::new(pool)
            .enumerate(
                "org.ersatzrs.addon.fixture",
                "https://example.test/list",
                "",
            )
            .await
            .expect_err("undeclared host");
        let message = error.to_string();
        assert!(message.contains("unsupported-url"));
        assert!(message.contains("example.test"));
        assert!(!message.contains("/list"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn provider_receives_the_same_canonical_url_used_for_host_validation() {
        use ersatzrs_core::addon_media_list::AddonMediaListProvider as _;

        let temporary = tempfile::tempdir().expect("temporary folder");
        let root = temporary.path().join("package");
        let pool = fixture_media_list_addon_with_script(
            &root,
            CANONICAL_URL_FIXTURE_SCRIPT,
            &["allowed.example.test"],
        )
        .await;

        super::InstalledAddonMediaListProvider::new(pool)
            .enumerate(
                "org.ersatzrs.addon.fixture",
                r"https://allowed.example.test\@evil.example/list",
                "",
            )
            .await
            .expect("provider receives canonical source URL");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn existing_list_remains_visible_when_new_host_validation_fails() {
        use ersatzrs_core::repositories::AddonMediaListRepository as _;

        let temporary = tempfile::tempdir().expect("temporary folder");
        let root = temporary.path().join("package");
        let pool = fixture_media_list_addon_with_script(&root, FIXTURE_SCRIPT, &["*"]).await;
        let repository = crate::repositories::SqliteAddonMediaListRepository(pool.clone());
        let source_url = "https://example.test/list";
        let list_id = repository
            .add_or_refresh(
                "org.ersatzrs.addon.fixture",
                source_url,
                None,
                ersatzrs_domain::AddonMediaListContentKind::Auto,
                "",
            )
            .await
            .expect("initial import");

        let manifest_json: String = sqlx::query_scalar(
            r#"SELECT "ManifestJson" FROM "AddonInstallation" WHERE "AddonId" = ?"#,
        )
        .bind("org.ersatzrs.addon.fixture")
        .fetch_one(&pool)
        .await
        .expect("installed manifest");
        let mut manifest: serde_json::Value =
            serde_json::from_str(&manifest_json).expect("manifest JSON");
        manifest["permissions"]["network_hosts"] = serde_json::json!(["allowed.example.test"]);
        sqlx::query(r#"UPDATE "AddonInstallation" SET "ManifestJson" = ? WHERE "AddonId" = ?"#)
            .bind(manifest.to_string())
            .bind("org.ersatzrs.addon.fixture")
            .execute(&pool)
            .await
            .expect("restrict network hosts");

        repository
            .add_or_refresh(
                "org.ersatzrs.addon.fixture",
                source_url,
                None,
                ersatzrs_domain::AddonMediaListContentKind::Auto,
                "",
            )
            .await
            .expect_err("new validation must reject saved URL");
        let (rows, total) = repository.list_admin(0, 10).await.expect("admin rows");
        assert_eq!(total, 1);
        assert_eq!(rows[0].id, list_id);
        let last_error = rows[0].last_error.as_deref().expect("saved failure");
        assert!(last_error.contains("unsupported-url"));
        assert!(last_error.contains("example.test"));
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
    fn legacy_timeout_explains_the_resumable_capability_upgrade() {
        let message =
            super::legacy_list_invocation_error("[process-failed] entrypoint timed out", false);
        assert!(message.contains("media-list.import.v1"));
        assert!(message.contains("resumable overview discovery"));
        assert_eq!(
            super::legacy_list_invocation_error("provider rejected input", false),
            "provider rejected input"
        );
    }

    #[test]
    fn chapter_fragments_keep_the_full_episode_and_calculate_stream_bounds() {
        let output = br#"{"record_type":"list","provider_id":"episode-list","name":"Episode"}
{"record_type":"item","provider_id":"episode:42","rank":0,"display_title":"Programme - Episode","title":"Episode","kind":"remote_stream","guids":["example://42"],"source_url":"https://media.example.test/episodes/42","duration_seconds":600,"additional_image_urls":["https://media.example.test/image.jpg"]}
"#;
        let mut list = decode_media_list_output(output).expect("valid v3 output");
        add_chapter_fragments(
            &mut list,
            "org.example.video",
            "0:00 Intro\n1:35 Topic\n4:12 Finish",
        )
        .expect("valid chapters");

        assert_eq!(list.items.len(), 4);
        assert_eq!(list.items[0].provider_id, "episode:42");
        assert!(!list.items[0]
            .source_url
            .as_deref()
            .unwrap_or_default()
            .contains("start="));
        assert_eq!(
            list.items[1].parent_provider_id.as_deref(),
            Some("episode:42")
        );
        assert_eq!(list.items[1].fragment_start_seconds, Some(0));
        assert_eq!(list.items[1].fragment_end_seconds, Some(95));
        assert_eq!(list.items[1].duration_seconds, Some(95));
        assert!(list.items[1]
            .source_url
            .as_deref()
            .is_some_and(|url| url.ends_with("?start=0&end=95")));
        assert_eq!(list.items[3].fragment_end_seconds, Some(600));
        assert!(list.items[3]
            .source_url
            .as_deref()
            .is_some_and(|url| url.ends_with("?start=252&end=600")));
        assert_eq!(
            list.items[3].additional_image_urls,
            list.items[0].additional_image_urls
        );
    }

    #[test]
    fn final_chapter_can_remain_open_when_duration_is_unknown() {
        let output = br#"{"record_type":"list","provider_id":"episode-list","name":"Episode"}
{"record_type":"item","provider_id":"episode:42","rank":0,"display_title":"Episode","title":"Episode","kind":"remote_stream","guids":[],"source_url":"https://media.example.test/episodes/42"}
"#;
        let mut list = decode_media_list_output(output).expect("valid v3 output");
        add_chapter_fragments(&mut list, "org.example.video", "0:00 Intro\n1:00 Remainder")
            .expect("valid chapters");
        assert_eq!(list.items[2].fragment_end_seconds, None);
        assert!(list.items[2]
            .source_url
            .as_deref()
            .is_some_and(|url| url.ends_with("?start=60")));
    }

    #[test]
    fn v3_item_fields_are_rejected_for_frozen_older_capabilities() {
        let output = br#"{"record_type":"list","provider_id":"x","name":"X"}
{"record_type":"item","provider_id":"one","rank":0,"display_title":"One","title":"One","kind":"remote_stream","source_url":"https://example.test/one","duration_seconds":60}
"#;
        let error = decode_media_list_output_for_capability(output, false, false, false)
            .expect_err("v2 must not accept v3 fields");
        assert!(error.to_string().contains("media-list.list.v3"));
    }

    #[test]
    fn chapters_require_media_list_v3_before_invocation() {
        let error = require_chapter_capability(CAPABILITY_MEDIA_LIST_LIST_V2, "0:00 Intro")
            .expect_err("v2 must not derive chapter fragments");
        assert!(error.to_string().contains("media-list.list.v3"));

        require_chapter_capability(CAPABILITY_MEDIA_LIST_LIST_V3, "0:00 Intro")
            .expect("v3 supports chapter fragments");
        require_chapter_capability(CAPABILITY_MEDIA_LIST_LIST_V2, "   ")
            .expect("blank chapter input remains compatible with v2");
    }

    #[test]
    fn rejects_and_names_unknown_media_list_fields() {
        let output = br#"{"record_type":"list","provider_id":"x","name":"X"}
{"record_type":"item","provider_id":"one","rank":0,"display_title":"One","title":"One","kind":"movie","geners":[]}
"#;
        let error = decode_media_list_output(output).expect_err("unknown field must reject");
        let message = error.to_string();
        assert!(message.contains("unknown field `geners`"));
        assert!(message.len() < 512);
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
