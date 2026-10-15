//! Installed native operand concepts; GUI/editor and client acceptance are separate.

use super::*;
use ersatzrs_domain::{TeletextColourPixel, TeletextColourReference, TeletextColumnOperation};
use ersatzrs_ffmpeg::teletext::TeletextRasterPixel;

fn response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut response = authoring_metadata::response(theme, page, subcode);
    response["service"]["page_enhancements"][0]["local_program"]["commands"] = json!([
        {"kind":"set_position","position":{"row":2,"column":8}},
        {"kind":"at_column","column":8,"operation":{"kind":"foreground","colour":18}},
        {"kind":"at_column","column":8,"operation":{"kind":"g0","code":65}},
        {"kind":"at_column","column":9,"operation":{"kind":"g0","code":66}},
        {"kind":"set_position","position":{"row":4,"column":8}},
        {"kind":"at_column","column":8,"operation":{"kind":"foreground","colour":18}},
        {"kind":"at_column","column":8,"operation":{"kind":"g0","code":67}}
    ]);
    // A nonspacing foreground applies through the right edge, including spaces.
    // Capturing only the command cell would not grant this complete effect.
    response["authoring"]["pages"][0]["regions"][0]["width"] = json!(39);
    response
}

fn narrow_response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut response = response(theme, page, subcode);
    response["authoring"]["pages"][0]["regions"][0]["column"] = json!(8);
    response["authoring"]["pages"][0]["regions"][0]["width"] = json!(1);
    response
}

fn foreground(colour: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::Foreground {
        colour: TeletextColourReference::new(colour).unwrap(),
    }
}

fn edit_foreground(
    exported: &ExportedOriginalTeletextDesign,
    head: i64,
    occurrence: usize,
    previous: u8,
    replacement: u8,
) -> NativeTeletextEditorialImportRequest {
    let owner = channel_documents::address(124, 32);
    let mut request = request(exported, head, &[]);
    let file = request
        .files
        .iter_mut()
        .find(|file| file.name == page_name(exported, owner))
        .unwrap();
    let mut records = parse_teletext_tti(&file.bytes, editorial_imports::policy().tti)
        .unwrap()
        .records()
        .to_vec();
    // Independent Table 28 literal: address 8, foreground mode 0, colour data.
    // The first matching local operand belongs to row 2; the second to row 4.
    let expected = 8 | (u32::from(previous) << 11);
    let word = records
        .iter_mut()
        .filter_map(|record| match record {
            TeletextTtiRecord::Packet { row: 26, words, .. } => Some(words),
            _ => None,
        })
        .flat_map(|words| words.iter_mut())
        .filter(|word| **word == expected)
        .nth(occurrence)
        .expect("the exported original must contain the existing foreground operand");
    *word = 8 | (u32::from(replacement) << 11);
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, editorial_imports::policy().tti)
            .unwrap(),
        editorial_imports::policy().tti,
    )
    .unwrap();
    request
}

