//! Authenticated Main+RightPanel permissions through the public native importer.
//! Prepared bitmap bytes only; no actual external editor or ordinary/client proof.

use super::*;
use ersatzrs_domain::{TeletextCandidatePlacement, TeletextRawDisplayArea};
use ersatzrs_web::host::teletext_editorial_designs::{
    NativeTeletextEditorialImportError, TeletextEditorialEffectsFailure,
};

#[derive(Clone, Copy)]
pub(in super::super) enum Case {
    MainOnly,
    MainAndRight,
}

impl Case {
    pub(in super::super) fn response(self) -> fn(&str, u16, u16) -> Value {
        match self {
            Self::MainOnly => super::response,
            Self::MainAndRight => response,
        }
    }

    pub(in super::super) fn assert_pixels(
        self,
        original: &EncodedTeletextAuthoredService,
        edited: &EncodedTeletextAuthoredService,
    ) {
        match self {
            Self::MainOnly => assert_only_glyph_pixels(original, edited),
            Self::MainAndRight => assert_panel_pixels(original, edited),
        }
    }

    pub(in super::super) async fn originals_and_retained(
        self,
        fixture: &Fixture,
        flow: &Flow,
    ) -> Option<resource_bank_admin::Retained> {
        if matches!(self, Self::MainOnly) {
            return None;
        }
        assert_originals(flow);
        Some(resource_bank_admin::Retained::capture(fixture).await)
    }
}

fn response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = super::response(theme, page, subcode);
    let mut objects = Vec::new();
    for metadata in value["service"]["page_enhancements"]
        .as_array_mut()
        .unwrap()
    {
        let owner = metadata["address"].clone();
        let target = json!({"scope":"local","owner":owner,"object_id":65520});
        let commands = metadata["local_program"]["commands"]
            .as_array_mut()
            .unwrap();
        assert_eq!(commands[0]["position"], json!({"row":1,"column":8}));
        assert_eq!(commands.last().unwrap()["operation"]["kind"], "drcs");
        commands.push(json!({"kind":"invoke_object","target":target,
            "object_type":"passive","offset":{"row":0,"column":32}}));
        objects.push(
            json!({"identity":target,"object_type":"passive","program":{"commands":[
                {"kind":"at_column","column":0,"operation":{"kind":"foreground","colour":18}},
                {"kind":"at_column","column":0,"operation":{"kind":"background","colour":0}},
                {"kind":"at_column","column":0,"operation":{"kind":"drcs",
                    "reference":{"table":"normal","subtable":0,"slot":0}}}
            ]}}),
        );
    }
    value["service"]["objects"]
        .as_array_mut()
        .unwrap()
        .extend(objects);
    for metadata in value["authoring"]["pages"].as_array_mut().unwrap() {
        metadata["regions"].as_array_mut().unwrap().push(json!({
            "id":"free-right-glyph","kind":"free","area":"right_panel",
            "row":1,"column":0,"width":1,"height":1}));
    }
    value
}

fn protected_response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = response(theme, page, subcode);
    for metadata in value["authoring"]["pages"].as_array_mut().unwrap() {
        metadata["regions"]
            .as_array_mut()
            .unwrap()
            .retain(|region| region["id"] != "free-right-glyph");
    }
    value
}

fn composition(
    service: &EncodedTeletextAuthoredService,
    page: u16,
) -> &ersatzrs_ffmpeg::teletext::TeletextMainComposition {
    service
        .pages()
        .iter()
        .find_map(|candidate| match candidate {
            EncodedTeletextAuthoredPage::NativeDisplay(value)
                if value.encoded().page().address().page.get() == page =>
            {
                Some(value.composition())
            }
            EncodedTeletextAuthoredPage::CompiledDisplay(value)
                if value.page().address().page.get() == page =>
            {
                Some(value.normalized().original())
            }
            _ => None,
        })
        .unwrap()
}

