//! Inclusive known-object interval lower bound (EN 300 706 Annex A.8).
//!
//! All three object types count together, regardless of selected panel placement
//! or screen effect. Pending intervals are retained, not assigned an invented
//! endpoint. A bound of fifteen known open objects alone is not full acceptance.

use std::mem::size_of;

use super::{
    TeletextDisplayGeometryError, TeletextDisplayGeometryFailure, TeletextObjectAddressInterval,
    TeletextObjectDisplayRegion, TeletextPageAddress,
};

/// Known simultaneous-open lower bound, not whole-page A.8/readiness approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextObjectOpenCount {
    known_intervals: usize,
    pending_intervals: usize,
    maximum_known_open: usize,
}

impl TeletextObjectOpenCount {
    #[must_use]
    pub const fn known_intervals(&self) -> usize {
        self.known_intervals
    }
    #[must_use]
    pub const fn pending_intervals(&self) -> usize {
        self.pending_intervals
    }
    /// Inclusive known raw intervals only. Pending intervals may increase this
    /// lower bound; successive opens, occupied overlap and C.3 remain separate.
    #[must_use]
    pub const fn maximum_known_open(&self) -> usize {
        self.maximum_known_open
    }
}

#[derive(Debug, Clone, Copy)]
struct Endpoint {
    position: (u16, u16),
    end: bool,
    instance: usize,
}

pub(super) fn endpoint_storage_bytes(instances: usize) -> Option<usize> {
    instances.checked_mul(2)?.checked_mul(size_of::<Endpoint>())
}

pub(super) fn endpoint_work(instances: usize) -> Option<usize> {
    let count = instances.checked_mul(2)?;
    let levels = usize::BITS - count.leading_zeros();
    count.checked_mul(levels as usize + 1)
}

pub(super) fn count_open_objects(
    owner: TeletextPageAddress,
    regions: &[TeletextObjectDisplayRegion],
) -> Result<TeletextObjectOpenCount, TeletextDisplayGeometryError> {
    // The geometry factory includes this scratch bound in its pre-allocation
    // fence. Keep the internal helper bounded independently too.
    if endpoint_storage_bytes(regions.len()).is_none_or(|bytes| bytes > isize::MAX as usize)
        || endpoint_work(regions.len()).is_none()
    {
        return Err(TeletextDisplayGeometryError {
            owner,
            position: None,
            location: None,
            root_source: None,
            invocation: None,
            reason: TeletextDisplayGeometryFailure::AllocationSize,
        });
    }
    let known_intervals = regions
        .iter()
        .filter(|region| {
            matches!(
                region.interval(),
                TeletextObjectAddressInterval::Known { .. }
            )
        })
        .count();
    // The upper-bound multiplication above proves this exact count fits.
    let mut endpoints = Vec::with_capacity(known_intervals * 2);
    for region in regions {
        if let TeletextObjectAddressInterval::Known { start, finish } = region.interval() {
            let instance = region.instance().id().get();
            endpoints.push(Endpoint {
                position: start,
                end: false,
                instance,
            });
            endpoints.push(Endpoint {
                position: finish,
                end: true,
                instance,
            });
        }
    }
    // Starts precede ends at the same raw cell: intervals are INCLUSIVE. Instance
    // IDs stabilize diagnostics, never invent physical transmission ordering.
    endpoints.sort_unstable_by_key(|point| (point.position, point.end, point.instance));
    let (mut open, mut maximum_known_open) = (0, 0);
    for endpoint in endpoints {
        if endpoint.end {
            open -= 1;
        } else {
            open += 1;
            maximum_known_open = maximum_known_open.max(open);
            if open > 15 {
                let region = &regions[endpoint.instance];
                let instance = region.instance();
                let start = instance.caller_position();
                return Err(TeletextDisplayGeometryError {
                    owner,
                    position: Some((start.row(), start.column())),
                    location: Some(instance.source()),
                    root_source: Some(instance.root_source()),
                    invocation: Some(instance.id()),
                    reason: TeletextDisplayGeometryFailure::TooManyOpenObjects {
                        count: open,
                        maximum: 15,
                    },
                });
            }
        }
    }
    Ok(TeletextObjectOpenCount {
        known_intervals,
        pending_intervals: regions.len() - known_intervals,
        maximum_known_open,
    })
}
