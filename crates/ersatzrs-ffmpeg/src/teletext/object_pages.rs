//! Full bounded POP/GPOP pages, EN 300 706 10.5/12.3 and annexes A/B.
//!
//! The supplied allocation remains tied to full semantic resource identities.
//! Object programs are encoded once, with fresh active position and DRCS state.
//! These pages do not establish atomic service inventory, lifecycle history,
//! carousel timing, display composition, publication or receiver readiness.

use ersatzrs_domain::{
    TeletextEnhancementCommand, TeletextEnhancementObjectCatalog, TeletextObjectIdentity,
    TeletextObjectType, TeletextResourceHeaderError, TeletextResourceHeaderMetadata,
    TeletextResourceIdentity, TeletextResourceKind, TeletextResourceLastPacket,
    TeletextResourceTransmission, TeletextResourceUpdateCounter,
};

use super::enhancement_program::{CommandWords, LoweringState, ResourceContext};
use super::object_allocation::{
    TeletextResourceObjectAllocation, TeletextResourceObjectAllocations,
};
use super::packets::{data_unit, hamming_24_18, hamming_8_4};
use super::{
    encode_teletext_resource_header, TeletextLocalProgramEncodingFailure,
    TeletextResourceFullUpdate, TeletextResourceTransmissionMode,
};

const TRIPLETS_PER_PACKET: usize = 13;

/// Explicit full transmission; the caller owns counter and service-wide mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextObjectPageTransmission {
    pub counter: TeletextResourceUpdateCounter,
    pub update: TeletextResourceFullUpdate,
    pub mode: TeletextResourceTransmissionMode,
    /// Whether the final marker closes the final logical subpage of this page.
    pub last_subpage: bool,
}

/// Static rejection, without retaining object command or textual content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextObjectPageEncodingFailure {
    #[error("resource must have POP or GPOP purpose")]
    Kind,
    #[error("resource command aggregate exceeds caller budget {maximum}")]
    CommandBudget { maximum: usize },
    #[error("semantic definition is missing for supplied allocation")]
    MissingDefinition,
    #[error("semantic definition has type {actual:?}, expected {expected:?}")]
    Type {
        expected: TeletextObjectType,
        actual: TeletextObjectType,
    },
    #[error("semantic definition has no exact physical allocation")]
    MissingAllocation,
    #[error("object body needs {count} triplets; physical capacity is {maximum}")]
    Capacity { count: usize, maximum: usize },
    #[error("{object_type:?} object cannot set screen or row colour")]
    ForbiddenColour { object_type: TeletextObjectType },
    #[error(transparent)]
    Lowering(TeletextLocalProgramEncodingFailure),
    #[error(transparent)]
    Header(TeletextResourceHeaderError),
}

/// Failure retains the exact resource, object and original command ordinal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("teletext resource {resource:?}, object {target:?}, command {command:?}: {reason}")]
pub struct TeletextObjectPageEncodingError {
    pub resource: TeletextResourceIdentity,
    pub target: Option<TeletextObjectIdentity>,
    /// `None` denotes resource/allocation checks rather than an authored command.
    pub command: Option<usize>,
    pub reason: TeletextObjectPageEncodingFailure,
}

/// Exact marker position, not a semantic ID reinterpreted as a wire pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextObjectDefinitionPosition {
    target: TeletextObjectIdentity,
    object_type: TeletextObjectType,
    object_number: u16,
    absolute_index: u16,
}

impl TeletextObjectDefinitionPosition {
    #[must_use]
    pub const fn target(&self) -> TeletextObjectIdentity {
        self.target
    }

    #[must_use]
    pub const fn object_type(&self) -> TeletextObjectType {
        self.object_type
    }

    /// Nine-bit invocation selector; separate from the pointer's destination.
    #[must_use]
    pub const fn object_number(&self) -> u16 {
        self.object_number
    }

    /// Zero-based triplet index from FIRST X3, including any X3/X4 pointer rows.
    #[must_use]
    pub const fn absolute_index(&self) -> u16 {
        self.absolute_index
    }
}

/// Immutable complete resource page, without a closing next-page/FF header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedTeletextObjectPage {
    header: TeletextResourceHeaderMetadata,
    transmission_mode: TeletextResourceTransmissionMode,
    units: Vec<[u8; 46]>,
    positions: Vec<TeletextObjectDefinitionPosition>,
}

impl EncodedTeletextObjectPage {
    #[must_use]
    pub const fn header(&self) -> TeletextResourceHeaderMetadata {
        self.header
    }

    #[must_use]
    pub const fn transmission_mode(&self) -> TeletextResourceTransmissionMode {
        self.transmission_mode
    }

