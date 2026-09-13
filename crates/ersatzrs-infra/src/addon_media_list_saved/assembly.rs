//! Initial source assembly, not the ordinary saved-content writer or reuse path.

use std::{collections::BTreeSet, path::Path, sync::Arc};

use ersatzrs_addon_contract::AddonManifest;
use ersatzrs_core::repositories::{RepositoryError, Result};

use super::{
    assets,
    capture::CapturedDraft,
    document_io, files,
    model::{SavedFile, SavedListDocument, SavedPlayback, SAVED_LIST_VERSION},
    published::CapturedPublication,
    receiving, sources, validation,
};
use crate::db::DbPool;
pub use crate::repositories::addon_publication_fs::ManagedTreeGuard;

/// Explicit local ownership supplied by the initial-materialization caller.
pub struct AssemblyContext<'a> {
    pub list_id: i64,
    pub tree: Arc<ManagedTreeGuard>,
    pub storage_parent: &'a Path,
    pub storage_owner: &'a str,
    pub artifact_root: &'a Path,
    pub installed: &'a AddonManifest,
}

/// Materialize a complete initial document from independently captured states.
///
/// This intentionally hashes new media and is not the ordinary Save/reuse path.
/// The caller supplies normalized roots and retains its content/revision/tree
/// fence through capture, this operation and eventual commitment. Each sequential
/// blocking installer owns a clone of the tree guard until completion, even if
/// its waiting future is cancelled. No runtime, transaction or document
/// writer is created here. Do not hold a database write transaction during this
/// work or invoke it inside an existing database-free tree-lock closure.
/// Prepared Kind 1 sources are never opened or copied. Failure may leave verified
/// immutable assets, but never acknowledges Save or changes the active document.
/// Contained replacement videos are hashed and referenced at their existing paths;
/// assembly does not create a second video file.
///
/// # Errors
/// Rejects unsafe, changed, missing or inconsistent required sources and content.
/// Undeclared local artwork is refused rather than silently omitted.
#[allow(clippy::too_many_lines)] // Keep initial source sequencing and state separation explicit.
pub async fn assemble_initial(
    pool: &DbPool,
    context: &AssemblyContext<'_>,
    captured: CapturedDraft,
    publication: Option<CapturedPublication>,
) -> Result<SavedListDocument> {
    let CapturedDraft {
        addon_id,
        provider_id,
        source_url,
        draft,
        artwork,
        replacements,
    } = captured;
    let (published, revision_files, artifacts, published_replacements) = match publication {
        Some(value) => (
            Some(value.publication),
            value.revision_files,
            value.artifacts,
            value.replacements,
        ),
        None => (None, Vec::new(), Vec::new(), Vec::new()),
    };
    let mut document = SavedListDocument {
        version: SAVED_LIST_VERSION,
        addon_id,
        provider_id,
        source_url,
        draft,
        published,
    };
    // Catch undeclared local URLs and conflicting inventories before installing.
    validation::validate_content(&document, context.installed).map_err(|_| invalid())?;
    document_io::serialized_size(&document).map_err(|_| invalid())?;
    for upload in artwork {
        if !document.draft.files.contains(&upload.file) {
            return Err(invalid());
        }
        let bytes = sources::load_owned_artwork(pool, context.list_id, &upload).await?;
        install_bytes(Arc::clone(&context.tree), upload.file, bytes).await?;
    }
    for file in revision_files {
        if !document
            .published
            .as_ref()
            .is_some_and(|state| state.files.contains(&super::published::saved_file(&file)))
        {
            return Err(invalid());
        }
        let tree = Arc::clone(&context.tree);
        let parent = context.storage_parent.to_owned();
        let owner = context.storage_owner.to_owned();
        let list_id = context.list_id;
        tokio::task::spawn_blocking(move || {
            sources::install_retained_file(tree.managed_root(), &parent, &owner, list_id, &file)
                .map_err(|_| invalid())
        })
        .await
        .map_err(|_| invalid())??;
    }
    if let Some(state) = &document.published {
        for subtitle in state.items.iter().flat_map(|item| &item.subtitles) {
            // Identical text may have distinct pinned language/clock owners.
            // Reuse only a source whose full ownership check accepts this slot.
            let mut original = None;
            for source in artifacts
                .iter()
                .filter(|source| source.file == subtitle.file)
            {
                if let Ok(text) = sources::load_owned_subtitle(
                    pool,
                    context.list_id,
                    state.revision,
                    source,
                    subtitle,
                    context.artifact_root,
                )
                .await
                {
                    original = Some(text);
                    break;
                }
            }
            let text = original.ok_or_else(invalid)?;
            install_bytes(
                Arc::clone(&context.tree),
                subtitle.file.clone(),
                text.into_bytes(),
            )
            .await?;
        }
    }
    for selected in replacements {
        let item = document
            .draft
            .items
            .iter_mut()
            .find(|item| item.content.provider_id == selected.provider_id)
            .ok_or_else(invalid)?;
        let replacement = item.replacement.as_mut().ok_or_else(invalid)?;
        replacement.file =
            sources::inspect_owned_replacement(pool, context.tree.managed_root(), &selected)
                .await?;
        if let Some(file) = &replacement.file {
            document.draft.files.push(file.clone());
        }
    }
    for selected in published_replacements {
        let state = document.published.as_mut().ok_or_else(invalid)?;
        let item = state
            .items
            .iter_mut()
            .find(|item| item.content.provider_id == selected.provider_id)
            .ok_or_else(invalid)?;
        let SavedPlayback::Replacement { replacement } = &mut item.playback else {
            return Err(invalid());
        };
        replacement.file =
            sources::inspect_owned_replacement(pool, context.tree.managed_root(), &selected)
                .await?;
        if let Some(file) = &replacement.file {
            state.files.push(file.clone());
        }
    }
    for files in std::iter::once(&mut document.draft.files)
        .chain(document.published.iter_mut().map(|state| &mut state.files))
    {
        files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
        files.dedup();
    }
    validation::validate_content(&document, context.installed).map_err(|_| invalid())?;
    document_io::serialized_size(&document).map_err(|_| invalid())?;
    verify_required(context.tree.managed_root(), &document).await?;
    Ok(document)
}

