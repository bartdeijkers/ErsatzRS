//! Bounded, provider-independent upstream JavaScript audio callbacks (ADR 0096).

use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, LazyLock,
};
use std::time::{Duration, Instant};

use rquickjs::{function::Args, Context, Function, Runtime, Value};
use serde_json::Value as Json;
use tokio::sync::Semaphore;

static FAILURES: AtomicU64 = AtomicU64::new(0);

pub(crate) fn failures_since_startup() -> u64 {
    FAILURES.load(Ordering::Relaxed)
}

fn record_failure() {
    FAILURES.fetch_add(1, Ordering::Relaxed);
}

const HEAP_BYTES: usize = 4_000_000;
const STACK_BYTES: usize = 256 * 1024;
const SCRIPT_BYTES: usize = 256 * 1024;
const DEADLINE: Duration = Duration::from_secs(4);
static WORKERS: LazyLock<Arc<Semaphore>> = LazyLock::new(|| Arc::new(Semaphore::new(2)));

struct CancelOnDrop(Arc<AtomicBool>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// Execute an owned callback request. No script or metadata is included in errors.
/// The worker owns its permit until the actual VM exits, including caller cancellation.
async fn execute(
    script: String,
    callback: &'static str,
    arguments: Vec<Json>,
    indices: Vec<usize>,
) -> Option<usize> {
    execute_bounded(
        script,
        callback,
        arguments,
        indices,
        WORKERS.clone(),
        DEADLINE,
    )
    .await
}

async fn execute_bounded(
    script: String,
    callback: &'static str,
    arguments: Vec<Json>,
    indices: Vec<usize>,
    workers: Arc<Semaphore>,
    budget: Duration,
) -> Option<usize> {
    if script.len() > SCRIPT_BYTES {
        record_failure();
        tracing::warn!("audio selector exceeds script size limit; using built-in selection");
        return None;
    }
    // A single deadline includes admission and queued blocking-worker time.
    let deadline = Instant::now() + budget;
    let Ok(Ok(permit)) = tokio::time::timeout(budget, workers.acquire_owned()).await else {
        record_failure();
        tracing::warn!("audio selector admission deadline exceeded; using built-in selection");
        return None;
    };
    let cancelled = Arc::new(AtomicBool::new(false));
    let _cancel = CancelOnDrop(cancelled.clone());
    let worker = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        if cancelled.load(Ordering::Relaxed) || Instant::now() >= deadline {
            return Err(());
        }
        run(&script, callback, arguments, &indices, cancelled, deadline).map_err(|_| ())
    });
    if let Ok(Ok(Ok(index))) =
        tokio::time::timeout(deadline.saturating_duration_since(Instant::now()), worker).await
    {
        index
    } else {
        record_failure();
        tracing::warn!(
            "audio selector execution failed or exceeded limits; using built-in selection"
        );
        None
    }
}

#[allow(clippy::float_cmp)] // Both operands are exact integer-valued doubles (u32 indexes).
fn run(
    script: &str,
    callback: &str,
    arguments: Vec<Json>,
    indices: &[usize],
    cancelled: Arc<AtomicBool>,
    deadline: Instant,
) -> rquickjs::Result<Option<usize>> {
    let runtime = Runtime::new()?;
    runtime.set_memory_limit(HEAP_BYTES);
    runtime.set_max_stack_size(STACK_BYTES);
    runtime.set_interrupt_handler(Some(Box::new(move || {
        cancelled.load(Ordering::Relaxed) || Instant::now() >= deadline
    })));
    let context = Context::full(&runtime)?;
    context.with(|ctx| {
        // Deliberately discard arguments: arbitrary log content can contain private metadata.
        // No Rust host callback means a logging loop cannot overwhelm the logger.
        ctx.eval::<(), _>("function log() {}")?;
        ctx.eval::<(), _>(script)?;
        let function: Function = ctx.globals().get(callback)?;
        let mut args = Args::new(ctx.clone(), arguments.len());
        for argument in arguments {
            args.push_arg(ctx.json_parse(argument.to_string())?)?;
        }
        let result: Value = function.call_arg(args)?;
        // Upstream casts numeric results to an integer. Validate against known audio
        // indexes before conversion, without accepting numeric strings or booleans.
        Ok(result
            .as_number()
            .filter(|number| number.is_finite())
            .and_then(|number| {
                indices.iter().copied().find(|index| {
                    u32::try_from(*index).is_ok_and(|index| number.trunc() == f64::from(index))
                })
            }))
    })
}

