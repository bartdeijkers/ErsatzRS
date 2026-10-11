//! Checked object closure and bounded semantic reference collection (ADR 0123).
//!
//! This walks the effective enhancement program supplied by the caller. Default
//! object selection, resolved positions, attribute inheritance, overlap, side
//! panels and physical resource allocation remain compiler responsibilities.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::{
    TeletextCharsetPair, TeletextColumnOperation, TeletextDrcsCatalog, TeletextDrcsGlyphReference,
    TeletextDrcsPageLinks, TeletextDrcsResourceError, TeletextEnhancementCommand,
    TeletextEnhancementProgram, TeletextObjectIdentity, TeletextObjectScope, TeletextObjectType,
    TeletextPageAddress,
};

/// One semantic definition; the program and identity have already been checked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeletextEnhancementObjectDefinition {
    pub identity: TeletextObjectIdentity,
    pub object_type: TeletextObjectType,
    pub program: TeletextEnhancementProgram,
}

/// Source command location, before the compiler resolves display coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextEnhancementSource {
    /// `None` denotes the effective page program, rather than a definition.
    pub object: Option<TeletextObjectIdentity>,
    /// Zero-based ordinal in the authored program, preserved without sorting.
    pub command: usize,
}

/// One expanded invocation, including repeated uses and its caller location.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextCollectedObjectInvocation {
    pub target: TeletextObjectIdentity,
    pub source_location: TeletextEnhancementSource,
}

/// Definition closure failure, including unused definitions with broken links.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TeletextObjectCatalogError {
    #[error("teletext object count {count} exceeds caller budget {maximum}")]
    ObjectBudget { count: usize, maximum: usize },
    #[error("teletext object command count exceeds caller budget {maximum}")]
    CommandBudget { maximum: usize },
    #[error("teletext object identity {identity:?} is repeated")]
    Duplicate { identity: TeletextObjectIdentity },
    #[error("teletext object {owner:?}, command {command}: {reason}")]
    Invocation {
        owner: TeletextObjectIdentity,
        command: usize,
        reason: TeletextObjectInvocationError,
    },
}

/// A rejected invocation, independent of resolved geometry and pointer slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextObjectInvocationError {
    #[error("target object {target:?} is missing")]
    Missing { target: TeletextObjectIdentity },
    #[error("target object {target:?} has type {actual:?}, expected {expected:?}")]
    Type {
        target: TeletextObjectIdentity,
        expected: TeletextObjectType,
        actual: TeletextObjectType,
    },
    #[error("{caller:?} object cannot invoke {target_type:?} object {target:?}")]
    TypeEdge {
        caller: TeletextObjectType,
        target: TeletextObjectIdentity,
        target_type: TeletextObjectType,
    },
    #[error("resource object {caller:?} cannot invoke local object {target:?}")]
    ResourceToLocal {
        caller: TeletextObjectIdentity,
        target: TeletextObjectIdentity,
    },
    #[error("local object {target:?} does not belong to display {display:?}")]
    LocalOwner {
        target: TeletextObjectIdentity,
        display: TeletextPageAddress,
    },
}

/// Trusted caller limits, separate from the producer's serialized content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextObjectCatalogBudget {
    pub max_objects: usize,
    /// Aggregate commands in all definitions, including uninvoked definitions.
    pub max_commands: usize,
}

/// Immutable checked closure. Deserialize definitions, then apply host budgets
/// through `new`; the catalog deliberately has no unbounded `Deserialize` path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextEnhancementObjectCatalog {
    definitions: Vec<TeletextEnhancementObjectDefinition>,
    index: BTreeMap<TeletextObjectIdentity, usize>,
}

