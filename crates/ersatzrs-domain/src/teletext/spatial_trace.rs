//! Bounded spatial events from a selected or supplied semantic main.
//!
//! Positions are retained without clipping to display or side-panel geometry.
//! This is not attribute execution, glyph priority, footprint resolution, wire
//! allocation or page readiness. A child invocation preserves its caller's AP;
//! only its own commands move its fresh relative AP (EN 300 706 Annex A.4).

use super::{
    TeletextEffectiveProgramSource, TeletextEnhancementCommand, TeletextEnhancementObjectCatalog,
    TeletextEnhancementProgram, TeletextEnhancementSource, TeletextObjectIdentity,
    TeletextObjectInvocationError, TeletextObjectOffset, TeletextObjectType, TeletextPageAddress,
    TeletextSemanticService, TeletextServiceError,
};

/// Trusted limits on expanded original commands and actual object calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextSpatialTraceBudget {
    /// Includes invocation commands, repeated bodies and the effective main.
    pub max_events: usize,
    /// The main program is not an invocation; every actual child call is one.
    pub max_invocations: usize,
}

/// Original coordinate, potentially outside the eventual display area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextSpatialPosition {
    row: u16,
    column: u16,
}

impl TeletextSpatialPosition {
    /// Exact original row, without clipping or suppression.
    #[must_use]
    pub const fn row(self) -> u16 {
        self.row
    }
    /// Exact original column, including unresolved side-panel coordinates.
    #[must_use]
    pub const fn column(self) -> u16 {
        self.column
    }

    fn add(self, row: u8, column: u8) -> Option<Self> {
        Some(Self {
            row: self.row.checked_add(u16::from(row))?,
            column: self.column.checked_add(u16::from(column))?,
        })
    }
}

/// Trace-local preorder identifier; repeated calls receive distinct identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextSpatialInvocationId(usize);
impl TeletextSpatialInvocationId {
    /// Zero-based identifier within this immutable trace.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }
}

/// Root selection provenance inherited unchanged by descendants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextSpatialRootSource {
    /// Ordinal in the selected or supplied local main, including an empty one.
    Local { command: usize },
    /// Original sparse MOT default slot, not its compacted program ordinal.
    DefaultSlot { slot: u8 },
}

/// One unchanged selected or supplied command, before wire lowering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextSpatialEvent {
    command: TeletextEnhancementCommand,
    relative_before: (u8, u8),
    relative_after: (u8, u8),
    absolute_before: TeletextSpatialPosition,
    absolute_after: TeletextSpatialPosition,
    invocation: Option<TeletextSpatialInvocationId>,
    called_invocation: Option<TeletextSpatialInvocationId>,
    source: TeletextEnhancementSource,
    root_source: TeletextSpatialRootSource,
}
impl TeletextSpatialEvent {
    /// Unchanged selected or supplied command, including original default roots.
    #[must_use]
    pub const fn command(self) -> TeletextEnhancementCommand {
        self.command
    }
    /// AP in this program immediately before the command.
    #[must_use]
    pub const fn relative_before(self) -> (u8, u8) {
        self.relative_before
    }
    /// AP in this program immediately after the command (calls do not move it).
    #[must_use]
    pub const fn relative_after(self) -> (u8, u8) {
        self.relative_after
    }
    /// Original absolute AP before the command.
    #[must_use]
    pub const fn absolute_before(self) -> TeletextSpatialPosition {
        self.absolute_before
    }
    /// Original absolute AP after the command.
    #[must_use]
    pub const fn absolute_after(self) -> TeletextSpatialPosition {
        self.absolute_after
    }
    /// Executing object instance, or `None` for the main program.
    #[must_use]
    pub const fn invocation(self) -> Option<TeletextSpatialInvocationId> {
        self.invocation
    }
    /// Unique child instance for an invocation command; no other command has one.
    #[must_use]
    pub const fn called_invocation(self) -> Option<TeletextSpatialInvocationId> {
        self.called_invocation
    }
    /// Original definition identity and command ordinal.
    #[must_use]
    pub const fn source(self) -> TeletextEnhancementSource {
        self.source
    }
    /// Original main command or sparse default slot that led to this event.
    #[must_use]
    pub const fn root_source(self) -> TeletextSpatialRootSource {
        self.root_source
    }
}

