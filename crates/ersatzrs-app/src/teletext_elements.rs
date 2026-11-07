//! Teletext element library: upload, preview, field bindings and channel links.
//!
//! Elements are user-supplied pages (for example made in `QTeletextMaker`).
//! This module stores and previews them and records which channels should
//! show them. It grants no publication or broadcast authority: including a
//! linked element in a channel's live teletext service is a separate,
//! explicitly published step that is not part of this module.

pub mod fields;
pub mod import;
pub mod level1;
pub mod model;

use chrono::{DateTime, Local, Utc};
use ersatzrs_core::repositories::{RepositoryError, TeletextElementRepository};
use ersatzrs_domain::{
    validate_teletext_element_bindings, ChannelTeletextElement, EntityId, TeletextElement,
    TeletextElementBinding, TeletextElementSummary, TeletextElementUsage,
    MAX_TELETEXT_ELEMENT_NAME_BYTES, TELETEXT_ELEMENT_CYCLE_SECONDS,
};

use crate::ApplicationServices;
use fields::{apply_field_values, bindings_for_markers, find_field_markers, FieldValues};
use import::{import_teletext_element_file, ElementImportError, ElementImportWarning};
use model::ElementPage;

/// Default subpage cycle when a file does not say otherwise.
pub const DEFAULT_TELETEXT_ELEMENT_CYCLE_SECONDS: u16 = 10;

/// Static failure categories; each maps to a translated admin message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextElementError {
    #[error("teletext element storage is unavailable")]
    Unavailable,
    #[error("teletext element not found")]
    NotFound,
    #[error("a teletext element with this name already exists")]
    DuplicateName,
    #[error("the teletext element was changed by someone else")]
    Stale,
    #[error("teletext element name is invalid")]
    InvalidName,
    #[error("teletext page number is invalid")]
    InvalidPageNumber,
    #[error("teletext subpage cycle is invalid")]
    InvalidCycle,
    #[error("teletext element fields are invalid")]
    InvalidBindings,
    #[error("page number is already used on this channel")]
    PageInUse,
    #[error(transparent)]
    Import(#[from] ElementImportError),
    #[error("teletext element storage failed")]
    Storage,
}

impl TeletextElementError {
    /// Stable code used in `?error=` redirects.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::NotFound => "not-found",
            Self::DuplicateName => "duplicate-name",
            Self::Stale => "stale",
            Self::InvalidName => "name",
            Self::InvalidPageNumber => "page-number",
            Self::InvalidCycle => "cycle",
            Self::InvalidBindings => "fields",
            Self::PageInUse => "page-in-use",
            Self::Import(ElementImportError::UnsupportedFormat) => "format",
            Self::Import(ElementImportError::Empty) => "empty",
            Self::Import(ElementImportError::TooLarge) => "too-large",
            Self::Import(ElementImportError::NoPage | ElementImportError::Damaged) => "damaged",
            Self::Storage => "storage",
        }
    }
}

fn storage(error: RepositoryError) -> TeletextElementError {
    match error {
        RepositoryError::NotFound { .. } => TeletextElementError::NotFound,
        RepositoryError::Conflict(message) if message.contains("name") => {
            TeletextElementError::DuplicateName
        }
        RepositoryError::Conflict(_) => TeletextElementError::Stale,
        other => {
            tracing::warn!(error = %other, "teletext element storage failed");
            TeletextElementError::Storage
        }
    }
}

fn clean_name(name: &str) -> Result<String, TeletextElementError> {
    let name: String = name.trim().chars().filter(|c| !c.is_control()).collect();
    if name.is_empty() || name.len() > MAX_TELETEXT_ELEMENT_NAME_BYTES {
        return Err(TeletextElementError::InvalidName);
    }
    Ok(name)
}

/// Result of an upload, with notes for the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextElementUpload {
    pub id: EntityId,
    pub warnings: Vec<ElementImportWarning>,
}

/// Everything the editor's preview needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextElementPreview {
    /// The page as designed, or filled with the channel's data.
    pub page: ElementPage,
    /// Values used for filling, when a channel was chosen.
    pub values: Option<FieldValues>,
}

