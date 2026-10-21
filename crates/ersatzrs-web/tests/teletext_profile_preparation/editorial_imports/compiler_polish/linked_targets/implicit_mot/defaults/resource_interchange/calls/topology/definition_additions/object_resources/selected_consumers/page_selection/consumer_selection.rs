//! Main and Local consumers cross one actual concept/publication/reopen boundary.
use super::*;
use ersatzrs_domain::{
    TeletextLocalObjectGlyphProgramOverlay, TeletextMainResourceInvocationOverlay,
};
use ersatzrs_web::host::teletext_editorial_designs::{
    select_teletext_editorial_object_consumers, NativeTeletextObjectConsumersSelectRequest,
};
#[path = "consumer_selection/actual_admin.rs"]
mod actual_admin;
#[path = "consumer_selection/cross_roles.rs"]
mod cross_roles;
#[path = "consumer_selection/lifecycle.rs"]
mod lifecycle;
#[path = "consumer_selection/source.rs"]
mod source;

fn active() -> TeletextObjectIdentity {
    TeletextObjectIdentity::global(resources()[0], TeletextObjectId::new(33)).unwrap()
}
fn old_passive() -> TeletextObjectIdentity {
    TeletextObjectIdentity::global(
        seed::target(20).resource().unwrap(),
        TeletextObjectId::new(34),
    )
    .unwrap()
}
fn local(page: u16) -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(
        channel_documents::address(page, 32),
        TeletextObjectId::new(65000),
    )
}
fn body(
    service: &TeletextDeclaredService,
    id: TeletextObjectIdentity,
) -> TeletextEnhancementProgram {
    service
        .semantic()
        .objects()
        .definition(id)
        .unwrap()
        .program
        .clone()
}
fn local_programs(
    before: &BankExpected,
    after: &BankExpected,
) -> Vec<TeletextLocalObjectGlyphProgramOverlay> {
    (124..127)
        .map(|page| {
            let expected = body(&before.declared, local(page));
            TeletextLocalObjectGlyphProgramOverlay::new_with_invocation_changes(
                local(page),
                expected.clone(),
                body(&after.declared, local(page)),
                (0..expected.commands().len()).map(Some).collect(),
            )
            .unwrap()
        })
        .collect()
}
fn resource_programs(
    before: &BankExpected,
    after: &BankExpected,
) -> Vec<TeletextResourceObjectProgramOverlay> {
    let expected = body(&before.declared, active());
    vec![
        TeletextResourceObjectProgramOverlay::new_with_invocation_changes(
            active(),
            expected.clone(),
            body(&after.declared, active()),
            (0..expected.commands().len()).map(Some).collect(),
            seed::encoding().composition.service,
        )
        .unwrap(),
    ]
}
fn main_programs(
    before: &BankExpected,
    after: &BankExpected,
) -> Vec<TeletextMainResourceInvocationOverlay> {
    (124..127)
        .map(|page| {
            let owner = channel_documents::address(page, 32);
            let command = |service: &TeletextDeclaredService| {
                service.semantic().page_enhancements()[&owner]
                    .local_program
                    .as_ref()
                    .unwrap()
                    .commands()[0]
            };
            TeletextMainResourceInvocationOverlay::new(
                owner,
                0,
                command(&before.declared),
                command(&after.declared),
            )
            .unwrap()
        })
        .collect()
}
fn request(
    current: &TeletextDesignExportReceipt,
    head: i64,
    before: &BankExpected,
    after: &BankExpected,
    selected: bool,
) -> NativeTeletextObjectConsumersSelectRequest {
    NativeTeletextObjectConsumersSelectRequest {
        name: "Coordinated Main Local and resource consumers".into(),
        receipt_id: current.metadata.id,
        receipt_digest: current.metadata.digest.clone(),
        expected_design_head: head,
        selections: selections(&before.declared, selected),
        programs: resource_programs(before, after),
        local_programs: local_programs(before, after),
        main_resource_invocations: main_programs(before, after),
    }
}
async fn select_all(
    fixture: &Fixture,
    concept: &SavedEditorialTeletextDesign,
    before: &BankExpected,
    after: &BankExpected,
    selected: bool,
) -> SavedEditorialTeletextDesign {
    let exported = drcs_bitmap::reexport(fixture, concept).await;
    let current = receipt(fixture, &exported).await;
    let result = select_teletext_editorial_object_consumers(
        &fixture.state.application,
        request(&current, concept.head().revision, before, after, selected),
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    let TeletextDesignMutation::Saved(saved) = result else {
        panic!("explicit complete consumer transaction saves a concept");
    };
    saved
}
async fn check(
    fixture: &Fixture,
    flow: &Flow,
    design_id: i64,
    revision: i64,
    expected: &BankExpected,
    selected: bool,
) -> TeletextDesignRevision {
    let stored = fixture
        .state
        .application
        .teletext_design_revision(design_id, revision, designs::budget().max_serialized_bytes)
        .await
        .unwrap()
        .unwrap();
    let decoded = decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
    if selected {
        let overlay = decoded.editorial_overlay().unwrap();
        assert_eq!(overlay.schema_version(), 28);
        assert_eq!(overlay.main_resource_invocations().len(), 3);
        assert_eq!(overlay.object_page_selections().len(), 3);
        assert_eq!(overlay.resource_object_program_overlays().len(), 1);
        assert_eq!(overlay.local_object_glyph_program_overlays().len(), 3);
    } else {
        assert!(
            decoded.editorial_overlay().is_none(),
            "exact root undo removes every consumer intent"
        );
    }
    let restored = reload_original_teletext_design_revision(
        &fixture.state.application,
        design_id,
        revision,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    let original = flow.saved[0].candidate();
    assert_eq!(restored.selection(), original.selection());
    assert_eq!(restored.metadata(), original.metadata());
    assert_eq!(restored.transmissions(), original.transmissions());
    assert_eq!(restored.sources().len(), original.sources().len());
    for (actual, source) in restored.sources().iter().zip(original.sources()) {
        assert_eq!(actual.bytes(), source.bytes());
        assert_eq!(actual.authoring(), source.authoring());
    }
    assert_bank_output(restored.encoded().unwrap(), expected);
    let preview = preview_original_teletext_design(
        &fixture.state.application,
        TeletextAuthoringPreviewRequest {
            channel_id: 1,
            design_id,
            revision,
            owner: Some(channel_documents::address(124, 32)),
        },
        designs::deadline(),
    )
    .await
    .unwrap();
    assert_eq!(preview.raster(), Some(&expected.rasters[0]));
    assert_eq!(preview.object_choices().main_calls.len(), 3);
    assert_eq!(
        preview
            .object_choices()
            .calls
            .iter()
            .filter(|call| call.caller.owner().is_some())
            .count(),
        3
    );
    stored
}
