//! Dependency-light operating-system integrations.
//!
//! Platform APIs live here when testing them through a large application crate
//! would pull unrelated runtime dependencies into a target-specific test.

pub mod single_instance;
pub mod windows_startup;
