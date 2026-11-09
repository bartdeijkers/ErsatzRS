//! UCU-15 row278: resolve `NextPlayoutDependents` before references are deleted.
use ersatzrs_core::repositories::Result;
use ersatzrs_domain::{PlayoutProjectionChange, CONFIG_KEY_FFMPEG_GLOBAL_WATERMARK_ID};

use crate::{
    db::{self, DbPool},
    repositories::SqlxResultExt as _,
};

/// Return distinct affected Next channel numbers, including source mirrors.
/// The caller must resolve before destructive edits and notify after commit.
/// No session, provider, scheduler or publication work occurs in this query.
///
/// # Errors
/// Returns a repository failure if dependency state cannot be read.
pub async fn channel_numbers(
    pool: &DbPool,
    change: PlayoutProjectionChange,
) -> Result<Vec<String>> {
    let id = match change {
        PlayoutProjectionChange::All => return all(pool).await,
        PlayoutProjectionChange::Watermark(id)
        | PlayoutProjectionChange::Deco(id)
        | PlayoutProjectionChange::DecoGroup(id)
        | PlayoutProjectionChange::DecoTemplate(id)
        | PlayoutProjectionChange::DecoTemplateGroup(id)
        | PlayoutProjectionChange::Playout(id)
        | PlayoutProjectionChange::MediaSource(id) => id,
    };
    // New objects cannot have dependents; invalid IDs still reach normal
    // command validation, with no mutation or unnecessary query here.
    if id <= 0 {
        return Ok(Vec::new());
    }
    if matches!(change, PlayoutProjectionChange::Watermark(_)) {
        let global: Option<Option<String>> =
            sqlx::query_scalar(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#)
                .bind(CONFIG_KEY_FFMPEG_GLOBAL_WATERMARK_ID)
                .fetch_optional(pool)
                .await
                .repository()?;
        if global
            .flatten()
            .as_deref()
            .and_then(|value| value.parse::<i64>().ok())
            == Some(id)
        {
            return all(pool).await;
        }
    }
    let roots = root_channels(change);
    let sql = format!(
        r#"WITH affected AS ({roots}) SELECT c."Number" FROM "Channel" c
           WHERE c."StreamingEngine"=1 AND (c."Id" IN (SELECT * FROM affected)
             OR (c."PlayoutSource"=1 AND c."MirrorSourceChannelId" IN (SELECT * FROM affected))) ORDER BY c."Id""#
    );
    sqlx::query_scalar(db::assert_sql_safe(&sql))
        .bind(id)
        .fetch_all(pool)
        .await
        .repository()
}

fn root_channels(change: PlayoutProjectionChange) -> String {
    match change {
        PlayoutProjectionChange::Watermark(_) => format!(
            r#"SELECT "Id" FROM "Channel" WHERE "WatermarkId" = ?1
               UNION SELECT p."ChannelId" FROM "PlayoutItemWatermark" w
                 JOIN "PlayoutItem" i ON i."Id"=w."PlayoutItemId"
                 JOIN "Playout" p ON p."Id"=i."PlayoutId" WHERE w."WatermarkId"=?1
               UNION {}"#,
            deco_channels(r#"SELECT "DecoId" FROM "DecoWatermark" WHERE "WatermarkId"=?1"#)
        ),
        PlayoutProjectionChange::Deco(_) => deco_channels("SELECT ?1"),
        PlayoutProjectionChange::DecoGroup(_) => {
            deco_channels(r#"SELECT "Id" FROM "Deco" WHERE "DecoGroupId"=?1"#)
        }
        PlayoutProjectionChange::DecoTemplate(_) => {
            r#"SELECT p."ChannelId" FROM "Playout" p JOIN "PlayoutTemplate" t
               ON t."PlayoutId"=p."Id" WHERE t."DecoTemplateId"=?1"#
                .to_owned()
        }
        PlayoutProjectionChange::DecoTemplateGroup(_) => {
            r#"SELECT p."ChannelId" FROM "Playout" p JOIN "PlayoutTemplate" t
               ON t."PlayoutId"=p."Id" JOIN "DecoTemplate" d ON d."Id"=t."DecoTemplateId"
               WHERE d."DecoTemplateGroupId"=?1"#
                .to_owned()
        }
        PlayoutProjectionChange::Playout(_) => {
            r#"SELECT "ChannelId" FROM "Playout" WHERE "Id"=?1"#.to_owned()
        }
        PlayoutProjectionChange::MediaSource(_) => r#"SELECT p."ChannelId" FROM "Playout" p
               JOIN "PlayoutItem" i ON i."PlayoutId"=p."Id"
               JOIN "MediaItem" m ON m."Id"=i."MediaItemId"
               JOIN "LibraryPath" lp ON lp."Id"=m."LibraryPathId"
               JOIN "Library" l ON l."Id"=lp."LibraryId" WHERE l."MediaSourceId"=?1"#
            .to_owned(),
        PlayoutProjectionChange::All => r#"SELECT "Id" FROM "Channel""#.to_owned(),
    }
}

fn deco_channels(decos: &str) -> String {
    // The only callers provide static SQL above; IDs remain bound parameters.
    format!(
        r#"SELECT p."ChannelId" FROM "Playout" p WHERE p."DecoId" IN ({decos})
           OR EXISTS (SELECT 1 FROM "PlayoutTemplate" t JOIN "DecoTemplateItem" i
             ON i."DecoTemplateId"=t."DecoTemplateId"
             WHERE t."PlayoutId"=p."Id" AND i."DecoId" IN ({decos}))"#
    )
}

async fn all(pool: &DbPool) -> Result<Vec<String>> {
    sqlx::query_scalar(r#"SELECT "Number" FROM "Channel" WHERE "StreamingEngine"=1 ORDER BY "Id""#)
        .fetch_all(pool)
        .await
        .repository()
}

#[cfg(test)]
#[path = "dependents_tests.rs"]
mod tests;
