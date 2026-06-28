use directories::BaseDirs;
use std::ffi::OsString;
use std::path::PathBuf;

/// Legacy Docker/runtime environment variable for the startup database URL.
pub const DATABASE_URL_ENV_VAR: &str = "DATABASE_URL";
/// Repository configuration environment variable for the startup database URL.
pub const ERSATZRS_DATABASE_URL_ENV_VAR: &str = "ErsatzRS_DATABASE__URL";
/// Uppercase variant commonly exported by shells and orchestrators.
pub const ERSATZRS_DATABASE_URL_ENV_VAR_UPPER: &str = "ERSATZRS_DATABASE__URL";

/// Startup configuration needed before the database-backed settings table is
/// available.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupConfig {
    pub database_url: Option<OsString>,
    pub home_dir: Option<PathBuf>,
}

impl StartupConfig {
    /// Load startup configuration from the repository configuration boundary.
    #[must_use]
    pub fn load() -> Self {
        Self {
            database_url: startup_database_url(),
            home_dir: BaseDirs::new().map(|base_dirs| base_dirs.home_dir().to_path_buf()),
        }
    }
}

#[must_use]
pub fn startup_database_url_from_vars(
    variables: impl IntoIterator<Item = (&'static str, Option<OsString>)>,
) -> Option<OsString> {
    variables
        .into_iter()
        .find_map(|(_key, value)| value.filter(|value| !value.is_empty()))
}

fn startup_database_url() -> Option<OsString> {
    startup_database_url_from_vars([
        (
            ERSATZRS_DATABASE_URL_ENV_VAR,
            environment_var(ERSATZRS_DATABASE_URL_ENV_VAR),
        ),
        (
            ERSATZRS_DATABASE_URL_ENV_VAR_UPPER,
            environment_var(ERSATZRS_DATABASE_URL_ENV_VAR_UPPER),
        ),
        (DATABASE_URL_ENV_VAR, environment_var(DATABASE_URL_ENV_VAR)),
    ])
}

#[allow(clippy::disallowed_methods)]
fn environment_var(key: &str) -> Option<OsString> {
    std::env::var_os(key)
}

#[cfg(test)]
mod tests {
    use super::{
        startup_database_url_from_vars, DATABASE_URL_ENV_VAR, ERSATZRS_DATABASE_URL_ENV_VAR,
        ERSATZRS_DATABASE_URL_ENV_VAR_UPPER,
    };
    use std::ffi::OsString;

    #[test]
    fn startup_database_url_prefers_repository_key() {
        let resolved = startup_database_url_from_vars([
            (
                ERSATZRS_DATABASE_URL_ENV_VAR,
                Some(OsString::from("sqlite:/config.db")),
            ),
            (
                ERSATZRS_DATABASE_URL_ENV_VAR_UPPER,
                Some(OsString::from("sqlite:/upper.db")),
            ),
            (
                DATABASE_URL_ENV_VAR,
                Some(OsString::from("sqlite:/docker.db")),
            ),
        ]);

        assert_eq!(resolved, Some(OsString::from("sqlite:/config.db")));
    }

    #[test]
    fn startup_database_url_keeps_docker_legacy_fallback() {
        let resolved = startup_database_url_from_vars([
            (ERSATZRS_DATABASE_URL_ENV_VAR, None),
            (ERSATZRS_DATABASE_URL_ENV_VAR_UPPER, None),
            (
                DATABASE_URL_ENV_VAR,
                Some(OsString::from("sqlite:///data/.ersatzrs/ErsatzRS.db")),
            ),
        ]);

        assert_eq!(
            resolved,
            Some(OsString::from("sqlite:///data/.ersatzrs/ErsatzRS.db"))
        );
    }

    #[test]
    fn startup_database_url_ignores_empty_values() {
        let resolved = startup_database_url_from_vars([
            (ERSATZRS_DATABASE_URL_ENV_VAR, Some(OsString::new())),
            (
                ERSATZRS_DATABASE_URL_ENV_VAR_UPPER,
                Some(OsString::from("sqlite:/upper.db")),
            ),
            (
                DATABASE_URL_ENV_VAR,
                Some(OsString::from("sqlite:/docker.db")),
            ),
        ]);

        assert_eq!(resolved, Some(OsString::from("sqlite:/upper.db")));
    }
}
