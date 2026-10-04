//! Real Block/YAML definitions for the shared decoded producer proof.

use super::*;
use chrono::{DateTime, Timelike as _};

pub(super) fn callback_listener(next: bool) -> Option<std::net::TcpListener> {
    // Block templates also publish gaps outside this capture's programme
    // window. Their descriptors need a bound callback address. This proof
    // must use only local files and never actually request those future gaps.
    next.then(|| {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        listener
    })
}

pub(super) fn assert_no_callbacks(listener: Option<std::net::TcpListener>) {
    if let Some(listener) = listener {
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock,
            "programme handover must not request a fallback gap"
        );
    }
}

pub(super) async fn configure(
    pool: &SqlitePool,
    source: Source,
    start: DateTime<Utc>,
) -> DateTime<Utc> {
    match source {
        Source::Yaml => {
            let definition = "content:\n  - key: main\n    collection: Refresh Collection\n    order: chronological\nplayout:\n  - count: 3\n    content: main\n  - count: 1\n    content: main\n  - repeat: true\n";
            sqlx::query("UPDATE Playout SET ScheduleKind=3, ProgramScheduleId=NULL, ScheduleFile=? WHERE Id=88")
                .bind(definition).execute(pool).await.unwrap();
            start
        }
        Source::Block => {
            // Template slots have whole-second clocks. Keep the first complete
            // occurrence slightly in the past, just like the other schedulers.
            let start = start.with_nanosecond(0).unwrap();
            block(pool, start).await;
            start
        }
        _ => start,
    }
}

async fn block(pool: &SqlitePool, start: DateTime<Utc>) {
    pool.execute(r#"
        UPDATE "Playout" SET "ScheduleKind"=2, "ProgramScheduleId"=NULL WHERE "Id"=88;
        INSERT INTO "BlockGroup" ("Id", "Name") VALUES (71, 'Producer blocks');
        INSERT INTO "Block" ("Id", "BlockGroupId", "Name", "DateUpdated", "Minutes", "StopScheduling")
            VALUES (700, 71, 'Programme', '7000', 1, 'BeforeDurationEnd');
        INSERT INTO "BlockItem" ("Id", "Index", "BlockId", "CollectionType", "CollectionId", "PlaybackOrder")
            VALUES (701, 0, 700, 'Collection', 1, 'Chronological');
        INSERT INTO "TemplateGroup" ("Id", "Name") VALUES (72, 'Producer templates');
        INSERT INTO "Template" ("Id", "TemplateGroupId", "Name", "DateUpdated")
            VALUES (800, 72, 'Daily', '8000');
        INSERT INTO "PlayoutTemplate" ("PlayoutId", "TemplateId", "DateUpdated", "Index",
            "DaysOfWeek", "DaysOfMonth", "MonthsOfYear", "LimitToDateRange", "StartMonth", "StartDay", "EndMonth", "EndDay")
            VALUES (88, 800, '80000', 0, '[0,1,2,3,4,5,6]',
                '[1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31]',
                '[1,2,3,4,5,6,7,8,9,10,11,12]', 0, 1, 1, 12, 31);
    "#).await.unwrap();
    for minute in 0..12 {
        let clock = start + Duration::minutes(minute);
        sqlx::query(r#"INSERT INTO "TemplateItem" ("Id", "TemplateId", "BlockId", "StartTime") VALUES (?, 800, 700, ?)"#)
            .bind(801 + minute)
            .bind(clock.format("%H:%M:%S").to_string())
            .execute(pool).await.unwrap();
    }
}
