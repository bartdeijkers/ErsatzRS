//! Bounded local definitions sharing a display's sixteen X26 designations.
//!
//! Semantic IDs remain intact while selectors follow exact marker positions.
//! This primitive does not merge base glyphs, infer packet absence, resolve
//! geometry/repertoires or establish publication or receiver readiness.

use ersatzrs_domain::{
    TeletextColourReference, TeletextEnhancementCommand, TeletextEnhancementObjectCatalog,
    TeletextEnhancementObjectDefinition, TeletextEnhancementProgram, TeletextEnhancementSource,
    TeletextObjectCatalogBudget, TeletextObjectIdentity, TeletextObjectType, TeletextPageAddress,
    TeletextResolvedObjectPageLinks,
};

use super::enhancement_program::{CommandWords, LoweringState, ResourceContext};
use super::packets::{hamming_24_18, hamming_8_4};
use super::{TeletextLocalProgramEncodingFailure, TeletextResourceObjectAllocations};

const PER_PACKET: usize = 13;
const CAPACITY: usize = 16 * PER_PACKET;

/// Static rejection retaining identities and locations, never authored content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextLocalObjectProgramEncodingFailure {
    #[error("catalog has {count} objects, exceeding trusted budget {maximum}")]
    ObjectBudget { count: usize, maximum: usize },
    #[error("main and owned local commands exceed trusted budget {maximum}")]
    CommandBudget { maximum: usize },
    #[error("local sequence needs {count} triplets; capacity is {maximum}")]
    Capacity { count: usize, maximum: usize },
    #[error("{object_type:?} object cannot set screen or row colour")]
    ForbiddenColour { object_type: TeletextObjectType },
    #[error(transparent)]
    Lowering(TeletextLocalProgramEncodingFailure),
}

/// Original authored location, independent of physical definition placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("teletext page {owner:?}, location {location:?}: {reason}")]
pub struct TeletextLocalObjectProgramEncodingError {
    pub owner: TeletextPageAddress,
    /// `None` denotes an aggregate input check, not an authored command.
    pub location: Option<TeletextEnhancementSource>,
    pub reason: TeletextLocalObjectProgramEncodingFailure,
}

/// Full semantic target and its marker in the shared triplet array.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextLocalObjectDefinitionPosition {
    target: TeletextObjectIdentity,
    object_type: TeletextObjectType,
    object_number: u8,
    absolute_index: u8,
}

impl TeletextLocalObjectDefinitionPosition {
    #[must_use]
    pub const fn target(&self) -> TeletextObjectIdentity {
        self.target
    }

    #[must_use]
    pub const fn object_type(&self) -> TeletextObjectType {
        self.object_type
    }

    /// Designation in the high nibble, triplet ordinal in the low nibble.
    #[must_use]
    pub const fn object_number(&self) -> u8 {
        self.object_number
    }

    #[must_use]
    pub const fn absolute_index(&self) -> u8 {
        self.absolute_index
    }
}

/// Final physical words belonging to one original packing command. Compatibility
/// selectors belong to its range; the semantic word is the last emitted word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextCompiledCommandPosition {
    source: TeletextEnhancementSource,
    first_word: usize,
    semantic_word: usize,
    original_local_command: Option<usize>,
}
impl TeletextCompiledCommandPosition {
    #[must_use]
    pub const fn source(&self) -> TeletextEnhancementSource {
        self.source
    }
    #[must_use]
    pub const fn first_word(&self) -> usize {
        self.first_word
    }
    #[must_use]
    pub const fn semantic_word(&self) -> usize {
        self.semantic_word
    }
    /// Original page-local command ordinal, bound only by the normalized
    /// augmented factory's retained-main provenance. Standalone packing and
    /// generated/default/object commands have no original local ordinal.
    /// This is correspondence metadata, not edit or publication permission.
    #[must_use]
    pub const fn original_local_command(&self) -> Option<usize> {
        self.original_local_command
    }
}

/// Immutable main program and all supplied local definitions for this owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedTeletextLocalObjectProgram {
    owner: TeletextPageAddress,
    packet_bodies: Vec<[u8; 40]>,
    main_triplet_count: usize,
    positions: Vec<TeletextLocalObjectDefinitionPosition>,
    commands: Vec<TeletextCompiledCommandPosition>,
}

