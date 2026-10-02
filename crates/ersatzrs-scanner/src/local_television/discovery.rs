//! Shared television scan/preview discovery. See ADR 0108.
//!
//! Upstream: `LocalFolderScanner.VideoFileExtensions` and
//! `TelevisionFolderScanner.ScanSeasons/ScanEpisodes`. Unknown show children
//! are diagnosed, never recursively promoted to seasons.

use std::collections::{BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ersatzrs_core::television_discovery::{
    DiscoveryIssue, DiscoveryIssueKind, TelevisionDiscoveryReport, TelevisionFolderPreview,
};
use ersatzrs_domain::{classify_extra_directory_name, classify_extra_video};

use super::{parse_season_number_for_folder, ExtraVideoWork, SeasonWork, ShowWork};

// Input candidates, not a decoder guarantee. Include the complete upstream
// video list and common MPEG/ASF/Flash/3GPP suffixes. The existing ffprobe path
// validates actual media and reports unavailable optional decoders.
const VIDEO_EXTENSIONS: &[&str] = &[
    "mpg", "mp2", "mpeg", "mpe", "mpv", "ogg", "ogv", "mp4", "m4p", "m4v", "avi", "wmv", "asf",
    "mov", "qt", "mkv", "m2ts", "mts", "ts", "m2t", "vob", "webm", "flv", "f4v", "3gp", "3g2",
    "mxf", "m2v", "m2p", "ogm", "dv", "nut", "rm", "rmvb", "divx", "wtv", "dvr-ms", "avs",
];
const SIDECAR_EXTENSIONS: &[&str] = &[
    "nfo", "xml", "jpg", "jpeg", "png", "gif", "tbn", "webp", "srt", "ass", "ssa", "sub", "idx",
    "vtt", "sup", "smi", "scc", "ttml",
];
const MAX_EXAMPLES: usize = 40;
const MAX_EXAMPLE_CHARS: usize = 512;

#[derive(Clone, Copy)]
struct Limits {
    entries: usize,
    depth: usize,
    elapsed: Option<Duration>,
}

impl Limits {
    const PREVIEW: Self = Self {
        entries: 10_000,
        depth: 32,
        elapsed: Some(Duration::from_secs(2)),
    };
    const SCAN: Self = Self {
        entries: usize::MAX,
        depth: usize::MAX,
        elapsed: None,
    };
}

/// Inspect at most 10,000 entries, 32 directory levels and two seconds of
/// cooperative filesystem work. Does not probe media, write files or publish
/// metadata. Call from a bounded blocking worker: an individual OS filesystem
/// call can outlive the cooperative deadline.
#[must_use]
pub fn preview_television_folder(root: &Path) -> TelevisionDiscoveryReport {
    discover(root, Limits::PREVIEW).1
}

/// Preview several roots with one shared entry/time budget. The caller limits
/// the number and length of root strings before admitting a blocking worker.
#[must_use]
pub fn preview_television_folders(roots: &[String]) -> Vec<TelevisionFolderPreview> {
    let started = Instant::now();
    let mut remaining_entries = Limits::PREVIEW.entries;
    roots
        .iter()
        .map(|root| {
            let limits = Limits {
                entries: remaining_entries,
                elapsed: Some(Duration::from_secs(2).saturating_sub(started.elapsed())),
                ..Limits::PREVIEW
            };
            let discovery = discover(Path::new(root), limits).1;
            remaining_entries = remaining_entries.saturating_sub(discovery.inspected_entries);
            TelevisionFolderPreview {
                path: root.clone(),
                discovery,
            }
        })
        .collect()
}

pub(super) fn collect_show_work(root: &str) -> Vec<ShowWork> {
    discover(Path::new(root), Limits::SCAN).0
}

pub(super) fn collect_show_work_with_report(
    root: &str,
) -> (Vec<ShowWork>, TelevisionDiscoveryReport) {
    discover(Path::new(root), Limits::SCAN)
}

struct Discovery<'a> {
    root: &'a Path,
    limits: Limits,
    started: Instant,
    visited: BTreeSet<PathBuf>,
    report: TelevisionDiscoveryReport,
}

