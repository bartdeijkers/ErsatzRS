use chrono::{DateTime, Duration, Utc};

/// Default playout build/reset cadence used by `ErsatzTV` background work.
pub const PLAYOUT_BUILD_INTERVAL: Duration = Duration::minutes(30);
/// Default provider connectivity poll cadence.
pub const PROVIDER_POLL_INTERVAL: Duration = Duration::minutes(5);
/// Default cache cleanup cadence.
pub const CACHE_CLEANUP_INTERVAL: Duration = Duration::hours(24);
/// Default search index dispatcher cadence.
pub const SEARCH_INDEX_REBUILD_INTERVAL: Duration = Duration::hours(24);
/// Default library refresh cadence when `scanner.library_refresh_interval` is unset.
///
/// Matches `ErsatzTV.Application.Configuration.Queries.GetLibraryRefreshIntervalHandler`,
/// which defaults to `0` (refresh disabled). The Rust port therefore returns an
/// empty interval when the config value is missing or zero, and `is_due` always
/// reports `false` for an empty interval — i.e. the scanner is opted in by
/// configuration, not by default.
pub const DEFAULT_LIBRARY_REFRESH_INTERVAL: Duration = Duration::zero();

/// One periodic background loop tracked by the scheduler binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaintenanceLoop {
    /// Stable loop name used in persisted `ConfigElement` timestamps.
    pub name: &'static str,
    /// How often this loop should run.
    pub interval: Duration,
}

impl MaintenanceLoop {
    /// Returns whether this loop is due at `now`.
    ///
    /// A loop with a non-positive interval is considered disabled and is never
    /// due (matches `ErsatzTV`'s `libraryRefreshInterval > 0` gate in
    /// `CallLocalLibraryScannerHandler.ScanIsRequired`).
    #[must_use]
    pub fn is_due(&self, last_run: Option<DateTime<Utc>>, now: DateTime<Utc>) -> bool {
        if self.interval <= Duration::zero() {
            return false;
        }
        last_run.is_none_or(|last_run| now.signed_duration_since(last_run) >= self.interval)
    }
}

/// Runtime-independent loop definitions used by `ersatzrs-scheduler`.
#[must_use]
pub fn maintenance_loops(library_refresh_interval: Duration) -> Vec<MaintenanceLoop> {
    vec![
        MaintenanceLoop {
            name: "playout",
            interval: PLAYOUT_BUILD_INTERVAL,
        },
        MaintenanceLoop {
            name: "library_refresh",
            interval: library_refresh_interval,
        },
        MaintenanceLoop {
            name: "provider_poll",
            interval: PROVIDER_POLL_INTERVAL,
        },
        MaintenanceLoop {
            name: "cache_cleanup",
            interval: CACHE_CLEANUP_INTERVAL,
        },
        MaintenanceLoop {
            name: "search_index",
            interval: SEARCH_INDEX_REBUILD_INTERVAL,
        },
    ]
}

/// Parse `scanner.library_refresh_interval` hours from `ConfigElement`.
///
/// `ErsatzTV` stores this value as an integer hour count, treats `0` as
/// "refresh disabled", and clamps to `[0, 999_999]`. See
/// `CallLocalLibraryScannerHandler.ScanIsRequired` for the canonical
/// implementation.
#[must_use]
pub fn scanner_library_refresh_interval(value: Option<&str>) -> Duration {
    let hours = value
        .and_then(|value| value.trim().parse::<i64>().ok())
        .unwrap_or(0)
        .clamp(0, 999_999);
    if hours <= 0 {
        DEFAULT_LIBRARY_REFRESH_INTERVAL
    } else {
        Duration::hours(hours)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loop_due_calculation_uses_interval() {
        let now = DateTime::parse_from_rfc3339("2026-05-02T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let loop_def = MaintenanceLoop {
            name: "playout",
            interval: Duration::minutes(30),
        };

        assert!(loop_def.is_due(None, now));
        assert!(loop_def.is_due(Some(now - Duration::minutes(30)), now));
        assert!(!loop_def.is_due(Some(now - Duration::minutes(29)), now));
    }

    #[test]
    fn disabled_loop_is_never_due() {
        let now = DateTime::parse_from_rfc3339("2026-05-02T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let disabled = MaintenanceLoop {
            name: "library_refresh",
            interval: Duration::zero(),
        };
        assert!(!disabled.is_due(None, now));
        assert!(!disabled.is_due(Some(now - Duration::days(7)), now));
    }

    #[test]
    fn scanner_interval_uses_hours_and_treats_zero_as_disabled() {
        assert_eq!(
            scanner_library_refresh_interval(Some("6")),
            Duration::hours(6)
        );
        assert_eq!(
            scanner_library_refresh_interval(Some("0")),
            Duration::zero()
        );
        assert_eq!(
            scanner_library_refresh_interval(Some("nope")),
            Duration::zero()
        );
        assert_eq!(scanner_library_refresh_interval(None), Duration::zero());
        assert_eq!(
            scanner_library_refresh_interval(Some("9999999")),
            Duration::hours(999_999)
        );
    }
}
