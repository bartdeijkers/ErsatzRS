//! Read-only admission of copied bytes, before receiving identities or activation.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use ersatzrs_addon_contract::AddonManifest;
use ersatzrs_domain::MediaPlaybackTimeline;
use serde_json::{Map, Value};

use super::files::{read_verified_file, verify_file, SavedFileError, SavedFileState};
use super::model::{SavedFile, SavedListDocument, SavedMetadata, SavedPlayback};
use super::validation::{metadata::urls, validate_content, SavedContentError};

/// Copied content is untrusted; diagnostics never contain its paths or metadata.
#[derive(Debug, thiserror::Error)]
pub enum ReceivingAssetError {
    #[error(transparent)]
    Content(#[from] SavedContentError),
    #[error(transparent)]
    File(#[from] SavedFileError),
    #[error("the saved artwork is not a supported image")]
    Artwork,
    #[error("the saved subtitle is not a valid manual SRT original")]
    Subtitle,
}

/// Validate copied content and return published provider IDs needing local preparation.
///
/// Only absence of exclusively prepared media is recoverable this way. Corrupt
/// or unsafe files and missing required artwork, subtitles or replacements refuse
/// admission. No modes, metadata, database rows or files are changed. Present
/// media still needs receiving bindings; verified bytes alone do not grant
/// playback ownership. Caller owns the enabled installation check and tree fence
/// through later projection, and must revalidate after releasing that fence.
///
/// # Errors
/// Rejects invalid saved content or required assets, using existing image/SRT limits.
pub async fn validate_received_assets(
    root: &Path,
    document: &SavedListDocument,
    installed: &AddonManifest,
) -> Result<BTreeSet<String>, ReceivingAssetError> {
    validate_content(document, installed)?;
    let files: BTreeMap<_, _> = document
        .draft
        .files
        .iter()
        .chain(document.published.iter().flat_map(|state| &state.files))
        .map(|file| (file.relative_path.as_str(), file))
        .collect();
    let mut images = BTreeSet::new();
    layer_images(&document.draft.metadata, &mut images);
    for item in &document.draft.items {
        layer_images(&item.metadata, &mut images);
    }
    let mut prepared = BTreeSet::new();
    let mut required: BTreeSet<_> = document
        .draft
        .files
        .iter()
        .map(|file| file.relative_path.as_str())
        .collect();
    let mut subtitles = BTreeSet::new();
    if let Some(publication) = &document.published {
        map_images(&publication.metadata, &mut images);
        for item in &publication.items {
            map_images(&item.metadata, &mut images);
            match &item.playback {
                SavedPlayback::DownloadAhead { file, .. } => {
                    prepared.insert(file.relative_path.as_str());
                }
                SavedPlayback::Replacement { replacement } => {
                    if let Some(file) = &replacement.file {
                        required.insert(file.relative_path.as_str());
                    }
                }
                SavedPlayback::Stream { .. } => {}
            }
            subtitles.extend(
                item.subtitles
                    .iter()
                    .map(|subtitle| subtitle.file.relative_path.as_str()),
            );
        }
    }
    let mut missing = BTreeSet::new();
    for (path, file) in files {
        if images.contains(path) {
            read_artwork(root, file).await?;
        }
        if subtitles.contains(path) {
            read_subtitle(root, file).await?;
        }
        if !images.contains(path)
            && !subtitles.contains(path)
            && verify_file(root, file).await? == SavedFileState::Missing
        {
            if !prepared.contains(path) || required.contains(path) {
                return Err(SavedFileError::Unavailable.into());
            }
            missing.insert(path);
        }
    }
    Ok(document
        .published
        .iter()
        .flat_map(|state| &state.items)
        .filter_map(|item| match &item.playback {
            SavedPlayback::DownloadAhead { file, .. }
                if missing.contains(file.relative_path.as_str()) =>
            {
                Some(item.content.provider_id.clone())
            }
            _ => None,
        })
        .collect())
}

pub(super) fn layer_images<'a>(metadata: &'a SavedMetadata, images: &mut BTreeSet<&'a str>) {
    map_images(&metadata.provider, images);
    map_images(&metadata.overrides, images);
    for field in ["artwork", "artwork_urls"] {
        if let Some(conflict) = metadata.conflicts.get(field) {
            for value in [
                &conflict.provider_before,
                &conflict.provider_after,
                &conflict.operator_value,
            ] {
                images.extend(urls(field, value).filter_map(|url| url.strip_prefix("./")));
            }
        }
    }
}

pub(super) fn map_images<'a>(metadata: &'a Map<String, Value>, images: &mut BTreeSet<&'a str>) {
    for field in ["artwork", "artwork_urls"] {
        if let Some(value) = metadata.get(field) {
            images.extend(urls(field, value).filter_map(|url| url.strip_prefix("./")));
        }
    }
}

/// Read verified image bytes for receiving-owned staging; no upload is created here.
///
/// # Errors
/// Rejects unsafe/missing/oversized bytes or an extension/signature mismatch.
pub async fn read_artwork(
    root: &Path,
    reference: &SavedFile,
) -> Result<(Vec<u8>, &'static str), ReceivingAssetError> {
    let extension = Path::new(&reference.relative_path)
        .extension()
        .and_then(|value| value.to_str());
    let content_type = match extension.map(str::to_ascii_lowercase).as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        _ => return Err(ReceivingAssetError::Artwork),
    };
    let bytes = read_verified_file(
        root,
        reference,
        crate::addon_artwork::MAX_ARTWORK_BYTES as u64,
    )
    .await?;
    crate::addon_artwork::validate_image(content_type, &bytes)
        .map_err(|_| ReceivingAssetError::Artwork)?;
    Ok((bytes, content_type))
}

/// Read the original SRT unchanged, validating through the existing identity timeline.
/// Timing/language compatibility is validated separately by saved-content validation.
///
/// # Errors
/// Rejects unsafe/missing/oversized, non-UTF8, malformed or cue-free originals.
pub async fn read_subtitle(
    root: &Path,
    reference: &SavedFile,
) -> Result<String, ReceivingAssetError> {
    let maximum = crate::addon_media_list_subtitle_artifacts::MAXIMUM_SUBTITLE_FILE_BYTES;
    let bytes = read_verified_file(root, reference, maximum).await?;
    let text = String::from_utf8(bytes).map_err(|_| ReceivingAssetError::Subtitle)?;
    let identity = MediaPlaybackTimeline::from_exclusions(i64::MAX as u64, 0..i64::MAX as u64, &[])
        .map_err(|_| ReceivingAssetError::Subtitle)?;
    let mapped = crate::subtitles::map_srt_through_timeline(
        &text,
        &identity,
        usize::try_from(2 * maximum).map_err(|_| ReceivingAssetError::Subtitle)?,
    )
    .map_err(|_| ReceivingAssetError::Subtitle)?;
    if mapped.is_empty() {
        return Err(ReceivingAssetError::Subtitle);
    }
    Ok(text)
}

#[cfg(test)]
mod tests;
