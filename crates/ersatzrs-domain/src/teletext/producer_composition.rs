//! Checked cold composition retaining owned producer contexts (ADR 0123).
//! Lifecycle, warm-cache equivalence, publication and client readiness are separate.

use super::{
    TeletextDeclaredDefaultReservation, TeletextDeclaredService, TeletextDeclaredServiceBudget,
    TeletextDeclaredServiceError, TeletextDisplayMotSelection, TeletextDrcsGlyphDefinition,
    TeletextDrcsPageLinks, TeletextDrcsSubtable, TeletextEffectivePresentation,
    TeletextEnhancementCommand, TeletextEnhancementObjectDefinition, TeletextLanguage,
    TeletextLevel25PresentationDefaults, TeletextMagazinePresentationDraft,
    TeletextMotDefaultObject, TeletextMotDrcsLink, TeletextMotLevel25Declaration,
    TeletextMotObjectLink, TeletextMotPageAssociation, TeletextMotPublicLinkIndex,
    TeletextNavigationLink, TeletextObjectCatalogError, TeletextPageAddress, TeletextPageDraft,
    TeletextPageSetDraft, TeletextPresentationSource, TeletextProducerLocalProgramPresence,
    TeletextProducerPagePresentation, TeletextProducerPresentation,
    TeletextProducerPresentationSource, TeletextProducerSecondarySelection,
    TeletextRawDataPageNumber, TeletextResolvedObjectPageLinks,
    TeletextResolvedPageEnhancementDraft, TeletextResolvedPageResourceSelection,
    TeletextResolvedServiceDraft, TeletextResourceIdentity, TeletextResourceKind,
    TeletextResourceSubpageIndex, TeletextRow, TeletextRun, TeletextRunContent,
    TeletextSemanticService, TeletextServiceBudget, TeletextServiceError, TeletextStructureError,
    TeletextX27LinkRole,
};
use std::{collections::BTreeMap, fmt, mem::size_of};

