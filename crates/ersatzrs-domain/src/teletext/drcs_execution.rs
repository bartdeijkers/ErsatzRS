//! Exact authored DRCS bitmap lookup for original glyph contenders.
//!
//! Scope/S1/slot are already explicit semantic references, not raw wire selector
//! state. This preserves losing and repeated candidates without certifying
//! rasterization, geometry, receiver behavior or publication readiness.

use std::mem::size_of;

use super::{
    TeletextCharsetValueError, TeletextCollectedDrcsError, TeletextCollectedEnhancementReferences,
    TeletextColumnOperation, TeletextDrcsBitmap, TeletextDrcsGlyphReference, TeletextDrcsGlyphSlot,
    TeletextDrcsPageLinks, TeletextDrcsTableScope, TeletextEnhancementCollectionBudget,
    TeletextEnhancementCollectionError, TeletextEnhancementCommand,
    TeletextEnhancementObjectCatalog, TeletextEnhancementProgram, TeletextEnhancementSource,
    TeletextGlyphCandidate, TeletextGlyphIdentity, TeletextGlyphIntent, TeletextPageAddress,
    TeletextResourceIdentity, TeletextResourceKind, TeletextResourceSubpageIndex,
    TeletextSemanticService, TeletextSpatialInvocationId, TeletextSpatialRootSource,
};

/// Trusted count policy, not a deserializable producer capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextDrcsExecutionBudget {
    /// All input intent records, including non-DRCS Base cells.
    pub max_candidates: usize,
    /// Original DRCS occurrences, including losers and repeated calls.
    pub max_references: usize,
}

/// One immutable resolved original reference, with no winning-glyph selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextExecutedDrcsGlyph {
    candidate_index: usize,
    candidate: TeletextGlyphCandidate,
    reference: TeletextDrcsGlyphReference,
    resource: TeletextResourceIdentity,
    bitmap: TeletextDrcsBitmap,
}

impl TeletextExecutedDrcsGlyph {
    /// Index into the exact supplied intent's candidate-aligned records.
    #[must_use]
    pub const fn candidate_index(&self) -> usize {
        self.candidate_index
    }
    #[must_use]
    pub const fn candidate(&self) -> &TeletextGlyphCandidate {
        &self.candidate
    }
    #[must_use]
    pub const fn reference(&self) -> TeletextDrcsGlyphReference {
        self.reference
    }
    #[must_use]
    pub const fn resource(&self) -> TeletextResourceIdentity {
        self.resource
    }
    #[must_use]
    pub const fn bitmap(&self) -> TeletextDrcsBitmap {
        self.bitmap
    }
}

/// Sparse original-candidate order, retaining every DRCS occurrence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextDrcsExecution {
    owner: TeletextPageAddress,
    records: Vec<TeletextExecutedDrcsGlyph>,
}

impl TeletextDrcsExecution {
    #[must_use]
    pub const fn owner(&self) -> TeletextPageAddress {
        self.owner
    }
    #[must_use]
    pub fn records(&self) -> &[TeletextExecutedDrcsGlyph] {
        &self.records
    }
}

