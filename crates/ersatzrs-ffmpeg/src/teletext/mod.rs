//! Authored packet coding and supplied source metadata (EN 300 706/472, ADR 0121).
//!
//! Encoding accepts host domain drafts, not raw add-on transport bytes.
//! `source_assembly` bounds supplied TS packets on one explicit PID; `source_pes`
//! checks one complete PES and `source_header` reports one supplied DVB unit.
//! These establish neither acquisition nor page readiness, nor grant
//! publication: carousel timing, PES/PSI/PID capacity,
//! output ownership and client acceptance remain separate transport obligations.

mod buffer;
mod carousel;
mod charset;
mod clock;
mod compositional;
mod drcs;
mod layout;
mod lift;
mod mot;
mod output;
mod packets;
mod pmt_install;
mod presentation;
mod psi;
mod psi_framing;
mod psi_monitor;
mod signalling;
pub mod source_assembly;
pub mod source_header;
pub mod source_pes;
mod transport;

use ersatzrs_domain::{TeletextPageAddress, TeletextStructureError};

pub use buffer::{TeletextBufferError, TeletextPacketBuffer};
pub use carousel::{TeletextCarousel, TeletextCarouselBatch, TeletextCarouselBatchKind};
pub use clock::{
    TeletextClockConfig, TeletextClockError, TeletextClockReport, TeletextClockValidator,
};
pub use compositional::encode_teletext_compositional_links;
pub use drcs::{encode_teletext_drcs_bodies, TeletextDrcsPacketBodies};
pub use lift::lift_teletext_v1_page_set;
pub use mot::{
    encode_teletext_mot_bodies, TeletextMotAllocationFailure, TeletextMotDefaultAllocation,
    TeletextMotDefaultAllocations, TeletextMotEncodingError, TeletextMotPacketBody,
};
pub use output::{
    TeletextOutputConfig, TeletextOutputError, TeletextOutputGate, TeletextOutputSchedule,
};
pub use packets::{
    encode_teletext_page, encode_teletext_page_set, EncodedTeletextPage, EncodedTeletextPageSet,
};
pub use pmt_install::{TeletextPmtInstallError, TeletextPmtInstallReport, TeletextPmtInstaller};
pub use psi::{TeletextPsiDiscovery, TeletextPsiError};
pub use psi_monitor::TeletextPsiMonitor;
pub use signalling::{TeletextMultiplexError, TeletextPmtPlan, TeletextServiceDescriptor};
pub use transport::{TeletextPacketizer, TeletextTimestamp, TeletextTransportError};

/// Explicit decoder region; language codes never silently choose wire tables.
///
/// The initial encoder implements the Western Latin region (default region 0)
/// with its seven national subsets, an explicit X/28 designation and Level 1.5
/// X/26 extensions. Older receivers may still need a matching local region.
/// Other regions and G0/G2 designations require separate implementation and
/// decoder evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextEncodingRegion {
    /// Latin G0/G2, EN 300 706 table 32's first region group.
    WesternLatin,
}

/// Encoding failure with location, without retaining programme text.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TeletextEncodingError {
    /// The draft failed generic structural validation.
    #[error(transparent)]
    Structure(#[from] TeletextStructureError),
    /// This region's national-option entry is reserved.
    #[error("teletext page {address:?} has an unsupported national option")]
    NationalOption { address: TeletextPageAddress },
    /// A Unicode scalar cannot be represented by the selected tables.
    #[error("teletext page {address:?} row {row} column {column} cannot encode U+{codepoint:04X}")]
    Character {
        address: TeletextPageAddress,
        row: u8,
        column: u8,
        codepoint: u32,
    },
    /// Spacing attributes cannot overwrite an authored glyph or header field.
    #[error("teletext page {address:?} row {row} column {column} lacks spacing-control cells")]
    ControlCells {
        address: TeletextPageAddress,
        row: u8,
        column: u8,
    },
    /// Alpha/mosaic black is not a portable Level 1/1.5 foreground.
    #[error(
        "teletext page {address:?} row {row} column {column} uses unsupported black foreground"
    )]
    BlackForeground {
        address: TeletextPageAddress,
        row: u8,
        column: u8,
    },
    /// Caption/newsflash content would be hidden by page semantics.
    #[error(
        "teletext page {address:?} row {row} column {column} has unboxed or suppressed content"
    )]
    HiddenContent {
        address: TeletextPageAddress,
        row: u8,
        column: u8,
    },
    /// Header double height has no portable display interpretation.
    #[error("teletext page {address:?} has unsupported double-height header content")]
    HeaderHeight { address: TeletextPageAddress },
    /// EN 300 706 bounds the per-page X/26 packets to sixteen.
    #[error("teletext page {address:?} exceeds sixteen character enhancement packets")]
    Enhancements { address: TeletextPageAddress },
    /// The editorial link sentinel cannot express an exact subpage target.
    #[error("teletext page {address:?} links to an exact reserved wildcard subcode")]
    LinkSubcode { address: TeletextPageAddress },
}

#[cfg(test)]
mod buffer_tests;
#[cfg(test)]
mod clock_tests;
#[cfg(test)]
mod compositional_tests;
#[cfg(test)]
mod drcs_tests;
#[cfg(test)]
mod lift_tests;
#[cfg(test)]
mod mot_tests;
#[cfg(test)]
mod pmt_install_tests;
#[cfg(test)]
mod presentation_tests;
#[cfg(test)]
mod psi_monitor_tests;
#[cfg(test)]
mod psi_tests;
#[cfg(test)]
mod signalling_runtime_tests;
#[cfg(test)]
mod signalling_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod transport_tests;
