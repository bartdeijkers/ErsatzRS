//! Context-qualified physical bodies, separate from immutable semantic bodies.

use std::collections::BTreeMap;

use ersatzrs_domain::{
    TeletextColourReference, TeletextColumnOperation, TeletextEnhancementCommand,
    TeletextEnhancementObjectCatalog, TeletextEnhancementProgram, TeletextEnhancementProgramError,
    TeletextEnhancementSource, TeletextObjectCatalogBudget, TeletextObjectCatalogError,
    TeletextObjectIdentity, TeletextObjectScope, TeletextObjectType,
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
    inserted: BTreeMap<TeletextObjectIdentity, [Option<usize>; 3]>,
}

impl ObjectHeaderSources {
    /// The only derived body writer materializes its original leading AP.
    pub(super) fn materialized_position(&self, actual: TeletextEnhancementSource) -> Option<usize> {
        let mapping = self.inserted.get(&actual.object?)?;
        (mapping.get(actual.command) == Some(&None)).then_some(0)
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
        let command = match self.inserted.get(&identity) {
            Some(mapping) => *mapping.get(actual.command)?,
            None => Some(actual.command),
        }?;
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
        }] = definition.program.commands()
        else {
            continue;
        };
        if position.row() != 0
            || position.column() < 8
            || !definition.object_type.may_invoke(*child_type)
        {
            continue;
        }
        if let Some(colour) = common_background(
            composition,
            definition.identity,
            definition.object_type,
            position.column(),
            work,
        )? {
            replacements.insert(definition.identity, (position.column(), colour));
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
        inserted: BTreeMap::new(),
    };
    for definition in &mut definitions {
        if let Some(&(column, colour)) = replacements.get(&definition.identity) {
            let mut body = definition.program.commands().to_vec();
            body.insert(
                1,
                TeletextEnhancementCommand::AtColumn {
                    column,
                    operation: TeletextColumnOperation::Background { colour },
                },
            );
            definition.program = TeletextEnhancementProgram::new(body)?;
            sources
                .inserted
                .insert(definition.identity, [Some(0), None, Some(1)]);
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

fn common_background(
    composition: &TeletextMainComposition,
    identity: TeletextObjectIdentity,
    object_type: TeletextObjectType,
    relative_column: u8,
    work: &mut Work,
) -> Result<Option<TeletextColourReference>, DerivationError> {
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
            let field = attributes.background();
            let Some(colour) = field.value().filter(|_| field.pending().is_none()) else {
                return Ok(None);
            };
            if common.is_some_and(|previous| previous != colour) {
                return Ok(None);
            }
            common = Some(colour);
        }
    }
    Ok(common)
}
