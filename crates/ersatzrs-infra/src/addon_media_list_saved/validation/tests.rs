use super::*;

use serde_json::json;

pub(super) fn fixture() -> (SavedListDocument, AddonManifest) {
    let mut document = crate::addon_media_list_saved::model::tests::fixture();
    let publication = document.published.as_mut().unwrap();
    publication
        .files
        .push(publication.items[0].subtitles[0].file.clone());
    let installed = serde_json::from_value(json!({
        "manifest_version": 5, "id": document.addon_id, "version": "1.0.0",
        "name": {"en-US": "Saved list fixture"},
        "summary": {"en-US": "Synthetic receiving validation"},
        "license": "MIT", "source_url": "https://example.invalid/source",
        "host_version": "*", "entrypoints": {}, "capabilities": []
    }))
    .unwrap();
    (document, installed)
}

#[test]
fn saved_content_validation_preserves_independent_states_and_unbound_options() {
    let (mut document, installed) = fixture();
    document
        .draft
        .options
        .item_values
        .insert("removed-owner".into(), std::collections::BTreeMap::new());
    let before = document.clone();
    validate_content(&document, &installed).unwrap();
    assert_eq!(document, before);
    assert_ne!(
        document.draft.items[0].content.provider_id,
        document.published.as_ref().unwrap().items[0]
            .content
            .provider_id
    );
}

#[test]
fn saved_content_validation_rejects_mismatched_identity_and_missing_cohort_members() {
    let (document, installed) = fixture();
    let mut different = installed.clone();
    different.id = "org.example.other".into();
    assert_eq!(
        validate_content(&document, &different),
        Err(SavedContentError::Identity)
    );
    let mut changed = document.clone();
    changed.draft.delivery.before_cutoff.clear();
    assert_eq!(
        validate_content(&changed, &installed),
        Err(SavedContentError::Items)
    );
    let mut changed = document.clone();
    changed
        .draft
        .options
        .item_values
        .insert(" ".into(), std::collections::BTreeMap::new());
    assert_eq!(
        validate_content(&changed, &installed),
        Err(SavedContentError::Items)
    );
    let mut changed = document;
    changed.version += 1;
    assert_eq!(
        validate_content(&changed, &installed),
        Err(SavedContentError::Document)
    );
}

#[test]
fn saved_content_validation_keeps_generated_identity_lengths() {
    let (mut document, installed) = fixture();
    let source = "s".repeat(384);
    let generated = format!("{source}:chapter:12");
    let mut parent = document.draft.items[0].clone();
    parent.content.provider_id = source.clone();
    document.draft.items[0].content.provider_id = generated.clone();
    document.draft.items[0].content.parent_provider_id = Some(source.clone());
    document.draft.items[0].content.fragment_start_seconds = Some(12);
    document.draft.items.push(parent);
    document
        .draft
        .delivery
        .before_cutoff
        .extend([source, generated]);
    validate_content(&document, &installed).unwrap();
}
