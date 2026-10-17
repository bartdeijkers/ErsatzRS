//! Bounded local X26 bodies from ordered semantic commands (EN 300 706 12.3).
//!
//! This primitive preserves command order and checked wire fields. It does not
//! allocate object pointers, resolve resources/repertoires, compose inherited
//! attributes or establish page readiness. Bodies precede DVB bit reversal;
//! headers, packet addresses, publication and transport remain separate.

use ersatzrs_domain::{
    TeletextColourReference, TeletextColumnOperation, TeletextDrcsTableScope,
    TeletextEnhancementCommand, TeletextEnhancementPosition, TeletextEnhancementProgram,
    TeletextEnhancementSource, TeletextFlashMode, TeletextFlashRate, TeletextG3Compatibility,
    TeletextObjectIdentity, TeletextObjectOffset, TeletextObjectScope, TeletextObjectType,
    TeletextPageAddress, TeletextRawDataPageNumber, TeletextResolvedObjectPageLinks,
};

use super::packets::{hamming_24_18, hamming_8_4};
use super::TeletextResourceObjectAllocations;

const TRIPLETS_PER_PACKET: usize = 13;
const MAX_PACKETS: usize = 16;
const MAX_TRIPLETS: usize = TRIPLETS_PER_PACKET * MAX_PACKETS;
// Address 63, mode 31, local-only termination data seven.
const TERMINATOR: u32 = 0x3fff;

/// Static lowering failure, without retaining authored text or partial bodies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextLocalProgramEncodingFailure {
    /// The borrowed command count exceeds the caller's trusted resource budget.
    #[error("command count exceeds trusted budget {maximum}")]
    CommandBudget { maximum: usize },
    /// X26 has sixteen designations; one triplet is reserved for termination.
    #[error("local X26 physical capacity exceeded")]
    PayloadCapacity,
    /// Row-zero positioning also encodes a colour and needs later lowering.
    #[error("row-zero position {position:?} needs context-dependent lowering")]
    NeedsRowZeroLowering {
        position: TeletextEnhancementPosition,
    },
    /// A later header selector cannot move the physical AP back to column zero.
    /// Semantic normalization remains an explicit final compiler obligation.
    #[error("compatible glyph position {position:?} needs context-dependent row lowering")]
    NeedsCompatibilityRowLowering { position: (u8, u8) },
    /// A semantic object ID is not a physical wire pointer.
    #[error(
        "object {target:?}, type {object_type:?}, offset {offset:?} needs physical allocation"
    )]
    NeedsObjectAllocation {
        target: TeletextObjectIdentity,
        object_type: TeletextObjectType,
        offset: Option<TeletextObjectOffset>,
    },
    /// A physical allocation never grants access to another selected resource.
    #[error("object {target:?} is outside selected resource {selected:?}")]
    ObjectSelection {
        target: TeletextObjectIdentity,
        selected: Option<TeletextRawDataPageNumber>,
    },
    /// Lookup uses the complete semantic identity and exact declared type.
    #[error("object {target:?}, type {object_type:?} has no physical allocation")]
    MissingObjectAllocation {
        target: TeletextObjectIdentity,
        object_type: TeletextObjectType,
    },
    /// Transitive counts still belong to the semantic service collector.
    #[error("local program exceeds 127 direct {object_type:?} object invocations")]
    InvocationCount { object_type: TeletextObjectType },
    /// Annex A.4 prohibits decreasing active screen addresses from this source.
    #[error("active position moves backwards from {previous:?} to {requested:?}")]
    ActivePositionOrder {
        previous: (u8, u8),
        requested: (u8, u8),
    },
}

/// Failure located in the original ordered local program, never a padded triplet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("teletext page {owner:?}, location {location:?}: {reason}")]
pub struct TeletextLocalProgramEncodingError {
    /// Exact display owner, including its raw subcode.
    pub owner: TeletextPageAddress,
    /// Command-budget rejection precedes scanning and has no command location.
    pub location: Option<TeletextEnhancementSource>,
    /// The remaining lowering obligation or violated bound.
    pub reason: TeletextLocalProgramEncodingFailure,
}

/// Immutable local bodies, not a complete page or publication capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedTeletextLocalProgram {
    owner: TeletextPageAddress,
    packet_bodies: Vec<[u8; 40]>,
    payload_triplet_count: usize,
}

