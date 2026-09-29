use super::*;

fn context() -> YamlContext {
    YamlContext::new(
        Playout::default(),
        SchedulingReferenceData::default(),
        Utc::now(),
        BTreeMap::new(),
    )
}

#[test]
fn yaml_ambient_checkpoint_preserves_rolls_and_graphics() {
    for schedule in [None, Some("Christmas".to_owned())] {
        let mut original = context();
        original.switch_to_schedule(schedule.clone());
        original.instruction_index = 4;
        original.post_roll_sequence = Some("idents".to_owned());
        original.mid_roll_sequence = Some(("ads".to_owned(), "count > 1".to_owned()));
        original.graphics_elements.insert(7, None);
        original
            .graphics_elements
            .insert(8, Some(r#"{"title":"x"}"#.to_owned()));
        let anchor = PlayoutAnchor {
            next_start: original.current_time,
            context: Some(original.serialize().unwrap()),
            ..PlayoutAnchor::default()
        };
        let mut restored = context();
        restored.restore(&anchor).unwrap();
        assert_eq!(restored.instruction_index, 4);
        assert_eq!(restored.active_schedule, schedule);
        assert_eq!(restored.post_roll_sequence, original.post_roll_sequence);
        assert_eq!(restored.mid_roll_sequence, original.mid_roll_sequence);
        assert_eq!(restored.graphics_elements, original.graphics_elements);
    }
}

#[test]
fn yaml_ambient_empty_and_legacy_checkpoints_clear_stale_settings() {
    let original = context();
    let serialized = original.serialize().unwrap();
    let mut legacy: serde_json::Value = serde_json::from_str(&serialized).unwrap();
    for key in [
        "post_roll_sequence",
        "mid_roll_sequence",
        "graphics_elements",
    ] {
        legacy.as_object_mut().unwrap().remove(key);
    }
    for serialized in [serialized, legacy.to_string()] {
        let mut restored = context();
        restored.post_roll_sequence = Some("stale".to_owned());
        restored.mid_roll_sequence = Some(("stale".to_owned(), "true".to_owned()));
        restored.graphics_elements.insert(9, None);
        restored
            .restore(&PlayoutAnchor {
                next_start: original.current_time,
                context: Some(serialized),
                ..PlayoutAnchor::default()
            })
            .unwrap();
        assert!(restored.post_roll_sequence.is_none());
        assert!(restored.mid_roll_sequence.is_none());
        assert!(restored.graphics_elements.is_empty());
    }
}

#[test]
fn yaml_ambient_continuation_matches_uninterrupted_items() {
    let yaml = r"
content:
  - key: main
  - key: bumpers
sequence:
  - key: ident
    items:
      - count: '1'
        content: bumpers
        disable_watermarks: true
playout:
  - post_roll: true
    sequence: ident
  - mid_roll: true
    sequence: ident
    expression: 'true'
  - graphics_on: lower-third
    variables:
      title: Now
  - count: '1'
    content: main
  - count: '1'
    content: main
  - count: '1'
    content: main
";
    let (definition, mut request) = request(yaml);
    request.finish = request.start + Duration::minutes(35);
    let oracle = YamlPlayoutEngine
        .schedule_definition(definition.clone(), request.clone())
        .unwrap();
    let mut first_request = request.clone();
    first_request.finish = request.start + Duration::minutes(1);
    let first = YamlPlayoutEngine
        .schedule_definition(definition.clone(), first_request)
        .unwrap();
    assert_eq!(first.added_items.len(), 3);
    request.start = first.anchor.as_ref().unwrap().next_start;
    request.playout.anchor = first.anchor;
    request.reference_data.existing_history = first.added_history;
    request.mode = PlayoutBuildMode::Continue;
    let continued = YamlPlayoutEngine
        .schedule_definition(definition, request)
        .unwrap();
    assert!(!continued.clear_items);
    let combined = first
        .added_items
        .into_iter()
        .chain(continued.added_items)
        .collect::<Vec<_>>();
    assert_eq!(combined, oracle.added_items);
}

#[test]
fn yaml_ambient_checkpoint_accepts_upstream_field_shapes() {
    let original = context();
    let mut value: serde_json::Value =
        serde_json::from_str(&original.serialize().unwrap()).unwrap();
    value["PostRollSequence"] = serde_json::json!("idents");
    value["MidRollSequence"] = serde_json::json!({"Sequence":"ads", "Expression":"count > 1"});
    value["GraphicsElements"] = serde_json::json!({"7":null, "8":"{\"title\":\"x\"}"});
    let mut restored = context();
    restored
        .restore(&PlayoutAnchor {
            next_start: original.current_time,
            context: Some(value.to_string()),
            ..PlayoutAnchor::default()
        })
        .unwrap();
    assert_eq!(restored.post_roll_sequence.as_deref(), Some("idents"));
    assert_eq!(
        restored.mid_roll_sequence,
        Some(("ads".to_owned(), "count > 1".to_owned()))
    );
    assert_eq!(restored.graphics_elements.get(&7), Some(&None));
    assert_eq!(
        restored.graphics_elements.get(&8),
        Some(&Some(r#"{"title":"x"}"#.to_owned()))
    );
    value["PostRollSequence"] = serde_json::Value::Null;
    value["MidRollSequence"] = serde_json::Value::Null;
    value["GraphicsElements"] = serde_json::Value::Null;
    restored
        .restore(&PlayoutAnchor {
            next_start: original.current_time,
            context: Some(value.to_string()),
            ..PlayoutAnchor::default()
        })
        .unwrap();
    assert!(restored.post_roll_sequence.is_none());
    assert!(restored.mid_roll_sequence.is_none());
    assert!(restored.graphics_elements.is_empty());
}