/// Static rejection with the original candidate's full semantic provenance.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("teletext DRCS execution for {owner:?} at {position:?}: {reason}")]
pub struct TeletextDrcsExecutionError {
    pub owner: TeletextPageAddress,
    pub position: Option<(u16, u16)>,
    pub location: Option<TeletextEnhancementSource>,
    pub root_source: Option<TeletextSpatialRootSource>,
    pub invocation: Option<TeletextSpatialInvocationId>,
    pub reason: TeletextDrcsExecutionFailure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextDrcsExecutionFailure {
    #[error("intent count {count} exceeds candidate budget {maximum}")]
    CandidateBudget { count: usize, maximum: usize },
    #[error("DRCS count {count} exceeds reference budget {maximum}")]
    ReferenceBudget { count: usize, maximum: usize },
    #[error("DRCS execution allocation size exceeds storage range")]
    AllocationSize,
    #[error("DRCS execution lookup work exceeds counter range")]
    WorkSize,
    #[error("the exact base page/subcode is absent from the service")]
    MissingPage,
    #[error("DRCS candidate does not match an invoked original source command")]
    CandidateMismatch,
    #[error("DRCS scope {table:?} has no supplied resource link")]
    MissingLink { table: TeletextDrcsTableScope },
    #[error("DRCS subtable {identity:?} is absent")]
    MissingSubtable { identity: TeletextResourceIdentity },
    #[error("DRCS subtable {identity:?} lacks slot {slot:?}")]
    MissingGlyph {
        identity: TeletextResourceIdentity,
        slot: TeletextDrcsGlyphSlot,
    },
}

/// Resolve the checked service's exact links and monochrome bitmap definitions.
///
/// Raw candidate count is fenced before scanning. Exact DRCS count and linear
/// output storage and conservative lookup work are checked before allocation.
/// Each lookup scans at most the
/// checked catalog's 1024 subtables and a table's 48 glyphs. Source checks use the
/// existing indexed object catalog and at most 381 collected invocations; there
/// is no cloned reference registry or per-candidate unbounded catalog traversal.
/// Original source commands are checked, but matching identifiers are not a
/// generation certificate. The authoritative factory must retain the same
/// immutable service and intent. Service construction already validates the
/// complete usage's two-subtable/24-distinct-glyph limits; they are not relaxed
/// or duplicated here. No receiver fallback, blank glyph or selector is guessed.
/// # Errors
/// Rejects count/storage/work budgets, a missing exact owner, inconsistent original
/// source, or absent link/subtable/glyph. No partial output or mutation escapes.
pub fn resolve_teletext_drcs_execution(
    service: &TeletextSemanticService,
    intent: &TeletextGlyphIntent,
    budget: TeletextDrcsExecutionBudget,
) -> Result<TeletextDrcsExecution, TeletextDrcsExecutionError> {
    let count = preflight(service, intent, budget, 1)?;
    let owner = intent.owner();
    let (_, collected) = service
        .collected(owner)
        .ok_or_else(|| execution_error(owner, None, TeletextDrcsExecutionFailure::MissingPage))?;
    let sources = SourceContext {
        program: service
            .page_enhancements()
            .get(&owner)
            .and_then(|page| page.local_program.as_ref()),
        catalog: service.objects(),
        collected,
    };
    resolve_sources(service, intent, sources, count)
}

/// Additive replay errors retain existing typed collection and lookup evidence.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TeletextDrcsProgramExecutionError {
    #[error(transparent)]
    Execution(#[from] TeletextDrcsExecutionError),
    #[error(transparent)]
    Collection(Box<TeletextEnhancementCollectionError>),
    #[error("DRCS replay dependencies for {owner:?}: {error}")]
    Dependencies {
        owner: TeletextPageAddress,
        #[source]
        error: Box<TeletextCollectedDrcsError>,
    },
    #[error("DRCS replay primary designation for {owner:?}: {error}")]
    Primary {
        owner: TeletextPageAddress,
        #[source]
        error: TeletextCharsetValueError,
    },
}
impl TeletextDrcsProgramExecutionError {
    #[must_use]
    pub fn owner(&self) -> TeletextPageAddress {
        match self {
            Self::Execution(error) => error.owner,
            Self::Collection(error) => error.display,
            Self::Dependencies { owner, .. } | Self::Primary { owner, .. } => *owner,
        }
    }
    #[must_use]
    pub fn location(&self) -> Option<TeletextEnhancementSource> {
        match self {
            Self::Execution(error) => error.location,
            Self::Collection(error) => Some(error.source_location),
            Self::Dependencies { error, .. } => error.source_location,
            Self::Primary { .. } => None,
        }
    }
}

/// Resolve a supplied physical main/catalog against the original service pixels.
///
/// Raw intent/reference budgets precede collection. The existing collector walks
/// the entire supplied program under the original service primary, then validates
/// all its DRCS dependencies, including occurrences absent from the intent. No
/// externally supplied collected-reference registry is treated as authority.
/// Exact referenced operations are checked against the supplied main/catalog;
/// aliases, placements, instances and matching IDs are not a generation proof.
/// Original service links, resource identities and bitmaps remain authoritative.
/// This does not compose base glyphs or certify geometry, raster or publication.
/// # Errors
/// Returns existing located budget/source/lookup errors, the actual collection or
/// dependency failure, or a missing exact owner/invalid selected primary. Inputs
/// are immutable; no partial execution escapes.
pub fn resolve_teletext_drcs_program(
    service: &TeletextSemanticService,
    program: &TeletextEnhancementProgram,
    catalog: &TeletextEnhancementObjectCatalog,
    intent: &TeletextGlyphIntent,
    collection_budget: TeletextEnhancementCollectionBudget,
    budget: TeletextDrcsExecutionBudget,
) -> Result<TeletextDrcsExecution, TeletextDrcsProgramExecutionError> {
    let count = preflight(service, intent, budget, 2)?;
    let owner = intent.owner();
    let primary = service
        .effective_presentation(owner)
        .ok_or_else(|| execution_error(owner, None, TeletextDrcsExecutionFailure::MissingPage))?
        .primary_designation()
        .map_err(|error| TeletextDrcsProgramExecutionError::Primary { owner, error })?;
    let collected = catalog
        .collect_display_references(owner, program, primary.pair(), collection_budget)
        .map_err(|error| TeletextDrcsProgramExecutionError::Collection(Box::new(error)))?;
    let sources = SourceContext {
        program: Some(program),
        catalog,
        collected: &collected,
    };
    // Reject contradicted original operations before consulting dependencies.
    // This is the same source guard as the service entry, using actual replay IDs.
    for record in intent.records() {
        if let Some(TeletextGlyphIdentity::Drcs { reference }) = record.glyph() {
            validate_source(sources, owner, *record.context().candidate(), reference)?;
        }
    }
    let links = service
        .page_enhancements()
        .get(&owner)
        .map_or_else(TeletextDrcsPageLinks::unlinked, |page| page.resources.drcs);
    collected
        .validate_drcs(service.drcs(), links)
        .map_err(|error| TeletextDrcsProgramExecutionError::Dependencies {
            owner,
            error: Box::new(error),
        })?;
    resolve_sources(service, intent, sources, count).map_err(Into::into)
}

#[derive(Clone, Copy)]
struct SourceContext<'a> {
    program: Option<&'a TeletextEnhancementProgram>,
    catalog: &'a TeletextEnhancementObjectCatalog,
    collected: &'a TeletextCollectedEnhancementReferences,
}

