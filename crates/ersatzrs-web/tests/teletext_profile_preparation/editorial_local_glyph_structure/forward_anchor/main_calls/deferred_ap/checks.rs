//! Complete declared/raw/preview/source oracles for direct Main context.

use super::*;
use ersatzrs_ffmpeg::teletext::TeletextMainCompositionSource;

pub(super) fn leading_foreground_span(
    service: &EncodedTeletextAuthoredService,
    first: usize,
    semantic: usize,
) {
    let display = service
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::CompiledDisplay(page)
                if page.page().address() == channel_documents::address(124, 32) =>
            {
                Some(page)
            }
            _ => None,
        })
        .unwrap();
    let header_colour = display
        .normalized()
        .original()
        .colour_context()
        .row_colours()[0]
        .get();
    let foreground = display
        .normalized()
        .original()
        .attribute_execution()
        .before_adaptive_cell(0, 8)
        .unwrap()
        .foreground()
        .value()
        .unwrap()
        .get();
    assert_eq!(
        foreground, 7,
        "this fixture retains the Main/Base white foreground"
    );
    let words = native_words(service);
    assert_eq!((first, semantic), (0, 1));
    // The main compatibility policy prepends the actual original header's
    // colour-aware row-zero selector. Both words belong to the same producer;
    // the semantic FG remains last. Passive Local paint (FG18) is not Main
    // context: materialization retains its exact original white foreground.
    assert_eq!(
        &words[first..=semantic],
        &[word(63, 7, u32::from(header_colour)), word(8, 0, 7)],
        "complete initial compatibility selector plus exact semantic FG7 span"
    );
}

pub(super) fn header_carrier(service: &EncodedTeletextAuthoredService) -> Option<usize> {
    let found: Vec<_> = encoded(service)
        .command_positions()
        .iter()
        .filter(|position| {
            position.original_main_source()
                == Some(TeletextMainCompositionSource::HeaderPosition { command: 0 })
        })
        .collect();
    assert!(
        found.len() <= 1,
        "one original Main AP has at most one carrier"
    );
    found.first().map(|position| {
        assert_eq!(position.source().object, None);
        assert_eq!(position.original_local_command(), None);
        assert_eq!(position.original_base_glyph(), None);
        leading_foreground_span(service, position.first_word(), position.semantic_word());
        let program = service.declared().semantic().page_enhancements()
            [&channel_documents::address(124, 32)]
            .local_program
            .as_ref()
            .unwrap();
        assert_eq!(program.commands()[0], anchor());
        // This correspondence authenticates a generated writer only. It does
        // not grant authored operand permissions or prove receiver C.2 timing.
        position.semantic_word()
    })
}

pub(super) fn main_provenance(service: &EncodedTeletextAuthoredService, stage: u8) {
    let program = service.declared().semantic().page_enhancements()
        [&channel_documents::address(124, 32)]
        .local_program
        .as_ref()
        .unwrap();
    assert_eq!(
        program,
        &main_program(stage),
        "authored Main must stay exact"
    );
    assert_eq!(header_carrier(service).is_some(), stage != 2);
    let positions = encoded(service).command_positions();
    for command in 1..program.commands().len() {
        let found: Vec<_> = positions
            .iter()
            .filter(|position| position.original_local_command() == Some(command))
            .collect();
        assert_eq!(
            found.len(),
            1,
            "original Main call {command} must retain one physical source: {positions:#?}"
        );
        assert_eq!(
            found[0].original_main_source(),
            Some(TeletextMainCompositionSource::OriginalPage { command })
        );
        assert_eq!(found[0].source().object, None);
        assert_eq!(found[0].original_base_glyph(), None);
    }
    assert_eq!(
        service
            .declared()
            .semantic()
            .objects()
            .definition(identity(65520))
            .unwrap()
            .program,
        body(0),
        "the shared Local body's own AP0,8 and full definition are untouched"
    );
}

pub(super) async fn concept(
    fixture: &Fixture,
    flow: &Flow,
    saved: &SavedEditorialTeletextDesign,
    expected: &Flow,
    stage: u8,
) {
    let restored = drcs_bitmap::restored(fixture, saved).await;
    let service = restored.encoded().unwrap();
    let expected_service = expected.accepted[0].service();
    assert_eq!(service.declared(), expected_service.declared());
    assert_eq!(designs::units(service), designs::units(expected_service));
    assert_eq!(
        external_editor_probe::preview_page(service, 124),
        external_editor_probe::preview_page(expected_service, 124)
    );
    assert_eq!(
        service.declared().semantic().page_enhancements()[&channel_documents::address(124, 32)]
            .local_program
            .as_ref(),
        Some(&main_program(stage)),
        "effective authored Main matches its independent complete factory"
    );
    let original = flow.saved[0].candidate();
    assert_eq!(restored.metadata(), original.metadata());
    assert_eq!(restored.selection(), original.selection());
    assert_eq!(restored.transmissions(), original.transmissions());
    assert_eq!(restored.sources().len(), original.sources().len());
    for (current, original) in restored.sources().iter().zip(original.sources()) {
        assert_eq!(current.bytes(), original.bytes());
        assert_eq!(current.authoring(), original.authoring());
    }
    assert_eq!(
        restored.producers()[0].original(),
        original.producers()[0].original()
    );
    let stored = drcs_bitmap::stored(fixture, saved).await;
    let mut document: Value = serde_json::from_slice(&stored.document).unwrap();
    document
        .as_object_mut()
        .unwrap()
        .remove("editorial_overlay");
    assert_eq!(
        document,
        serde_json::from_slice::<Value>(&flow.originals[0].document).unwrap()
    );
    let exported = drcs_bitmap::reexport(fixture, saved).await;
    let latest = receipt(fixture, &exported).await;
    assert_complete_tti(
        &drcs_bitmap::from_receipt(&latest, saved.head().revision),
        expected,
    );
}

pub(super) async fn output(
    fixture: &Fixture,
    expected: &AcceptedTeletextOutputCandidate,
    night: &AcceptedTeletextOutputCandidate,
) {
    for (channel, expected) in [(1, expected), (2, night)] {
        generations::assert_same_output(&recover(fixture, channel).await, expected);
        assert_eq!(
            fixture
                .state
                .application
                .teletext_publication_request(channel)
                .await
                .unwrap(),
            Some(expected.generation().request.clone())
        );
    }
}
