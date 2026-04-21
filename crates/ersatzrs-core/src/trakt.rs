//! Trakt list contracts ported from `ErsatzTV.Core/Trakt` and
//! `ErsatzTV.Application/MediaCollections/Commands/AddTraktListHandler.cs`.

use async_trait::async_trait;
use ersatzrs_domain::{TraktList, TraktListItemKind};

use crate::repositories::Result;

/// Parsed Trakt list identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraktListLocator {
    pub user: String,
    pub list: String,
}

/// Trakt list item plus provider GUIDs returned by the API client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraktListItemWithGuids {
    pub trakt_id: i32,
    pub rank: i32,
    pub display_title: String,
    pub title: String,
    pub year: Option<i32>,
    pub season: i32,
    pub episode: i32,
    pub kind: TraktListItemKind,
    pub guids: Vec<String>,
}

/// Trakt API surface used by the list synchronizer.
#[async_trait]
pub trait TraktApiClient: Send + Sync {
    async fn get_user_list(&self, user: &str, list: &str) -> Result<TraktList>;
    async fn get_user_list_items(
        &self,
        user: &str,
        list: &str,
    ) -> Result<Vec<TraktListItemWithGuids>>;
}

/// Parse the same URL and shorthand forms accepted by `ErsatzTV`.
///
/// Supported forms:
///
/// - `https://trakt.tv/users/{user}/lists/{list}`
/// - `https://app.trakt.tv/users/{user}/lists/{list}`
/// - `https://trakt.tv/users/{user}/{list}`
/// - `https://trakt.tv/lists/{user}/{list}`
/// - `{user}/lists/{list}`
/// - `{user}/{list}`
///
/// # Errors
///
/// Returns an error message when the host, path shape, or slug characters are
/// invalid.
pub fn parse_trakt_list_locator(input: &str) -> std::result::Result<TraktListLocator, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("Invalid Trakt list url".to_string());
    }

    let segments = if let Some(rest) = trimmed.strip_prefix("https://") {
        parse_absolute_segments(rest)?
    } else if trimmed.contains("://") {
        return Err("Invalid Trakt list url".to_string());
    } else {
        trimmed.split('/').collect::<Vec<_>>()
    };

    locator_from_segments(&segments).ok_or_else(|| "Invalid Trakt list url".to_string())
}

fn parse_absolute_segments(rest: &str) -> std::result::Result<Vec<&str>, String> {
    let without_query = rest.split_once(['?', '#']).map_or(rest, |(path, _)| path);
    let mut parts = without_query.split('/').collect::<Vec<_>>();
    let host = parts.first().copied().unwrap_or_default();
    if host != "trakt.tv" && host != "app.trakt.tv" {
        return Err("Invalid Trakt list url".to_string());
    }

    parts.remove(0);
    Ok(parts)
}

fn locator_from_segments(segments: &[&str]) -> Option<TraktListLocator> {
    let segments = segments
        .iter()
        .copied()
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();

    let (user, list) = if segments.len() >= 4 && segments[0] == "users" && segments[2] == "lists" {
        (segments[1], segments[3])
    } else if segments.len() >= 3 && (segments[0] == "users" || segments[0] == "lists") {
        (segments[1], segments[2])
    } else if segments.len() >= 3 && segments[1] == "lists" {
        (segments[0], segments[2])
    } else if segments.len() >= 2 {
        (segments[0], segments[1])
    } else {
        return None;
    };

    if !is_slug(user) || !is_slug(list) {
        return None;
    }

    Some(TraktListLocator {
        user: user.to_string(),
        list: list.to_string(),
    })
}

fn is_slug(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

#[cfg(test)]
mod tests {
    use super::parse_trakt_list_locator;

    #[test]
    fn parses_trakt_url_forms() {
        for input in [
            "https://trakt.tv/users/some-user/lists/watchlist",
            "https://app.trakt.tv/users/some-user/lists/watchlist",
            "https://trakt.tv/users/some-user/watchlist",
            "https://trakt.tv/lists/some-user/watchlist",
            "some-user/lists/watchlist",
            "some-user/watchlist",
        ] {
            let parsed = parse_trakt_list_locator(input).expect(input);
            assert_eq!(parsed.user, "some-user");
            assert_eq!(parsed.list, "watchlist");
        }
    }

    #[test]
    fn rejects_non_trakt_urls() {
        let error = parse_trakt_list_locator("https://example.com/users/u/lists/l")
            .expect_err("non-trakt host must fail");
        assert_eq!(error, "Invalid Trakt list url");
    }
}
