use super::*;
use crate::addon_media_list_saved::validation::tests::fixture;
use ersatzrs_core::repositories::AddonMediaListMetadataConflict;
use serde_json::json;

fn image(path: &str, hash: char) -> SavedFile {
    SavedFile {
        relative_path: path.into(),
        sha256: hash.to_string().repeat(64),
        byte_length: 3,
    }
}

#[test]
fn saved_metadata_validation_preserves_operator_fields_without_wire_limits() {
    let (mut document, _) = fixture();
    document.draft.metadata.provider = serde_json::from_value(json!({"title": 123})).unwrap();
    document.draft.metadata.overrides = serde_json::from_value(json!({
        "title": "Operator title", "year": 3000, "plot": "x".repeat(256 * 1024 + 1),
        "artwork_urls": ["not a publishable image URL yet"],
        "legacy_extension": {"retained": true}
    }))
    .unwrap();
    let before = document.clone();
    validate_metadata(&document).unwrap();
    assert_eq!(document, before);
    document
        .draft
        .metadata
        .overrides
        .insert("genres".into(), json!("not an array"));
    assert_eq!(
        validate_metadata(&document),
        Err(SavedContentError::Metadata)
    );
}

#[test]
fn saved_metadata_validation_checks_each_layer_and_conflict_file_reference() {
    let (mut document, _) = fixture();
    document.draft.files = vec![
        image("old.png", 'a'),
        image("new.png", 'b'),
        image("before.png", 'c'),
    ];
    document.draft.metadata.provider =
        serde_json::from_value(json!({"artwork_urls": ["./old.png"]})).unwrap();
    document.draft.metadata.overrides =
        serde_json::from_value(json!({"artwork_urls": ["./new.png"]})).unwrap();
    document.draft.metadata.conflicts.insert(
        "artwork_urls".into(),
        AddonMediaListMetadataConflict {
            provider_before: json!(["./before.png"]),
            provider_after: json!(["./old.png"]),
            operator_value: json!(["./new.png"]),
        },
    );
    validate_metadata(&document).unwrap();
    document.draft.files.pop();
    assert_eq!(validate_metadata(&document), Err(SavedContentError::Files));
}

#[test]
fn saved_metadata_validation_checks_url_slots_not_person_names_or_roles() {
    let (mut document, _) = fixture();
    document.draft.metadata.overrides.insert(
        "people".into(),
        json!([
            {"name":"./Example", "role":"./Role", "order":null, "thumb":null}
        ]),
    );
    document.draft.metadata.conflicts.insert(
        "people".into(),
        AddonMediaListMetadataConflict {
            provider_before: json!([{"name":"./Before", "thumb":null}]),
            provider_after: json!([{"name":"./After", "thumb":null}]),
            operator_value: document.draft.metadata.overrides["people"].clone(),
        },
    );
    let before = document.clone();
    validate_metadata(&document).unwrap();
    assert_eq!(document, before);
    document.draft.metadata.overrides.get_mut("people").unwrap()[0]["thumb"] = json!("./actor.png");
    assert_eq!(validate_metadata(&document), Err(SavedContentError::Files));
    document.draft.files.push(image("actor.png", 'd'));
    validate_metadata(&document).unwrap();
}

#[test]
fn saved_metadata_validation_rejects_unsafe_conflicting_and_unlisted_paths() {
    let (mut document, _) = fixture();
    document.draft.files.push(image("poster.png", 'a'));
    document.draft.files.push(image("Poster.png", 'a'));
    assert_eq!(validate_metadata(&document), Err(SavedContentError::Files));
    document.draft.files.pop();
    document.draft.files.push(image("poster.png", 'b'));
    assert_eq!(validate_metadata(&document), Err(SavedContentError::Files));
    document.draft.files.pop();
    document.draft.files.push(image("../outside.png", 'b'));
    assert_eq!(validate_metadata(&document), Err(SavedContentError::Files));
    document.draft.files.pop();
    document
        .draft
        .metadata
        .overrides
        .insert("artwork_urls".into(), json!(["./unlisted.png"]));
    assert_eq!(validate_metadata(&document), Err(SavedContentError::Files));
    document
        .draft
        .metadata
        .overrides
        .insert("artwork_urls".into(), json!(["ersatzrs-upload://42"]));
    assert_eq!(validate_metadata(&document), Err(SavedContentError::Files));
}

#[test]
fn saved_metadata_validation_keeps_published_files_separate_and_checks_byte_identity() {
    let (mut document, _) = fixture();
    let publication = document.published.as_mut().unwrap();
    publication.files.push(image("poster.png", 'a'));
    publication
        .metadata
        .insert("artwork_urls".into(), json!(["./poster.png"]));
    document.draft.files.push(image("draft-poster.png", 'b'));
    document
        .draft
        .metadata
        .overrides
        .insert("artwork_urls".into(), json!(["./draft-poster.png"]));
    validate_metadata(&document).unwrap();
    document.draft.files.push(image("poster.png", 'b'));
    assert_eq!(validate_metadata(&document), Err(SavedContentError::Files));
    document.draft.files.pop();
    let SavedPlayback::DownloadAhead { file, .. } =
        &mut document.published.as_mut().unwrap().items[0].playback
    else {
        panic!("fixture");
    };
    file.byte_length += 1;
    assert_eq!(validate_metadata(&document), Err(SavedContentError::Files));
}
