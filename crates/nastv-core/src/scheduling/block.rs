use chrono::{DateTime, Duration, Utc};
use nastv_domain::{BlockStopScheduling, EntityId, FillerKind};
use serde::{Deserialize, Serialize};

use super::{
    cleanup_history, history, lookup_media, playout_item, PlayoutSchedulingEngine, SchedulingError,
    SchedulingRequest, SchedulingResult,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectiveBlock {
    pub block_id: EntityId,
    pub template_item_id: EntityId,
    pub name: String,
    pub start: DateTime<Utc>,
    pub duration: Duration,
    pub stop_scheduling: BlockStopScheduling,
    pub items: Vec<EffectiveBlockItem>,
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Duration, Utc};
    use nastv_domain::Playout;

    use super::*;
    use crate::scheduling::{SchedulingMediaItem, SchedulingReferenceData};

    #[test]
    fn block_scheduling_matches_ersatztv_one_item_per_block_item_snapshot() {
        let start = DateTime::parse_from_rfc3339("2026-04-15T08:00:00Z")
            .expect("valid time")
            .with_timezone(&Utc);
        let mut reference_data = SchedulingReferenceData::default();
        reference_data.media_by_key.insert(
            "movies".to_owned(),
            vec![
                SchedulingMediaItem::new(1, Duration::minutes(30)),
                SchedulingMediaItem::new(2, Duration::minutes(45)),
            ],
        );
        reference_data.media_by_key.insert(
            "shorts".to_owned(),
            vec![SchedulingMediaItem::new(9, Duration::minutes(5))],
        );
        reference_data.effective_blocks.push(EffectiveBlock {
            block_id: 10,
            template_item_id: 20,
            name: "Morning".to_owned(),
            start,
            duration: Duration::hours(1),
            stop_scheduling: BlockStopScheduling::BeforeDurationEnd,
            items: vec![
                EffectiveBlockItem {
                    id: 1,
                    content: "movies".to_owned(),
                    include_in_program_guide: true,
                    disable_watermarks: false,
                    watermark_ids: vec![42],
                    graphics_element_ids: vec![],
                },
                EffectiveBlockItem {
                    id: 2,
                    content: "shorts".to_owned(),
                    include_in_program_guide: false,
                    disable_watermarks: true,
                    watermark_ids: vec![],
                    graphics_element_ids: vec![77],
                },
            ],
        });

        let result = BlockSchedulingEngine
            .schedule(SchedulingRequest {
                playout: Playout {
                    id: 100,
                    ..Playout::default()
                },
                start,
                finish: start + Duration::hours(2),
                mode: crate::scheduling::PlayoutBuildMode::Reset,
                reference_data,
            })
            .expect("block schedule builds");

        insta::assert_json_snapshot!(serde_json::json!({
            "items": result.added_items.iter().map(|item| serde_json::json!({
                "media": item.media_item_id,
                "start": item.start.to_rfc3339(),
                "finish": item.finish.to_rfc3339(),
                "filler": format!("{:?}", item.filler_kind),
                "guide_group": item.guide_group,
                "watermarks": item.watermark_ids,
                "graphics": item.graphics_element_ids,
                "disable_watermarks": item.disable_watermarks,
            })).collect::<Vec<_>>(),
            "history": result.added_history.iter().map(|history| &history.key).collect::<Vec<_>>(),
        }));
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectiveBlockItem {
    pub id: EntityId,
    pub content: String,
    pub include_in_program_guide: bool,
    pub disable_watermarks: bool,
    pub watermark_ids: Vec<EntityId>,
    pub graphics_element_ids: Vec<EntityId>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct BlockSchedulingEngine;

impl PlayoutSchedulingEngine for BlockSchedulingEngine {
    fn schedule(&self, request: SchedulingRequest) -> Result<SchedulingResult, SchedulingError> {
        let mut result = SchedulingResult {
            remove_before: Some(request.start),
            ..SchedulingResult::default()
        };

        let mut blocks = request.reference_data.effective_blocks.clone();
        blocks.sort_by_key(|block| block.start);

        for block in blocks {
            if block.start >= request.finish {
                continue;
            }

            let block_finish = block.start + block.duration;
            let mut current_time = block.start.max(request.start);

            for block_item in &block.items {
                if current_time >= block_finish {
                    break;
                }

                let mut enumerator = lookup_media(&request.reference_data, &block_item.content);
                if enumerator.is_empty() {
                    result.warnings.push(format!(
                        "block item {} skipped empty content {}",
                        block_item.id, block_item.content
                    ));
                    continue;
                }

                if current_time < block_finish {
                    let Some(media_item) = enumerator.current().cloned() else {
                        break;
                    };
                    let item_finish = current_time + media_item.duration;
                    if block.stop_scheduling == BlockStopScheduling::BeforeDurationEnd
                        && item_finish > block_finish
                    {
                        break;
                    }

                    let item = playout_item(
                        request.playout.id,
                        &media_item,
                        current_time,
                        media_item.duration,
                        i32::try_from(block.template_item_id).unwrap_or_default(),
                        if block_item.include_in_program_guide {
                            FillerKind::None
                        } else {
                            FillerKind::GuideMode
                        },
                        None,
                        block_item.disable_watermarks,
                        block_item.watermark_ids.clone(),
                        block_item.graphics_element_ids.clone(),
                        Some(format!(
                            r#"{{"blockId":{},"blockItemId":{},"enumerator":"{}","index":{}}}"#,
                            block.block_id, block_item.id, enumerator.key, enumerator.index
                        )),
                    );

                    result.added_history.push(history(
                        request.playout.id,
                        &block_item.content,
                        &item,
                        None,
                        enumerator.current_index(),
                        super::history_details_for_media_item(&media_item),
                        false,
                    ));
                    current_time = item.finish;
                    result.added_items.push(item);
                    enumerator.advance();
                }
            }
        }

        result.history_to_remove.extend(cleanup_history(
            &request.reference_data.existing_history,
            request.start,
            request.reference_data.max_playout_offset,
        ));
        Ok(result)
    }
}
