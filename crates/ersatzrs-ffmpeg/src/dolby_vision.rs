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

/// What ErsatzRS may do with a Dolby Vision source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DolbyVisionHandling {
    /// The base layer is a conforming HDR10 signal, so it can be treated as
    /// HDR10 and the Dolby Vision metadata dropped.
    ///
    /// The muxed output no longer satisfies Dolby Vision, so ADR 0064 requires
    /// that its signalling be removed rather than retained.
    Hdr10CompatibleBaseLayer,
    /// The base layer is a conforming HLG signal; same reasoning.
    HlgCompatibleBaseLayer,
    /// No conventional-signal fallback is guaranteed, and no fixture has proven
    /// the conversion produces correct colour. Reject visibly.
    Unproven { reason: &'static str },
}

impl DolbyVisionConfiguration {
    /// Decide how this source may be handled.
    ///
    /// The rules follow the Dolby Vision profile definitions rather than a
    /// guess:
    ///
    /// * Profile 8 declares its base-layer compatibility explicitly, so a
    ///   compatibility id of 1 (HDR10) or 4 (HLG) is a standard guarantee that
    ///   the base layer decodes correctly on its own.
    /// * Profile 7 is dual layer. Its base layer is HDR10, but only after the
    ///   enhancement layer is separated; that requires `dovi_split` and a
    ///   fixture, so it stays unproven here.
    /// * Profile 5 uses IPT-PQ-C2 colour with no conventional fallback at all.
    ///   Decoding it as if it were BT.2020 produces visibly wrong colour, which
    ///   is exactly the outcome ADR 0064 forbids.
    #[must_use]
    pub const fn handling(&self) -> DolbyVisionHandling {
        match self.profile {
            8 => match self.bl_signal_compatibility_id {
                1 => DolbyVisionHandling::Hdr10CompatibleBaseLayer,
                4 => DolbyVisionHandling::HlgCompatibleBaseLayer,
                _ => DolbyVisionHandling::Unproven {
                    reason: "Dolby Vision profile 8 with no conventional base-layer compatibility",
                },
            },
            7 => DolbyVisionHandling::Unproven {
                reason: "Dolby Vision profile 7 needs dual-layer separation, which is unproven",
            },
            5 => DolbyVisionHandling::Unproven {
                reason:
                    "Dolby Vision profile 5 has no HDR10 fallback and would play with wrong colour",
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
                | DolbyVisionHandling::HlgCompatibleBaseLayer
        )
    }

    /// Operator-facing explanation for health and troubleshooting.
    #[must_use]
    pub fn evidence(&self) -> String {
        match self.handling() {
            DolbyVisionHandling::Hdr10CompatibleBaseLayer => format!(
                "Dolby Vision profile {} plays from its HDR10-compatible base layer; Dolby Vision signalling is removed",
                self.profile
            ),
            DolbyVisionHandling::HlgCompatibleBaseLayer => format!(
                "Dolby Vision profile {} plays from its HLG-compatible base layer; Dolby Vision signalling is removed",
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
    fn profile_8_4_plays_from_its_hlg_base_layer() {
        assert_eq!(
            configuration(8, 4).handling(),
            DolbyVisionHandling::HlgCompatibleBaseLayer
        );
    }

    #[test]
    fn profile_5_is_rejected_rather_than_played_with_wrong_colour() {
        // ADR 0064: profile 5 has no assumed HDR10 fallback, and playing it as
        // if it did produces plausible but incorrect colour.
        let config = configuration(5, 0);

        assert!(!config.is_playable());
        assert!(
            config.evidence().contains("no HDR10 fallback"),
            "{}",
            config.evidence()
        );
    }

    #[test]
    fn profile_7_stays_unproven_until_dual_layer_separation_is_characterized() {
        // `dovi_split` is a profile-7 multilayer tool, not a general decoder,
        // and no fixture has proven the separated base layer yet.
        let config = configuration(7, 0);

        assert!(!config.is_playable());
        assert!(
            config.evidence().contains("dual-layer"),
            "{}",
            config.evidence()
        );
    }

    #[test]
    fn profile_8_without_a_compatibility_id_is_not_assumed_playable() {
        // Compatibility id 0 means "no conventional signal", so the base layer
        // carries no standard guarantee even though the profile usually does.
        assert!(!configuration(8, 0).is_playable());
    }

    #[test]
    fn an_unrecognised_profile_is_rejected() {
        assert!(!configuration(9, 1).is_playable());
        assert!(!configuration(4, 1).is_playable());
    }
}
