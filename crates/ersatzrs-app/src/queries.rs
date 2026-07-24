//! Application query requests and read models.

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, NaiveTime, Utc};
use ersatzrs_core::scheduling::{
    BlockDefinition, BlockTemplate, BlockTemplateItem, BlockTemplateSchedule, EffectiveBlock,
    EffectiveBlockItem, PlayoutBuildMode, SchedulingCollectionKey, SchedulingRequest,
    SchedulingSmartShufflePolicy,
};
use ersatzrs_domain::{
    BlockStopScheduling, CollectionType, EntityId, PlaybackOrder, Playout, PlayoutScheduleKind,
};

use crate::commands::{ScanOperationId, ScanRequest};
use crate::operations::OperationKey;
use crate::ports::BlockPreviewBuilder;
use crate::{ApplicationError, Result};

/// One unsaved block item adapted from a transport draft.
#[derive(Clone, Debug, PartialEq)]
pub struct PreviewBlockItem {
    /// Draft order shown in the editor.
    pub index: i32,
    /// Real scheduler collection/reference identity.
    pub collection_key: SchedulingCollectionKey,
    /// Real scheduler playback order.
    pub playback_order: PlaybackOrder,
    /// Whether generated rows remain normal guide content.
    pub include_in_program_guide: bool,
    /// Whether generated rows suppress channel/global watermarks.
    pub disable_watermarks: bool,
    /// Watermarks attached to this unsaved item.
    pub watermark_ids: Vec<EntityId>,
    /// Graphics elements attached to this unsaved item.
    pub graphics_element_ids: Vec<EntityId>,
    /// Unsaved Smart Shuffle attachment for this item.
    pub smart_shuffle_policy: SchedulingSmartShufflePolicy,
}

/// Query the real block scheduler for an unsaved editor draft.
#[derive(Clone, Debug, PartialEq)]
pub struct PreviewBlockPlayout {
    /// Unsaved block name.
    pub name: String,
    /// Unsaved block duration in minutes.
    pub minutes: i32,
    /// Unsaved block stop behavior.
    pub stop_scheduling: BlockStopScheduling,
    /// Unsaved ordered block items.
    pub items: Vec<PreviewBlockItem>,
}

/// Enriched row returned from the real block builder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreviewBlockPlayoutRow {
    /// Real media identity selected by the scheduler.
    pub media_item_id: EntityId,
    /// Actual generated start instant.
    pub start: DateTime<Utc>,
    /// Actual generated finish instant.
    pub finish: DateTime<Utc>,
    /// Title hydrated from real media/reference data.
    pub title: String,
    /// Actual generated row duration.
    pub duration: Duration,
}

/// Execute an unsaved block preview through the real scheduling builder.
///
/// This mirrors
/// `$ERSATZTV_SRC/ErsatzTV.Application/Scheduling/Commands/PreviewBlockPlayoutHandler.cs`:
/// construct temporary template/playout/reference state, build in Reset mode,
/// then enrich the builder's actual added rows.
///
/// # Errors
///
/// Returns a typed validation, repository, scheduling, or enrichment failure.
pub async fn preview_block_playout(
    query: PreviewBlockPlayout,
    repository: &dyn ersatzrs_core::scheduling::BlockPreviewRepository,
    builder: &dyn BlockPreviewBuilder,
) -> Result<Vec<PreviewBlockPlayoutRow>> {
    preview_block_playout_at(query, repository, builder, Utc::now()).await
}

async fn preview_block_playout_at(
    query: PreviewBlockPlayout,
    repository: &dyn ersatzrs_core::scheduling::BlockPreviewRepository,
    builder: &dyn BlockPreviewBuilder,
    start: DateTime<Utc>,
) -> Result<Vec<PreviewBlockPlayoutRow>> {
    validate_preview_query(&query)?;

    let (playout, effective_blocks, policies) = temporary_block_state(&query, start);
    let temporary_items = effective_blocks
        .iter()
        .flat_map(|block| block.items.iter().cloned())
        .collect::<Vec<_>>();
    let mut reference_data = repository
        .load_block_preview_content(&temporary_items, &policies)
        .await?;
    reference_data.effective_blocks = effective_blocks;

    let media_items = reference_data.media_items_by_id.clone();
    let result = builder
        .build(SchedulingRequest {
            playout,
            start,
            finish: start + Duration::days(1),
            mode: PlayoutBuildMode::Reset,
            reference_data,
        })
        .map_err(|error| ApplicationError::Scheduling {
            operation: "block preview",
            message: error.to_string(),
        })?;

    result
        .added_items
        .into_iter()
        .map(|item| {
            let media =
                media_items
                    .get(&item.media_item_id)
                    .ok_or(ApplicationError::MissingReference {
                        kind: "block preview media item",
                        id: item.media_item_id,
                    })?;
            Ok(PreviewBlockPlayoutRow {
                media_item_id: item.media_item_id,
                start: item.start,
                finish: item.finish,
                title: media.title.clone().unwrap_or_default(),
                duration: item.finish - item.start,
            })
        })
        .collect()
}

