//! ISO-639 language-code expansion used by media metadata projections.
//!
//! Ports `ErsatzTV.Core/Metadata/LanguageCodeService.cs` and
//! `ErsatzTV.Infrastructure/Metadata/LanguageCodeCache.cs`. ErsatzTV maps
//! media stream language tags through its embedded ISO-639-2 text resource and
//! then uses .NET `CultureInfo.EnglishName` at the UI edge. ErsatzRS keeps the
//! same embedded ISO resource but reads the English-name column directly, so
//! the display name is data-driven and identical across supported platforms.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

static LANGUAGE_DATA: &str = include_str!("../../../assets/iso-639-2_utf-8.txt");
static LANGUAGE_CODES: OnceLock<LanguageCodeCache> = OnceLock::new();

#[derive(Debug)]
struct LanguageCodeCache {
    code_to_group: HashMap<String, Vec<String>>,
    english_name_by_code: HashMap<String, String>,
}

/// Expand media stream language codes to all equivalent ISO-639 codes.
///
/// Blank codes are ignored. Unknown non-blank codes are preserved to match
/// ErsatzTV's `LanguageCodeService.GetAllLanguageCodes(List<string>)`, which
/// seeds the result set with the input codes before unioning known groups.
#[must_use]
pub fn all_language_codes<I, S>(codes: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let cache = cache();
    let mut result = HashSet::new();

    for code in codes {
        let code = normalize_code(code.as_ref());
        if code.is_empty() {
            continue;
        }

        result.insert(code.clone());
        if let Some(group) = cache.code_to_group.get(&code) {
            result.extend(group.iter().cloned());
        }
    }

    let mut result: Vec<String> = result.into_iter().collect();
    result.sort();
    result
}

/// Resolve an ISO-639 code to its English language name.
#[must_use]
pub fn english_name(code: &str) -> Option<String> {
    cache()
        .english_name_by_code
        .get(&normalize_code(code))
        .cloned()
}

fn cache() -> &'static LanguageCodeCache {
    LANGUAGE_CODES.get_or_init(|| LanguageCodeCache::parse(LANGUAGE_DATA))
}

fn normalize_code(code: &str) -> String {
    code.trim()
        .trim_start_matches('\u{feff}')
        .to_ascii_lowercase()
}

impl LanguageCodeCache {
    fn parse(input: &str) -> Self {
        let mut code_to_group = HashMap::new();
        let mut english_name_by_code = HashMap::new();

        for line in input.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let split: Vec<&str> = line.split('|').collect();
            if split.len() != 5 {
                continue;
            }

            let english_name = split[3].split(';').next().unwrap_or_default().trim();
            let mut group = Vec::new();
            for raw_code in [split[0], split[1], split[2]] {
                let code = normalize_code(raw_code);
                if !code.is_empty() && !group.contains(&code) {
                    group.push(code);
                }
            }

            if group.is_empty() {
                continue;
            }

            for code in &group {
                if !english_name.is_empty() {
                    english_name_by_code.insert(code.clone(), english_name.to_string());
                }
                code_to_group.insert(code.clone(), group.clone());
            }
        }

        Self {
            code_to_group,
            english_name_by_code,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_three_and_two_letter_codes_to_shared_group() {
        assert_eq!(
            all_language_codes(["eng"]),
            vec!["en".to_string(), "eng".to_string()]
        );
        assert_eq!(
            all_language_codes(["en"]),
            vec!["en".to_string(), "eng".to_string()]
        );
    }

    #[test]
    fn resolves_english_names_from_vendored_iso_resource() {
        assert_eq!(english_name("nld").as_deref(), Some("Dutch"));
        assert_eq!(english_name("dut").as_deref(), Some("Dutch"));
        assert_eq!(english_name("nl").as_deref(), Some("Dutch"));
    }

    #[test]
    fn preserves_unknown_codes_for_service_parity() {
        assert_eq!(all_language_codes(["  zzx  "]), vec!["zzx".to_string()]);
        assert_eq!(english_name("zzx"), None);
    }
}
