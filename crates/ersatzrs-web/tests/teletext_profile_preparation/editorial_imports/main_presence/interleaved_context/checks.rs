//! Original semantic Main, generated context and complete native oracles.
use super::*;

pub(super) struct Expected {
    declared: TeletextDeclaredService,
    program: TeletextEnhancementProgram,
    raster: TeletextRaster,
    units: Vec<Vec<[u8; 46]>>,
    magazine: Vec<[u8; 46]>,
    fallback: [[u8; 40]; 25],
    pub name: String,
    pub packets: Vec<TeletextTtiRecord>,
    pub body_words: usize,
}

fn provenance(
    service: &EncodedTeletextAuthoredService,
    designation: u8,
    glyph: u8,
) -> [[u8; 40]; 25] {
    let owner = channel_documents::address(124, 32);
    let page = service
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::CompiledDisplay(page)
                if page.page().address() == owner =>
            {
                Some(page)
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(
        serde_json::to_value(program(service).unwrap()).unwrap(),
        json!({"commands":[
            {"kind":"set_position","position":{"row":2,"column":0}},
            {"kind":"at_column","column":0,"operation":{"kind":"designate_charset","designation":designation}},
            {"kind":"at_column","column":9,"operation":{"kind":"g0","code":glyph}}
        ]})
    );
    let mut kinds = Vec::new();
    let mut originals = Vec::new();
    for position in page.normalized().encoded().command_positions() {
        match position.original_main_source().unwrap() {
            TeletextMainCompositionSource::BaseCell {
                position: cell,
                kind,
            } => {
                assert_eq!((cell.row(), cell.column()), (2, 8));
                assert_eq!(position.original_local_command(), None);
                assert_eq!(
                    position.original_base_glyph(),
                    (kind == TeletextBaseCellCommandKind::Glyph).then_some(cell)
                );
                kinds.push(kind);
            }
            TeletextMainCompositionSource::OriginalPage { command } => {
                assert_eq!(position.original_local_command(), Some(command));
                assert_eq!(position.original_base_glyph(), None);
                originals.push(command);
            }
            other => panic!("context fixture must not acquire foreign source authority: {other:?}"),
        }
    }
    assert_eq!(originals, [0, 1, 2]);
    let expected_kinds = if designation == 0 {
        vec![TeletextBaseCellCommandKind::Glyph]
    } else {
        vec![
            TeletextBaseCellCommandKind::LatinCharset,
            TeletextBaseCellCommandKind::Glyph,
            TeletextBaseCellCommandKind::CharsetRestore,
        ]
    };
    assert_eq!(kinds, expected_kinds);
    std::array::from_fn(|row| {
        std::array::from_fn(|column| {
            page.normalized().original().base_display().cells()[row][column].code()
        })
    })
}

pub(super) async fn qualify(
    response: fn(&str, u16, u16) -> Value,
    designation: u8,
    glyph: u8,
) -> Expected {
    let (fixture, flow) = Box::pin(setup_with_response(response)).await;
    let service = flow.accepted[0].service();
    let fallback = provenance(service, designation, glyph);
    let name = flow.exported[0].sidecar().project.pages[0].name.clone();
    let file = flow.exported[0]
        .files()
        .iter()
        .find(|file| file.name == name)
        .unwrap();
    let packets = parse_teletext_tti(&file.bytes, policy().tti)
        .unwrap()
        .records()
        .iter()
        .filter(|record| matches!(record, TeletextTtiRecord::Packet { row: 26, .. }))
        .cloned()
        .collect();
    let result = Expected {
        declared: service.declared().clone(),
        program: program(service).unwrap().clone(),
        raster: pixels(service),
        units: designs::units(service),
        magazine: service.magazine_units().to_vec(),
        fallback,
        name,
        packets,
        body_words: service
            .pages()
            .iter()
            .find_map(|page| match page {
                EncodedTeletextAuthoredPage::CompiledDisplay(page)
                    if page.page().address() == channel_documents::address(124, 32) =>
                {
                    page.normalized().encoded().command_positions().last()
                }
                _ => None,
            })
            .unwrap()
            .semantic_word()
            + 1,
    };
    fixture.state.pool.close().await;
    result
}
pub(super) fn unchanged_base(root: &Expected, edited: &Expected) {
    assert_eq!(
        root.declared.semantic().base(),
        edited.declared.semantic().base()
    );
    assert_eq!(root.fallback, edited.fallback);
    assert_eq!(root.magazine, edited.magazine);
    assert_eq!(
        root.declared.resource_inventory(),
        edited.declared.resource_inventory()
    );
}
pub(super) fn service(service: &EncodedTeletextAuthoredService, expected: &Expected) {
    assert_eq!(service.declared(), &expected.declared);
    assert_eq!(program(service), Some(&expected.program));
    assert_eq!(designs::units(service), expected.units);
    assert_eq!(service.magazine_units(), expected.magazine);
    assert_eq!(pixels(service), expected.raster);
}
pub(super) async fn saved(
    fixture: &Fixture,
    flow: &Flow,
    saved: &SavedEditorialTeletextDesign,
    expected: &Expected,
) {
    let restored = Box::pin(drcs_bitmap::restored(fixture, saved)).await;
    service(restored.encoded().unwrap(), expected);
    let original = flow.saved[0].candidate();
    assert_eq!(restored.selection(), original.selection());
    assert_eq!(restored.metadata(), original.metadata());
    assert_eq!(restored.transmissions(), original.transmissions());
    assert_eq!(restored.sources().len(), original.sources().len());
    for (after, before) in restored.sources().iter().zip(original.sources()) {
        assert_eq!(after.bytes(), before.bytes());
        assert_eq!(after.authoring(), before.authoring());
    }
    assert_eq!(
        restored.producers()[0].original(),
        original.producers()[0].original()
    );
    drop(restored);
    let exported = Box::pin(drcs_bitmap::reexport(fixture, saved)).await;
    let file = exported
        .files()
        .iter()
        .find(|file| file.name == expected.name)
        .unwrap();
    let actual = parse_teletext_tti(&file.bytes, policy().tti)
        .unwrap()
        .records()
        .iter()
        .filter(|record| matches!(record, TeletextTtiRecord::Packet { row: 26, .. }))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(actual, expected.packets);
    assert_retained_storage(fixture, flow).await;
    assert!(fixture.playout.is_empty());
}
pub(super) async fn published(
    fixture: &Fixture,
    day: &AcceptedTeletextOutputCandidate,
    night: &AcceptedTeletextOutputCandidate,
) {
    for (channel, expected) in [(1, day), (2, night)] {
        assert_eq!(
            fixture
                .state
                .application
                .accepted_teletext_generation(channel)
                .await
                .unwrap(),
            Some(expected.generation().clone())
        );
        assert_eq!(
            fixture
                .state
                .application
                .teletext_publication_request(channel)
                .await
                .unwrap(),
            Some(expected.generation().request.clone())
        );
        generations::assert_same_output(&recover(fixture, channel).await, expected);
    }
}
pub(super) async fn publish_after(
    fixture: &Fixture,
    saved: &SavedEditorialTeletextDesign,
    previous: &AcceptedTeletextOutputCandidate,
) -> AcceptedTeletextOutputCandidate {
    use ersatzrs_core::repositories::TeletextPublicationMutation;
    let pending =
        ersatzrs_web::host::teletext_designs::request_original_teletext_design_publication(
            &fixture.state.application,
            saved.head().id,
            saved.head().revision,
            saved.digest().to_owned(),
            saved.head().revision,
            Some(previous.generation().request.id),
            designs::budget(),
            channel_documents::policy(),
            designs::deadline(),
        )
        .await
        .unwrap();
    let TeletextPublicationMutation::Requested(pending) = pending else {
        panic!("explicit publication must use the current request CAS")
    };
    assert_eq!(
        fixture
            .state
            .application
            .accepted_teletext_generation(1)
            .await
            .unwrap(),
        Some(previous.generation().clone())
    );
    generations::assert_same_output(&recover(fixture, 1).await, previous);
    generations::accepted(
        accept_original_teletext_generation(
            &fixture.state.application,
            TeletextGenerationTarget {
                request: pending.request().clone(),
                expected_design_head: saved.head().revision,
                expected_generation_id: Some(previous.generation().id),
            },
            designs::budget(),
            channel_documents::policy(),
            designs::deadline(),
        )
        .await
        .unwrap(),
    )
}
