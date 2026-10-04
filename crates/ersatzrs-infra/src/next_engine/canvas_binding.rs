//! Occurrence identity for the private canvas callback. Mutable graphics are
//! intentionally excluded: removing graphics must yield transparent frames.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::super::schedule::ScheduledMedia;

/// A published canvas refers to one exact programme occurrence, never a path or
/// provider credential. The callback also requires an admitted process session.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CanvasBinding {
    /// Transient preview/fallback sources are issued for one process only.
    /// Persisted manifests use the active process header when this is absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<uuid::Uuid>,
    /// Next resets a dynamically resolved item's start. Retain that signed
    /// origin so its chunk offsets still address the original programme.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub playout_start: Option<DateTime<Utc>>,
    /// Frozen video track used for source-content geometry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video_stream_index: Option<i32>,
    pub channel: i64,
    pub item: i64,
    pub media: i64,
    pub version: i64,
    pub file: i64,
    pub start: DateTime<Utc>,
    pub finish: DateTime<Utc>,
    pub in_point_ms: i64,
    pub out_point_ms: i64,
    pub expires: DateTime<Utc>,
}

impl CanvasBinding {
    /// Bind the target-channel timeline, including already-applied mirror
    /// offsets. Expiry is supplied by the owning publication/session.
    #[must_use]
    pub fn new(channel: i64, media: &ScheduledMedia, expires: DateTime<Utc>) -> Self {
        Self {
            session: None,
            playout_start: None,
            video_stream_index: None,
            channel,
            item: media.item.id,
            media: media.item.media_item_id,
            version: media.media_version_id,
            file: media.media_file_id,
            start: media.item.start,
            finish: media.item.finish,
            in_point_ms: media.item.in_point.num_milliseconds(),
            out_point_ms: media.item.out_point.num_milliseconds(),
            expires,
        }
    }

    #[must_use]
    /// Restrict a transient source to its owning admitted process.
    pub fn for_session(mut self, session: uuid::Uuid) -> Self {
        self.session = Some(session);
        self
    }

    /// Revalidate against the current persisted or session-owned preview item.
    /// Call before and after asynchronous preparation; matching graphics IDs are
    /// deliberately unnecessary because the callback renders current selection.
    #[must_use]
    pub fn matches(&self, channel: i64, media: &ScheduledMedia, now: DateTime<Utc>) -> bool {
        self.expires > now
            && self.finish > self.start
            && self.in_point_ms >= 0
            && self.out_point_ms >= 0
            && self.video_stream_index.is_none_or(|index| index >= 0)
            && self
                .playout_start
                .is_none_or(|start| start >= self.start && start < self.finish)
            && *self
                == Self {
                    session: self.session,
                    playout_start: self.playout_start,
                    video_stream_index: self.video_stream_index,
                    ..Self::new(channel, media, self.expires)
                }
    }

    pub(crate) fn timeline(
        &self,
        request: &super::CanvasRequest,
    ) -> Result<super::CanvasTimeline, super::InvalidCanvasRequest> {
        super::CanvasTimeline::new(self.start, self.finish, &self.programme_request(request)?)
    }

    pub(crate) fn programme_request(
        &self,
        request: &super::CanvasRequest,
    ) -> Result<super::CanvasRequest, super::InvalidCanvasRequest> {
        let start = self.playout_start.unwrap_or(self.start);
        if start < self.start || start >= self.finish {
            return Err(super::InvalidCanvasRequest);
        }
        let delta = u64::try_from((start - self.start).num_milliseconds())
            .map_err(|_| super::InvalidCanvasRequest)?;
        let offset = u64::try_from(request.offset().as_millis())
            .ok()
            .and_then(|offset| offset.checked_add(delta))
            .ok_or(super::InvalidCanvasRequest)?;
        super::CanvasRequest::parse(
            &offset.to_string(),
            &request.duration().as_millis().to_string(),
            request.frame_rate(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeDelta;
    use ersatzrs_domain::PlayoutItem;

    fn media() -> ScheduledMedia {
        let start = DateTime::from_timestamp(1_000, 0).unwrap();
        ScheduledMedia {
            item: PlayoutItem {
                id: 8,
                media_item_id: 9,
                start,
                finish: start + TimeDelta::minutes(5),
                in_point: TimeDelta::seconds(10),
                out_point: TimeDelta::seconds(310),
                ..PlayoutItem::default()
            },
            media_version_id: 10,
            media_file_id: 11,
            path: "synthetic.mp4".into(),
            remote_stream_id: None,
            candidate_overlays: None,
        }
    }

    #[test]
    fn canvas_binding_rejects_changed_occurrence_and_expiry_but_allows_graphics_removal() {
        let mut media = media();
        let now = media.item.start;
        let binding = CanvasBinding::new(1, &media, media.item.finish + TimeDelta::seconds(30));
        assert!(binding.matches(1, &media, now));
        assert!(!binding.matches(2, &media, now));
        assert!(!binding.matches(1, &media, binding.expires));
        media.item.disable_watermarks = true;
        assert!(binding.matches(1, &media, now));
        for field in [
            "item", "media", "version", "file", "start", "finish", "in", "out",
        ] {
            let mut changed = media.clone();
            match field {
                "item" => changed.item.id += 1,
                "media" => changed.item.media_item_id += 1,
                "version" => changed.media_version_id += 1,
                "file" => changed.media_file_id += 1,
                "start" => changed.item.start += TimeDelta::seconds(1),
                "finish" => changed.item.finish += TimeDelta::seconds(1),
                "in" => changed.item.in_point += TimeDelta::seconds(1),
                _ => changed.item.out_point += TimeDelta::seconds(1),
            }
            assert!(!binding.matches(1, &changed, now), "{field}");
        }
    }

    #[test]
    fn canvas_binding_preserves_mirror_and_transient_preview_identity() {
        let mut media = media();
        media.item.id = 0;
        media.item.start += TimeDelta::hours(1);
        media.item.finish += TimeDelta::hours(1);
        let binding = CanvasBinding::new(2, &media, media.item.finish);
        let roundtrip: CanvasBinding =
            serde_json::from_str(&serde_json::to_string(&binding).unwrap()).unwrap();
        assert!(roundtrip.matches(2, &media, media.item.start));
        media.item.start -= TimeDelta::hours(1);
        media.item.finish -= TimeDelta::hours(1);
        assert!(!roundtrip.matches(2, &media, media.item.start));
    }
}