impl EncodedTeletextLocalProgram {
    /// Exact display owner, including its raw subcode.
    #[must_use]
    pub const fn owner(&self) -> TeletextPageAddress {
        self.owner
    }

    /// Bodies in designation order zero through at most fifteen.
    #[must_use]
    pub fn packet_bodies(&self) -> &[[u8; 40]] {
        &self.packet_bodies
    }

    /// Command/selector triplets, excluding the mandatory terminator and padding.
    #[must_use]
    pub const fn payload_triplet_count(&self) -> usize {
        self.payload_triplet_count
    }
}

/// Compile the directly lowerable subset of an ordered local enhancement program.
///
/// The trusted command budget is checked against the borrowed slice length
/// before inspecting commands. A complete preflight then checks lowering,
/// active-position order and physical capacity before allocating any bodies.
/// At most 207 payload triplets plus termination occupy sixteen X26 bodies.
/// The empty program still has one termination-only body. Unused triplets
/// repeat the local-only termination marker, with data seven.
///
/// Normal/global DRCS selections start independently unknown. The first use of
/// each scope emits an explicit Level 2.5 selector even for S1 zero; later uses
/// reuse only that scope's exact S1. The local program's initial active position
/// is (0,0), as specified by clause 12.3.2. Initial column commands are preserved;
/// header geometry, charset assignments and inherited attribute validity remain
/// composition obligations. Row-zero colour has an explicit wire operation;
/// row-zero positioning alone needs a later colour/context-aware lowering.
/// # Errors
/// Returns a located budget/capacity/order error, or an explicit requirement for
/// row-zero position lowering or physical object allocation. No partial result
/// or caller mutation occurs.
pub fn compile_teletext_local_program(
    owner: TeletextPageAddress,
    program: &TeletextEnhancementProgram,
    max_commands: usize,
) -> Result<EncodedTeletextLocalProgram, TeletextLocalProgramEncodingError> {
    compile(owner, program, max_commands, None)
}

/// Compile local commands with one immutable public/global object allocation map.
///
/// Each invocation must match the supplied display's resolved resource links and
/// the complete allocated semantic target/type. Origin modifiers immediately
/// precede their invocation; neither changes the caller's active position or
/// DRCS latches. The wire number comes from checked allocation, never a masked
/// host ID. Local definitions, resolved object geometry and complete dependency
/// validation still require service composition. This does not encode the
/// referenced object definition pages or grant publication authority.
/// # Errors
/// Returns the same atomic/located lowering errors as the context-free compiler,
/// or a selection/allocation/direct-invocation-count failure. Local targets still
/// require their owner's shared X26 definition allocation.
pub fn compile_teletext_local_program_with_resource_objects(
    owner: TeletextPageAddress,
    program: &TeletextEnhancementProgram,
    max_commands: usize,
    allocations: &TeletextResourceObjectAllocations,
    links: TeletextResolvedObjectPageLinks,
) -> Result<EncodedTeletextLocalProgram, TeletextLocalProgramEncodingError> {
    compile(
        owner,
        program,
        max_commands,
        Some(ResourceContext {
            allocations,
            links: Some(links),
            local_objects: None,
        }),
    )
}

#[derive(Clone, Copy)]
pub(super) struct ResourceContext<'a> {
    allocations: &'a TeletextResourceObjectAllocations,
    links: Option<TeletextResolvedObjectPageLinks>,
    local_objects: Option<&'a [(TeletextObjectIdentity, TeletextObjectType, u8)]>,
}

impl<'a> ResourceContext<'a> {
    // Shared definitions can serve several displays. Their original catalogue
    // edges retain exact resource/type lookup here; the semantic service checks
    // every consuming display's transitive selected-resource closure separately.
    pub(super) const fn object_definition(
        allocations: &'a TeletextResourceObjectAllocations,
    ) -> Self {
        Self {
            allocations,
            links: None,
            local_objects: None,
        }
    }

    // The local-page compiler owns this complete, exact-owner/type map. Its
    // sizing pass discards provisional words; only packed positions are emitted.
    pub(super) const fn local_page(
        allocations: &'a TeletextResourceObjectAllocations,
        links: TeletextResolvedObjectPageLinks,
        local_objects: &'a [(TeletextObjectIdentity, TeletextObjectType, u8)],
    ) -> Self {
        Self {
            allocations,
            links: Some(links),
            local_objects: Some(local_objects),
        }
    }