/// Explicit host service metadata, separate from invocation receipts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextProducerCompositionMetadata {
    pub language: TeletextLanguage,
    pub entry: TeletextPageAddress,
}
/// Raw input limits precede coalescing; final model limits count actual output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextProducerCompositionBudget {
    pub max_producers: usize,
    /// All nested input records, shared banks and fixed MOT links/slots included.
    pub max_input_items: usize,
    /// Original text, mosaic cells, language bytes and physical overlay cells,
    /// without truncation.
    pub max_input_content_bytes: usize,
    /// Local/object commands and supplied/actual populated default slots.
    pub max_input_commands: usize,
    /// Conservative clone/index storage before copying composed content.
    pub max_storage_bytes: usize,
    /// Visited records and compared commands/values, including shared banks.
    pub max_work: usize,
    pub service: TeletextServiceBudget,
    pub declarations: TeletextDeclaredServiceBudget,
}
/// The only presentation transformation performed by this cold factory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextProducerPageDerivation {
    Unchanged,
    MagazineToPage { magazine: u8 },
}
/// Original source ordinal and exact address, without namespace remapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextComposedProducerPage {
    address: TeletextPageAddress,
    producer: usize,
    page: usize,
    derivation: TeletextProducerPageDerivation,
}
impl TeletextComposedProducerPage {
    #[must_use]
    pub const fn address(&self) -> TeletextPageAddress {
        self.address
    }
    #[must_use]
    pub const fn producer_ordinal(&self) -> usize {
        self.producer
    }
    #[must_use]
    pub const fn page_ordinal(&self) -> usize {
        self.page
    }
    #[must_use]
    pub const fn derivation(&self) -> TeletextProducerPageDerivation {
        self.derivation
    }
}
/// Numeric locations; diagnostics contain no original document text.
#[derive(Debug, thiserror::Error)]
#[error("cold teletext composition at producer {producer:?}, page {page:?}, display {display:?}, resource {resource:?}: {reason:?}")]
pub struct TeletextProducerCompositionError {
    pub producer: Option<usize>,
    pub page: Option<usize>,
    pub display: Option<TeletextPageAddress>,
    pub resource: Option<TeletextResourceIdentity>,
    pub reason: TeletextProducerCompositionFailure,
}
#[derive(Debug)]
pub enum TeletextProducerCompositionFailure {
    Empty,
    Budget {
        category: &'static str,
        count: usize,
        maximum: usize,
    },
    StorageOverflow,
    MissingEntry,
    DuplicateDisplay {
        first_producer: usize,
        first_page: usize,
    },
    ResourceConflict {
        first_producer: usize,
    },
    /// Different same-magazine banks need a future checked lowering stage.
    MotConflict {
        first_producer: usize,
        first_mot: TeletextResourceIdentity,
    },
    Snapshot,
    Context {
        category: &'static str,
    },
    Semantic(Box<TeletextServiceError>),
    Declarations(TeletextDeclaredServiceError),
}
/// Immutable cold candidate owning all original contributions unchanged.
pub struct TeletextColdProducerComposition {
    producers: Vec<TeletextProducerPresentation>,
    pages: Vec<TeletextComposedProducerPage>,
    metadata: TeletextProducerCompositionMetadata,
    composed: TeletextDeclaredService,
}
impl TeletextColdProducerComposition {
    /// Compose without inventing receiver defaults or resource namespaces.
    /// # Errors
    /// Raw storage/count fences precede indexes and content copies. Collisions,
    /// incompatible banks and changed original effects yield no partial result.
    /// Actual encoded-header collisions and warm-cache behavior remain unproven.
    pub fn new(
        producers: Vec<TeletextProducerPresentation>,
        metadata: TeletextProducerCompositionMetadata,
        budget: TeletextProducerCompositionBudget,
    ) -> Result<Self, TeletextProducerCompositionError> {
        let mut work = Preflight::new(&budget);
        preflight(&producers, &metadata, &mut work)?;
        let indexes = index_inputs(&producers, &metadata, &mut work)?;
        let magazines = common_magazines(&producers, &mut work)?;
        let ComposedInputs {
            draft,
            pages,
            companions,
            mots,
            selections,
            overlays,
        } = compose_inputs(&producers, &metadata, &indexes, &magazines, &mut work)?;
        let semantic = TeletextSemanticService::new_with_object_companions(
            draft,
            companions.clone(),
            budget.service,
        )
        .map_err(|reason| semantic_error(reason, &indexes))?;
        let semantic = semantic
            .with_physical_base_overlays(overlays, budget.service)
            .map_err(|reason| semantic_error(reason, &indexes))?;
        let composed = TeletextDeclaredService::new(
            semantic,
            mots,
            selections,
            companions,
            budget.declarations,
        )
        .map_err(|reason| locate_error(declaration_error(reason), &indexes))?;
        check_contexts(&producers, &pages, &composed, &mut work)?;
        Ok(Self {
            producers,
            pages,
            metadata,
            composed,
        })
    }
    #[must_use]
    pub fn producers(&self) -> &[TeletextProducerPresentation] {
        &self.producers
    }
    #[must_use]
    pub fn pages(&self) -> &[TeletextComposedProducerPage] {
        &self.pages
    }
    #[must_use]
    pub fn provenance(&self) -> &[TeletextComposedProducerPage] {
        &self.pages
    }
    #[must_use]
    pub const fn composed(&self) -> &TeletextDeclaredService {
        &self.composed
    }
    /// Replace only the composed service's physical rows. Original producer
    /// contributions and their presentation contexts remain immutable. Existing
    /// input, storage and work fences cover retained originals and new buffers
    /// before any replacement index is allocated.
    /// # Errors
    /// Rejects invalid overlay owners or cold/combined service budget overflow.
    pub fn with_physical_base_overlays(
        mut self,
        overlays: Vec<super::TeletextPhysicalBaseOverlay>,
        budget: TeletextProducerCompositionBudget,
    ) -> Result<Self, TeletextProducerCompositionError> {
        let mut work = Preflight::new(&budget);
        preflight(&self.producers, &self.metadata, &mut work)?;
        preflight_overlays(overlays.iter(), overlays.len(), &mut work)?;
        self.composed = self
            .composed
            .with_physical_base_overlays(overlays, budget.service)
            .map_err(|reason| {
                let display = match &reason {
                    TeletextServiceError::PhysicalBaseOverlayOwner { address, .. } => {
                        Some(*address)
                    }
                    _ => None,
                };
                let owner = display
                    .and_then(|address| self.pages.iter().find(|page| page.address == address));
                TeletextProducerCompositionError {
                    producer: owner.map(TeletextComposedProducerPage::producer_ordinal),
                    page: owner.map(TeletextComposedProducerPage::page_ordinal),
                    display,
                    ..failure(TeletextProducerCompositionFailure::Semantic(Box::new(
                        reason,
                    )))
                }
            })?;
        Ok(self)
    }
    #[must_use]
    pub const fn metadata(&self) -> &TeletextProducerCompositionMetadata {
        &self.metadata
    }
    #[must_use]
    pub const fn entry(&self) -> TeletextPageAddress {
        self.metadata.entry
    }
    /// Move the declaration to encoding while retaining original associations.
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        Vec<TeletextProducerPresentation>,
        Vec<TeletextComposedProducerPage>,
        TeletextProducerCompositionMetadata,
        TeletextDeclaredService,
    ) {
        (self.producers, self.pages, self.metadata, self.composed)
    }
}
impl fmt::Debug for TeletextColdProducerComposition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TeletextColdProducerComposition")
            .field("producers", &self.producers.len())
            .field("pages", &self.pages.len())
            .field("entry", &self.metadata.entry)
            .finish_non_exhaustive()
    }
}
fn failure(reason: TeletextProducerCompositionFailure) -> TeletextProducerCompositionError {
    TeletextProducerCompositionError {
        producer: None,
        page: None,
        display: None,
        resource: None,
        reason,
    }
}
fn location(
    producer: usize,
    page: usize,
    display: TeletextPageAddress,
    reason: TeletextProducerCompositionFailure,
) -> TeletextProducerCompositionError {
    TeletextProducerCompositionError {
        producer: Some(producer),
        page: Some(page),
        display: Some(display),
        ..failure(reason)
    }
}
fn resource_error(
    producer: usize,
    resource: TeletextResourceIdentity,
    reason: TeletextProducerCompositionFailure,
) -> TeletextProducerCompositionError {
    TeletextProducerCompositionError {
        producer: Some(producer),
        resource: Some(resource),
        ..failure(reason)
    }
}
fn declaration_error(reason: TeletextDeclaredServiceError) -> TeletextProducerCompositionError {
    TeletextProducerCompositionError {
        display: reason.display,
        resource: reason.resource.or(reason.mot),
        ..failure(TeletextProducerCompositionFailure::Declarations(reason))
    }
}
fn locate_error(
    mut error: TeletextProducerCompositionError,
    indexes: &InputIndexes,
) -> TeletextProducerCompositionError {
    if let Some((producer, page)) = error
        .display
        .and_then(|display| indexes.displays.get(&display))
        .copied()
    {
        error.producer = Some(producer);
        error.page = Some(page);
    } else if let Some(owner) = error.resource.and_then(|resource| {
        indexes
            .resources
            .get(&(resource.page(), resource.logical_subpage()))
    }) {
        error.producer = Some(owner.producer);
    }
    error
}
fn semantic_error(
    reason: TeletextServiceError,
    indexes: &InputIndexes,
) -> TeletextProducerCompositionError {
    let display = match &reason {
        TeletextServiceError::Base(
            TeletextStructureError::DuplicatePage { address }
            | TeletextStructureError::Row { address, .. }
            | TeletextStructureError::Run { address, .. }
            | TeletextStructureError::Overlap { address, .. }
            | TeletextStructureError::DoubleHeight { address, .. }
            | TeletextStructureError::Navigation { address, .. }
            | TeletextStructureError::MissingTarget { address, .. },
        )
        | TeletextServiceError::DuplicatePage { address }
        | TeletextServiceError::MissingPage { address }
        | TeletextServiceError::PhysicalBaseOverlayOwner { address, .. } => Some(*address),
        TeletextServiceError::MissingLocalOwner { identity }
        | TeletextServiceError::LocalPresence { identity } => identity.owner(),
        TeletextServiceError::ObjectLink { display, .. }
        | TeletextServiceError::CompositionalResource { display, .. }
        | TeletextServiceError::CompositionalUsage { display, .. }
        | TeletextServiceError::DefaultScope { display, .. }
        | TeletextServiceError::ObjectSelection { display, .. }
        | TeletextServiceError::Charset { display, .. } => Some(*display),
        TeletextServiceError::Collection { error, .. } => Some(error.display),
        _ => None,
    };
    let resource = match &reason {
        TeletextServiceError::ResourceCollision { page, subpage } => indexes
            .resources
            .get(&(*page, *subpage))
            .map(|owner| owner.identity),
        TeletextServiceError::ObjectCompanionKind { identity } => Some(*identity),
        TeletextServiceError::Objects(TeletextObjectCatalogError::Duplicate { identity }) => {
            identity.resource()
        }
        TeletextServiceError::Objects(TeletextObjectCatalogError::Invocation { owner, .. }) => {
            owner.resource()
        }
        _ => None,
    };
    locate_error(
        TeletextProducerCompositionError {
            display,
            resource,
            ..failure(TeletextProducerCompositionFailure::Semantic(Box::new(
                reason,
            )))
        },
        indexes,
    )
}
struct Preflight<'a> {
    budget: &'a TeletextProducerCompositionBudget,
    items: usize,
    content: usize,
    commands: usize,
    storage: usize,
    work: usize,
    producer: Option<usize>,
}
impl<'a> Preflight<'a> {
    fn new(budget: &'a TeletextProducerCompositionBudget) -> Self {
        Self {
            budget,
            items: 0,
            content: 0,
            commands: 0,
            storage: 0,
            work: 0,
            producer: None,
        }
    }
    fn charge(
        &self,
        count: usize,
        amount: usize,
        maximum: usize,
        category: &'static str,
    ) -> Result<usize, TeletextProducerCompositionError> {
        let next = count
            .checked_add(amount)
            .ok_or_else(|| failure(TeletextProducerCompositionFailure::StorageOverflow))?;
        if next > maximum {
            return Err(TeletextProducerCompositionError {
                producer: self.producer,
                ..failure(TeletextProducerCompositionFailure::Budget {
                    category,
                    count: next,
                    maximum,
                })
            });
        }
        Ok(next)
    }
    fn records<T>(&mut self, count: usize) -> Result<(), TeletextProducerCompositionError> {
        self.items = self.charge(self.items, count, self.budget.max_input_items, "input item")?;
        // Charge raw duplicate banks too. The fixed overhead conservatively
        // reserves indexes/provenance in addition to clone payload storage.
        let bytes = count
            .checked_mul(
                size_of::<T>()
                    .checked_add(128)
                    .ok_or_else(|| failure(TeletextProducerCompositionFailure::StorageOverflow))?,
            )
            .ok_or_else(|| failure(TeletextProducerCompositionFailure::StorageOverflow))?;
        self.bytes(bytes)?;
        self.visit_sum(count, count)
    }
    fn bytes(&mut self, count: usize) -> Result<(), TeletextProducerCompositionError> {
        self.storage = self.charge(
            self.storage,
            count,
            self.budget.max_storage_bytes,
            "clone storage",
        )?;
        if isize::try_from(self.storage).is_err() {
            return Err(failure(TeletextProducerCompositionFailure::StorageOverflow));
        }
        Ok(())
    }
    fn content(&mut self, count: usize) -> Result<(), TeletextProducerCompositionError> {
        self.content = self.charge(
            self.content,
            count,
            self.budget.max_input_content_bytes,
            "input content byte",
        )?;
        self.bytes(count)?;
        self.visit_sum(count, count)
    }
    fn commands(&mut self, count: usize) -> Result<(), TeletextProducerCompositionError> {
        self.commands = self.charge(
            self.commands,
            count,
            self.budget.max_input_commands,
            "input command",
        )?;
        self.records::<TeletextEnhancementCommand>(count)
    }
    fn visit_sum(
        &mut self,
        left: usize,
        right: usize,
    ) -> Result<(), TeletextProducerCompositionError> {
        let count = left
            .checked_add(right)
            .ok_or_else(|| failure(TeletextProducerCompositionFailure::StorageOverflow))?;
        self.visit(count)
    }
    fn visit(&mut self, count: usize) -> Result<(), TeletextProducerCompositionError> {
        self.work = self.charge(self.work, count, self.budget.max_work, "work")?;
        Ok(())
    }
}
fn preflight(
    producers: &[TeletextProducerPresentation],
    metadata: &TeletextProducerCompositionMetadata,
    work: &mut Preflight<'_>,
) -> Result<(), TeletextProducerCompositionError> {
    work.charge(0, producers.len(), work.budget.max_producers, "producer")?;
    if producers.is_empty() {
        return Err(failure(TeletextProducerCompositionFailure::Empty));
    }
    work.records::<TeletextProducerPresentation>(producers.len())?;
    work.content(metadata.language.as_str().len())?;
    for (ordinal, producer) in producers.iter().enumerate() {
        work.producer = Some(ordinal);
        preflight_producer(producer, work)?;
    }
    work.producer = None;
    Ok(())
}
fn preflight_producer(
    producer: &TeletextProducerPresentation,
    work: &mut Preflight<'_>,
) -> Result<(), TeletextProducerCompositionError> {
    let original = producer.original();
    let service = original.semantic();
    let pages = &service.base().pages;
    work.records::<TeletextPageDraft>(pages.len())?;
    work.records::<TeletextComposedProducerPage>(pages.len())?;
    // Includes potential derived metadata for originally metadata-less pages.
    work.records::<TeletextResolvedPageEnhancementDraft>(pages.len())?;
    work.records::<TeletextProducerPagePresentation>(producer.pages().len())?;
    work.content(service.base().language.as_str().len())?;
    for page in pages {
        preflight_page(page, work)?;
    }
    preflight_overlays(
        service.physical_base_overlays().values(),
        service.physical_base_overlays().len(),
        work,
    )?;
    work.records::<TeletextMagazinePresentationDraft>(service.magazine_presentations().len())?;
    for metadata in service.page_enhancements().values() {
        if let Some(program) = &metadata.local_program {
            work.commands(program.commands().len())?;
        }
        work.commands(metadata.resources.defaults.iter().flatten().count())?;
    }
    work.records::<TeletextEnhancementObjectDefinition>(service.objects().definitions().len())?;
    for object in service.objects().definitions() {
        work.commands(object.program.commands().len())?;
    }
    work.records::<TeletextDrcsSubtable>(service.drcs().subtables().len())?;
    for table in service.drcs().subtables() {
        work.records::<TeletextDrcsGlyphDefinition>(table.glyphs().len())?;
    }
    work.records::<TeletextResourceIdentity>(original.resource_inventory().len())?;
    // Companions go into both the semantic model and checked declaration.
    work.records::<TeletextResourceIdentity>(original.empty_object_pages().len())?;
    work.records::<TeletextResourceIdentity>(original.empty_object_pages().len())?;
    work.records::<TeletextDisplayMotSelection>(original.selections().len())?;
    work.records::<TeletextDeclaredDefaultReservation>(original.defaults().len())?;
    preflight_mots(original.mots(), work)
}
fn preflight_overlays<'a>(
    overlays: impl Iterator<Item = &'a super::TeletextPhysicalBaseOverlay>,
    count: usize,
    work: &mut Preflight<'_>,
) -> Result<(), TeletextProducerCompositionError> {
    work.records::<super::TeletextPhysicalBaseOverlay>(count)?;
    for overlay in overlays {
        work.records::<super::TeletextPhysicalBaseRow>(overlay.rows().len())?;
        for _ in overlay.rows() {
            work.content(40)?;
        }
    }
    Ok(())
}
fn preflight_page(
    page: &TeletextPageDraft,
    work: &mut Preflight<'_>,
) -> Result<(), TeletextProducerCompositionError> {
    work.content(page.language.as_str().len())?;
    work.records::<TeletextRow>(page.rows.len())?;
    work.records::<TeletextNavigationLink>(page.navigation.len())?;
    for row in &page.rows {
        work.records::<TeletextRun>(row.runs.len())?;
        for run in &row.runs {
            work.content(match &run.content {
                TeletextRunContent::Text { text } => text.len(),
                TeletextRunContent::Mosaics { cells, .. } => cells.len(),
            })?;
        }
    }
    Ok(())
}
fn preflight_mots(
    mots: &[TeletextMotLevel25Declaration],
    work: &mut Preflight<'_>,
) -> Result<(), TeletextProducerCompositionError> {
    work.records::<TeletextMotLevel25Declaration>(mots.len())?;
    for mot in mots {
        work.records::<TeletextMotPageAssociation>(mot.associations().len())?;
        work.records::<TeletextMotObjectLink>(8)?;
        work.records::<TeletextMotDrcsLink>(8)?;
        work.records::<TeletextMotDefaultObject>(16)?;
        for index in std::iter::once(None).chain(TeletextMotPublicLinkIndex::ALL.map(Some)) {
            work.commands(
                mot.links()
                    .object_link(index)
                    .defaults()
                    .iter()
                    .filter(|slot| matches!(slot, TeletextMotDefaultObject::Object { .. }))
                    .count(),
            )?;
        }
    }
    Ok(())
}
type ResourceKey = (TeletextRawDataPageNumber, TeletextResourceSubpageIndex);
#[derive(Clone, Copy)]
struct ResourceOwner {
    producer: usize,
    identity: TeletextResourceIdentity,
}
struct InputIndexes {
    displays: BTreeMap<TeletextPageAddress, (usize, usize)>,
    resources: BTreeMap<ResourceKey, ResourceOwner>,
    mots: [Option<ResourceOwner>; 8],
}
fn index_inputs(
    producers: &[TeletextProducerPresentation],
    metadata: &TeletextProducerCompositionMetadata,
    work: &mut Preflight<'_>,
) -> Result<InputIndexes, TeletextProducerCompositionError> {
    let mut displays = BTreeMap::new();
    let mut resources = BTreeMap::new();
    let mut mots: [Option<ResourceOwner>; 8] = [None; 8];
    for (ordinal, producer) in producers.iter().enumerate() {
        work.producer = Some(ordinal);
        for (page, value) in producer
            .original()
            .semantic()
            .base()
            .pages
            .iter()
            .enumerate()
        {
            work.visit(1)?;
            if let Some((first_producer, first_page)) =
                displays.insert(value.address, (ordinal, page))
            {
                return Err(location(
                    ordinal,
                    page,
                    value.address,
                    TeletextProducerCompositionFailure::DuplicateDisplay {
                        first_producer,
                        first_page,
                    },
                ));
            }
        }
        for mot in producer.original().mots() {
            work.visit_sum(mot.associations().len(), 1)?;
            let index = usize::from(mot.identity().page().magazine() - 1);
            if let Some(first) = mots[index] {
                work.visit(producers[first.producer].original().mots().len())?;
                work.visit_sum(mot.associations().len(), 16)?;
                if first.identity != mot.identity()
                    || find_mot(&producers[first.producer], first.identity) != Some(mot)
                {
                    return Err(resource_error(
                        ordinal,
                        mot.identity(),
                        TeletextProducerCompositionFailure::MotConflict {
                            first_producer: first.producer,
                            first_mot: first.identity,
                        },
                    ));
                }
            } else {
                mots[index] = Some(ResourceOwner {
                    producer: ordinal,
                    identity: mot.identity(),
                });
            }
        }
        for &identity in producer.original().resource_inventory() {
            work.visit(1)?;
            let key = (identity.page(), identity.logical_subpage());
            if let Some(first) = resources.get(&key).copied() {
                if !same_resource(producers, first, ordinal, identity, work)? {
                    return Err(resource_error(
                        ordinal,
                        identity,
                        TeletextProducerCompositionFailure::ResourceConflict {
                            first_producer: first.producer,
                        },
                    ));
                }
            } else {
                resources.insert(
                    key,
                    ResourceOwner {
                        producer: ordinal,
                        identity,
                    },
                );
            }
        }
    }
    work.producer = None;
    if !displays.contains_key(&metadata.entry) {
        return Err(TeletextProducerCompositionError {
            display: Some(metadata.entry),
            ..failure(TeletextProducerCompositionFailure::MissingEntry)
        });
    }
    Ok(InputIndexes {
        displays,
        resources,
        mots,
    })
}
fn find_mot(
    producer: &TeletextProducerPresentation,
    identity: TeletextResourceIdentity,
) -> Option<&TeletextMotLevel25Declaration> {
    producer
        .original()
        .mots()
        .iter()
        .find(|mot| mot.identity() == identity)
}
fn same_resource(
    producers: &[TeletextProducerPresentation],
    first: ResourceOwner,
    ordinal: usize,
    identity: TeletextResourceIdentity,
    work: &mut Preflight<'_>,
) -> Result<bool, TeletextProducerCompositionError> {
    if first.identity != identity {
        return Ok(false);
    }
    let left = producers[first.producer].original();
    let right = producers[ordinal].original();
    match identity.kind() {
        TeletextResourceKind::Mot => {
            work.visit_sum(left.mots().len(), right.mots().len())?;
            let a = find_mot(&producers[first.producer], identity);
            let b = find_mot(&producers[ordinal], identity);
            work.visit_sum(
                a.map_or(0, |mot| mot.associations().len()),
                b.map_or(0, |mot| mot.associations().len()),
            )?;
            work.visit(32)?;
            Ok(a == b)
        }
        TeletextResourceKind::Drcs | TeletextResourceKind::Gdrcs => {
            work.visit_sum(
                left.semantic().drcs().subtables().len(),
                right.semantic().drcs().subtables().len(),
            )?;
            let a = left
                .semantic()
                .drcs()
                .subtables()
                .iter()
                .find(|table| table.identity() == identity);
            let b = right
                .semantic()
                .drcs()
                .subtables()
                .iter()
                .find(|table| table.identity() == identity);
            work.visit_sum(
                a.map_or(0, |table| table.glyphs().len()),
                b.map_or(0, |table| table.glyphs().len()),
            )?;
            Ok(a == b)
        }
        TeletextResourceKind::Pop | TeletextResourceKind::Gpop => {
            same_objects(left, right, identity, work)
        }
        // No owned MIP payload exists in the current checked service model.
        // Identity equality alone cannot establish a shared resource body.
        TeletextResourceKind::Mip => Ok(false),
    }
}
fn same_objects(
    left: &TeletextDeclaredService,
    right: &TeletextDeclaredService,
    identity: TeletextResourceIdentity,
    work: &mut Preflight<'_>,
) -> Result<bool, TeletextProducerCompositionError> {
    let a = left.semantic().objects().definitions();
    let b = right.semantic().objects().definitions();
    work.visit_sum(a.len(), b.len())?;
    let mut a = a
        .iter()
        .filter(|object| object.identity.resource() == Some(identity));
    let mut b = b
        .iter()
        .filter(|object| object.identity.resource() == Some(identity));
    loop {
        match (a.next(), b.next()) {
            (None, None) => return Ok(true),
            (Some(a), Some(b)) => {
                work.visit_sum(a.program.commands().len(), b.program.commands().len())?;
                work.visit(1)?;
                if a != b {
                    return Ok(false);
                }
            }
            _ => return Ok(false),
        }
    }
}
type Magazines = [Option<TeletextLevel25PresentationDefaults>; 8];
fn common_magazines(
    producers: &[TeletextProducerPresentation],
    work: &mut Preflight<'_>,
) -> Result<Magazines, TeletextProducerCompositionError> {
    let mut values: Magazines = [None; 8];
    let mut blocked = [false; 8];
    for (ordinal, producer) in producers.iter().enumerate() {
        work.producer = Some(ordinal);
        for (&magazine, &value) in producer.original().semantic().magazine_presentations() {
            work.visit(1)?;
            let index = usize::from(magazine - 1);
            if values[index].is_some_and(|old| old != value) {
                blocked[index] = true;
            }
            values[index] = Some(value);
        }
        for page in producer.pages() {
            work.visit(1)?;
            if matches!(
                page.source(),
                TeletextProducerPresentationSource::Header { .. }
                    | TeletextProducerPresentationSource::MotFallback { .. }
            ) {
                blocked[usize::from(page.address().page.magazine() - 1)] = true;
            }
        }
    }
    for (value, blocked) in values.iter_mut().zip(blocked) {
        if blocked {
            *value = None;
        }
    }
    work.producer = None;
    Ok(values)
}
struct ComposedInputs {
    draft: TeletextResolvedServiceDraft,
    pages: Vec<TeletextComposedProducerPage>,
    companions: Vec<TeletextResourceIdentity>,
    mots: Vec<TeletextMotLevel25Declaration>,
    selections: Vec<TeletextDisplayMotSelection>,
    overlays: Vec<super::TeletextPhysicalBaseOverlay>,
}
#[derive(Default)]
struct InputCounts {
    pages: usize,
    objects: usize,
    drcs: usize,
    companions: usize,
    mots: usize,
    overlays: usize,
}
fn capacities(
    producers: &[TeletextProducerPresentation],
    work: &mut Preflight<'_>,
) -> Result<InputCounts, TeletextProducerCompositionError> {
    let mut counts = InputCounts::default();
    for producer in producers {
        work.visit(1)?;
        let service = producer.original();
        for (total, amount) in [
            (&mut counts.pages, service.semantic().base().pages.len()),
            (
                &mut counts.objects,
                service.semantic().objects().definitions().len(),
            ),
            (
                &mut counts.drcs,
                service.semantic().drcs().subtables().len(),
            ),
            (&mut counts.companions, service.empty_object_pages().len()),
            (&mut counts.mots, service.mots().len()),
            (
                &mut counts.overlays,
                service.semantic().physical_base_overlays().len(),
            ),
        ] {
            *total = total
                .checked_add(amount)
                .ok_or_else(|| failure(TeletextProducerCompositionFailure::StorageOverflow))?;
        }
    }
    Ok(counts)
}
fn compose_inputs(
    producers: &[TeletextProducerPresentation],
    metadata: &TeletextProducerCompositionMetadata,
    indexes: &InputIndexes,
    magazines: &Magazines,
    work: &mut Preflight<'_>,
) -> Result<ComposedInputs, TeletextProducerCompositionError> {
    let counts = capacities(producers, work)?;
    let mut output = ComposedInputs {
        draft: TeletextResolvedServiceDraft {
            base: TeletextPageSetDraft {
                language: metadata.language.clone(),
                initial_page: metadata.entry.page,
                pages: Vec::with_capacity(counts.pages),
            },
            pages: Vec::with_capacity(counts.pages),
            magazines: Vec::with_capacity(magazines.iter().flatten().count()),
            objects: Vec::with_capacity(counts.objects),
            drcs: Vec::with_capacity(counts.drcs),
        },
        pages: Vec::with_capacity(counts.pages),
        companions: Vec::with_capacity(counts.companions),
        mots: Vec::with_capacity(counts.mots),
        selections: Vec::with_capacity(counts.pages),
        overlays: Vec::with_capacity(counts.overlays),
    };
    for (ordinal, producer) in producers.iter().enumerate() {
        work.producer = Some(ordinal);
        copy_pages(producer, ordinal, magazines, indexes, &mut output, work)?;
        copy_resources(producer, ordinal, indexes, &mut output, work)?;
        for overlay in producer
            .original()
            .semantic()
            .physical_base_overlays()
            .values()
        {
            work.visit(1)?;
            work.visit(overlay.rows().len())?;
            output.overlays.push(overlay.clone());
        }
    }
    for (index, value) in magazines.iter().enumerate() {
        if let Some(presentation) = value {
            output
                .draft
                .magazines
                .push(TeletextMagazinePresentationDraft {
                    magazine: u8::try_from(index + 1).map_err(|_| {
                        failure(TeletextProducerCompositionFailure::StorageOverflow)
                    })?,
                    presentation: *presentation,
                });
        }
    }
    work.producer = None;
    Ok(output)
}
fn empty_metadata(address: TeletextPageAddress) -> TeletextResolvedPageEnhancementDraft {
    TeletextResolvedPageEnhancementDraft {
        address,
        presentation: None,
        compositional_links: None,
        local_program: None,
        resources: TeletextResolvedPageResourceSelection {
            objects: TeletextResolvedObjectPageLinks {
                public: None,
                global: None,
            },
            drcs: TeletextDrcsPageLinks::unlinked(),
            defaults: [None; 4],
        },
    }
}
fn copy_pages(
    producer: &TeletextProducerPresentation,
    ordinal: usize,
    magazines: &Magazines,
    indexes: &InputIndexes,
    output: &mut ComposedInputs,
    work: &mut Preflight<'_>,
) -> Result<(), TeletextProducerCompositionError> {
    let original = producer.original().semantic();
    for (page, base) in original.base().pages.iter().enumerate() {
        work.visit(1)?;
        let snapshot = producer
            .pages()
            .get(page)
            .filter(|value| value.address() == base.address)
            .ok_or_else(|| {
                location(
                    ordinal,
                    page,
                    base.address,
                    TeletextProducerCompositionFailure::Snapshot,
                )
            })?;
        let mut enhanced = original.page_enhancements().get(&base.address).cloned();
        let index = usize::from(base.address.page.magazine() - 1);
        let derivation =
            if let TeletextProducerPresentationSource::Magazine { magazine } = snapshot.source() {
                if magazines[index].is_none() {
                    if !matches!(
                        snapshot.secondary_selection(),
                        TeletextProducerSecondarySelection::Declared(_)
                    ) {
                        return Err(location(
                            ordinal,
                            page,
                            base.address,
                            TeletextProducerCompositionFailure::Snapshot,
                        ));
                    }
                    enhanced
                        .get_or_insert_with(|| empty_metadata(base.address))
                        .presentation = Some(*snapshot.effective_defaults());
                    TeletextProducerPageDerivation::MagazineToPage { magazine }
                } else {
                    TeletextProducerPageDerivation::Unchanged
                }
            } else {
                TeletextProducerPageDerivation::Unchanged
            };
        output.draft.base.pages.push(base.clone());
        if let Some(enhanced) = enhanced {
            output.draft.pages.push(enhanced);
        }
        output.pages.push(TeletextComposedProducerPage {
            address: base.address,
            producer: ordinal,
            page,
            derivation,
        });
        output.selections.push(TeletextDisplayMotSelection {
            display: base.address,
            mot: indexes.mots[index].map(|owner| owner.identity),
        });
    }
    Ok(())
}
fn selected_resource(
    indexes: &InputIndexes,
    ordinal: usize,
    identity: TeletextResourceIdentity,
) -> bool {
    indexes
        .resources
        .get(&(identity.page(), identity.logical_subpage()))
        .is_some_and(|owner| owner.producer == ordinal)
}
fn copy_resources(
    producer: &TeletextProducerPresentation,
    ordinal: usize,
    indexes: &InputIndexes,
    output: &mut ComposedInputs,
    work: &mut Preflight<'_>,
) -> Result<(), TeletextProducerCompositionError> {
    let original = producer.original();
    for object in original.semantic().objects().definitions() {
        work.visit(1)?;
        if object
            .identity
            .resource()
            .is_none_or(|identity| selected_resource(indexes, ordinal, identity))
        {
            output.draft.objects.push(object.clone());
        }
    }
    for table in original.semantic().drcs().subtables() {
        work.visit(1)?;
        if selected_resource(indexes, ordinal, table.identity()) {
            output.draft.drcs.push(table.clone());
        }
    }
    for &identity in original.empty_object_pages() {
        work.visit(1)?;
        if selected_resource(indexes, ordinal, identity) {
            output.companions.push(identity);
        }
    }
    for mot in original.mots() {
        work.visit(1)?;
        if selected_resource(indexes, ordinal, mot.identity()) {
            output.mots.push(mot.clone());
        }
    }
    Ok(())
}
fn check_contexts(
    producers: &[TeletextProducerPresentation],
    pages: &[TeletextComposedProducerPage],
    composed: &TeletextDeclaredService,
    work: &mut Preflight<'_>,
) -> Result<(), TeletextProducerCompositionError> {
    for page in pages {
        work.producer = Some(page.producer);
        let producer = &producers[page.producer];
        let snapshot = &producer.pages()[page.page];
        work.visit_sum(
            producer.original().resolutions().len(),
            composed.resolutions().len(),
        )?;
        let old = producer
            .original()
            .resolutions()
            .iter()
            .find(|value| value.display() == page.address);
        let new = composed
            .resolutions()
            .iter()
            .find(|value| value.display() == page.address);
        let fail = |category| {
            location(
                page.producer,
                page.page,
                page.address,
                TeletextProducerCompositionFailure::Context { category },
            )
        };
        let (old, new) = old.zip(new).ok_or_else(|| fail("resolution"))?;
        if old.defaults() != new.defaults()
            || old.conditional_pop_fallback() != new.conditional_pop_fallback()
            || TeletextX27LinkRole::ALL
                .into_iter()
                .any(|role| old.role(role) != new.role(role))
        {
            return Err(fail("MOT/X27 effects"));
        }
        if old.mot().is_some() && old.mot() != new.mot() {
            return Err(fail("original MOT"));
        }
        work.visit(composed.semantic().base().pages.len())?;
        if !same_presentation(
            snapshot,
            page.derivation,
            composed.semantic().effective_presentation(page.address),
        ) {
            return Err(fail("presentation"));
        }
        let local = composed
            .semantic()
            .page_enhancements()
            .get(&page.address)
            .and_then(|value| value.local_program.as_ref());
        let presence = match local {
            None => TeletextProducerLocalProgramPresence::Absent,
            Some(value) if value.commands().is_empty() => {
                TeletextProducerLocalProgramPresence::PresentEmpty
            }
            Some(_) => TeletextProducerLocalProgramPresence::PresentNonEmpty,
        };
        if presence != snapshot.local_program_presence() {
            return Err(fail("Local presence"));
        }
    }
    work.producer = None;
    Ok(())
}
fn same_presentation(
    original: &TeletextProducerPagePresentation,
    derivation: TeletextProducerPageDerivation,
    current: Option<TeletextEffectivePresentation<'_>>,
) -> bool {
    match (original.source(), current) {
        (
            TeletextProducerPresentationSource::Page,
            Some(TeletextEffectivePresentation::Extension {
                source: TeletextPresentationSource::Page,
                defaults,
            }),
        ) => {
            original.effective_defaults() == defaults
                && original.secondary_selection()
                    == TeletextProducerSecondarySelection::Declared(defaults.secondary)
        }
        (
            TeletextProducerPresentationSource::Magazine { .. },
            Some(TeletextEffectivePresentation::Extension { source, defaults }),
        ) => {
            let expected = if matches!(
                derivation,
                TeletextProducerPageDerivation::MagazineToPage { .. }
            ) {
                TeletextPresentationSource::Page
            } else {
                TeletextPresentationSource::Magazine
            };
            source == expected
                && original.effective_defaults() == defaults
                && original.secondary_selection()
                    == TeletextProducerSecondarySelection::Declared(defaults.secondary)
        }
        (
            TeletextProducerPresentationSource::Header { national_option },
            Some(TeletextEffectivePresentation::Header {
                national_option: current,
            }),
        ) => {
            national_option == current
                && original.secondary_selection()
                    == TeletextProducerSecondarySelection::DecoderDependent
        }
        (
            TeletextProducerPresentationSource::MotFallback {
                national_option, ..
            },
            Some(TeletextEffectivePresentation::MotFallback {
                national_option: current,
                defaults,
            }),
        ) => {
            national_option == current
                && original.effective_defaults() == defaults
                && original.secondary_selection()
                    == TeletextProducerSecondarySelection::DecoderDependent
        }
        _ => false,
    }
}

#[cfg(test)]
#[path = "producer_composition_tests.rs"]
mod tests;
