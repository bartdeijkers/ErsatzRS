//! Coherent acquisition ranks original resources before the actual display page.
use super::*;
use crate::teletext::{
    authored_service_tests::{declared, transmissions},
    *,
};

fn source(global: bool) -> (TeletextDeclaredService, TeletextObjectIdentity) {
    let resource = TeletextResourceIdentity::new(
        if global {
            TeletextResourceKind::Gpop
        } else {
            TeletextResourceKind::Pop
        },
        TeletextRawDataPageNumber::new(2, if global { 0xb0 } else { 0xa0 }).unwrap(),
        TeletextResourceSubpageIndex::ZERO,
    )
    .unwrap();
    let target = if global {
        TeletextObjectIdentity::global(resource, TeletextObjectId::new(21)).unwrap()
    } else {
        TeletextObjectIdentity::public(resource, TeletextObjectId::new(21)).unwrap()
    };
    let mut draft = fixture_draft_with_active_body(
        |passive, _| {
            let active = TeletextEnhancementCommand::InvokeObject {
                target,
                object_type: TeletextObjectType::Active,
                offset: None,
            };
            vec![
                fg(8, 1),
                active,
                position(9),
                passive,
                TeletextEnhancementCommand::SetPosition {
                    position: TeletextEnhancementPosition::new(1, 8).unwrap(),
                },
                active,
            ]
        },
        vec![fg(0, 3), fg(0, 4)],
    );
    draft.objects.push(TeletextEnhancementObjectDefinition {
        identity: target,
        object_type: TeletextObjectType::Active,
        program: TeletextEnhancementProgram::new(vec![fg(0, 3), fg(0, 4)]).unwrap(),
    });
    draft.base.pages[0].rows.push(TeletextRow {
        row: 0,
        runs: vec![TeletextRun {
            column: 9,
            style: TeletextStyle::default(),
            content: TeletextRunContent::Text { text: "ZZ".into() },
        }],
    });
    if global {
        draft.pages[0].resources.objects.global = Some(resource.page());
        let declaration = |page| {
            TeletextX27LinkDeclaration::new(
                page,
                TeletextX27LinkValidity::Level25Only,
                TeletextSubpageRequirementMask::new(
                    if matches!(page, TeletextX27LinkTarget::Unspecified { .. }) {
                        u16::MAX
                    } else {
                        1
                    },
                ),
            )
            .unwrap()
        };
        let unused = declaration(TeletextX27LinkTarget::Unspecified {
            magazine: TeletextX27Magazine::new(2).unwrap(),
        });
        draft.pages[0].compositional_links = Some(
            TeletextX27Level25Links::new(
                declaration(TeletextX27LinkTarget::Page {
                    page: resource.page(),
                }),
                declaration(TeletextX27LinkTarget::Page {
                    page: draft.pages[0].resources.objects.public.unwrap(),
                }),
                unused,
                unused,
            )
            .unwrap(),
        );
    }
    (declared(draft), target)
}

