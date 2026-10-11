//! Bounded POP/GPOP object-number allocation (EN 300 706 12.3.3/10.6.4.1).
//!
//! Semantic IDs are never truncated into physical pointers. Each resource and
//! object type has its own thirty-two slots; defaults reserve its first sixteen
//! slots, which the eight-bit MOT selector can address. This does not allocate
//! local objects, packet bodies, display geometry or publication authority.

use std::collections::BTreeMap;

use ersatzrs_domain::{
    TeletextDefaultObjectSelection, TeletextEnhancementObjectCatalog, TeletextObjectIdentity,
    TeletextObjectType, TeletextResourceIdentity,
};

use super::mot::TeletextMotDefaultAllocation;

const OBJECTS_PER_TYPE: u8 = 32;
const DEFAULTS_PER_TYPE: u8 = 16;

/// Rejected allocation, without retaining command or textual content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextObjectAllocationError {
    /// Includes local definitions; the count is checked before scanning any object.
    #[error("teletext object count {count} exceeds caller budget {maximum}")]
    ObjectBudget { count: usize, maximum: usize },
    /// Raw supplied slots consume validation work even when targets repeat.
    #[error("teletext default input count {count} exceeds caller budget {maximum}")]
    DefaultBudget { count: usize, maximum: usize },
    #[error("teletext default {ordinal}, target {target:?}, type {object_type:?} is not a resource object")]
    DefaultScope {
        ordinal: usize,
        target: TeletextObjectIdentity,
        object_type: TeletextObjectType,
    },
    #[error("teletext default {ordinal}, target {target:?}, type {object_type:?} is missing")]
    MissingDefault {
        ordinal: usize,
        target: TeletextObjectIdentity,
        object_type: TeletextObjectType,
    },
    #[error(
        "teletext default {ordinal}, target {target:?} has type {actual:?}, expected {expected:?}"
    )]
    DefaultType {
        ordinal: usize,
        target: TeletextObjectIdentity,
        expected: TeletextObjectType,
        actual: TeletextObjectType,
    },
    #[error("teletext resource {resource:?}, type {object_type:?}, target {target:?}: object count {count} exceeds {maximum}")]
    ObjectCapacity {
        resource: TeletextResourceIdentity,
        object_type: TeletextObjectType,
        target: TeletextObjectIdentity,
        count: usize,
        maximum: usize,
    },
    #[error("teletext resource {resource:?}, type {object_type:?}, target {target:?}: distinct default count {count} exceeds {maximum}")]
    DefaultCapacity {
        resource: TeletextResourceIdentity,
        object_type: TeletextObjectType,
        target: TeletextObjectIdentity,
        count: usize,
        maximum: usize,
    },
}

/// One exact semantic target/type associated with a nine-bit physical number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextResourceObjectAllocation {
    target: TeletextObjectIdentity,
    object_type: TeletextObjectType,
    object_number: u16,
}

impl TeletextResourceObjectAllocation {
    /// Complete resource scope/page/S1 and unchanged logical host ID.
    #[must_use]
    pub const fn target(&self) -> TeletextObjectIdentity {
        self.target
    }

    /// Pointer-table object type, not inferred from the semantic ID.
    #[must_use]
    pub const fn object_type(&self) -> TeletextObjectType {
        self.object_type
    }

    /// `S1 | half<<4 | group<<5 | pointer_packet_index<<7`, in zero through 511.
    #[must_use]
    pub const fn object_number(&self) -> u16 {
        self.object_number
    }

    /// An exact MOT selector when this pointer lies in packets one or two.
    ///
    /// Later pointer packets need nine-bit explicit invocation and cannot be
    /// represented by MOT's eight-bit default selector. This does not make the
    /// object a selected default or validate any MOT declaration.
    #[must_use]
    pub fn mot_default_allocation(&self) -> Option<TeletextMotDefaultAllocation> {
        u8::try_from(self.object_number)
            .ok()
            .map(|selector| TeletextMotDefaultAllocation {
                target: self.target,
                object_type: self.object_type,
                selector,
            })
    }
}

/// Immutable bounded assignment with exact semantic and resource indexes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextResourceObjectAllocations {
    entries: Vec<TeletextResourceObjectAllocation>,
    index: BTreeMap<(TeletextObjectIdentity, TeletextObjectType), usize>,
    resources: BTreeMap<TeletextResourceIdentity, Vec<usize>>,
}

impl TeletextResourceObjectAllocations {
    /// Assignment order: distinct defaults first, then remaining authored definitions.
    #[must_use]
    pub fn entries(&self) -> &[TeletextResourceObjectAllocation] {
        &self.entries
    }

    /// Lookup requires both the complete target identity and its exact type.
    #[must_use]
    pub fn lookup(
        &self,
        target: TeletextObjectIdentity,
        object_type: TeletextObjectType,
    ) -> Option<&TeletextResourceObjectAllocation> {
        self.index
            .get(&(target, object_type))
            .map(|&ordinal| &self.entries[ordinal])
    }