    /// Header, mandatory X28/0, contiguous pointer rows, then ordered content.
    #[must_use]
    pub fn data_units(&self) -> &[[u8; 46]] {
        &self.units
    }

    /// Definitions in physical assignment order, encoded exactly once.
    #[must_use]
    pub fn definition_positions(&self) -> &[TeletextObjectDefinitionPosition] {
        &self.positions
    }
}

/// Encode one explicitly declared full POP/GPOP resource without rebinding IDs.
///
/// Raw commands in this resource are budgeted before lowering. Exact allocation
/// and catalog completeness/type are checked before deriving pointer extent.
/// All markers, expanded command words and a final terminator must fit before
/// any body allocation: capacities are 507, 494 or 481 triplets with two, three
/// or four pointer rows. Definitions reset lowering state independently.
///
/// X1/X2 always exist; X3/X4 pointer extensions are included only when needed.
/// Unused pointer halves are 511. The absolute object-body origin is FIRST X3,
/// even when pointer extensions displace actual content. Final termination uses
/// data one or three, according to `last_subpage`, and pads the final packet.
/// An explicitly declared empty resource contains unused X1/X2 plus a
/// termination-only X3, without inventing a definition. At most 43 DVB units
/// are returned. Source ownership and complete-service validation remain caller
/// obligations; row-zero position lowering and local-object packing remain open.
/// # Errors
/// Rejects wrong purpose, budget/map/type/capacity mismatches, forbidden colour
/// operations or located shared lowering failures. No partial output is returned.
pub fn encode_teletext_object_page(
    resource: TeletextResourceIdentity,
    catalog: &TeletextEnhancementObjectCatalog,
    allocations: &TeletextResourceObjectAllocations,
    tx: TeletextObjectPageTransmission,
    max_commands: usize,
) -> Result<EncodedTeletextObjectPage, TeletextObjectPageEncodingError> {
    if !matches!(
        resource.kind(),
        TeletextResourceKind::Pop | TeletextResourceKind::Gpop
    ) {
        return Err(error(
            resource,
            None,
            None,
            TeletextObjectPageEncodingFailure::Kind,
        ));
    }
    validate_inputs(resource, catalog, allocations, max_commands)?;
    let plan = preflight(resource, catalog, allocations)?;
    let last = usize::from(plan.body_start) + (plan.word_count - 1) / TRIPLETS_PER_PACKET;
    let last = u8::try_from(last).map_err(|_| {
        error(
            resource,
            None,
            None,
            TeletextObjectPageEncodingFailure::Capacity {
                count: plan.word_count,
                maximum: plan.capacity(),
            },
        )
    })?;
    let header = TeletextResourceLastPacket::new(last)
        .and_then(|last_packet| {
            TeletextResourceHeaderMetadata::new(
                resource,
                last_packet,
                tx.counter,
                match tx.update {
                    TeletextResourceFullUpdate::Unchanged => {
                        TeletextResourceTransmission::FullUnchangedFirst
                    }
                    TeletextResourceFullUpdate::Changed => {
                        TeletextResourceTransmission::FullChangedFirst
                    }
                },
            )
        })
        .map_err(|reason| {
            error(
                resource,
                None,
                None,
                TeletextObjectPageEncodingFailure::Header(reason),
            )
        })?;
    let words = encode_words(resource, catalog, allocations, tx.last_subpage, &plan)?;
    let mut units =
        Vec::with_capacity(2 + usize::from(plan.pointer_packets) + words.len().div_ceil(13));
    units.push(encode_teletext_resource_header(header, tx.mode));
    units.push(data_unit(
        resource.page().wire_magazine(),
        28,
        &function_body(resource),
        false,
    ));
    append_pointer_rows(resource, &plan, &mut units);
    append_content_rows(resource, &plan, &words, tx.last_subpage, &mut units);
    Ok(EncodedTeletextObjectPage {
        header,
        transmission_mode: tx.mode,
        units,
        positions: plan.positions,
    })
}

fn validate_inputs(
    resource: TeletextResourceIdentity,
    catalog: &TeletextEnhancementObjectCatalog,
    allocations: &TeletextResourceObjectAllocations,
    max_commands: usize,
) -> Result<(), TeletextObjectPageEncodingError> {
    let mut remaining = max_commands;
    for definition in catalog
        .definitions()
        .iter()
        .filter(|definition| definition.identity.resource() == Some(resource))
    {
        remaining = remaining
            .checked_sub(definition.program.commands().len())
            .ok_or_else(|| {
                error(
                    resource,
                    Some(definition.identity),
                    None,
                    TeletextObjectPageEncodingFailure::CommandBudget {
                        maximum: max_commands,
                    },
                )
            })?;
    }
    for allocation in allocations.resource_entries(resource) {
        definition(resource, catalog, allocation)?;
    }
    for item in catalog
        .definitions()
        .iter()
        .filter(|definition| definition.identity.resource() == Some(resource))
    {
        if allocations
            .lookup(item.identity, item.object_type)
            .is_none()
        {
            return Err(error(
                resource,
                Some(item.identity),
                None,
                TeletextObjectPageEncodingFailure::MissingAllocation,
            ));
        }
    }
    Ok(())
}