async fn units(fixture: &Fixture, stored: &TeletextDesignRevision) -> Vec<Vec<[u8; 46]>> {
    let restored = reload_original_teletext_design_revision(
        &fixture.state.application,
        stored.design_id,
        stored.revision,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    let encoded = restored.encoded().unwrap();
    std::iter::once(encoded.magazine_units().to_vec())
        .chain(
            encoded
                .pages()
                .iter()
                .map(|page| page.data_units().to_vec()),
        )
        .collect()
}

fn assert_operand(stored: &TeletextDesignRevision, replacement: u8) {
    let overlay = overlay(stored).unwrap();
    assert_eq!(overlay.schema_version(), 2);
    assert!(overlay.pages().is_empty());
    let operands = overlay.local_program_operands();
    assert_eq!(operands.len(), 1);
    let operand = operands[0];
    assert_eq!(operand.owner(), channel_documents::address(124, 32));
    assert_eq!(operand.command(), 1);
    assert_eq!(operand.column(), 8);
    assert_eq!(operand.expected(), foreground(18));
    assert_eq!(operand.replacement(), foreground(replacement));
}

#[tokio::test]
async fn native_operand_import_reexport_cumulative_replace_reopen_and_root_revert() {
    let (mut fixture, flow) = editorial_imports::setup_with_response(response).await;
    super::super::authoring::setting(
        &fixture,
        "teletext.authoring.policy",
        &super::super::authoring::policy().to_string(),
    )
    .await;
    let root = &flow.originals[0];
    let original_units = units(&fixture, root).await;
    let other_units = units(&fixture, &flow.originals[1]).await;
    let before = scoped::stable_state(&fixture).await;
    let calls = std::fs::read(fixture.root.path().join("authoring-calls")).unwrap();
    let configuration = designs::config(&fixture).await;
    let exported = scoped::export(&fixture, root).await;
    let first = scoped::import(&fixture, edit_foreground(&exported, 1, 0, 18, 4)).await;
    let stored = revision(&fixture, &first).await;
    assert_eq!(stored.revision, 2);
    assert_root_context(root, &stored);
    assert_operand(&stored, 4);
    let first_units = units(&fixture, &stored).await;
    assert_ne!(first_units, original_units);
    let owner = channel_documents::address(124, 32);
    let pixels = legacy_pixels(&fixture, &first, owner).await;
    let original_pixels = ersatzrs_web::host::teletext_authoring::preview_original_teletext_design(
        &fixture.state.application,
        ersatzrs_web::host::teletext_authoring::TeletextAuthoringPreviewRequest {
            channel_id: 1,
            design_id: root.design_id,
            revision: root.revision,
            owner: Some(owner),
        },
        designs::deadline(),
    )
    .await
    .unwrap();
    assert_ne!(&pixels, original_pixels.raster().unwrap());
    assert!(pixels.pixels().iter().any(|pixel| matches!(
        pixel,
        TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(colour))
            if colour.get() == [0, 0, 15]
    )));
    drop(original_pixels);
    assert!(pixels.diagnostics().is_empty());
    // Import and preview do not change requests, accepted generations or profiles.
    assert_eq!(scoped::stable_state(&fixture).await, before);
    fixture = fixture.reopen().await;
    assert_eq!(units(&fixture, &stored).await, first_units);
    assert_eq!(legacy_pixels(&fixture, &first, owner).await, pixels);
    let latest = scoped::export(&fixture, &stored).await;
    assert_unchanged_latest(&fixture, &latest, 2).await;
    let second = scoped::import(&fixture, edit_foreground(&latest, 2, 0, 4, 2)).await;
    let stored = revision(&fixture, &second).await;
    assert_eq!(stored.revision, 3);
    assert_root_context(root, &stored);
    // The incoming baseline was colour 4, but storage remains anchored to root 18.
    assert_operand(&stored, 2);
    let latest = scoped::export(&fixture, &stored).await;
    let reverted = scoped::import(&fixture, edit_foreground(&latest, 3, 0, 2, 18)).await;
    let stored = revision(&fixture, &reverted).await;
    assert_eq!(stored.revision, 4);
    assert_eq!(stored.document, root.document);
    assert_eq!(stored.digest, root.digest);
    assert!(overlay(&stored).is_none());
    assert_eq!(units(&fixture, &stored).await, original_units);
    fixture = fixture.reopen().await;
    assert_eq!(units(&fixture, &stored).await, original_units);
    let latest = scoped::export(&fixture, &stored).await;
    assert_unchanged_latest(&fixture, &latest, 4).await;
    assert_eq!(scoped::stable_state(&fixture).await, before);
    assert_eq!(designs::config(&fixture).await, configuration);
    assert_eq!(
        std::fs::read(fixture.root.path().join("authoring-calls")).unwrap(),
        calls
    );
    assert_eq!(units(&fixture, &flow.originals[1]).await, other_units);
}

#[tokio::test]
async fn native_operand_import_rejects_generated_and_rightward_protected_effects_without_head_change(
) {
    for (response, occurrence) in [
        (response as fn(&str, u16, u16) -> Value, 1),
        (narrow_response as fn(&str, u16, u16) -> Value, 0),
    ] {
        let (fixture, flow) = editorial_imports::setup_with_response(response).await;
        let exported = scoped::export(&fixture, &flow.originals[0]).await;
        let before = scoped::stable_state(&fixture).await;
        let revisions: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "TeletextDesignRevision""#)
            .fetch_one(&fixture.state.pool)
            .await
            .unwrap();
        let heads: Vec<(i64, i64)> =
            sqlx::query_as(r#"SELECT "Id", "Revision" FROM "TeletextDesign" ORDER BY "Id""#)
                .fetch_all(&fixture.state.pool)
                .await
                .unwrap();
        let result = import_teletext_editorial_design(
            &fixture.state.application,
            edit_foreground(&exported, 1, occurrence, 18, 4),
            editorial_imports::policy(),
            designs::deadline(),
        )
        .await;
        assert!(matches!(
            result,
            Err(NativeTeletextEditorialImportError::Effects(_))
        ));
        assert_eq!(scoped::stable_state(&fixture).await, before);
        assert_eq!(
            sqlx::query_as::<_, (i64, i64)>(
                r#"SELECT "Id", "Revision" FROM "TeletextDesign" ORDER BY "Id""#
            )
            .fetch_all(&fixture.state.pool)
            .await
            .unwrap(),
            heads
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(r#"SELECT COUNT(*) FROM "TeletextDesignRevision""#)
                .fetch_one(&fixture.state.pool)
                .await
                .unwrap(),
            revisions
        );
    }
}
