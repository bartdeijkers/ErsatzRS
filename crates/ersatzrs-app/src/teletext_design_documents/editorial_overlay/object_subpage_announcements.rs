//! Private V26 dependency intent; numeric association does not authorize replay.
use super::super::MAX_BINDINGS;
use super::{
    invalid, limit, map_only, StoredOverlay, TeletextDesignDocumentBudget,
    TeletextDesignDocumentError, TeletextProfileDocument, TeletextSavedEditorialOverlay,
};
use ersatzrs_domain::{TeletextObjectSubpageAnnouncement, TeletextResourceIdentity};
use serde::{ser::SerializeSeq, Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeSet;

/// An explicit display/bank dependency tied to an immutable producer ordinal.
/// This structural role grants no source, package, editor or publication authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextSavedObjectSubpageAnnouncement {
    document_index: usize,
    announcement: TeletextObjectSubpageAnnouncement,
}
impl TeletextSavedObjectSubpageAnnouncement {
    /// Associate checked dependency intent with its original producer position.
    /// # Errors
    /// Rejects an out-of-range ordinal. Attachment checks actual source membership.
    pub fn new(
        document_index: usize,
        announcement: TeletextObjectSubpageAnnouncement,
    ) -> Result<Self, TeletextDesignDocumentError> {
        if document_index >= MAX_BINDINGS {
            return Err(invalid());
        }
        Ok(Self {
            document_index,
            announcement,
        })
    }
    #[must_use]
    pub const fn document_index(&self) -> usize {
        self.document_index
    }
    #[must_use]
    pub const fn announcement(&self) -> TeletextObjectSubpageAnnouncement {
        self.announcement
    }
}
impl TeletextSavedEditorialOverlay {
    /// Capture dependency-only intent without fabricating an original edit.
    /// # Errors
    /// Rejects invalid provenance and empty or duplicate dependency intent.
    pub fn new_with_object_subpage_announcements(
        provenance: super::TeletextSavedEditorialProvenance,
        announcements: Vec<TeletextSavedObjectSubpageAnnouncement>,
    ) -> Result<Self, TeletextDesignDocumentError> {
        Self::new_parts(provenance, vec![], vec![], vec![], vec![])?
            .with_object_subpage_announcements(announcements)
    }

