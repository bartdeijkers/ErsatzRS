#![cfg(not(feature = "mysql"))]
mod common;

use ersatzrs_core::repositories::WatermarkRepository;
use ersatzrs_domain::{
    ChannelWatermark, ChannelWatermarkImageSource, ChannelWatermarkMode, WatermarkLocation,
    WatermarkSize,
};
use ersatzrs_infra::repositories::make_repositories;

fn watermark(name: &str) -> ChannelWatermark {
    ChannelWatermark {
        id: 0,
        name: name.to_owned(),
        mode: ChannelWatermarkMode::OpacityExpression,
        image_source: ChannelWatermarkImageSource::Custom,
        image: Some("station.png".to_owned()),
        original_content_type: Some("image/png".to_owned()),
        location: WatermarkLocation::TopMiddle,
        size: WatermarkSize::Scaled,
        width_percent: 20.0,
        horizontal_margin_percent: 4.0,
        vertical_margin_percent: 6.0,
        frequency_minutes: 10,
        duration_seconds: 20,
        opacity: 75,
        place_within_source_content: true,
        opacity_expression: Some("if(lt(t,1),0,1)".to_owned()),
        z_index: 3,
    }
}

#[tokio::test]
async fn watermark_admin_create_update_copy_delete_and_absence_are_typed() {
    let repos = make_repositories(common::test_db().await);
    let mut original = watermark("station");
    let id = repos.watermark.save_admin(&original).await.unwrap();

    assert!(id > 0);
    assert!(repos.watermark.name_exists(0, "station").await.unwrap());
    let loaded = repos.watermark.get_admin(id).await.unwrap().unwrap();
    assert_eq!(
        loaded,
        ChannelWatermark {
            id,
            ..original.clone()
        }
    );

    original.id = id;
    original.name = "station updated".to_owned();
    original.location = WatermarkLocation::BottomLeft;
    assert_eq!(repos.watermark.save_admin(&original).await.unwrap(), id);

    let copy_id = repos
        .watermark
        .copy_admin(id, "station copy")
        .await
        .unwrap();
    assert_ne!(copy_id, id);
    assert_eq!(
        repos
            .watermark
            .get_admin(copy_id)
            .await
            .unwrap()
            .unwrap()
            .name,
        "station copy"
    );
    assert!(repos.watermark.get_admin(99_999).await.unwrap().is_none());

    repos.watermark.delete_admin(id).await.unwrap();
    assert!(repos.watermark.delete_admin(id).await.is_err());
    assert!(repos.watermark.copy_admin(id, "missing").await.is_err());
}

#[tokio::test]
async fn watermark_admin_reads_report_closed_pool_errors() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    pool.close().await;

    assert!(repos.watermark.list_admin().await.is_err());
    assert!(repos.watermark.get_admin(1).await.is_err());
    assert!(repos.watermark.name_exists(0, "station").await.is_err());
}

#[tokio::test]
async fn watermark_admin_list_has_deterministic_name_then_id_order() {
    let repos = make_repositories(common::test_db().await);
    let charlie = repos
        .watermark
        .save_admin(&watermark("Charlie"))
        .await
        .unwrap();
    let alpha = repos
        .watermark
        .save_admin(&watermark("Alpha"))
        .await
        .unwrap();
    let bravo = repos
        .watermark
        .save_admin(&watermark("Bravo"))
        .await
        .unwrap();

    let rows = repos.watermark.list_admin().await.unwrap();
    assert_eq!(
        rows.into_iter()
            .map(|row| (row.id, row.name))
            .collect::<Vec<_>>(),
        vec![
            (alpha, "Alpha".to_owned()),
            (bravo, "Bravo".to_owned()),
            (charlie, "Charlie".to_owned()),
        ]
    );
}