    /// Unique exact resources in canonical domain identity order.
    pub fn resources(&self) -> impl Iterator<Item = TeletextResourceIdentity> + '_ {
        self.resources.keys().copied()
    }

    /// Borrow assigned entries for one exact resource, preserving assignment order.
    /// Unknown resources yield no entries; another S1 or purpose never matches.
    pub fn resource_entries(
        &self,
        resource: TeletextResourceIdentity,
    ) -> impl Iterator<Item = &TeletextResourceObjectAllocation> {
        self.resources
            .get(&resource)
            .into_iter()
            .flatten()
            .map(|&ordinal| &self.entries[ordinal])
    }
}

/// Assign bounded resource pointers without changing semantic ownership or S1.
///
/// Both catalog definition count and raw default-input count must fit the trusted
/// caller budget before any scanning or new indexing. Every default is checked
/// against the complete existing catalog before allocating slots. Repeated valid
/// defaults consume validation work but no additional physical/default slots.
/// Distinct defaults retain supplied order; remaining definitions retain catalog
/// authored order. Local definitions deliberately remain unallocated for later
/// local-page packing. The catalog already bounds and validates its command
/// closure; this routine does not clone or execute those programs.
/// # Errors
/// Rejects input budgets, invalid default existence/type/scope, a seventeenth
/// distinct default or thirty-third object of one type in one exact resource.
/// Errors retain full identities; no spill, rebinding or partial result occurs.
pub fn allocate_teletext_resource_objects(
    catalog: &TeletextEnhancementObjectCatalog,
    defaults: &[TeletextDefaultObjectSelection],
    max_objects: usize,
) -> Result<TeletextResourceObjectAllocations, TeletextObjectAllocationError> {
    let count = catalog.definitions().len();
    if count > max_objects {
        return Err(TeletextObjectAllocationError::ObjectBudget {
            count,
            maximum: max_objects,
        });
    }
    if defaults.len() > max_objects {
        return Err(TeletextObjectAllocationError::DefaultBudget {
            count: defaults.len(),
            maximum: max_objects,
        });
    }
    validate_defaults(catalog, defaults)?;
    let mut allocations = TeletextResourceObjectAllocations {
        entries: Vec::new(),
        index: BTreeMap::new(),
        resources: BTreeMap::new(),
    };
    let mut counts = BTreeMap::new();
    for default in defaults {
        // Validation above establishes a resource before any slot is assigned.
        if let Some(resource) = default.target.resource() {
            assign(
                &mut allocations,
                &mut counts,
                resource,
                default.target,
                default.object_type,
                true,
            )?;
        }
    }
    for definition in catalog.definitions() {
        if let Some(resource) = definition.identity.resource() {
            assign(
                &mut allocations,
                &mut counts,
                resource,
                definition.identity,
                definition.object_type,
                false,
            )?;
        }
    }
    Ok(allocations)
}

fn validate_defaults(
    catalog: &TeletextEnhancementObjectCatalog,
    defaults: &[TeletextDefaultObjectSelection],
) -> Result<(), TeletextObjectAllocationError> {
    for (ordinal, default) in defaults.iter().enumerate() {
        if default.target.resource().is_none() {
            return Err(TeletextObjectAllocationError::DefaultScope {
                ordinal,
                target: default.target,
                object_type: default.object_type,
            });
        }
        let definition = catalog.definition(default.target).ok_or(
            TeletextObjectAllocationError::MissingDefault {
                ordinal,
                target: default.target,
                object_type: default.object_type,
            },
        )?;
        if definition.object_type != default.object_type {
            return Err(TeletextObjectAllocationError::DefaultType {
                ordinal,
                target: default.target,
                expected: default.object_type,
                actual: definition.object_type,
            });
        }
    }
    Ok(())
}

#[derive(Default)]
struct AllocationCounts {
    total: u8,
    defaults: u8,
}

fn assign(
    allocations: &mut TeletextResourceObjectAllocations,
    counts: &mut BTreeMap<(TeletextResourceIdentity, TeletextObjectType), AllocationCounts>,
    resource: TeletextResourceIdentity,
    target: TeletextObjectIdentity,
    object_type: TeletextObjectType,
    is_default: bool,
) -> Result<(), TeletextObjectAllocationError> {
    if allocations.index.contains_key(&(target, object_type)) {
        return Ok(());
    }
    let count = counts.entry((resource, object_type)).or_default();
    if is_default && count.defaults >= DEFAULTS_PER_TYPE {
        return Err(TeletextObjectAllocationError::DefaultCapacity {
            resource,
            object_type,
            target,
            count: usize::from(count.defaults) + 1,
            maximum: usize::from(DEFAULTS_PER_TYPE),
        });
    }
    if count.total >= OBJECTS_PER_TYPE {
        return Err(TeletextObjectAllocationError::ObjectCapacity {
            resource,
            object_type,
            target,
            count: usize::from(count.total) + 1,
            maximum: usize::from(OBJECTS_PER_TYPE),
        });
    }
    let ordinal = u16::from(count.total);
    let object_number = u16::from(resource.logical_subpage().get())
        | ((ordinal % 2) << 4)
        | (((ordinal % 8) / 2) << 5)
        | ((ordinal / 8) << 7);
    let entry = allocations.entries.len();
    allocations.entries.push(TeletextResourceObjectAllocation {
        target,
        object_type,
        object_number,
    });
    allocations.index.insert((target, object_type), entry);
    allocations
        .resources
        .entry(resource)
        .or_default()
        .push(entry);
    count.total += 1;
    if is_default {
        count.defaults += 1;
    }
    Ok(())
}
