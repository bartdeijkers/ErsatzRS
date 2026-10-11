//! Typed selected-page correspondence from one producer-owned compiled assembly.
//! Full fragment, attribute, mask and C.1 comparison remains independently required.

use super::{
    EncodedTeletextAugmentedOriginProgram, TeletextAugmentedOriginSource,
    TeletextBaseCellCommandKind, TeletextMainCompositionSource, TeletextOriginCarrierCommandRole,
};
use ersatzrs_domain::{
    resolve_teletext_latin_replacement_identity, TeletextDisplayPending, TeletextEnhancementSource,
    TeletextExecutedGlyphIntent, TeletextGlyphCandidate, TeletextGlyphDisposition,
    TeletextGlyphIdentity, TeletextPageAddress, TeletextSpatialInvocationId,
    TeletextSpatialRootSource,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextAugmentedGlyphComparisonBudget {
    pub max_records: usize,
    pub max_cells: usize,
    pub max_diagnostics: usize,
    pub max_work: usize,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextAugmentedGlyphSide {
    Original,
    Actual,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextAugmentedGlyphSource {
    pub origin: Option<usize>,
    pub candidate: Option<usize>,
    pub raw_position: Option<(u16, u16)>,
    pub location: Option<TeletextEnhancementSource>,
    pub root_source: Option<TeletextSpatialRootSource>,
    pub invocation: Option<TeletextSpatialInvocationId>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextAugmentedGlyphFinding {
    Correspondence,
    Identity {
        original: Option<TeletextGlyphIdentity>,
        actual: Option<TeletextGlyphIdentity>,
    },
    Drcs,
    Pending {
        side: TeletextAugmentedGlyphSide,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextAugmentedGlyphDiagnostic {
    pub position: (u8, u8),
    pub original: TeletextAugmentedGlyphSource,
    pub actual: TeletextAugmentedGlyphSource,
    pub finding: TeletextAugmentedGlyphFinding,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextAugmentedGlyphComparisonFailure {
    #[error("{kind} count {count} exceeds trusted maximum {maximum}")]
    Budget {
        kind: &'static str,
        count: usize,
        maximum: usize,
    },
    #[error("comparison storage arithmetic exceeds supported range")]
    AllocationSize,
    #[error("comparison storage allocation failed")]
    Allocation,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("augmented glyph comparison for {owner:?} at {position:?}: {reason}")]
pub struct TeletextAugmentedGlyphComparisonError {
    pub owner: TeletextPageAddress,
    pub position: Option<(u8, u8)>,
    pub reason: TeletextAugmentedGlyphComparisonFailure,
}
/// Borrowed producer pairing, not matching-generation or receiver acceptance.
/// Zero findings establishes only selected typed correspondence under this model;
/// full geometry/attributes/colours, nonvisible uncertainty and admission remain
/// accessible through the inputs and require their separate checks.
#[derive(Debug)]
pub struct TeletextAugmentedGlyphComparison<'a> {
    compiled: &'a EncodedTeletextAugmentedOriginProgram<'a>,
    diagnostics: Vec<TeletextAugmentedGlyphDiagnostic>,
    cells: usize,
}
impl TeletextAugmentedGlyphComparison<'_> {
    #[must_use]
    pub const fn compiled(&self) -> &EncodedTeletextAugmentedOriginProgram<'_> {
        self.compiled
    }
    #[must_use]
    pub fn diagnostics(&self) -> &[TeletextAugmentedGlyphDiagnostic] {
        &self.diagnostics
    }
    #[must_use]
    pub const fn compared_cells(&self) -> usize {
        self.cells
    }
    /// Append-only history, including superseded and nonvisible uncertainty.
    #[must_use]
    pub fn original_pending(&self) -> &[TeletextDisplayPending] {
        self.compiled
            .augmented()
            .placement()
            .plan()
            .composition()
            .display_resolution()
            .pending()
    }
    #[must_use]
    pub fn actual_pending(&self) -> &[TeletextDisplayPending] {
        self.compiled
            .physical()
            .replay()
            .display_resolution()
            .pending()
    }
}
/// Compare every selected visual cell using actual resolved glyph identities,
/// exact carrier origin roles and paired retained invocation ancestry. No mask
/// equality is used as identity evidence. Original Base Unicode replacements
/// use their existing explicit Latin recipe; actual Base uses coded fallback.
/// # Errors
/// Fences raw records/cells/storage before allocation and charges every source
/// lookup. Budget failure returns no partial result and mutates neither input.
pub fn compare_teletext_augmented_origin_glyphs<'a>(
    compiled: &'a EncodedTeletextAugmentedOriginProgram<'a>,
    budget: TeletextAugmentedGlyphComparisonBudget,
) -> Result<TeletextAugmentedGlyphComparison<'a>, TeletextAugmentedGlyphComparisonError> {
    let mut builder = Builder {
        result: TeletextAugmentedGlyphComparison {
            compiled,
            diagnostics: Vec::new(),
            cells: 0,
        },
        budget,
        work: 0,
    };
    builder.preflight()?;
    let original = compiled.augmented().placement().plan().composition();
    let actual = compiled.physical().replay();
    let width = original
        .display_resolution()
        .width()
        .max(actual.display_resolution().width());
    for row in 0..25 {
        for column in 0..width {
            builder.cell((row, column))?;
            builder.result.cells += 1;
        }
    }
    Ok(builder.result)
}

type Error = TeletextAugmentedGlyphComparisonError;
type Failure = TeletextAugmentedGlyphComparisonFailure;
type Finding = TeletextAugmentedGlyphFinding;
struct Builder<'a> {
    result: TeletextAugmentedGlyphComparison<'a>,
    budget: TeletextAugmentedGlyphComparisonBudget,
    work: usize,
}
impl Builder<'_> {
    fn error(&self, position: Option<(u8, u8)>, reason: Failure) -> Error {
        Error {
            owner: self
                .result
                .compiled
                .augmented()
                .placement()
                .plan()
                .composition()
                .owner(),
            position,
            reason,
        }
    }
    fn bound(&self, kind: &'static str, count: usize, maximum: usize) -> Result<(), Error> {
        if count > maximum {
            Err(self.error(
                None,
                Failure::Budget {
                    kind,
                    count,
                    maximum,
                },
            ))
        } else {
            Ok(())
        }
    }
    fn charge(&mut self, count: usize, position: (u8, u8)) -> Result<(), Error> {
        self.work = self
            .work
            .checked_add(count)
            .ok_or_else(|| self.error(Some(position), Failure::AllocationSize))?;
        if self.work > self.budget.max_work {
            return Err(self.error(
                Some(position),
                Failure::Budget {
                    kind: "work",
                    count: self.work,
                    maximum: self.budget.max_work,
                },
            ));
        }
        Ok(())
    }
    fn preflight(&mut self) -> Result<(), Error> {
        let compiled = self.result.compiled;
        let original = compiled.augmented().placement().plan().composition();
        let actual = compiled.physical().replay();
        let records = [
            original.glyph_intent().records().len(),
            actual.glyph_intent().records().len(),
            original.display_resolution().origins().len(),
            actual.display_resolution().origins().len(),
            original.drcs_execution().records().len(),
            actual.drcs_execution().records().len(),
            original.spatial_trace().events().len(),
            actual.spatial_trace().events().len(),
            compiled.invocations().len(),
            compiled.augmented().carriers().len(),
            compiled
                .augmented()
                .placement()
                .catalog()
                .definitions()
                .len(),
            original.display_resolution().pending().len(),
            actual.display_resolution().pending().len(),
        ]
        .into_iter()
        .try_fold(0usize, usize::checked_add)
        .ok_or_else(|| self.error(None, Failure::AllocationSize))?;
        self.bound("records", records, self.budget.max_records)?;
        let cells = usize::from(
            original
                .display_resolution()
                .width()
                .max(actual.display_resolution().width()),
        ) * 25;
        self.bound("cells", cells, self.budget.max_cells)?;
        self.bound("work", cells, self.budget.max_work)?;
        // At most two pending findings and one correspondence/identity/DRCS finding per cell.
        let capacity = cells
            .checked_mul(3)
            .ok_or_else(|| self.error(None, Failure::AllocationSize))?
            .min(self.budget.max_diagnostics);
        let bytes = capacity
            .checked_mul(std::mem::size_of::<TeletextAugmentedGlyphDiagnostic>())
            .ok_or_else(|| self.error(None, Failure::AllocationSize))?;
        if isize::try_from(bytes).is_err() {
            return Err(self.error(None, Failure::AllocationSize));
        }
        self.result
            .diagnostics
            .try_reserve_exact(capacity)
            .map_err(|_| self.error(None, Failure::Allocation))
    }
    fn push(
        &mut self,
        position: (u8, u8),
        original: TeletextAugmentedGlyphSource,
        actual: TeletextAugmentedGlyphSource,
        finding: Finding,
    ) -> Result<(), Error> {
        let count = self
            .result
            .diagnostics
            .len()
            .checked_add(1)
            .ok_or_else(|| self.error(Some(position), Failure::AllocationSize))?;
        if count > self.budget.max_diagnostics {
            return Err(self.error(
                Some(position),
                Failure::Budget {
                    kind: "diagnostics",
                    count,
                    maximum: self.budget.max_diagnostics,
                },
            ));
        }
        self.result
            .diagnostics
            .push(TeletextAugmentedGlyphDiagnostic {
                position,
                original,
                actual,
                finding,
            });
        Ok(())
    }
    fn cell(&mut self, position: (u8, u8)) -> Result<(), Error> {
        self.charge(1, position)?;
        let compiled = self.result.compiled;
        let original = compiled.augmented().placement().plan().composition();
        let actual = compiled.physical().replay();
        if self.pending_fragments(position)? {
            return Ok(());
        }
        let first = selected(
            original.display_resolution(),
            original.glyph_intent(),
            position,
        );
        let second = selected(actual.display_resolution(), actual.glyph_intent(), position);
        if first.is_none() && second.is_none() {
            return Ok(());
        }
        let mut original_source = source(first);
        let mut actual_source = source(second);
        original_source.candidate = first
            .map(|(index, _)| original.display_resolution().origins()[index].candidate_index());
        actual_source.candidate =
            second.map(|(index, _)| actual.display_resolution().origins()[index].candidate_index());
        let (Some((oi, expected)), Some((ai, found))) = (first, second) else {
            return self.push(
                position,
                original_source,
                actual_source,
                Finding::Correspondence,
            );
        };
        for (side, index, record, display) in [
            (
                TeletextAugmentedGlyphSide::Original,
                oi,
                expected,
                original.display_resolution(),
            ),
            (
                TeletextAugmentedGlyphSide::Actual,
                ai,
                found,
                actual.display_resolution(),
            ),
        ] {
            if record.disposition() != TeletextGlyphDisposition::Assigned
                || display.origins()[index].pending().is_some()
            {
                self.push(
                    position,
                    original_source,
                    actual_source,
                    Finding::Pending { side },
                )?;
            }
        }
        if !self.corresponds(
            oi,
            expected.context().candidate(),
            found.context().candidate(),
            position,
        )? {
            return self.push(
                position,
                original_source,
                actual_source,
                Finding::Correspondence,
            );
        }
        self.compare_identity(
            position,
            (oi, expected),
            (ai, found),
            original_source,
            actual_source,
        )
    }
    fn pending_fragments(&mut self, position: (u8, u8)) -> Result<bool, Error> {
        let compiled = self.result.compiled;
        let original = compiled.augmented().placement().plan().composition();
        let actual = compiled.physical().replay();
        let pending = [
            (
                TeletextAugmentedGlyphSide::Original,
                original.display_resolution().cell(position.0, position.1),
            ),
            (
                TeletextAugmentedGlyphSide::Actual,
                actual.display_resolution().cell(position.0, position.1),
            ),
        ];
        if pending.iter().any(|(_, cell)| {
            cell.is_some_and(|cell| {
                cell.fragment() == ersatzrs_domain::TeletextDisplayFragment::Pending
            })
        }) {
            for (side, cell) in pending {
                if cell.is_some_and(|cell| {
                    cell.fragment() == ersatzrs_domain::TeletextDisplayFragment::Pending
                }) {
                    self.push(
                        position,
                        source(None),
                        source(None),
                        Finding::Pending { side },
                    )?;
                }
            }
            return Ok(true);
        }
        Ok(false)
    }
    fn compare_identity(
        &mut self,
        position: (u8, u8),
        first: (usize, &TeletextExecutedGlyphIntent),
        second: (usize, &TeletextExecutedGlyphIntent),
        original_source: TeletextAugmentedGlyphSource,
        actual_source: TeletextAugmentedGlyphSource,
    ) -> Result<(), Error> {
        let compiled = self.result.compiled;
        let original = compiled.augmented().placement().plan().composition();
        let actual = compiled.physical().replay();
        let (oi, expected) = first;
        let (ai, found) = second;
        let mut identity = expected.glyph();
        if let Some(base) = expected.context().candidate().base() {
            if let Some(operation) =
                original.base_replacement(base.position().row(), base.position().column())
            {
                identity = resolve_teletext_latin_replacement_identity(operation).ok();
            }
        }
        // A carrier's explicit SPACE preserves a blank/control's visible glyph role;
        // original coded controls remain in the immutable Base and full comparison.
        if matches!(
            identity,
            Some(TeletextGlyphIdentity::Blank | TeletextGlyphIdentity::SpacingControl { .. })
        ) && found.context().candidate().operation().is_some()
        {
            identity = space_identity();
        }
        if expected.disposition() != TeletextGlyphDisposition::Assigned
            || found.disposition() != TeletextGlyphDisposition::Assigned
        {
            return Ok(());
        }
        if identity != found.glyph() {
            return self.push(
                position,
                original_source,
                actual_source,
                Finding::Identity {
                    original: identity,
                    actual: found.glyph(),
                },
            );
        }
        if matches!(identity, Some(TeletextGlyphIdentity::Drcs { .. })) {
            self.charge(
                original.drcs_execution().records().len() + actual.drcs_execution().records().len(),
                position,
            )?;
            let a = original.drcs_execution().records().iter().find(|r| {
                r.candidate_index() == original.display_resolution().origins()[oi].candidate_index()
            });
            let b = actual.drcs_execution().records().iter().find(|r| {
                r.candidate_index() == actual.display_resolution().origins()[ai].candidate_index()
            });
            if !matches!((a,b),(Some(a),Some(b)) if a.reference()==b.reference() && a.resource()==b.resource() && a.bitmap()==b.bitmap())
            {
                self.push(position, original_source, actual_source, Finding::Drcs)?;
            }
        }
        Ok(())
    }
    fn corresponds(
        &mut self,
        origin: usize,
        expected: &TeletextGlyphCandidate,
        actual: &TeletextGlyphCandidate,
        position: (u8, u8),
    ) -> Result<bool, Error> {
        let compiled = self.result.compiled;
        let augmented = compiled.augmented();
        if let Some(base) = actual.base() {
            return Ok(expected
                .base()
                .is_some_and(|old| old.position() == base.position()));
        }
        let Some(location) = actual.source() else {
            return Ok(false);
        };
        // emitter_source scans carriers before exact retained catalogue lookup.
        self.charge(
            augmented.carriers().len()
                + augmented.placement().catalog().definitions().len()
                + compiled.invocations().len()
                + 1,
            position,
        )?;
        let mapped = augmented.emitter_source(location);
        let retained = |object, command| {
            expected
                .source()
                .is_some_and(|source| source.object == object && source.command == command)
                && expected
                    .invocation()
                    .and_then(|id| compiled.actual_for_original(id))
                    == actual.invocation()
        };
        Ok(match mapped {
            Some(TeletextAugmentedOriginSource::CarrierBody {
                carrier,
                role: TeletextOriginCarrierCommandRole::Glyph,
            }) => {
                augmented.carriers()[carrier].original_origin_index() == origin
                    && compiled.actual_for_carrier(carrier) == actual.invocation()
            }
            Some(TeletextAugmentedOriginSource::RetainedObject { identity, command }) => {
                retained(Some(identity), command)
            }
            Some(TeletextAugmentedOriginSource::RetainedMain { provenance, .. }) => {
                match provenance {
                    TeletextMainCompositionSource::OriginalPage { command } => {
                        retained(None, command)
                    }
                    TeletextMainCompositionSource::DefaultSlot { slot } => {
                        expected.root_source()
                            == Some(TeletextSpatialRootSource::DefaultSlot { slot })
                            && expected
                                .invocation()
                                .and_then(|id| compiled.actual_for_original(id))
                                == actual.invocation()
                    }
                    TeletextMainCompositionSource::BaseCell {
                        position,
                        kind: TeletextBaseCellCommandKind::Glyph,
                    } => expected
                        .base()
                        .is_some_and(|base| base.position() == position),
                    _ => false,
                }
            }
            _ => false,
        })
    }
}
fn selected<'a>(
    display: &ersatzrs_domain::TeletextDisplayResolution,
    intent: &'a ersatzrs_domain::TeletextGlyphIntent,
    position: (u8, u8),
) -> Option<(usize, &'a TeletextExecutedGlyphIntent)> {
    let origin = display.cell(position.0, position.1)?.origin()?;
    let candidate = display.origins().get(origin)?.candidate_index();
    Some((origin, intent.records().get(candidate)?))
}
fn source(value: Option<(usize, &TeletextExecutedGlyphIntent)>) -> TeletextAugmentedGlyphSource {
    let candidate = value.map(|(_, record)| record.context().candidate());
    TeletextAugmentedGlyphSource {
        origin: value.map(|(index, _)| index),
        candidate: None,
        raw_position: candidate.map(TeletextGlyphCandidate::position),
        location: candidate.and_then(TeletextGlyphCandidate::source),
        root_source: candidate.and_then(TeletextGlyphCandidate::root_source),
        invocation: candidate.and_then(TeletextGlyphCandidate::invocation),
    }
}
fn space_identity() -> Option<TeletextGlyphIdentity> {
    let operation = ersatzrs_domain::TeletextColumnOperation::G0Diacritic {
        code: ersatzrs_domain::TeletextGlyphCode::new(0x20).ok()?,
        diacritic: ersatzrs_domain::TeletextDiacriticIndex::new(0).ok()?,
    };
    resolve_teletext_latin_replacement_identity(operation).ok()
}
