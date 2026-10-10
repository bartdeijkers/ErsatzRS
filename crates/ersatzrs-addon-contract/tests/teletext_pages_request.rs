use ersatzrs_addon_contract::{TeletextPagesRequest, TeletextPagesRequestError};

// Independent public-wire example, not serialization of the production DTOs.
const REQUEST: &[u8] = br#"{
  "schema_version":1,"generation":" guide-epoch ","locale":"nl-NL",
  "timezone":"Europe/Amsterdam","captured_at":"2026-03-28T13:30:00.123456789+01:00",
  "window":{"start":"2026-03-27T23:00:00Z","finish":"2026-03-30T22:00:00Z"},
  "channels":[{"id":7,"number":"7","name":"Synthetic channel",
    "programmes":[{"identity":{"playout_id":11,"first_item_id":0},
      "start":"2026-03-28T12:00:00Z","stop":"2026-03-28T13:00:00Z",
      "title":"Guide <title> & caf\u00e9","custom_title":null,"sub_title":null,
      "description":null,"segments":[]}],
    "context":{"current":{"playout_id":11,"first_item_id":0},
      "next":null,"continuing":null,"later_today":[],"ambiguous":false}}]
}"#;

#[test]
fn public_request_preserves_literal_context_zero_identity_and_utc_precision() {
    let request = TeletextPagesRequest::from_json(REQUEST).unwrap();
    assert_eq!(request.generation, " guide-epoch ");
    assert_eq!(
        request.captured_at.to_rfc3339(),
        "2026-03-28T12:30:00.123456789+00:00"
    );
    assert_eq!(
        (request.window.finish - request.window.start).num_hours(),
        71
    );
    let programme = &request.channels[0].programmes[0];
    assert_eq!(programme.identity.first_item_id, 0);
    assert_eq!(programme.title, "Guide <title> & café");
    assert_eq!(programme.custom_title, None);
    assert_eq!(
        request.channels[0].context.current,
        Some(programme.identity)
    );
    assert_eq!(
        TeletextPagesRequest::from_json(&request.to_json().unwrap()).unwrap(),
        request
    );
}

#[test]
fn public_request_rejects_nested_duplicate_and_missing_nullable_metadata() {
    let raw = std::str::from_utf8(REQUEST).unwrap();
    let duplicate = raw.replace(
        "\"first_item_id\":0",
        "\"first_item_id\":0,\"first_item_id\":0",
    );
    assert_eq!(
        TeletextPagesRequest::from_json(duplicate.as_bytes()),
        Err(TeletextPagesRequestError::DuplicateKey)
    );
    let missing = raw.replace("\"description\":null,", "");
    assert_eq!(
        TeletextPagesRequest::from_json(missing.as_bytes()),
        Err(TeletextPagesRequestError::InvalidJson)
    );
}