impl Discovery<'_> {
    fn deadline_reached(&mut self) -> bool {
        let reached = self
            .limits
            .elapsed
            .is_some_and(|limit| self.started.elapsed() >= limit);
        self.report.truncated |= reached;
        reached
    }

    fn stopped(&mut self) -> bool {
        if self.report.inspected_entries >= self.limits.entries
            || self
                .limits
                .elapsed
                .is_some_and(|limit| self.started.elapsed() >= limit)
        {
            self.report.truncated = true;
        }
        self.report.truncated
    }

    fn issue(&mut self, kind: DiscoveryIssueKind, path: &Path) {
        *self.report.issue_counts.entry(kind).or_default() += 1;
        if self.report.examples.len() < MAX_EXAMPLES {
            self.report.examples.push(DiscoveryIssue {
                kind,
                path: path
                    .strip_prefix(self.root)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .chars()
                    .take(MAX_EXAMPLE_CHARS)
                    .collect(),
            });
        }
    }

    fn entries(&mut self, directory: &Path, depth: usize) -> Vec<PathBuf> {
        if self.stopped() {
            return Vec::new();
        }
        if depth > self.limits.depth {
            self.issue(DiscoveryIssueKind::DepthLimit, directory);
            // A depth limit makes the report partial but does not prevent
            // inspection of other shallower directories.
            return Vec::new();
        }
        let Ok(canonical) = directory.canonicalize() else {
            self.issue(DiscoveryIssueKind::UnreadableEntry, directory);
            return Vec::new();
        };
        if !self.visited.insert(canonical) {
            self.issue(DiscoveryIssueKind::RepeatedDirectory, directory);
            return Vec::new();
        }
        let Ok(mut entries) = std::fs::read_dir(directory) else {
            self.issue(DiscoveryIssueKind::UnreadableEntry, directory);
            return Vec::new();
        };
        let mut paths = Vec::new();
        while !self.stopped() {
            let Some(entry) = entries.next() else {
                break;
            };
            self.report.inspected_entries += 1;
            match entry {
                Ok(entry) => {
                    let path = entry.path();
                    if path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.starts_with('.'))
                    {
                        self.report.hidden_entries += 1;
                    } else {
                        paths.push(path);
                    }
                }
                Err(_) => self.issue(DiscoveryIssueKind::UnreadableEntry, directory),
            }
        }
        paths.sort();
        paths
    }

    fn video(&mut self, path: &Path) -> bool {
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if VIDEO_EXTENSIONS.contains(&extension.as_str()) {
            return true;
        }
        if !SIDECAR_EXTENSIONS.contains(&extension.as_str()) {
            self.issue(DiscoveryIssueKind::IgnoredExtension, path);
        }
        false
    }

    fn files(
        &mut self,
        directory: &Path,
        depth: usize,
        extras_only: bool,
    ) -> (Vec<PathBuf>, Vec<ExtraVideoWork>) {
        let mut episodes = Vec::new();
        let mut extras = Vec::new();
        let mut queue = VecDeque::from([(directory.to_path_buf(), depth)]);
        while let Some((directory, depth)) = queue.pop_front() {
            for path in self.entries(&directory, depth) {
                if self.deadline_reached() {
                    return (episodes, extras);
                }
                let Ok(metadata) = path.metadata() else {
                    self.issue(DiscoveryIssueKind::UnreadableEntry, &path);
                    continue;
                };
                if metadata.is_dir() {
                    queue.push_back((path, depth + 1));
                } else if metadata.is_file() && self.video(&path) {
                    if let Some(extra) = classify_extra_video(self.root, &path) {
                        extras.push(ExtraVideoWork { path, extra });
                    } else if !extras_only {
                        episodes.push(path);
                    }
                }
            }
        }
        episodes.sort();
        extras.sort_by(|left, right| left.path.cmp(&right.path));
        (episodes, extras)
    }

    fn show(&mut self, folder: PathBuf) -> Option<ShowWork> {
        let unreadable_before = self
            .report
            .issue_counts
            .get(&DiscoveryIssueKind::UnreadableEntry)
            .copied()
            .unwrap_or(0);
        let mut seasons = Vec::new();
        let mut extra_files = Vec::new();
        for child in self.entries(&folder, 1) {
            if self.deadline_reached() {
                break;
            }
            let Ok(metadata) = child.metadata() else {
                self.issue(DiscoveryIssueKind::UnreadableEntry, &child);
                continue;
            };
            if metadata.is_file() && self.video(&child) {
                if let Some(extra) = classify_extra_video(self.root, &child) {
                    extra_files.push(ExtraVideoWork { path: child, extra });
                } else {
                    self.issue(DiscoveryIssueKind::UnsupportedNesting, &child);
                }
            } else if metadata.is_dir() {
                if child
                    .file_name()
                    .and_then(|name| name.to_str())
                    .and_then(classify_extra_directory_name)
                    .is_some()
                {
                    extra_files.extend(self.files(&child, 2, true).1);
                } else if let Some(season_number) = parse_season_number_for_folder(&child) {
                    let (episode_files, extra_files) = self.files(&child, 2, false);
                    if !episode_files.is_empty() || !extra_files.is_empty() {
                        seasons.push(SeasonWork {
                            folder: child,
                            season_number,
                            episode_files,
                            extra_files,
                        });
                    }
                } else {
                    self.issue(DiscoveryIssueKind::UnsupportedNesting, &child);
                }
            }
        }
        if seasons.is_empty() && extra_files.is_empty() {
            if !self.report.truncated
                && !self
                    .report
                    .issue_counts
                    .contains_key(&DiscoveryIssueKind::DepthLimit)
                && self
                    .report
                    .issue_counts
                    .get(&DiscoveryIssueKind::UnreadableEntry)
                    .copied()
                    .unwrap_or(0)
                    == unreadable_before
            {
                self.issue(DiscoveryIssueKind::EmptyShow, &folder);
            }
            return None;
        }
        self.report.shows += 1;
        self.report.seasons += seasons.len();
        self.report.episodes += seasons
            .iter()
            .map(|season| season.episode_files.len())
            .sum::<usize>();
        self.report.extras += extra_files.len()
            + seasons
                .iter()
                .map(|season| season.extra_files.len())
                .sum::<usize>();
        Some(ShowWork {
            folder,
            seasons,
            extra_files,
        })
    }
}

