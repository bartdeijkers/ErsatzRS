//! Author context permission must not waive generated or out-of-zone glyphs.
use super::*;
use crate::host::teletext_editorial_effects::{
    validate_teletext_editorial_transition_with_contexts, SecondaryDefaultContextChange,
};

fn source(secondary: u8) -> (TeletextDeclaredService, TeletextPhysicalBaseOverlay) {
    let policy = policy();
    let original = declared(0, b'R', false);
    let pages = original
        .semantic()
        .page_enhancements()
        .values()
        .cloned()
        .map(|mut page| {
            page.local_program = None;
            if page.address == address(123) {
                page.presentation.as_mut().unwrap().secondary =
                    Some(TeletextPageCharsetDesignation::new(secondary).unwrap());
            }
            page
        })
        .collect();
    let semantic = TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base: original.semantic().base().clone(),
            pages,
            magazines: vec![],
            objects: vec![],
            drcs: vec![],
        },
        policy.validation.service,
    )
    .unwrap();
    let declared = TeletextDeclaredService::new(
        semantic,
        vec![],
        original.selections().to_vec(),
        vec![],
        policy.validation.declarations,
    )
    .unwrap();
    let mut cells = [b' '; 40];
    cells[8] = 0x1b;
    cells[9] = b'R';
    let overlay = overlay(1, cells);
    (
        declared
            .with_physical_base_overlays(vec![overlay.clone()], policy.validation.service)
            .unwrap(),
        overlay,
    )
}

#[test]
fn authenticated_context_still_rejects_protected_secondary_glyph_effects() {
    let (old, overlay) = source(0);
    let (new, _) = source(0x37);
    let before = old.semantic().page_enhancements()[&address(123)].presentation;
    let after = new.semantic().page_enhancements()[&address(123)].presentation;
    let encode = |value| {
        ersatzrs_ffmpeg::teletext::encode_teletext_authored_service_before(
            value,
            &[],
            &policy().encoding,
            deadline(),
        )
        .unwrap()
    };
    let (old, new) = (encode(old), encode(new));
    let changes = [SecondaryDefaultContextChange {
        owner: address(123),
        before,
        after,
    }];
    let overlays = [overlay];
    let edits = TeletextEditorialEdits {
        rows: &overlays,
        ..Default::default()
    };
    let free = project("free", 1, 8, 32, 1);
    // A raw editor request cannot authorize presentation changes even in Free.
    assert!(validate_teletext_editorial_transition_with_operands(
        &old,
        &new,
        &free,
        TeletextEditorialEdits::default(),
        edits,
        &policy().encoding,
        deadline()
    )
    .is_err());
    validate_teletext_editorial_transition_with_contexts(
        &old,
        &new,
        &free,
        &(TeletextEditorialEdits::default(), edits, &changes),
        &policy().encoding,
        deadline(),
    )
    .unwrap();
    for protected in [
        project("generated", 1, 8, 32, 1),
        project("free", 2, 8, 32, 1),
    ] {
        let failure = validate_teletext_editorial_transition_with_contexts(
            &old,
            &new,
            &protected,
            &(TeletextEditorialEdits::default(), edits, &changes),
            &policy().encoding,
            deadline(),
        )
        .unwrap_err();
        assert_eq!(failure.page, Some(address(123)));
        assert_eq!(failure.position, Some((1, 9)));
        assert_eq!(
            failure.reason,
            TeletextEditorialEffectsFailure::ProtectedDisplay
        );
    }
}

#[test]
fn inactive_panel_materialization_requires_exact_context_and_preserves_active_geometry() {
    use crate::host::teletext_editorial_effects::secondary_defaults::same_panels;
    let (source, _) = source(0);
    let defaults = source.semantic().page_enhancements()[&address(123)]
        .presentation
        .unwrap();
    let context = SecondaryDefaultContextChange {
        owner: address(123),
        before: None,
        after: Some(defaults),
    };
    assert_eq!(defaults.panels.level25_widths(), (0, 0));
    assert!(!same_panels(None, Some(defaults.panels), None));
    assert!(same_panels(None, Some(defaults.panels), Some(&context)));
    assert!(!same_panels(Some(defaults.panels), None, Some(&context)));
    let mut active = defaults;
    active.panels = TeletextSidePanels {
        left: true,
        right: false,
        status: TeletextPanelStatus::Level25And35,
        width: TeletextPanelWidth::new(8).unwrap(),
    };
    let changed = SecondaryDefaultContextChange {
        after: Some(active),
        ..context
    };
    assert!(!same_panels(None, Some(active.panels), Some(&changed)));
}
