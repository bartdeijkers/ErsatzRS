//! Exercise opaque IDs through the actual stream projection, not an ID mock.
use super::{fixture_pool, item, project};
use chrono::Duration;
use ersatzrs_domain::Channel;
use std::collections::HashSet;

#[tokio::test]
async fn unsaved_projection_ids_distinguish_media_and_fractional_bounds_but_remain_stable() {
    let pool = fixture_pool().await;
    let channel = Channel::default();
    let mut first = item();
    first.id = 0;
    let initial = project(&pool, &channel, &first).await.unwrap();
    let repeated = project(&pool, &channel, &first).await.unwrap();
    assert_eq!(initial.id, repeated.id);
    assert_ne!(initial.id, "0");

    let mut ids = HashSet::from([initial.id]);
    let mut other_media = first.clone();
    other_media.media_item_id += 1;
    let mut other_start = first.clone();
    other_start.start += Duration::nanoseconds(100);
    let mut other_finish = first.clone();
    other_finish.finish += Duration::nanoseconds(100);
    for occurrence in [other_media, other_start, other_finish] {
        let projected = project(&pool, &channel, &occurrence).await.unwrap();
        assert!(
            ids.insert(projected.id),
            "distinct occurrences must not share Next state"
        );
    }
    let saved = project(&pool, &channel, &item()).await.unwrap();
    assert_eq!(saved.id, "42", "saved SQL identities remain unchanged");
}