fn preflight(
    service: &TeletextSemanticService,
    intent: &TeletextGlyphIntent,
    budget: TeletextDrcsExecutionBudget,
    source_checks: usize,
) -> Result<usize, TeletextDrcsExecutionError> {
    let owner = intent.owner();
    let count = intent.records().len();
    if count > budget.max_candidates {
        return Err(execution_error(
            owner,
            intent
                .records()
                .get(budget.max_candidates)
                .map(|record| record.context().candidate()),
            TeletextDrcsExecutionFailure::CandidateBudget {
                count,
                maximum: budget.max_candidates,
            },
        ));
    }
    if service.collected(owner).is_none() {
        return Err(execution_error(
            owner,
            None,
            TeletextDrcsExecutionFailure::MissingPage,
        ));
    }
    let count = intent
        .records()
        .iter()
        .filter(|record| matches!(record.glyph(), Some(TeletextGlyphIdentity::Drcs { .. })))
        .count();
    if count > budget.max_references {
        let candidate = intent
            .records()
            .iter()
            .filter(|record| matches!(record.glyph(), Some(TeletextGlyphIdentity::Drcs { .. })))
            .nth(budget.max_references)
            .map(|record| record.context().candidate());
        return Err(execution_error(
            owner,
            candidate,
            TeletextDrcsExecutionFailure::ReferenceBudget {
                count,
                maximum: budget.max_references,
            },
        ));
    }
    if count
        .checked_mul(size_of::<TeletextExecutedDrcsGlyph>())
        .is_none_or(|bytes| bytes > isize::MAX as usize)
    {
        return Err(execution_error(
            owner,
            None,
            TeletextDrcsExecutionFailure::AllocationSize,
        ));
    }
    // Per occurrence: bounded catalog scan, glyph scan, and invoked-target scan.
    // Indexed source-command lookup and the bounded candidate passes add linear
    // work; no growing per-reference registry is constructed.
    if count.checked_mul(1024 + 48 + 381 * source_checks).is_none() {
        return Err(execution_error(
            owner,
            None,
            TeletextDrcsExecutionFailure::WorkSize,
        ));
    }
    Ok(count)
}