/// One actual call, independent of the target's logical or physical object number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextSpatialInvocation {
    id: TeletextSpatialInvocationId,
    parent: Option<TeletextSpatialInvocationId>,
    target: TeletextObjectIdentity,
    object_type: TeletextObjectType,
    origin: TeletextSpatialPosition,
    caller_position: TeletextSpatialPosition,
    offset: Option<TeletextObjectOffset>,
    source: TeletextEnhancementSource,
    root_source: TeletextSpatialRootSource,
}
impl TeletextSpatialInvocation {
    /// Unique actual-call identifier in preorder.
    #[must_use]
    pub const fn id(self) -> TeletextSpatialInvocationId {
        self.id
    }
    /// Calling object instance, or `None` for a main/default-root call.
    #[must_use]
    pub const fn parent(self) -> Option<TeletextSpatialInvocationId> {
        self.parent
    }
    /// Full semantic target, preserving local owner and resource S1.
    #[must_use]
    pub const fn target(self) -> TeletextObjectIdentity {
        self.target
    }
    /// Checked declared target type.
    #[must_use]
    pub const fn object_type(self) -> TeletextObjectType {
        self.object_type
    }
    /// Caller absolute AP plus this call's immediate offset; child AP starts zero.
    #[must_use]
    pub const fn origin(self) -> TeletextSpatialPosition {
        self.origin
    }
    /// Parent origin plus its relative AP before this call, without the offset.
    #[must_use]
    pub const fn caller_position(self) -> TeletextSpatialPosition {
        self.caller_position
    }
    /// Original immediate offset, preserving explicit absence.
    #[must_use]
    pub const fn offset(self) -> Option<TeletextObjectOffset> {
        self.offset
    }
    /// Original caller definition and command ordinal.
    #[must_use]
    pub const fn source(self) -> TeletextEnhancementSource {
        self.source
    }
    /// Original originating local command or sparse default slot.
    #[must_use]
    pub const fn root_source(self) -> TeletextSpatialRootSource {
        self.root_source
    }
}

/// Immutable original spatial trace, with no renderer or publication authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextSpatialTrace {
    owner: TeletextPageAddress,
    original_program_source: TeletextEffectiveProgramSource,
    events: Vec<TeletextSpatialEvent>,
    invocations: Vec<TeletextSpatialInvocation>,
}
impl TeletextSpatialTrace {
    /// Exact base page/subcode whose original effective program was traced.
    #[must_use]
    pub const fn owner(&self) -> TeletextPageAddress {
        self.owner
    }
    /// Original explicit-local/default/base-only selection, including empty local.
    #[must_use]
    pub fn original_program_source(&self) -> &TeletextEffectiveProgramSource {
        &self.original_program_source
    }
    /// Original commands in logical call preorder, not packet transmission order.
    #[must_use]
    pub fn events(&self) -> &[TeletextSpatialEvent] {
        &self.events
    }
    /// Distinct actual calls in preorder, including empty object bodies.
    #[must_use]
    pub fn invocations(&self) -> &[TeletextSpatialInvocation] {
        &self.invocations
    }
}

/// Located trace rejection; it never returns a partial trace.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("teletext spatial trace for {owner:?} at {location:?}: {reason}")]
pub struct TeletextSpatialTraceError {
    pub owner: TeletextPageAddress,
    pub location: Option<TeletextEnhancementSource>,
    /// Executing instance for the failing command; root commands have none.
    pub invocation: Option<TeletextSpatialInvocationId>,
    pub root_source: Option<TeletextSpatialRootSource>,
    pub reason: TeletextSpatialTraceFailure,
}