fn validate_preview_query(query: &PreviewBlockPlayout) -> Result<()> {
    if query.minutes <= 0 {
        return Err(ApplicationError::Validation {
            field: "minutes",
            message: "must be greater than zero".to_owned(),
        });
    }

    for item in &query.items {
        validate_collection_key(&item.collection_key)?;
    }
    Ok(())
}

fn validate_collection_key(key: &SchedulingCollectionKey) -> Result<()> {
    let (field, valid) = match key.collection_type {
        CollectionType::Collection => ("collection_id", key.collection_id.is_some()),
        CollectionType::TelevisionShow
        | CollectionType::TelevisionSeason
        | CollectionType::Artist
        | CollectionType::Movie
        | CollectionType::Episode
        | CollectionType::MusicVideo
        | CollectionType::OtherVideo
        | CollectionType::Song
        | CollectionType::Image
        | CollectionType::RemoteStream => ("media_item_id", key.media_item_id.is_some()),
        CollectionType::MultiCollection => {
            ("multi_collection_id", key.multi_collection_id.is_some())
        }
        CollectionType::SmartCollection => {
            ("smart_collection_id", key.smart_collection_id.is_some())
        }
        CollectionType::Playlist => ("playlist_id", key.playlist_id.is_some()),
        CollectionType::SearchQuery => (
            "search_query",
            key.search_query
                .as_deref()
                .is_some_and(|query| !query.trim().is_empty()),
        ),
        CollectionType::RerunFirstRun
        | CollectionType::RerunRerun
        | CollectionType::FakeCollection
        | CollectionType::FakePlaylistItem => {
            return Err(ApplicationError::Validation {
                field: "collection_type",
                message: format!(
                    "{:?} is not supported by the real block scheduler",
                    key.collection_type
                ),
            });
        }
    };

    if valid {
        Ok(())
    } else {
        Err(ApplicationError::Validation {
            field,
            message: "is required for this collection type".to_owned(),
        })
    }
}

fn temporary_block_state(
    query: &PreviewBlockPlayout,
    start: DateTime<Utc>,
) -> (
    Playout,
    Vec<EffectiveBlock>,
    BTreeMap<String, SchedulingSmartShufflePolicy>,
) {
    const TEMPORARY_ID: EntityId = 1;

    let mut draft_items = query.items.clone();
    draft_items.sort_by_key(|item| item.index);
    let mut policies = BTreeMap::new();
    let items = draft_items
        .into_iter()
        .enumerate()
        .map(|(index, item)| {
            let id = i64::try_from(index).unwrap_or_default() + 1;
            policies.insert(id.to_string(), item.smart_shuffle_policy);
            EffectiveBlockItem {
                id,
                block_id: TEMPORARY_ID,
                content: format!("block-preview-item:{id}"),
                collection_key: item.collection_key,
                playback_order: item.playback_order,
                include_in_program_guide: item.include_in_program_guide,
                disable_watermarks: item.disable_watermarks,
                watermark_ids: item.watermark_ids,
                graphics_element_ids: item.graphics_element_ids,
            }
        })
        .collect::<Vec<_>>();

    let block = BlockDefinition {
        id: TEMPORARY_ID,
        updated_ticks: 0,
        name: query.name.clone(),
        duration: Duration::minutes(i64::from(query.minutes)),
        stop_scheduling: query.stop_scheduling,
        items,
    };
    let template = BlockTemplate {
        id: TEMPORARY_ID,
        updated_ticks: 0,
        items: vec![BlockTemplateItem {
            id: TEMPORARY_ID,
            block_id: TEMPORARY_ID,
            start_time: Some(NaiveTime::MIN),
            sort_order: 0,
        }],
    };
    let schedule = BlockTemplateSchedule {
        id: TEMPORARY_ID,
        updated_ticks: 0,
        start: None,
        finish: None,
        index: 0,
        days_of_week: (0..=6).collect(),
        days_of_month: (1..=31).collect(),
        months_of_year: (1..=12).collect(),
        limit_to_date_range: false,
        start_month: 1,
        start_day: 1,
        start_year: None,
        end_month: 12,
        end_day: 31,
        end_year: None,
        template,
        blocks: BTreeMap::from([(TEMPORARY_ID, block)]),
        deco: None,
        deco_template_items: Vec::new(),
    };
    let effective_blocks = EffectiveBlock::from_templates(&[schedule], start, 1);
    let playout = Playout {
        id: TEMPORARY_ID,
        schedule_kind: PlayoutScheduleKind::Block,
        ..Playout::default()
    };

    (playout, effective_blocks, policies)
}

