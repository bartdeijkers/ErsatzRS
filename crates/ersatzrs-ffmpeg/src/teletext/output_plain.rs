//! Ordinary audiovisual continuation after a terminal teletext output failure.
//!
//! The gate only substitutes null packets and rewrites PMT slots. Retained
//! source packets that were never exposed are therefore the exact source
//! multiplex. Continuing them withdraws the authored stream from the PMT with
//! an advanced version and continuing output CC; every other packet, including
//! audio, video and PCR, is forwarded byte-for-byte at its original position.

use super::{
    TeletextMultiplexError, TeletextOutputError, TeletextOutputGate, TeletextPmtInstallError,
    TeletextPmtInstaller,
};
use crate::teletext::signalling::MAX_WINDOW_PACKETS;

const PACKET_BYTES: usize = 188;

/// Source continuation without the authored stream, taken from a failed gate.
///
/// It carries only source bytes that were never exposed to the sink. The owner
/// writes `push` output in order, then `finish` output at EOF. Pacing, client
/// writes, cancellation and producer lifetime remain with the owner. A source
/// whose PMT changes or whose framing breaks still fails explicitly.
pub struct TeletextPlainContinuation {
    installer: TeletextPmtInstaller,
    carry: Vec<[u8; PACKET_BYTES]>,
    partial: Vec<u8>,
}

impl TeletextPlainContinuation {
    pub(super) fn new(
        installer: &TeletextPmtInstaller,
        carry: Vec<[u8; PACKET_BYTES]>,
        partial: Vec<u8>,
    ) -> Result<Self, TeletextOutputError> {
        let mut installer = installer.clone();
        let plan = installer.prepare_reconfiguration(None)?;
        installer.reconfigure(plan)?;
        Ok(Self {
            installer,
            carry,
            partial,
        })
    }

    /// Source packets and bytes currently held before they can be forwarded.
    #[must_use]
    pub fn held_bytes(&self) -> usize {
        self.carry.len() * PACKET_BYTES + self.partial.len()
    }

    /// Append source bytes and return every packet that can now be written.
    ///
    /// Packets wait only while a source PMT section is incomplete or the
    /// withdrawal has neither a source PMT slot nor a null packet to use.
    /// # Errors
    /// A changed source PMT, malformed packets or an unbounded wait fail.
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<u8>, TeletextOutputError> {
        let mut bytes = bytes;
        if !self.partial.is_empty() {
            let count = bytes.len().min(PACKET_BYTES - self.partial.len());
            self.partial.extend_from_slice(&bytes[..count]);
            bytes = &bytes[count..];
            if self.partial.len() == PACKET_BYTES {
                let mut packet = [0_u8; PACKET_BYTES];
                packet.copy_from_slice(&self.partial);
                self.carry.push(packet);
                self.partial.clear();
            }
        }
        let mut chunks = bytes.chunks_exact(PACKET_BYTES);
        for chunk in chunks.by_ref() {
            let mut packet = [0_u8; PACKET_BYTES];
            packet.copy_from_slice(chunk);
            self.carry.push(packet);
        }
        self.partial.extend_from_slice(chunks.remainder());
        self.release(false)
    }

    /// Release the remaining packets at source EOF.
    /// # Errors
    /// A partial packet or an incomplete PMT section at EOF fails.
    pub fn finish(mut self) -> Result<Vec<u8>, TeletextOutputError> {
        if !self.partial.is_empty() {
            return Err(super::TeletextBufferError::PartialPacket.into());
        }
        self.release(true)
    }

    fn release(&mut self, final_window: bool) -> Result<Vec<u8>, TeletextOutputError> {
        if self.carry.is_empty() {
            return Ok(Vec::new());
        }
        let mut output = Vec::new();
        for window in self.carry.chunks(MAX_WINDOW_PACKETS) {
            let mut window = window.to_vec();
            match self.installer.install(&mut window) {
                Ok(_) => output.extend(window),
                Err(
                    error @ (TeletextPmtInstallError::Incomplete
                    | TeletextPmtInstallError::Multiplex(TeletextMultiplexError::Capacity {
                        ..
                    })),
                ) if output.len() + window.len() == self.carry.len() => {
                    if final_window {
                        if error != TeletextPmtInstallError::Incomplete {
                            // EOF ends the stream before the withdrawal found
                            // room; the complete tail stays byte-exact.
                            output.extend(window);
                            break;
                        }
                        // An incomplete source table at EOF is never exposed.
                        return Err(error.into());
                    }
                    // Only the last chunk can wait for more source input.
                    if window.len() >= MAX_WINDOW_PACKETS {
                        return Err(TeletextMultiplexError::Window.into());
                    }
                    break;
                }
                Err(error) => return Err(error.into()),
            }
        }
        self.carry.drain(..output.len());
        Ok(output.into_iter().flatten().collect())
    }
}

impl TeletextOutputGate {
    /// Take the source continuation after a terminal failure, if one is safe.
    ///
    /// Available only once: after a failure that left every unexposed source
    /// byte retained and the PMT continuity consistent with delivered output.
    /// Failures of the retention buffer itself, exposed windows and closed
    /// generations have no continuation; their bytes are already discarded.
    #[must_use]
    pub fn take_plain_continuation(&mut self) -> Option<TeletextPlainContinuation> {
        self.plain.take()
    }
}
