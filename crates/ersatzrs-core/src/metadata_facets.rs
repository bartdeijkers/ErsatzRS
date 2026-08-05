//! Uniform metadata-facet normalization shared by scanners, search, and the
//! admin read side.

use crate::language_codes::canonical_language_code;

/// The six metadata facets rendered by every supported media detail surface.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MediaFacetSet {
    pub content_ratings: Vec<String>,
    pub studios: Vec<String>,
    pub music_labels: Vec<String>,
    pub languages: Vec<String>,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
}

/// Trim, drop blanks, and deduplicate values case-insensitively while
/// preserving first-source order.
#[must_use]
pub fn normalize_facet_values<I, S>(values: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut result = Vec::new();
    for value in values {
        let value = value.as_ref().trim();
        if !value.is_empty()
            && !result
                .iter()
                .any(|existing: &String| existing.eq_ignore_ascii_case(value))
        {
            result.push(value.to_owned());
        }
    }
    result
}

/// Normalize `/`-separated content-rating strings without interpreting the
/// rating system or value.
#[must_use]
pub fn normalize_content_ratings<I, S>(values: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    normalize_facet_values(values.into_iter().flat_map(|value| {
        value
            .as_ref()
            .split('/')
            .map(str::to_owned)
            .collect::<Vec<_>>()
    }))
}

/// Normalize explicit language values through the canonical ISO service.
#[must_use]
pub fn normalize_language_codes<I, S>(values: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    normalize_facet_values(
        values
            .into_iter()
            .filter_map(|value| canonical_language_code(value.as_ref())),
    )
}

/// Split embedded metadata only on semicolons and NUL separators. Commas are
/// intentionally retained because they commonly occur inside names.
#[must_use]
pub fn split_embedded_values(value: &str) -> Vec<String> {
    normalize_facet_values(value.split([';', '\0']))
}

/// Select the first non-empty normalized source. This encodes local scanner
/// precedence: sidecar, then embedded tags, then fallback metadata.
#[must_use]
pub fn preferred_facet_values(
    sidecar: &[String],
    embedded: &[String],
    fallback: &[String],
) -> Vec<String> {
    for source in [sidecar, embedded, fallback] {
        let values = normalize_facet_values(source);
        if !values.is_empty() {
            return values;
        }
    }
    Vec::new()
}

/// Union lower-precedence values onto an authoritative value list.
#[must_use]
pub fn union_facet_values(primary: &[String], additional: &[String]) -> Vec<String> {
    normalize_facet_values(primary.iter().chain(additional))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_are_trimmed_and_deduplicated_in_first_source_order() {
        assert_eq!(
            normalize_facet_values(["  Alpha ", "beta", "ALPHA", ""]),
            ["Alpha", "beta"]
        );
    }

    #[test]
    fn ratings_keep_arbitrary_values_and_split_slashes() {
        assert_eq!(
            normalize_content_ratings([" NL:12 / US:PG-13 ", "nl:12"]),
            ["NL:12", "US:PG-13"]
        );
    }

    #[test]
    fn embedded_values_do_not_split_commas() {
        assert_eq!(
            split_embedded_values("Earth, Wind & Fire; Soul\0Live"),
            ["Earth, Wind & Fire", "Soul", "Live"]
        );
    }

    #[test]
    fn precedence_uses_first_non_empty_source() {
        assert_eq!(
            preferred_facet_values(&["Sidecar".into()], &["Embedded".into()], &[]),
            ["Sidecar"]
        );
        assert_eq!(
            preferred_facet_values(&[], &[" Embedded ".into()], &["Fallback".into()]),
            ["Embedded"]
        );
    }
}