/// Structural/work failures, distinct from future rendered-area validation.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TeletextSpatialTraceFailure {
    #[error("original spatial event budget {maximum} exhausted")]
    EventBudget { maximum: usize },
    #[error("original spatial invocation budget {maximum} exhausted")]
    InvocationBudget { maximum: usize },
    #[error(transparent)]
    Service(Box<TeletextServiceError>),
    #[error(transparent)]
    Invocation(TeletextObjectInvocationError),
    #[error("active position moves backwards from {previous:?} to {requested:?}")]
    ActivePositionOrder {
        previous: (u8, u8),
        requested: (u8, u8),
    },
    #[error("original absolute coordinate exceeds its storage range")]
    CoordinateOverflow,
    #[error("effective program and original selection provenance disagree")]
    SelectionState,
}

/// Trace the original effective program for one existing exact base page.
///
/// Both deterministic passes borrow the same immutable program/catalog. Counting
/// precedes exact output allocation; recursion is bounded by the checked catalog's
/// strictly descending object-type edges. Relative AP order is checked in every
/// program independently; a child does not change the caller's order or AP.
/// Invocation origins do not establish written areas or panel/glyph footprints.
/// # Errors
/// Rejects missing exact owners, exhausted expanded work budgets, backwards
/// relative AP moves, or arithmetic/checked-selection inconsistency. No input is
/// modified and no partial output escapes on failure.
pub fn resolve_teletext_spatial_trace(
    service: &TeletextSemanticService,
    owner: TeletextPageAddress,
    budget: TeletextSpatialTraceBudget,
) -> Result<TeletextSpatialTrace, TeletextSpatialTraceError> {
    let (selection, program) =
        service
            .effective_program(owner)
            .map_err(|error| TeletextSpatialTraceError {
                owner,
                location: None,
                invocation: None,
                root_source: None,
                reason: TeletextSpatialTraceFailure::Service(Box::new(error)),
            })?;
    resolve_program(owner, &program, service.objects(), selection, budget)
}

/// Trace a supplied host program and checked catalog as one physical Local main.
///
/// Main ordinals refer to this exact supplied program, including generated base
/// replacements and carrier calls. No base glyphs are composed or inserted here.
/// This does not establish an existing service page, selected resource links,
/// complete A.8 admission, physical transmission order or publication readiness.
/// The caller must collect the complete program through
/// `TeletextEnhancementObjectCatalog::collect_display_references` using its actual
/// original primary repertoire and validate its dependencies separately.
/// # Errors
/// Rejects expanded event/invocation budgets, broken invocation closure, backwards
/// relative AP moves or arithmetic overflow. No input is modified or partial
/// trace returned. The existing service entry retains its own default-slot tags.
pub fn resolve_teletext_spatial_program(
    owner: TeletextPageAddress,
    program: &TeletextEnhancementProgram,
    catalog: &TeletextEnhancementObjectCatalog,
    budget: TeletextSpatialTraceBudget,
) -> Result<TeletextSpatialTrace, TeletextSpatialTraceError> {
    resolve_program(
        owner,
        program,
        catalog,
        TeletextEffectiveProgramSource::Local,
        budget,
    )
}

fn resolve_program(
    owner: TeletextPageAddress,
    program: &TeletextEnhancementProgram,
    catalog: &TeletextEnhancementObjectCatalog,
    selection: TeletextEffectiveProgramSource,
    budget: TeletextSpatialTraceBudget,
) -> Result<TeletextSpatialTrace, TeletextSpatialTraceError> {
    let mut count = Walker::new(owner, catalog, &selection, budget, None);
    count.walk(program, Context::main())?;
    let output = Output {
        events: Vec::with_capacity(count.events),
        invocations: Vec::with_capacity(count.invocations),
    };
    let mut emit = Walker::new(owner, catalog, &selection, budget, Some(output));
    emit.walk(program, Context::main())?;
    // Output is absent only for the counting pass; preserve a static error rather
    // than relying on a runtime panic if the internal pass contract ever changes.
    let output = emit.output.ok_or(TeletextSpatialTraceError {
        owner,
        location: None,
        invocation: None,
        root_source: None,
        reason: TeletextSpatialTraceFailure::SelectionState,
    })?;
    Ok(TeletextSpatialTrace {
        owner,
        original_program_source: selection,
        events: output.events,
        invocations: output.invocations,
    })
}

