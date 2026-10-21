//! Private V27 object/V30 DRCS exact-old role intent; storage is not replay authority.
use super::super::MAX_BINDINGS;
use super::{
    invalid, limit, StoredOverlay, TeletextDesignDocumentBudget, TeletextDesignDocumentError,
    TeletextProfileDocument, TeletextSavedEditorialOverlay, TeletextSavedEditorialProvenance,
};
use ersatzrs_domain::{
    TeletextObjectPageSelectionChange, TeletextResourceKind, TeletextX27LinkRole,
    TeletextX27LinkTarget,
};
use serde::{ser::SerializeSeq, Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeSet;
#[path = "object_page_selections/wire.rs"]
mod wire;
pub(in super::super) use wire::StoredSelection;

/// Exact typed selection associated with an immutable producer-document ordinal.
/// Its original role/provenance is retained, never inferred from editor input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextSavedObjectPageSelectionChange {
    document_index: usize,
    change: TeletextObjectPageSelectionChange,
}
impl TeletextSavedObjectPageSelectionChange {
    /// # Errors
    /// Rejects an out-of-range ordinal; attachment checks source membership.
    pub fn new(
        document_index: usize,
        change: TeletextObjectPageSelectionChange,
    ) -> Result<Self, TeletextDesignDocumentError> {
        if document_index >= MAX_BINDINGS {
            return Err(invalid());
        }
        Ok(Self {
            document_index,
            change,
        })
    }
    #[must_use]
    pub const fn document_index(&self) -> usize {
        self.document_index
    }
    #[must_use]
    pub const fn change(&self) -> TeletextObjectPageSelectionChange {
        self.change
    }
}
impl TeletextSavedEditorialOverlay {
    /// Capture selection-only intent without fabricating an original edit.
    /// # Errors
    /// Rejects invalid provenance and empty or duplicate selection intent.
    pub fn new_with_object_page_selections(
        provenance: TeletextSavedEditorialProvenance,
        selections: Vec<TeletextSavedObjectPageSelectionChange>,
    ) -> Result<Self, TeletextDesignDocumentError> {
        Self::new_parts(provenance, vec![], vec![], vec![], vec![])?
            .with_object_page_selections(selections)
    }
    /// Compose exact-old selection intent with retained editorial roles.
    /// # Errors
    /// Rejects empty/repeated attachment and duplicate display/role owners.
    pub fn with_object_page_selections(
        mut self,
        selections: Vec<TeletextSavedObjectPageSelectionChange>,
    ) -> Result<Self, TeletextDesignDocumentError> {
        if selections.is_empty() || !self.object_page_selections.is_empty() {
            return Err(invalid());
        }
        distinct(&selections)?;
        self.object_page_selections = selections;
        Ok(self)
    }
    pub(super) const fn has_drcs_page_selections(&self) -> bool {
        let selections = self.object_page_selections.as_slice();
        let mut index = 0;
        while index < selections.len() {
            if matches!(
                selections[index].change.role(),
                TeletextX27LinkRole::Drcs | TeletextX27LinkRole::Gdrcs
            ) {
                return true;
            }
            index += 1;
        }
        false
    }
    /// Typed instructions; native replay still authenticates originals and effects.
    #[must_use]
    pub fn object_page_selections(&self) -> &[TeletextSavedObjectPageSelectionChange] {
        &self.object_page_selections
    }
}
fn distinct(
    values: &[TeletextSavedObjectPageSelectionChange],
) -> Result<(), TeletextDesignDocumentError> {
    let mut seen = BTreeSet::new();
    for value in values {
        if !seen.insert((value.change.display(), value.change.role().wire_code())) {
            return Err(invalid());
        }
    }
    Ok(())
}
#[derive(Default, Serialize)]
pub(super) struct OptionalStoredSelections(pub(super) Option<Vec<StoredSelection>>);
impl OptionalStoredSelections {
    pub(super) fn is_absent(&self) -> bool {
        self.0.is_none()
    }
}
impl<'de> Deserialize<'de> for OptionalStoredSelections {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Vec::<StoredSelection>::deserialize(d).map(|v| Self(Some(v)))
    }
}
pub(super) struct BorrowedSelections<'a>(pub(super) &'a [TeletextSavedObjectPageSelectionChange]);
impl Serialize for BorrowedSelections<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for value in self.0 {
            sequence.serialize_element(&StoredSelection::from(*value))?;
        }
        sequence.end()
    }
}
pub(super) fn check_version(stored: &StoredOverlay) -> Result<(), TeletextDesignDocumentError> {
    let has_drcs = stored
        .object_page_selections
        .0
        .as_ref()
        .is_some_and(|values| values.iter().any(StoredSelection::is_drcs));
    // Historical wire enums listed DRCS, but V27–V29 never admitted those roles.
    if (stored.schema_version == super::DRCS_PAGE_SELECTION_VERSION) != has_drcs {
        return Err(invalid());
    }
    if matches!(
        stored.schema_version,
        super::OBJECT_PAGE_SELECTION_VERSION | super::DRCS_PAGE_SELECTION_VERSION
    ) {
        if stored
            .object_page_selections
            .0
            .as_ref()
            .is_none_or(Vec::is_empty)
        {
            return Err(invalid());
        }
    } else if matches!(
        stored.schema_version,
        super::MAIN_RESOURCE_INVOCATION_VERSION | super::MAIN_PROGRAM_PRESENCE_VERSION
    ) {
        if stored
            .object_page_selections
            .0
            .as_ref()
            .is_some_and(Vec::is_empty)
        {
            return Err(invalid());
        }
    } else if !stored.object_page_selections.is_absent() {
        return Err(invalid());
    }
    Ok(())
}
pub(super) fn restore(
    values: Vec<StoredSelection>,
) -> Result<Vec<TeletextSavedObjectPageSelectionChange>, TeletextDesignDocumentError> {
    values.into_iter().map(TryInto::try_into).collect()
}
pub(super) fn validate_association(
    overlay: &TeletextSavedEditorialOverlay,
    documents: &[TeletextProfileDocument],
    budget: TeletextDesignDocumentBudget,
) -> Result<(), TeletextDesignDocumentError> {
    distinct(overlay.object_page_selections())?;
    limit(
        overlay.object_page_selections().len(),
        budget.validation.service.max_structure_items,
        "editorial object page selections",
    )?;
    for value in overlay.object_page_selections() {
        let TeletextProfileDocument::Authored(service) =
            documents.get(value.document_index).ok_or_else(invalid)?
        else {
            return Err(invalid());
        };
        let change = value.change;
        let display = change.display();
        super::unique_original_owner(display, documents)?;
        if !service
            .semantic()
            .base()
            .pages
            .iter()
            .any(|page| page.address == display)
        {
            return Err(invalid());
        }
        let TeletextX27LinkTarget::Page { page } = change.replacement().target() else {
            return Err(invalid());
        };
        let kind = change.role().kind();
        // Structural only: prior announcements can alter the expected X27 body.
        // Exact original role/link equality belongs to authenticated native replay.
        let exists = |resource: ersatzrs_domain::TeletextResourceIdentity| {
            resource.kind() == kind && resource.page() == page
        };
        let resources = service
            .resource_inventory()
            .iter()
            .copied()
            .chain(
                overlay
                    .object_resource_additions()
                    .iter()
                    // New object-bank authority never creates a DRCS source bank.
                    .filter(|bank| {
                        matches!(kind, TeletextResourceKind::Pop | TeletextResourceKind::Gpop)
                            && bank.document_index() == value.document_index
                    })
                    .map(|bank| bank.addition().resource()),
            )
            .filter(|resource| exists(*resource))
            .collect::<Vec<_>>();
        if resources.is_empty() {
            return Err(invalid());
        }
        let mask = change.replacement().required_subpages();
        for subpage in 0..16 {
            let subpage = ersatzrs_domain::TeletextResourceSubpageIndex::new(subpage)
                .map_err(|_| invalid())?;
            if mask.requires(subpage)
                && !resources
                    .iter()
                    .any(|resource| resource.logical_subpage() == subpage)
            {
                return Err(invalid());
            }
        }
    }
    Ok(())
}

pub(super) fn structure_items(
    overlay: &TeletextSavedEditorialOverlay,
) -> Result<usize, TeletextDesignDocumentError> {
    super::object_subpage_announcements::structure_items(overlay)?
        .checked_add(overlay.object_page_selections.len())
        .ok_or_else(invalid)
}