fn discover(root: &Path, limits: Limits) -> (Vec<ShowWork>, TelevisionDiscoveryReport) {
    let mut discovery = Discovery {
        root,
        limits,
        started: Instant::now(),
        visited: BTreeSet::new(),
        report: TelevisionDiscoveryReport::default(),
    };
    let mut shows = Vec::new();
    for path in discovery.entries(root, 0) {
        if discovery.deadline_reached() {
            break;
        }
        match path.metadata() {
            Ok(metadata) if metadata.is_dir() => {
                if let Some(show) = discovery.show(path) {
                    shows.push(show);
                }
            }
            Ok(metadata) if metadata.is_file() => {
                if discovery.video(&path) {
                    discovery.issue(DiscoveryIssueKind::UnsupportedNesting, &path);
                }
            }
            Err(_) => discovery.issue(DiscoveryIssueKind::UnreadableEntry, &path),
            _ => {}
        }
    }
    discovery.report.truncated |= discovery
        .report
        .issue_counts
        .contains_key(&DiscoveryIssueKind::DepthLimit);
    (shows, discovery.report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(root: &Path, name: &str) {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"discovery does not decode or modify these bytes").unwrap();
    }

    #[test]
    fn television_discovery_preview_matches_scan_candidates_and_preserves_sidecars() {
        let temp = tempfile::tempdir().unwrap();
        for extension in VIDEO_EXTENSIONS {
            file(temp.path(), &format!("Show/Season 01/episode.{extension}"));
        }
        file(temp.path(), "Show/Season 01/uppercase.MPEG");
        file(temp.path(), "Show/Season 01/Disc 2/episode.mpg");
        file(temp.path(), "Show/Season 01/Trailers/clip.ts");
        file(temp.path(), "Show/Featurettes/clip.m2ts");
        for extension in SIDECAR_EXTENSIONS {
            file(temp.path(), &format!("Show/Season 01/episode.{extension}"));
        }
        let before = std::fs::read(temp.path().join("Show/Season 01/episode.srt")).unwrap();
        let (shows, report) = discover(temp.path(), Limits::SCAN);
        assert_eq!(preview_television_folder(temp.path()), report);
        assert_eq!(shows.len(), 1);
        assert_eq!(report.episodes, VIDEO_EXTENSIONS.len() + 2);
        assert_eq!(report.extras, 2);
        assert_eq!(report.seasons, 1);
        assert!(!report.truncated);
        assert!(report.issue_counts.is_empty(), "{report:?}");
        assert_eq!(
            std::fs::read(temp.path().join("Show/Season 01/episode.srt")).unwrap(),
            before
        );
        assert_eq!(preview_television_folder(temp.path()), report);
    }

    #[test]
    fn television_discovery_reports_layout_and_extensions_without_promoting_packages() {
        let temp = tempfile::tempdir().unwrap();
        file(temp.path(), "Package/Show/Season 1/episode.mp4");
        file(temp.path(), "root-episode.mpg");
        file(temp.path(), "EmptyShow/tvshow.nfo");
        file(temp.path(), "Show/loose-episode.mp4");
        file(temp.path(), "Show/Season 1/episode.mpeg");
        file(temp.path(), "Show/Season 1/unknown.unsupported");
        file(temp.path(), "Show/Season 1/source.bin");
        file(temp.path(), "Show/Season 1/._metadata.mpeg");
        let report = preview_television_folder(temp.path());
        assert_eq!(report.episodes, 1);
        assert_eq!(report.shows, 1);
        assert_eq!(report.hidden_entries, 1);
        assert_eq!(
            report.issue_counts[&DiscoveryIssueKind::UnsupportedNesting],
            3
        );
        assert_eq!(report.issue_counts[&DiscoveryIssueKind::EmptyShow], 2);
        assert_eq!(
            report.issue_counts[&DiscoveryIssueKind::IgnoredExtension],
            2
        );
        assert!(report
            .examples
            .iter()
            .all(|issue| !Path::new(&issue.path).is_absolute()));
        assert!(report
            .examples
            .iter()
            .any(|issue| issue.path.ends_with(".unsupported")));
    }

    #[test]
    fn television_discovery_bounds_entries_depth_time_and_samples() {
        let temp = tempfile::tempdir().unwrap();
        for index in 0..100 {
            file(temp.path(), &format!("Show/Season 1/{index}.unknown"));
        }
        let full = preview_television_folder(temp.path());
        assert_eq!(
            full.issue_counts[&DiscoveryIssueKind::IgnoredExtension],
            100
        );
        assert_eq!(full.examples.len(), MAX_EXAMPLES);
        let limited = discover(
            temp.path(),
            Limits {
                entries: 12,
                ..Limits::PREVIEW
            },
        )
        .1;
        assert_eq!(limited.inspected_entries, 12);
        assert!(limited.truncated);
        let expired = discover(
            temp.path(),
            Limits {
                elapsed: Some(Duration::ZERO),
                ..Limits::PREVIEW
            },
        )
        .1;
        assert!(expired.truncated);
        assert_eq!(expired.inspected_entries, 0);
        let shallow = discover(
            temp.path(),
            Limits {
                depth: 1,
                ..Limits::PREVIEW
            },
        )
        .1;
        assert!(shallow.truncated);
        assert_eq!(shallow.issue_counts[&DiscoveryIssueKind::DepthLimit], 1);
        assert_eq!(shallow.episodes, 0);
    }

    #[test]
    fn television_discovery_missing_root_is_an_error_not_an_empty_success() {
        let temp = tempfile::tempdir().unwrap();
        let report = preview_television_folder(&temp.path().join("missing"));
        assert_eq!(report.issue_counts[&DiscoveryIssueKind::UnreadableEntry], 1);
        assert_eq!(report.shows, 0);
    }

    // Unix provides unprivileged symlink creation; Windows needs developer mode
    // or a privilege grant. The canonical-directory cycle guard is portable.
    #[cfg(unix)]
    #[test]
    fn television_discovery_directory_cycles_do_not_duplicate_episodes() {
        let temp = tempfile::tempdir().unwrap();
        file(temp.path(), "Show/Season 1/episode.ts");
        std::os::unix::fs::symlink(temp.path(), temp.path().join("Show/Season 1/cycle")).unwrap();
        let report = preview_television_folder(temp.path());
        assert_eq!(report.episodes, 1);
        assert_eq!(
            report.issue_counts[&DiscoveryIssueKind::RepeatedDirectory],
            1
        );
        assert!(!report.truncated);
    }
}
