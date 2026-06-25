use std::path::{Path, PathBuf};

use crate::normalize_sub_kind_token;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtraParentHint {
    Sibling {
        folder: PathBuf,
        primary_stem: String,
    },
    Folder {
        owner_folder: PathBuf,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtraVideoClassification {
    pub sub_kind: String,
    pub parent_hint: ExtraParentHint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlternateVersionClassification {
    pub base_title: String,
    pub sub_kind: Option<String>,
    pub canonical_hint: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VersionToken {
    sub_kind: String,
    canonical_hint: bool,
}

#[must_use]
pub fn classify_extra_video(library_root: &Path, path: &Path) -> Option<ExtraVideoClassification> {
    let stem = path.file_stem()?.to_str()?;
    if let Some((primary_stem, sub_kind)) = classify_extra_file_stem(stem) {
        return Some(ExtraVideoClassification {
            sub_kind,
            parent_hint: ExtraParentHint::Sibling {
                folder: path.parent().unwrap_or(library_root).to_path_buf(),
                primary_stem,
            },
        });
    }

    let relative_parent = path.parent()?.strip_prefix(library_root).ok()?;
    let mut owner = library_root.to_path_buf();
    for component in relative_parent.components() {
        let component_text = component.as_os_str().to_string_lossy();
        if let Some(sub_kind) = classify_extra_directory_name(&component_text) {
            return Some(ExtraVideoClassification {
                sub_kind,
                parent_hint: ExtraParentHint::Folder {
                    owner_folder: owner,
                },
            });
        }
        owner.push(component.as_os_str());
    }

    None
}

#[must_use]
pub fn classify_extra_video_path(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    if let Some((_, sub_kind)) = classify_extra_file_stem(stem) {
        return Some(sub_kind);
    }

    path.parent()?.components().find_map(|component| {
        component
            .as_os_str()
            .to_str()
            .and_then(classify_extra_directory_name)
    })
}

#[must_use]
pub fn classify_extra_file_stem(stem: &str) -> Option<(String, String)> {
    const EXTRA_FILE_SUFFIXES: &[&str] = &[
        "behindthescenes",
        "deleted",
        "featurette",
        "interview",
        "scene",
        "short",
        "trailer",
        "other",
    ];

    let lower = stem.to_ascii_lowercase();
    EXTRA_FILE_SUFFIXES.iter().find_map(|suffix| {
        if !lower.ends_with(suffix) {
            return None;
        }
        let primary = stem[..stem.len().saturating_sub(suffix.len())]
            .trim_end_matches([' ', '.', '-', '_'])
            .trim()
            .to_owned();
        if primary.is_empty() {
            return None;
        }
        normalize_sub_kind_token(suffix).map(|sub_kind| (primary, sub_kind))
    })
}

#[must_use]
pub fn classify_extra_directory_name(name: &str) -> Option<String> {
    match name.to_ascii_lowercase().as_str() {
        "behind the scenes" => normalize_sub_kind_token("behind_the_scenes"),
        "deleted scenes" => normalize_sub_kind_token("deleted_scene"),
        "featurettes" => normalize_sub_kind_token("featurette"),
        "interviews" => normalize_sub_kind_token("interview"),
        "scenes" => normalize_sub_kind_token("scene"),
        "shorts" => normalize_sub_kind_token("short"),
        "trailers" => normalize_sub_kind_token("trailer"),
        "other" | "extras" | "specials" => normalize_sub_kind_token("other"),
        _ => None,
    }
}

#[must_use]
pub fn classify_alternate_version(
    library_root: &Path,
    title: &str,
    path: Option<&Path>,
) -> AlternateVersionClassification {
    let title_detection = detect_version_token(title);
    let path_detection =
        path.and_then(|path| detect_version_token_for_relative_path(library_root, path));
    let detection = title_detection.clone().or(path_detection);

    let base_source = if title.trim().is_empty() {
        path.and_then(Path::file_stem)
            .and_then(|stem| stem.to_str())
            .unwrap_or_default()
    } else {
        title
    };
    let base_title = if title_detection.is_some() || title.trim().is_empty() {
        strip_version_tokens(base_source)
    } else {
        base_source.trim().to_owned()
    };

    AlternateVersionClassification {
        base_title,
        sub_kind: detection
            .as_ref()
            .and_then(|token| (!token.canonical_hint).then(|| token.sub_kind.clone())),
        canonical_hint: detection.is_none_or(|token| token.canonical_hint),
    }
}

#[must_use]
pub fn normalize_group_key(value: &str) -> String {
    normalized_words(value)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn detect_version_token_for_relative_path(
    library_root: &Path,
    path: &Path,
) -> Option<VersionToken> {
    let parent = path.parent()?;
    let relative = parent.strip_prefix(library_root).unwrap_or(parent);
    relative.components().find_map(|component| {
        component
            .as_os_str()
            .to_str()
            .and_then(detect_folder_version_token)
    })
}

fn detect_version_token(value: &str) -> Option<VersionToken> {
    let normalized = normalized_words(value);
    detect_alternate_token(&normalized)
        .or_else(|| detect_canonical_token(&normalized))
        .filter(|_| title_has_version_marker(value, &normalized))
}

fn detect_folder_version_token(value: &str) -> Option<VersionToken> {
    let normalized = normalized_words(value);
    detect_alternate_token(&normalized).or_else(|| detect_canonical_token(&normalized))
}

fn detect_alternate_token(normalized: &str) -> Option<VersionToken> {
    [
        ("unplugged", "unplugged"),
        ("acoustic", "acoustic"),
        ("director s cut", "directors_cut"),
        ("directors cut", "directors_cut"),
        ("director cut", "directors_cut"),
        ("extended mix", "extended"),
        ("extended version", "extended"),
        ("extended", "extended"),
        ("remastered", "remaster"),
        ("remaster", "remaster"),
        ("remix", "remix"),
        ("live", "live"),
    ]
    .into_iter()
    .find_map(|(phrase, sub_kind)| {
        contains_word_phrase(normalized, phrase).then(|| VersionToken {
            sub_kind: normalize_sub_kind_token(sub_kind).unwrap_or_else(|| sub_kind.to_owned()),
            canonical_hint: false,
        })
    })
}

fn detect_canonical_token(normalized: &str) -> Option<VersionToken> {
    [
        ("official music video", "official_video"),
        ("official video", "official_video"),
        ("studio version", "studio"),
        ("studio", "studio"),
    ]
    .into_iter()
    .find_map(|(phrase, sub_kind)| {
        contains_word_phrase(normalized, phrase).then(|| VersionToken {
            sub_kind: normalize_sub_kind_token(sub_kind).unwrap_or_else(|| sub_kind.to_owned()),
            canonical_hint: true,
        })
    })
}

fn title_has_version_marker(original: &str, normalized: &str) -> bool {
    let trimmed = original.trim();
    let has_group_marker = (trimmed.contains('(') && trimmed.contains(')'))
        || (trimmed.contains('[') && trimmed.contains(']'))
        || (trimmed.contains('{') && trimmed.contains('}'));
    let has_separator = [" - ", " – ", " — ", "_", " / "]
        .iter()
        .any(|separator| trimmed.contains(separator));
    let explicit_phrase = [
        "official music video",
        "official video",
        "directors cut",
        "director cut",
        "director s cut",
        "extended version",
        "extended mix",
        "studio version",
    ]
    .iter()
    .any(|phrase| contains_word_phrase(normalized, phrase));

    has_group_marker || has_separator || explicit_phrase
}

fn strip_version_tokens(value: &str) -> String {
    let normalized = normalized_words(value);
    let words = normalized.split_whitespace().collect::<Vec<_>>();
    let phrases = [
        "official music video",
        "official video",
        "director s cut",
        "directors cut",
        "director cut",
        "extended version",
        "extended mix",
        "studio version",
        "remastered",
        "remaster",
        "remix",
        "unplugged",
        "acoustic",
        "extended",
        "studio",
        "live",
    ]
    .map(normalized_words);

    let mut kept = Vec::new();
    let mut index = 0usize;
    while index < words.len() {
        if let Some(match_len) = phrases
            .iter()
            .map(|phrase| phrase.split_whitespace().collect::<Vec<_>>())
            .find_map(|phrase_words| {
                words[index..]
                    .starts_with(&phrase_words)
                    .then_some(phrase_words.len())
            })
        {
            index += match_len;
        } else {
            kept.push(words[index]);
            index += 1;
        }
    }

    if kept.is_empty() {
        value.trim().to_owned()
    } else {
        kept.join(" ")
    }
}

fn contains_word_phrase(normalized: &str, phrase: &str) -> bool {
    let haystack = format!(" {normalized} ");
    let needle = format!(" {} ", normalized_words(phrase));
    haystack.contains(&needle)
}

fn normalized_words(value: &str) -> String {
    let mut out = String::new();
    let mut last_was_space = true;
    for ch in value.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            last_was_space = false;
        } else if !last_was_space {
            out.push(' ');
            last_was_space = true;
        }
    }
    out.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_extra_suffixes_and_directories() {
        let suffix = classify_extra_video(
            Path::new("/library"),
            Path::new("/library/Movie/Movie-featurette.mkv"),
        )
        .expect("featurette suffix");
        assert_eq!(suffix.sub_kind, "featurette");

        let directory = classify_extra_video(
            Path::new("/library"),
            Path::new("/library/Movie/Featurettes/Making Of.mkv"),
        )
        .expect("featurette directory");
        assert_eq!(directory.sub_kind, "featurette");
        assert_eq!(
            classify_extra_directory_name("Deleted Scenes").as_deref(),
            Some("deleted_scene")
        );
        assert!(
            classify_extra_video(Path::new("/library"), Path::new("/library/Movie.mkv")).is_none()
        );
    }

    #[test]
    fn classifies_alternate_version_tokens_without_rewriting_unmarked_titles() {
        let live = classify_alternate_version(Path::new("/library"), "Song (Live)", None);
        assert_eq!(live.base_title, "song");
        assert_eq!(live.sub_kind.as_deref(), Some("live"));

        let folder = classify_alternate_version(
            Path::new("/library"),
            "Once in a Lifetime",
            Some(Path::new(
                "/library/Unplugged Sessions/Once in a Lifetime.mkv",
            )),
        );
        assert_eq!(folder.base_title, "Once in a Lifetime");
        assert_eq!(folder.sub_kind.as_deref(), Some("unplugged"));

        let official =
            classify_alternate_version(Path::new("/library"), "Song (Official Video)", None);
        assert_eq!(official.base_title, "song");
        assert_eq!(official.sub_kind, None);
        assert!(official.canonical_hint);

        let unmarked = classify_alternate_version(Path::new("/library"), "Live Wire", None);
        assert_eq!(unmarked.base_title, "Live Wire");
        assert_eq!(unmarked.sub_kind, None);
    }
}
