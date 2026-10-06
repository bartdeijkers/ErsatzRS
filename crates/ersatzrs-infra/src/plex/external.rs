//! Bounded single-item metadata reads for external JSON playouts.

use super::{project_media_item, PlexConnectionConfig, PlexMediaItem, PlexMetadataResponse};

const MAX_METADATA_BYTES: usize = 8 * 1024 * 1024;

/// The caller owns the startup deadline and cancellation. No input values or
/// provider response bodies are included in this module's errors.
pub(crate) async fn fetch(
    config: &PlexConnectionConfig,
    rating_key: &str,
) -> Result<PlexMediaItem, ()> {
    let key = super::plex_rating_key(rating_key).ok_or(())?;
    let mut url = reqwest::Url::parse(&config.uri).map_err(|_| ())?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(());
    }
    url.path_segments_mut()?
        .pop_if_empty()
        .extend(["library", "metadata", key]);
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|_| ())?;
    let mut response = client
        .get(url)
        .header(super::ACCEPT, super::APPLICATION_JSON)
        .header("X-Plex-Token", &config.token)
        .send()
        .await
        .map_err(|_| ())?;
    if !response.status().is_success()
        || response
            .content_length()
            .is_some_and(|len| len > MAX_METADATA_BYTES as u64)
    {
        return Err(());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| ())? {
        if chunk.len() > MAX_METADATA_BYTES.saturating_sub(bytes.len()) {
            return Err(());
        }
        bytes.extend_from_slice(&chunk);
    }
    let response: PlexMetadataResponse = serde_json::from_slice(&bytes).map_err(|_| ())?;
    response
        .media_container
        .metadata
        .into_iter()
        .find_map(project_media_item)
        .ok_or(())
}
