//! Preserve the stored metadata shape and provenance, not provider-ingestion rules.

use std::collections::BTreeMap;

use ersatzrs_core::repositories::AddonMediaListNfoMetadata;
use serde::Deserialize as _;
use serde_json::{Map, Value};

use super::super::files::validate_reference;
use super::super::model::{SavedFile, SavedListDocument, SavedMetadata, SavedPlayback};
use super::SavedContentError;

pub(super) fn validate_metadata(document: &SavedListDocument) -> Result<(), SavedContentError> {
    // Both states refer to one physical copied tree. Repeated references to the
    // same bytes are fine; one path cannot denote two different file contents.
    FileRegistry::new(
        document.draft.files.iter().chain(
            document
                .published
                .iter()
                .flat_map(|publication| publication.files.iter()),
        ),
    )?;
    let draft_files = FileRegistry::new(&document.draft.files)?;
    layers(&document.draft.metadata, &draft_files)?;
    for item in &document.draft.items {
        layers(&item.metadata, &draft_files)?;
        if let Some(file) = item
            .replacement
            .as_ref()
            .and_then(|replacement| replacement.file.as_ref())
        {
            draft_files.reference(file)?;
        }
    }
    if let Some(published) = &document.published {
        let files = FileRegistry::new(&published.files)?;
        merged(&published.metadata, &Map::new())?;
        artwork(&published.metadata, &files)?;
        for item in &published.items {
            merged(&item.metadata, &Map::new())?;
            artwork(&item.metadata, &files)?;
            match &item.playback {
                SavedPlayback::DownloadAhead { file, .. } => files.reference(file)?,
                SavedPlayback::Replacement { replacement } => {
                    if let Some(file) = &replacement.file {
                        files.reference(file)?;
                    }
                }
                SavedPlayback::Stream { .. } => {}
            }
            for subtitle in &item.subtitles {
                files.reference(&subtitle.file)?;
            }
        }
    }
    Ok(())
}

fn layers(metadata: &SavedMetadata, files: &FileRegistry<'_>) -> Result<(), SavedContentError> {
    merged(&metadata.provider, &metadata.overrides)?;
    artwork(&metadata.provider, files)?;
    artwork(&metadata.overrides, files)?;
    for field in ["artwork", "artwork_urls", "people"] {
        if let Some(conflict) = metadata.conflicts.get(field) {
            for value in [
                &conflict.provider_before,
                &conflict.provider_after,
                &conflict.operator_value,
            ] {
                references(field, value, files)?;
            }
        }
    }
    Ok(())
}

fn merged(
    provider: &Map<String, Value>,
    overrides: &Map<String, Value>,
) -> Result<(), SavedContentError> {
    // Exactly the top-level override rule used by metadata_form_from_json.
    // Feed borrowed values to the existing tolerant stored DTO; no field
    // whitelist, wire-only limits, or intermediate whole metadata copy.
    let fields = provider
        .iter()
        .filter(|(key, _)| !overrides.contains_key(*key))
        .chain(overrides.iter())
        .map(|(key, value)| (key.as_str(), value));
    AddonMediaListNfoMetadata::deserialize(
        serde::de::value::MapDeserializer::<_, serde_json::Error>::new(fields),
    )
    .map_err(|_| SavedContentError::Metadata)?;
    Ok(())
}

fn artwork(
    metadata: &Map<String, Value>,
    files: &FileRegistry<'_>,
) -> Result<(), SavedContentError> {
    for field in ["artwork", "artwork_urls", "people"] {
        if let Some(value) = metadata.get(field) {
            references(field, value, files)?;
        }
    }
    Ok(())
}

fn references(
    field: &str,
    value: &Value,
    files: &FileRegistry<'_>,
) -> Result<(), SavedContentError> {
    // Names, roles and unknown descriptive fields are not file references.
    // Masked provider/conflict values retain their stored form; only actual URL
    // slots can acquire a receiving upload handle during materialization.
    for entry in value.as_array().into_iter().flatten() {
        let url = match field {
            "artwork" => entry.get("url"),
            "people" => entry.get("thumb"),
            "artwork_urls" => Some(entry),
            _ => None,
        };
        let Some(url) = url.and_then(Value::as_str) else {
            continue;
        };
        if url.starts_with(crate::addon_artwork::UPLOAD_URL_PREFIX) {
            return Err(SavedContentError::Files);
        }
        if let Some(path) = url.strip_prefix("./") {
            files.path(path)?;
        }
    }
    Ok(())
}

struct FileRegistry<'a>(BTreeMap<String, &'a SavedFile>);

impl<'a> FileRegistry<'a> {
    fn new(files: impl IntoIterator<Item = &'a SavedFile>) -> Result<Self, SavedContentError> {
        let mut registry = Self(BTreeMap::new());
        for file in files {
            validate_reference(file).map_err(|_| SavedContentError::Files)?;
            let key = file.relative_path.to_lowercase();
            if let Some(existing) = registry.0.insert(key, file) {
                if existing != file {
                    return Err(SavedContentError::Files);
                }
            }
        }
        Ok(registry)
    }

    fn path(&self, path: &str) -> Result<&SavedFile, SavedContentError> {
        self.0
            .get(&path.to_lowercase())
            .copied()
            .filter(|file| file.relative_path == path)
            .ok_or(SavedContentError::Files)
    }

    fn reference(&self, file: &SavedFile) -> Result<(), SavedContentError> {
        if self.path(&file.relative_path)? != file {
            return Err(SavedContentError::Files);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
