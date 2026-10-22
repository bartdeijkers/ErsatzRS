//! Deferred song background identity; credentials are installed only at publication.

use chrono::{DateTime, TimeDelta, Utc};
use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_domain::{Channel, ChannelSongVideoMode, PlayoutItem, Resolution};
use serde::{Deserialize, Serialize};

use crate::db::DbPool;

use super::{canvas::CanvasBinding, media::MediaRelay, protocol, schedule};

/// Exact occurrence and advertised PNG geometry, never a private media path.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SongBinding {
    pub occurrence: CanvasBinding,
    pub width: i32,
    pub height: i32,
    pub mode: ChannelSongVideoMode,
}

#[derive(Clone, Serialize)]
pub(crate) struct SongProjection {
    binding: SongBinding,
    hint: protocol::ProbeHint,
}

impl SongProjection {
    pub(crate) async fn new(
        pool: &DbPool,
        channel: &Channel,
        item: &PlayoutItem,
        media_version_id: i64,
        resolution: &Resolution,
    ) -> Result<Self> {
        let media = schedule::bind_media(pool, item.clone()).await?;
        if media.media_version_id != media_version_id {
            return Err(unavailable());
        }
        let width = u32::try_from(resolution.width)
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(unavailable)?;
        let height = u32::try_from(resolution.height)
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(unavailable)?;
        let duration_ms = u64::try_from((item.finish - item.start).num_milliseconds())
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(unavailable)?;
        let expires = item
            .finish
            .checked_add_signed(TimeDelta::hours(2))
            .ok_or_else(unavailable)?;
        Ok(Self {
            binding: SongBinding {
                occurrence: CanvasBinding::new(channel.id, &media, expires),
                width: resolution.width,
                height: resolution.height,
                mode: channel.song_video_mode,
            },
            hint: protocol::ProbeHint {
                format_name: Some("png_pipe".into()),
                duration_ms: Some(duration_ms),
                audio: Vec::new(),
                subtitle: Vec::new(),
                video: vec![protocol::VideoProbeHint {
                    source_frame_cut: None,
                    codec: "png".into(),
                    width,
                    height,
                    pix_fmt: "rgb24".into(),
                    stream_index: 0,
                    frame_rate: None,
                    profile: None,
                    field_order: None,
                    sample_aspect_ratio: Some("1:1".into()),
                    display_aspect_ratio: None,
                    color_range: None,
                    color_space: None,
                    color_transfer: None,
                    color_primaries: None,
                    dv_profile: None,
                    has_hdr10_metadata: None,
                    rotation: None,
                }],
            },
        })
    }

    pub(crate) fn publish(
        &self,
        relay: &MediaRelay,
        session: Option<uuid::Uuid>,
        start: DateTime<Utc>,
    ) -> Result<protocol::Source> {
        let mut binding = self.binding.clone();
        if start < binding.occurrence.start
            || start >= binding.occurrence.finish
            || (binding.occurrence.item == 0 && session.is_none())
        {
            return Err(unavailable());
        }
        binding.occurrence.playout_start = (start != binding.occurrence.start).then_some(start);
        binding.occurrence.session = session;
        // Keep the projection fingerprint stable, then renew the emitted ticket
        // for historical troubleshooting without changing session revocation.
        if session.is_some() {
            binding.occurrence.expires =
                super::troubleshooting::callback_expiry(binding.occurrence.finish)?;
        }
        relay.song_source(&binding, self.hint.clone())
    }
}

fn unavailable() -> RepositoryError {
    RepositoryError::unavailable("Next song projection", "song occurrence is invalid")
}

#[cfg(test)]
mod tests {
    use super::super::{
        media::{CANVAS_HEADER, SESSION_HEADER, SONG_HEADER},
        protocol::Source,
    };
    use super::*;

    fn projection(item: i64) -> SongProjection {
        let start = Utc::now();
        SongProjection {
            binding: SongBinding {
                occurrence: CanvasBinding {
                    session: None,
                    playout_start: None,
                    video_stream_index: None,
                    channel: 1,
                    item,
                    media: 3,
                    version: 4,
                    file: 5,
                    start,
                    finish: start + TimeDelta::seconds(30),
                    in_point_ms: 0,
                    out_point_ms: 30_000,
                    expires: start + TimeDelta::hours(2),
                },
                width: 320,
                height: 180,
                mode: ChannelSongVideoMode::Default,
            },
            hint: protocol::ProbeHint {
                video: Vec::new(),
                audio: Vec::new(),
                subtitle: Vec::new(),
                format_name: Some("png_pipe".into()),
                duration_ms: Some(30_000),
            },
        }
    }

