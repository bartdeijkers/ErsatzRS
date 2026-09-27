/// Sink that scanner entry points emit fractional progress into.
///
/// Mirrors the role of `IScannerProxyService` + the `LibraryScanProgress`
/// notification in `ErsatzTV`. A sink is scoped to a single library scan,
/// so implementations do not need to carry the library id themselves.
///
/// Implementations must be `Send + Sync` because scanners run on
/// `tokio::task::spawn`. `report` is called from inside async code with
/// values in the range `[0.0, 1.0]`; values are clamped by consumers.
pub trait ScanProgressSink: Send + Sync {
    fn report(&self, fraction: f32);
}

/// `()` is a no-op sink, used as the default when no progress reporting is
/// wired up (CLI scanner binary, unit tests, repository-level callers).
impl ScanProgressSink for () {
    fn report(&self, _fraction: f32) {}
}

/// Bounded reason for a playout refresh discovered while reconciling a scan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanPlayoutRefreshReason {
    /// A managed remote-stream playlist gained one or more stable members.
    RemoteStreamMembershipAdded,
    /// A managed remote-stream playlist lost one or more stable members.
    RemoteStreamMembershipRemoved,
    /// Existing stable members changed their playable projection.
    RemoteStreamMembershipReplaced,
    /// Upstream identity replacement changed the persisted media-item identity.
    RemoteStreamMembershipReidentified,
    /// Stable members retained identity but changed playlist order.
    RemoteStreamMembershipReordered,
    /// More than one bounded membership-change category applies.
    RemoteStreamMembershipMixed,
    /// A native provider collection changed membership, order or searchable tags.
    ProviderCollectionChanged,
    /// Plex network membership changed searchable show or episode metadata.
    ProviderNetworkChanged,
    /// Coalesced native provider and remote-stream dependency changes.
    ContentMembershipMixed,
}

impl ScanPlayoutRefreshReason {
    /// Stable diagnostic token that never includes provider or media identity.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RemoteStreamMembershipAdded => "remote-stream-membership-added",
            Self::RemoteStreamMembershipRemoved => "remote-stream-membership-removed",
            Self::RemoteStreamMembershipReplaced => "remote-stream-membership-replaced",
            Self::RemoteStreamMembershipReidentified => "remote-stream-membership-reidentified",
            Self::RemoteStreamMembershipReordered => "remote-stream-membership-reordered",
            Self::RemoteStreamMembershipMixed => "remote-stream-membership-mixed",
            Self::ProviderCollectionChanged => "provider-collection-changed",
            Self::ProviderNetworkChanged => "provider-network-changed",
            Self::ContentMembershipMixed => "content-membership-mixed",
        }
    }

    /// Decode a persisted bounded diagnostic token.
    #[must_use]
    pub fn from_token(token: &str) -> Option<Self> {
        match token {
            "remote-stream-membership-added" => Some(Self::RemoteStreamMembershipAdded),
            "remote-stream-membership-removed" => Some(Self::RemoteStreamMembershipRemoved),
            "remote-stream-membership-replaced" => Some(Self::RemoteStreamMembershipReplaced),
            "remote-stream-membership-reidentified" => {
                Some(Self::RemoteStreamMembershipReidentified)
            }
            "remote-stream-membership-reordered" => Some(Self::RemoteStreamMembershipReordered),
            "remote-stream-membership-mixed" => Some(Self::RemoteStreamMembershipMixed),
            "provider-collection-changed" => Some(Self::ProviderCollectionChanged),
            "provider-network-changed" => Some(Self::ProviderNetworkChanged),
            "content-membership-mixed" => Some(Self::ContentMembershipMixed),
            _ => None,
        }
    }

    /// Coalesce multiple discoveries for one playout without losing that the
    /// scan crossed more than one dependency category.
    #[must_use]
    pub const fn merge(self, other: Self) -> Self {
        if self as u8 == other as u8 {
            self
        } else if matches!(
            self,
            Self::ProviderCollectionChanged
                | Self::ProviderNetworkChanged
                | Self::ContentMembershipMixed
        ) || matches!(
            other,
            Self::ProviderCollectionChanged
                | Self::ProviderNetworkChanged
                | Self::ContentMembershipMixed
        ) {
            Self::ContentMembershipMixed
        } else {
            Self::RemoteStreamMembershipMixed
        }
    }
}

/// One dependency-scoped playout refresh produced by a completed scan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScanPlayoutRefresh {
    pub playout_id: crate::EntityId,
    pub reason: ScanPlayoutRefreshReason,
}
