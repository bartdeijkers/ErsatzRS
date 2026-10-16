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