impl EncodedTeletextLocalObjectProgram {
    pub(super) fn bind_original_local_commands(
        &mut self,
        mut original: impl FnMut(TeletextEnhancementSource) -> Option<usize>,
    ) {
        for position in &mut self.commands {
            position.original_local_command = if position.source.object.is_none() {
                original(position.source)
            } else {
                None
            };
        }
    }

    /// Actual final emission positions, never the placeholder sizing pass.
    #[must_use]
    pub fn command_positions(&self) -> &[TeletextCompiledCommandPosition] {
        &self.commands
    }

    #[must_use]
    pub const fn owner(&self) -> TeletextPageAddress {
        self.owner
    }

    #[must_use]
    pub fn packet_bodies(&self) -> &[[u8; 40]] {
        &self.packet_bodies
    }

    /// Main command/selector words, excluding termination, definitions and padding.
    #[must_use]
    pub const fn main_triplet_count(&self) -> usize {
        self.main_triplet_count
    }

    #[must_use]
    pub fn definition_positions(&self) -> &[TeletextLocalObjectDefinitionPosition] {
        &self.positions
    }
}

/// Compile a supplied main program and all exact-owner local definitions.
///
/// Raw catalog count is fenced before scanning; main plus every owned local
/// command is charged before lowering. Definitions retain catalog order, even
/// when unused. Complete sizing includes markers, selector/origin expansion and
/// termination before body allocation. Forward references use the completed
/// physical map; host IDs are never truncated. Each definition resets active
/// position, direct invocation counts and both DRCS latches independently.
///
/// With definitions, the main terminates with data six and the final definition
/// with data five; adjacent markers end preceding definitions. Without local
/// definitions the original local-only data-seven termination is preserved.
/// Final termination repeats through padding. Actual program presence is a
/// caller input: an empty supplied program does not represent absent X26.
/// Catalog closure, selected-resource closure, geometry, repertoire and expanded
/// semantic work budgets remain service composition obligations.
/// # Errors
/// Returns bounded/located lowering failures without partial output or mutation.
pub fn compile_teletext_local_program_with_objects(
    owner: TeletextPageAddress,
    program: &TeletextEnhancementProgram,
    catalog: &TeletextEnhancementObjectCatalog,
    allocations: &TeletextResourceObjectAllocations,
    links: TeletextResolvedObjectPageLinks,
    budget: TeletextObjectCatalogBudget,
) -> Result<EncodedTeletextLocalObjectProgram, TeletextLocalObjectProgramEncodingError> {
    compile_with_context(owner, program, catalog, allocations, links, budget, None)
}

pub(super) fn compile_with_context(
    owner: TeletextPageAddress,
    program: &TeletextEnhancementProgram,
    catalog: &TeletextEnhancementObjectCatalog,
    allocations: &TeletextResourceObjectAllocations,
    links: TeletextResolvedObjectPageLinks,
    budget: TeletextObjectCatalogBudget,
    header_colour: Option<TeletextColourReference>,
) -> Result<EncodedTeletextLocalObjectProgram, TeletextLocalObjectProgramEncodingError> {
    let input = Input {
        owner,
        program,
        catalog,
        allocations,
        links,
        header_colour,
    };
    let local_count = input.validate_budget(budget)?;
    // Bound even the temporary map: each definition consumes a marker, and
    // separate main/final termination consumes two words when locals exist.
    let minimum = local_count.saturating_add(if local_count == 0 { 1 } else { 2 });
    input.check_capacity(minimum, None)?;
    let placeholders: Vec<_> = input
        .locals()
        .map(|item| (item.identity, item.object_type, 0))
        .collect();
    let plan = input.preflight(&placeholders)?;
    let numbers: Vec<_> = plan
        .positions
        .iter()
        .map(|item| (item.target, item.object_type, item.object_number))
        .collect();
    let (packet_bodies, commands) = input.encode(&numbers, &plan)?;
    Ok(EncodedTeletextLocalObjectProgram {
        owner,
        packet_bodies,
        commands,
        main_triplet_count: plan.main_count,
        positions: plan.positions,
    })
}

struct Input<'a> {
    owner: TeletextPageAddress,
    program: &'a TeletextEnhancementProgram,
    catalog: &'a TeletextEnhancementObjectCatalog,
    allocations: &'a TeletextResourceObjectAllocations,
    links: TeletextResolvedObjectPageLinks,
    header_colour: Option<TeletextColourReference>,
}

