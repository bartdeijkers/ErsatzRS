//! Application commands, queries, and cross-adapter orchestration.
//!
//! This crate is storage- and transport-neutral. Concrete use-case services
//! will be added to this container as complete vertical slices move out of the
//! web adapter.

/// Native application services assembled by the composition root.
///
/// The container is intentionally empty until the first vertical slice lands;
/// defining it here gives each adapter one stable application entry point
/// without inventing placeholder use cases.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct ApplicationServices;

impl ApplicationServices {
    /// Creates the application-service container.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use super::ApplicationServices;

    #[test]
    fn application_services_has_a_stable_empty_starting_point() {
        let services = ApplicationServices::new();
        assert_eq!(format!("{services:?}"), "ApplicationServices");
    }
}