impl TeletextEnhancementObjectCatalog {
    /// Check count/work limits before indexing, then validate every graph edge.
    /// # Errors
    /// Rejects duplicates, missing/mistyped targets, forbidden nesting or local
    /// owner escape. Type edges strictly descend, so accepted graphs are acyclic.
    /// Callers must also bound serialized input bytes before parsing definitions.
    pub fn new(
        definitions: Vec<TeletextEnhancementObjectDefinition>,
        budget: TeletextObjectCatalogBudget,
    ) -> Result<Self, TeletextObjectCatalogError> {
        if definitions.len() > budget.max_objects {
            return Err(TeletextObjectCatalogError::ObjectBudget {
                count: definitions.len(),
                maximum: budget.max_objects,
            });
        }
        let mut remaining = budget.max_commands;
        for definition in &definitions {
            remaining = remaining
                .checked_sub(definition.program.commands().len())
                .ok_or(TeletextObjectCatalogError::CommandBudget {
                    maximum: budget.max_commands,
                })?;
        }
        let mut index = BTreeMap::new();
        for (ordinal, definition) in definitions.iter().enumerate() {
            if index.insert(definition.identity, ordinal).is_some() {
                return Err(TeletextObjectCatalogError::Duplicate {
                    identity: definition.identity,
                });
            }
        }
        let catalog = Self { definitions, index };
        for definition in &catalog.definitions {
            for (command, value) in definition.program.commands().iter().enumerate() {
                if let TeletextEnhancementCommand::InvokeObject {
                    target,
                    object_type,
                    ..
                } = value
                {
                    catalog
                        .validate_invocation(*target, *object_type, Some(definition))
                        .map_err(|reason| TeletextObjectCatalogError::Invocation {
                            owner: definition.identity,
                            command,
                            reason,
                        })?;
                }
            }
        }
        Ok(catalog)
    }

    /// Original authored order; the lookup index does not reorder definitions.
    #[must_use]
    pub fn definitions(&self) -> &[TeletextEnhancementObjectDefinition] {
        &self.definitions
    }

    /// Lookup one exact semantic definition without scanning other objects.
    #[must_use]
    pub fn definition(
        &self,
        identity: TeletextObjectIdentity,
    ) -> Option<&TeletextEnhancementObjectDefinition> {
        self.index
            .get(&identity)
            .map(|&ordinal| &self.definitions[ordinal])
    }

    fn validate_invocation(
        &self,
        target: TeletextObjectIdentity,
        expected: TeletextObjectType,
        caller: Option<&TeletextEnhancementObjectDefinition>,
    ) -> Result<&TeletextEnhancementObjectDefinition, TeletextObjectInvocationError> {
        let definition = self
            .definition(target)
            .ok_or(TeletextObjectInvocationError::Missing { target })?;
        if definition.object_type != expected {
            return Err(TeletextObjectInvocationError::Type {
                target,
                expected,
                actual: definition.object_type,
            });
        }
        if let Some(caller) = caller {
            if !caller.object_type.may_invoke(expected) {
                return Err(TeletextObjectInvocationError::TypeEdge {
                    caller: caller.object_type,
                    target,
                    target_type: expected,
                });
            }
            if target.scope() == TeletextObjectScope::Local {
                let display = caller.identity.owner().ok_or(
                    TeletextObjectInvocationError::ResourceToLocal {
                        caller: caller.identity,
                        target,
                    },
                )?;
                target
                    .validate_display_owner(display)
                    .map_err(|_| TeletextObjectInvocationError::LocalOwner { target, display })?;
            }
        }
        Ok(definition)
    }

    /// Collect complete references from an already selected effective program.
    /// # Errors
    /// Rejects caller work/output budgets, broken root invocations, a third
    /// primary/X26 repertoire pair or more than 127 invocations of any one type.
    /// Each invocation is traversed again: repeated use still consumes work.
    /// This does not validate spatial open-object limits or resolved glyphs.
    pub fn collect_display_references(
        &self,
        display: TeletextPageAddress,
        program: &TeletextEnhancementProgram,
        primary_pair: TeletextCharsetPair,
        budget: TeletextEnhancementCollectionBudget,
    ) -> Result<TeletextCollectedEnhancementReferences, TeletextEnhancementCollectionError> {
        let mut state = CollectionState {
            display,
            budget,
            commands: 0,
            invocations: [0; 3],
            pairs: BTreeSet::from([primary_pair]),
            references: Vec::new(),
            sources: Vec::new(),
            object_references: Vec::new(),
        };
        self.collect_program(program, None, &mut state)?;
        Ok(TeletextCollectedEnhancementReferences {
            display,
            references: state.references,
            sources: state.sources,
            pairs: state.pairs,
            commands: state.commands,
            invocations: state.invocations,
            object_references: state.object_references,
        })
    }