#[derive(Clone, Copy)]
struct Context {
    object: Option<TeletextObjectIdentity>,
    invocation: Option<TeletextSpatialInvocationId>,
    origin: TeletextSpatialPosition,
    root_source: Option<TeletextSpatialRootSource>,
}
impl Context {
    const fn main() -> Self {
        Self {
            object: None,
            invocation: None,
            origin: TeletextSpatialPosition { row: 0, column: 0 },
            root_source: None,
        }
    }
}
#[derive(Clone, Copy)]
struct CallSite {
    parent: Option<TeletextSpatialInvocationId>,
    caller_position: TeletextSpatialPosition,
    source: TeletextEnhancementSource,
    root_source: TeletextSpatialRootSource,
}

struct Output {
    events: Vec<TeletextSpatialEvent>,
    invocations: Vec<TeletextSpatialInvocation>,
}
struct Walker<'a> {
    owner: TeletextPageAddress,
    catalog: &'a TeletextEnhancementObjectCatalog,
    selection: &'a TeletextEffectiveProgramSource,
    budget: TeletextSpatialTraceBudget,
    events: usize,
    invocations: usize,
    output: Option<Output>,
}
impl<'a> Walker<'a> {
    fn new(
        owner: TeletextPageAddress,
        catalog: &'a TeletextEnhancementObjectCatalog,
        selection: &'a TeletextEffectiveProgramSource,
        budget: TeletextSpatialTraceBudget,
        output: Option<Output>,
    ) -> Self {
        Self {
            owner,
            catalog,
            selection,
            budget,
            events: 0,
            invocations: 0,
            output,
        }
    }

    fn root_source(&self, context: Context, command: usize) -> Option<TeletextSpatialRootSource> {
        context.root_source.or_else(|| match self.selection {
            TeletextEffectiveProgramSource::Local => {
                Some(TeletextSpatialRootSource::Local { command })
            }
            TeletextEffectiveProgramSource::Defaults { slots } => slots
                .get(command)
                .copied()
                .map(|slot| TeletextSpatialRootSource::DefaultSlot { slot }),
            TeletextEffectiveProgramSource::BaseOnly => None,
        })
    }

    fn walk(
        &mut self,
        program: &TeletextEnhancementProgram,
        context: Context,
    ) -> Result<(), TeletextSpatialTraceError> {
        let mut ap = (0, 0);
        for (ordinal, &command) in program.commands().iter().enumerate() {
            let source = TeletextEnhancementSource {
                object: context.object,
                command: ordinal,
            };
            let root = self.root_source(context, ordinal);
            let owner = self.owner;
            let fail = |reason| TeletextSpatialTraceError {
                owner,
                location: Some(source),
                invocation: context.invocation,
                root_source: root,
                reason,
            };
            if self.events >= self.budget.max_events {
                return Err(fail(TeletextSpatialTraceFailure::EventBudget {
                    maximum: self.budget.max_events,
                }));
            }
            self.events += 1;
            let root = root.ok_or_else(|| fail(TeletextSpatialTraceFailure::SelectionState))?;
            let next = match command {
                TeletextEnhancementCommand::SetPosition { position } => {
                    (position.row(), position.column())
                }
                TeletextEnhancementCommand::SetRowColour { row, .. } => (row, 0),
                TeletextEnhancementCommand::AtColumn { column, .. } => (ap.0, column),
                TeletextEnhancementCommand::SetScreenColour { .. }
                | TeletextEnhancementCommand::InvokeObject { .. } => ap,
            };
            if next < ap {
                return Err(fail(TeletextSpatialTraceFailure::ActivePositionOrder {
                    previous: ap,
                    requested: next,
                }));
            }
            let absolute_before = context
                .origin
                .add(ap.0, ap.1)
                .ok_or_else(|| fail(TeletextSpatialTraceFailure::CoordinateOverflow))?;
            let absolute_after = context
                .origin
                .add(next.0, next.1)
                .ok_or_else(|| fail(TeletextSpatialTraceFailure::CoordinateOverflow))?;
            let child = if let TeletextEnhancementCommand::InvokeObject {
                target,
                object_type,
                offset,
            } = command
            {
                Some(self.prepare_call(
                    target,
                    object_type,
                    offset,
                    CallSite {
                        parent: context.invocation,
                        caller_position: absolute_before,
                        source,
                        root_source: root,
                    },
                )?)
            } else {
                None
            };
            if let Some(output) = &mut self.output {
                output.events.push(TeletextSpatialEvent {
                    command,
                    relative_before: ap,
                    relative_after: next,
                    absolute_before,
                    absolute_after,
                    invocation: context.invocation,
                    called_invocation: child.map(|c| c.id),
                    source,
                    root_source: root,
                });
            }
            if let Some(call) = child {
                self.walk_child(call)?;
            }
            ap = next;
        }
        Ok(())
    }

