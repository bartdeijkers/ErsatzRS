//! Source correspondence cannot discard hidden writer payload or placement.
use super::*;

#[test]
fn writer_source_correspondence_preserves_every_payload_field() {
    let original = TeletextDisplayWriter {
        source: TeletextAttributeSource::PanelDefault { row: 1 },
        requested: (1, 2),
        effective: (1, 3),
        placed: (1, 4),
        values: [None; FIELDS],
        explicit: false,
    };
    let relocated = TeletextDisplayWriter {
        source: TeletextAttributeSource::PanelDefault { row: 2 },
        ..original
    };
    assert_ne!(original, relocated);
    assert!(original.same_payload(&relocated));
    for field in 0..FIELDS {
        let mut changed = relocated;
        changed.values[field] = Some(Value::Flag(true));
        assert!(!original.same_payload(&changed));
    }
    for changed in [
        TeletextDisplayWriter {
            requested: (2, 2),
            ..relocated
        },
        TeletextDisplayWriter {
            effective: (2, 3),
            ..relocated
        },
        TeletextDisplayWriter {
            placed: (2, 4),
            ..relocated
        },
        TeletextDisplayWriter {
            explicit: true,
            ..relocated
        },
    ] {
        assert!(!original.same_payload(&changed));
    }
}