fn cycle_from_page(page: &ElementPage) -> u16 {
    page.subpages
        .iter()
        .find_map(|subpage| match subpage.cycle {
            Some((value, crate::teletext_tti::TeletextTtiCycleUnit::Time)) => {
                u16::try_from(value).ok()
            }
            _ => None,
        })
        .filter(|value| TELETEXT_ELEMENT_CYCLE_SECONDS.contains(value))
        .unwrap_or(DEFAULT_TELETEXT_ELEMENT_CYCLE_SECONDS)
}

/// Parse a stored element's canonical document.
/// # Errors
/// Returns `Storage` when stored bytes are no longer readable.
pub fn teletext_element_page(
    element: &TeletextElement,
) -> Result<ElementPage, TeletextElementError> {
    ElementPage::from_canonical_tti(&element.document).map_err(|_| TeletextElementError::Storage)
}

impl ApplicationServices {
    fn teletext_element_store(
        &self,
    ) -> Result<&(dyn TeletextElementRepository + 'static), TeletextElementError> {
        self.ports()
            .teletext_elements()
            .ok_or(TeletextElementError::Unavailable)
    }

    /// All elements with usage counts.
    /// # Errors
    /// Storage failures.
    pub async fn list_teletext_elements(
        &self,
    ) -> Result<Vec<TeletextElementSummary>, TeletextElementError> {
        self.teletext_element_store()?
            .list_summaries()
            .await
            .map_err(storage)
    }

    /// One element.
    /// # Errors
    /// Storage failures.
    pub async fn get_teletext_element(
        &self,
        id: EntityId,
    ) -> Result<Option<TeletextElement>, TeletextElementError> {
        self.teletext_element_store()?
            .get(id)
            .await
            .map_err(storage)
    }

    /// Import a file as a new element. Name and page number come from the file.
    /// # Errors
    /// Import, naming and storage failures.
    pub async fn upload_teletext_element(
        &self,
        file_name: &str,
        bytes: &[u8],
    ) -> Result<TeletextElementUpload, TeletextElementError> {
        let store = self.teletext_element_store()?;
        let imported = import_teletext_element_file(file_name, bytes)?;
        let document = imported
            .page
            .to_tti()
            .map_err(|_| TeletextElementError::Import(ElementImportError::Damaged))?;
        let bindings = bindings_for_markers(&find_field_markers(&imported.page), &[]);
        let base = clean_name(&imported.page.description)
            .or_else(|_| clean_name("Teletext"))
            .unwrap_or_default();
        let existing: Vec<String> = store
            .list_summaries()
            .await
            .map_err(storage)?
            .into_iter()
            .map(|summary| summary.name)
            .collect();
        let name = unique_name(&base, &existing);
        let element = TeletextElement {
            id: 0,
            name,
            source_format: imported.format,
            source_file_name: file_name
                .chars()
                .filter(|c| !c.is_control())
                .take(255)
                .collect(),
            document,
            page_number: imported.page.page,
            subpage_count: u16::try_from(imported.page.subpages.len()).unwrap_or(u16::MAX),
            cycle_seconds: cycle_from_page(&imported.page),
            bindings,
            revision: 0,
            updated_at: String::new(),
        };
        let id = store.create(&element).await.map_err(storage)?;
        Ok(TeletextElementUpload {
            id,
            warnings: imported.warnings,
        })
    }

    /// Replace an element's page with a new version of the file, keeping its
    /// name, page number and the field choices whose markers still exist.
    /// # Errors
    /// Import, stale-revision and storage failures.
    pub async fn replace_teletext_element_file(
        &self,
        id: EntityId,
        expected_revision: i64,
        file_name: &str,
        bytes: &[u8],
    ) -> Result<Vec<ElementImportWarning>, TeletextElementError> {
        let store = self.teletext_element_store()?;
        let mut element = store
            .get(id)
            .await
            .map_err(storage)?
            .ok_or(TeletextElementError::NotFound)?;
        let imported = import_teletext_element_file(file_name, bytes)?;
        element.document = imported
            .page
            .to_tti()
            .map_err(|_| TeletextElementError::Import(ElementImportError::Damaged))?;
        element.bindings =
            bindings_for_markers(&find_field_markers(&imported.page), &element.bindings);
        element.source_format = imported.format;
        element.source_file_name = file_name
            .chars()
            .filter(|c| !c.is_control())
            .take(255)
            .collect();
        element.subpage_count = u16::try_from(imported.page.subpages.len()).unwrap_or(u16::MAX);
        store
            .update(&element, expected_revision)
            .await
            .map_err(storage)?;
        Ok(imported.warnings)
    }

