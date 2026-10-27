//! Native policy context retains every original fallback unit byte-for-byte.
use super::*;
use crate::teletext::presentation_tests::{decode, field};

fn header_original(owner: TeletextPageAddress, local: bool) -> TeletextDeclaredService {
    let base = original(owner, [1, 2, 3], 4).semantic().base().clone();
    let semantic = TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base,
            pages: if local {
                vec![TeletextResolvedPageEnhancementDraft {
                    address: owner,
                    presentation: None,
                    compositional_links: None,
                    local_program: Some(
                        TeletextEnhancementProgram::new(vec![
                            TeletextEnhancementCommand::SetPosition {
                                position: TeletextEnhancementPosition::new(2, 8).unwrap(),
                            },
                            TeletextEnhancementCommand::AtColumn {
                                column: 8,
                                operation: TeletextColumnOperation::G2 {
                                    code: TeletextGlyphCode::new(0x23).unwrap(),
                                },
                            },
                        ])
                        .unwrap(),
                    ),
                    resources: TeletextResolvedPageResourceSelection {
                        objects: TeletextResolvedObjectPageLinks {
                            public: None,
                            global: None,
                        },
                        drcs: TeletextDrcsPageLinks::unlinked(),
                        defaults: [None; 4],
                    },
                }]
            } else {
                vec![]
            },
            magazines: vec![],
            objects: vec![],
            drcs: vec![],
        },
        semantic_limits(),
    )
    .unwrap();
    TeletextDeclaredService::new(
        semantic,
        vec![],
        vec![TeletextDisplayMotSelection {
            display: owner,
            mot: None,
        }],
        vec![],
        declaration_limits(),
    )
    .unwrap()
}

fn fallback_and_enhancement_units(page: &EncodedTeletextAuthoredPage) -> Vec<[u8; 46]> {
    page.data_units()
        .iter()
        .filter(
            |unit| match source_header::observe_dvb_teletext_header(unit).unwrap() {
                source_header::TeletextHeaderObservation::Header(_) => true,
                source_header::TeletextHeaderObservation::NotHeader(packet) => {
                    !matches!(packet.packet_number, 28 | 29)
                }
                source_header::TeletextHeaderObservation::Terminator(_) => false,
                source_header::TeletextHeaderObservation::NotTeletext { .. } => {
                    panic!("native page contains a non-teletext unit")
                }
            },
        )
        .copied()
        .collect()
}

#[test]
fn secondary_default_agreement_native_words_and_header_body_fallback_remain_exact() {
    for local in [false, true] {
        assert_native_policy(local);
    }
}

fn assert_native_policy(local: bool) {
    let owner = address(200, 0x20);
    let original = header_original(owner, local);
    let baseline = encode_teletext_authored_service(original.clone(), &[], &limits()).unwrap();
    assert!(!packet_numbers(&baseline.pages()[0]).contains(&28));
    assert_eq!(packet_numbers(&baseline.pages()[0]).contains(&26), local);
    let original_units = fallback_and_enhancement_units(&baseline.pages()[0]);
    let mut no_second = None;
    for secondary in [None, Some(1), Some(0x10)] {
        let choice = secondary.map_or(TeletextSecondaryDefaultChoice::NoSecondG0 {}, |code| {
            TeletextSecondaryDefaultChoice::Designated {
                designation: TeletextPageCharsetDesignation::new(code).unwrap(),
            }
        });
        let agreement = TeletextSecondaryDefaultAgreement::new(choice);
        let candidate = compose(vec![
            producer(original.clone()).with_secondary_default_agreement(Some(agreement))
        ]);
        assert_eq!(candidate.producers()[0].original(), &original);
        assert_eq!(
            candidate.producers()[0].pages()[0].secondary_selection(),
            TeletextProducerSecondarySelection::DecoderDependent
        );
        let encoded =
            encode_teletext_authored_service(candidate.composed().clone(), &[], &limits()).unwrap();
        assert_eq!(encoded.pages().len(), 1);
        assert!(encoded.magazine_units().is_empty());
        let page = &encoded.pages()[0];
        assert_eq!(fallback_and_enhancement_units(page), original_units);
        assert_eq!(packet_numbers(page).contains(&26), local);
        let units: Vec<_> = page.data_units().iter().filter(|unit|
            matches!(source_header::observe_dvb_teletext_header(unit).unwrap(),
                source_header::TeletextHeaderObservation::NotHeader(packet) if packet.packet_number==28)).collect();
        assert_eq!(units.len(), 1);
        let body = std::array::from_fn(|index| units[0][6 + index].reverse_bits());
        // Existing independent Hamming-position reader checks all13 parity words.
        let words = decode(&body);
        assert_eq!(field(&words, 0, 7), 0);
        assert_eq!(field(&words, 7, 7), 1);
        assert_eq!(field(&words, 14, 7), secondary.map_or(127, u32::from));
        if let Some(control) = no_second {
            for bit in (0..234).filter(|bit| !(14..21).contains(bit)) {
                assert_eq!(
                    field(&words, bit, 1),
                    field(&control, bit, 1),
                    "changed unrelated bit{bit}"
                );
            }
        } else {
            no_second = Some(words);
        }
    }
}
