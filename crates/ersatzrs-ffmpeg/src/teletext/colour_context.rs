//! Original display colours before inserting positioning-only X26 triplets.
//!
//! EN 300 706 table 28, clauses 13.2/13.3 and Annex A.5. This bounded prepass
//! resolves the effective semantic program, including later Active invocations;
//! it does not encode packets, validate glyph/panel geometry or grant readiness.

use ersatzrs_domain::{
    TeletextColourReference, TeletextEffectivePresentation, TeletextEnhancementCommand,
    TeletextEnhancementObjectCatalog, TeletextEnhancementProgram, TeletextEnhancementSource,
    TeletextObjectIdentity, TeletextObjectType, TeletextPageAddress, TeletextSemanticService,
};

/// Static failure at an original command, without retaining authored content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextColourContextFailure {
    #[error("exact display owner is absent from the semantic service")]
    UnknownOwner,
    #[error("inspected main and direct Active commands exceed trusted budget {maximum}")]
    CommandBudget { maximum: usize },
    #[error("object {target:?} has no definition in the supplied catalogue")]
    MissingDefinition { target: TeletextObjectIdentity },
    #[error("object {target:?} is {actual:?}, not the invoked Active type")]
    DefinitionType {
        target: TeletextObjectIdentity,
        actual: TeletextObjectType,
    },
    #[error("relative active position moves backwards from {previous:?} to {requested:?}")]
    ActivePositionOrder {
        previous: (u8, u8),
        requested: (u8, u8),
    },
    #[error("absolute colour row {row} lies outside display rows zero through 24")]
    RowGeometry { row: u16 },
}

/// Exact display and original main/object location; never a synthetic selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("teletext page {owner:?}, location {location:?}, invocation {invocation:?}: {reason}")]
pub struct TeletextColourContextError {
    pub owner: TeletextPageAddress,
    pub location: Option<TeletextEnhancementSource>,
    /// Main command invoking the object; repeated calls retain distinct locations.
    pub invocation: Option<usize>,
    pub reason: TeletextColourContextFailure,
}

/// Immutable original screen and row colours, not a publication capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextColourContext {
    owner: TeletextPageAddress,
    screen: TeletextColourReference,
    rows: [TeletextColourReference; 25],
}

impl TeletextColourContext {
    #[must_use]
    pub const fn owner(&self) -> TeletextPageAddress {
        self.owner
    }

    #[must_use]
    pub const fn screen_colour(&self) -> TeletextColourReference {
        self.screen
    }

    #[must_use]
    pub const fn row_colours(&self) -> &[TeletextColourReference; 25] {
        &self.rows
    }

    #[must_use]
    pub fn row_colour(&self, row: u8) -> Option<TeletextColourReference> {
        self.rows.get(usize::from(row)).copied()
    }
}

/// Resolve original colours before a composer inserts a row-zero selector.
///
/// The effective program preserves absent/local-empty/default selection. Its raw
/// length and each repeated direct Active body's length are charged before colour
/// processing. Adaptive/Passive bodies are not inspected: their colour commands
/// are standard-inapplicable, and checked type edges prohibit nested Active calls.
/// A complete compiler must still reject non-Active colour declarations; this
/// narrow prepass is not that validation. This budget counts
/// inspected semantic commands, not the service's already checked expanded work.
///
/// Original main colours outrank Active colours; Active calls retain invocation
/// order. A real down-to-bottom command updates its entire tail, while a later
/// this-only command overrides only its addressed row. Explicitly invoked screen
/// colour is kept distinct from the presentation's default screen colour: only
/// the former supplies missing row colours. Absent presentation falls back to
/// black. Page/magazine precedence comes from the service's effective metadata.
///
/// Relative main/Active positions retain screen order. Absolute colour rows use
/// exact invocation origins and offsets without clamping. This does not resolve
/// side-panel columns, glyph footprints, attributes or charset state. A composer
/// must keep its inserted selectors separate from this original-colour prepass.
/// # Errors
/// Returns an exact-owner, bounded-work, relative-order, missing-definition or
/// absolute-colour-row error. No partial context or caller mutation is returned.
pub fn resolve_teletext_colour_context(
    service: &TeletextSemanticService,
    owner: TeletextPageAddress,
    max_commands: usize,
) -> Result<TeletextColourContext, TeletextColourContextError> {
    let (_, program) = service.effective_program(owner).map_err(|_| {
        error(
            owner,
            None,
            None,
            TeletextColourContextFailure::UnknownOwner,
        )
    })?;
    resolve_program(service, owner, &program, service.objects(), max_commands)
}

