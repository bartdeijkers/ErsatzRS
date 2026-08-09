//! Dolby Vision input policy (ADR 0064, milestone `p8a-02`).
//!
//! ADR 0064 is explicit that Dolby Vision is an *input* policy first, and that
//! support is introduced from characterized fixtures rather than inferred from
//! codec names. This module therefore encodes what the standard guarantees
//! about each profile's base layer and what ErsatzRS has actually proven, and
//! it defaults to visible rejection instead of plausible-but-wrong colour.
//!
//! Dolby Vision **output** is not supported and is not decided here.

/// Dolby Vision configuration record, as `ffprobe` reports it in the
/// `DOVI configuration record` stream side data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DolbyVisionConfiguration {
    /// `dv_profile`: 5, 7, 8, and so on.
    pub profile: u8,
    /// `dv_bl_signal_compatibility_id`: which conventional signal, if any, the
    /// base layer is compatible with. 1 means HDR10, 4 means HLG, 0 means none.
    pub bl_signal_compatibility_id: u8,
    /// `rpu_present_flag`: the reference processing unit metadata is present.
    pub rpu_present: bool,
    /// `el_present_flag`: an enhancement layer is present (profile 7 dual
    /// layer).
    pub el_present: bool,
    /// `bl_present_flag`: the base layer is present.
    pub bl_present: bool,
}

/// Positive Dolby Vision detection whose provider metadata is insufficient for
/// the strict profile policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncompleteDolbyVisionMetadata {
    /// Provider-neutral reason surfaced by playback and health checks.
    pub reason: String,
}

/// What ErsatzRS may do with a Dolby Vision source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DolbyVisionHandling {
    /// Profile 5 requires RPU-aware libplacebo conversion. Admission is
    /// output- and runtime-operation-specific and is checked at the command
    /// boundary.
    LibplaceboRequired,
    /// The base layer is a conforming HDR10 signal, so it can be treated as
    /// HDR10 and the Dolby Vision metadata dropped.
    ///
    /// The muxed output no longer satisfies Dolby Vision, so ADR 0064 requires
    /// that its signalling be removed rather than retained.
    Hdr10CompatibleBaseLayer,
    /// Profile 7 carries an HDR10 base layer interleaved with its enhancement
    /// layer. The characterized production path decodes that base layer
    /// directly in software; `dovi_split` remains a fixture oracle, not a
    /// runtime dependency.
    Hdr10BaseLayerDirectDecode,
    /// The persisted tuple or selected runtime operation is outside the exact
    /// characterized contract. Reject visibly.
    Unproven { reason: &'static str },
}

impl DolbyVisionConfiguration {
    /// Decide how this source may be handled.
    ///
    /// The rules follow the Dolby Vision profile definitions rather than a
    /// guess:
    ///
    /// * Only profile 8.1 has been fixture-proven: HDR10-compatible BL, RPU
    ///   present, and no enhancement layer.
    /// * Profile 7 is dual layer. A complete BL+EL+RPU record is eligible for
    ///   the fixture-proven direct software base-layer decode path; HEVC Main10
    ///   input is checked separately at the command boundary.
    /// * Profile 5 uses IPT-PQ-C2 colour with no conventional fallback at all.
    ///   Decoding it as if it were BT.2020 produces visibly wrong colour, which
    ///   is exactly the outcome ADR 0064 forbids.
    #[must_use]
    pub const fn handling(&self) -> DolbyVisionHandling {
        match self.profile {
            5 if self.bl_present
                && self.rpu_present
                && !self.el_present
                && self.bl_signal_compatibility_id == 0 =>
            {
                DolbyVisionHandling::LibplaceboRequired
            }
            5 => DolbyVisionHandling::Unproven {
                reason:
                    "Dolby Vision profile 5 has an incomplete or malformed configuration record",
            },
            8 if !self.bl_present => DolbyVisionHandling::Unproven {
                reason: "Dolby Vision profile 8 has no base layer",
            },
            8 if !self.rpu_present || self.el_present => DolbyVisionHandling::Unproven {
                reason: "Dolby Vision profile 8 is not an exact complete single-layer record",
            },
            8 if self.bl_signal_compatibility_id == 1 => {
                DolbyVisionHandling::Hdr10CompatibleBaseLayer
            }
            8 if self.bl_signal_compatibility_id == 4 => DolbyVisionHandling::Unproven {
                reason: "Dolby Vision profile 8.4 HLG playback has not been fixture-proven",
            },
            8 => DolbyVisionHandling::Unproven {
                reason: "Dolby Vision profile 8 has no conventional base-layer compatibility",
            },
            7 if self.bl_present
                && self.el_present
                && self.rpu_present
                && self.bl_signal_compatibility_id == 6 =>
            {
                DolbyVisionHandling::Hdr10BaseLayerDirectDecode
            }
            7 => DolbyVisionHandling::Unproven {
                reason: "Dolby Vision profile 7 has an incomplete or uncharacterized configuration record",
            },
            _ => DolbyVisionHandling::Unproven {
                reason: "unrecognised Dolby Vision profile",
            },
        }
    }

