//! Bounded local X26 bodies from ordered semantic commands (EN 300 706 12.3).
//!
//! This primitive preserves command order and checked wire fields. It does not
//! allocate object pointers, resolve resources/repertoires, compose inherited
//! attributes or establish page readiness. Bodies precede DVB bit reversal;
//! headers, packet addresses, publication and transport remain separate.

use ersatzrs_domain::{
    TeletextColumnOperation, TeletextDrcsTableScope, TeletextEnhancementCommand,
    TeletextEnhancementPosition, TeletextEnhancementProgram, TeletextEnhancementSource,
    TeletextFlashMode, TeletextFlashRate, TeletextG3Compatibility, TeletextObjectIdentity,
    TeletextObjectOffset, TeletextObjectScope, TeletextObjectType, TeletextPageAddress,
    TeletextRawDataPageNumber, TeletextResolvedObjectPageLinks,
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
        Some(ResourceContext { allocations, links }),
    )
}

#[derive(Clone, Copy)]
struct ResourceContext<'a> {
    allocations: &'a TeletextResourceObjectAllocations,
    links: TeletextResolvedObjectPageLinks,
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
        let words = state.lower(owner, ordinal, *command, context)?;
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
        let words = state.lower(owner, ordinal, *command, context)?;
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

struct CommandWords {
    values: [u32; 2],
    len: usize,
}

impl CommandWords {
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

struct LoweringState {
    active_position: (u8, u8),
    global_s1: Option<u8>,
    normal_s1: Option<u8>,
    invocations: [u8; 3],
}

impl LoweringState {
    const fn new() -> Self {
        Self {
            active_position: (0, 0),
            global_s1: None,
            normal_s1: None,
            invocations: [0; 3],
        }
    }

    fn lower(
        &mut self,
        owner: TeletextPageAddress,
        ordinal: usize,
        command: TeletextEnhancementCommand,
        context: Option<ResourceContext<'_>>,
    ) -> Result<CommandWords, TeletextLocalProgramEncodingError> {
        let value = match command {
            TeletextEnhancementCommand::SetPosition { position } => {
                if position.row() == 0 {
                    return Err(located_error(
                        owner,
                        ordinal,
                        TeletextLocalProgramEncodingFailure::NeedsRowZeroLowering { position },
                    ));
                }
                self.move_to(owner, ordinal, (position.row(), position.column()))?;
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
                self.move_to(owner, ordinal, (row, 0))?;
                let data = colour.get() | if following_rows { 0x60 } else { 0 };
                if row == 0 {
                    word(63, 7, data)
                } else {
                    word(row_address(row), 1, data)
                }
            }
            TeletextEnhancementCommand::AtColumn { column, operation } => {
                self.move_to(owner, ordinal, (self.active_position.0, column))?;
                return Ok(self.column(column, operation));
            }
            TeletextEnhancementCommand::InvokeObject {
                target,
                object_type,
                offset,
            } => {
                return self.invoke(owner, ordinal, target, object_type, offset, context);
            }
        };
        Ok(CommandWords::one(value))
    }

    fn invoke(
        &mut self,
        owner: TeletextPageAddress,
        ordinal: usize,
        target: TeletextObjectIdentity,
        object_type: TeletextObjectType,
        offset: Option<TeletextObjectOffset>,
        context: Option<ResourceContext<'_>>,
    ) -> Result<CommandWords, TeletextLocalProgramEncodingError> {
        let fail = |reason| located_error(owner, ordinal, reason);
        let Some((context, resource)) = context.zip(target.resource()) else {
            return Err(fail(
                TeletextLocalProgramEncodingFailure::NeedsObjectAllocation {
                    target,
                    object_type,
                    offset,
                },
            ));
        };
        let (selected, scope) = match target.scope() {
            TeletextObjectScope::Public => (context.links.public, 0x10),
            TeletextObjectScope::Global => (context.links.global, 0x18),
            TeletextObjectScope::Local => {
                return Err(fail(
                    TeletextLocalProgramEncodingFailure::NeedsObjectAllocation {
                        target,
                        object_type,
                        offset,
                    },
                ));
            }
        };
        if selected != Some(resource.page()) {
            return Err(fail(TeletextLocalProgramEncodingFailure::ObjectSelection {
                target,
                selected,
            }));
        }
        let allocation = context
            .allocations
            .lookup(target, object_type)
            .ok_or_else(|| {
                fail(
                    TeletextLocalProgramEncodingFailure::MissingObjectAllocation {
                        target,
                        object_type,
                    },
                )
            })?;
        let (type_index, type_code) = match object_type {
            TeletextObjectType::Active => (0, 1),
            TeletextObjectType::Adaptive => (1, 2),
            TeletextObjectType::Passive => (2, 3),
        };
        if self.invocations[type_index] == 127 {
            return Err(fail(TeletextLocalProgramEncodingFailure::InvocationCount {
                object_type,
            }));
        }
        self.invocations[type_index] += 1;
        let [low, high] = allocation.object_number().to_le_bytes();
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

    fn move_to(
        &mut self,
        owner: TeletextPageAddress,
        ordinal: usize,
        position: (u8, u8),
    ) -> Result<(), TeletextLocalProgramEncodingError> {
        if position < self.active_position {
            return Err(located_error(
                owner,
                ordinal,
                TeletextLocalProgramEncodingFailure::ActivePositionOrder {
                    previous: self.active_position,
                    requested: position,
                },
            ));
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
