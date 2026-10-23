//! First-packet facts from the same bounded, already-owned producer prefix.
use std::collections::{btree_map::Entry, BTreeMap, BTreeSet};

use ersatzrs_domain::MediaStreamKind;
use tokio::{sync::oneshot, time::Instant};

use super::{finish_remote_ffprobe, FfprobeExtractor, FfprobeMediaVersion, Result};

impl FfprobeExtractor {
    pub(crate) async fn extract_producer_start_cancellable(
        &self,
        prefix: &[u8],
        deadline: Instant,
        cancellation: oneshot::Receiver<()>,
    ) -> Result<(FfprobeMediaVersion, BTreeMap<u32, bool>)> {
        let (status, stdout, stderr) = self
            .extract_prefix_output(prefix, false, true, deadline, cancellation)
            .await?;
        let media =
            finish_remote_ffprobe(status, "published interval media", &stdout, &stderr, None)?;
        let starts = packet_starts(&stdout, &media);
        Ok((media, starts))
    }
}

fn packet_starts(bytes: &[u8], media: &FfprobeMediaVersion) -> BTreeMap<u32, bool> {
    let videos: BTreeSet<_> = media
        .streams
        .iter()
        .filter(|s| s.kind == MediaStreamKind::Video)
        .filter_map(|s| u32::try_from(s.index).ok())
        .collect();
    let document: serde_json::Value = serde_json::from_slice(bytes).unwrap_or_default();
    let mut first = BTreeMap::new();
    for packet in document
        .get("packets")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(index) = packet
            .get("stream_index")
            .and_then(serde_json::Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
            .filter(|i| videos.contains(i))
        else {
            continue;
        };
        if let Entry::Vacant(entry) = first.entry(index) {
            // A malformed/corrupt first packet cannot be replaced by a later keyframe.
            let flag = match packet.get("flags").and_then(serde_json::Value::as_str) {
                Some("K__") => Some(true),
                Some("___") => Some(false),
                _ => None,
            };
            entry.insert(flag);
        }
    }
    first
        .into_iter()
        .filter_map(|(index, flag)| flag.map(|flag| (index, flag)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ersatzrs_domain::{MediaStream, MediaVersion};
    use serde_json::json;

    #[test]
    fn only_first_actual_video_packet_flags_qualify_copy_start() {
        let media = FfprobeMediaVersion {
            version: MediaVersion::default(),
            streams: vec![MediaStream {
                index: 3,
                kind: MediaStreamKind::Video,
                ..Default::default()
            }],
            field_orders: BTreeMap::default(),
            chapters: vec![],
            format_name: None,
            format_metadata: BTreeMap::default(),
        };
        for (flag, expected) in [
            ("K__", Some(true)),
            ("___", Some(false)),
            ("K_C", None),
            ("K_D", None),
            ("", None),
        ] {
            let input = json!({"packets":[{"stream_index":2,"flags":"K__"},
                {"stream_index":3,"flags":flag},{"stream_index":3,"flags":"K__"}]});
            let actual = packet_starts(&serde_json::to_vec(&input).unwrap(), &media);
            assert_eq!(actual.get(&3).copied(), expected);
            assert!(!actual.contains_key(&2));
        }
        assert!(packet_starts(b"{}", &media).is_empty());
    }
}
