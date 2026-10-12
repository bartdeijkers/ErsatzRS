//! Explicit opt-in host policy for selected-original recovery and direct output.
//!
//! This parser performs no storage access, admission, publication or capability
//! assessment. Absence is handled by the caller as ordinary raw playback. Every
//! numeric limit is supplied by the operator; there are no compiler defaults.

use std::time::{Duration, Instant};

use ersatzrs_app::{
    teletext_channel_documents::TeletextChannelDocumentInputBudget,
    teletext_design_documents::TeletextDesignDocumentBudget,
    teletext_profile_documents::TeletextProfileDocumentBudget,
};
use ersatzrs_domain::{
    TeletextDeclaredServiceBudget, TeletextEnhancementCollectionBudget,
    TeletextObjectCatalogBudget, TeletextProducerCompositionBudget, TeletextServiceBudget,
    TeletextSpatialTraceBudget,
};
use ersatzrs_ffmpeg::teletext::{
    TeletextAugmentedGlyphComparisonBudget, TeletextAugmentedInvocationBudget,
    TeletextAugmentedOriginProgramBudget, TeletextAuthoredServiceEncodingBudget,
    TeletextBaseCompatibilityRealizationBudget, TeletextEncodingRegion,
    TeletextMainCompositionBudget, TeletextNormalizedMainProgramBudget,
    TeletextOriginCarrierBudget, TeletextOriginCarrierPlacementBudget,
    TeletextPhysicalComparisonBudget,
};
use serde::Deserialize;

use super::teletext_channel_documents::TeletextChannelDocumentEncodingPolicy;
use crate::transport::iptv::teletext_source_policy::{
    TeletextSourcePolicy, TeletextSourcePolicyLimits,
};

/// Explicit optional `ConfigElement` JSON; absence keeps ordinary raw output.
pub(crate) const NATIVE_TELETEXT_RUNTIME_POLICY_SETTING: &str = "teletext.output.policy";

const MAX_POLICY_BYTES: usize = 65_536;

pub(crate) struct NativeTeletextRuntimePolicy {
    pub(crate) design: TeletextDesignDocumentBudget,
    pub(crate) encoding: TeletextChannelDocumentEncodingPolicy,
    pub(crate) source: TeletextSourcePolicy,
    pub(crate) recovery_timeout: Duration,
}

/// Static errors deliberately exclude raw configuration or Serde payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum TeletextRuntimePolicyError {
    #[error("teletext output policy exceeds its configuration byte limit")]
    TooLarge,
    #[error("teletext output policy does not match its required schema")]
    InvalidJson,
    #[error("teletext output policy version is unsupported")]
    UnsupportedVersion,
    #[error("teletext output recovery timeout must be positive and representable")]
    InvalidRecoveryTimeout,
    #[error("teletext output source policy is invalid")]
    InvalidSourcePolicy,
}

/// Parse only a present explicit policy; missing configuration is not an error
/// here because the ordinary stream owner decides whether to opt in at all.
pub(crate) fn parse_native_teletext_runtime_policy(
    raw: &str,
) -> Result<NativeTeletextRuntimePolicy, TeletextRuntimePolicyError> {
    if raw.len() > MAX_POLICY_BYTES {
        return Err(TeletextRuntimePolicyError::TooLarge);
    }
    let dto: RuntimePolicy =
        serde_json::from_str(raw).map_err(|_| TeletextRuntimePolicyError::InvalidJson)?;
    if dto.version != 1 {
        return Err(TeletextRuntimePolicyError::UnsupportedVersion);
    }
    let recovery_timeout = Duration::from_millis(dto.recovery_timeout_ms);
    if recovery_timeout.is_zero() || Instant::now().checked_add(recovery_timeout).is_none() {
        return Err(TeletextRuntimePolicyError::InvalidRecoveryTimeout);
    }
    let source = TeletextSourcePolicy::new(dto.source.into())
        .map_err(|_| TeletextRuntimePolicyError::InvalidSourcePolicy)?;
    let validation = dto.validation.into();
    Ok(NativeTeletextRuntimePolicy {
        design: dto.design.with_validation(validation),
        encoding: TeletextChannelDocumentEncodingPolicy {
            input: dto.input.into(),
            validation,
            // Recovery supplies the exact recorded V1 region independently;
            // this required factory field is never inferred from JSON/content.
            v1_region: TeletextEncodingRegion::WesternLatin,
            composition: dto.composition.into(),
            encoding: dto.encoding.into(),
            // Saved entry and lifecycle records replace these placeholders in
            // reload_original_teletext_design_revision_with_hold before encoding.
            transmissions: Vec::new(),
            entry: None,
        },
        source,
        recovery_timeout,
    })
}

