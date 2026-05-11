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
