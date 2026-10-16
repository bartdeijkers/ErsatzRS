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
type CurrentResources<'a> = BTreeMap<
    Owner,
    (
        TeletextResourceUpdateCounter,
        &'a EncodedTeletextAuthoredPage,
    ),
>;

/// A completed carousel retains its effective emitted counters separately from
/// the immutable accepted baseline. Pending candidates never enter this view.
#[derive(Clone, Copy)]
pub(super) struct CompletedResourceService<'a> {
    pub service: &'a EncodedTeletextAuthoredService,
    pub selected: Option<&'a super::resource_lifecycle::SelectedResourceHeaders>,
}

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
        self.validate_with_selected(
            next,
            completed.map(|service| CompletedResourceService {
                service,
                selected: None,
            }),
        )
    }

    pub(super) fn validate_with_selected(
        &self,
        next: &EncodedTeletextAuthoredService,
        completed: Option<CompletedResourceService<'_>>,
    ) -> Result<(), TeletextTransportError> {
        let current = completed.map(current_resources);
        for page in next.pages() {
            let Some(header) = page.resource_header() else {
                continue;
            };
            if let Some((previous, equal)) = self.previous(page, current.as_ref()) {
                check(header, previous, equal)?;
            }
        }
        Ok(())
    }

    /// Select only checked service-bound headers against completed receiver state.
    /// Unknown owners retain their explicit cold baseline; history is not changed.
    pub(super) fn select_headers(
        &self,
        next: &EncodedTeletextAuthoredService,
        completed: Option<CompletedResourceService<'_>>,
    ) -> super::resource_lifecycle::SelectedResourceHeaders {
        let mut selected = super::resource_lifecycle::SelectedResourceHeaders::new();
        let Some(plan) = next.resource_lifecycle_variants() else {
            return selected;
        };
        let current = completed.map(current_resources);
        for entry in plan.entries() {
            let page = &next.pages()[entry.physical_page_index()];
            let Some(header) = page.resource_header() else {
                continue;
            };
            let (counter, changed) =
                if let Some((previous, equal)) = self.previous(page, current.as_ref()) {
                    if equal
                        && header.transmission() == TeletextResourceTransmission::FullUnchangedFirst
                    {
                        (previous, false)
                    } else {
                        (previous.next_modulo_eight(), true)
                    }
                } else {
                    (
                        header.update_counter(),
                        header.transmission() == TeletextResourceTransmission::FullChangedFirst,
                    )
                };
            selected.insert(entry.physical_page_index(), entry.select(counter, changed));
        }
        selected
    }

    fn previous(
        &self,
        next: &EncodedTeletextAuthoredPage,
        current: Option<&CurrentResources<'_>>,
    ) -> Option<(TeletextResourceUpdateCounter, bool)> {
        let header = next.resource_header()?;
        let key = owner(header.identity());
        if let Some((counter, previous)) = current.and_then(|pages| pages.get(&key)) {
            Some((*counter, same_content(previous, next)))
        } else {
            self.resources.get(&key).map(|previous| {
                let equal = current.is_none()
                    && previous
                        .page
                        .as_ref()
                        .is_some_and(|page| same_content(page, next));
                (previous.counter, equal)
            })
        }
    }

    /// Consume a fully completed generation; removed owners keep only counters.
    pub(super) fn record_completed(&mut self, service: EncodedTeletextAuthoredService) {
        self.record_completed_with_selected(service, &BTreeMap::default());
    }

    pub(super) fn record_completed_with_selected(
        &mut self,
        service: EncodedTeletextAuthoredService,
        selected: &super::resource_lifecycle::SelectedResourceHeaders,
    ) {
        let counters = selected_counters(&service, selected);
        for previous in self.resources.values_mut() {
            previous.page = None;
        }
        for page in service.into_resource_pages() {
            if let Some(header) = page.resource_header() {
                self.resources.insert(
                    owner(header.identity()),
                    CompletedResource {
                        counter: counters
                            .get(&owner(header.identity()))
                            .copied()
                            .unwrap_or(header.update_counter()),
                        page: Some(page),
                    },
                );
            }
        }
    }

    /// Retain only completed resource bodies when another epoch shares content.
    /// Native/compiled displays and their semantic projections are never cloned.
    pub(super) fn record_completed_shared(&mut self, service: &EncodedTeletextAuthoredService) {
        self.record_completed_shared_with_selected(service, &BTreeMap::default());
    }

    pub(super) fn record_completed_shared_with_selected(
        &mut self,
        service: &EncodedTeletextAuthoredService,
        selected: &super::resource_lifecycle::SelectedResourceHeaders,
    ) {
        for previous in self.resources.values_mut() {
            previous.page = None;
        }
        for (index, page) in service.pages().iter().enumerate() {
            let resource = match page {
                EncodedTeletextAuthoredPage::Object(page) => {
                    EncodedTeletextAuthoredPage::Object(page.clone())
                }
                EncodedTeletextAuthoredPage::Resource(page) => {
                    EncodedTeletextAuthoredPage::Resource(page.clone())
                }
                EncodedTeletextAuthoredPage::NativeDisplay(_)
                | EncodedTeletextAuthoredPage::CompiledDisplay(_) => continue,
            };
            if let Some(header) = resource.resource_header() {
                self.resources.insert(
                    owner(header.identity()),
                    CompletedResource {
                        counter: selected
                            .get(&index)
                            .map_or(header.update_counter(), |selected| {
                                selected.metadata.update_counter()
                            }),
                        page: Some(resource),
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

fn current_resources(completed: CompletedResourceService<'_>) -> CurrentResources<'_> {
    completed
        .service
        .pages()
        .iter()
        .enumerate()
        .filter_map(|(index, page)| {
            page.resource_header().map(|header| {
                let counter = completed
                    .selected
                    .and_then(|selected| selected.get(&index))
                    .map_or(header.update_counter(), |selected| {
                        selected.metadata.update_counter()
                    });
                (owner(header.identity()), (counter, page))
            })
        })
        .collect()
}

fn selected_counters(
    service: &EncodedTeletextAuthoredService,
    selected: &super::resource_lifecycle::SelectedResourceHeaders,
) -> BTreeMap<Owner, TeletextResourceUpdateCounter> {
    selected
        .iter()
        .filter_map(|(index, selected)| {
            service
                .pages()
                .get(*index)
                .and_then(EncodedTeletextAuthoredPage::resource_header)
                .map(|header| (owner(header.identity()), selected.metadata.update_counter()))
        })
        .collect()
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
