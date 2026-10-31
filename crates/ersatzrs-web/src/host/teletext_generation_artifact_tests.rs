//! Versioned fingerprints keep baseline bytes and bounded header authority distinct.

use super::*;
use ersatzrs_domain::*;
use ersatzrs_ffmpeg::teletext::{
    encode_teletext_authored_service, TeletextResourceFullUpdate,
    TeletextServiceResourceTransmission,
};
use std::time::Duration;

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(30)
}

fn service(counter: u8, pixels: u16) -> EncodedTeletextAuthoredService {
    let policy = crate::host::teletext_runtime_policy::parse_native_teletext_runtime_policy(
        include_str!("../../tests/fixtures/teletext-output-policy-v1.json"),
    )
    .unwrap()
    .encoding;
    let address = TeletextPageAddress {
        page: TeletextPageNumber::new(200).unwrap(),
        subcode: TeletextSubcode::new(0).unwrap(),
    };
    let language = TeletextLanguage::new("eng".to_owned()).unwrap();
    let resource = TeletextResourceIdentity::new(
        TeletextResourceKind::Drcs,
        TeletextRawDataPageNumber::new(2, 0xc0).unwrap(),
        TeletextResourceSubpageIndex::new(0).unwrap(),
    )
    .unwrap();
    let semantic = TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base: TeletextPageSetDraft {
                initial_page: address.page,
                language: language.clone(),
                pages: vec![TeletextPageDraft {
                    address,
                    kind: TeletextPageKind::FullPage,
                    language,
                    national_option: TeletextNationalOption::new(0).unwrap(),
                    rows: Vec::new(),
                    navigation: Vec::new(),
                }],
            },
            pages: vec![TeletextResolvedPageEnhancementDraft {
                address,
                presentation: None,
                compositional_links: None,
                local_program: None,
                resources: TeletextResolvedPageResourceSelection {
                    objects: TeletextResolvedObjectPageLinks {
                        public: None,
                        global: None,
                    },
                    drcs: TeletextDrcsPageLinks::unlinked(),
                    defaults: [None; 4],
                },
            }],
            magazines: Vec::new(),
            objects: Vec::new(),
            drcs: vec![TeletextDrcsSubtable::new(
                resource,
                vec![TeletextDrcsGlyphDefinition {
                    slot: TeletextDrcsGlyphSlot::new(0).unwrap(),
                    bitmap: TeletextDrcsBitmap::new([pixels; 10]).unwrap(),
                }],
            )
            .unwrap()],
        },
        policy.validation.service,
    )
    .unwrap();
    let declared = TeletextDeclaredService::new(
        semantic,
        Vec::new(),
        vec![TeletextDisplayMotSelection {
            display: address,
            mot: None,
        }],
        Vec::new(),
        policy.validation.declarations,
    )
    .unwrap();
    encode_teletext_authored_service(
        declared,
        &[TeletextServiceResourceTransmission {
            resource,
            counter: TeletextResourceUpdateCounter::new(counter).unwrap(),
            update: TeletextResourceFullUpdate::Unchanged,
            last_subpage: false,
        }],
        &policy.encoding,
    )
    .unwrap()
}

#[test]
fn lifecycle_artifact_requires_version_bound_authorization_and_retains_exact_baseline() {
    let baseline = service(1, 0x801);
    let legacy = artifact_digest(&baseline, deadline()).unwrap();
    let units: Vec<_> = baseline
        .pages()
        .iter()
        .map(|p| p.data_units().to_vec())
        .collect();
    assert!(artifact_digest_for_version(&baseline, 3, deadline()).is_err());
    let authorized = baseline
        .with_resource_lifecycle_variants(deadline())
        .unwrap();
    assert!(artifact_digest(&authorized, deadline()).is_err());
    assert_eq!(
        authorized
            .pages()
            .iter()
            .map(|p| p.data_units().to_vec())
            .collect::<Vec<_>>(),
        units
    );
    let digest = artifact_digest_for_version(&authorized, 3, deadline()).unwrap();
    assert_ne!(digest, legacy);
    assert_eq!(
        digest,
        artifact_digest_for_version(
            &service(1, 0x801)
                .with_resource_lifecycle_variants(deadline())
                .unwrap(),
            3,
            deadline()
        )
        .unwrap()
    );
    for version in [0, 2, 4, 5] {
        assert!(artifact_digest_for_version(&authorized, version, deadline()).is_err());
    }
}

