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

/// Read-only preparation task counts, distinct from affected media items.
/// Aggregate legacy readiness counters remain available to older clients.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AddonMediaListPreparationSummary {
    pub interval_tasks: u64,
    pub download_tasks: u64,
    pub subtitle_tasks: u64,
    pub affected_items: u64,
    pub completed_tasks: u64,
    pub queued_tasks: u64,
    pub running_tasks: u64,
    pub waiting_tasks: u64,
    pub failed_tasks: u64,
    pub unqueued_tasks: u64,
    pub started_at: Option<String>,
    /// Latest durable job activity, including heartbeats; not byte progress.
    pub last_activity_at: Option<String>,
    /// Latest completed required task; never inferred from a worker heartbeat.
    pub last_progress_at: Option<String>,
    pub completed_at: Option<String>,
    pub retry_at: Option<String>,
}
