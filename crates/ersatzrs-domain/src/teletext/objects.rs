//! Semantic object identities and invocation type edges (EN 300 706 clause 13).
//!
//! Logical host IDs do not encode physical pointer-table locations. These values
//! do not validate an object graph, expanded display positions or publication.

use serde::{Deserialize, Serialize};

use super::{TeletextPageAddress, TeletextResourceIdentity, TeletextResourceKind};

/// Object behaviour, separate from its local/public/global source scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum TeletextObjectType {
    /// Inherits underlying attributes and can change the remainder of a row.
    Active,
    /// Adapts underlying attributes within the object's addressed span.
    Adaptive,
    /// Supplies its own attributes and changes only explicitly addressed cells.
    Passive,
}

impl TeletextObjectType {
    /// Permitted nested invocation edges from EN 300 706 clauses 13.3–13.5.
    ///
    /// This tests type permission only; source scope, target existence, positions
    /// and page-wide open-object limits require separate graph/display checks.
    #[must_use]
    pub const fn may_invoke(self, target: Self) -> bool {
        matches!(
            (self, target),
            (Self::Active, Self::Adaptive | Self::Passive) | (Self::Adaptive, Self::Passive)
        )
    }
}

/// Where an object is defined, independent of its invocation behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum TeletextObjectScope {
    /// Definition owned by one exact display page/subcode.
    Local,
    /// Definition supplied by a public object resource.
    Public,
    /// Definition supplied by a global object resource.
    Global,
}

/// Logical host object identifier; zero is valid and no physical address is implied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TeletextObjectId(u16);

impl TeletextObjectId {
    /// Preserve the complete semantic identifier without allocating pointer slots.
    #[must_use]
    pub const fn new(id: u16) -> Self {
        Self(id)
    }

    /// Original host identifier, not an EN 300 706 object number or pointer.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// Located identity error, containing checked identifiers rather than object content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextObjectIdentityError {
    #[error("teletext {scope:?} object cannot use resource {resource:?}")]
    WrongResourceKind {
        scope: TeletextObjectScope,
        resource: TeletextResourceIdentity,
    },
    #[error("teletext local object {object_id:?} belongs to {owner:?}, not {display:?}")]
    LocalOwner {
        object_id: TeletextObjectId,
        owner: TeletextPageAddress,
        display: TeletextPageAddress,
    },
}

/// Checked object location, preserving exact local subcodes and resource logical S1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "ObjectIdentityFields", into = "ObjectIdentityFields")]
pub struct TeletextObjectIdentity(ObjectIdentityFields);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "scope", rename_all = "snake_case", deny_unknown_fields)]
enum ObjectIdentityFields {
    Local {
        owner: TeletextPageAddress,
        object_id: TeletextObjectId,
    },
    Public {
        resource: TeletextResourceIdentity,
        object_id: TeletextObjectId,
    },
    Global {
        resource: TeletextResourceIdentity,
        object_id: TeletextObjectId,
    },
}

impl TeletextObjectIdentity {
    /// Associate a local definition with one exact checked display address.
    #[must_use]
    pub const fn local(owner: TeletextPageAddress, object_id: TeletextObjectId) -> Self {
        Self(ObjectIdentityFields::Local { owner, object_id })
    }

    /// Associate a public definition with a checked POP resource.
    /// # Errors
    /// Returns `WrongResourceKind` unless the resource purpose is `Pop`.
    pub fn public(
        resource: TeletextResourceIdentity,
        object_id: TeletextObjectId,
    ) -> Result<Self, TeletextObjectIdentityError> {
        if resource.kind() != TeletextResourceKind::Pop {
            return Err(TeletextObjectIdentityError::WrongResourceKind {
                scope: TeletextObjectScope::Public,
                resource,
            });
        }
        Ok(Self(ObjectIdentityFields::Public {
            resource,
            object_id,
        }))
    }

    /// Associate a global definition with a checked GPOP resource.
    /// # Errors
    /// Returns `WrongResourceKind` unless the resource purpose is `Gpop`.
    pub fn global(
        resource: TeletextResourceIdentity,
        object_id: TeletextObjectId,
    ) -> Result<Self, TeletextObjectIdentityError> {
        if resource.kind() != TeletextResourceKind::Gpop {
            return Err(TeletextObjectIdentityError::WrongResourceKind {
                scope: TeletextObjectScope::Global,
                resource,
            });
        }
        Ok(Self(ObjectIdentityFields::Global {
            resource,
            object_id,
        }))
    }

    /// Definition scope; this alone does not grant invocation permission.
    #[must_use]
    pub const fn scope(self) -> TeletextObjectScope {
        match self.0 {
            ObjectIdentityFields::Local { .. } => TeletextObjectScope::Local,
            ObjectIdentityFields::Public { .. } => TeletextObjectScope::Public,
            ObjectIdentityFields::Global { .. } => TeletextObjectScope::Global,
        }
    }

    /// Unchanged semantic identifier within this owner/resource.
    #[must_use]
    pub const fn object_id(self) -> TeletextObjectId {
        match self.0 {
            ObjectIdentityFields::Local { object_id, .. }
            | ObjectIdentityFields::Public { object_id, .. }
            | ObjectIdentityFields::Global { object_id, .. } => object_id,
        }
    }

    /// Exact display owner for a local definition; resource objects have none.
    #[must_use]
    pub const fn owner(self) -> Option<TeletextPageAddress> {
        match self.0 {
            ObjectIdentityFields::Local { owner, .. } => Some(owner),
            ObjectIdentityFields::Public { .. } | ObjectIdentityFields::Global { .. } => None,
        }
    }

    /// Checked logical resource for public/global definitions; locals have none.
    #[must_use]
    pub const fn resource(self) -> Option<TeletextResourceIdentity> {
        match self.0 {
            ObjectIdentityFields::Local { .. } => None,
            ObjectIdentityFields::Public { resource, .. }
            | ObjectIdentityFields::Global { resource, .. } => Some(resource),
        }
    }

    /// Require a local definition to belong to the exact display page/subcode.
    /// # Errors
    /// Returns `LocalOwner` for a different page or raw subcode. Resource object
    /// identities have no display owner; graph source permissions remain separate.
    pub fn validate_display_owner(
        self,
        display: TeletextPageAddress,
    ) -> Result<(), TeletextObjectIdentityError> {
        if let Some(owner) = self.owner() {
            if owner != display {
                return Err(TeletextObjectIdentityError::LocalOwner {
                    object_id: self.object_id(),
                    owner,
                    display,
                });
            }
        }
        Ok(())
    }
}

impl TryFrom<ObjectIdentityFields> for TeletextObjectIdentity {
    type Error = TeletextObjectIdentityError;

    fn try_from(value: ObjectIdentityFields) -> Result<Self, Self::Error> {
        match value {
            ObjectIdentityFields::Local { owner, object_id } => Ok(Self::local(owner, object_id)),
            ObjectIdentityFields::Public {
                resource,
                object_id,
            } => Self::public(resource, object_id),
            ObjectIdentityFields::Global {
                resource,
                object_id,
            } => Self::global(resource, object_id),
        }
    }
}

impl From<TeletextObjectIdentity> for ObjectIdentityFields {
    fn from(value: TeletextObjectIdentity) -> Self {
        value.0
    }
}
