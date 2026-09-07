use serde::{Deserialize, Serialize};

/// Saved list policy; effective playback delivery is frozen at publication.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListDeliveryMode {
    /// Resolve and stream media on demand.
    #[default]
    Stream,
    /// Prepare included finite videos before publication.
    DownloadAhead,
    /// Prepare only finite identities first imported after the saved cutoff.
    DownloadAheadNewVideos,
}

#[cfg(test)]
mod tests {
    use super::AddonMediaListDeliveryMode;

    #[test]
    fn delivery_modes_keep_provider_neutral_wire_names() {
        for (mode, wire) in [
            (AddonMediaListDeliveryMode::Stream, "\"stream\""),
            (
                AddonMediaListDeliveryMode::DownloadAhead,
                "\"download_ahead\"",
            ),
            (
                AddonMediaListDeliveryMode::DownloadAheadNewVideos,
                "\"download_ahead_new_videos\"",
            ),
        ] {
            assert_eq!(serde_json::to_string(&mode).unwrap(), wire);
            assert_eq!(
                serde_json::from_str::<AddonMediaListDeliveryMode>(wire).unwrap(),
                mode
            );
        }
        assert_eq!(
            AddonMediaListDeliveryMode::default(),
            AddonMediaListDeliveryMode::Stream
        );
        assert!(serde_json::from_str::<AddonMediaListDeliveryMode>("\"unsupported\"").is_err());
    }
}
