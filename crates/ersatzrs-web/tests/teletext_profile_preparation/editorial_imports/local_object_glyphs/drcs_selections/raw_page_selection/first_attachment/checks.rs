//! Both independently authored complete factories precede native mutation claims.
use super::*;

fn raster(service: &EncodedTeletextAuthoredService, page: u16) -> TeletextRaster {
    if page != source::display().page.get() {
        return external_editor_probe::preview_page(service, page);
    }
    // The retained two-display helper has a fixed124/125 navigation oracle.
    // The new display copies original124's navigation to125, not125's to124.
    let preview = project_teletext_preview(
        service,
        source::display(),
        TeletextColourPixelOptions {
            reveal_concealed: false,
            flash_phase: TeletextFlashPhase::Original,
        },
        preview_budget(),
    )
    .unwrap();
    assert!(preview.raster().diagnostics().is_empty());
    assert_eq!(preview.navigation()[0].target.page.get(), 125);
    preview.raster().clone()
}

pub(super) fn same(
    actual: &EncodedTeletextAuthoredService,
    expected: &EncodedTeletextAuthoredService,
) {
    assert_eq!(actual.declared(), expected.declared());
    assert_eq!(designs::units(actual), designs::units(expected));
    assert_eq!(actual.magazine_units(), expected.magazine_units());
    for page in [124, 125, 227] {
        let preview = project_teletext_preview(
            actual,
            channel_documents::address(page, 32),
            TeletextColourPixelOptions {
                reveal_concealed: false,
                flash_phase: TeletextFlashPhase::Original,
            },
            preview_budget(),
        )
        .unwrap();
        assert!(preview.raster().diagnostics().is_empty());
        assert_eq!(preview.raster(), &raster(expected, page));
    }
}
pub(super) fn qualified(before: &Flow, after: &Flow, explicit: bool) {
    for index in 0..2 {
        let old = before.accepted[index].service();
        let new = after.accepted[index].service();
        let old_metadata = &old.declared().semantic().page_enhancements()[&source::display()];
        assert_eq!(old_metadata.compositional_links.is_some(), explicit);
        let old_resolution = old
            .declared()
            .resolutions()
            .iter()
            .find(|v| v.display() == source::display())
            .unwrap();
        let new_resolution = new
            .declared()
            .resolutions()
            .iter()
            .find(|v| v.display() == source::display())
            .unwrap();
        assert!(old_resolution.mot().is_none());
        assert!(new_resolution.mot().is_none());
        for role in TeletextX27LinkRole::ALL {
            assert_eq!(
                *old_resolution.role(role),
                TeletextMotRoleResolution::Unselected
            );
            assert!(
                matches!(*new_resolution.role(role), TeletextMotRoleResolution::Selected { target, .. } if target == source::target(role))
            );
            assert!(
                TeletextObjectPageSelectionChange::new(
                    source::display(),
                    role,
                    old_metadata.compositional_links,
                    *old_resolution.role(role),
                    source::declaration(role)
                )
                .is_err(),
                "historical selected-role constructor retains its original refusal"
            );
        }
        assert_eq!(
            old.declared().semantic().base(),
            new.declared().semantic().base()
        );
        assert_eq!(
            old.declared().semantic().objects(),
            new.declared().semantic().objects()
        );
        assert_eq!(
            old.declared().semantic().drcs(),
            new.declared().semantic().drcs()
        );
        assert_eq!(
            old.declared().resource_inventory(),
            new.declared().resource_inventory()
        );
        assert_eq!(old.declared().mots(), new.declared().mots());
        assert_eq!(old.declared().defaults(), new.declared().defaults());
        assert_eq!(old.declared().selections(), new.declared().selections());
        assert_eq!(old.magazine_units(), new.magazine_units());
        assert_eq!(old.pages().len(), new.pages().len());
        let mut changed = 0;
        for (old_page, new_page) in old.pages().iter().zip(new.pages()) {
            assert_eq!(old_page.resource_header(), new_page.resource_header());
            if old_page.data_units() != new_page.data_units() {
                changed += 1;
                assert!(matches!(new_page,
                    EncodedTeletextAuthoredPage::NativeDisplay(page) if page.encoded().page().address() == source::display()));
            }
        }
        assert_eq!(changed, 1, "only the new display's X27 data units change");
        for page in [124, 125, 227] {
            assert_eq!(
                raster(old, page),
                raster(new, page),
                "first attachment grants a bank, never inserts a consumer or changes pixels"
            );
        }
    }
}
pub(super) async fn concept(
    fixture: &Fixture,
    flow: &Flow,
    saved: &SavedEditorialTeletextDesign,
    expected: &EncodedTeletextAuthoredService,
    selections: &[TeletextSavedObjectPageSelectionChange],
) {
    let stored = drcs_bitmap::stored(fixture, saved).await;
    let decoded = decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
    if selections.is_empty() {
        assert!(decoded.editorial_overlay().is_none());
        assert_eq!(stored.document, flow.originals[0].document);
    } else {
        let overlay = decoded.editorial_overlay().unwrap();
        assert_eq!(overlay.schema_version(), 31);
        assert_eq!(overlay.object_page_selections(), selections);
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
    }
    let current = drcs_bitmap::restored(fixture, saved).await;
    let original = flow.saved[0].candidate();
    assert_eq!(current.selection(), original.selection());
    assert_eq!(current.metadata(), original.metadata());
    assert_eq!(current.transmissions(), original.transmissions());
    for (actual, expected) in current.sources().iter().zip(original.sources()) {
        assert_eq!(actual.bytes(), expected.bytes());
        assert_eq!(actual.authoring(), expected.authoring());
    }
    assert_eq!(current.sources().len(), original.sources().len());
    assert_eq!(
        current.producers()[0].original(),
        original.producers()[0].original()
    );
    same(current.encoded().unwrap(), expected);
}
