use super::{
    ClassicContext, EntityId, FillerKind, FillerMode, PlayoutItem, ProgramScheduleItem,
    SchedulingMediaItem,
};
use chrono::{DateTime, Duration, Timelike, Utc};
use ersatzrs_domain::MediaChapter;
use std::collections::VecDeque;

impl ClassicContext {
    /// Mirror PlayoutModeSchedulerBase.AddFiller: chapter intervals retain their
    /// source seeks while the wall-clock timeline includes the intervening filler.
    pub(super) fn build_chapter_block(
        &mut self,
        schedule: &ProgramScheduleItem,
        media: &SchedulingMediaItem,
        chapters: &[MediaChapter],
        start: DateTime<Utc>,
        guide_finish: Option<DateTime<Utc>>,
        scheduler: &'static str,
    ) -> Option<Vec<PlayoutItem>> {
        let before = self.filler_media.clone();
        if self.pad_filler_count(schedule) > 1 {
            self.result.warnings.push(format!(
                "classic schedule item {} skipped multiple pad fillers",
                schedule.id
            ));
            return Some(vec![
                self.primary_playout_item(schedule, media, start, scheduler)
            ]);
        }
        let mid_id = schedule.mid_roll_filler_id?;
        let mid = self
            .request
            .reference_data
            .classic_filler_presets
            .get(&mid_id)?
            .clone();
        if mid.filler_mode == FillerMode::Pad
            && mid.pad_to_nearest_minute.is_none_or(|value| value <= 0)
        {
            self.result.warnings.push(format!(
                "classic schedule item {} skipped invalid mid-roll pad target",
                schedule.id
            ));
            return Some(vec![
                self.primary_playout_item(schedule, media, start, scheduler)
            ]);
        }
        let mut pre = schedule.pre_roll_filler_id.map_or_else(Vec::new, |id| {
            self.add_count_or_duration_filler(schedule, id, start, FillerKind::PreRoll)
        });
        let mut post = schedule.post_roll_filler_id.map_or_else(Vec::new, |id| {
            self.add_count_or_duration_filler(schedule, id, start, FillerKind::PostRoll)
        });
        let mut content = Vec::new();
        for (index, chapter) in chapters.iter().enumerate() {
            let mut item = self.primary_playout_item(schedule, media, start, scheduler);
            item.in_point = chapter.start_time;
            item.out_point = chapter.end_time;
            item.finish = item.start + (chapter.end_time - chapter.start_time);
            item.guide_finish = guide_finish;
            content.push(item);
            if index + 1 < chapters.len() && mid.filler_mode != FillerMode::Pad {
                content.extend(self.add_count_or_duration_filler(
                    schedule,
                    mid_id,
                    start,
                    FillerKind::MidRoll,
                ));
            }
        }
        if mid.filler_mode == FillerMode::Pad {
            let total = total_duration(&pre) + total_duration(&content) + total_duration(&post);
            let remaining = pad_budget(start, total, mid.pad_to_nearest_minute);
            content = self.distribute_mid_pad(
                schedule,
                mid_id,
                content,
                start,
                remaining,
                mid.allow_watermarks,
            );
        } else if let Some(pad_id) = self.pad_filler_id(schedule) {
            let duration = total_duration(&pre) + total_duration(&content) + total_duration(&post);
            let kind = self.effective_pad_kind(schedule, media, pad_id)?;
            let pad = self.add_pad_filler(schedule, pad_id, start, start + duration, kind);
            if kind == FillerKind::PreRoll {
                pre.splice(0..0, pad);
            } else {
                post.extend(pad);
            }
        }
        pre.extend(content);
        pre.extend(post);
        let mut current = start;
        for item in &mut pre {
            let duration = item.finish - item.start;
            item.start = current;
            item.finish = current + duration;
            current = item.finish;
        }
        if guide_finish.is_some_and(|finish| current > finish) {
            self.filler_media = before;
            return None;
        }
        Some(pre)
    }

    fn distribute_mid_pad(
        &mut self,
        schedule: &ProgramScheduleItem,
        filler_id: EntityId,
        chapters: Vec<PlayoutItem>,
        start: DateTime<Utc>,
        remaining: Duration,
        allow_watermarks: bool,
    ) -> Vec<PlayoutItem> {
        let breaks = chapters.len().saturating_sub(1);
        let average = remaining / i32::try_from(breaks.max(1)).unwrap_or(i32::MAX);
        let mut queue: VecDeque<_> = self
            .add_duration_filler(
                schedule,
                filler_id,
                start,
                Self::effective_filler_kind(schedule, FillerKind::MidRoll),
                remaining,
                allow_watermarks,
            )
            .into();
        let mut result = Vec::new();
        let mut filled = Duration::zero();
        for (index, chapter) in chapters.into_iter().enumerate() {
            result.push(chapter);
            if index >= breaks {
                continue;
            }
            let mut in_break = Duration::zero();
            while in_break < average && filled < remaining {
                let items = if let Some(item) = queue.pop_front() {
                    vec![item]
                } else {
                    let gap = (average - in_break).min(remaining - filled);
                    self.fallback_items(schedule, start, start + gap)
                };
                if items.is_empty() {
                    break;
                }
                let duration = total_duration(&items);
                filled += duration;
                in_break += duration;
                result.extend(items);
            }
        }
        result
    }
}

fn total_duration(items: &[PlayoutItem]) -> Duration {
    items.iter().map(|item| item.finish - item.start).sum()
}

pub(super) fn pad_budget(
    start: DateTime<Utc>,
    duration: Duration,
    minutes: Option<i32>,
) -> Duration {
    let Some(minutes) = minutes.filter(|value| *value > 0) else {
        return Duration::zero();
    };
    let finish = start + duration;
    let current_minute = i64::from(finish.minute());
    let step = i64::from(minutes);
    let target_minute = ((current_minute + step - 1) / step) * step;
    let mut target = finish - Duration::minutes(current_minute) + Duration::minutes(target_minute)
        - Duration::seconds(i64::from(finish.second()))
        - Duration::nanoseconds(i64::from(finish.nanosecond()));
    if target <= finish {
        target += Duration::minutes(step);
    }
    target - finish
}