fn definition<'a>(
    resource: TeletextResourceIdentity,
    catalog: &'a TeletextEnhancementObjectCatalog,
    allocation: &TeletextResourceObjectAllocation,
) -> Result<&'a ersatzrs_domain::TeletextEnhancementObjectDefinition, TeletextObjectPageEncodingError>
{
    let target = allocation.target();
    let item = catalog.definition(target).ok_or_else(|| {
        error(
            resource,
            Some(target),
            None,
            TeletextObjectPageEncodingFailure::MissingDefinition,
        )
    })?;
    if item.object_type != allocation.object_type() {
        return Err(error(
            resource,
            Some(target),
            None,
            TeletextObjectPageEncodingFailure::Type {
                expected: allocation.object_type(),
                actual: item.object_type,
            },
        ));
    }
    Ok(item)
}

struct PagePlan {
    pointer_packets: u8,
    body_start: u8,
    word_count: usize,
    positions: Vec<TeletextObjectDefinitionPosition>,
}

impl PagePlan {
    fn capacity(&self) -> usize {
        usize::from(42 - self.body_start) * TRIPLETS_PER_PACKET
    }
}

fn preflight(
    resource: TeletextResourceIdentity,
    catalog: &TeletextEnhancementObjectCatalog,
    allocations: &TeletextResourceObjectAllocations,
) -> Result<PagePlan, TeletextObjectPageEncodingError> {
    let maximum_packet = allocations
        .resource_entries(resource)
        .map(|entry| entry.object_number() >> 7)
        .max()
        .unwrap_or(0);
    let pointer_packets = u8::try_from((maximum_packet + 1).max(2)).map_err(|_| {
        error(
            resource,
            None,
            None,
            TeletextObjectPageEncodingFailure::Kind,
        )
    })?;
    let mut plan = PagePlan {
        pointer_packets,
        body_start: pointer_packets + 1,
        word_count: 1,
        positions: Vec::new(),
    };
    let origin = usize::from(plan.body_start - 3) * TRIPLETS_PER_PACKET;
    for allocation in allocations.resource_entries(resource) {
        let item = definition(resource, catalog, allocation)?;
        let absolute = origin + plan.word_count - 1;
        let absolute_index = u16::try_from(absolute).map_err(|_| {
            error(
                resource,
                Some(item.identity),
                None,
                TeletextObjectPageEncodingFailure::Capacity {
                    count: plan.word_count,
                    maximum: plan.capacity(),
                },
            )
        })?;
        plan.positions.push(TeletextObjectDefinitionPosition {
            target: item.identity,
            object_type: item.object_type,
            object_number: allocation.object_number(),
            absolute_index,
        });
        charge(&mut plan, resource, item.identity, None, 1)?;
        let mut state = LoweringState::new();
        for (ordinal, command) in item.program.commands().iter().enumerate() {
            let words = lower(
                resource,
                item.identity,
                item.object_type,
                ordinal,
                *command,
                &mut state,
                allocations,
            )?;
            charge(
                &mut plan,
                resource,
                item.identity,
                Some(ordinal),
                words.words().len(),
            )?;
        }
    }
    Ok(plan)
}

fn charge(
    plan: &mut PagePlan,
    resource: TeletextResourceIdentity,
    target: TeletextObjectIdentity,
    command: Option<usize>,
    count: usize,
) -> Result<(), TeletextObjectPageEncodingError> {
    plan.word_count += count;
    if plan.word_count > plan.capacity() {
        return Err(error(
            resource,
            Some(target),
            command,
            TeletextObjectPageEncodingFailure::Capacity {
                count: plan.word_count,
                maximum: plan.capacity(),
            },
        ));
    }
    Ok(())
}