async fn install_bytes(tree: Arc<ManagedTreeGuard>, file: SavedFile, bytes: Vec<u8>) -> Result<()> {
    tokio::task::spawn_blocking(move || {
        assets::install(tree.managed_root(), &file, &mut bytes.as_slice()).map_err(|_| invalid())
    })
    .await
    .map_err(|_| invalid())?
}

async fn verify_required(root: &Path, document: &SavedListDocument) -> Result<()> {
    let mut images = BTreeSet::new();
    receiving::layer_images(&document.draft.metadata, &mut images);
    for item in &document.draft.items {
        receiving::layer_images(&item.metadata, &mut images);
    }
    let mut prepared = BTreeSet::new();
    let mut required: BTreeSet<_> = document
        .draft
        .files
        .iter()
        .map(|file| &file.relative_path)
        .collect();
    if let Some(state) = &document.published {
        receiving::map_images(&state.metadata, &mut images);
        for item in &state.items {
            receiving::map_images(&item.metadata, &mut images);
            match &item.playback {
                SavedPlayback::DownloadAhead { file, .. } => {
                    prepared.insert(&file.relative_path);
                }
                SavedPlayback::Replacement { replacement } => {
                    if let Some(file) = &replacement.file {
                        required.insert(&file.relative_path);
                    }
                }
                SavedPlayback::Stream { .. } => {}
            }
            required.extend(
                item.subtitles
                    .iter()
                    .map(|subtitle| &subtitle.file.relative_path),
            );
        }
    }
    for file in document
        .draft
        .files
        .iter()
        .chain(document.published.iter().flat_map(|state| &state.files))
    {
        if images.contains(file.relative_path.as_str()) {
            receiving::read_artwork(root, file)
                .await
                .map_err(|_| invalid())?;
        } else if (!prepared.contains(&file.relative_path)
            || required.contains(&file.relative_path))
            && files::verify_file(root, file)
                .await
                .map_err(|_| invalid())?
                != files::SavedFileState::Verified
        {
            return Err(invalid());
        }
    }
    Ok(())
}

fn invalid() -> RepositoryError {
    RepositoryError::Conflict(
        "the initial saved list sources are unavailable or inconsistent".into(),
    )
}

#[cfg(test)]
mod tests;