fn check(global: bool) {
    let (source, target) = source(global);
    let unchanged = source.clone();
    let standalone = compile_teletext_normalized_main_program(
        source.semantic(),
        source.semantic().base().pages[0].address,
        limits().normalization,
    )
    .unwrap_err();
    let TeletextNormalizedMainProgramFailure::Original(error) = standalone.reason else {
        panic!("standalone compilation has no service acquisition authority");
    };
    let TeletextComposedMainEncodingError::Packing(error) = *error else {
        panic!("original header packing refusal required");
    };
    assert_eq!(
        error.location,
        Some(TeletextMainCompositionSource::OriginalPage { command: 3 })
    );
    let encoded =
        encode_teletext_authored_service(source, &transmissions(&unchanged), &limits()).unwrap();
    assert_eq!(encoded.declared(), &unchanged);
    let EncodedTeletextAuthoredPage::CompiledDisplay(page) = &encoded.pages()[0] else {
        panic!("complete shared display required");
    };
    for attributes in [
        page.normalized().original().attribute_execution(),
        page.normalized().physical().replay().attribute_execution(),
    ] {
        let foreground = attributes.cell(0, 10).unwrap().foreground();
        assert_eq!(
            foreground.value(),
            Some(TeletextColourReference::new(1).unwrap())
        );
        assert_eq!(foreground.pending(), None);
        assert_eq!(
            attributes.cell(1, 8).unwrap().foreground().value(),
            Some(TeletextColourReference::new(4).unwrap()),
            "a second original invocation makes resource execution observable"
        );
    }
    assert_eq!(
        page.normalized().original().spatial_trace().invocations()[0].target(),
        target
    );
    assert_resource_words(&encoded, target);
    assert_fallback(&unchanged.semantic().base().pages[0], page.page());
    let mut carousel = TeletextCarousel::new_authored(encoded).unwrap();
    let first = crate::teletext::carousel::authored_service_carousel_tests::cycle(&mut carousel);
    let headers = first
        .iter()
        .flat_map(TeletextCarouselBatch::data_units)
        .filter_map(
            |unit| match source_header::observe_dvb_teletext_header(unit).unwrap() {
                source_header::TeletextHeaderObservation::Header(header) => {
                    Some(header.display_page().is_some())
                }
                _ => None,
            },
        )
        .collect::<Vec<_>>();
    let display = headers.iter().position(|display| *display).unwrap();
    let resource = headers.iter().rposition(|display| !*display).unwrap();
    assert!(
        resource < display,
        "actual producer must authenticate B.7 group order"
    );
    let second = crate::teletext::carousel::authored_service_carousel_tests::cycle(&mut carousel);
    if let Some(directory) =
        crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_RESOURCE_C2_FIXTURE_DIR"])
    {
        let units = first
            .iter()
            .chain(&second)
            .flat_map(|b| b.data_units().iter().map(|u| u.to_vec()))
            .collect::<Vec<_>>();
        let name = if global { "global" } else { "public" };
        std::fs::write(
            std::path::Path::new(&directory).join(format!("{name}-complete.json")),
            serde_json::to_vec(&serde_json::json!({"page":512,"subcode":0,"units":units})).unwrap(),
        )
        .unwrap();
    }
}

fn assert_resource_words(service: &EncodedTeletextAuthoredService, target: TeletextObjectIdentity) {
    let page = service
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::Object(page)
                if Some(page.header().identity()) == target.resource() =>
            {
                Some(page)
            }
            _ => None,
        })
        .unwrap();
    let words = page
        .command_positions()
        .iter()
        .filter(|p| p.source().object == Some(target))
        .collect::<Vec<_>>();
    assert_eq!(words.len(), 2, "unchanged Active remains FG3/FG4 only");
    let body = page.data_units().iter().find(|unit| matches!(
        source_header::observe_dvb_teletext_header(unit).unwrap(),
        source_header::TeletextHeaderObservation::NotHeader(address) if address.packet_number == 3
    )).unwrap();
    for ((ordinal, colour), word) in [(0, 3_u32), (1, 4)].into_iter().zip(words) {
        assert_eq!(word.source().command, ordinal);
        assert!(word.semantic_word() < 13);
        let start = 7 + 3 * word.semantic_word();
        assert_eq!(
            &body[start..start + 3],
            &packets::hamming_24_18(colour << 11).map(u8::reverse_bits)
        );
    }
}

fn assert_fallback(base: &TeletextPageDraft, encoded: &EncodedTeletextPage) {
    let plain = encode_teletext_page(base, TeletextEncodingRegion::WesternLatin).unwrap();
    let actual = project_encoded_teletext_display(encoded, 128).unwrap();
    let expected = project_encoded_teletext_display(&plain, 128).unwrap();
    assert_eq!(actual.header, expected.header);
    let rows = |records: Vec<TeletextEncodedDisplayRecord>| {
        records
            .into_iter()
            .filter(|r| !matches!(r, TeletextEncodedDisplayRecord::Triplets { .. }))
            .collect::<Vec<_>>()
    };
    assert_eq!(rows(actual.records), rows(expected.records));
}

#[test]
fn coherent_public_active_order_reaches_complete_factory_and_fallback() {
    check(false);
}

#[test]
fn coherent_global_active_order_reaches_complete_factory_and_fallback() {
    check(true);
}
