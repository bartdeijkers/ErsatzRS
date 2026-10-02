//! Read-only television discovery diagnostics shared by scanner and administration.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// A reason that a filesystem entry was not discovered as programme media.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DiscoveryIssueKind {
    /// A non-media file whose suffix is not a known metadata/track sidecar.
    IgnoredExtension,
    /// Media or a directory outside the supported root/show/season structure.
    UnsupportedNesting,
    /// A fully inspected show contained no episode or extra candidates.
    EmptyShow,
    /// A directory or entry could not be inspected.
    UnreadableEntry,
    /// A directory alias or cycle refers to a directory already inspected.
    RepeatedDirectory,
    /// The preview depth bound prevented further inspection.
    DepthLimit,
}

/// One bounded, root-relative diagnostic example, suitable for the operator UI.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveryIssue {
    /// Typed reason; adapters localize the label.
    pub kind: DiscoveryIssueKind,
    /// Root-relative path; never written to general service logs.
    pub path: String,
}

/// Counts describe discovery candidates, not successfully decoded media.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelevisionDiscoveryReport {
    /// Entries inspected, including metadata and directories.
    pub inspected_entries: usize,
    /// Shows with at least one episode or extra candidate.
    pub shows: usize,
    /// Season directories with episode or extra candidates.
    pub seasons: usize,
    /// Ordinary episode candidates.
    pub episodes: usize,
    /// Classified bonus-media candidates; never counted as episodes.
    pub extras: usize,
    /// Hidden and `AppleDouble` entries deliberately ignored.
    pub hidden_entries: usize,
    /// Complete issue counts for the inspected portion, independent of sample limits.
    pub issue_counts: BTreeMap<DiscoveryIssueKind, usize>,
    /// Bounded examples; absence does not imply that an issue count is zero.
    pub examples: Vec<DiscoveryIssue>,
    /// Work stopped at a preview limit; counts are only a partial inventory.
    pub truncated: bool,
}

/// One submitted folder and its read-only candidate inventory.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelevisionFolderPreview {
    /// The operator-submitted root, for matching the result to the draft form.
    pub path: String,
    /// Discovery counts and bounded diagnostic samples.
    pub discovery: TelevisionDiscoveryReport,
}