    /// True when playback may proceed by decoding the base layer.
    #[must_use]
    pub const fn is_playable(&self) -> bool {
        matches!(
            self.handling(),
            DolbyVisionHandling::Hdr10CompatibleBaseLayer
                | DolbyVisionHandling::Hdr10BaseLayerDirectDecode
        )
    }

    /// Operator-facing explanation for health and troubleshooting.
    #[must_use]
    pub fn evidence(&self) -> String {
        match self.handling() {
            DolbyVisionHandling::LibplaceboRequired => {
                "Dolby Vision profile 5 requires a positively proven RPU-aware libplacebo operation"
                    .to_owned()
            }
            DolbyVisionHandling::Hdr10CompatibleBaseLayer => format!(
                "Dolby Vision profile {} plays from its HDR10-compatible base layer; Dolby Vision signalling is removed",
                self.profile
            ),
            DolbyVisionHandling::Hdr10BaseLayerDirectDecode => format!(
                "Dolby Vision profile {} plays through the fixture-proven direct software decode of its HDR10 base layer; Dolby Vision signalling is removed",
                self.profile
            ),
            DolbyVisionHandling::Unproven { reason } => reason.to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn configuration(profile: u8, compatibility: u8) -> DolbyVisionConfiguration {
        DolbyVisionConfiguration {
            profile,
            bl_signal_compatibility_id: compatibility,
            rpu_present: true,
            el_present: false,
            bl_present: true,
        }
    }

    #[test]
    fn profile_8_1_plays_from_its_hdr10_base_layer() {
        let config = configuration(8, 1);

        assert_eq!(
            config.handling(),
            DolbyVisionHandling::Hdr10CompatibleBaseLayer
        );
        assert!(config.is_playable());
        assert!(config.evidence().contains("HDR10-compatible base layer"));
    }

    #[test]
    fn profile_8_4_is_rejected_until_fixture_proven() {
        assert!(matches!(
            configuration(8, 4).handling(),
            DolbyVisionHandling::Unproven { .. }
        ));
    }

    #[test]
    fn profile_5_is_rejected_rather_than_played_with_wrong_colour() {
        // ADR 0064: profile 5 has no assumed HDR10 fallback, and playing it as
        // if it did produces plausible but incorrect colour.
        let config = configuration(5, 0);

        assert!(!config.is_playable());
        assert!(
            config.evidence().contains("libplacebo"),
            "{}",
            config.evidence()
        );
    }

    #[test]
    fn complete_profile_7_record_uses_direct_hdr10_base_layer_decode() {
        let mut config = configuration(7, 6);
        config.el_present = true;

        assert_eq!(
            config.handling(),
            DolbyVisionHandling::Hdr10BaseLayerDirectDecode
        );
        assert!(config.is_playable());
        assert!(config.evidence().contains("direct software decode"));
    }

    #[test]
    fn incomplete_profile_7_record_stays_unproven() {
        let config = configuration(7, 6);

        assert!(!config.is_playable());
        assert!(config.evidence().contains("uncharacterized"));
    }

    #[test]
    fn profile_8_without_a_compatibility_id_is_not_assumed_playable() {
        // Compatibility id 0 means "no conventional signal", so the base layer
        // carries no standard guarantee even though the profile usually does.
        assert!(!configuration(8, 0).is_playable());
    }

    #[test]
    fn profile_8_without_a_base_layer_is_not_assumed_playable() {
        let mut config = configuration(8, 1);
        config.bl_present = false;

        assert!(!config.is_playable());
        assert!(config.evidence().contains("no base layer"));
    }

    #[test]
    fn an_unrecognised_profile_is_rejected() {
        assert!(!configuration(9, 1).is_playable());
        assert!(!configuration(4, 1).is_playable());
    }
}