fn assert_originals(flow: &Flow) {
    let day = &flow.accepted[0].selection().bindings[0];
    let night = &flow.accepted[1].selection().bindings[0];
    assert_eq!(day.addon_id, night.addon_id);
    assert_ne!(day.profile_id, night.profile_id);
    for accepted in &flow.accepted {
        let service = accepted.service();
        assert!(service.compiler_drcs_resources().is_empty());
        for page in [124, 125] {
            let original = composition(service, page);
            let geometry = original.display_geometry();
            assert_eq!(
                (
                    geometry.left_width(),
                    geometry.right_width(),
                    geometry.total_width()
                ),
                (4, 12, 56)
            );
            assert!(geometry
                .records()
                .iter()
                .any(|record| record.candidate().position() == (1, 40)
                    && record.area() == TeletextRawDisplayArea::RightPanel
                    && record.placement()
                        == TeletextCandidatePlacement::Placed { row: 1, column: 44 }));
            for column in [12, 44] {
                let cell = original.display_resolution().cell(1, column).unwrap();
                let index = original.display_resolution().origins()[cell.origin().unwrap()]
                    .candidate_index();
                let executed = original
                    .drcs_execution()
                    .records()
                    .iter()
                    .find(|record| record.candidate_index() == index)
                    .unwrap();
                assert_eq!(executed.resource().kind(), TeletextResourceKind::Drcs);
                assert_eq!(
                    executed.resource().page(),
                    TeletextRawDataPageNumber::new(1, 0xc0).unwrap()
                );
                assert_eq!(executed.resource().logical_subpage().get(), 0);
                assert_eq!(executed.reference().slot.get(), 0);
                assert_eq!(executed.bitmap().get()[0], 0x801);
            }
        }
    }
    for saved in &flow.saved {
        for source in saved.candidate().sources() {
            let value: Value = serde_json::from_slice(source.bytes()).unwrap();
            assert_eq!(
                serde_json::to_value(source.authoring().unwrap()).unwrap(),
                value["authoring"]
            );
            for metadata in value["authoring"]["pages"].as_array().unwrap() {
                assert!(metadata["regions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|region| region["id"] == "free-right-glyph"
                        && region["area"] == "right_panel"
                        && region["column"] == 0
                        && region["width"] == 1));
            }
        }
    }
}

fn assert_panel_pixels(
    original: &EncodedTeletextAuthoredService,
    edited: &EncodedTeletextAuthoredService,
) {
    assert_eq!(original.magazine_units(), edited.magazine_units());
    assert_eq!(original.inventory(), edited.inventory());
    for page in [124, 125] {
        let before = external_editor_probe::preview_page(original, page);
        let after = external_editor_probe::preview_page(edited, page);
        assert_eq!((before.width(), before.height()), (56 * 12, 25 * 10));
        assert_eq!(
            (before.width(), before.height()),
            (after.width(), after.height())
        );
        let changed: Vec<_> = before
            .pixels()
            .iter()
            .zip(after.pixels())
            .enumerate()
            .filter_map(|(index, (a, b))| (a != b).then_some(index))
            .collect();
        assert_eq!(
            changed,
            [
                (4 + 8) * 12 + 10 + before.width() * 10,
                (4 + 40) * 12 + 10 + before.width() * 10
            ]
        );
        for index in changed {
            assert!(matches!(after.pixels()[index],TeletextRasterPixel::Colour(
                TeletextColourPixel::Rgb(rgb)) if rgb.get() == [15,0,0]));
        }
    }
    assert_eq!(original.pages().len(), edited.pages().len());
    for (old, new) in original.pages().iter().zip(edited.pages()) {
        assert_eq!(old.resource_header(), new.resource_header());
        if old
            .resource_header()
            .is_some_and(|header| header.identity().kind() == TeletextResourceKind::Drcs)
        {
            assert_ne!(old.data_units(), new.data_units());
        } else {
            assert_eq!(old.data_units(), new.data_units());
        }
    }
}

pub(super) async fn assert_retained(
    retained: Option<&resource_bank_admin::Retained>,
    fixture: &Fixture,
    flow: &Flow,
) {
    if let Some(retained) = retained {
        retained.assert_unchanged(fixture, flow).await;
    }
}

#[tokio::test]
async fn fresh_v4_panel_zone_bitmap_import_preview_publication_reopen_repeat_and_undo() {
    Box::pin(shared_case(false, None, Case::MainAndRight)).await;
}

#[tokio::test]
async fn fresh_v4_shared_panel_bitmap_without_captured_panel_zone_is_rejected() {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
        protected_response,
        Fixture::new_persistent().await,
        drcs_companion::encoding_policy,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let request = bitmap_request(&flow.receipts[0], 1, b'C');
    assert_eq!(
        request.sidecar,
        write_teletext_export_sidecar(flow.exported[0].sidecar(), policy().project).unwrap()
    );
    let error = rejected(&fixture, &flow, request).await;
    let NativeTeletextEditorialImportError::Effects(error) = error else {
        panic!("protected right-panel effect must reject: {error:?}")
    };
    assert_eq!(
        error.reason,
        TeletextEditorialEffectsFailure::ProtectedDisplay
    );
    assert_eq!(error.page, Some(channel_documents::address(124, 32)));
    assert_eq!(error.position, Some((1, 44)));
    retained.assert_unchanged(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    fixture.state.pool.close().await;
}

#[tokio::test]
#[ignore = "requires the pinned retained actual singleton QTM bank input and Save As output"]
async fn retained_actual_qtm_panel_bitmap_import_preview_publication_reopen_repeat_and_undo() {
    let directory =
        super::super::super::client_capture::selected("ERSATZRS_QTM_DRCS_CALIBRATION_DIR");
    let directory = std::path::PathBuf::from(directory);
    Box::pin(shared_case(false, Some(&directory), Case::MainAndRight)).await;
}
