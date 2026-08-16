//! Parsing and normalization for Schatkamer source URLs.
//!
//! Keeping this provider-specific grammar here gives every host entry point one
//! authoritative interpretation while leaving the general media-list domain
//! free of website details.

use std::collections::BTreeMap;

use thiserror::Error;
use url::Url;

pub const ADDON_ID: &str = "org.ersatzrs.addon.beeldengeluid";
const HOST: &str = "schatkamer.beeldengeluid.nl";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchatkamerUrl {
    pub canonical_url: String,
    pub source: SchatkamerSource,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SchatkamerSource {
    PersonalList {
        id: String,
    },
    Series {
        id: String,
        slug: String,
    },
    Video {
        series_id: String,
        series_slug: String,
        video_id: String,
    },
    Search(Box<SearchParameters>),
}

impl SchatkamerSource {
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::PersonalList { .. } => "personal-list",
            Self::Series { .. } => "series",
            Self::Video { .. } => "video",
            Self::Search(_) => "search",
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SearchParameters {
    pub terms: Vec<String>,
    pub sorting: Vec<String>,
    pub media_types: Vec<String>,
    pub dates: Vec<String>,
    pub date_from: Vec<String>,
    pub date_to: Vec<String>,
    pub broadcasters: Vec<String>,
    pub collections: Vec<String>,
    pub genres: Vec<String>,
    pub people: Vec<String>,
    pub subjects: Vec<String>,
    pub unknown: BTreeMap<String, Vec<String>>,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum SchatkamerUrlError {
    #[error("the source must be an absolute HTTP or HTTPS URL")]
    InvalidUrl,
    #[error("only schatkamer.beeldengeluid.nl URLs are supported")]
    UnsupportedHost,
    #[error("this Schatkamer URL type is not supported")]
    UnsupportedPath,
    #[error("the Schatkamer URL contains an invalid identifier")]
    InvalidIdentifier,
}

/// Parse and normalize one supported Schatkamer hyperlink.
///
/// HTTP is upgraded to HTTPS, fragments and default ports are removed, and
/// URL-encoded search parameters are decoded by `url::Url` without losing
/// repeated filter values.
///
/// # Errors
///
/// Returns an error for malformed URLs, another host, unsupported paths, or
/// structurally invalid provider identifiers.
pub fn parse_schatkamer_url(input: &str) -> Result<SchatkamerUrl, SchatkamerUrlError> {
    let mut url = Url::parse(input.trim()).map_err(|_| SchatkamerUrlError::InvalidUrl)?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(SchatkamerUrlError::InvalidUrl);
    }
    if !url
        .host_str()
        .is_some_and(|host| host.eq_ignore_ascii_case(HOST))
    {
        return Err(SchatkamerUrlError::UnsupportedHost);
    }
    url.set_scheme("https")
        .map_err(|()| SchatkamerUrlError::InvalidUrl)?;
    url.set_port(None)
        .map_err(|()| SchatkamerUrlError::InvalidUrl)?;
    url.set_fragment(None);

    let segments = url
        .path_segments()
        .ok_or(SchatkamerUrlError::UnsupportedPath)?
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    let source = match segments.as_slice() {
        ["lijst", id] => {
            if !is_uuid(id) {
                return Err(SchatkamerUrlError::InvalidIdentifier);
            }
            SchatkamerSource::PersonalList {
                id: (*id).to_owned(),
            }
        }
        [kind, id, slug] if matches!(*kind, "serie" | "programma") => {
            validate_numeric_id(id)?;
            SchatkamerSource::Series {
                id: (*id).to_owned(),
                slug: (*slug).to_owned(),
            }
        }
        ["serie", series_id, series_slug, "aflevering", video_id] => {
            validate_numeric_id(series_id)?;
            validate_numeric_id(video_id)?;
            SchatkamerSource::Video {
                series_id: (*series_id).to_owned(),
                series_slug: (*series_slug).to_owned(),
                video_id: (*video_id).to_owned(),
            }
        }
        ["zoeken"] => SchatkamerSource::Search(Box::new(parse_search_parameters(&url))),
        ["zoeken", term] => {
            let mut parameters = parse_search_parameters(&url);
            let term = percent_encoding::percent_decode_str(term).decode_utf8_lossy();
            if !term.trim().is_empty() {
                parameters.terms.insert(0, term.into_owned());
            }
            SchatkamerSource::Search(Box::new(parameters))
        }
        _ => return Err(SchatkamerUrlError::UnsupportedPath),
    };

    let retained_query = url
        .query_pairs()
        .filter(|(name, _)| !matches!(name.as_ref(), "pagina" | "page"))
        .map(|(name, value)| (name.into_owned(), value.into_owned()))
        .collect::<Vec<_>>();
    url.set_query(None);
    if !retained_query.is_empty() {
        url.query_pairs_mut().extend_pairs(retained_query);
    }

    let normalized_path = url.path().trim_end_matches('/').to_owned();
    url.set_path(if normalized_path.is_empty() {
        "/"
    } else {
        &normalized_path
    });
    Ok(SchatkamerUrl {
        canonical_url: url.to_string(),
        source,
    })
}

/// Add canonical chapter bounds while preserving unrelated query parameters.
///
/// # Errors
///
/// Returns an error when `input` is not an absolute URL.
pub fn fragment_stream_url(
    input: &str,
    start_seconds: u64,
    end_seconds: Option<u64>,
) -> Result<String, SchatkamerUrlError> {
    let mut url = Url::parse(input).map_err(|_| SchatkamerUrlError::InvalidUrl)?;
    let retained = url
        .query_pairs()
        .filter(|(name, _)| !matches!(name.as_ref(), "start" | "end"))
        .map(|(name, value)| (name.into_owned(), value.into_owned()))
        .collect::<Vec<_>>();
    url.set_query(None);
    {
        let mut query = url.query_pairs_mut();
        query.extend_pairs(retained);
        query.append_pair("start", &start_seconds.to_string());
        if let Some(end_seconds) = end_seconds {
            query.append_pair("end", &end_seconds.to_string());
        }
    }
    Ok(url.to_string())
}

fn parse_search_parameters(url: &Url) -> SearchParameters {
    let mut result = SearchParameters::default();
    for (name, value) in url.query_pairs() {
        let value = value.into_owned();
        match name.as_ref() {
            "q" | "query" | "zoekterm" | "trefwoord" => result.terms.push(value),
            "sort" | "sortering" | "sorteer" | "volgorde" => result.sorting.push(value),
            "type" | "mediatype" => result.media_types.push(value),
            "datum" | "date" | "datumbereik" => result.dates.push(value),
            "van" | "vanaf" | "date_from" | "datum_van" => result.date_from.push(value),
            "tot" | "date_to" | "datum_tot" => result.date_to.push(value),
            "omroep" => result.broadcasters.push(value),
            "collectie" => result.collections.push(value),
            "genre" => result.genres.push(value),
            "persoon" => result.people.push(value),
            "onderwerp" => result.subjects.push(value),
            "pagina" | "page" => {}
            _ => result
                .unknown
                .entry(name.into_owned())
                .or_default()
                .push(value),
        }
    }
    result
}

fn validate_numeric_id(id: &str) -> Result<(), SchatkamerUrlError> {
    if id.is_empty() || !id.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(SchatkamerUrlError::InvalidIdentifier);
    }
    Ok(())
}

fn is_uuid(value: &str) -> bool {
    let groups = [8, 4, 4, 4, 12];
    let mut parts = value.split('-');
    groups.iter().all(|length| {
        parts.next().is_some_and(|part| {
            part.len() == *length && part.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
    }) && parts.next().is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_every_supported_url_type() {
        assert!(matches!(
            parse_schatkamer_url(
                "https://schatkamer.beeldengeluid.nl/lijst/14df8d33-ce8a-4680-a83b-0cc2a9c58bcd"
            )
            .unwrap()
            .source,
            SchatkamerSource::PersonalList { .. }
        ));
        assert!(matches!(
            parse_schatkamer_url(
                "https://schatkamer.beeldengeluid.nl/serie/2101608030021508731/kreatief-met-kurk"
            )
            .unwrap()
            .source,
            SchatkamerSource::Series { .. }
        ));
        assert!(matches!(
            parse_schatkamer_url(
                "https://schatkamer.beeldengeluid.nl/programma/2101608030021508731/kreatief-met-kurk"
            )
            .unwrap()
            .source,
            SchatkamerSource::Series { .. }
        ));
        assert!(matches!(
            parse_schatkamer_url("https://schatkamer.beeldengeluid.nl/serie/2101608030021828731/ster-reclame/aflevering/2101608040033953431")
                .unwrap().source,
            SchatkamerSource::Video { .. }
        ));
        assert!(matches!(
            parse_schatkamer_url("https://schatkamer.beeldengeluid.nl/zoeken?q=journaal")
                .unwrap()
                .source,
            SchatkamerSource::Search(_)
        ));
        let path_search = parse_schatkamer_url(
            "https://schatkamer.beeldengeluid.nl/zoeken/Andr%C3%A9%20van%20Duin",
        )
        .unwrap();
        let SchatkamerSource::Search(parameters) = path_search.source else {
            panic!("expected path search");
        };
        assert_eq!(parameters.terms, ["André van Duin"]);
    }

    #[test]
    fn rejects_invalid_or_unsupported_urls() {
        assert_eq!(
            parse_schatkamer_url("not a URL").unwrap_err(),
            SchatkamerUrlError::InvalidUrl
        );
        assert_eq!(
            parse_schatkamer_url("https://example.com/zoeken?q=x").unwrap_err(),
            SchatkamerUrlError::UnsupportedHost
        );
        assert_eq!(
            parse_schatkamer_url("https://schatkamer.beeldengeluid.nl/onbekend").unwrap_err(),
            SchatkamerUrlError::UnsupportedPath
        );
    }

    #[test]
    fn normalizes_scheme_port_trailing_slash_and_fragment() {
        let parsed =
            parse_schatkamer_url("http://SCHATKAMER.BEELDENGELUID.NL:80/zoeken/?q=test#resultaten")
                .unwrap();
        assert_eq!(
            parsed.canonical_url,
            "https://schatkamer.beeldengeluid.nl/zoeken?q=test"
        );
    }

    #[test]
    fn parses_combined_repeated_unicode_filters_and_unknown_values() {
        let parsed = parse_schatkamer_url(
            "https://schatkamer.beeldengeluid.nl/zoeken?q=Polygoon+journaal&sortering=oudste&mediatype=video&datum=1945-1950&van=1945-01-01&tot=1950-12-31&omroep=VPRO&collectie=Nederlands+Instituut&genre=actualit%C3%A9&genre=documentaire&persoon=Andr%C3%A9+van+Duin&onderwerp=ruimtevaart&experimenteel=%E2%9C%93",
        )
        .unwrap();
        let SchatkamerSource::Search(filters) = parsed.source else {
            panic!("expected search source");
        };
        assert_eq!(filters.terms, ["Polygoon journaal"]);
        assert_eq!(filters.sorting, ["oudste"]);
        assert_eq!(filters.media_types, ["video"]);
        assert_eq!(filters.dates, ["1945-1950"]);
        assert_eq!(filters.date_from, ["1945-01-01"]);
        assert_eq!(filters.date_to, ["1950-12-31"]);
        assert_eq!(filters.broadcasters, ["VPRO"]);
        assert_eq!(filters.collections, ["Nederlands Instituut"]);
        assert_eq!(filters.genres, ["actualité", "documentaire"]);
        assert_eq!(filters.people, ["André van Duin"]);
        assert_eq!(filters.subjects, ["ruimtevaart"]);
        assert_eq!(filters.unknown["experimenteel"], ["✓"]);
    }

    #[test]
    fn generates_bounded_and_open_ended_fragment_urls() {
        assert_eq!(
            fragment_stream_url(
                "https://schatkamer.beeldengeluid.nl/serie/1/s/aflevering/2?token=x&start=1",
                95,
                Some(252),
            )
            .unwrap(),
            "https://schatkamer.beeldengeluid.nl/serie/1/s/aflevering/2?token=x&start=95&end=252"
        );
        assert_eq!(
            fragment_stream_url(
                "https://schatkamer.beeldengeluid.nl/serie/1/s/aflevering/2",
                252,
                None,
            )
            .unwrap(),
            "https://schatkamer.beeldengeluid.nl/serie/1/s/aflevering/2?start=252"
        );
    }
}
