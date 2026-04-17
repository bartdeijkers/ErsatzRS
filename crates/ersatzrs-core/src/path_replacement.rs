#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathReplacement {
    pub source_path: String,
    pub replacement_path: String,
    pub source_platform: PathPlatform,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathPlatform {
    Unix,
    Windows,
}

#[must_use]
pub fn replace_path(
    replacements: &[PathReplacement],
    path: &str,
    target_platform: PathPlatform,
) -> String {
    let Some(replacement) = replacements
        .iter()
        .find(|replacement| replacement_matches(replacement, path))
    else {
        return path.to_owned();
    };

    let suffix = path
        .get(replacement.source_path.len()..)
        .unwrap_or_default()
        .to_owned();
    let mut final_path = format!("{}{}", replacement.replacement_path, suffix);
    if replacement.source_platform == PathPlatform::Windows && target_platform == PathPlatform::Unix
    {
        final_path = final_path.replace('\\', "/");
    } else if replacement.source_platform == PathPlatform::Unix
        && target_platform == PathPlatform::Windows
    {
        final_path = final_path.replace('/', "\\");
    }

    final_path
}

#[must_use]
pub fn replace_network_path(
    source_platform: PathPlatform,
    path: &str,
    network_path: &str,
    replacement: &str,
) -> String {
    replace_path(
        &[PathReplacement {
            source_path: network_path.to_owned(),
            replacement_path: replacement.to_owned(),
            source_platform,
        }],
        path,
        source_platform,
    )
}

fn replacement_matches(replacement: &PathReplacement, path: &str) -> bool {
    if replacement.source_path.trim().is_empty() {
        return false;
    }

    let separator = match replacement.source_platform {
        PathPlatform::Unix => '/',
        PathPlatform::Windows => '\\',
    };
    let prefix = if replacement.source_path.ends_with(separator) {
        replacement.source_path.clone()
    } else {
        format!("{}{separator}", replacement.source_path)
    };

    starts_with_ignore_ascii_case(path, &prefix)
}

fn starts_with_ignore_ascii_case(value: &str, prefix: &str) -> bool {
    value
        .get(..prefix.len())
        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(prefix))
}

#[cfg(test)]
mod tests {
    use super::{replace_network_path, replace_path, PathPlatform, PathReplacement};

    #[test]
    fn replaces_unix_provider_path_for_unix_target() {
        let result = replace_path(
            &[PathReplacement {
                source_path: "/media/movies".to_owned(),
                replacement_path: "/mnt/movies".to_owned(),
                source_platform: PathPlatform::Unix,
            }],
            "/media/movies/Big Buck Bunny.mkv",
            PathPlatform::Unix,
        );

        assert_eq!(result, "/mnt/movies/Big Buck Bunny.mkv");
    }

    #[test]
    fn replaces_windows_provider_path_for_container_target() {
        let result = replace_path(
            &[PathReplacement {
                source_path: r"D:\Movies".to_owned(),
                replacement_path: "/mnt/movies".to_owned(),
                source_platform: PathPlatform::Windows,
            }],
            r"D:\Movies\Big Buck Bunny.mkv",
            PathPlatform::Unix,
        );

        assert_eq!(result, "/mnt/movies/Big Buck Bunny.mkv");
    }

    #[test]
    fn converts_unix_provider_path_for_windows_target() {
        let result = replace_path(
            &[PathReplacement {
                source_path: "/mnt/movies".to_owned(),
                replacement_path: r"D:\Movies".to_owned(),
                source_platform: PathPlatform::Unix,
            }],
            "/mnt/movies/Big Buck Bunny.mkv",
            PathPlatform::Windows,
        );

        assert_eq!(result, r"D:\Movies\Big Buck Bunny.mkv");
    }

    #[test]
    fn ignores_non_boundary_prefix_matches() {
        let result = replace_path(
            &[PathReplacement {
                source_path: "/media/movie".to_owned(),
                replacement_path: "/mnt/movie".to_owned(),
                source_platform: PathPlatform::Unix,
            }],
            "/media/movies/file.mkv",
            PathPlatform::Unix,
        );

        assert_eq!(result, "/media/movies/file.mkv");
    }

    #[test]
    fn replaces_network_path_against_source_platform() {
        let result = replace_network_path(
            PathPlatform::Windows,
            r"\\server\share\file.mkv",
            r"\\server\share",
            "/mnt/share",
        );

        assert_eq!(result, r"/mnt/share\file.mkv");
    }
}