#[test]
fn lifecycle_artifact_binds_body_and_authored_seed_even_with_identical_header_family() {
    let a = service(1, 0x801)
        .with_resource_lifecycle_variants(deadline())
        .unwrap();
    let seed = service(2, 0x801)
        .with_resource_lifecycle_variants(deadline())
        .unwrap();
    assert_eq!(
        a.resource_lifecycle_variants().unwrap().entries()[0].headers(),
        seed.resource_lifecycle_variants().unwrap().entries()[0].headers()
    );
    let body = service(1, 0x803)
        .with_resource_lifecycle_variants(deadline())
        .unwrap();
    let digest = artifact_digest_for_version(&a, 3, deadline()).unwrap();
    assert_ne!(
        digest,
        artifact_digest_for_version(&seed, 3, deadline()).unwrap()
    );
    assert_ne!(
        digest,
        artifact_digest_for_version(&body, 3, deadline()).unwrap()
    );
    let expired = Instant::now().checked_sub(Duration::from_secs(1)).unwrap();
    assert!(matches!(
        artifact_digest_for_version(&a, 3, expired),
        Err(NativeTeletextGenerationError::Design(
            NativeTeletextDesignError::TimedOut
        ))
    ));
}

fn guide_qualification() -> ersatzrs_core::repositories::TeletextGuideGenerationQualification {
    use chrono::TimeZone as _;
    use ersatzrs_core::{
        programme::ProgrammeClock,
        repositories::{
            TeletextGuideSourceFence, TELETEXT_GUIDE_CONTEXT_DIGEST_VERSION,
            TELETEXT_GUIDE_SOURCE_FENCE_VERSION,
        },
    };
    use std::num::NonZeroU8;
    let captured = chrono::Utc
        .with_ymd_and_hms(2026, 10, 28, 12, 0, 0)
        .unwrap();
    let (start, finish) = ProgrammeClock::named("UTC")
        .unwrap()
        .calendar_day_bounds(captured, NonZeroU8::new(3).unwrap())
        .unwrap();
    TeletextGuideSourceFence {
        version: TELETEXT_GUIDE_SOURCE_FENCE_VERSION,
        context_digest_version: TELETEXT_GUIDE_CONTEXT_DIGEST_VERSION,
        context_digest: [7; 32],
        channel_ids: vec![1, 2],
        captured_at: captured,
        start,
        finish,
        locale: "en-us".to_owned(),
        timezone: "UTC".to_owned(),
    }
    .qualify_current(std::iter::empty(), captured)
    .unwrap()
}

#[test]
fn guide_artifact_binds_immutable_source_role_expiry_and_native_lifecycle() {
    use ersatzrs_core::repositories::{
        TELETEXT_EFFECTIVE_GUIDE_GENERATION_ARTIFACT_VERSION,
        TELETEXT_GUIDE_GENERATION_ARTIFACT_VERSION,
    };
    let qualification = guide_qualification();
    let baseline = service(1, 0x801)
        .with_resource_lifecycle_variants(deadline())
        .unwrap();
    let digest = |service: &EncodedTeletextAuthoredService,
                  proof: &ersatzrs_core::repositories::TeletextGuideGenerationQualification,
                  role| {
        super::super::teletext_guide_generations::fingerprint(service, proof, role, deadline())
            .unwrap()
    };
    let guide_digest =
        |service: &EncodedTeletextAuthoredService,
         proof: &ersatzrs_core::repositories::TeletextGuideGenerationQualification| {
            digest(service, proof, TELETEXT_GUIDE_GENERATION_ARTIFACT_VERSION)
        };
    let original = guide_digest(&baseline, &qualification);
    assert_eq!(original, guide_digest(&baseline, &qualification));
    assert_ne!(
        original,
        artifact_digest_for_version(&baseline, 3, deadline()).unwrap()
    );
    assert_ne!(
        original,
        digest(
            &baseline,
            &qualification,
            TELETEXT_EFFECTIVE_GUIDE_GENERATION_ARTIFACT_VERSION
        )
    );
    let mut changed_source = qualification.clone();
    changed_source.source.context_digest[0] ^= 1;
    let mut changed_order = qualification.clone();
    changed_order.source.channel_ids.reverse();
    let mut changed_capture = qualification.clone();
    changed_capture.source.captured_at += chrono::Duration::nanoseconds(1);
    let mut changed_expiry = qualification.clone();
    changed_expiry.valid_until -= chrono::Duration::nanoseconds(1);
    for changed in [
        changed_source,
        changed_order,
        changed_capture,
        changed_expiry,
    ] {
        assert_ne!(original, guide_digest(&baseline, &changed));
    }
    for (counter, pixels) in [(1, 0x803), (2, 0x801)] {
        let changed = service(counter, pixels)
            .with_resource_lifecycle_variants(deadline())
            .unwrap();
        assert_ne!(original, guide_digest(&changed, &qualification));
    }
}