struct Plan {
    main_count: usize,
    word_count: usize,
    positions: Vec<TeletextLocalObjectDefinitionPosition>,
}

impl Input<'_> {
    fn main_state(&self) -> LoweringState {
        self.header_colour
            .map_or_else(LoweringState::new, LoweringState::new_with_compatibility)
    }

    fn locals(&self) -> impl Iterator<Item = &TeletextEnhancementObjectDefinition> {
        self.catalog
            .definitions()
            .iter()
            .filter(|item| item.identity.owner() == Some(self.owner))
    }

    fn error(
        &self,
        location: Option<TeletextEnhancementSource>,
        reason: TeletextLocalObjectProgramEncodingFailure,
    ) -> TeletextLocalObjectProgramEncodingError {
        TeletextLocalObjectProgramEncodingError {
            owner: self.owner,
            location,
            reason,
        }
    }

    fn validate_budget(
        &self,
        budget: TeletextObjectCatalogBudget,
    ) -> Result<usize, TeletextLocalObjectProgramEncodingError> {
        let count = self.catalog.definitions().len();
        if count > budget.max_objects {
            return Err(self.error(
                None,
                TeletextLocalObjectProgramEncodingFailure::ObjectBudget {
                    count,
                    maximum: budget.max_objects,
                },
            ));
        }
        let command_error = || {
            self.error(
                None,
                TeletextLocalObjectProgramEncodingFailure::CommandBudget {
                    maximum: budget.max_commands,
                },
            )
        };
        let mut remaining = budget
            .max_commands
            .checked_sub(self.program.commands().len())
            .ok_or_else(command_error)?;
        let mut count = 0;
        for item in self.locals() {
            remaining = remaining
                .checked_sub(item.program.commands().len())
                .ok_or_else(command_error)?;
            count += 1;
        }
        Ok(count)
    }

    fn check_capacity(
        &self,
        count: usize,
        location: Option<TeletextEnhancementSource>,
    ) -> Result<(), TeletextLocalObjectProgramEncodingError> {
        if count > CAPACITY {
            return Err(self.error(
                location,
                TeletextLocalObjectProgramEncodingFailure::Capacity {
                    count,
                    maximum: CAPACITY,
                },
            ));
        }
        Ok(())
    }

    fn lower(
        &self,
        state: &mut LoweringState,
        command: TeletextEnhancementCommand,
        location: TeletextEnhancementSource,
        object_type: Option<TeletextObjectType>,
        numbers: &[(TeletextObjectIdentity, TeletextObjectType, u8)],
    ) -> Result<CommandWords, TeletextLocalObjectProgramEncodingError> {
        if let Some(object_type) = object_type {
            if object_type != TeletextObjectType::Active
                && matches!(
                    command,
                    TeletextEnhancementCommand::SetScreenColour { .. }
                        | TeletextEnhancementCommand::SetRowColour { .. }
                )
            {
                return Err(self.error(
                    Some(location),
                    TeletextLocalObjectProgramEncodingFailure::ForbiddenColour { object_type },
                ));
            }
        }
        state
            .lower(
                command,
                Some(ResourceContext::local_page(
                    self.allocations,
                    self.links,
                    numbers,
                )),
            )
            .map_err(|reason| {
                self.error(
                    Some(location),
                    TeletextLocalObjectProgramEncodingFailure::Lowering(reason),
                )
            })
    }

    fn preflight(
        &self,
        numbers: &[(TeletextObjectIdentity, TeletextObjectType, u8)],
    ) -> Result<Plan, TeletextLocalObjectProgramEncodingError> {
        let mut count = if numbers.is_empty() { 1 } else { 2 };
        let mut state = self.main_state();
        for (command, &value) in self.program.commands().iter().enumerate() {
            let location = TeletextEnhancementSource {
                object: None,
                command,
            };
            count += self
                .lower(&mut state, value, location, None, numbers)?
                .words()
                .len();
            self.check_capacity(count, Some(location))?;
        }
        let main_count = count - if numbers.is_empty() { 1 } else { 2 };
        let mut positions = Vec::with_capacity(numbers.len());
        for item in self.locals() {
            // The count includes final termination, which follows all definitions.
            let absolute_index = u8::try_from(count - 1).map_err(|_| {
                self.error(
                    None,
                    TeletextLocalObjectProgramEncodingFailure::Capacity {
                        count,
                        maximum: CAPACITY,
                    },
                )
            })?;
            positions.push(TeletextLocalObjectDefinitionPosition {
                target: item.identity,
                object_type: item.object_type,
                object_number: ((absolute_index / 13) << 4) | (absolute_index % 13),
                absolute_index,
            });
            count += 1;
            self.check_capacity(count, None)?;
            state = LoweringState::new();
            for (command, &value) in item.program.commands().iter().enumerate() {
                let location = TeletextEnhancementSource {
                    object: Some(item.identity),
                    command,
                };
                count += self
                    .lower(&mut state, value, location, Some(item.object_type), numbers)?
                    .words()
                    .len();
                self.check_capacity(count, Some(location))?;
            }
        }
        Ok(Plan {
            main_count,
            word_count: count,
            positions,
        })
    }

    fn encode(
        &self,
        numbers: &[(TeletextObjectIdentity, TeletextObjectType, u8)],
        plan: &Plan,
    ) -> Result<
        (Vec<[u8; 40]>, Vec<TeletextCompiledCommandPosition>),
        TeletextLocalObjectProgramEncodingError,
    > {
        let final_marker = terminator(if numbers.is_empty() { 7 } else { 5 });
        let mut bodies = vec![[0; 40]; plan.word_count.div_ceil(PER_PACKET)];
        for (designation, body) in (0_u8..16).zip(&mut bodies) {
            body[0] = hamming_8_4(designation);
            for triplet in body[1..].chunks_exact_mut(3) {
                triplet.copy_from_slice(&hamming_24_18(final_marker));
            }
        }
        let mut emission = Emission {
            index: 0,
            commands: Vec::new(),
        };
        self.write_program(self.program, None, numbers, &mut bodies, &mut emission)?;
        write_word(
            &mut bodies,
            &mut emission.index,
            terminator(if numbers.is_empty() { 7 } else { 6 }),
        );
        for (item, position) in self.locals().zip(&plan.positions) {
            let number = u32::from(position.object_number);
            let object_type = match item.object_type {
                TeletextObjectType::Active => 1,
                TeletextObjectType::Adaptive => 2,
                TeletextObjectType::Passive => 3,
            };
            write_word(
                &mut bodies,
                &mut emission.index,
                (0x28 | (number >> 7)) | ((0x14 | object_type) << 6) | ((number & 127) << 11),
            );
            self.write_program(
                &item.program,
                Some(item),
                numbers,
                &mut bodies,
                &mut emission,
            )?;
        }
        if !numbers.is_empty() {
            write_word(&mut bodies, &mut emission.index, final_marker);
        }
        Ok((bodies, emission.commands))
    }

    fn write_program(
        &self,
        program: &TeletextEnhancementProgram,
        object: Option<&TeletextEnhancementObjectDefinition>,
        numbers: &[(TeletextObjectIdentity, TeletextObjectType, u8)],
        bodies: &mut [[u8; 40]],
        emission: &mut Emission,
    ) -> Result<(), TeletextLocalObjectProgramEncodingError> {
        let mut state = if object.is_none() {
            self.main_state()
        } else {
            LoweringState::new()
        };
        for (command, &value) in program.commands().iter().enumerate() {
            let location = TeletextEnhancementSource {
                object: object.map(|item| item.identity),
                command,
            };
            let words = self.lower(
                &mut state,
                value,
                location,
                object.map(|item| item.object_type),
                numbers,
            )?;
            // A deferred position emits no semantic word. Preserve the authored
            // ordinals of following commands without inventing timing evidence.
            if words.words().is_empty() {
                continue;
            }
            let first_word = emission.index;
            for &word in words.words() {
                write_word(bodies, &mut emission.index, word);
            }
            emission.commands.push(TeletextCompiledCommandPosition {
                source: location,
                first_word,
                semantic_word: emission.index - 1,
                original_local_command: None,
            });
        }
        Ok(())
    }
}

struct Emission {
    index: usize,
    commands: Vec<TeletextCompiledCommandPosition>,
}

const fn terminator(data: u32) -> u32 {
    63 | (31 << 6) | (data << 11)
}

fn write_word(bodies: &mut [[u8; 40]], index: &mut usize, word: u32) {
    let offset = 1 + (*index % PER_PACKET) * 3;
    bodies[*index / PER_PACKET][offset..offset + 3].copy_from_slice(&hamming_24_18(word));
    *index += 1;
}