use crate::db::DbPool;
use ersatzrs_core::language_codes;
use ersatzrs_domain::{Channel, StreamingMode};
use ersatzrs_ffmpeg::SourceAudioStream;
use serde_json::json;
use sqlx::Row;
use tokio::io::AsyncReadExt as _;

/// Effective preferences after item/channel/global resolution. Raw HLS Direct
/// without a preference keeps all audio, bypassing the upstream callback.
pub struct AudioScriptPreferences<'a> {
    /// Effective language, including an explicit global original-language choice.
    pub language: Option<&'a str>,
    /// Effective item/channel audio-title preference.
    pub title: Option<&'a str>,
    /// Actual transport mode, not merely the channel's configured default.
    pub streaming_mode: StreamingMode,
}

/// Attempt the upstream movie/episode callback; `None` requests ordinary selection.
/// Call only after exact pins and custom YAML selection have been considered.
/// Script failures deliberately never fail playback or expose their exception text.
pub async fn select(
    pool: &DbPool,
    channel: &Channel,
    media_item_id: i64,
    media_version_id: i64,
    preferences: AudioScriptPreferences<'_>,
    audio: &[SourceAudioStream],
) -> Option<usize> {
    if preferences.streaming_mode == StreamingMode::HttpLiveStreamingDirect
        && preferences
            .language
            .is_none_or(|value| value.trim().is_empty())
        && preferences
            .title
            .is_none_or(|value| value.trim().is_empty())
    {
        return None;
    }
    match request(
        pool,
        channel,
        media_item_id,
        media_version_id,
        preferences.language,
    )
    .await
    {
        Ok(Some((script, callback, arguments))) => {
            execute(
                script,
                callback,
                arguments,
                audio.iter().map(|stream| stream.index).collect(),
            )
            .await
        }
        Ok(None) => None,
        Err(_) => {
            record_failure();
            tracing::warn!("audio selector could not be loaded; using built-in selection");
            None
        }
    }
}