fn resolve_sources(
    service: &TeletextSemanticService,
    intent: &TeletextGlyphIntent,
    sources: SourceContext<'_>,
    count: usize,
) -> Result<TeletextDrcsExecution, TeletextDrcsExecutionError> {
    let owner = intent.owner();
    let mut records = Vec::with_capacity(count);
    for (candidate_index, record) in intent.records().iter().enumerate() {
        if let Some(TeletextGlyphIdentity::Drcs { reference }) = record.glyph() {
            let candidate = *record.context().candidate();
            validate_source(sources, owner, candidate, reference)?;
            let (resource, bitmap) = lookup(service, owner, candidate, reference)?;
            records.push(TeletextExecutedDrcsGlyph {
                candidate_index,
                candidate,
                reference,
                resource,
                bitmap,
            });
        }
    }
    Ok(TeletextDrcsExecution { owner, records })
}

fn validate_source(
    sources: SourceContext<'_>,
    owner: TeletextPageAddress,
    candidate: TeletextGlyphCandidate,
    reference: TeletextDrcsGlyphReference,
) -> Result<(), TeletextDrcsExecutionError> {
    let fail = || {
        execution_error(
            owner,
            Some(&candidate),
            TeletextDrcsExecutionFailure::CandidateMismatch,
        )
    };
    let source = candidate.source().ok_or_else(fail)?;
    let program = if let Some(target) = source.object {
        if !sources
            .collected
            .object_references()
            .iter()
            .any(|invocation| invocation.target == target)
        {
            return Err(fail());
        }
        &sources.catalog.definition(target).ok_or_else(fail)?.program
    } else {
        sources.program.ok_or_else(fail)?
    };
    let operation = TeletextColumnOperation::Drcs { reference };
    if candidate.operation() != Some(operation)
        || !matches!(program.commands().get(source.command),
            Some(TeletextEnhancementCommand::AtColumn { operation: actual, .. }) if *actual == operation)
    {
        return Err(fail());
    }
    Ok(())
}

fn lookup(
    service: &TeletextSemanticService,
    owner: TeletextPageAddress,
    candidate: TeletextGlyphCandidate,
    reference: TeletextDrcsGlyphReference,
) -> Result<(TeletextResourceIdentity, TeletextDrcsBitmap), TeletextDrcsExecutionError> {
    let fail = |reason| execution_error(owner, Some(&candidate), reason);
    let page = service
        .page_enhancements()
        .get(&owner)
        .and_then(|page| page.resources.drcs.page(reference.table))
        .ok_or_else(|| {
            fail(TeletextDrcsExecutionFailure::MissingLink {
                table: reference.table,
            })
        })?;
    let kind = match reference.table {
        TeletextDrcsTableScope::Normal => TeletextResourceKind::Drcs,
        TeletextDrcsTableScope::Global => TeletextResourceKind::Gdrcs,
    };
    let subpage = TeletextResourceSubpageIndex::new(reference.subtable.get())
        .map_err(|_| fail(TeletextDrcsExecutionFailure::CandidateMismatch))?;
    let identity = TeletextResourceIdentity::new(kind, page, subpage)
        .map_err(|_| fail(TeletextDrcsExecutionFailure::CandidateMismatch))?;
    let table = service
        .drcs()
        .subtables()
        .iter()
        .find(|table| table.identity() == identity)
        .ok_or_else(|| fail(TeletextDrcsExecutionFailure::MissingSubtable { identity }))?;
    let glyph = table
        .glyphs()
        .iter()
        .find(|glyph| glyph.slot == reference.slot)
        .ok_or_else(|| {
            fail(TeletextDrcsExecutionFailure::MissingGlyph {
                identity,
                slot: reference.slot,
            })
        })?;
    Ok((identity, glyph.bitmap))
}

fn execution_error(
    owner: TeletextPageAddress,
    candidate: Option<&TeletextGlyphCandidate>,
    reason: TeletextDrcsExecutionFailure,
) -> TeletextDrcsExecutionError {
    TeletextDrcsExecutionError {
        owner,
        position: candidate.map(TeletextGlyphCandidate::position),
        location: candidate.and_then(TeletextGlyphCandidate::source),
        root_source: candidate.and_then(TeletextGlyphCandidate::root_source),
        invocation: candidate.and_then(TeletextGlyphCandidate::invocation),
        reason,
    }
}
