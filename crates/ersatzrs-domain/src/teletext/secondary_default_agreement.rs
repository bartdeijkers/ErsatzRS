//! Explicit author/receiver agreement, distinct from original declarations.
use super::TeletextPageCharsetDesignation;

/// Accepted application agreement from ADR 0123, not a universal decoder default.
pub const TELETEXT_SECONDARY_DEFAULT_AGREEMENT_V1: &str = "ersatzrs-secondary-default-v1";

/// A required explicit choice: missing/null data cannot mean no second G0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TeletextSecondaryDefaultChoice {
    // A struct variant enforces closed fields; serde unit variants ignore extras.
    NoSecondG0 {},
    /// Exact assigned Table 33 G0/national designation, without alias rewriting.
    Designated {
        designation: TeletextPageCharsetDesignation,
    },
}

/// Versioned intent scoped to one producer's originally Header/POP displays.
/// Explicit X28/M29 declarations retain precedence. This value grants neither
/// author consent, persisted revision authority nor receiver/publication proof.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "policy", deny_unknown_fields)]
pub enum TeletextSecondaryDefaultAgreement {
    #[serde(rename = "ersatzrs-secondary-default-v1")]
    V1 {
        choice: TeletextSecondaryDefaultChoice,
    },
}
impl TeletextSecondaryDefaultAgreement {
    #[must_use]
    pub const fn new(choice: TeletextSecondaryDefaultChoice) -> Self {
        Self::V1 { choice }
    }

    #[must_use]
    pub const fn policy_id(self) -> &'static str {
        TELETEXT_SECONDARY_DEFAULT_AGREEMENT_V1
    }

    /// Resolve only the second G0, never the primary G0/G2 or another attribute.
    #[must_use]
    pub const fn secondary(self) -> Option<TeletextPageCharsetDesignation> {
        match self {
            Self::V1 {
                choice: TeletextSecondaryDefaultChoice::NoSecondG0 {},
            } => None,
            Self::V1 {
                choice: TeletextSecondaryDefaultChoice::Designated { designation },
            } => Some(designation),
        }
    }
}

#[cfg(test)]
#[path = "secondary_default_agreement_tests.rs"]
mod tests;
