use super::*;
use crate::addon_media_list_delivery::portable::{
    capture_in_tx, restore_in_tx, PortableDeliveryPolicy,
};

async fn capture(pool: &DbPool) -> PortableDeliveryPolicy {
    let mut tx = pool.begin().await.unwrap();
    let content = capture_in_tx(&mut tx, 1).await.unwrap();
    tx.commit().await.unwrap();
    content
}

async fn ledger(pool: &DbPool) -> Vec<(i64, String)> {
    sqlx::query_as(
        r#"SELECT "Id", "ProviderId" FROM "AddonMediaListKnownIdentity"
           WHERE "AddonMediaListId" = 1 ORDER BY "ProviderId""#,
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn portable_delivery_round_trip_keeps_removed_cohorts_with_fresh_ids() {
    use AddonMediaListDeliveryMode::{DownloadAhead, DownloadAheadNewVideos, Stream};
    let source = database(true).await;
    insert_item(&source, 1, "old").await;
    insert_item(&source, 1, "removed-old").await;
    save(&source, DownloadAheadNewVideos, now()).await;
    insert_item(&source, 1, "new").await;
    insert_item(&source, 1, "removed-new").await;
    sqlx::query(r#"DELETE FROM "AddonMediaListItem" WHERE "ProviderId" LIKE 'removed-%'"#)
        .execute(&source)
        .await
        .unwrap();
    let content = capture(&source).await;
    assert_eq!(content.before_cutoff, ["old", "removed-old"]);
    assert_eq!(content.after_cutoff, ["new", "removed-new"]);
    let json = serde_json::to_value(&content).unwrap();
    assert_eq!(json.as_object().unwrap().len(), 4);
    let copied: PortableDeliveryPolicy = serde_json::from_value(json).unwrap();

    let receiving = database(true).await;
    insert_item(&receiving, 2, "unrelated").await;
    // A different local insertion order must not change the imported cohort.
    insert_item(&receiving, 1, "new").await;
    insert_item(&receiving, 1, "old").await;
    let mut tx = receiving.begin().await.unwrap();
    restore_in_tx(&mut tx, 1, &copied).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(capture(&receiving).await, content);
    assert_ne!(ledger(&receiving).await, ledger(&source).await);
    let first_ids = ledger(&receiving).await;
    let mut tx = receiving.begin().await.unwrap();
    let mut reordered = copied.clone();
    reordered.before_cutoff.reverse();
    reordered.cutoff_at = Some("2026-09-07T12:00:00Z".to_owned());
    restore_in_tx(&mut tx, 1, &reordered).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(ledger(&receiving).await, first_ids);
    assert_eq!(capture(&receiving).await, content);

    insert_item(&receiving, 1, "removed-old").await;
    insert_item(&receiving, 1, "removed-new").await;
    insert_item(&receiving, 1, "future").await;
    let result = deliveries(&receiving).await;
    assert_eq!(result["old"], Stream);
    assert_eq!(result["removed-old"], Stream);
    assert_eq!(result["new"], DownloadAhead);
    assert_eq!(result["removed-new"], DownloadAhead);
    assert_eq!(result["future"], DownloadAhead);
    assert_eq!(load(&receiving, 2).await.unwrap().mode, Stream);
    let other_count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "AddonMediaListKnownIdentity" WHERE "AddonMediaListId" = 2"#,
    )
    .fetch_one(&receiving)
    .await
    .unwrap();
    assert_eq!(other_count, 1);

    // The receiving installation still supports the agreed fresh-cutoff behavior.
    save(&receiving, Stream, now()).await;
    let later = now() + chrono::Duration::hours(1);
    save(&receiving, DownloadAheadNewVideos, later).await;
    assert!(deliveries(&receiving)
        .await
        .values()
        .all(|mode| *mode == Stream));
    assert_ne!(capture(&receiving).await.cutoff_at, content.cutoff_at);
    insert_item(&receiving, 1, "after-reenable").await;
    assert_eq!(
        deliveries(&receiving).await["after-reenable"],
        DownloadAhead
    );
}

#[tokio::test]
async fn portable_delivery_restores_all_modes_and_empty_pre_cutoff() {
    for mode in [
        AddonMediaListDeliveryMode::Stream,
        AddonMediaListDeliveryMode::DownloadAhead,
        AddonMediaListDeliveryMode::DownloadAheadNewVideos,
    ] {
        let source = database(true).await;
        save(&source, mode, now()).await;
        insert_item(&source, 1, "first").await;
        let content = capture(&source).await;
        let receiving = database(true).await;
        let mut tx = receiving.begin().await.unwrap();
        restore_in_tx(&mut tx, 1, &content).await.unwrap();
        tx.commit().await.unwrap();
        insert_item(&receiving, 1, "first").await;
        assert_eq!(capture(&receiving).await, content);
        assert_eq!(deliveries(&receiving).await, deliveries(&source).await);
        if mode == AddonMediaListDeliveryMode::DownloadAheadNewVideos {
            assert_eq!(
                load(&receiving, 1).await.unwrap().cutoff_identity_id,
                Some(0)
            );
        }
    }
}

#[tokio::test]
async fn portable_delivery_rejects_invalid_or_incomplete_content_before_writing() {
    let pool = database(true).await;
    insert_item(&pool, 1, "existing").await;
    let original = capture(&pool).await;
    let original_ids = ledger(&pool).await;
    let mut invalid = Vec::new();
    let mut duplicate = original.clone();
    duplicate.before_cutoff.push("existing".to_owned());
    invalid.push(duplicate);
    let mut missing = original.clone();
    missing.before_cutoff.clear();
    invalid.push(missing);
    let mut wrong_cutoff = original.clone();
    wrong_cutoff.mode = AddonMediaListDeliveryMode::DownloadAheadNewVideos;
    invalid.push(wrong_cutoff.clone());
    wrong_cutoff.cutoff_at = Some("not-a-date".to_owned());
    invalid.push(wrong_cutoff.clone());
    wrong_cutoff.cutoff_at = Some("2026-09-07T12:00:00Z".to_owned());
    wrong_cutoff.after_cutoff.push("existing".to_owned());
    invalid.push(wrong_cutoff);
    let mut wrong_mode = original.clone();
    wrong_mode.after_cutoff.push("unexpected".to_owned());
    invalid.push(wrong_mode);
    for content in invalid {
        let mut tx = pool.begin().await.unwrap();
        assert!(restore_in_tx(&mut tx, 1, &content).await.is_err());
        // Validation failures have not changed any data, even if committed.
        tx.commit().await.unwrap();
        assert_eq!(capture(&pool).await, original);
        assert_eq!(ledger(&pool).await, original_ids);
    }
}

#[tokio::test]
async fn portable_delivery_caller_rollback_preserves_policy_and_identity_ids() {
    let pool = database(true).await;
    insert_item(&pool, 1, "existing").await;
    let original = capture(&pool).await;
    let original_ids = ledger(&pool).await;
    let mut changed = original.clone();
    changed.mode = AddonMediaListDeliveryMode::DownloadAhead;
    changed.before_cutoff.push("removed".to_owned());
    let mut tx = pool.begin().await.unwrap();
    restore_in_tx(&mut tx, 1, &changed).await.unwrap();
    assert_eq!(capture_in_tx(&mut tx, 1).await.unwrap(), changed);
    tx.rollback().await.unwrap();
    assert_eq!(capture(&pool).await, original);
    assert_eq!(ledger(&pool).await, original_ids);
}