    fn header<'a>(headers: &'a [String], name: &str) -> &'a str {
        headers
            .iter()
            .find_map(|entry| {
                let (key, value) = entry.split_once(": ")?;
                (key == name).then_some(value)
            })
            .unwrap()
    }

    #[test]
    fn song_ticket_is_purpose_bound_and_rejects_geometry_mode_occurrence_and_expiry_changes() {
        let relay = MediaRelay::new();
        relay.configure("127.0.0.1:8409".parse().unwrap()).unwrap();
        let projection = projection(2);
        let binding = &projection.binding;
        let Source::Http {
            uri,
            headers,
            in_point_ms,
            out_point_ms,
            is_live,
            ..
        } = projection
            .publish(&relay, None, binding.occurrence.start)
            .unwrap()
        else {
            panic!("song source")
        };
        assert!(uri.contains("/ffmpeg/next-song-background?binding="));
        assert!(!uri.contains(header(&headers, SONG_HEADER)));
        assert_eq!(
            header(&headers, SESSION_HEADER),
            "{{ERSATZRS_NEXT_SESSION}}"
        );
        assert_eq!(
            (in_point_ms, out_point_ms, is_live),
            (None, None, Some(false))
        );
        let published_url = url::Url::parse(&uri).unwrap();
        let encoded = published_url
            .query_pairs()
            .find(|(key, _)| key == "binding")
            .unwrap()
            .1
            .into_owned();
        let published_binding: SongBinding = serde_json::from_str(&encoded).unwrap();
        let binding = &published_binding;
        let ticket = header(&headers, SONG_HEADER);
        assert!(relay.authorized_song(binding, ticket, binding.occurrence.start));
        for field in [
            "width", "height", "mode", "channel", "item", "media", "version", "file", "start",
            "finish", "in", "out", "expires", "session", "origin",
        ] {
            let mut changed = binding.clone();
            match field {
                "width" => changed.width += 1,
                "height" => changed.height += 1,
                "mode" => changed.mode = ChannelSongVideoMode::WithProgress,
                "channel" => changed.occurrence.channel += 1,
                "item" => changed.occurrence.item += 1,
                "media" => changed.occurrence.media += 1,
                "version" => changed.occurrence.version += 1,
                "file" => changed.occurrence.file += 1,
                "start" => changed.occurrence.start += TimeDelta::milliseconds(1),
                "finish" => changed.occurrence.finish += TimeDelta::milliseconds(1),
                "in" => changed.occurrence.in_point_ms += 1,
                "out" => changed.occurrence.out_point_ms += 1,
                "expires" => changed.occurrence.expires += TimeDelta::milliseconds(1),
                "session" => changed.occurrence.session = Some(uuid::Uuid::new_v4()),
                "origin" => changed.occurrence.playout_start = Some(binding.occurrence.start),
                _ => unreachable!(),
            }
            assert!(
                !relay.authorized_song(&changed, ticket, binding.occurrence.start),
                "{field}"
            );
        }
        assert!(!relay.authorized_song(binding, ticket, binding.occurrence.expires));
        assert!(!MediaRelay::new().authorized_song(binding, ticket, binding.occurrence.start));
        let Source::Http {
            headers: canvas, ..
        } = relay
            .canvas_source(&binding.occurrence, projection.hint)
            .unwrap()
        else {
            panic!("canvas source")
        };
        assert!(!relay.authorized_song(
            binding,
            header(&canvas, CANVAS_HEADER),
            binding.occurrence.start
        ));
    }

    #[test]
    fn transient_song_publication_requires_and_retains_its_exact_session_and_origin() {
        let relay = MediaRelay::new();
        relay.configure("127.0.0.1:8409".parse().unwrap()).unwrap();
        let projection = projection(0);
        let start = projection.binding.occurrence.start + TimeDelta::seconds(3);
        assert!(projection.publish(&relay, None, start).is_err());
        assert!(projection
            .publish(
                &relay,
                Some(uuid::Uuid::new_v4()),
                projection.binding.occurrence.finish
            )
            .is_err());
        let session = uuid::Uuid::new_v4();
        let Source::Http { uri, headers, .. } =
            projection.publish(&relay, Some(session), start).unwrap()
        else {
            panic!("song source")
        };
        assert_eq!(header(&headers, SESSION_HEADER), session.to_string());
        let url = url::Url::parse(&uri).unwrap();
        let encoded = url
            .query_pairs()
            .find(|(key, _)| key == "binding")
            .unwrap()
            .1;
        let binding: SongBinding = serde_json::from_str(&encoded).unwrap();
        assert_eq!(binding.occurrence.session, Some(session));
        assert_eq!(binding.occurrence.playout_start, Some(start));
        assert!(relay.authorized_song(&binding, header(&headers, SONG_HEADER), start));
    }
    #[test]
    fn historical_song_ticket_is_fresh_without_mutating_fingerprinted_projection() {
        let relay = MediaRelay::new();
        relay.configure("127.0.0.1:8409".parse().unwrap()).unwrap();
        let mut projection = projection(2);
        let old = Utc::now() - TimeDelta::days(3);
        projection.binding.occurrence.start = old;
        projection.binding.occurrence.finish = old + TimeDelta::seconds(30);
        projection.binding.occurrence.expires = old + TimeDelta::hours(2);
        let before = serde_json::to_value(&projection).unwrap();
        let session = uuid::Uuid::new_v4();
        let Source::Http { uri, headers, .. } =
            projection.publish(&relay, Some(session), old).unwrap()
        else {
            panic!("song source");
        };
        let url = url::Url::parse(&uri).unwrap();
        let encoded = url
            .query_pairs()
            .find(|(key, _)| key == "binding")
            .unwrap()
            .1
            .into_owned();
        let published: SongBinding = serde_json::from_str(&encoded).unwrap();
        assert!(published.occurrence.expires > Utc::now());
        assert_eq!(published.occurrence.session, Some(session));
        assert!(relay.authorized_song(&published, header(&headers, SONG_HEADER), Utc::now()));
        assert!(!relay.authorized_song(
            &published,
            header(&headers, SONG_HEADER),
            published.occurrence.expires
        ));
        assert_eq!(serde_json::to_value(&projection).unwrap(), before);
    }
}
