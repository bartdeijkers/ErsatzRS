//! Service-bound finite resource headers; selection remains response-local.

use std::{collections::BTreeMap, time::Instant};

use ersatzrs_domain::{
    TeletextResourceHeaderMetadata, TeletextResourceIdentity, TeletextResourceTransmission,
    TeletextResourceUpdateCounter,
};

use super::{
    encode_teletext_resource_header, EncodedTeletextAuthoredPage, EncodedTeletextAuthoredService,
    TeletextResourceTransmissionMode,
};

const MAX_RESOURCE_OWNERS: usize = 8 * 255 * 16;

/// Failure to derive a checked header plan from its immutable native service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextResourceLifecycleError {
    #[error("teletext resource lifecycle derivation exceeded its deadline")]
    TimedOut,
    #[error("teletext resource lifecycle plan exceeds its finite owner/storage bound")]
    Budget,
    #[error("teletext resource {resource:?} is not a complete first transmission")]
    Unsupported { resource: TeletextResourceIdentity },
    #[error("teletext resource {resource:?} header differs from its native encoder")]
    Correspondence { resource: TeletextResourceIdentity },
}

/// Checked first-header alternatives for one exact native physical resource.
/// Entries cannot be constructed or attached independently of their service.
#[derive(Debug)]
pub struct TeletextResourceLifecycleEntry {
    resource: TeletextResourceIdentity,
    physical_page_index: usize,
    headers: [[u8; 46]; 16],
    metadata: [TeletextResourceHeaderMetadata; 16],
}

impl TeletextResourceLifecycleEntry {
    /// Exact native resource purpose, raw page and logical S1.
    #[must_use]
    pub const fn resource(&self) -> TeletextResourceIdentity {
        self.resource
    }

    /// Index into the owning service's unchanged physical page vector.
    #[must_use]
    pub const fn physical_page_index(&self) -> usize {
        self.physical_page_index
    }

    /// Counter order 0 through 7, each with unchanged-first then changed-first.
    /// All sixteen arrays retain every non-lifecycle byte of the baseline.
    #[must_use]
    pub const fn headers(&self) -> &[[u8; 46]; 16] {
        &self.headers
    }

    pub(super) fn select(
        &self,
        counter: TeletextResourceUpdateCounter,
        changed: bool,
    ) -> SelectedResourceHeader {
        let unchanged = usize::from(counter.get()) * 2;
        let first = unchanged + usize::from(changed);
        SelectedResourceHeader {
            metadata: self.metadata[first],
            first: self.headers[first],
            repeat: self.headers[unchanged],
        }
    }
}

/// Immutable alternatives derived from every full resource in native physical
/// page order. The owning service alone can construct and retain this plan.
#[derive(Debug)]
pub struct TeletextResourceLifecyclePlan {
    entries: Vec<TeletextResourceLifecycleEntry>,
}

impl TeletextResourceLifecyclePlan {
    /// Native resource order, with each entry's unchanged/changed counter pairs.
    #[must_use]
    pub fn entries(&self) -> &[TeletextResourceLifecycleEntry] {
        &self.entries
    }

    pub(super) fn derive(
        service: &EncodedTeletextAuthoredService,
        deadline: Instant,
    ) -> Result<Self, TeletextResourceLifecycleError> {
        check_deadline(deadline)?;
        let count = service
            .pages()
            .iter()
            .filter(|page| page.resource_header().is_some())
            .count();
        if count > MAX_RESOURCE_OWNERS {
            return Err(TeletextResourceLifecycleError::Budget);
        }
        let mut entries = Vec::new();
        entries
            .try_reserve_exact(count)
            .map_err(|_| TeletextResourceLifecycleError::Budget)?;
        for (physical_page_index, page) in service.pages().iter().enumerate() {
            check_deadline(deadline)?;
            let Some(baseline) = page.resource_header() else {
                continue;
            };
            let mode = match page {
                EncodedTeletextAuthoredPage::Object(page) => page.transmission_mode(),
                EncodedTeletextAuthoredPage::Resource(page) => page.transmission_mode(),
                _ => {
                    return Err(TeletextResourceLifecycleError::Correspondence {
                        resource: baseline.identity(),
                    })
                }
            };
            entries.push(derive_entry(
                page,
                baseline,
                mode,
                physical_page_index,
                deadline,
            )?);
        }
        check_deadline(deadline)?;
        Ok(Self { entries })
    }
}

fn derive_entry(
    page: &EncodedTeletextAuthoredPage,
    baseline: TeletextResourceHeaderMetadata,
    mode: TeletextResourceTransmissionMode,
    physical_page_index: usize,
    deadline: Instant,
) -> Result<TeletextResourceLifecycleEntry, TeletextResourceLifecycleError> {
    let resource = baseline.identity();
    if !matches!(
        baseline.transmission(),
        TeletextResourceTransmission::FullUnchangedFirst
            | TeletextResourceTransmission::FullChangedFirst
    ) {
        return Err(TeletextResourceLifecycleError::Unsupported { resource });
    }
    let original = encode_teletext_resource_header(baseline, mode);
    if page.data_units().first() != Some(&original) {
        return Err(TeletextResourceLifecycleError::Correspondence { resource });
    }
    let mut headers = [original; 16];
    let mut metadata = [baseline; 16];
    for counter in 0..8_u8 {
        for changed in [false, true] {
            check_deadline(deadline)?;
            let index = usize::from(counter) * 2 + usize::from(changed);
            let state = TeletextResourceHeaderMetadata::new(
                resource,
                baseline.last_packet(),
                TeletextResourceUpdateCounter::new(counter)
                    .map_err(|_| TeletextResourceLifecycleError::Correspondence { resource })?,
                if changed {
                    TeletextResourceTransmission::FullChangedFirst
                } else {
                    TeletextResourceTransmission::FullUnchangedFirst
                },
            )
            .map_err(|_| TeletextResourceLifecycleError::Correspondence { resource })?;
            let header = encode_teletext_resource_header(state, mode);
            if header
                .iter()
                .zip(original)
                .enumerate()
                .any(|(offset, (a, b))| offset != 9 && offset != 12 && *a != b)
            {
                return Err(TeletextResourceLifecycleError::Correspondence { resource });
            }
            headers[index] = header;
            metadata[index] = state;
        }
    }
    Ok(TeletextResourceLifecycleEntry {
        resource,
        physical_page_index,
        headers,
        metadata,
    })
}

fn check_deadline(deadline: Instant) -> Result<(), TeletextResourceLifecycleError> {
    if Instant::now() >= deadline {
        Err(TeletextResourceLifecycleError::TimedOut)
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct SelectedResourceHeader {
    pub metadata: TeletextResourceHeaderMetadata,
    pub first: [u8; 46],
    pub repeat: [u8; 46],
}

pub(super) type SelectedResourceHeaders = BTreeMap<usize, SelectedResourceHeader>;

#[cfg(test)]
#[path = "resource_lifecycle_tests.rs"]
mod tests;