/// Request the current state of one long-running operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GetOperationStatus {
    /// Stable operation identity to inspect.
    pub key: OperationKey,
}

impl GetOperationStatus {
    /// Create an operation-status query.
    #[must_use]
    pub const fn new(key: OperationKey) -> Self {
        Self { key }
    }
}

/// Application-level state returned for an operation query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperationStatus {
    /// No worker currently owns the operation lock.
    Idle,
    /// A caller or worker currently owns the operation lock.
    Running,
}

/// Lifecycle state for one accepted scan operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScanOperationState {
    /// Accepted work is waiting in the FIFO queue.
    Queued,
    /// The sole scanner worker is executing the request.
    Running,
    /// The request completed successfully and the library is idle.
    Completed,
    /// The request failed; the diagnostic remains observable after cleanup.
    Failed {
        /// Storage-neutral worker or adapter diagnostic.
        message: String,
    },
}

/// Application read model for one scan operation.
#[derive(Clone, Debug, PartialEq)]
pub struct ScanOperationSnapshot {
    /// Stable application operation identity.
    pub operation_id: ScanOperationId,
    /// Validated request accepted for execution.
    pub request: ScanRequest,
    /// Current or terminal lifecycle state.
    pub state: ScanOperationState,
    /// Active fractional progress in `[0.0, 1.0]`.
    ///
    /// Terminal operations never retain progress.
    pub progress: Option<f32>,
}

/// Aggregate library status used by polling adapters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LibraryScanStatus {
    /// No operation has been observed for this library.
    Idle,
    /// At least one accepted request is waiting.
    Queued,
    /// A request currently owns the shared library exclusion boundary.
    Running,
    /// The latest request completed and the library is idle.
    Completed,
    /// The latest request failed and the library is idle.
    Failed {
        /// Storage-neutral worker or adapter diagnostic.
        message: String,
    },
}

/// Query one library's aggregate scan state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GetLibraryScanStatus {
    /// Persisted library identity.
    pub library_id: EntityId,
}

impl GetLibraryScanStatus {
    /// Create a library scan status query.
    #[must_use]
    pub const fn new(library_id: EntityId) -> Self {
        Self { library_id }
    }
}

#[cfg(test)]
mod block_preview_tests {
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use chrono::{Duration, TimeZone};
    use ersatzrs_core::playout_builder::PlayoutBuilder;
    use ersatzrs_core::repositories::{RepositoryError, Result as RepositoryResult};
    use ersatzrs_core::scheduling::{
        BlockPreviewRepository, EffectiveBlockItem, SchedulingError, SchedulingMediaItem,
        SchedulingReferenceData, SchedulingResult,
    };
    use ersatzrs_domain::{BlockStopScheduling, CollectionType, PlaybackOrder, PlayoutItem};

    use super::*;

    #[derive(Default)]
    struct HydratingRepository {
        calls: Mutex<Vec<Vec<EffectiveBlockItem>>>,
        empty: bool,
        fail: bool,
    }

    #[async_trait]
    impl BlockPreviewRepository for HydratingRepository {
        async fn load_block_preview_content(
            &self,
            items: &[EffectiveBlockItem],
            _policies: &BTreeMap<String, SchedulingSmartShufflePolicy>,
        ) -> RepositoryResult<SchedulingReferenceData> {
            self.calls
                .lock()
                .expect("preview calls lock")
                .push(items.to_vec());
            if self.fail {
                return Err(RepositoryError::unavailable(
                    "load block preview content",
                    "database closed",
                ));
            }

            let mut reference_data = SchedulingReferenceData::default();
            if self.empty {
                return Ok(reference_data);
            }
            for item in items {
                let media_id = 1_000 + item.id;
                let media = SchedulingMediaItem {
                    title: Some(format!("Real media {media_id}")),
                    ..SchedulingMediaItem::new(media_id, Duration::minutes(1))
                };
                reference_data
                    .media_items_by_id
                    .insert(media_id, media.clone());
                reference_data
                    .media_by_key
                    .insert(item.content.clone(), vec![media]);
            }
            Ok(reference_data)
        }
    }