fn lower(
    resource: TeletextResourceIdentity,
    target: TeletextObjectIdentity,
    object_type: TeletextObjectType,
    ordinal: usize,
    command: TeletextEnhancementCommand,
    state: &mut LoweringState,
    allocations: &TeletextResourceObjectAllocations,
) -> Result<CommandWords, TeletextObjectPageEncodingError> {
    if object_type != TeletextObjectType::Active
        && matches!(
            command,
            TeletextEnhancementCommand::SetScreenColour { .. }
                | TeletextEnhancementCommand::SetRowColour { .. }
        )
    {
        return Err(error(
            resource,
            Some(target),
            Some(ordinal),
            TeletextObjectPageEncodingFailure::ForbiddenColour { object_type },
        ));
    }
    state
        .lower(
            command,
            Some(ResourceContext::object_definition(allocations)),
        )
        .map_err(|reason| {
            error(
                resource,
                Some(target),
                Some(ordinal),
                TeletextObjectPageEncodingFailure::Lowering(reason),
            )
        })
}

fn encode_words(
    resource: TeletextResourceIdentity,
    catalog: &TeletextEnhancementObjectCatalog,
    allocations: &TeletextResourceObjectAllocations,
    last_subpage: bool,
    plan: &PagePlan,
) -> Result<Vec<u32>, TeletextObjectPageEncodingError> {
    let mut words = Vec::with_capacity(plan.word_count);
    for allocation in allocations.resource_entries(resource) {
        let item = definition(resource, catalog, allocation)?;
        let number = allocation.object_number();
        words.push(word(
            0x28 | ((number >> 7) & 3),
            0x14 | type_code(item.object_type),
            number & 0x7f,
        ));
        let mut state = LoweringState::new();
        for (ordinal, command) in item.program.commands().iter().enumerate() {
            words.extend_from_slice(
                lower(
                    resource,
                    item.identity,
                    item.object_type,
                    ordinal,
                    *command,
                    &mut state,
                    allocations,
                )?
                .words(),
            );
        }
    }
    words.push(termination(last_subpage));
    Ok(words)
}

fn append_pointer_rows(
    resource: TeletextResourceIdentity,
    plan: &PagePlan,
    units: &mut Vec<[u8; 46]>,
) {
    let mut rows = [[0x3ffff_u32; 13]; 4];
    for row in &mut rows {
        row[0] = 0;
    }
    for position in &plan.positions {
        let number = position.object_number;
        let packet = usize::from(number >> 7);
        let triplet = usize::from(((number >> 5) & 3) * 3 + type_code(position.object_type));
        let shift = ((number >> 4) & 1) * 9;
        rows[packet][triplet] = (rows[packet][triplet] & !(0x1ff << shift))
            | (u32::from(position.absolute_index) << shift);
    }
    for (packet, words) in (1_u8..=plan.pointer_packets).zip(rows) {
        units.push(data_unit(
            resource.page().wire_magazine(),
            packet,
            &body(1, &words),
            false,
        ));
    }
}

fn append_content_rows(
    resource: TeletextResourceIdentity,
    plan: &PagePlan,
    words: &[u32],
    last_subpage: bool,
    units: &mut Vec<[u8; 46]>,
) {
    for (row, content) in (plan.body_start..=41).zip(words.chunks(TRIPLETS_PER_PACKET)) {
        let mut padded = [termination(last_subpage); 13];
        padded[..content.len()].copy_from_slice(content);
        let (packet, designation) = if row < 26 { (row, 0) } else { (26, row - 26) };
        units.push(data_unit(
            resource.page().wire_magazine(),
            packet,
            &body(designation, &padded),
            false,
        ));
    }
}

fn function_body(resource: TeletextResourceIdentity) -> [u8; 40] {
    let mut words = [0; 13];
    words[0] = 0xff80 | u32::from(resource.kind().function_code()) | (2 << 4);
    body(0, &words)
}

fn body(designation: u8, words: &[u32; 13]) -> [u8; 40] {
    let mut result = [0; 40];
    result[0] = hamming_8_4(designation);
    for (target, word) in result[1..].chunks_exact_mut(3).zip(words) {
        target.copy_from_slice(&hamming_24_18(*word));
    }
    result
}

fn type_code(object_type: TeletextObjectType) -> u16 {
    match object_type {
        TeletextObjectType::Active => 1,
        TeletextObjectType::Adaptive => 2,
        TeletextObjectType::Passive => 3,
    }
}

fn word(address: u16, mode: u16, data: u16) -> u32 {
    u32::from(address) | (u32::from(mode) << 6) | (u32::from(data) << 11)
}

fn termination(last_subpage: bool) -> u32 {
    word(63, 31, if last_subpage { 3 } else { 1 })
}

fn error(
    resource: TeletextResourceIdentity,
    target: Option<TeletextObjectIdentity>,
    command: Option<usize>,
    reason: TeletextObjectPageEncodingFailure,
) -> TeletextObjectPageEncodingError {
    TeletextObjectPageEncodingError {
        resource,
        target,
        command,
        reason,
    }
}
