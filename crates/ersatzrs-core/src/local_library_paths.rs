//! Local path rules from ErsatzTV.Core/Libraries/LocalLibraryPaths.cs.
//!
//! These are filesystem paths, never URLs. Comparison is lexical and case
//! insensitive to preserve the upstream library-overlap contract on every OS.

use std::path::{Component, Path, PathBuf};

/// Whether a path is fully qualified for the host platform.
#[must_use]
pub fn is_fully_qualified(path: &str) -> bool {
    let Some(first) = path.chars().next() else {
        return false;
    };
    if !(matches!(first, '/' | '\\') || first.is_ascii_alphabetic())
        || path.contains('\0')
        || !Path::new(path).is_absolute()
    {
        return false;
    }
    // Windows requires a share as well as a server for UNC paths. On Unix,
    // absolute roots (including //) use the native Path::is_absolute rule.
    #[cfg(windows)]
    if path.starts_with(['/', '\\']) && path.chars().nth(1).is_some_and(|c| matches!(c, '/' | '\\'))
    {
        return path
            .split(['/', '\\'])
            .filter(|part| !part.is_empty())
            .count()
            >= 2;
    }
    true
}

/// Normalize dot segments without interpreting percent escapes or resolving
/// symlinks. Invalid legacy values return None rather than breaking other saves.
#[must_use]
pub fn normalize(path: &str) -> Option<PathBuf> {
    if path.trim().is_empty() || path.contains('\0') {
        return None;
    }
    let absolute = std::path::absolute(path).ok()?;
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    Some(normalized)
}

/// Stable comparison key, including for malformed paths in older databases.
#[must_use]
pub fn comparison_key(path: &str) -> Option<String> {
    normalize(path).map(|path| {
        let case_mapper = icu_casemap::CaseMapper::new();
        path.to_string_lossy()
            .trim_end_matches(std::path::MAIN_SEPARATOR)
            .chars()
            .map(|character| case_mapper.simple_uppercase(character))
            .collect()
    })
}

/// Whether two local-library roots overlap. Media kinds use persisted values;
/// Images (6) and Other Videos (4) may intentionally share a root.
#[must_use]
pub fn conflicts(left_kind: i32, left: &str, right_kind: i32, right: &str) -> bool {
    if matches!((left_kind, right_kind), (4, 6) | (6, 4)) {
        return false;
    }
    let (Some(left), Some(right)) = (comparison_key(left), comparison_key(right)) else {
        return false;
    };
    let left = format!("{left}{}", std::path::MAIN_SEPARATOR);
    let right = format!("{right}{}", std::path::MAIN_SEPARATOR);
    left.starts_with(&right) || right.starts_with(&left)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_library_paths_reject_relative_and_disguised_inputs() {
        for path in [
            "",
            " ",
            "media/movies",
            "./media",
            "../media",
            "~/media",
            ".",
            " /media",
            "\u{a0}/media",
            "\u{200b}/media",
            "\u{feff}/media",
            "\"/media\"",
            "'/media'",
            "file:///media",
            "smb://server/share",
        ] {
            assert!(!is_fully_qualified(path), "{path:?}");
            let _ = comparison_key(path);
            let _ = conflicts(1, path, 1, "media");
        }
    }

    // Native Unix and Windows cases intentionally have separate counterparts.
    #[cfg(unix)]
    #[test]
    fn local_library_paths_unix_qualification_and_normalization() {
        for path in [
            "/",
            "/media",
            "/media/movies/",
            "/media/100%20Movies",
            "/media/tv/../movies",
        ] {
            assert!(is_fully_qualified(path));
        }
        assert_eq!(
            comparison_key("/media/tv/../Movies/"),
            Some("/MEDIA/MOVIES".into())
        );
        assert_ne!(
            comparison_key("/media/100%20Movies"),
            comparison_key("/media/100 Movies")
        );
        assert_ne!(
            comparison_key("/media/Straße"),
            comparison_key("/media/STRASSE")
        );
        assert_eq!(
            comparison_key("/media/Ähnliche"),
            comparison_key("/media/ähnliche")
        );
        assert!(conflicts(1, "/", 1, "/media"));
        assert!(conflicts(1, "/media/tv", 2, "/media//TV/kids"));
        assert!(!conflicts(1, "/media/tv", 1, "/media/tv2"));
        assert!(!conflicts(4, "/media", 6, "/media/child"));
        assert!(!conflicts(6, "/media/child", 4, "/media"));
    }

    #[cfg(windows)]
    #[test]
    fn local_library_paths_windows_qualification_and_normalization() {
        for path in [
            r"C:\",
            r"C:\Media",
            "C:/Media",
            r"\\nas\share",
            r"\\?\C:\Media",
            r"\\?\UNC\nas\share\Media",
        ] {
            assert!(is_fully_qualified(path), "{path}");
        }
        for path in [r"\Media", "/media", "C:Media", r"\\nas"] {
            assert!(!is_fully_qualified(path), "{path}");
        }
        assert_eq!(
            comparison_key(r"C:\Media\TV"),
            comparison_key("c:/media/movies/../TV/")
        );
        assert!(conflicts(1, r"\\nas\share\TV", 2, "//NAS/share/tv/kids"));
        assert!(!conflicts(1, r"C:\Media\TV", 1, r"D:\Media\TV"));
        assert!(!conflicts(1, r"C:\Media\TV", 1, r"C:\Media\TV2"));
        assert!(!conflicts(4, r"C:\Media", 6, r"C:\Media\child"));
    }
}
