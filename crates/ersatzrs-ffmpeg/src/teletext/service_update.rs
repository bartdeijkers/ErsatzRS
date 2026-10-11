//! Consecutive full-resource lifecycle checks over actual immutable encoded pages.
//!
//! The owner confirms completed delivery before replacing a generation; these
//! local checks neither deliver nor acknowledge the planned closing batch.
//! Unknown owners remain the host's responsibility; M29 has no resource counter.

use std::collections::BTreeMap;

use ersatzrs_domain::{
    TeletextRawDataPageNumber, TeletextResourceHeaderMetadata, TeletextResourceIdentity,
    TeletextResourceTransmission, TeletextResourceUpdateCounter,
};

use super::{
    packets, EncodedTeletextAuthoredPage, EncodedTeletextAuthoredService, EncodedTeletextPageSet,
    TeletextTransportError,
};

type Owner = (TeletextRawDataPageNumber, u8);

#[derive(Debug)]
struct CompletedResource {
    counter: TeletextResourceUpdateCounter,
    page: Option<EncodedTeletextAuthoredPage>,
}

/// Latest completed bodies and counter-only tombstones, never pending candidates.
/// Checked raw addresses/S1 bound this map to 8 * 255 * 16 = 32,640 keys.
#[derive(Debug, Default)]
pub(super) struct ResourceUpdateHistory {
    resources: BTreeMap<Owner, CompletedResource>,
}

impl ResourceUpdateHistory {
    /// Validate without moving bodies or advancing any historical counter.
    /// A supplied completed generation supersedes older retained live snapshots.
    pub(super) fn validate(
        &self,
        next: &EncodedTeletextAuthoredService,
        completed: Option<&EncodedTeletextAuthoredService>,
    ) -> Result<(), TeletextTransportError> {
        // Authenticated candidates have unique physical raw page/S1 owners.
        // This temporary borrowed index has the same finite key namespace.
        let current = completed.map(|service| {
            service
                .pages()
                .iter()
                .filter_map(|page| {
                    page.resource_header()
                        .map(|header| (owner(header.identity()), page))
                })
                .collect::<BTreeMap<_, _>>()
        });
        for page in next.pages() {
            let Some(header) = page.resource_header() else {
                continue;
            };
            let key = owner(header.identity());
            if let Some(previous) = current.as_ref().and_then(|pages| pages.get(&key)) {
                if let Some(previous_header) = previous.resource_header() {
                    check(
                        header,
                        previous_header.update_counter(),
                        same_content(previous, page),
                    )?;
                }
            } else if let Some(previous) = self.resources.get(&key) {
                // Absence from a newer completed service retires an older live
                // owner even before that service has been moved into history.
                let equal = completed.is_none()
                    && previous
                        .page
                        .as_ref()
                        .is_some_and(|previous| same_content(previous, page));
                check(header, previous.counter, equal)?;
            }
        }
        Ok(())
    }

    /// Consume a fully completed generation; removed owners keep only counters.
    pub(super) fn record_completed(&mut self, service: EncodedTeletextAuthoredService) {
        for previous in self.resources.values_mut() {
            previous.page = None;
        }
        for page in service.into_resource_pages() {
            if let Some(header) = page.resource_header() {
                self.resources.insert(
                    owner(header.identity()),
                    CompletedResource {
                        counter: header.update_counter(),
                        page: Some(page),
                    },
                );
            }
        }
    }

    /// A completed legacy display may overwrite a retained resource raw page.
    /// Keep every known S1 counter but discard matching cached bodies only.
    pub(super) fn retire_display_owners(&mut self, set: &EncodedTeletextPageSet) {
        for page in set.pages() {
            let number = page.address().page;
            // Checked decimal display identities cannot encode reserved FF.
            if let Ok(raw) = TeletextRawDataPageNumber::new(number.magazine(), number.wire_page()) {
                for previous in self
                    .resources
                    .range_mut((raw, 0)..=(raw, 15))
                    .map(|(_, entry)| entry)
                {
                    previous.page = None;
                }
            }
        }
    }
}

fn owner(resource: TeletextResourceIdentity) -> Owner {
    (resource.page(), resource.logical_subpage().get())
}

fn check(
    header: TeletextResourceHeaderMetadata,
    previous: TeletextResourceUpdateCounter,
    equal: bool,
) -> Result<(), TeletextTransportError> {
    // Byte-identical deliberate full refreshes may declare Changed/next S2.
    // Equal final bytes do not imply all intervening receiver history is known.
    let (expected, expected_transmission) =
        if equal && header.transmission() == TeletextResourceTransmission::FullUnchangedFirst {
            (previous, TeletextResourceTransmission::FullUnchangedFirst)
        } else {
            (
                previous.next_modulo_eight(),
                TeletextResourceTransmission::FullChangedFirst,
            )
        };
    if header.update_counter() != expected || header.transmission() != expected_transmission {
        return Err(TeletextTransportError::ResourceUpdate {
            resource: header.identity(),
            previous,
            requested: header.update_counter(),
            expected,
            transmission: header.transmission(),
            expected_transmission,
        });
    }
    Ok(())
}

fn same_content(
    previous: &EncodedTeletextAuthoredPage,
    next: &EncodedTeletextAuthoredPage,
) -> bool {
    let (Some(previous_header), Some(next_header)) =
        (previous.resource_header(), next.resource_header())
    else {
        return false;
    };
    previous_header.identity().kind() == next_header.identity().kind()
        && previous_header.last_packet() == next_header.last_packet()
        && same_units(
            previous_header,
            previous.data_units(),
            next_header,
            next.data_units(),
        )
}

fn same_units(
    previous_header: TeletextResourceHeaderMetadata,
    previous: &[[u8; 46]],
    next_header: TeletextResourceHeaderMetadata,
    next: &[[u8; 46]],
) -> bool {
    previous.len() == next.len()
        && previous
            .iter()
            .zip(next)
            .enumerate()
            .all(|(row, (previous, next))| {
                previous
                    .iter()
                    .zip(next)
                    .enumerate()
                    .all(|(column, (&previous, &next))| {
                        if row == 0 && column == 9 {
                            // This entire Hamming nibble contains only S2 and C4.
                            true
                        } else if row == 0 && column == 12 {
                            normalized_c8(previous_header, previous)
                                == normalized_c8(next_header, next)
                        } else {
                            previous == next
                        }
                    })
            })
}

fn normalized_c8(header: TeletextResourceHeaderMetadata, value: u8) -> u8 {
    if header.transmission().update_indicator() {
        value ^ (packets::hamming_8_4(0) ^ packets::hamming_8_4(2)).reverse_bits()
    } else {
        value
    }
}

#[cfg(test)]
#[path = "service_update_tests.rs"]
pub(super) mod tests;