    #[derive(Default)]
    struct CapturingBuilder {
        requests: Mutex<Vec<SchedulingRequest>>,
        output_media_ids: Vec<EntityId>,
        fail: bool,
    }

    impl BlockPreviewBuilder for CapturingBuilder {
        fn build(
            &self,
            request: SchedulingRequest,
        ) -> std::result::Result<SchedulingResult, SchedulingError> {
            self.requests
                .lock()
                .expect("builder requests lock")
                .push(request.clone());
            if self.fail {
                return Err(SchedulingError::InvalidDuration(
                    "builder rejected draft".to_owned(),
                ));
            }
            let start = request.start;
            Ok(SchedulingResult {
                added_items: self
                    .output_media_ids
                    .iter()
                    .enumerate()
                    .map(|(index, media_item_id)| {
                        let row_start =
                            start + Duration::minutes(i64::try_from(index).unwrap_or(0));
                        PlayoutItem {
                            playout_id: request.playout.id,
                            media_item_id: *media_item_id,
                            start: row_start,
                            finish: row_start + Duration::minutes(1),
                            ..PlayoutItem::default()
                        }
                    })
                    .collect(),
                ..SchedulingResult::default()
            })
        }
    }

    fn preview_item(index: i32, collection_key: SchedulingCollectionKey) -> PreviewBlockItem {
        PreviewBlockItem {
            index,
            collection_key,
            playback_order: PlaybackOrder::Chronological,
            include_in_program_guide: true,
            disable_watermarks: false,
            watermark_ids: Vec::new(),
            graphics_element_ids: Vec::new(),
            smart_shuffle_policy: SchedulingSmartShufflePolicy::default(),
        }
    }

    fn query(items: Vec<PreviewBlockItem>) -> PreviewBlockPlayout {
        PreviewBlockPlayout {
            name: "Unsaved preview".to_owned(),
            minutes: 60,
            stop_scheduling: BlockStopScheduling::AfterDurationEnd,
            items,
        }
    }

    async fn run_preview(
        query: PreviewBlockPlayout,
        repository: &dyn BlockPreviewRepository,
        builder: &dyn BlockPreviewBuilder,
    ) -> Result<Vec<PreviewBlockPlayoutRow>> {
        preview_block_playout_at(
            query,
            repository,
            builder,
            Utc.with_ymd_and_hms(2026, 7, 24, 12, 0, 0)
                .single()
                .expect("fixed preview time"),
        )
        .await
    }

    fn collection_key(kind: CollectionType, id: EntityId) -> SchedulingCollectionKey {
        let mut key = SchedulingCollectionKey {
            collection_type: kind,
            ..SchedulingCollectionKey::default()
        };
        match kind {
            CollectionType::Collection => key.collection_id = Some(id),
            CollectionType::MultiCollection => key.multi_collection_id = Some(id),
            CollectionType::SmartCollection => key.smart_collection_id = Some(id),
            CollectionType::Playlist => key.playlist_id = Some(id),
            CollectionType::SearchQuery => key.search_query = Some(format!("title:{id}")),
            _ => key.media_item_id = Some(id),
        }
        key
    }