    fn object_number(
        self,
        target: TeletextObjectIdentity,
        object_type: TeletextObjectType,
        offset: Option<TeletextObjectOffset>,
    ) -> Result<(u16, u8), TeletextLocalProgramEncodingFailure> {
        let needs_allocation = || TeletextLocalProgramEncodingFailure::NeedsObjectAllocation {
            target,
            object_type,
            offset,
        };
        let (selected, scope) = match target.scope() {
            TeletextObjectScope::Local => {
                let numbers = self.local_objects.ok_or_else(needs_allocation)?;
                let (_, _, number) = numbers
                    .iter()
                    .find(|(identity, kind, _)| *identity == target && *kind == object_type)
                    .ok_or(
                        TeletextLocalProgramEncodingFailure::MissingObjectAllocation {
                            target,
                            object_type,
                        },
                    )?;
                return Ok((u16::from(*number), 0x08));
            }
            TeletextObjectScope::Public => (self.links.map(|links| links.public), 0x10),
            TeletextObjectScope::Global => (self.links.map(|links| links.global), 0x18),
        };
        let resource = target.resource().ok_or_else(needs_allocation)?;
        if let Some(selected) = selected {
            if selected != Some(resource.page()) {
                return Err(TeletextLocalProgramEncodingFailure::ObjectSelection {
                    target,
                    selected,
                });
            }
        }
        let allocation = self.allocations.lookup(target, object_type).ok_or(
            TeletextLocalProgramEncodingFailure::MissingObjectAllocation {
                target,
                object_type,
            },
        )?;
        Ok((allocation.object_number(), scope))
    }
}

fn compile(
    owner: TeletextPageAddress,
    program: &TeletextEnhancementProgram,
    max_commands: usize,
    context: Option<ResourceContext<'_>>,
) -> Result<EncodedTeletextLocalProgram, TeletextLocalProgramEncodingError> {
    if program.commands().len() > max_commands {
        return Err(TeletextLocalProgramEncodingError {
            owner,
            location: None,
            reason: TeletextLocalProgramEncodingFailure::CommandBudget {
                maximum: max_commands,
            },
        });
    }
    let mut state = LoweringState::new();
    let mut payload_triplet_count = 0;
    for (ordinal, command) in program.commands().iter().enumerate() {
        let words = state
            .lower(*command, context)
            .map_err(|reason| located_error(owner, ordinal, reason))?;
        payload_triplet_count += words.len;
        if payload_triplet_count >= MAX_TRIPLETS {
            return Err(located_error(
                owner,
                ordinal,
                TeletextLocalProgramEncodingFailure::PayloadCapacity,
            ));
        }
    }
    let packet_count = (payload_triplet_count + 1).div_ceil(TRIPLETS_PER_PACKET);
    let mut packet_bodies = vec![[0; 40]; packet_count];
    for (designation, body) in (0_u8..16).zip(&mut packet_bodies) {
        body[0] = hamming_8_4(designation);
        for target in body[1..].chunks_exact_mut(3) {
            target.copy_from_slice(&hamming_24_18(TERMINATOR));
        }
    }
    state = LoweringState::new();
    let mut index = 0;
    for (ordinal, command) in program.commands().iter().enumerate() {
        let words = state
            .lower(*command, context)
            .map_err(|reason| located_error(owner, ordinal, reason))?;
        for &value in &words.values[..words.len] {
            let body = &mut packet_bodies[index / TRIPLETS_PER_PACKET];
            let offset = 1 + (index % TRIPLETS_PER_PACKET) * 3;
            body[offset..offset + 3].copy_from_slice(&hamming_24_18(value));
            index += 1;
        }
    }
    Ok(EncodedTeletextLocalProgram {
        owner,
        packet_bodies,
        payload_triplet_count,
    })
}

fn located_error(
    owner: TeletextPageAddress,
    ordinal: usize,
    reason: TeletextLocalProgramEncodingFailure,
) -> TeletextLocalProgramEncodingError {
    TeletextLocalProgramEncodingError {
        owner,
        location: Some(TeletextEnhancementSource {
            object: None,
            command: ordinal,
        }),
        reason,
    }
}

