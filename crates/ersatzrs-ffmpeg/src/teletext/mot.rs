//! Level 2.5 MOT packet bodies, without selection, header lifecycle or readiness.

use ersatzrs_domain::{
    TeletextMotAssociationSelection, TeletextMotDefaultObject, TeletextMotLevel25Declaration,
    TeletextMotPublicLinkIndex, TeletextObjectIdentity, TeletextObjectType,
    TeletextResourceIdentity,
};

use super::packets::hamming_8_4;

/// Caller-supplied physical allocation for one active semantic default object.
/// The selector is `S1 | half<<4 | group<<5 | pointer_packet<<7` (table 21).
/// No host object ID is truncated to create this allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextMotDefaultAllocation {
    pub target: TeletextObjectIdentity,
    pub object_type: TeletextObjectType,
    pub selector: u8,
}

/// GPOP then POP1–7, each with two original default-object slots.
pub type TeletextMotDefaultAllocations = [[Option<TeletextMotDefaultAllocation>; 2]; 8];

/// A complete pre-bit-reversal packet body, with its explicit MOT row number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextMotPacketBody {
    pub row: u8,
    pub body: [u8; 40],
}

/// Located failure in supplied allocation, before returning any packet bodies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("teletext MOT {owner:?}, object link {link}, default slot {slot}: {reason:?}")]
pub struct TeletextMotEncodingError {
    pub owner: TeletextResourceIdentity,
    /// Zero is GPOP; one through seven are the original POP slots.
    pub link: u8,
    pub slot: u8,
    pub reason: TeletextMotAllocationFailure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextMotAllocationFailure {
    Missing,
    Unexpected,
    TargetOrType,
    Subpage,
}

/// Encode rows 1–14, 19, 20 and 21 of an explicitly owned Level 2.5 MOT.
///
/// Missing sparse associations and reserved padding are canonical zero nibbles;
/// raw FF associations, ignored pointer numbers, GPOP fallback bits and counts
/// are retained. This is an authoring convention, not a byte-lossless raw import
/// promise. Level 3.5 banks and optional X28 function/coding are not emitted.
/// Header update bits, partial-page lifecycle, resource allocation correctness,
/// MOT/X27 resolution, transport and client proof remain separate obligations.
/// # Errors
/// Rejects missing, unexpected, mismatched or wrong-S1 default allocations.
pub fn encode_teletext_mot_bodies(
    declaration: &TeletextMotLevel25Declaration,
    allocations: &TeletextMotDefaultAllocations,
) -> Result<[TeletextMotPacketBody; 17], TeletextMotEncodingError> {
    const ROWS: [u8; 17] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 19, 20, 21];
    let links = declaration.links();
    let mut selectors = [[0_u8; 2]; 8];
    let indices = std::iter::once(None).chain(TeletextMotPublicLinkIndex::ALL.map(Some));
    for (link, (index, allocation)) in (0_u8..8).zip(indices.zip(allocations)) {
        for (slot, (default, allocation)) in (0_u8..2).zip(
            links
                .object_link(index)
                .defaults()
                .into_iter()
                .zip(allocation),
        ) {
            selectors[usize::from(link)][usize::from(slot)] = match (default, allocation) {
                (TeletextMotDefaultObject::Object { .. }, None) => {
                    Err(TeletextMotAllocationFailure::Missing)
                }
                (
                    TeletextMotDefaultObject::Object {
                        target,
                        object_type,
                    },
                    Some(actual),
                ) => {
                    if actual.target != target || actual.object_type != object_type {
                        Err(TeletextMotAllocationFailure::TargetOrType)
                    } else if target.resource().is_none_or(|resource| {
                        resource.logical_subpage().get() != actual.selector & 15
                    }) {
                        Err(TeletextMotAllocationFailure::Subpage)
                    } else {
                        Ok(actual.selector)
                    }
                }
                (_, Some(_)) => Err(TeletextMotAllocationFailure::Unexpected),
                (
                    TeletextMotDefaultObject::NoObject { raw_number }
                    | TeletextMotDefaultObject::Dormant { raw_number, .. },
                    None,
                ) => Ok(raw_number),
            }
            .map_err(|reason| TeletextMotEncodingError {
                owner: declaration.identity(),
                link,
                slot,
                reason,
            })?;
        }
    }

    let mut nibbles = [[0_u8; 40]; 17];
    for association in declaration.associations() {
        let tens = usize::from(association.page() >> 4);
        let units = usize::from(association.page() & 15);
        let (row, pair) = if units < 10 {
            let ordinal = 10 * tens + units;
            (ordinal / 20, ordinal % 20)
        } else {
            let ordinal = 6 * tens + units - 10;
            (8 + ordinal / 18, ordinal % 18)
        };
        nibbles[row][pair * 2] = association_nibble(association.objects());
        nibbles[row][pair * 2 + 1] = association_nibble(association.drcs());
    }
    let indices = std::iter::once(None).chain(TeletextMotPublicLinkIndex::ALL.map(Some));
    for (ordinal, index) in indices.enumerate() {
        let object = links.object_link(index);
        let record = &mut nibbles[14 + ordinal / 4][10 * (ordinal % 4)..10 * (ordinal % 4) + 10];
        record[0] = object.target().wire_magazine() | (u8::from(object.supersedable()) << 3);
        record[1] = object.target().page_byte() >> 4;
        record[2] = object.target().page_byte() & 15;
        record[3] = object.extent().maximum().get();
        record[4] = object.raw_fallback().get();
        let defaults = object.defaults();
        record[5] = default_type(defaults[0]) | (default_type(defaults[1]) << 2);
        for (slot, selector) in selectors[ordinal].into_iter().enumerate() {
            record[6 + slot * 2] = selector & 15;
            record[7 + slot * 2] = selector >> 4;
        }
        let drcs = links.drcs_link(index);
        let record = &mut nibbles[16][4 * ordinal..4 * ordinal + 4];
        record[0] = drcs.target().wire_magazine() | (u8::from(drcs.supersedable()) << 3);
        record[1] = drcs.target().page_byte() >> 4;
        record[2] = drcs.target().page_byte() & 15;
        record[3] = drcs.extent().maximum().get();
    }
    let counts = links.counts();
    for (target, value) in nibbles[16][34..].chunks_exact_mut(2).zip([
        counts.magazine_mask(),
        counts.object_pages(),
        counts.drcs_pages(),
    ]) {
        target[0] = value & 15;
        target[1] = value >> 4;
    }
    Ok(std::array::from_fn(|index| TeletextMotPacketBody {
        row: ROWS[index],
        body: nibbles[index].map(hamming_8_4),
    }))
}

fn association_nibble(selection: TeletextMotAssociationSelection) -> u8 {
    (u8::from(selection.global()) << 3)
        | selection
            .public()
            .map_or(0, TeletextMotPublicLinkIndex::get)
}

fn default_type(default: TeletextMotDefaultObject) -> u8 {
    match default {
        TeletextMotDefaultObject::NoObject { .. } => 0,
        TeletextMotDefaultObject::Dormant { object_type, .. }
        | TeletextMotDefaultObject::Object { object_type, .. } => match object_type {
            TeletextObjectType::Active => 1,
            TeletextObjectType::Adaptive => 2,
            TeletextObjectType::Passive => 3,
        },
    }
}
