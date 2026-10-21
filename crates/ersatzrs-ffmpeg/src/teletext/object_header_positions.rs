//! Context-qualified physical bodies, separate from immutable semantic bodies.

use std::collections::{BTreeMap, BTreeSet};

use ersatzrs_domain::{
    TeletextColumnOperation, TeletextEnhancementCommand, TeletextEnhancementObjectCatalog,
    TeletextEnhancementProgram, TeletextEnhancementProgramError, TeletextEnhancementSource,
    TeletextExecutedAttribute, TeletextExecutedAttributes, TeletextFlashMode, TeletextFlashRate,
    TeletextObjectCatalogBudget, TeletextObjectCatalogError, TeletextObjectIdentity,
    TeletextObjectScope, TeletextObjectType,
};

use super::TeletextMainComposition;

#[derive(Debug, thiserror::Error)]
pub(super) enum DerivationError {
    #[error("object header derivation exceeds work budget {maximum}")]
    WorkBudget { maximum: usize },
    #[error("object header derivation storage arithmetic overflows")]
    AllocationSize,
    #[error(transparent)]
    Catalog(#[from] TeletextObjectCatalogError),
    #[error(transparent)]
    Program(#[from] TeletextEnhancementProgramError),
}

#[derive(Debug, Clone)]
pub(super) struct ObjectHeaderSources {
    lengths: BTreeMap<TeletextObjectIdentity, usize>,
    inserted: BTreeSet<TeletextObjectIdentity>,
}

impl ObjectHeaderSources {
    /// The only derived body writer materializes its original leading AP.
    pub(super) fn materialized_position(&self, actual: TeletextEnhancementSource) -> Option<usize> {
        let identity = actual.object?;
        if actual.command >= *self.lengths.get(&identity)? {
            return None;
        }
        (self.inserted.contains(&identity) && actual.command == 1).then_some(0)
    }

    /// Derived setters cannot become original writers or transmission witnesses.
    pub(super) fn retained_source(
        &self,
        actual: TeletextEnhancementSource,
    ) -> Option<TeletextEnhancementSource> {
        let Some(identity) = actual.object else {
            return Some(actual);
        };
        if actual.command >= *self.lengths.get(&identity)? {
            return None;
        }
        let command = if self.inserted.contains(&identity) {
            match actual.command {
                0 => 0,
                1 => return None,
                // The only insertion is immediately after the leading AP.
                // All later commands retain their exact original ordinal.
                command => command - 1,
            }
        } else {
            actual.command
        };
        Some(TeletextEnhancementSource {
            object: Some(identity),
            command,
        })
    }
}

#[derive(Debug, Clone)]
pub(super) struct DerivedObjectHeaderPositions {
    catalog: TeletextEnhancementObjectCatalog,
    sources: ObjectHeaderSources,
}

impl DerivedObjectHeaderPositions {
    pub(super) const fn catalog(&self) -> &TeletextEnhancementObjectCatalog {
        &self.catalog
    }

    pub(super) fn retained_source(
        &self,
        actual: TeletextEnhancementSource,
    ) -> Option<TeletextEnhancementSource> {
        self.sources.retained_source(actual)
    }

    pub(super) fn into_parts(self) -> (TeletextEnhancementObjectCatalog, ObjectHeaderSources) {
        (self.catalog, self.sources)
    }
}

struct Work {
    used: usize,
    maximum: usize,
}

impl Work {
    fn charge(&mut self, amount: usize) -> Result<(), DerivationError> {
        let next = self
            .used
            .checked_add(amount)
            .ok_or(DerivationError::AllocationSize)?;
        if next > self.maximum {
            return Err(DerivationError::WorkBudget {
                maximum: self.maximum,
            });
        }
        self.used = next;
        Ok(())
    }
}

/// Returns admitted work even on failure. Unsupported contexts retain their
/// original bodies and therefore their ordinary lowering refusal.
pub(super) fn derive(
    composition: &TeletextMainComposition,
    catalog: &TeletextEnhancementObjectCatalog,
    budget: TeletextObjectCatalogBudget,
    work_ceiling: usize,
) -> (Result<DerivedObjectHeaderPositions, DerivationError>, usize) {
    let mut work = Work {
        used: 0,
        maximum: work_ceiling,
    };
    let result = derive_inner(composition, catalog, budget, &mut work);
    (result, work.used)
}

fn derive_inner(
    composition: &TeletextMainComposition,
    catalog: &TeletextEnhancementObjectCatalog,
    budget: TeletextObjectCatalogBudget,
    work: &mut Work,
) -> Result<DerivedObjectHeaderPositions, DerivationError> {
    let count = catalog.definitions().len();
    if count > budget.max_objects {
        return Err(TeletextObjectCatalogError::ObjectBudget {
            count,
            maximum: budget.max_objects,
        }
        .into());
    }
    work.charge(count)?;
    let mut commands = 0usize;
    let mut replacements = BTreeMap::new();
    for definition in catalog.definitions() {
        let length = definition.program.commands().len();
        work.charge(length)?;
        commands = commands
            .checked_add(length)
            .filter(|total| *total <= budget.max_commands)
            .ok_or(TeletextObjectCatalogError::CommandBudget {
                maximum: budget.max_commands,
            })?;
        if definition.identity.scope() != TeletextObjectScope::Local
            || definition.identity.owner() != Some(composition.owner())
            || !matches!(
                definition.object_type,
                TeletextObjectType::Active | TeletextObjectType::Adaptive
            )
        {
            continue;
        }
        let [TeletextEnhancementCommand::SetPosition { position }, TeletextEnhancementCommand::InvokeObject {
            object_type: child_type,
            ..
        }, ..] = definition.program.commands()
        else {
            continue;
        };
        if position.row() != 0
            || position.column() < 8
            || !definition.object_type.may_invoke(*child_type)
        {
            continue;
        }
        if let Some(operation) = header_operation(
            composition,
            definition.identity,
            definition.object_type,
            position.column(),
            work,
        )? {
            replacements.insert(definition.identity, (position.column(), operation));
        }
    }
    let total = commands
        .checked_add(replacements.len())
        .filter(|total| *total <= budget.max_commands)
        .ok_or(TeletextObjectCatalogError::CommandBudget {
            maximum: budget.max_commands,
        })?;
    // Reserve emission and checked catalogue reconstruction before cloning.
    work.charge(
        total
            .checked_add(count)
            .ok_or(DerivationError::AllocationSize)?,
    )?;
    let mut definitions = catalog.definitions().to_vec();
    let mut sources = ObjectHeaderSources {
        lengths: BTreeMap::new(),
        inserted: BTreeSet::new(),
    };
    for definition in &mut definitions {
        if let Some(&(column, operation)) = replacements.get(&definition.identity) {
            let mut body = definition.program.commands().to_vec();
            body.insert(
                1,
                TeletextEnhancementCommand::AtColumn { column, operation },
            );
            definition.program = TeletextEnhancementProgram::new(body)?;
            sources.inserted.insert(definition.identity);
        }
        sources
            .lengths
            .insert(definition.identity, definition.program.commands().len());
    }
    Ok(DerivedObjectHeaderPositions {
        catalog: TeletextEnhancementObjectCatalog::new(definitions, budget)?,
        sources,
    })
}

fn header_operation(
    composition: &TeletextMainComposition,
    identity: TeletextObjectIdentity,
    object_type: TeletextObjectType,
    column: u8,
    work: &mut Work,
) -> Result<Option<TeletextColumnOperation>, DerivationError> {
    if let Some(colour) = common_value(
        composition,
        identity,
        object_type,
        column,
        TeletextExecutedAttributes::background,
        work,
    )? {
        return Ok(Some(TeletextColumnOperation::Background { colour }));
    }
    if let Some(colour) = common_value(
        composition,
        identity,
        object_type,
        column,
        TeletextExecutedAttributes::foreground,
        work,
    )? {
        return Ok(Some(TeletextColumnOperation::Foreground { colour }));
    }
    Ok(common_value(
        composition,
        identity,
        object_type,
        column,
        TeletextExecutedAttributes::flash,
        work,
    )?
    // Other flash encodings may advance receiver phase counters. Mode/rate
    // equality alone does not establish preservation of that temporal state.
    .filter(|flash| flash.mode() == TeletextFlashMode::Steady && flash.rate() == TeletextFlashRate::Slow1Hz)
    .map(|flash| TeletextColumnOperation::Flash {
        mode: flash.mode(),
        rate: flash.rate(),
    }))
}

fn common_value<T: Copy + Eq>(
    composition: &TeletextMainComposition,
    identity: TeletextObjectIdentity,
    object_type: TeletextObjectType,
    relative_column: u8,
    read: fn(TeletextExecutedAttributes) -> TeletextExecutedAttribute<T>,
    work: &mut Work,
) -> Result<Option<T>, DerivationError> {
    let mut common = None;
    let width = composition.display_geometry().total_width();
    for invocation in composition.spatial_trace().invocations() {
        work.charge(1)?;
        if invocation.target() != identity {
            continue;
        }
        if invocation.object_type() != object_type {
            return Ok(None);
        }
        let origin = invocation.origin();
        let Some(first) = origin.column().checked_add(u16::from(relative_column)) else {
            return Ok(None);
        };
        let (Ok(row), Ok(first)) = (u8::try_from(origin.row()), u8::try_from(first)) else {
            return Ok(None);
        };
        if first >= width {
            return Ok(None);
        }
        // Active writes affect the remaining row; a final Adaptive call may
        // leave an open extent. Qualify every possible tail of every invocation.
        // This completed Local/Active projection is only a candidate witness,
        // not a call-time Active default. Actual C.2 and full comparison still
        // reject a candidate introducing conflicting same-priority writers.
        for column in first..width {
            work.charge(1)?;
            let Some(attributes) = composition
                .attribute_execution()
                .before_adaptive_cell(row, column)
            else {
                return Ok(None);
            };
            // The original Local/Active projection excludes Adaptive and
            // Passive composites. A value is never a guessed call-time default.
            let field = read(*attributes);
            let Some(value) = field.value().filter(|_| field.pending().is_none()) else {
                return Ok(None);
            };
            if common.is_some_and(|previous| previous != value) {
                return Ok(None);
            }
            common = Some(value);
        }
    }
    Ok(common)
}
