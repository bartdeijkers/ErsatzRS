//! Checked additions to existing resource catalogues, without authoring authority.
use super::{
    TeletextEnhancementObjectDefinition, TeletextObjectIdentity, TeletextResourceIdentity,
    TeletextServiceBudget, TeletextServiceError,
};

/// A definition addition rejected before service ownership can be checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextResourceObjectDefinitionAdditionError {
    #[error("teletext definition addition requires a Public POP or Global GPOP identity")]
    Scope,
    #[error("teletext definition addition exceeds caller {dimension} budget {maximum}")]
    Budget {
        dimension: &'static str,
        maximum: usize,
    },
}

/// One new complete resource definition, retaining its checked native identity.
///
/// This does not create a resource, replace an existing definition or grant edit,
/// package or publication authority. Applying it requires an unused identity in
/// an existing compatible resource and ordinary complete service validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextResourceObjectDefinitionAddition {
    definition: TeletextEnhancementObjectDefinition,
    resource: TeletextResourceIdentity,
}

impl TeletextResourceObjectDefinitionAddition {
    /// Check resource scope and trusted input bounds without resolving ownership.
    ///
    /// Active, Adaptive and Passive definitions retain their existing typed
    /// program semantics. Dependencies and actual physical capacity require full
    /// service reconstruction and native compilation, respectively.
    /// # Errors
    /// Rejects Local identities or exhausted object, structure or command budgets.
    pub fn new(
        definition: TeletextEnhancementObjectDefinition,
        budget: TeletextServiceBudget,
    ) -> Result<Self, TeletextResourceObjectDefinitionAdditionError> {
        use TeletextResourceObjectDefinitionAdditionError as Error;
        let Some(resource) = definition.identity.resource() else {
            return Err(Error::Scope);
        };
        for (dimension, count, maximum) in [
            ("object", 1, budget.max_objects),
            ("structure item", 1, budget.max_structure_items),
            (
                "authored command",
                definition.program.commands().len(),
                budget.max_commands,
            ),
        ] {
            if count > maximum {
                return Err(Error::Budget { dimension, maximum });
            }
        }
        Ok(Self {
            definition,
            resource,
        })
    }

    /// Complete checked Public/Global definition; no mutable access is granted.
    #[must_use]
    pub const fn definition(&self) -> &TeletextEnhancementObjectDefinition {
        &self.definition
    }

    /// Exact resource purpose, raw page, logical S1 and new object ID.
    #[must_use]
    pub const fn identity(&self) -> TeletextObjectIdentity {
        self.definition.identity
    }

    pub(super) const fn resource(&self) -> TeletextResourceIdentity {
        self.resource
    }
}

/// A complete addition batch rejected without returning a partial service.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TeletextResourceObjectDefinitionAdditionApplyError {
    #[error("teletext definition additions must not be empty")]
    Empty,
    #[error("teletext definition addition {identity:?} is repeated")]
    Duplicate { identity: TeletextObjectIdentity },
    #[error("teletext definition addition {identity:?} already exists")]
    Occupied { identity: TeletextObjectIdentity },
    #[error("teletext definition addition requires existing resource {identity:?}")]
    MissingResource { identity: TeletextResourceIdentity },
    #[error(transparent)]
    Service(#[from] TeletextServiceError),
}