/// Reuse the original presentation defaults with the actual supplied program.
///
/// Only main and direct Active colour commands contribute, under the same bounded
/// policy as the selected-service entry. Definitions come from the supplied checked
/// catalogue, including newly materialized objects. This is a colour-plane guard,
/// not full program, resource-link, geometry or publication admission. Callers must
/// collect and validate the actual complete program separately.
pub(super) fn resolve_teletext_program_colour_context(
    service: &TeletextSemanticService,
    owner: TeletextPageAddress,
    program: &TeletextEnhancementProgram,
    catalog: &TeletextEnhancementObjectCatalog,
    max_commands: usize,
) -> Result<TeletextColourContext, TeletextColourContextError> {
    if service.effective_presentation(owner).is_none() {
        return Err(error(
            owner,
            None,
            None,
            TeletextColourContextFailure::UnknownOwner,
        ));
    }
    resolve_program(service, owner, program, catalog, max_commands)
}

fn resolve_program(
    service: &TeletextSemanticService,
    owner: TeletextPageAddress,
    program: &TeletextEnhancementProgram,
    catalog: &TeletextEnhancementObjectCatalog,
    max_commands: usize,
) -> Result<TeletextColourContext, TeletextColourContextError> {
    preflight(catalog, owner, program, max_commands)?;
    let (default_screen, default_row) = match service.effective_presentation(owner) {
        Some(
            TeletextEffectivePresentation::Extension { defaults, .. }
            | TeletextEffectivePresentation::MotFallback { defaults, .. },
        ) => (defaults.screen_colour, defaults.row_colour),
        Some(TeletextEffectivePresentation::Header { .. }) => (
            TeletextColourReference::BLACK,
            TeletextColourReference::BLACK,
        ),
        None => {
            return Err(error(
                owner,
                None,
                None,
                TeletextColourContextFailure::UnknownOwner,
            ))
        }
    };
    let mut main = Colours::new();
    let mut active = Colours::new();
    let mut position = (0, 0);
    for (command, value) in program.commands().iter().enumerate() {
        let location = TeletextEnhancementSource {
            object: None,
            command,
        };
        advance(&mut position, value)
            .map_err(|reason| error(owner, Some(location), None, reason))?;
        main.apply(value, 0)
            .map_err(|reason| error(owner, Some(location), None, reason))?;
        if let TeletextEnhancementCommand::InvokeObject {
            target,
            object_type: TeletextObjectType::Active,
            offset,
        } = *value
        {
            let row = u16::from(position.0) + offset.map_or(0, |offset| u16::from(offset.row()));
            inspect_active(catalog, owner, target, command, row, &mut active)?;
        }
    }
    let explicit_screen = main.screen.or(active.screen);
    Ok(TeletextColourContext {
        owner,
        screen: explicit_screen.unwrap_or(default_screen),
        rows: std::array::from_fn(|row| {
            main.rows[row]
                .or(active.rows[row])
                .or(explicit_screen)
                .unwrap_or(default_row)
        }),
    })
}

fn error(
    owner: TeletextPageAddress,
    location: Option<TeletextEnhancementSource>,
    invocation: Option<usize>,
    reason: TeletextColourContextFailure,
) -> TeletextColourContextError {
    TeletextColourContextError {
        owner,
        location,
        invocation,
        reason,
    }
}