// usize deserialization rejects values not representable on the current target,
// including supported 32-bit native targets. No casts or architecture guesses.
// All DTO fields are mandatory; null, missing, duplicate and unknown fields fail.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimePolicy {
    version: u32,
    recovery_timeout_ms: u64,
    design: DesignLimits,
    input: InputLimits,
    validation: ValidationLimits,
    composition: ProducerCompositionLimits,
    encoding: EncodingLimits,
    source: SourceLimits,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DesignLimits {
    #[serde(rename = "max_serialized_bytes")]
    serialized_bytes: usize,
    #[serde(rename = "max_documents")]
    documents: usize,
    #[serde(rename = "max_response_bytes")]
    response_bytes: usize,
    #[serde(rename = "max_context_bytes")]
    context_bytes: usize,
    #[serde(rename = "max_context_field_bytes")]
    context_field_bytes: usize,
    #[serde(rename = "max_resource_transmissions")]
    resource_transmissions: usize,
}
impl DesignLimits {
    fn with_validation(
        self,
        validation: TeletextProfileDocumentBudget,
    ) -> TeletextDesignDocumentBudget {
        TeletextDesignDocumentBudget {
            max_serialized_bytes: self.serialized_bytes,
            max_documents: self.documents,
            max_response_bytes: self.response_bytes,
            max_context_bytes: self.context_bytes,
            max_context_field_bytes: self.context_field_bytes,
            max_resource_transmissions: self.resource_transmissions,
            validation,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ServiceLimits {
    #[serde(rename = "max_records")]
    records: usize,
    #[serde(rename = "max_objects")]
    objects: usize,
    #[serde(rename = "max_structure_items")]
    structure_items: usize,
    #[serde(rename = "max_base_content_bytes")]
    base_content_bytes: usize,
    #[serde(rename = "max_commands")]
    commands: usize,
    #[serde(rename = "max_collected_commands")]
    collected_commands: usize,
    #[serde(rename = "max_collected_drcs_references")]
    collected_drcs_references: usize,
}

impl From<ServiceLimits> for TeletextServiceBudget {
    fn from(value: ServiceLimits) -> Self {
        Self {
            max_records: value.records,
            max_objects: value.objects,
            max_structure_items: value.structure_items,
            max_base_content_bytes: value.base_content_bytes,
            max_commands: value.commands,
            max_collected_commands: value.collected_commands,
            max_collected_drcs_references: value.collected_drcs_references,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DeclarationLimits {
    #[serde(rename = "max_mots")]
    mots: usize,
    #[serde(rename = "max_displays")]
    displays: usize,
    #[serde(rename = "max_resources")]
    resources: usize,
    #[serde(rename = "max_defaults")]
    defaults: usize,
    #[serde(rename = "max_objects")]
    objects: usize,
}

impl From<DeclarationLimits> for TeletextDeclaredServiceBudget {
    fn from(value: DeclarationLimits) -> Self {
        Self {
            max_mots: value.mots,
            max_displays: value.displays,
            max_resources: value.resources,
            max_defaults: value.defaults,
            max_objects: value.objects,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InputLimits {
    max_producers: usize,
    max_response_bytes: usize,
}

impl From<InputLimits> for TeletextChannelDocumentInputBudget {
    fn from(value: InputLimits) -> Self {
        Self {
            max_producers: value.max_producers,
            max_response_bytes: value.max_response_bytes,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogLimits {
    max_objects: usize,
    max_commands: usize,
}

impl From<CatalogLimits> for TeletextObjectCatalogBudget {
    fn from(value: CatalogLimits) -> Self {
        Self {
            max_objects: value.max_objects,
            max_commands: value.max_commands,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CollectionLimits {
    max_commands: usize,
    max_drcs_references: usize,
}

impl From<CollectionLimits> for TeletextEnhancementCollectionBudget {
    fn from(value: CollectionLimits) -> Self {
        Self {
            max_commands: value.max_commands,
            max_drcs_references: value.max_drcs_references,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SpatialLimits {
    max_events: usize,
    max_invocations: usize,
}

impl From<SpatialLimits> for TeletextSpatialTraceBudget {
    fn from(value: SpatialLimits) -> Self {
        Self {
            max_events: value.max_events,
            max_invocations: value.max_invocations,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RealizationLimits {
    #[serde(rename = "max_commands")]
    commands: usize,
    #[serde(rename = "max_candidates")]
    candidates: usize,
    #[serde(rename = "max_origins")]
    origins: usize,
    #[serde(rename = "max_seeds")]
    seeds: usize,
    #[serde(rename = "max_edges")]
    edges: usize,
    #[serde(rename = "max_updates")]
    updates: usize,
}

impl From<RealizationLimits> for TeletextBaseCompatibilityRealizationBudget {
    fn from(value: RealizationLimits) -> Self {
        Self {
            max_commands: value.commands,
            max_candidates: value.candidates,
            max_origins: value.origins,
            max_seeds: value.seeds,
            max_edges: value.edges,
            max_updates: value.updates,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AncestryLimits {
    #[serde(rename = "max_original_invocations")]
    original_invocations: usize,
    #[serde(rename = "max_actual_invocations")]
    actual_invocations: usize,
    #[serde(rename = "max_carriers")]
    carriers: usize,
    #[serde(rename = "max_work")]
    work: usize,
}

impl From<AncestryLimits> for TeletextAugmentedInvocationBudget {
    fn from(value: AncestryLimits) -> Self {
        Self {
            max_original_invocations: value.original_invocations,
            max_actual_invocations: value.actual_invocations,
            max_carriers: value.carriers,
            max_work: value.work,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GlyphComparisonLimits {
    #[serde(rename = "max_records")]
    records: usize,
    #[serde(rename = "max_cells")]
    cells: usize,
    #[serde(rename = "max_diagnostics")]
    diagnostics: usize,
    #[serde(rename = "max_work")]
    work: usize,
}

impl From<GlyphComparisonLimits> for TeletextAugmentedGlyphComparisonBudget {
    fn from(value: GlyphComparisonLimits) -> Self {
        Self {
            max_records: value.records,
            max_cells: value.cells,
            max_diagnostics: value.diagnostics,
            max_work: value.work,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DisplayComparisonLimits {
    #[serde(rename = "max_records")]
    records: usize,
    #[serde(rename = "max_cells")]
    cells: usize,
    #[serde(rename = "max_diagnostics")]
    diagnostics: usize,
    #[serde(rename = "max_updates")]
    updates: usize,
}

impl From<DisplayComparisonLimits> for TeletextPhysicalComparisonBudget {
    fn from(value: DisplayComparisonLimits) -> Self {
        Self {
            max_records: value.records,
            max_cells: value.cells,
            max_diagnostics: value.diagnostics,
            max_updates: value.updates,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ValidationLimits {
    service: ServiceLimits,
    declarations: DeclarationLimits,
}

impl From<ValidationLimits> for TeletextProfileDocumentBudget {
    fn from(value: ValidationLimits) -> Self {
        Self {
            service: value.service.into(),
            declarations: value.declarations.into(),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProducerCompositionLimits {
    max_producers: usize,
    max_input_items: usize,
    max_input_content_bytes: usize,
    max_input_commands: usize,
    max_storage_bytes: usize,
    max_work: usize,
    service: ServiceLimits,
    declarations: DeclarationLimits,
}

impl From<ProducerCompositionLimits> for TeletextProducerCompositionBudget {
    fn from(value: ProducerCompositionLimits) -> Self {
        Self {
            max_producers: value.max_producers,
            max_input_items: value.max_input_items,
            max_input_content_bytes: value.max_input_content_bytes,
            max_input_commands: value.max_input_commands,
            max_storage_bytes: value.max_storage_bytes,
            max_work: value.max_work,
            service: value.service.into(),
            declarations: value.declarations.into(),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MainCompositionLimits {
    max_input_commands: usize,
    max_output_commands: usize,
    collection: CollectionLimits,
}

impl From<MainCompositionLimits> for TeletextMainCompositionBudget {
    fn from(value: MainCompositionLimits) -> Self {
        Self {
            max_input_commands: value.max_input_commands,
            max_output_commands: value.max_output_commands,
            collection: value.collection.into(),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlacementLimits {
    max_commands: usize,
    max_origins: usize,
    max_edges: usize,
    max_work: usize,
    catalog: CatalogLimits,
    spatial: SpatialLimits,
}

impl From<PlacementLimits> for TeletextOriginCarrierPlacementBudget {
    fn from(value: PlacementLimits) -> Self {
        Self {
            max_commands: value.max_commands,
            max_origins: value.max_origins,
            max_edges: value.max_edges,
            max_work: value.max_work,
            catalog: value.catalog.into(),
            spatial: value.spatial.into(),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecipeLimits {
    max_candidates: usize,
    max_origins: usize,
    max_id_checks: usize,
    max_output_commands: usize,
    catalog: CatalogLimits,
}

impl From<RecipeLimits> for TeletextOriginCarrierBudget {
    fn from(value: RecipeLimits) -> Self {
        Self {
            max_candidates: value.max_candidates,
            max_origins: value.max_origins,
            max_id_checks: value.max_id_checks,
            max_output_commands: value.max_output_commands,
            catalog: value.catalog.into(),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AssemblyLimits {
    max_input_commands: usize,
    max_output_commands: usize,
    max_carriers: usize,
    max_identity_checks: usize,
    max_work: usize,
    recipe: RecipeLimits,
}

impl From<AssemblyLimits> for TeletextAugmentedOriginProgramBudget {
    fn from(value: AssemblyLimits) -> Self {
        Self {
            max_input_commands: value.max_input_commands,
            max_output_commands: value.max_output_commands,
            max_carriers: value.max_carriers,
            max_identity_checks: value.max_identity_checks,
            max_work: value.max_work,
            recipe: value.recipe.into(),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NormalizationLimits {
    max_resource_objects: usize,
    composition: MainCompositionLimits,
    realization: RealizationLimits,
    placement: PlacementLimits,
    assembly: AssemblyLimits,
    packing: CatalogLimits,
    replay: CollectionLimits,
    ancestry: AncestryLimits,
    glyph_comparison: GlyphComparisonLimits,
    display_comparison: DisplayComparisonLimits,
}

impl From<NormalizationLimits> for TeletextNormalizedMainProgramBudget {
    fn from(value: NormalizationLimits) -> Self {
        Self {
            max_resource_objects: value.max_resource_objects,
            composition: value.composition.into(),
            realization: value.realization.into(),
            placement: value.placement.into(),
            assembly: value.assembly.into(),
            packing: value.packing.into(),
            replay: value.replay.into(),
            ancestry: value.ancestry.into(),
            glyph_comparison: value.glyph_comparison.into(),
            display_comparison: value.display_comparison.into(),
        }
    }
}

// Required integers become enabled credits; zero is exhausted, never disabled.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EncodingLimits {
    max_pages: usize,
    max_units: usize,
    max_commands: usize,
    max_display_updates: usize,
    max_glyph_comparison_work: usize,
    max_display_comparison_updates: usize,
    max_realization_updates: usize,
    max_placement_work: usize,
    max_assembly_work: usize,
    max_ancestry_work: usize,
    max_collected_commands: usize,
    max_collected_drcs_references: usize,
    max_spatial_event_visits: usize,
    max_spatial_invocation_visits: usize,
    max_identity_checks: usize,
    max_attribute_cell_updates: usize,
    normalization: NormalizationLimits,
}

impl From<EncodingLimits> for TeletextAuthoredServiceEncodingBudget {
    fn from(value: EncodingLimits) -> Self {
        Self {
            max_pages: value.max_pages,
            max_units: value.max_units,
            max_commands: value.max_commands,
            max_display_updates: Some(value.max_display_updates),
            max_glyph_comparison_work: Some(value.max_glyph_comparison_work),
            max_display_comparison_updates: Some(value.max_display_comparison_updates),
            max_realization_updates: Some(value.max_realization_updates),
            max_placement_work: Some(value.max_placement_work),
            max_assembly_work: Some(value.max_assembly_work),
            max_ancestry_work: Some(value.max_ancestry_work),
            max_collected_commands: Some(value.max_collected_commands),
            max_collected_drcs_references: Some(value.max_collected_drcs_references),
            max_spatial_event_visits: Some(value.max_spatial_event_visits),
            max_spatial_invocation_visits: Some(value.max_spatial_invocation_visits),
            max_identity_checks: Some(value.max_identity_checks),
            max_attribute_cell_updates: Some(value.max_attribute_cell_updates),
            normalization: value.normalization.into(),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceLimits {
    elementary_pid: u16,
    rate_tolerance_ppm: u32,
    pcr_jitter_ticks: u64,
    pcr_gap_slack_ticks: u64,
    pcr_gap_slack_packets: u64,
    min_pcr_intervals: u32,
    max_window_packets: usize,
    max_buffer_packets: usize,
    probe_maximum_bytes: usize,
    startup_maximum_bytes: usize,
    max_hold_ms: u64,
    playout_delay_ms: u64,
    max_lateness_ms: u64,
    first_byte_timeout_ms: u64,
    probe_maximum_time_ms: u64,
}

impl From<SourceLimits> for TeletextSourcePolicyLimits {
    fn from(value: SourceLimits) -> Self {
        Self {
            elementary_pid: value.elementary_pid,
            rate_tolerance_ppm: value.rate_tolerance_ppm,
            pcr_jitter_ticks: value.pcr_jitter_ticks,
            pcr_gap_slack_ticks: value.pcr_gap_slack_ticks,
            pcr_gap_slack_packets: value.pcr_gap_slack_packets,
            min_pcr_intervals: value.min_pcr_intervals,
            max_window_packets: value.max_window_packets,
            max_buffer_packets: value.max_buffer_packets,
            probe_maximum_bytes: value.probe_maximum_bytes,
            startup_maximum_bytes: value.startup_maximum_bytes,
            max_hold: Duration::from_millis(value.max_hold_ms),
            playout_delay: Duration::from_millis(value.playout_delay_ms),
            max_lateness: Duration::from_millis(value.max_lateness_ms),
            first_byte_timeout: Duration::from_millis(value.first_byte_timeout_ms),
            probe_maximum_time: Duration::from_millis(value.probe_maximum_time_ms),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    // This explicit synthetic-test policy is not a product/default envelope.
    const FIXTURE: &str = include_str!("../../tests/fixtures/teletext-output-policy-v1.json");
    const CREDIT_KEYS: [&str; 13] = [
        "max_display_updates",
        "max_glyph_comparison_work",
        "max_display_comparison_updates",
        "max_realization_updates",
        "max_placement_work",
        "max_assembly_work",
        "max_ancestry_work",
        "max_collected_commands",
        "max_collected_drcs_references",
        "max_spatial_event_visits",
        "max_spatial_invocation_visits",
        "max_identity_checks",
        "max_attribute_cell_updates",
    ];

    fn fixture() -> Value {
        serde_json::from_str(FIXTURE).unwrap()
    }

    fn parse(value: &Value) -> NativeTeletextRuntimePolicy {
        parse_native_teletext_runtime_policy(&serde_json::to_string(value).unwrap()).unwrap()
    }

    fn assert_error(value: &Value, expected: TeletextRuntimePolicyError) {
        match parse_native_teletext_runtime_policy(&serde_json::to_string(value).unwrap()) {
            Err(actual) => assert_eq!(actual, expected),
            Ok(_) => panic!("invalid explicit policy was accepted"),
        }
    }

    fn credits(value: &NativeTeletextRuntimePolicy) -> [Option<usize>; 13] {
        let budget = value.encoding.encoding;
        [
            budget.max_display_updates,
            budget.max_glyph_comparison_work,
            budget.max_display_comparison_updates,
            budget.max_realization_updates,
            budget.max_placement_work,
            budget.max_assembly_work,
            budget.max_ancestry_work,
            budget.max_collected_commands,
            budget.max_collected_drcs_references,
            budget.max_spatial_event_visits,
            budget.max_spatial_invocation_visits,
            budget.max_identity_checks,
            budget.max_attribute_cell_updates,
        ]
    }

    fn assert_storage_and_model_limits(value: &NativeTeletextRuntimePolicy) {
        let design = value.design;
        assert_eq!(
            [
                design.max_serialized_bytes,
                design.max_documents,
                design.max_response_bytes,
                design.max_context_bytes,
                design.max_context_field_bytes,
                design.max_resource_transmissions
            ],
            [262_144, 8, 65_536, 65_536, 4096, 16]
        );
        let input = value.encoding.input;
        assert_eq!([input.max_producers, input.max_response_bytes], [8, 32_768]);
        let validation = value.encoding.validation;
        assert_eq!(design.validation.service, validation.service);
        assert_eq!(design.validation.declarations, validation.declarations);
        assert_eq!(
            validation.service,
            TeletextServiceBudget {
                max_records: 16,
                max_objects: 16,
                max_structure_items: 4096,
                max_base_content_bytes: 8192,
                max_commands: 8192,
                max_collected_commands: 16_384,
                max_collected_drcs_references: 16,
            }
        );
        assert_eq!(
            validation.declarations,
            TeletextDeclaredServiceBudget {
                max_mots: 4,
                max_displays: 8,
                max_resources: 16,
                max_defaults: 16,
                max_objects: 16,
            }
        );
        let composition = value.encoding.composition;
        assert_eq!(
            [
                composition.max_producers,
                composition.max_input_items,
                composition.max_input_content_bytes,
                composition.max_input_commands,
                composition.max_storage_bytes,
                composition.max_work
            ],
            [8, 16_384, 65_536, 16_384, 16_777_216, 1_000_000]
        );
        assert_eq!(composition.service, validation.service);
        assert_eq!(composition.declarations, validation.declarations);
        assert!(value.encoding.entry.is_none());
        assert!(value.encoding.transmissions.is_empty());
        assert_eq!(
            value.encoding.v1_region,
            TeletextEncodingRegion::WesternLatin
        );
    }

    fn assert_normalization_limits(value: &NativeTeletextRuntimePolicy) {
        let stage = value.encoding.encoding.normalization;
        let catalog = TeletextObjectCatalogBudget {
            max_objects: 64,
            max_commands: 8192,
        };
        let collection = TeletextEnhancementCollectionBudget {
            max_commands: 8192,
            max_drcs_references: 16,
        };
        assert_eq!(
            stage.composition,
            TeletextMainCompositionBudget {
                max_input_commands: 4096,
                max_output_commands: 4096,
                collection,
            }
        );
        assert_eq!(
            stage.realization,
            TeletextBaseCompatibilityRealizationBudget {
                max_commands: 4096,
                max_candidates: 4096,
                max_origins: 8192,
                max_seeds: 64,
                max_edges: 4096,
                max_updates: 100_000,
            }
        );
        assert_eq!(
            stage.placement,
            TeletextOriginCarrierPlacementBudget {
                max_commands: 4096,
                max_origins: 8192,
                max_edges: 4096,
                catalog,
                spatial: TeletextSpatialTraceBudget {
                    max_events: 8192,
                    max_invocations: 512
                },
                max_work: 100_000,
            }
        );
        assert_eq!(
            stage.assembly,
            TeletextAugmentedOriginProgramBudget {
                max_input_commands: 4096,
                max_output_commands: 4096,
                max_carriers: 64,
                max_identity_checks: 100_000,
                max_work: 100_000,
                recipe: TeletextOriginCarrierBudget {
                    max_candidates: 4096,
                    max_origins: 8192,
                    catalog,
                    max_id_checks: 65_600,
                    max_output_commands: 6,
                },
            }
        );
        assert_eq!(stage.packing, catalog);
        assert_eq!(stage.replay, collection);
        assert_eq!(
            stage.ancestry,
            TeletextAugmentedInvocationBudget {
                max_original_invocations: 256,
                max_actual_invocations: 512,
                max_carriers: 64,
                max_work: 100_000,
            }
        );
        assert_eq!(
            stage.glyph_comparison,
            TeletextAugmentedGlyphComparisonBudget {
                max_records: 100_000,
                max_cells: 1800,
                max_diagnostics: 4096,
                max_work: 1_000_000,
            }
        );
        assert_eq!(
            stage.display_comparison,
            TeletextPhysicalComparisonBudget {
                max_records: 100_000,
                max_cells: 1800,
                max_diagnostics: 32_000,
                max_updates: 1_000_000,
            }
        );
        assert_eq!(stage.max_resource_objects, 64);
    }

    #[test]
    fn explicit_fixture_maps_complete_bounds_without_content_inference() {
        let value = parse(&fixture());
        assert_eq!(value.recovery_timeout, Duration::from_secs(5));
        assert_storage_and_model_limits(&value);
        assert_normalization_limits(&value);
        assert_eq!(
            [
                value.encoding.encoding.max_pages,
                value.encoding.encoding.max_units,
                value.encoding.encoding.max_commands
            ],
            [16, 1024, 8192]
        );
        assert_eq!(
            credits(&value),
            [
                Some(134_217_728),
                Some(262_144),
                Some(33_554_432),
                Some(131_072),
                Some(4096),
                Some(8192),
                Some(4096),
                Some(8192),
                Some(1024),
                Some(16_384),
                Some(4096),
                Some(131_072),
                Some(32_768),
            ]
        );
        assert_eq!(
            value.source.limits(),
            TeletextSourcePolicyLimits {
                elementary_pid: 512,
                rate_tolerance_ppm: 1000,
                pcr_jitter_ticks: 14,
                pcr_gap_slack_ticks: 540_000,
                pcr_gap_slack_packets: 36,
                min_pcr_intervals: 2,
                max_window_packets: 1024,
                max_buffer_packets: 8192,
                max_hold: Duration::from_secs(2),
                playout_delay: Duration::from_millis(100),
                max_lateness: Duration::from_millis(100),
                first_byte_timeout: Duration::from_secs(5),
                probe_maximum_bytes: 1024 * 188,
                probe_maximum_time: Duration::from_secs(5),
                startup_maximum_bytes: 8192 * 188,
            }
        );
    }

    #[test]
    fn credits_are_independent_required_and_zero_remains_enabled() {
        let mut value = fixture();
        for (index, key) in CREDIT_KEYS.into_iter().enumerate() {
            value["encoding"][key] = json!(index + 1);
        }
        assert_eq!(
            credits(&parse(&value)),
            std::array::from_fn(|index| Some(index + 1))
        );
        for key in CREDIT_KEYS {
            let mut missing = value.clone();
            missing["encoding"].as_object_mut().unwrap().remove(key);
            assert_error(&missing, TeletextRuntimePolicyError::InvalidJson);
            let mut null = value.clone();
            null["encoding"][key] = Value::Null;
            assert_error(&null, TeletextRuntimePolicyError::InvalidJson);
            value["encoding"][key] = json!(0);
        }
        assert_eq!(credits(&parse(&value)), [Some(0); 13]);
    }

    #[test]
    fn producer_composition_limits_remain_independent_of_source_validation() {
        let mut value = fixture();
        value["composition"]["service"]["max_records"] = json!(7);
        value["composition"]["declarations"]["max_displays"] = json!(3);
        let policy = parse(&value);
        assert_eq!(policy.encoding.composition.service.max_records, 7);
        assert_eq!(policy.encoding.composition.declarations.max_displays, 3);
        assert_eq!(policy.encoding.validation.service.max_records, 16);
        assert_eq!(policy.encoding.validation.declarations.max_displays, 8);
        assert_eq!(
            policy.design.validation.service,
            policy.encoding.validation.service
        );
    }

    #[test]
    fn raw_byte_limit_is_inclusive_and_precedes_json_parsing() {
        let mut boundary = FIXTURE.to_owned();
        boundary.extend(std::iter::repeat_n(' ', MAX_POLICY_BYTES - FIXTURE.len()));
        assert!(parse_native_teletext_runtime_policy(&boundary).is_ok());
        boundary.push(' ');
        assert!(matches!(
            parse_native_teletext_runtime_policy(&boundary),
            Err(TeletextRuntimePolicyError::TooLarge)
        ));
        assert!(matches!(
            parse_native_teletext_runtime_policy(&" ".repeat(MAX_POLICY_BYTES + 1)),
            Err(TeletextRuntimePolicyError::TooLarge)
        ));
        let non_ascii = "é".repeat(MAX_POLICY_BYTES / 2 + 1);
        assert!(matches!(
            parse_native_teletext_runtime_policy(&non_ascii),
            Err(TeletextRuntimePolicyError::TooLarge)
        ));
    }

    #[test]
    fn version_schema_and_duplicates_are_rejected_without_defaults() {
        for version in [0, 2, u32::MAX] {
            let mut value = fixture();
            value["version"] = json!(version);
            assert_error(&value, TeletextRuntimePolicyError::UnsupportedVersion);
        }
        for raw in ["", "{}", r#"{"version":1,"recovery_timeout_ms":1}"#] {
            assert!(matches!(
                parse_native_teletext_runtime_policy(raw),
                Err(TeletextRuntimePolicyError::InvalidJson)
            ));
        }
        let duplicate = FIXTURE.replacen(
            "\"max_pages\": 16",
            "\"max_pages\": 16, \"max_pages\": 17",
            1,
        );
        assert_ne!(duplicate, FIXTURE);
        assert!(matches!(
            parse_native_teletext_runtime_policy(&duplicate),
            Err(TeletextRuntimePolicyError::InvalidJson)
        ));
        for pointer in [
            "/recovery_timeout_ms",
            "/design/max_documents",
            "/validation/service",
            "/encoding/normalization/placement/spatial/max_events",
            "/source/max_hold_ms",
        ] {
            let mut value = fixture();
            *value.pointer_mut(pointer).unwrap() = Value::Null;
            assert_error(&value, TeletextRuntimePolicyError::InvalidJson);
        }
    }

    #[test]
    fn every_nested_schema_is_closed_and_content_cannot_be_configured() {
        let pointers = [
            "",
            "/design",
            "/input",
            "/validation",
            "/validation/service",
            "/validation/declarations",
            "/composition",
            "/composition/service",
            "/composition/declarations",
            "/encoding",
            "/encoding/normalization",
            "/encoding/normalization/composition",
            "/encoding/normalization/composition/collection",
            "/encoding/normalization/realization",
            "/encoding/normalization/placement",
            "/encoding/normalization/placement/catalog",
            "/encoding/normalization/placement/spatial",
            "/encoding/normalization/assembly",
            "/encoding/normalization/assembly/recipe",
            "/encoding/normalization/assembly/recipe/catalog",
            "/encoding/normalization/packing",
            "/encoding/normalization/replay",
            "/encoding/normalization/ancestry",
            "/encoding/normalization/glyph_comparison",
            "/encoding/normalization/display_comparison",
            "/source",
        ];
        for pointer in pointers {
            let mut value = fixture();
            value
                .pointer_mut(pointer)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert(
                    "unknown-private-value".to_owned(),
                    json!("synthetic-private-marker"),
                );
            assert_error(&value, TeletextRuntimePolicyError::InvalidJson);
        }
        for (section, key) in [
            ("encoding", "entry"),
            ("encoding", "transmissions"),
            ("encoding", "v1_region"),
            ("source", "program_number"),
            ("source", "pcr_pid"),
            ("source", "muxrate_bps"),
        ] {
            let mut value = fixture();
            value[section][key] = json!(1);
            assert_error(&value, TeletextRuntimePolicyError::InvalidJson);
        }
    }

    #[test]
    fn timeout_and_source_validation_use_checked_existing_rules() {
        let mut value = fixture();
        value["recovery_timeout_ms"] = json!(0);
        assert_error(&value, TeletextRuntimePolicyError::InvalidRecoveryTimeout);
        for (key, invalid) in [
            ("elementary_pid", 0),
            ("rate_tolerance_ppm", 1_000_000),
            ("min_pcr_intervals", 0),
            ("max_window_packets", 32_769),
            ("max_buffer_packets", 0),
            ("probe_maximum_bytes", 0),
            ("startup_maximum_bytes", 0),
            ("max_hold_ms", 100),
            ("first_byte_timeout_ms", 0),
            ("probe_maximum_time_ms", 0),
            ("playout_delay_ms", 0),
            ("max_lateness_ms", 2000),
        ] {
            let mut value = fixture();
            value["source"][key] = json!(invalid);
            assert_error(&value, TeletextRuntimePolicyError::InvalidSourcePolicy);
        }
        // Do not assume an OS-specific maximum Instant or reject a duration
        // the existing validator accepts; raw numeric overflow fails the schema.
        let overflow = (u128::from(u64::MAX) + 1).to_string();
        for (field, original) in [("recovery_timeout_ms", 5000), ("max_hold_ms", 2000)] {
            let invalid = FIXTURE.replacen(
                &format!("\"{field}\": {original}"),
                &format!("\"{field}\": {overflow}"),
                1,
            );
            assert_ne!(invalid, FIXTURE);
            assert!(matches!(
                parse_native_teletext_runtime_policy(&invalid),
                Err(TeletextRuntimePolicyError::InvalidJson)
            ));
        }
    }

    #[test]
    fn numeric_types_reject_target_overflow_negative_and_fractional_limits() {
        let overflow = (usize::MAX as u128 + 1).to_string();
        let invalid = FIXTURE.replacen(
            "\"max_serialized_bytes\": 262144",
            &format!("\"max_serialized_bytes\": {overflow}"),
            1,
        );
        assert_ne!(invalid, FIXTURE);
        assert!(matches!(
            parse_native_teletext_runtime_policy(&invalid),
            Err(TeletextRuntimePolicyError::InvalidJson)
        ));
        for invalid in [json!(-1), json!(1.5), json!("32")] {
            let mut value = fixture();
            value["encoding"]["max_identity_checks"] = invalid;
            assert_error(&value, TeletextRuntimePolicyError::InvalidJson);
        }
    }

    #[test]
    fn parse_errors_never_echo_raw_configuration_or_field_names() {
        let mut value = fixture();
        value["synthetic-private-key"] = json!("synthetic-private-value");
        let Err(error) = parse_native_teletext_runtime_policy(&value.to_string()) else {
            panic!("invalid policy accepted");
        };
        for rendered in [error.to_string(), format!("{error:?}")] {
            assert!(!rendered.contains("synthetic-private"));
            assert!(!rendered.contains("synthetic-private-key"));
            assert!(!rendered.contains("synthetic-private-value"));
        }
    }
}
