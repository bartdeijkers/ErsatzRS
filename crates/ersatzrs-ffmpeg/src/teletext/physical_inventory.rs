//! Final identity fence for explicitly supplied, synthesized page headers.
//!
//! Display raw subcodes and resource logical S1 are distinct. Resource header
//! metadata supplies actual `nn`/counter bits only after body compilation. This
//! identity check neither proves complete service inventory nor grants readiness,
//! publication, ownership or transport authority.

use std::collections::BTreeMap;

use ersatzrs_domain::{
    TeletextPageAddress, TeletextRawDataPageNumber, TeletextResourceHeaderMetadata,
    TeletextResourceSubpageIndex,
};

/// One unique compiled page, supplied once per service generation.
///
/// Include explicitly emitted empty companion pages and MOTs. Repeated references
/// are not additional pages. Carousel retransmissions and continuation fragments
/// are not separate inventory records. Resource metadata must come from the
/// corresponding compiled body, not an inferred subcode or semantic host ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextPhysicalPageInventoryRecord {
    Display(TeletextPageAddress),
    Resource(TeletextResourceHeaderMetadata),
}

/// Numeric locations in the supplied page list, without authored content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextPhysicalPageInventoryError {
    #[error("page inventory has {count} records, exceeding trusted budget {maximum}")]
    Budget { count: usize, maximum: usize },
    #[error("page records {first} and {second} share physical header {magazine}/{page:02X}/{subcode:04X}")]
    PhysicalCollision {
        first: usize,
        second: usize,
        /// Logical magazine 1 through 8, rather than magazine-eight wire zero.
        magazine: u8,
        /// Original BCD display or raw resource page byte.
        page: u8,
        subcode: u16,
    },
    #[error("resource records {first} and {second} share logical owner {page}/{subpage:?}")]
    ResourceOwnerCollision {
        first: usize,
        second: usize,
        page: TeletextRawDataPageNumber,
        subpage: TeletextResourceSubpageIndex,
    },
}

/// Check actual physical headers and stable resource ownership before emission.
///
/// The raw record count is checked in constant time before indexing. Physical
/// identity is logical magazine/raw page byte/exact subcode: purpose, lifecycle
/// and header controls cannot distinguish two colliding pages. Resource owners
/// additionally use raw page/logical S1, independent of kind, `nn` and counter.
/// Different kinds at different S1 values remain legal; whole-page purpose
/// exclusivity is not invented. Display subcodes are never reduced to S1.
///
/// Callers must supply all unique compiled pages, including empty companions and
/// MOTs, once per generation. This checks only the supplied identities, not body
/// correspondence, dependency completeness, packet order or carousel timing.
/// # Errors
/// Rejects the trusted count budget before inspecting records, then reports the
/// first physical or logical-resource collision with zero-based record indices.
/// Inputs are borrowed and remain unchanged; no partial inventory is returned.
pub fn validate_teletext_physical_page_inventory(
    records: &[TeletextPhysicalPageInventoryRecord],
    max_records: usize,
) -> Result<(), TeletextPhysicalPageInventoryError> {
    if records.len() > max_records {
        return Err(TeletextPhysicalPageInventoryError::Budget {
            count: records.len(),
            maximum: max_records,
        });
    }
    let mut physical = BTreeMap::new();
    let mut resources = BTreeMap::new();
    for (second, record) in records.iter().enumerate() {
        let (magazine, page, subcode) = match *record {
            TeletextPhysicalPageInventoryRecord::Display(address) => (
                address.page.magazine(),
                address.page.wire_page(),
                address.subcode.get(),
            ),
            TeletextPhysicalPageInventoryRecord::Resource(header) => (
                header.identity().page().magazine(),
                header.identity().page().page(),
                header.physical_subcode(),
            ),
        };
        if let Some(&first) = physical.get(&(magazine, page, subcode)) {
            return Err(TeletextPhysicalPageInventoryError::PhysicalCollision {
                first,
                second,
                magazine,
                page,
                subcode,
            });
        }
        physical.insert((magazine, page, subcode), second);
        if let TeletextPhysicalPageInventoryRecord::Resource(header) = *record {
            let page = header.identity().page();
            let subpage = header.identity().logical_subpage();
            if let Some(&first) = resources.get(&(page, subpage)) {
                return Err(TeletextPhysicalPageInventoryError::ResourceOwnerCollision {
                    first,
                    second,
                    page,
                    subpage,
                });
            }
            resources.insert((page, subpage), second);
        }
    }
    Ok(())
}