    fn prepare_call(
        &mut self,
        target: TeletextObjectIdentity,
        object_type: TeletextObjectType,
        offset: Option<TeletextObjectOffset>,
        site: CallSite,
    ) -> Result<TeletextSpatialInvocation, TeletextSpatialTraceError> {
        let CallSite {
            parent,
            caller_position,
            source,
            root_source,
        } = site;
        let owner = self.owner;
        let fail = |reason| TeletextSpatialTraceError {
            owner,
            location: Some(source),
            invocation: parent,
            root_source: Some(root_source),
            reason,
        };
        if self.invocations >= self.budget.max_invocations {
            return Err(fail(TeletextSpatialTraceFailure::InvocationBudget {
                maximum: self.budget.max_invocations,
            }));
        }
        let id = TeletextSpatialInvocationId(self.invocations);
        self.invocations += 1;
        let definition = self.catalog.definition(target).ok_or_else(|| {
            fail(TeletextSpatialTraceFailure::Invocation(
                TeletextObjectInvocationError::Missing { target },
            ))
        })?;
        if definition.object_type != object_type {
            return Err(fail(TeletextSpatialTraceFailure::Invocation(
                TeletextObjectInvocationError::Type {
                    target,
                    expected: object_type,
                    actual: definition.object_type,
                },
            )));
        }
        target.validate_display_owner(self.owner).map_err(|_| {
            fail(TeletextSpatialTraceFailure::Invocation(
                TeletextObjectInvocationError::LocalOwner {
                    target,
                    display: self.owner,
                },
            ))
        })?;
        let origin = offset
            .map_or(Some(caller_position), |value| {
                caller_position.add(value.row(), value.column())
            })
            .ok_or_else(|| fail(TeletextSpatialTraceFailure::CoordinateOverflow))?;
        let call = TeletextSpatialInvocation {
            id,
            parent,
            target,
            object_type,
            origin,
            caller_position,
            offset,
            source,
            root_source,
        };
        if let Some(output) = &mut self.output {
            output.invocations.push(call);
        }
        Ok(call)
    }

    fn walk_child(
        &mut self,
        call: TeletextSpatialInvocation,
    ) -> Result<(), TeletextSpatialTraceError> {
        // The checked catalog owns all nested edges and strictly descending types;
        // this exact lookup preserves default resource owners and full host IDs.
        let definition = self
            .catalog
            .definition(call.target)
            .ok_or(TeletextSpatialTraceError {
                owner: self.owner,
                location: Some(call.source),
                invocation: call.parent,
                root_source: Some(call.root_source),
                reason: TeletextSpatialTraceFailure::Invocation(
                    TeletextObjectInvocationError::Missing {
                        target: call.target,
                    },
                ),
            })?;
        self.walk(
            &definition.program,
            Context {
                object: Some(call.target),
                invocation: Some(call.id),
                origin: call.origin,
                root_source: Some(call.root_source),
            },
        )
    }
}
