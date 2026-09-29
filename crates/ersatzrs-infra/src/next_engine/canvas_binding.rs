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
            && *self
                == Self {
                    session: self.session,
                    ..Self::new(channel, media, self.expires)
                }
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