    #[tokio::test]
    async fn unsaved_draft_builds_temporary_state_and_uses_reset() {
        let repository = HydratingRepository::default();
        let builder = CapturingBuilder {
            output_media_ids: vec![1_001, 1_002],
            ..CapturingBuilder::default()
        };
        let items = vec![
            preview_item(20, collection_key(CollectionType::TelevisionShow, 8)),
            PreviewBlockItem {
                index: 10,
                collection_key: collection_key(CollectionType::Collection, 7),
                playback_order: PlaybackOrder::Shuffle,
                include_in_program_guide: false,
                disable_watermarks: true,
                watermark_ids: vec![3],
                graphics_element_ids: vec![4],
                smart_shuffle_policy: SchedulingSmartShufflePolicy::default(),
            },
        ];

        let rows = run_preview(query(items), &repository, &builder)
            .await
            .expect("preview succeeds");

        let requests = builder.requests.lock().expect("builder requests lock");
        let request = requests.first().expect("one builder request");
        assert_eq!(request.mode, PlayoutBuildMode::Reset);
        assert_eq!(
            request.start,
            Utc.with_ymd_and_hms(2026, 7, 24, 12, 0, 0)
                .single()
                .expect("fixed preview request time")
        );
        assert_eq!(request.playout.schedule_kind, PlayoutScheduleKind::Block);
        assert_eq!(request.reference_data.effective_blocks.len(), 1);
        let block = &request.reference_data.effective_blocks[0];
        assert_eq!(block.name, "Unsaved preview");
        assert_eq!(block.duration, Duration::minutes(60));
        assert_eq!(block.stop_scheduling, BlockStopScheduling::AfterDurationEnd);
        assert_eq!(
            block
                .items
                .iter()
                .map(|item| item.collection_key.collection_type)
                .collect::<Vec<_>>(),
            vec![CollectionType::Collection, CollectionType::TelevisionShow]
        );
        assert!(!block.items[0].include_in_program_guide);
        assert!(block.items[0].disable_watermarks);
        assert_eq!(block.items[0].watermark_ids, vec![3]);
        assert_eq!(block.items[0].graphics_element_ids, vec![4]);
        assert_eq!(
            rows.iter().map(|row| row.media_item_id).collect::<Vec<_>>(),
            vec![1_001, 1_002]
        );
        assert_eq!(rows[0].title, "Real media 1001");
        assert_eq!(rows[0].duration, Duration::minutes(1));
    }

    #[tokio::test]
    async fn every_real_block_collection_kind_uses_the_real_builder_path() {
        let kinds = [
            CollectionType::Collection,
            CollectionType::TelevisionShow,
            CollectionType::TelevisionSeason,
            CollectionType::Artist,
            CollectionType::MultiCollection,
            CollectionType::SmartCollection,
            CollectionType::Playlist,
            CollectionType::SearchQuery,
            CollectionType::Movie,
            CollectionType::Episode,
            CollectionType::MusicVideo,
            CollectionType::OtherVideo,
            CollectionType::Song,
            CollectionType::Image,
            CollectionType::RemoteStream,
        ];
        let items = kinds
            .iter()
            .enumerate()
            .map(|(index, kind)| {
                preview_item(
                    i32::try_from(index).unwrap_or_default(),
                    collection_key(*kind, i64::try_from(index).unwrap_or_default() + 1),
                )
            })
            .collect();
        let repository = HydratingRepository::default();

        let rows = run_preview(query(items), &repository, &PlayoutBuilder::default())
            .await
            .expect("real builder preview succeeds");

        let calls = repository.calls.lock().expect("preview calls lock");
        assert_eq!(
            calls[0]
                .iter()
                .map(|item| item.collection_key.collection_type)
                .collect::<Vec<_>>(),
            kinds
        );
        assert_eq!(rows.len(), kinds.len());
        assert_eq!(
            rows.iter().map(|row| row.media_item_id).collect::<Vec<_>>(),
            (1_001..=1_015).collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn valid_empty_output_is_not_a_failure() {
        let repository = HydratingRepository {
            empty: true,
            ..HydratingRepository::default()
        };

        let rows = run_preview(
            query(vec![preview_item(
                0,
                collection_key(CollectionType::Collection, 7),
            )]),
            &repository,
            &PlayoutBuilder::default(),
        )
        .await
        .expect("empty preview remains valid");

        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn repository_builder_and_enrichment_failures_remain_typed() {
        let item = preview_item(0, collection_key(CollectionType::Collection, 7));
        let repository_failure = run_preview(
            query(vec![item.clone()]),
            &HydratingRepository {
                fail: true,
                ..HydratingRepository::default()
            },
            &CapturingBuilder::default(),
        )
        .await
        .expect_err("repository failure propagates");
        assert!(matches!(
            repository_failure,
            ApplicationError::Unavailable {
                operation: "load block preview content",
                ..
            }
        ));

        let builder_failure = run_preview(
            query(vec![item.clone()]),
            &HydratingRepository::default(),
            &CapturingBuilder {
                fail: true,
                ..CapturingBuilder::default()
            },
        )
        .await
        .expect_err("builder failure propagates");
        assert!(matches!(
            builder_failure,
            ApplicationError::Scheduling {
                operation: "block preview",
                ..
            }
        ));

        let enrichment_failure = run_preview(
            query(vec![item]),
            &HydratingRepository::default(),
            &CapturingBuilder {
                output_media_ids: vec![9_999],
                ..CapturingBuilder::default()
            },
        )
        .await
        .expect_err("missing enrichment propagates");
        assert!(matches!(
            enrichment_failure,
            ApplicationError::MissingReference {
                kind: "block preview media item",
                id: 9_999
            }
        ));
    }
}