    /// Save name, suggested page number and subpage cycle.
    /// # Errors
    /// Validation, stale-revision and storage failures.
    pub async fn save_teletext_element_details(
        &self,
        id: EntityId,
        expected_revision: i64,
        name: &str,
        page_number: u16,
        cycle_seconds: u16,
    ) -> Result<(), TeletextElementError> {
        let store = self.teletext_element_store()?;
        let mut element = store
            .get(id)
            .await
            .map_err(storage)?
            .ok_or(TeletextElementError::NotFound)?;
        element.name = clean_name(name)?;
        if !(0x100..=0x8ff).contains(&page_number) {
            return Err(TeletextElementError::InvalidPageNumber);
        }
        if !TELETEXT_ELEMENT_CYCLE_SECONDS.contains(&cycle_seconds) {
            return Err(TeletextElementError::InvalidCycle);
        }
        element.page_number = page_number;
        element.cycle_seconds = cycle_seconds;
        store
            .update(&element, expected_revision)
            .await
            .map_err(storage)?;
        Ok(())
    }

    /// Save the field bindings.
    /// # Errors
    /// Validation, stale-revision and storage failures.
    pub async fn save_teletext_element_bindings(
        &self,
        id: EntityId,
        expected_revision: i64,
        bindings: Vec<TeletextElementBinding>,
    ) -> Result<(), TeletextElementError> {
        let store = self.teletext_element_store()?;
        let mut element = store
            .get(id)
            .await
            .map_err(storage)?
            .ok_or(TeletextElementError::NotFound)?;
        validate_teletext_element_bindings(&bindings, usize::from(element.subpage_count))
            .map_err(|_| TeletextElementError::InvalidBindings)?;
        element.bindings = bindings;
        store
            .update(&element, expected_revision)
            .await
            .map_err(storage)?;
        Ok(())
    }

    /// Delete an element and its channel links.
    /// # Errors
    /// Storage failures.
    pub async fn delete_teletext_element(&self, id: EntityId) -> Result<(), TeletextElementError> {
        if self
            .teletext_element_store()?
            .delete(id)
            .await
            .map_err(storage)?
        {
            Ok(())
        } else {
            Err(TeletextElementError::NotFound)
        }
    }

    /// Channels that show an element.
    /// # Errors
    /// Storage failures.
    pub async fn teletext_element_usage(
        &self,
        id: EntityId,
    ) -> Result<Vec<TeletextElementUsage>, TeletextElementError> {
        self.teletext_element_store()?
            .usage(id)
            .await
            .map_err(storage)
    }

    /// A channel's custom pages, for the channel teletext page and page set.
    /// # Errors
    /// Storage failures.
    pub async fn channel_teletext_elements(
        &self,
        channel_id: EntityId,
    ) -> Result<Vec<ChannelTeletextElement>, TeletextElementError> {
        self.teletext_element_store()?
            .list_channel_elements(channel_id)
            .await
            .map_err(storage)
    }

    /// Link an element to a channel (or update the link). Two custom pages
    /// on one channel cannot share a page number. Add-on ranges are checked
    /// by the channel page set, which owns that knowledge.
    /// # Errors
    /// Validation and storage failures.
    pub async fn save_channel_teletext_element(
        &self,
        channel_id: EntityId,
        element_id: EntityId,
        page_number: Option<u16>,
        enabled: bool,
    ) -> Result<EntityId, TeletextElementError> {
        let store = self.teletext_element_store()?;
        if page_number.is_some_and(|page| !(0x100..=0x8ff).contains(&page)) {
            return Err(TeletextElementError::InvalidPageNumber);
        }
        let element = store
            .get(element_id)
            .await
            .map_err(storage)?
            .ok_or(TeletextElementError::NotFound)?;
        let effective = page_number.unwrap_or(element.page_number);
        let existing = store
            .list_channel_elements(channel_id)
            .await
            .map_err(storage)?;
        if existing
            .iter()
            .any(|link| link.element_id != element_id && link.effective_page_number == effective)
        {
            return Err(TeletextElementError::PageInUse);
        }
        store
            .save_channel_element(channel_id, element_id, page_number, enabled)
            .await
            .map_err(storage)
    }

