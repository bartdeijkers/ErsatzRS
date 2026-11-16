//! Count actual SQL reads during reference preparation, independently of elapsed time.
use super::*;
use std::sync::{Arc, Mutex};
use tracing::{field::Visit, Event, Subscriber};
use tracing_subscriber::{layer::Context, prelude::*, Layer};

#[derive(Clone, Default)]
struct Reads(Arc<Mutex<Vec<String>>>);

struct Statement(String);
impl Visit for Statement {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "db.statement" {
            self.0 = format!("{value:?}");
        }
    }
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "db.statement" {
            value.clone_into(&mut self.0);
        }
    }
}
impl<S: Subscriber> Layer<S> for Reads {
    fn on_event(&self, event: &Event<'_>, _context: Context<'_, S>) {
        if event.metadata().target() == "sqlx::query" {
            let mut statement = Statement(String::new());
            event.record(&mut statement);
            self.0.lock().unwrap().push(statement.0);
        }
    }
}

#[tokio::test]
async fn row294_measure_classic_repeated_smart_collection_reads() {
    let reads = Reads::default();
    // SQLx workers run on other threads; nextest isolates this test process.
    tracing::subscriber::set_global_default(tracing_subscriber::registry().with(reads.clone()))
        .unwrap();
    let pool = common::test_db().await;
    seed_scheduling_fixture(&pool).await;
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 901).await;
    repos
        .program_schedule
        .upsert(&ProgramSchedule {
            id: 301,
            name: "repeated source".into(),
            ..ProgramSchedule::default()
        })
        .await
        .unwrap();
    for id in 401..=403 {
        repos
            .program_schedule
            .upsert_item(&ProgramScheduleItem {
                id,
                program_schedule_id: 301,
                collection_type: CollectionType::SmartCollection,
                smart_collection_id: Some(2),
                playback_order: PlaybackOrder::Chronological,
                playout_mode: PlayoutMode::One,
                sort_order: i32::try_from(id).unwrap(),
                ..ProgramScheduleItem::default()
            })
            .await
            .unwrap();
    }
    pool.execute(r#"INSERT INTO "Playout" ("Id", "ChannelId", "ProgramScheduleId", "ScheduleKind", "Seed") VALUES (901,901,301,'Classic',123)"#).await.unwrap();
    reads.0.lock().unwrap().clear();
    let reference = repos
        .scheduling
        .classic_reference_data(
            901,
            Utc.with_ymd_and_hms(2026, 11, 16, 8, 0, 0).unwrap(),
            Duration::zero(),
        )
        .await
        .unwrap();
    let queries = reads.0.lock().unwrap();
    let smart = queries
        .iter()
        .filter(|s| s.contains("FROM \"SmartCollection\" WHERE \"Id\" = ?"))
        .count();
    let anchors = queries
        .iter()
        .filter(|s| s.contains("FROM \"PlayoutProgramScheduleAnchor\" AS"))
        .count();
    let groups = queries
        .iter()
        .filter(|s| s.contains("FROM \"PlayoutScheduleItemFillGroupIndex\" AS"))
        .count();
    println!(
        "row294 baseline: smart={smart}, anchors={anchors}, groups={groups}, total_sql={}",
        queries.len()
    );
    assert_eq!(reference.media_by_key.len(), 1, "source identity is shared");
    assert!(reference
        .media_by_key
        .values()
        .all(|items| !items.is_empty()));
    assert_eq!((anchors, groups), (1, 1), "state is already bulk loaded");
    assert_eq!(
        smart, 3,
        "baseline: one smart query for each duplicate source"
    );
}
