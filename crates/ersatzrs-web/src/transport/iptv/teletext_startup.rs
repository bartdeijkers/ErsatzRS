//! Exact already-consumed startup bytes with their original source clock.

use std::time::Duration;
use tokio::time::Instant;

use super::teletext_output::TeletextPumpError;

/// A bounded raw prefix, before any source byte was written to the output sink.
/// All contained bytes conservatively retain the oldest observed arrival.
/// These timestamps and bytes are supplied by the source owner, not an
/// authenticated source or publication receipt. No reopening is permitted.
pub(super) struct TeletextStartupPrefix {
    bytes: Vec<u8>,
    epoch: Instant,
    first_arrival: Duration,
}

impl TeletextStartupPrefix {
    #[cfg(test)]
    pub(super) fn new(
        bytes: Vec<u8>,
        epoch: Instant,
        first_observed: Instant,
    ) -> Result<Self, TeletextPumpError> {
        Self::new_preserving(bytes, epoch, first_observed).map_err(|(error, _)| error)
    }

    pub(super) fn new_preserving(
        bytes: Vec<u8>,
        epoch: Instant,
        first_observed: Instant,
    ) -> Result<Self, (TeletextPumpError, Vec<u8>)> {
        let now = Instant::now();
        if bytes.is_empty()
            || bytes.capacity() > 32 * 1024 * 1024
            || first_observed < epoch
            || first_observed > now
        {
            return Err((TeletextPumpError::Configuration, bytes));
        }
        Ok(Self {
            bytes,
            epoch,
            first_arrival: first_observed.duration_since(epoch),
        })
    }

    pub(super) const fn epoch(&self) -> Instant {
        self.epoch
    }

    pub(super) const fn first_arrival(&self) -> Duration {
        self.first_arrival
    }

    pub(super) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(super) fn retained_bytes(&self) -> usize {
        self.bytes.capacity()
    }

    pub(super) fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}
