//! Bank-aware source, lifecycle, projection and exact role-root assertions.
use super::*;

pub(super) fn qualify(original: &Flow, banks: &Flow, attached: &Flow) {
    for index in 0..2 {
        let old = original.accepted[index].service();
        let added = banks.accepted[index].service();
        let selected = attached.accepted[index].service();
        assert_eq!(
            old.declared().semantic().base(),
            added.declared().semantic().base()
        );
        assert_eq!(
            old.declared().semantic().drcs(),
            added.declared().semantic().drcs()
        );
        assert_eq!(old.declared().mots(), added.declared().mots());
        assert_eq!(old.declared().defaults(), added.declared().defaults());
        assert_eq!(old.declared().selections(), added.declared().selections());
        assert_eq!(
            added.declared().semantic().objects(),
            selected.declared().semantic().objects()
        );
        assert_eq!(
            added.declared().resource_inventory(),
            selected.declared().resource_inventory()
        );
        assert_eq!(
            added.declared().semantic().objects().definitions().len(),
            old.declared().semantic().objects().definitions().len() + 2
        );
        for definition in old.declared().semantic().objects().definitions() {
            assert_eq!(
                added
                    .declared()
                    .semantic()
                    .objects()
                    .definition(definition.identity),
                Some(definition)
            );
        }
        let original_resolution = old
            .declared()
            .resolutions()
            .iter()
            .find(|resolution| resolution.display() == source::display())
            .unwrap();
        assert!(original_resolution.mot().is_none());
        assert!(TeletextX27LinkRole::ALL
            .into_iter()
            .all(|role| *original_resolution.role(role) == TeletextMotRoleResolution::Unselected));
        for (bank_index, resource) in factories::resources().into_iter().enumerate() {
            assert!(!old.declared().resource_inventory().contains(&resource));
            assert!(added.declared().resource_inventory().contains(&resource));
            assert_eq!(
                added
                    .declared()
                    .semantic()
                    .objects()
                    .definition(factories::definition(bank_index).identity),
                Some(&factories::definition(bank_index))
            );
        }
        for page in [124, 125, 227] {
            let render = |service| {
                project_teletext_preview(
                    service,
                    channel_documents::address(page, 32),
                    TeletextColourPixelOptions {
                        reveal_concealed: false,
                        flash_phase: TeletextFlashPhase::Original,
                    },
                    preview_budget(),
                )
                .unwrap()
                .raster()
                .clone()
            };
            assert_eq!(render(old), render(added));
            assert_eq!(
                render(old),
                render(selected),
                "links alone never insert consumers"
            );
        }
    }
}
pub(super) async fn concept(
    fixture: &Fixture,
    flow: &Flow,
    saved: &SavedEditorialTeletextDesign,
    expected: &EncodedTeletextAuthoredService,
    attached: bool,
) {
    let stored = drcs_bitmap::stored(fixture, saved).await;
    let decoded = decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
    let overlay = decoded.editorial_overlay().unwrap();
    assert_eq!(overlay.schema_version(), if attached { 31 } else { 25 });
    assert_eq!(overlay.object_resource_additions(), factories::additions());
    assert_eq!(
        overlay.object_page_selections().len(),
        if attached { 2 } else { 0 }
    );
    for selection in overlay.object_page_selections() {
        assert_eq!(selection.document_index(), 0);
        assert_eq!(selection.change().display(), source::display());
        assert_eq!(selection.change().expected_links(), None);
        assert_eq!(
            selection.change().expected_role(),
            TeletextMotRoleResolution::Unselected
        );
    }
    assert!(overlay.resource_object_definition_additions().is_empty());
    assert!(overlay.resource_object_program_overlays().is_empty());
    assert!(overlay.pages().is_empty());
    assert!(overlay.drcs_bitmap_overlays().is_empty());
    assert!(overlay.local_program_operands().is_empty());
    assert!(overlay.local_object_operands().is_empty());
    let mut document: Value = serde_json::from_slice(&stored.document).unwrap();
    document
        .as_object_mut()
        .unwrap()
        .remove("editorial_overlay");
    assert_eq!(
        document,
        serde_json::from_slice::<Value>(&flow.originals[0].document).unwrap()
    );
    let current = drcs_bitmap::restored(fixture, saved).await;
    let original = flow.saved[0].candidate();
    assert_eq!(current.selection(), original.selection());
    assert_eq!(current.metadata(), original.metadata());
    assert_eq!(current.sources().len(), original.sources().len());
    assert_eq!(current.producers().len(), original.producers().len());
    for (after, before) in current.sources().iter().zip(original.sources()) {
        assert_eq!(after.bytes(), before.bytes());
        assert_eq!(after.authoring(), before.authoring());
    }
    for (after, before) in current.producers().iter().zip(original.producers()) {
        assert_eq!(after.original(), before.original());
    }
    assert_eq!(
        current.transmissions().len(),
        original.transmissions().len() + 2
    );
    for transmission in original.transmissions() {
        assert!(current.transmissions().contains(transmission));
    }
    for resource in factories::resources() {
        assert!(current
            .transmissions()
            .contains(&factories::transmission(resource)));
    }
    checks::same(current.encoded().unwrap(), expected);
    if !attached {
        assert!(current
            .encoded()
            .unwrap()
            .declared()
            .semantic()
            .page_enhancements()[&source::display()]
            .compositional_links
            .is_none());
    }
    preview(fixture, saved, expected, attached).await;
}
async fn preview(
    fixture: &Fixture,
    saved: &SavedEditorialTeletextDesign,
    expected: &EncodedTeletextAuthoredService,
    attached: bool,
) {
    let preview = preview_original_teletext_design(
        &fixture.state.application,
        TeletextAuthoringPreviewRequest {
            channel_id: 1,
            design_id: saved.head().id,
            revision: saved.head().revision,
            owner: Some(source::display()),
        },
        designs::deadline(),
    )
    .await
    .unwrap();
    let raster = project_teletext_preview(
        expected,
        source::display(),
        TeletextColourPixelOptions {
            reveal_concealed: false,
            flash_phase: TeletextFlashPhase::Original,
        },
        preview_budget(),
    )
    .unwrap();
    assert_eq!(preview.raster(), Some(raster.raster()));
    for resource in factories::resources() {
        assert!(
            preview.object_resources().contains(&resource),
            "new bank is offered before attachment"
        );
        let matching =
            |choice: &ersatzrs_web::host::teletext_authoring::TeletextObjectPageRoleObservation| {
                choice.display == source::display() && choice.role == resource.kind()
            };
        assert_eq!(
            preview
                .object_choices()
                .unselected_roles
                .iter()
                .any(matching),
            !attached
        );
        assert_eq!(
            preview
                .object_choices()
                .restorable_attachments
                .iter()
                .any(matching),
            attached
        );
    }
}
