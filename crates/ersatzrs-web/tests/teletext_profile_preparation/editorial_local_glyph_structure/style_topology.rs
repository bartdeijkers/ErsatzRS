//! Existing Local styling gains a new non-spacing boundary inside its free zone.
use super::*;

#[path = "style_topology/actual_editor.rs"]
mod actual_editor;

fn inserted_words() -> Vec<u32> {
    let mut words = glyph_words(0);
    // Original FG/BG/charset, A, A, then the new foreground before mosaic@1.
    words.insert(5, 1 | (19 << 11));
    words
}

fn styles() -> [(TeletextColumnOperation, u32); 3] {
    [
        (
            TeletextColumnOperation::Foreground {
                colour: TeletextColourReference::new(19).unwrap(),
            },
            word(1, 0, 19),
        ),
        (
            TeletextColumnOperation::Background {
                colour: TeletextColourReference::new(1).unwrap(),
            },
            word(1, 3, 1),
        ),
        (
            TeletextColumnOperation::Attributes {
                double_height: false,
                double_width: false,
                boxed: false,
                conceal: false,
                invert: true,
                underline_or_separated_mosaics: false,
            },
            word(1, 12, 16),
        ),
    ]
}
fn edited_words(styles: &[(TeletextColumnOperation, u32)]) -> Vec<u32> {
    let mut words = glyph_words(0);
    words.splice(5..5, styles.iter().map(|(_, word)| *word));
    words
}
fn expected(styles: &[(TeletextColumnOperation, u32)]) -> TeletextLocalObjectGlyphProgramOverlay {
    let original = program(0);
    let mut commands = super::commands(0);
    commands.splice(
        5..5,
        styles.iter().map(|(operation, _)| column(1, *operation)),
    );
    let retained = (0..5)
        .map(Some)
        .chain(std::iter::repeat_n(None, styles.len()))
        .chain((5..original.commands().len()).map(Some))
        .collect();
    TeletextLocalObjectGlyphProgramOverlay::new_with_style_commands(
        identity(65520),
        original,
        TeletextEnhancementProgram::new(commands).unwrap(),
        retained,
    )
    .unwrap()
}
async fn assert_concept(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    expected: &TeletextLocalObjectGlyphProgramOverlay,
) -> TeletextRaster {
    let revision = drcs_bitmap::stored(fixture, concept).await;
    let documents =
        decode_teletext_design_documents(&revision.document, designs::budget()).unwrap();
    let overlay = documents.editorial_overlay().unwrap();
    assert_eq!(overlay.schema_version(), 13);
    assert_eq!(
        overlay.local_object_glyph_program_overlays(),
        std::slice::from_ref(expected)
    );
    assert!(overlay.pages().is_empty());
    assert!(overlay.local_program_operands().is_empty());
    assert!(overlay.local_object_operands().is_empty());
    assert!(overlay.drcs_bitmap_overlays().is_empty());
    assert_eq!(
        overlay.provenance().original_digest,
        flow.originals[0].digest
    );
    let mut original: Value = serde_json::from_slice(&revision.document).unwrap();
    original
        .as_object_mut()
        .unwrap()
        .remove("editorial_overlay");
    assert_eq!(
        original,
        serde_json::from_slice::<Value>(&flow.originals[0].document).unwrap()
    );
    let restored = drcs_bitmap::restored(fixture, concept).await;
    let before = flow.accepted[0].service();
    let after = restored.encoded().unwrap();
    let declared = before
        .declared()
        .clone()
        .with_local_object_glyph_program_overlays(
            std::slice::from_ref(expected),
            policy().encoding.validation.service,
        )
        .unwrap();
    assert_eq!(after.declared(), &declared);
    assert_eq!(restored.selection(), flow.saved[0].candidate().selection());
    assert_eq!(
        restored.sources().len(),
        flow.saved[0].candidate().sources().len()
    );
    for (current, original) in restored
        .sources()
        .iter()
        .zip(flow.saved[0].candidate().sources())
    {
        assert_eq!(current.bytes(), original.bytes());
        assert_eq!(current.authoring(), original.authoring());
    }
    assert_eq!(non_x26_records(before), non_x26_records(after));
    assert_eq!(before.magazine_units(), after.magazine_units());
    assert_eq!(before.inventory(), after.inventory());
    assert_eq!(before.pages().len(), after.pages().len());
    for (old, new) in before.pages().iter().zip(after.pages()) {
        if !matches!(old, EncodedTeletextAuthoredPage::CompiledDisplay(page)
            if page.page().address() == channel_documents::address(124, 32))
        {
            assert_eq!(old.data_units(), new.data_units());
        }
    }
    assert_eq!(
        before
            .declared()
            .semantic()
            .objects()
            .definition(identity(65521)),
        after
            .declared()
            .semantic()
            .objects()
            .definition(identity(65521))
    );
    let before = external_editor_probe::preview_page(before, 124);
    let after = external_editor_probe::preview_page(after, 124);
    assert_eq!((after.width(), after.height()), (672, 250));
    let mut changed = [false; 2];
    for (index, (old, new)) in before.pixels().iter().zip(after.pixels()).enumerate() {
        if old == new {
            continue;
        }
        let (row, column) = (index / after.width(), index % after.width());
        assert!((10..20).contains(&row) && (44 * 12..52 * 12).contains(&column));
        changed[(column / 12 - 44) / 4] = true;
    }
    if expected.replacement().commands().len() == expected.expected().commands().len() + 1 {
        // The fixed RED's lone FG19 insertion has no net raster effect in this
        // fixture. Its exact command/map remains proven above; the combined
        // styles below independently require visible changes in both consumers.
        assert_eq!(changed, [false, false]);
        assert_eq!(after, before);
    } else {
        assert_eq!(changed, [true, true]);
    }
    after
}
async fn positive(factory: fn(&str, u16, u16) -> Value, styles: &[(TeletextColumnOperation, u32)]) {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
        factory,
        Fixture::new_persistent().await,
        drcs_companion::encoding_policy,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let request = request_body_words(
        &flow.receipts[0],
        1,
        flow.accepted[0].service(),
        &glyph_words(0),
        edited_words(styles),
    );
    let concept = import(&fixture, request).await;
    assert_eq!(concept.head().revision, 2);
    let expected = expected(styles);
    let preview = Box::pin(assert_concept(&fixture, &flow, &concept, &expected)).await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    assert_eq!(
        Box::pin(assert_concept(&fixture, &flow, &concept, &expected)).await,
        preview
    );
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    let current = drcs_bitmap::restored(&fixture, &concept).await;
    let exported = drcs_bitmap::reexport(&fixture, &concept).await;
    let latest = receipt(&fixture, &exported).await;
    let undone = import(
        &fixture,
        request_body_words(
            &latest,
            2,
            current.encoded().unwrap(),
            &edited_words(styles),
            glyph_words(0),
        ),
    )
    .await;
    assert_eq!(undone.head().revision, 3);
    assert_eq!(
        drcs_bitmap::stored(&fixture, &undone).await.document,
        flow.originals[0].document
    );
    let current = drcs_bitmap::restored(&fixture, &undone).await;
    for (old, new) in flow.accepted[0]
        .service()
        .pages()
        .iter()
        .zip(current.encoded().unwrap().pages())
    {
        assert_eq!(old.data_units(), new.data_units());
    }
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}
#[tokio::test]
async fn local_style_topology_native_inserts_foreground_into_existing_zone() {
    assert_eq!(inserted_words(), edited_words(&styles()[..1]));
    Box::pin(positive(zone, &styles()[..1])).await;
}
#[tokio::test]
async fn local_style_topology_native_combines_styles_free_zone_reopen_and_exact_undo() {
    for factory in [free as fn(&str, u16, u16) -> Value, zone] {
        Box::pin(positive(factory, &styles())).await;
    }
}
#[tokio::test]
async fn local_style_topology_native_refuses_protected_shared_consumers_and_invalid_words() {
    for factory in [narrow as fn(&str, u16, u16) -> Value, generated] {
        let (fixture, flow) = Box::pin(setup_with_fixture_policy(
            factory,
            Fixture::new_persistent().await,
            drcs_companion::encoding_policy,
        ))
        .await;
        let retained = resource_bank_admin::Retained::capture(&fixture).await;
        let NativeTeletextEditorialImportError::Effects(error) = reject(
            &fixture,
            &flow,
            request_body_words(
                &flow.receipts[0],
                1,
                flow.accepted[0].service(),
                &glyph_words(0),
                edited_words(&styles()),
            ),
        )
        .await
        else {
            panic!("second consumer must retain its original protection");
        };
        assert_eq!(
            error.reason,
            TeletextEditorialEffectsFailure::ProtectedDisplay
        );
        assert_eq!(error.page, Some(channel_documents::address(124, 32)));
        assert!(matches!(error.position, Some((1, 48..=51))));
        retained.assert_unchanged(&fixture, &flow).await;
        fixture.state.pool.close().await;
    }
    let (fixture, flow) = Box::pin(setup_with_fixture_policy(
        zone,
        Fixture::new_persistent().await,
        drcs_companion::encoding_policy,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let mut removed_charset = edited_words(&styles());
    removed_charset.remove(2);
    let mut bad_colour = inserted_words();
    bad_colour[5] = word(1, 0, 32);
    let mut bad_attributes = inserted_words();
    bad_attributes[5] = word(1, 12, 8);
    for words in [removed_charset, bad_colour, bad_attributes] {
        assert!(matches!(
            reject(
                &fixture,
                &flow,
                request_body_words(
                    &flow.receipts[0],
                    1,
                    flow.accepted[0].service(),
                    &glyph_words(0),
                    words
                )
            )
            .await,
            NativeTeletextEditorialImportError::Tti(_)
        ));
        retained.assert_unchanged(&fixture, &flow).await;
    }
    fixture.state.pool.close().await;
}
