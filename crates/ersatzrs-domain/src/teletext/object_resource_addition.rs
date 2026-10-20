//! Explicit new resource sets, separate from additions to retained object banks.
use super::{
    TeletextDeclaredServiceError, TeletextEnhancementObjectDefinition, TeletextObjectIdentity,
    TeletextResourceIdentity, TeletextResourceKind, TeletextServiceBudget, TeletextServiceError,
};
use std::collections::BTreeSet;

/// One complete new POP/GPOP logical bank, including an explicitly empty bank.
///
/// This native intent grants no editor, package, storage or publication authority.
/// Existing-bank Add/Copy and uploaded TTI cannot implicitly construct it. The
/// host must separately authorize the new declaration, dependencies and lifecycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextObjectResourceAddition {
    resource: TeletextResourceIdentity,
    definitions: Vec<TeletextEnhancementObjectDefinition>,
}

/// A new bank rejected before its existing-service ownership can be checked.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TeletextObjectResourceAdditionError {
    #[error("teletext object resource addition requires POP/GPOP, received {resource:?}")]
    Kind { resource: TeletextResourceIdentity },
    #[error("teletext definition {identity:?} does not belong to new resource {resource:?}")]
    Owner {
        resource: TeletextResourceIdentity,
        identity: TeletextObjectIdentity,
    },
    #[error("teletext new resource repeats definition {identity:?}")]
    DuplicateDefinition { identity: TeletextObjectIdentity },
    #[error("teletext new resource exceeds caller {dimension} budget {maximum}")]
    Budget {
        dimension: &'static str,
        maximum: usize,
    },
}

impl TeletextObjectResourceAddition {
    /// Check raw bounds, purpose and exact native definition owners.
    ///
    /// Definition types/programs remain intact. Whole-service graph validation,
    /// physical packing and declaration/selection checks are separate boundaries.
    /// # Errors
    /// Rejects exhausted limits, non-object resources, foreign/Local definitions
    /// and duplicate object identities. No definitions are truncated or rebound.
    pub fn new(
        resource: TeletextResourceIdentity,
        definitions: Vec<TeletextEnhancementObjectDefinition>,
        budget: TeletextServiceBudget,
    ) -> Result<Self, TeletextObjectResourceAdditionError> {
        use TeletextObjectResourceAdditionError as Error;
        for (dimension, count, maximum) in [
            ("record", 1, budget.max_records),
            ("object", definitions.len(), budget.max_objects),
            (
                "structure item",
                usize::from(definitions.is_empty()),
                budget.max_structure_items,
            ),
        ] {
            if count > maximum {
                return Err(Error::Budget { dimension, maximum });
            }
        }
        if !matches!(
            resource.kind(),
            TeletextResourceKind::Pop | TeletextResourceKind::Gpop
        ) {
            return Err(Error::Kind { resource });
        }
        let mut commands = budget.max_commands;
        let mut identities = BTreeSet::new();
        for definition in &definitions {
            commands = commands
                .checked_sub(definition.program.commands().len())
                .ok_or(Error::Budget {
                    dimension: "authored command",
                    maximum: budget.max_commands,
                })?;
            let identity = definition.identity;
            if identity.resource() != Some(resource) {
                return Err(Error::Owner { resource, identity });
            }
            if !identities.insert(identity) {
                return Err(Error::DuplicateDefinition { identity });
            }
        }
        Ok(Self {
            resource,
            definitions,
        })
    }

    /// Exact kind, raw page and logical S1, without physical update metadata.
    #[must_use]
    pub const fn resource(&self) -> TeletextResourceIdentity {
        self.resource
    }

    /// Complete checked-owner definitions, including dormant graph members.
    #[must_use]
    pub fn definitions(&self) -> &[TeletextEnhancementObjectDefinition] {
        &self.definitions
    }
}

/// A batch rejected without exposing a partially extended service.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TeletextObjectResourceAdditionApplyError {
    #[error("teletext new resource batch must not be empty")]
    Empty,
    #[error("teletext resource addition repeats or occupies {resource:?}")]
    Occupied { resource: TeletextResourceIdentity },
    #[error(transparent)]
    Service(#[from] TeletextServiceError),
    #[error(transparent)]
    Definition(super::TeletextResourceObjectDefinitionAdditionApplyError),
    #[error(transparent)]
    Declaration(#[from] TeletextDeclaredServiceError),
}

impl From<super::TeletextResourceObjectDefinitionAdditionApplyError>
    for TeletextObjectResourceAdditionApplyError
{
    fn from(error: super::TeletextResourceObjectDefinitionAdditionApplyError) -> Self {
        match error {
            super::TeletextResourceObjectDefinitionAdditionApplyError::Service(reason) => {
                Self::Service(reason)
            }
            reason => Self::Definition(reason),
        }
    }
}
