use super::*;
use crate::repositories::make_repositories;
use ersatzrs_core::repositories::{
    ChannelRepository, DecoRepository, PlayoutRepository, WatermarkRepository,
};
use ersatzrs_domain::{ChannelPlayoutSource, ChannelWatermark, PlayoutItem, StreamingEngine};

struct Fixture {
    pool: DbPool,
    watermark: i64,
    deco: i64,
    group: i64,
    template: i64,
    template_group: i64,
}

async fn fixture() -> Fixture {
    let (pool, _, channel, start) = super::super::schedule::tests::fixture().await;
    let repos = make_repositories(pool.clone());
    let watermark = repos
        .watermark
        .save_admin(&ChannelWatermark {
            name: "Fixture".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    for id in 1..=9 {
        let mut current = channel.clone();
        current.id = id;
        current.unique_id = uuid::Uuid::new_v4();
        current.number = id.to_string();
        current.streaming_engine = if id == 5 {
            StreamingEngine::Legacy
        } else {
            StreamingEngine::Next
        };
        current.watermark_id = matches!(id, 1 | 5).then_some(watermark);
        if matches!(id, 6 | 7 | 9) {
            current.mirror_source_channel_id = Some(if id == 9 { 5 } else { 1 });
            current.playout_source = if id == 7 {
                ChannelPlayoutSource::Generated
            } else {
                ChannelPlayoutSource::Mirror
            };
        }
        repos.channel.upsert(&current).await.unwrap();
    }
    for id in 2..=4 {
        sqlx::query(
            r#"INSERT INTO "Playout" ("Id","ChannelId","ScheduleKind","Seed") VALUES (?,?,0,1)"#,
        )
        .bind(id)
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    }
    repos
        .playout
        .upsert_item(&PlayoutItem {
            id: 3,
            playout_id: 2,
            media_item_id: 10,
            start,
            finish: start + chrono::Duration::seconds(30),
            out_point: chrono::Duration::seconds(30),
            ..Default::default()
        })
        .await
        .unwrap();
    for item in [1, 3] {
        sqlx::query(
            r#"INSERT INTO "PlayoutItemWatermark" ("PlayoutItemId","WatermarkId") VALUES (?,?)"#,
        )
        .bind(item)
        .bind(watermark)
        .execute(&pool)
        .await
        .unwrap();
    }
    let group = repos.deco.create_group_admin("Fixture").await.unwrap();
    let deco = repos.deco.create_admin(group, "Fixture").await.unwrap();
    let template_group = repos
        .deco
        .create_template_group_admin("Fixture")
        .await
        .unwrap();
    let template = repos
        .deco
        .create_template_admin(template_group, "Fixture")
        .await
        .unwrap();
    let result = Fixture {
        pool,
        watermark,
        deco,
        group,
        template,
        template_group,
    };
    seed_templates(&result).await;
    result
}

async fn seed_templates(f: &Fixture) {
    for sql in [
        r#"INSERT INTO "TemplateGroup" ("Id","Name") VALUES (10,'Fixture')"#,
        r#"INSERT INTO "Template" ("Id","Name","TemplateGroupId","DateUpdated") VALUES (10,'Fixture',10,'2026-11-06')"#,
    ] {
        sqlx::query(sql).execute(&f.pool).await.unwrap();
    }
    sqlx::query(r#"INSERT INTO "DecoWatermark" ("DecoId","WatermarkId") VALUES (?,?)"#)
        .bind(f.deco)
        .bind(f.watermark)
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "Playout" SET "DecoId"=? WHERE "Id"=3"#)
        .bind(f.deco)
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "DecoTemplateItem" ("DecoTemplateId","DecoId","StartTime") VALUES (?,?,'00:00:00')"#)
        .bind(f.template).bind(f.deco).execute(&f.pool).await.unwrap();
    sqlx::query(r#"INSERT INTO "PlayoutTemplate" ("PlayoutId","TemplateId","DecoTemplateId","EndDay","EndMonth","Index","LimitToDateRange","StartDay","StartMonth","DateUpdated") VALUES (4,10,?,1,1,0,0,1,1,'2026-11-06 00:00:00')"#)
        .bind(f.template).execute(&f.pool).await.unwrap();
}

#[tokio::test]
async fn dependency_matrix_selects_actual_roots_and_active_mirrors_once() {
    let f = fixture().await;
    for (change, expected) in [
        (
            PlayoutProjectionChange::Watermark(f.watermark),
            vec!["1", "2", "3", "4", "6", "9"],
        ),
        (PlayoutProjectionChange::Deco(f.deco), vec!["3", "4"]),
        (PlayoutProjectionChange::DecoGroup(f.group), vec!["3", "4"]),
        (PlayoutProjectionChange::DecoTemplate(f.template), vec!["4"]),
        (
            PlayoutProjectionChange::DecoTemplateGroup(f.template_group),
            vec!["4"],
        ),
        (PlayoutProjectionChange::Playout(1), vec!["1", "6"]),
        (PlayoutProjectionChange::MediaSource(1), vec!["1", "2", "6"]),
        (
            PlayoutProjectionChange::All,
            vec!["1", "2", "3", "4", "6", "7", "8", "9"],
        ),
        (PlayoutProjectionChange::Watermark(999), vec![]),
        (PlayoutProjectionChange::Watermark(0), vec![]),
    ] {
        assert_eq!(
            channel_numbers(&f.pool, change).await.unwrap(),
            expected,
            "{change:?}"
        );
    }
}

#[tokio::test]
async fn global_watermark_selects_every_next_channel_but_never_native() {
    let f = fixture().await;
    sqlx::query(r#"INSERT INTO "ConfigElement" ("Key","Value") VALUES (?, ?)"#)
        .bind(CONFIG_KEY_FFMPEG_GLOBAL_WATERMARK_ID)
        .bind(f.watermark.to_string())
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(
        channel_numbers(&f.pool, PlayoutProjectionChange::Watermark(f.watermark))
            .await
            .unwrap(),
        ["1", "2", "3", "4", "6", "7", "8", "9"]
    );
    sqlx::query(r#"UPDATE "ConfigElement" SET "Value"=NULL WHERE "Key"=?"#)
        .bind(CONFIG_KEY_FFMPEG_GLOBAL_WATERMARK_ID)
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(
        channel_numbers(&f.pool, PlayoutProjectionChange::Watermark(f.watermark))
            .await
            .unwrap(),
        ["1", "2", "3", "4", "6", "9"]
    );
}

#[tokio::test]
async fn deleting_references_requires_the_pre_mutation_dependency_snapshot() {
    let f = fixture().await;
    let captured = channel_numbers(&f.pool, PlayoutProjectionChange::DecoGroup(f.group))
        .await
        .unwrap();
    make_repositories(f.pool.clone())
        .deco
        .delete_group_admin(f.group)
        .await
        .unwrap();
    assert_eq!(captured, ["3", "4"]);
    assert!(
        channel_numbers(&f.pool, PlayoutProjectionChange::DecoGroup(f.group))
            .await
            .unwrap()
            .is_empty()
    );
}