    /// Retain explicit object-subpage dependencies beside existing editorial roles.
    /// # Errors
    /// Rejects empty/repeated attachment and duplicate display/resource owners.
    /// Native replay must still authenticate selection, effects and current state.
    pub fn with_object_subpage_announcements(
        mut self,
        announcements: Vec<TeletextSavedObjectSubpageAnnouncement>,
    ) -> Result<Self, TeletextDesignDocumentError> {
        if announcements.is_empty() || !self.object_subpage_announcements.is_empty() {
            return Err(invalid());
        }
        distinct(&announcements)?;
        self.object_subpage_announcements = announcements;
        Ok(self)
    }
    /// Stored dependency instructions, never implicit native admission authority.
    #[must_use]
    pub fn object_subpage_announcements(&self) -> &[TeletextSavedObjectSubpageAnnouncement] {
        &self.object_subpage_announcements
    }
}
fn distinct(
    values: &[TeletextSavedObjectSubpageAnnouncement],
) -> Result<(), TeletextDesignDocumentError> {
    let mut owners = BTreeSet::new();
    for value in values {
        if !owners.insert((value.announcement.display(), value.announcement.resource())) {
            return Err(invalid());
        }
    }
    Ok(())
}
#[derive(Serialize)]
pub(in super::super) struct StoredAnnouncement {
    document_index: usize,
    display: super::super::Entry,
    resource: TeletextResourceIdentity,
}
impl<'de> Deserialize<'de> for StoredAnnouncement {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            document_index: usize,
            display: super::super::Entry,
            resource: TeletextResourceIdentity,
        }
        let value: Fields = map_only(deserializer)?;
        Ok(Self {
            document_index: value.document_index,
            display: value.display,
            resource: value.resource,
        })
    }
}
#[derive(Default, Serialize)]
pub(super) struct OptionalStoredAnnouncements(pub(super) Option<Vec<StoredAnnouncement>>);
impl OptionalStoredAnnouncements {
    pub(super) fn is_absent(&self) -> bool {
        self.0.is_none()
    }
}
impl<'de> Deserialize<'de> for OptionalStoredAnnouncements {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Vec::<StoredAnnouncement>::deserialize(deserializer).map(|value| Self(Some(value)))
    }
}
pub(super) struct BorrowedAnnouncements<'a>(
    pub(super) &'a [TeletextSavedObjectSubpageAnnouncement],
);
impl Serialize for BorrowedAnnouncements<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for value in self.0 {
            sequence.serialize_element(&StoredAnnouncement {
                document_index: value.document_index,
                display: super::super::Entry {
                    page: value.announcement.display().page,
                    subcode: value.announcement.display().subcode,
                },
                resource: value.announcement.resource(),
            })?;
        }
        sequence.end()
    }
}
pub(super) fn check_version(stored: &StoredOverlay) -> Result<(), TeletextDesignDocumentError> {
    if stored.schema_version == super::OBJECT_SUBPAGE_ANNOUNCEMENT_VERSION {
        if stored
            .object_subpage_announcements
            .0
            .as_ref()
            .is_none_or(Vec::is_empty)
        {
            return Err(invalid());
        }
    } else if matches!(
        stored.schema_version,
        super::OBJECT_PAGE_SELECTION_VERSION
            | super::MAIN_RESOURCE_INVOCATION_VERSION
            | super::MAIN_PROGRAM_PRESENCE_VERSION
    ) {
        if stored
            .object_subpage_announcements
            .0
            .as_ref()
            .is_some_and(Vec::is_empty)
        {
            return Err(invalid());
        }
    } else if !stored.object_subpage_announcements.is_absent() {
        return Err(invalid());
    }
    Ok(())
}
pub(super) fn restore(
    values: Vec<StoredAnnouncement>,
) -> Result<Vec<TeletextSavedObjectSubpageAnnouncement>, TeletextDesignDocumentError> {
    values
        .into_iter()
        .map(|value| {
            TeletextSavedObjectSubpageAnnouncement::new(
                value.document_index,
                TeletextObjectSubpageAnnouncement::new(
                    ersatzrs_domain::TeletextPageAddress {
                        page: value.display.page,
                        subcode: value.display.subcode,
                    },
                    value.resource,
                )
                .map_err(|_| invalid())?,
            )
        })
        .collect()
}
pub(super) fn validate_association(
    overlay: &TeletextSavedEditorialOverlay,
    documents: &[TeletextProfileDocument],
    budget: TeletextDesignDocumentBudget,
) -> Result<(), TeletextDesignDocumentError> {
    let values = overlay.object_subpage_announcements();
    distinct(values)?;
    limit(
        values.len(),
        budget.validation.service.max_structure_items,
        "editorial object subpage announcements",
    )?;
    for value in values {
        let TeletextProfileDocument::Authored(service) =
            documents.get(value.document_index).ok_or_else(invalid)?
        else {
            return Err(invalid());
        };
        let display = value.announcement.display();
        let resource = value.announcement.resource();
        super::unique_original_owner(display, documents)?;
        if !service
            .semantic()
            .base()
            .pages
            .iter()
            .any(|page| page.address == display)
            || !(service.resource_inventory().contains(&resource)
                || overlay.object_resource_additions().iter().any(|bank| {
                    bank.document_index() == value.document_index
                        && bank.addition().resource() == resource
                }))
        {
            return Err(invalid());
        }
    }
    Ok(())
}

pub(super) fn structure_items(
    overlay: &TeletextSavedEditorialOverlay,
) -> Result<usize, TeletextDesignDocumentError> {
    overlay
        .pages
        .len()
        .checked_add(overlay.object_subpage_announcements.len())
        .ok_or_else(invalid)
}