fn definition(
    catalog: &TeletextEnhancementObjectCatalog,
    owner: TeletextPageAddress,
    target: TeletextObjectIdentity,
    invocation: usize,
) -> Result<&TeletextEnhancementProgram, TeletextColourContextError> {
    let located = |reason| {
        error(
            owner,
            Some(TeletextEnhancementSource {
                object: None,
                command: invocation,
            }),
            Some(invocation),
            reason,
        )
    };
    let definition = catalog
        .definition(target)
        .ok_or_else(|| located(TeletextColourContextFailure::MissingDefinition { target }))?;
    if definition.object_type != TeletextObjectType::Active {
        return Err(located(TeletextColourContextFailure::DefinitionType {
            target,
            actual: definition.object_type,
        }));
    }
    Ok(&definition.program)
}

fn preflight(
    catalog: &TeletextEnhancementObjectCatalog,
    owner: TeletextPageAddress,
    main: &TeletextEnhancementProgram,
    maximum: usize,
) -> Result<(), TeletextColourContextError> {
    let budget = |location| {
        error(
            owner,
            location,
            None,
            TeletextColourContextFailure::CommandBudget { maximum },
        )
    };
    let mut remaining = maximum
        .checked_sub(main.commands().len())
        .ok_or_else(|| budget(None))?;
    for (command, value) in main.commands().iter().enumerate() {
        if let TeletextEnhancementCommand::InvokeObject {
            target,
            object_type: TeletextObjectType::Active,
            ..
        } = *value
        {
            let body = definition(catalog, owner, target, command)?;
            remaining = remaining
                .checked_sub(body.commands().len())
                .ok_or_else(|| {
                    budget(Some(TeletextEnhancementSource {
                        object: None,
                        command,
                    }))
                })?;
        }
    }
    Ok(())
}

fn inspect_active(
    catalog: &TeletextEnhancementObjectCatalog,
    owner: TeletextPageAddress,
    target: TeletextObjectIdentity,
    invocation: usize,
    origin_row: u16,
    colours: &mut Colours,
) -> Result<(), TeletextColourContextError> {
    let body = definition(catalog, owner, target, invocation)?;
    let mut position = (0, 0);
    for (command, value) in body.commands().iter().enumerate() {
        let location = TeletextEnhancementSource {
            object: Some(target),
            command,
        };
        advance(&mut position, value)
            .and_then(|()| colours.apply(value, origin_row))
            .map_err(|reason| error(owner, Some(location), Some(invocation), reason))?;
    }
    Ok(())
}

fn advance(
    position: &mut (u8, u8),
    command: &TeletextEnhancementCommand,
) -> Result<(), TeletextColourContextFailure> {
    let requested = match *command {
        TeletextEnhancementCommand::SetPosition { position } => (position.row(), position.column()),
        TeletextEnhancementCommand::SetRowColour { row, .. } => (row, 0),
        TeletextEnhancementCommand::AtColumn { column, .. } => (position.0, column),
        _ => return Ok(()),
    };
    if requested < *position {
        return Err(TeletextColourContextFailure::ActivePositionOrder {
            previous: *position,
            requested,
        });
    }
    *position = requested;
    Ok(())
}

struct Colours {
    screen: Option<TeletextColourReference>,
    rows: [Option<TeletextColourReference>; 25],
}

impl Colours {
    const fn new() -> Self {
        Self {
            screen: None,
            rows: [None; 25],
        }
    }

    fn apply(
        &mut self,
        command: &TeletextEnhancementCommand,
        origin_row: u16,
    ) -> Result<(), TeletextColourContextFailure> {
        match *command {
            TeletextEnhancementCommand::SetScreenColour { colour } => self.screen = Some(colour),
            TeletextEnhancementCommand::SetRowColour {
                row,
                colour,
                following_rows,
            } => {
                let row = origin_row + u16::from(row);
                if row > 24 {
                    return Err(TeletextColourContextFailure::RowGeometry { row });
                }
                let row = usize::from(row);
                if following_rows {
                    self.rows[row..].fill(Some(colour));
                } else {
                    self.rows[row] = Some(colour);
                }
            }
            _ => {}
        }
        Ok(())
    }
}