    fn collect_program(
        &self,
        program: &TeletextEnhancementProgram,
        owner: Option<TeletextObjectIdentity>,
        state: &mut CollectionState,
    ) -> Result<(), TeletextEnhancementCollectionError> {
        for (command, value) in program.commands().iter().enumerate() {
            let source = TeletextEnhancementSource {
                object: owner,
                command,
            };
            let fail = |reason| TeletextEnhancementCollectionError {
                display: state.display,
                source_location: source,
                reason,
            };
            if state.commands == state.budget.max_commands {
                return Err(fail(TeletextEnhancementCollectionFailure::CommandBudget {
                    maximum: state.budget.max_commands,
                }));
            }
            state.commands += 1;
            match value {
                TeletextEnhancementCommand::InvokeObject {
                    target,
                    object_type,
                    ..
                } => {
                    // Catalog closure has checked nested edges; root ownership is
                    // display-specific and must be checked on every collection.
                    target.validate_display_owner(state.display).map_err(|_| {
                        fail(TeletextEnhancementCollectionFailure::Invocation(
                            TeletextObjectInvocationError::LocalOwner {
                                target: *target,
                                display: state.display,
                            },
                        ))
                    })?;
                    let definition = self
                        .validate_invocation(*target, *object_type, None)
                        .map_err(|reason| {
                            fail(TeletextEnhancementCollectionFailure::Invocation(reason))
                        })?;
                    let type_index = object_type_index(*object_type);
                    if state.invocations[type_index] == 127 {
                        return Err(fail(
                            TeletextEnhancementCollectionFailure::InvocationCount {
                                object_type: *object_type,
                            },
                        ));
                    }
                    state.invocations[type_index] += 1;
                    state
                        .object_references
                        .push(TeletextCollectedObjectInvocation {
                            target: *target,
                            source_location: source,
                        });
                    self.collect_program(&definition.program, Some(*target), state)?;
                }
                TeletextEnhancementCommand::AtColumn { operation, .. } => match operation {
                    TeletextColumnOperation::DesignateCharset { designation } => {
                        if !state.pairs.contains(&designation.pair()) && state.pairs.len() == 2 {
                            return Err(fail(TeletextEnhancementCollectionFailure::CharsetPairs {
                                pair: designation.pair(),
                            }));
                        }
                        state.pairs.insert(designation.pair());
                    }
                    TeletextColumnOperation::Drcs { reference } => {
                        if state.references.len() == state.budget.max_drcs_references {
                            return Err(fail(
                                TeletextEnhancementCollectionFailure::ReferenceBudget {
                                    maximum: state.budget.max_drcs_references,
                                },
                            ));
                        }
                        state.references.push(*reference);
                        state.sources.push(source);
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        Ok(())
    }
}

const fn object_type_index(value: TeletextObjectType) -> usize {
    match value {
        TeletextObjectType::Active => 0,
        TeletextObjectType::Adaptive => 1,
        TeletextObjectType::Passive => 2,
    }
}

/// Host-owned traversal and output budgets; not authored or deserialized values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextEnhancementCollectionBudget {
    pub max_commands: usize,
    pub max_drcs_references: usize,
}

/// Failed collection; no partially collected result escapes to a publisher.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("teletext display {display:?}, source {source_location:?}: {reason}")]
pub struct TeletextEnhancementCollectionError {
    pub display: TeletextPageAddress,
    pub source_location: TeletextEnhancementSource,
    pub reason: TeletextEnhancementCollectionFailure,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TeletextEnhancementCollectionFailure {
    #[error(transparent)]
    Invocation(#[from] TeletextObjectInvocationError),
    #[error("command work exceeds caller budget {maximum}")]
    CommandBudget { maximum: usize },
    #[error("DRCS reference output exceeds caller budget {maximum}")]
    ReferenceBudget { maximum: usize },
    #[error("Level 2.5 permits only two enhancement repertoire pairs; adding {pair:?}")]
    CharsetPairs { pair: TeletextCharsetPair },
    #[error("Level 2.5 permits only 127 {object_type:?} invocations per page")]
    InvocationCount { object_type: TeletextObjectType },
}

struct CollectionState {
    display: TeletextPageAddress,
    budget: TeletextEnhancementCollectionBudget,
    commands: usize,
    invocations: [usize; 3],
    pairs: BTreeSet<TeletextCharsetPair>,
    references: Vec<TeletextDrcsGlyphReference>,
    sources: Vec<TeletextEnhancementSource>,
    object_references: Vec<TeletextCollectedObjectInvocation>,
}

/// References from one successful traversal; not an encoded/ready generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextCollectedEnhancementReferences {
    display: TeletextPageAddress,
    references: Vec<TeletextDrcsGlyphReference>,
    sources: Vec<TeletextEnhancementSource>,
    pairs: BTreeSet<TeletextCharsetPair>,
    commands: usize,
    invocations: [usize; 3],
    object_references: Vec<TeletextCollectedObjectInvocation>,
}

impl TeletextCollectedEnhancementReferences {
    /// Expanded calls remain bounded by command work and 127 calls per type.
    #[must_use]
    pub fn object_references(&self) -> &[TeletextCollectedObjectInvocation] {
        &self.object_references
    }
    #[must_use]
    pub fn references(&self) -> &[TeletextDrcsGlyphReference] {
        &self.references
    }
    #[must_use]
    pub fn sources(&self) -> &[TeletextEnhancementSource] {
        &self.sources
    }
    #[must_use]
    pub fn charset_pairs(&self) -> &BTreeSet<TeletextCharsetPair> {
        &self.pairs
    }
    #[must_use]
    pub const fn command_count(&self) -> usize {
        self.commands
    }
    #[must_use]
    pub fn invocation_count(&self, object_type: TeletextObjectType) -> usize {
        self.invocations[object_type_index(object_type)]
    }

    /// Resolve all collected glyphs against the existing checked DRCS catalog.
    /// # Errors
    /// Preserves the original DRCS diagnostic and maps its reference ordinal
    /// back to the semantic source. Link-only failures have no command location.
    pub fn validate_drcs(
        &self,
        catalog: &TeletextDrcsCatalog,
        links: TeletextDrcsPageLinks,
    ) -> Result<(), TeletextCollectedDrcsError> {
        catalog
            .validate_display_usage(self.display, links, &self.references)
            .map_err(|error| {
                let reference = match &error {
                    TeletextDrcsResourceError::MissingLink { reference, .. }
                    | TeletextDrcsResourceError::MissingSubtable { reference, .. }
                    | TeletextDrcsResourceError::MissingGlyph { reference, .. }
                    | TeletextDrcsResourceError::DisplaySubtables { reference, .. }
                    | TeletextDrcsResourceError::DisplayGlyphs { reference, .. } => {
                        Some(*reference)
                    }
                    _ => None,
                };
                TeletextCollectedDrcsError {
                    source_location: reference.and_then(|r| self.sources.get(r).copied()),
                    error,
                }
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{error}; enhancement source {source_location:?}")]
pub struct TeletextCollectedDrcsError {
    pub source_location: Option<TeletextEnhancementSource>,
    #[source]
    pub error: TeletextDrcsResourceError,
}
