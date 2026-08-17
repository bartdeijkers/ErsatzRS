//! Provider-neutral host derivation helpers for add-on chapter fragments.

use ersatzrs_addon_contract::validate_addon_id;
use url::Url;

use crate::addons::AddonError;

/// Add canonical chapter bounds while preserving unrelated query parameters.
///
/// # Errors
///
/// Returns an error when the source is not an absolute URL.
pub fn fragment_stream_url(
    input: &str,
    start_seconds: u64,
    end_seconds: Option<u64>,
) -> Result<String, AddonError> {
    let mut url = Url::parse(input)
        .map_err(|_| AddonError::Validation("chapter source URL must be absolute".to_owned()))?;
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

/// Stable provider-neutral GUID for one host-derived fragment.
#[must_use]
pub fn fragment_guid(addon_id: &str, parent_provider_id: &str, start_seconds: u64) -> String {
    debug_assert!(validate_addon_id(addon_id).is_ok());
    let parent =
        url::form_urlencoded::byte_serialize(parent_provider_id.as_bytes()).collect::<String>();
    format!("addon-chapter://{addon_id}/{parent}/{start_seconds}")
}

#[cfg(test)]
mod tests {
    use super::{fragment_guid, fragment_stream_url};

    #[test]
    fn chapter_identity_and_bounds_are_provider_neutral() {
        assert_eq!(
            fragment_guid("org.example.video", "video/one", 20),
            "addon-chapter://org.example.video/video%2Fone/20"
        );
        assert_eq!(
            fragment_stream_url("https://example.test/watch?v=1&start=9", 20, Some(40)).unwrap(),
            "https://example.test/watch?v=1&start=20&end=40"
        );
    }
}