pub(super) struct CommandWords {
    values: [u32; 2],
    len: usize,
}

impl CommandWords {
    pub(super) fn words(&self) -> &[u32] {
        &self.values[..self.len]
    }
    const fn none() -> Self {
        Self {
            values: [0; 2],
            len: 0,
        }
    }
    const fn one(value: u32) -> Self {
        Self {
            values: [value, 0],
            len: 1,
        }
    }

    const fn two(first: u32, second: u32) -> Self {
        Self {
            values: [first, second],
            len: 2,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct LoweringState {
    active_position: (u8, u8),
    // A forward header position can be deferred until a real column operation.
    // Ordering still uses the authored AP; compatibility selectors use the AP
    // actually emitted. Never substitute one for the other at an invocation.
    emitted_position: (u8, u8),
    global_s1: Option<u8>,
    normal_s1: Option<u8>,
    invocations: [u8; 3],
    compatibility: Option<CompatibilityContext>,
}

#[derive(Clone, Copy)]
struct CompatibilityContext {
    row: Option<u8>,
    header_colour: TeletextColourReference,
}

impl LoweringState {
    pub(super) const fn new() -> Self {
        Self {
            active_position: (0, 0),
            emitted_position: (0, 0),
            global_s1: None,
            normal_s1: None,
            invocations: [0; 3],
            compatibility: None,
        }
    }

    /// Opt-in main-program policy for the row address seen by Level 1.5.
    /// Definition compilers keep `new`: objects retain independent wire state.
    pub(super) const fn new_with_compatibility(header_colour: TeletextColourReference) -> Self {
        Self {
            compatibility: Some(CompatibilityContext {
                row: None,
                header_colour,
            }),
            ..Self::new()
        }
    }

    pub(super) fn lower(
        &mut self,
        command: TeletextEnhancementCommand,
        context: Option<ResourceContext<'_>>,
    ) -> Result<CommandWords, TeletextLocalProgramEncodingFailure> {
        // A failed compound command exposes neither a partial word sequence nor
        // advanced AP, DRCS latches or invocation counts to the next command.
        let mut next = *self;
        let words = next.lower_command(command, context)?;
        next.observe_compatibility(&words);
        if matches!(command, TeletextEnhancementCommand::InvokeObject { target, .. }
            if target.scope() == TeletextObjectScope::Local)
        {
            // A local marker's high bit changes the raw invocation address
            // between the sizing placeholder and final forward allocation.
            // Retain no compatibility-row assumption after this command in
            // either pass, so restoration count is independent of that number.
            if let Some(context) = &mut next.compatibility {
                context.row = None;
            }
        }
        *self = next;
        Ok(words)
    }

    fn lower_command(
        &mut self,
        command: TeletextEnhancementCommand,
        context: Option<ResourceContext<'_>>,
    ) -> Result<CommandWords, TeletextLocalProgramEncodingFailure> {
        let value = match command {
            TeletextEnhancementCommand::SetPosition { position } => {
                if position.row() == 0 {
                    // EN 300 706 12.3.2: the enhanced AP starts at (0,0).
                    // Its identity move has no row-colour or compatibility-row
                    // effect; mode7 would incorrectly add those effects.
                    if position.column() == 0 && self.active_position == (0, 0) {
                        return Ok(CommandWords::none());
                    }
                    if self.compatibility.is_some() && self.active_position.0 == 0 {
                        self.move_to((0, position.column()))?;
                        return Ok(CommandWords::none());
                    }
                    return Err(TeletextLocalProgramEncodingFailure::NeedsRowZeroLowering {
                        position,
                    });
                }
                self.move_to((position.row(), position.column()))?;
                self.emitted_position = self.active_position;
                word(row_address(position.row()), 4, position.column())
            }
            TeletextEnhancementCommand::SetScreenColour { colour } => {
                // Annex A.5 recommends address 41 for Level 1.5 compatibility.
                word(41, 0, colour.get())
            }
            TeletextEnhancementCommand::SetRowColour {
                row,
                colour,
                following_rows,
            } => {
                self.move_to((row, 0))?;
                self.emitted_position = self.active_position;
                let data = colour.get() | if following_rows { 0x60 } else { 0 };
                if row == 0 {
                    word(63, 7, data)
                } else {
                    word(row_address(row), 1, data)
                }
            }
            TeletextEnhancementCommand::AtColumn { column, operation } => {
                let previous = self.emitted_position;
                self.move_to((self.active_position.0, column))?;
                let selector = self.compatibility_selector(previous, column, operation)?;
                let words = self.column(column, operation);
                self.emitted_position = self.active_position;
                // A compatible glyph lowers to one word. An initial header
                // DRCS pair is kept atomic: its selector would immediately
                // disturb the compatibility row even after a header prefix.
                if let Some(selector) = selector {
                    if words.len == 1 {
                        return Ok(CommandWords::two(selector, words.values[0]));
                    }
                    // Do not emit a redundant prefix or grow this atomic pair.
                    // A later compatible header glyph needs normalization.
                }
                return Ok(words);
            }
            TeletextEnhancementCommand::InvokeObject {
                target,
                object_type,
                offset,
            } => {
                if self.active_position != self.emitted_position {
                    // Adjusting only the origin modifier would preserve the
                    // object's origin but change Annex A.8's caller-AP interval.
                    // Keep that normalization obligation explicit and located.
                    return Err(TeletextLocalProgramEncodingFailure::NeedsRowZeroLowering {
                        position: TeletextEnhancementPosition::new(
                            self.active_position.0,
                            self.active_position.1,
                        )
                        .expect("active position comes from checked commands"),
                    });
                }
                return self.invoke(target, object_type, offset, context);
            }
        };
        Ok(CommandWords::one(value))
    }

    fn compatibility_selector(
        &self,
        previous: (u8, u8),
        column: u8,
        operation: TeletextColumnOperation,
    ) -> Result<Option<u32>, TeletextLocalProgramEncodingFailure> {
        let Some(context) = self.compatibility else {
            return Ok(None);
        };
        let row = self.active_position.0;
        if context.row == Some(row) {
            return Ok(None);
        }
        // A colour-aware initial selector is also safe before attributes and
        // enhanced-only glyphs. Later header positioning must be normalized by
        // the semantic compiler rather than emitting a backwards row opcode.
        if row == 0 && previous == (0, 0) {
            return Ok(Some(word(63, 7, context.header_colour.get())));
        }
        if !matches!(
            operation,
            TeletextColumnOperation::G2 { .. }
                | TeletextColumnOperation::G0Diacritic { .. }
                | TeletextColumnOperation::G3 {
                    compatibility: TeletextG3Compatibility::Level15,
                    ..
                }
        ) {
            return Ok(None);
        }
        if row == 0 {
            return Err(
                TeletextLocalProgramEncodingFailure::NeedsCompatibilityRowLowering {
                    position: (row, column),
                },
            );
        }
        Ok(Some(word(row_address(row), 4, column)))
    }

    fn observe_compatibility(&mut self, words: &CommandWords) {
        let Some(context) = &mut self.compatibility else {
            return;
        };
        for &value in words.words() {
            let address = value.to_le_bytes()[0] & 63;
            let mode = (value >> 6).to_le_bytes()[0] & 31;
            if address >= 40 {
                context.row = Some(if address == 40 {
                    24
                } else if address == 63 && mode == 7 {
                    0
                } else {
                    address - 40
                });
            }
        }
    }

    fn invoke(
        &mut self,
        target: TeletextObjectIdentity,
        object_type: TeletextObjectType,
        offset: Option<TeletextObjectOffset>,
        context: Option<ResourceContext<'_>>,
    ) -> Result<CommandWords, TeletextLocalProgramEncodingFailure> {
        let Some(context) = context else {
            return Err(TeletextLocalProgramEncodingFailure::NeedsObjectAllocation {
                target,
                object_type,
                offset,
            });
        };
        let (number, scope) = context.object_number(target, object_type, offset)?;
        let (type_index, type_code) = match object_type {
            TeletextObjectType::Active => (0, 1),
            TeletextObjectType::Adaptive => (1, 2),
            TeletextObjectType::Passive => (2, 3),
        };
        if self.invocations[type_index] == 127 {
            return Err(TeletextLocalProgramEncodingFailure::InvocationCount { object_type });
        }
        self.invocations[type_index] += 1;
        let [low, high] = number.to_le_bytes();
        let invocation = word(
            0x20 | scope | (low >> 7) | (high << 1),
            0x10 | type_code,
            low & 0x7f,
        );
        Ok(offset.map_or_else(
            || CommandWords::one(invocation),
            |offset| CommandWords::two(word(40 + offset.row(), 16, offset.column()), invocation),
        ))
    }

    fn move_to(&mut self, position: (u8, u8)) -> Result<(), TeletextLocalProgramEncodingFailure> {
        if position < self.active_position {
            return Err(TeletextLocalProgramEncodingFailure::ActivePositionOrder {
                previous: self.active_position,
                requested: position,
            });
        }
        self.active_position = position;
        Ok(())
    }

    fn column(&mut self, column: u8, operation: TeletextColumnOperation) -> CommandWords {
        let (mode, data) = match operation {
            TeletextColumnOperation::Foreground { colour } => (0, colour.get()),
            TeletextColumnOperation::Background { colour } => (3, colour.get()),
            TeletextColumnOperation::DesignateCharset { designation } => (8, designation.get()),
            TeletextColumnOperation::G0 { code } => (9, code.get()),
            TeletextColumnOperation::G1 { mosaic } => {
                let blocks = mosaic.blocks();
                (1, 0x20 | (blocks & 31) | ((blocks & 32) << 1))
            }
            TeletextColumnOperation::G1Alpha { code } => (1, code.get()),
            TeletextColumnOperation::G2 { code } => (15, code.get()),
            TeletextColumnOperation::G3 {
                code,
                compatibility,
            } => (
                match compatibility {
                    TeletextG3Compatibility::Level15 => 2,
                    TeletextG3Compatibility::Level25 => 11,
                },
                code.get(),
            ),
            TeletextColumnOperation::G0Diacritic { code, diacritic } => {
                (16 | diacritic.get(), code.get())
            }
            TeletextColumnOperation::Drcs { reference } => {
                let (selection, scope) = match reference.table {
                    TeletextDrcsTableScope::Normal => (&mut self.normal_s1, 0x40),
                    TeletextDrcsTableScope::Global => (&mut self.global_s1, 0),
                };
                let s1 = reference.subtable.get();
                let glyph = word(column, 13, scope | reference.slot.get());
                if *selection != Some(s1) {
                    *selection = Some(s1);
                    return CommandWords::two(word(40, 24, scope | 0x10 | s1), glyph);
                }
                return CommandWords::one(glyph);
            }
            TeletextColumnOperation::Attributes {
                double_height,
                double_width,
                boxed,
                conceal,
                invert,
                underline_or_separated_mosaics,
            } => (
                12,
                u8::from(double_height)
                    | (u8::from(boxed) << 1)
                    | (u8::from(conceal) << 2)
                    | (u8::from(invert) << 4)
                    | (u8::from(underline_or_separated_mosaics) << 5)
                    | (u8::from(double_width) << 6),
            ),
            TeletextColumnOperation::Flash { mode, rate } => {
                (7, flash_mode(mode) | (flash_rate(rate) << 2))
            }
        };
        CommandWords::one(word(column, mode, data))
    }
}

const fn row_address(row: u8) -> u8 {
    if row == 24 {
        40
    } else {
        40 + row
    }
}

fn word(address: u8, mode: u8, data: u8) -> u32 {
    u32::from(address) | (u32::from(mode) << 6) | (u32::from(data) << 11)
}

const fn flash_mode(mode: TeletextFlashMode) -> u8 {
    match mode {
        TeletextFlashMode::Steady => 0,
        TeletextFlashMode::Normal => 1,
        TeletextFlashMode::Inverted => 2,
        TeletextFlashMode::AdjacentClut => 3,
    }
}

const fn flash_rate(rate: TeletextFlashRate) -> u8 {
    match rate {
        TeletextFlashRate::Slow1Hz => 0,
        TeletextFlashRate::Fast2HzPhase1 => 1,
        TeletextFlashRate::Fast2HzPhase2 => 2,
        TeletextFlashRate::Fast2HzPhase3 => 3,
        TeletextFlashRate::Fast2HzIncremental => 4,
        TeletextFlashRate::Fast2HzDecremental => 5,
    }
}
