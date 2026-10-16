//! A declarative separated mosaic leaves a raw latch in the original G0 text.
//! Genuine Save As still changes only A7 to literal5B; permissions stay exact.

use super::compiler_polish_underline as underline;
use super::*;
use ersatzrs_domain::teletext::*;
use serde_json::{json, Value};

pub(super) fn response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = underline::response(theme, page, subcode);
    if theme != "day" || page != 124 {
        return value;
    }
    let authoring = value["authoring"].clone();
    let runs = value["service"]["pages"][0]["rows"][0]["runs"]
        .as_array_mut()
        .unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0]["column"], 7);
    assert_eq!(runs[0]["content"], json!({"kind":"text","text":"A"}));
    let style = runs[0]["style"].clone();
    runs.insert(0, json!({"column":3,"content":{"kind":"mosaics","cells":[63],"separated":true},"style":style}));
    assert_eq!(value["authoring"], authoring);
    value
}

pub(super) fn assert_originals(flow: &Flow) {
    underline::assert_originals(flow);
    assert_eq!(
        response("night", 124, 32),
        underline::response("night", 124, 32)
    );
    let service = flow.accepted[0].service();
    let page = service
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::CompiledDisplay(page)
                if page.page().address() == channel_documents::address(124, 32) =>
            {
                Some(page.page())
            }
            _ => None,
        })
        .unwrap();
    let projection = project_encoded_teletext_display(page, 128).unwrap();
    let cells = projection
        .records
        .iter()
        .find_map(|record| match record {
            TeletextEncodedDisplayRecord::BaseRow { row: 1, cells } => Some(cells),
            _ => None,
        })
        .unwrap();
    for (column, code) in [(1, 0x17), (2, 0x1a), (3, 0x7f), (6, 0x07), (7, b'A')] {
        assert_eq!(cells[column], code, "original Base row1 column{column}");
    }
    let composition = polish_carrier::composition(service);
    let left = composition.display_geometry().left_width();
    for column in [left + 7, left + 8] {
        let attrs = composition
            .display_resolution()
            .cell(1, column)
            .unwrap()
            .attributes();
        assert_eq!(attrs.spacing_separated().value(), Some(true));
        assert_eq!(attrs.underline_or_separated_mosaics().value(), Some(true));
    }
    assert_mosaic(service);
}

pub(super) fn assert_mosaic(service: &ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService) {
    let composition = polish_carrier::composition(service);
    let column = composition.display_geometry().left_width() + 3;
    let cell = composition.display_resolution().cell(1, column).unwrap();
    assert_eq!(cell.attributes().spacing_separated().value(), Some(true));
    assert_eq!(
        cell.attributes().underline_or_separated_mosaics().value(),
        Some(false)
    );
    let origin = cell.origin().unwrap();
    let candidate = composition.display_resolution().origins()[origin].candidate_index();
    assert_eq!(
        composition.glyph_layers().candidates()[candidate].priority(),
        TeletextGlyphPriority::Base
    );
    assert_eq!(
        composition.glyph_intent().records()[candidate].glyph(),
        Some(TeletextGlyphIdentity::G1Mosaic {
            mosaic: TeletextMosaic::new(63).unwrap()
        })
    );
    let glyph =
        ersatzrs_ffmpeg::teletext::project_teletext_comparison_glyph(composition, cell).unwrap();
    // Existing independent six-block geometry golden: separation blanks rows
    // 0/3/7 and keeps left0x780 plus right0x01E in each occupied band.
    let mask = [0_u16, 0x79e, 0x79e, 0, 0x79e, 0x79e, 0x79e, 0, 0x79e, 0x79e];
    assert_eq!(glyph.mask.unwrap().rows(), mask);
    assert_eq!(
        glyph.effects,
        ersatzrs_ffmpeg::teletext::TeletextComparisonGlyphEffects::Mosaic { separated: true }
    );
    let raster = external_editor_probe::preview_page(service, 124);
    for (row, mask) in mask.into_iter().enumerate() {
        for x in 0..12 {
            let expected = if mask & (1 << (11 - x)) == 0 {
                [0, 0, 0]
            } else {
                [15, 15, 15]
            };
            assert!(
                matches!(raster.pixel(usize::from(column)*12+x,10+row),Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(value))) if value.get()==expected),
                "protected G1 pixel{x},{row}"
            );
        }
    }
}

#[tokio::test]
async fn no_bank_polish_spacing_concept_previews_publishes_and_reopens() {
    Box::pin(underline::run(response, assert_originals, None)).await;
}

#[tokio::test]
#[ignore = "requires fresh original spacing-latched no-bank export, actual pinned QTM Save As and editor-proof.json; no ordinary/client claim"]
async fn actual_qtm_no_bank_polish_spacing_edit_previews_publishes_and_reopens() {
    let directory =
        std::path::PathBuf::from(client_capture::selected("ERSATZRS_QTM_COMPILER_POLISH_DIR"));
    Box::pin(underline::run(response, assert_originals, Some(&directory))).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires fresh spacing-latched no-bank actual QTM Save As and FFmpeg9/ffprobe/TSDuck; independent decoder/client proof is separate"]
async fn actual_qtm_no_bank_polish_spacing_concept_serves_two_ordinary_channels() {
    Box::pin(compiler_polish_underline_delivery::run(
        response,
        assert_originals,
    ))
    .await;
}