    /// Remove a channel's link.
    /// # Errors
    /// Storage failures.
    pub async fn remove_channel_teletext_element(
        &self,
        channel_id: EntityId,
        link_id: EntityId,
    ) -> Result<(), TeletextElementError> {
        self.teletext_element_store()?
            .remove_channel_element(channel_id, link_id)
            .await
            .map_err(storage)?;
        Ok(())
    }

    /// Field values for one channel at one moment: name, number, local date
    /// and time, and the current and next programme from the playout.
    /// # Errors
    /// Storage failures; a channel without playout yields empty programmes.
    pub async fn teletext_element_field_values(
        &self,
        channel_id: EntityId,
        now: DateTime<Utc>,
    ) -> Result<Option<FieldValues>, TeletextElementError> {
        let state = self
            .ports()
            .channels()
            .get_watch_state(
                channel_id,
                now,
                now - chrono::Duration::hours(6),
                now + chrono::Duration::hours(24),
            )
            .await
            .map_err(storage)?;
        let Some(state) = state else {
            return Ok(None);
        };
        let local = now.with_timezone(&Local);
        let mut programmes: Vec<_> = state
            .programmes
            .iter()
            .filter(|programme| !programme.is_gap && programme.finish > now)
            .collect();
        programmes.sort_by_key(|programme| programme.start);
        let current = programmes
            .iter()
            .position(|programme| programme.start <= now);
        let (now_programme, next_programme) = match current {
            Some(index) => (programmes.get(index), programmes.get(index + 1)),
            None => (None, programmes.first()),
        };
        let start = |programme: Option<&&ersatzrs_core::repositories::ChannelWatchProgramme>| {
            programme.map_or_else(String::new, |programme| {
                programme
                    .start
                    .with_timezone(&Local)
                    .format("%H:%M")
                    .to_string()
            })
        };
        let title = |programme: Option<&&ersatzrs_core::repositories::ChannelWatchProgramme>| {
            programme.map_or_else(String::new, |programme| programme.title.clone())
        };
        Ok(Some(FieldValues {
            channel_name: state.name,
            channel_number: state.number,
            date: local.format("%d-%m-%Y").to_string(),
            time: local.format("%H:%M").to_string(),
            now_title: title(now_programme),
            now_start: start(now_programme),
            next_title: title(next_programme),
            next_start: start(next_programme),
        }))
    }

    /// Preview an element as designed, or filled for a channel.
    /// # Errors
    /// Not-found and storage failures.
    pub async fn preview_teletext_element(
        &self,
        id: EntityId,
        channel_id: Option<EntityId>,
        now: DateTime<Utc>,
    ) -> Result<TeletextElementPreview, TeletextElementError> {
        let element = self
            .get_teletext_element(id)
            .await?
            .ok_or(TeletextElementError::NotFound)?;
        let page = teletext_element_page(&element)?;
        let Some(channel_id) = channel_id else {
            return Ok(TeletextElementPreview { page, values: None });
        };
        let values = self.teletext_element_field_values(channel_id, now).await?;
        let page = values.as_ref().map_or_else(
            || page.clone(),
            |values| apply_field_values(&page, &element.bindings, values),
        );
        Ok(TeletextElementPreview { page, values })
    }
}

fn unique_name(base: &str, existing: &[String]) -> String {
    if !existing.iter().any(|name| name == base) {
        return base.to_owned();
    }
    (2..=existing.len() + 1)
        .map(|index| format!("{base} ({index})"))
        .find(|candidate| !existing.iter().any(|name| name == candidate))
        .unwrap_or_else(|| base.to_owned())
}

#[cfg(test)]
mod tests;