type Request = (String, &'static str, Vec<Json>);

async fn request(
    pool: &DbPool,
    channel: &Channel,
    item: i64,
    version: i64,
    language: Option<&str>,
) -> Result<Option<Request>, Box<dyn std::error::Error + Send + Sync>> {
    request_in_folder(
        pool,
        channel,
        item,
        version,
        language,
        &crate::file_system_layout::scripts_folder().join("audio-stream-selector"),
    )
    .await
}

async fn request_in_folder(
    pool: &DbPool,
    channel: &Channel,
    item: i64,
    version: i64,
    language: Option<&str>,
    folder: &std::path::Path,
) -> Result<Option<Request>, Box<dyn std::error::Error + Send + Sync>> {
    let kind: Option<String> = sqlx::query_scalar(
        r#"SELECT CASE WHEN EXISTS(SELECT 1 FROM "Movie" WHERE "Id" = ?) THEN 'movie' WHEN EXISTS(SELECT 1 FROM "Episode" WHERE "Id" = ?) THEN 'episode' END"#
    ).bind(item).bind(item).fetch_one(pool).await?;
    let Some(kind) = kind else {
        return Ok(None);
    };
    let path = folder.join(format!("{kind}.js"));
    match tokio::fs::metadata(&path).await {
        Ok(metadata) if metadata.is_file() => {}
        Ok(_) => return Err("selector is not a regular file".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    }
    let file = match tokio::fs::File::open(path).await {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let mut script = String::new();
    file.take((SCRIPT_BYTES + 1) as u64)
        .read_to_string(&mut script)
        .await?;
    if script.len() > SCRIPT_BYTES {
        return Err("script size limit exceeded".into());
    }
    let mut arguments = vec![json!(channel.number), json!(channel.name)];
    let callback = if kind == "movie" {
        let Some(metadata) = sqlx::query(
            r#"SELECT "Id", "Title" FROM "MovieMetadata" WHERE "MovieId" = ? ORDER BY "Id" LIMIT 1"#,
        )
        .bind(item)
        .fetch_optional(pool)
        .await?
        else {
            return Ok(None);
        };
        arguments.push(json!(metadata.try_get::<Option<String>, _>("Title")?));
        arguments.push(json!(
            guids(pool, "MovieMetadataId", metadata.try_get("Id")?).await?
        ));
        "selectMovieAudioStreamIndex"
    } else {
        let Some(metadata) = sqlx::query(
            r#"SELECT "em"."Id", "em"."EpisodeNumber", "s"."SeasonNumber", "sm"."Id" AS "ShowMetadataId", "sm"."Title" FROM "EpisodeMetadata" "em" JOIN "Episode" "e" ON "e"."Id" = "em"."EpisodeId" JOIN "Season" "s" ON "s"."Id" = "e"."SeasonId" JOIN "ShowMetadata" "sm" ON "sm"."ShowId" = "s"."ShowId" WHERE "e"."Id" = ? ORDER BY "em"."Id", "sm"."Id" LIMIT 1"#
        ).bind(item).fetch_optional(pool).await? else { return Ok(None); };
        arguments.push(json!(metadata.try_get::<Option<String>, _>("Title")?));
        arguments.push(json!(
            guids(pool, "ShowMetadataId", metadata.try_get("ShowMetadataId")?).await?
        ));
        arguments.push(json!(metadata.try_get::<i32, _>("SeasonNumber")?));
        arguments.push(json!(metadata.try_get::<i32, _>("EpisodeNumber")?));
        arguments.push(json!(
            guids(pool, "EpisodeMetadataId", metadata.try_get("Id")?).await?
        ));
        "selectEpisodeAudioStreamIndex"
    };
    arguments.push(json!(language_codes::all_language_codes(language)));
    // Read original codec and forced flags; the pipeline's reduced codec enum does
    // not preserve the upstream JavaScript metadata contract.
    let rows = sqlx::query(r#"SELECT "Index", "Channels", "Codec", "Default", "Forced", "Language", "Title" FROM "MediaStream" WHERE "MediaVersionId" = ? AND "MediaStreamKind" = 2 ORDER BY "Index" LIMIT 257"#)
        .bind(version).fetch_all(pool).await?;
    if rows.len() > 256 {
        return Err("audio metadata size limit exceeded".into());
    }
    let mut streams = Vec::with_capacity(rows.len());
    for row in rows {
        streams.push(json!({
            "index": row.try_get::<i32, _>("Index")?,
            "channels": row.try_get::<i32, _>("Channels")?,
            "codec": row.try_get::<Option<String>, _>("Codec")?,
            "isDefault": row.try_get::<bool, _>("Default")?,
            "isForced": row.try_get::<bool, _>("Forced")?,
            "language": row.try_get::<Option<String>, _>("Language")?,
            "title": row.try_get::<Option<String>, _>("Title")?,
        }));
    }
    arguments.push(json!(streams));
    if serde_json::to_vec(&arguments)?.len() > SCRIPT_BYTES {
        return Err("selector metadata size limit exceeded".into());
    }
    Ok(Some((script, callback, arguments)))
}

async fn guids(pool: &DbPool, column: &str, id: i64) -> Result<Vec<String>, sqlx::Error> {
    // `column` is an internal constant, never script/user input.
    let sql = match column {
        "MovieMetadataId" => {
            r#"SELECT "Guid" FROM "MetadataGuid" WHERE "MovieMetadataId" = ? ORDER BY "Id" LIMIT 257"#
        }
        "ShowMetadataId" => {
            r#"SELECT "Guid" FROM "MetadataGuid" WHERE "ShowMetadataId" = ? ORDER BY "Id" LIMIT 257"#
        }
        "EpisodeMetadataId" => {
            r#"SELECT "Guid" FROM "MetadataGuid" WHERE "EpisodeMetadataId" = ? ORDER BY "Id" LIMIT 257"#
        }
        _ => return Err(sqlx::Error::RowNotFound),
    };
    let values: Vec<Option<String>> = sqlx::query_scalar(sql).bind(id).fetch_all(pool).await?;
    if values.len() > 256 {
        return Err(sqlx::Error::RowNotFound);
    }
    Ok(values
        .into_iter()
        .flatten()
        .map(|guid| guid.replace("://", "_"))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn eval(body: &str) -> Option<usize> {
        execute_bounded(
            format!("function selectMovieAudioStreamIndex() {{ {body} }}"),
            "selectMovieAudioStreamIndex",
            vec![],
            vec![1, 2],
            Arc::new(Semaphore::new(1)),
            Duration::from_millis(100),
        )
        .await
    }

    #[tokio::test]
    async fn numeric_results_are_restricted_to_existing_audio() {
        assert_eq!(eval("return 2").await, Some(2));
        assert_eq!(eval("return 2.5").await, Some(2));
        for body in [
            "return null",
            "return '2'",
            "return true",
            "return 0",
            "return 99",
            "return NaN",
            "return Infinity",
            "throw Error('private metadata')",
        ] {
            assert_eq!(eval(body).await, None, "{body}");
        }
    }

    #[tokio::test]
    async fn memory_stack_and_loop_limits_fall_back() {
        for body in [
            "while(true) {}",
            "const a=[]; while(true) a.push(new Array(10000).fill('x'))",
            "function recurse(){return recurse()} return recurse()",
            "try {while(true){}} catch(e) {while(true){}}",
        ] {
            assert_eq!(eval(body).await, None);
        }
        assert_eq!(eval("return 1").await, Some(1));
    }

    #[tokio::test]
    async fn deadline_releases_worker_and_heap_limit_fails_before_deadline() {
        let workers = Arc::new(Semaphore::new(1));
        assert_eq!(
            execute_bounded(
                "while(true) {}".into(),
                "unused",
                vec![],
                vec![],
                workers.clone(),
                Duration::from_millis(50)
            )
            .await,
            None
        );
        let permit = tokio::time::timeout(Duration::from_secs(1), workers.acquire_owned())
            .await
            .unwrap()
            .unwrap();
        drop(permit);
        let start = Instant::now();
        assert!(run(
            "const memory = new ArrayBuffer(8000000); function choose() { return 1; }",
            "choose",
            vec![],
            &[],
            Arc::new(AtomicBool::new(false)),
            start + DEADLINE
        )
        .is_err());
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "heap allocation must fail without waiting for the execution deadline"
        );
    }

    #[tokio::test]
    async fn cancellation_releases_actual_worker_capacity() {
        let workers = Arc::new(Semaphore::new(1));
        let execution = tokio::spawn(execute_bounded(
            "while(true) {}".into(),
            "unused",
            vec![],
            vec![1],
            workers.clone(),
            DEADLINE,
        ));
        tokio::time::timeout(Duration::from_secs(1), async {
            while workers.available_permits() != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        execution.abort();
        let _ = execution.await;
        // Acquiring this permit proves the blocking worker exited, not only its waiter.
        let permit = tokio::time::timeout(Duration::from_secs(1), workers.clone().acquire_owned())
            .await
            .unwrap()
            .unwrap();
        drop(permit);
        assert_eq!(
            execute_bounded(
                "function choose(){return 1}".into(),
                "choose",
                vec![],
                vec![1],
                workers,
                Duration::from_secs(1)
            )
            .await,
            Some(1)
        );
    }

    #[tokio::test]
    async fn callback_receives_owned_json_without_host_capabilities() {
        let script = "function selectEpisodeAudioStreamIndex(number,name,title,showGuids,season,episode,episodeGuids,languages,audio) { if(number !== '7' || name !== 'Fixture' || title !== 'Show' || showGuids[0] !== 'tvdb_12' || season !== 2 || episode !== 3 || episodeGuids[0] !== 'tvdb_13' || languages[0] !== 'nl' || audio[1].language !== 'nld') throw Error('shape'); log('discarded'); if(typeof process !== 'undefined' || typeof require !== 'undefined' || typeof System !== 'undefined' || typeof fetch !== 'undefined') throw Error('host'); return audio[1].index; }";
        let arguments = vec![
            json!("7"),
            json!("Fixture"),
            json!("Show"),
            json!(["tvdb_12"]),
            json!(2),
            json!(3),
            json!(["tvdb_13"]),
            json!(["nl", "nld"]),
            json!([{"index":1,"language":"eng"},{"index":2,"language":"nld"}]),
        ];
        assert_eq!(
            execute(
                script.into(),
                "selectEpisodeAudioStreamIndex",
                arguments,
                vec![1, 2]
            )
            .await,
            Some(2)
        );
    }
}

#[cfg(test)]
mod metadata_tests {
    use super::*;

    #[tokio::test]
    async fn movie_file_metadata_guids_and_original_audio_flags_reach_callback() {
        let pool = crate::next_engine::selection::tests::fixture_pool().await;
        let root = tempfile::tempdir().unwrap();
        let channel = Channel {
            number: "7".into(),
            name: "Fixture".into(),
            ..Default::default()
        };
        assert!(
            request_in_folder(&pool, &channel, 10, 100, Some("nl"), root.path())
                .await
                .unwrap()
                .is_none()
        );
        sqlx::query(
            r#"INSERT INTO "MetadataGuid" ("MovieMetadataId", "Guid") VALUES (20, 'tmdb://123')"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(r#"UPDATE "MediaStream" SET "Forced" = 1, "Codec" = 'eac3' WHERE "Id" = 3"#)
            .execute(&pool)
            .await
            .unwrap();
        let script = "function selectMovieAudioStreamIndex(number,name,title,guids,languages,audio) { if(number !== '7' || name !== 'Fixture' || title !== 'Synthetic' || guids[0] !== 'tmdb_123' || !languages.includes('nl') || !languages.includes('nld') || audio.length !== 2 || audio[0].isDefault !== true || audio[1].isForced !== true || audio[1].codec !== 'eac3' || audio[1].channels !== 2 || audio[1].title !== 'Dutch') throw Error('shape'); return audio[1].index; }";
        tokio::fs::write(root.path().join("movie.js"), script)
            .await
            .unwrap();
        let (script, callback, args) =
            request_in_folder(&pool, &channel, 10, 100, Some("nl"), root.path())
                .await
                .unwrap()
                .unwrap();
        assert_eq!(execute(script, callback, args, vec![1, 2]).await, Some(2));
        tokio::fs::write(root.path().join("movie.js"), "x".repeat(SCRIPT_BYTES + 1))
            .await
            .unwrap();
        assert!(
            request_in_folder(&pool, &channel, 10, 100, None, root.path())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn episode_metadata_joins_do_not_duplicate_guid_arrays() {
        let pool = crate::next_engine::selection::tests::fixture_pool().await;
        for sql in [
            r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (11,1,0),(12,1,0),(13,1,0)"#,
            r#"INSERT INTO "Show" ("Id") VALUES (11)"#,
            r#"INSERT INTO "Season" ("Id", "ShowId", "SeasonNumber") VALUES (12,11,2)"#,
            r#"INSERT INTO "Episode" ("Id", "SeasonId") VALUES (13,12)"#,
            r#"INSERT INTO "ShowMetadata" ("Id","ShowId","Title","MetadataKind","DateAdded","DateUpdated") VALUES (30,11,'Synthetic Show',0,'2026-09-23','2026-09-23'),(31,11,'Other',1,'2026-09-23','2026-09-23')"#,
            r#"INSERT INTO "EpisodeMetadata" ("Id","EpisodeId","EpisodeNumber","MetadataKind","DateAdded","DateUpdated") VALUES (40,13,3,0,'2026-09-23','2026-09-23')"#,
            r#"INSERT INTO "MetadataGuid" ("ShowMetadataId", "Guid") VALUES (30,'tvdb://1'),(30,'tmdb://2')"#,
            r#"INSERT INTO "MetadataGuid" ("EpisodeMetadataId", "Guid") VALUES (40,'tvdb://3')"#,
        ] {
            sqlx::query(sql).execute(&pool).await.unwrap();
        }
        let root = tempfile::tempdir().unwrap();
        tokio::fs::write(
            root.path().join("episode.js"),
            "function selectEpisodeAudioStreamIndex() { return null; }",
        )
        .await
        .unwrap();
        let (_, callback, args) =
            request_in_folder(&pool, &Channel::default(), 13, 100, None, root.path())
                .await
                .unwrap()
                .unwrap();
        assert_eq!(callback, "selectEpisodeAudioStreamIndex");
        assert_eq!(args[2], json!("Synthetic Show"));
        assert_eq!(args[3], json!(["tvdb_1", "tmdb_2"]));
        assert_eq!(args[4], json!(2));
        assert_eq!(args[5], json!(3));
        assert_eq!(args[6], json!(["tvdb_3"]));
        assert_eq!(args[7], json!([]));
    }
}
