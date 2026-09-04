//! Read-only aggregate audit for an ErsatzRS SQLite runtime database.

use std::{
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
    time::Duration,
};

use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OpenFlags};

// Keep these in sync with the persisted enum mapping in
// ersatzrs-infra::repositories::common.
const PUBLICATION_EVENT_ORIGINAL_FAILURE: i64 = 9;
const PUBLICATION_EVENT_RECOVERY_FAILURE: i64 = 10;
const PUBLICATION_EVENT_HISTORY_TRUNCATED: i64 = 14;

const REQUIRED_TABLES: &[&str] = &[
    "Actor",
    "AddonMediaListTelevisionShow",
    "Artwork",
    "Episode",
    "EpisodeMetadata",
    "Playout",
    "PlayoutItem",
    "PlayoutProgramScheduleAnchor",
    "Season",
];

pub fn run(args: Vec<String>) -> ExitCode {
    match execute(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("runtime-audit: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn execute(args: &[String]) -> Result<()> {
    let options = match Options::parse(args)? {
        ParsedOptions::Help => {
            print_help();
            return Ok(());
        }
        ParsedOptions::Run(options) => options,
    };
    let report = audit_for_options(&options)?;
    print!("{}", report.render(&options.database, options.redact));
    Ok(())
}

fn audit_for_options(options: &Options) -> Result<AuditReport> {
    match audit_database(&options.database) {
        Ok(report) => Ok(report),
        Err(error) if !options.redact => Err(error),
        Err(_) => bail!(
            "database audit failed in redacted mode; rerun without --redact for detailed diagnostics"
        ),
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Options {
    database: PathBuf,
    redact: bool,
}

enum ParsedOptions {
    Help,
    Run(Options),
}

impl Options {
    fn parse(args: &[String]) -> Result<ParsedOptions> {
        let mut database = None;
        let mut redact = false;
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "-h" | "--help" => return Ok(ParsedOptions::Help),
                "--db" => {
                    let value = args.get(index + 1).context("--db requires a path")?;
                    if value.is_empty() {
                        bail!("--db requires a non-empty path");
                    }
                    database = Some(PathBuf::from(value));
                    index += 2;
                }
                "--redact" => {
                    redact = true;
                    index += 1;
                }
                option if option.starts_with('-') => bail!("unknown option `{option}`"),
                value => bail!("unexpected argument `{value}`; pass the database with --db"),
            }
        }
        let database = database.context("missing required --db <PATH>")?;
        Ok(ParsedOptions::Run(Options { database, redact }))
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct AuditReport {
    integrity: String,
    file_size: u64,
    latest_migration: Option<i64>,
    anchors: AnchorAudit,
    timeline: TimelineAudit,
    episodes: EpisodeAudit,
    failed_build_statuses: i64,
    publication_history: PublicationHistoryAudit,
    addon_lifecycle: AddonLifecycleAudit,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct AnchorAudit {
    playouts: Vec<ClassicAnchorRow>,
    total: i64,
    dated: i64,
    undated: i64,
}

#[derive(Debug, PartialEq, Eq)]
struct ClassicAnchorRow {
    playout_id: i64,
    total: i64,
    dated: i64,
    undated: i64,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct TimelineAudit {
    rows: i64,
    playouts: i64,
    duplicate_slot_groups: i64,
    duplicate_slot_extras: i64,
    adjacent_overlaps: i64,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct EpisodeAudit {
    logical_episodes: i64,
    metadata_rows: i64,
    joined_thumbnail_rows: i64,
    multiplied_rows: i64,
    duplicate_artwork_groups: i64,
    duplicate_artwork_extras: i64,
    actor_referenced_duplicate_extras: i64,
    projected_season_episode_counts: Vec<i64>,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct PublicationHistoryAudit {
    available: bool,
    streams: i64,
    correlations: i64,
    events: i64,
    active: i64,
    recovery_required: i64,
    terminal: i64,
    truncated: i64,
    original_failures: i64,
    recovery_failures: i64,
    recent_streams: Vec<PublicationHistoryStream>,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct AddonLifecycleAudit {
    available: bool,
    lifecycle_active: i64,
    lifecycle_waiting: i64,
    lifecycle_expired: i64,
    lifecycle_failed: i64,
    checkpoints: i64,
    retained_revisions: i64,
    tombstones: i64,
    artifacts: i64,
    artifact_pending: i64,
    artifact_ready: i64,
    artifact_failed: i64,
    artifact_bytes: i64,
    artifact_reserved_bytes: i64,
    tool_active: i64,
    tool_waiting: i64,
    tool_expired: i64,
    tool_failed: i64,
    tool_updated: i64,
    tool_already_current: i64,
    tool_manual_update_required: i64,
    tool_rolled_back: i64,
    tool_failed_outcome: i64,
}

#[derive(Debug, PartialEq, Eq)]
struct PublicationHistoryStream {
    correlation_id: i64,
    state: i64,
    live_lease: bool,
    phase: i64,
    outcome: Option<i64>,
    events: i64,
    first_sequence: i64,
    last_sequence: i64,
    original_failures: i64,
    recovery_failures: i64,
    truncated: bool,
    error_category: Option<i64>,
    os_error_code: Option<i64>,
}

impl AuditReport {
    fn findings(&self) -> Vec<String> {
        let mut findings = Vec::new();
        let missing_checkpoint_playouts = self
            .anchors
            .playouts
            .iter()
            .filter(|row| row.total > 0 && row.dated == 0)
            .count();
        if missing_checkpoint_playouts > 0 {
            findings.push(format!(
                "{missing_checkpoint_playouts} Classic playout(s) have anchors but no dated Refresh checkpoint"
            ));
        }
        if self.timeline.duplicate_slot_extras > 0 {
            findings.push(format!(
                "{} extra PlayoutItem row(s) share an exact playout/start/finish/media slot",
                self.timeline.duplicate_slot_extras
            ));
        }
        if self.timeline.adjacent_overlaps > 0 {
            findings.push(format!(
                "{} adjacent PlayoutItem overlap(s) were detected",
                self.timeline.adjacent_overlaps
            ));
        }
        if self.failed_build_statuses > 0 {
            findings.push(format!(
                "{} persisted playout build status row(s) report failure",
                self.failed_build_statuses
            ));
        }
        if self.episodes.duplicate_artwork_extras > 0 {
            findings.push(format!(
                "{} exact duplicate episode Artwork row(s) were detected",
                self.episodes.duplicate_artwork_extras
            ));
        }
        if self.episodes.actor_referenced_duplicate_extras > 0 {
            findings.push(format!(
                "{} same-source duplicate episode Artwork extra row(s) belong to groups with Actor references",
                self.episodes.actor_referenced_duplicate_extras
            ));
        }
        if self.episodes.multiplied_rows > 0 {
            findings.push(format!(
                "the episode-thumbnail join adds {} row(s) beyond metadata cardinality",
                self.episodes.multiplied_rows
            ));
        }
        findings
    }

    fn render(&self, database: &Path, redact: bool) -> String {
        let label = if redact {
            "<redacted>".to_owned()
        } else {
            database.display().to_string()
        };
        let migration = self
            .latest_migration
            .map_or_else(|| "none".to_owned(), |version| version.to_string());
        let mut output = format!(
            "runtime-audit: database={label}\n\
             runtime-audit: mode=read-only size_bytes={} integrity={} latest_successful_migration={}\n\
             runtime-audit: classic_anchors playouts={} total={} dated={} undated={}\n",
            self.file_size,
            self.integrity,
            migration,
            self.anchors.playouts.len(),
            self.anchors.total,
            self.anchors.dated,
            self.anchors.undated
        );
        for (index, row) in self.anchors.playouts.iter().enumerate() {
            let identity = if redact {
                format!("classic_playout={}", index + 1)
            } else {
                format!("playout_id={}", row.playout_id)
            };
            output.push_str(&format!(
                "runtime-audit: {identity} anchors={} dated={} undated={}\n",
                row.total, row.dated, row.undated
            ));
        }
        output.push_str(&format!(
            "runtime-audit: timeline rows={} playouts={} duplicate_slot_groups={} duplicate_slot_extras={} adjacent_overlaps={} failed_build_statuses={}\n\
             runtime-audit: episodes logical={} metadata_rows={} thumbnail_join_rows={} multiplied_rows={} duplicate_artwork_groups={} duplicate_artwork_extras={} actor_referenced_duplicate_extras={} projected_season_episode_counts=[{}]\n",
            self.timeline.rows,
            self.timeline.playouts,
            self.timeline.duplicate_slot_groups,
            self.timeline.duplicate_slot_extras,
            self.timeline.adjacent_overlaps,
            self.failed_build_statuses,
            self.episodes.logical_episodes,
            self.episodes.metadata_rows,
            self.episodes.joined_thumbnail_rows,
            self.episodes.multiplied_rows,
            self.episodes.duplicate_artwork_groups,
            self.episodes.duplicate_artwork_extras,
            self.episodes.actor_referenced_duplicate_extras,
            self.episodes
                .projected_season_episode_counts
                .iter()
                .map(i64::to_string)
                .collect::<Vec<_>>()
                .join(",")
        ));
        output.push_str(&format!(
            "runtime-audit: publication_history available={} streams={} correlations={} events={} active={} recovery_required={} terminal={} truncated={} original_failures={} recovery_failures={}\n",
            self.publication_history.available,
            self.publication_history.streams,
            self.publication_history.correlations,
            self.publication_history.events,
            self.publication_history.active,
            self.publication_history.recovery_required,
            self.publication_history.terminal,
            self.publication_history.truncated,
            self.publication_history.original_failures,
            self.publication_history.recovery_failures,
        ));
        for stream in &self.publication_history.recent_streams {
            let identity = format!("publication_correlation={}", stream.correlation_id);
            output.push_str(&format!(
                "runtime-audit: {identity} events={} sequence={}..{} state={} phase={} outcome={} original_failures={} recovery_failures={} truncated={} error_category={} os_error_code={}\n",
                stream.events,
                stream.first_sequence,
                stream.last_sequence,
                publication_state_name(stream.state, stream.phase, stream.live_lease),
                publication_phase_name(stream.phase),
                stream.outcome.map_or("none", publication_outcome_name),
                stream.original_failures,
                stream.recovery_failures,
                stream.truncated,
                stream.error_category.map_or("none", publication_error_category_name),
                stream
                    .os_error_code
                    .map_or_else(|| "none".to_owned(), |code| code.to_string()),
            ));
        }
        output.push_str(&format!(
            "runtime-audit: addon_lifecycle available={} active={} waiting={} expired={} failed={} checkpoints={} retained_revisions={} tombstones={}\n\
             runtime-audit: addon_artifacts total={} pending={} ready={} failed={} bytes={} reserved_bytes={}\n\
             runtime-audit: addon_tool_maintenance active={} waiting={} expired={} failed={} outcomes_updated={} outcomes_already_current={} outcomes_manual_update_required={} outcomes_rolled_back={} outcomes_failed={}\n",
            self.addon_lifecycle.available,
            self.addon_lifecycle.lifecycle_active,
            self.addon_lifecycle.lifecycle_waiting,
            self.addon_lifecycle.lifecycle_expired,
            self.addon_lifecycle.lifecycle_failed,
            self.addon_lifecycle.checkpoints,
            self.addon_lifecycle.retained_revisions,
            self.addon_lifecycle.tombstones,
            self.addon_lifecycle.artifacts,
            self.addon_lifecycle.artifact_pending,
            self.addon_lifecycle.artifact_ready,
            self.addon_lifecycle.artifact_failed,
            self.addon_lifecycle.artifact_bytes,
            self.addon_lifecycle.artifact_reserved_bytes,
            self.addon_lifecycle.tool_active,
            self.addon_lifecycle.tool_waiting,
            self.addon_lifecycle.tool_expired,
            self.addon_lifecycle.tool_failed,
            self.addon_lifecycle.tool_updated,
            self.addon_lifecycle.tool_already_current,
            self.addon_lifecycle.tool_manual_update_required,
            self.addon_lifecycle.tool_rolled_back,
            self.addon_lifecycle.tool_failed_outcome,
        ));
        let findings = self.findings();
        for finding in &findings {
            output.push_str(&format!("runtime-audit: finding: {finding}\n"));
        }
        output.push_str(&format!(
            "runtime-audit: completed with {} finding(s); no database writes were performed\n",
            findings.len()
        ));
        output
    }
}

fn audit_database(path: &Path) -> Result<AuditReport> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("database file does not exist: {}", path.display()))?;
    if !metadata.is_file() {
        bail!("database path is not a file: {}", path.display());
    }
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("opening {} read-only", path.display()))?;
    connection
        .busy_timeout(Duration::from_secs(2))
        .context("configuring read-only audit timeout")?;
    validate_schema(&connection)?;

    let integrity: String = connection
        .query_row("PRAGMA quick_check(1)", [], |row| row.get(0))
        .context("running SQLite quick_check")?;
    if integrity != "ok" {
        bail!("SQLite quick_check failed: {integrity}");
    }

    Ok(AuditReport {
        integrity,
        file_size: metadata.len(),
        latest_migration: latest_migration(&connection)?,
        anchors: audit_anchors(&connection)?,
        timeline: audit_timeline(&connection)?,
        episodes: audit_episodes(&connection)?,
        failed_build_statuses: audit_failed_builds(&connection)?,
        publication_history: audit_publication_history(&connection)?,
        addon_lifecycle: audit_addon_lifecycle(&connection)?,
    })
}

fn validate_schema(connection: &Connection) -> Result<()> {
    let mut missing = Vec::new();
    for table in REQUIRED_TABLES {
        if !table_exists(connection, table)? {
            missing.push(*table);
        }
    }
    if !missing.is_empty() {
        bail!(
            "not an initialized ErsatzRS SQLite database; missing table(s): {}",
            missing.join(", ")
        );
    }
    Ok(())
}

fn table_exists(connection: &Connection, table: &str) -> Result<bool> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            [table],
            |row| row.get(0),
        )
        .with_context(|| format!("checking table {table}"))
}

fn latest_migration(connection: &Connection) -> Result<Option<i64>> {
    if !table_exists(connection, "_sqlx_migrations")? {
        return Ok(None);
    }
    connection
        .query_row(
            "SELECT MAX(version) FROM _sqlx_migrations WHERE success = 1",
            [],
            |row| row.get(0),
        )
        .context("reading latest successful migration")
}

fn audit_anchors(connection: &Connection) -> Result<AnchorAudit> {
    let mut statement = connection.prepare(
        r#"SELECT p."Id",
                  COUNT(a."Id") AS "Total",
                  COALESCE(SUM(CASE WHEN a."Id" IS NOT NULL
                                         AND NULLIF(TRIM(a."AnchorDate"), '') IS NULL
                                    THEN 1 ELSE 0 END), 0) AS "Undated",
                  COALESCE(SUM(CASE WHEN NULLIF(TRIM(a."AnchorDate"), '') IS NOT NULL
                                    THEN 1 ELSE 0 END), 0) AS "Dated"
           FROM "Playout" p
           LEFT JOIN "PlayoutProgramScheduleAnchor" a ON a."PlayoutId" = p."Id"
           WHERE p."ScheduleKind" = 1
           GROUP BY p."Id"
           ORDER BY p."Id""#,
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok(ClassicAnchorRow {
                playout_id: row.get(0)?,
                total: row.get(1)?,
                undated: row.get(2)?,
                dated: row.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(AnchorAudit {
        total: rows.iter().map(|row| row.total).sum(),
        dated: rows.iter().map(|row| row.dated).sum(),
        undated: rows.iter().map(|row| row.undated).sum(),
        playouts: rows,
    })
}

fn audit_timeline(connection: &Connection) -> Result<TimelineAudit> {
    let (rows, playouts) = connection.query_row(
        r#"SELECT COUNT(*), COUNT(DISTINCT "PlayoutId") FROM "PlayoutItem""#,
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let (duplicate_slot_groups, duplicate_slot_extras) = connection.query_row(
        r#"SELECT COUNT(*), COALESCE(SUM("RowCount" - 1), 0)
           FROM (
               SELECT COUNT(*) AS "RowCount"
               FROM "PlayoutItem"
               GROUP BY "PlayoutId", "Start", "Finish", "MediaItemId"
               HAVING COUNT(*) > 1
           ) duplicates"#,
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let adjacent_overlaps = connection.query_row(
        r#"WITH ordered AS (
               SELECT "PlayoutId", "Start",
                      LAG("Finish") OVER (
                          PARTITION BY "PlayoutId"
                          ORDER BY "Start", "Finish", "Id"
                      ) AS "PreviousFinish"
               FROM "PlayoutItem"
           )
           SELECT COUNT(*) FROM ordered
           WHERE "PreviousFinish" IS NOT NULL AND "Start" < "PreviousFinish""#,
        [],
        |row| row.get(0),
    )?;
    Ok(TimelineAudit {
        rows,
        playouts,
        duplicate_slot_groups,
        duplicate_slot_extras,
        adjacent_overlaps,
    })
}

fn audit_episodes(connection: &Connection) -> Result<EpisodeAudit> {
    let (logical_episodes, metadata_rows) = connection.query_row(
        r#"SELECT (SELECT COUNT(*) FROM "Episode"),
                  (SELECT COUNT(*) FROM "EpisodeMetadata")"#,
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let joined_thumbnail_rows = connection.query_row(
        r#"SELECT COUNT(*)
           FROM "Episode" e
           JOIN "EpisodeMetadata" em ON em."EpisodeId" = e."Id"
           LEFT JOIN "Artwork" a
             ON a."EpisodeMetadataId" = em."Id" AND a."ArtworkKind" = 1"#,
        [],
        |row| row.get(0),
    )?;
    let (duplicate_artwork_groups, duplicate_artwork_extras) = connection.query_row(
        r#"SELECT COUNT(*), COALESCE(SUM("RowCount" - 1), 0)
           FROM (
               SELECT COUNT(*) AS "RowCount"
               FROM "Artwork"
               WHERE "EpisodeMetadataId" IS NOT NULL
               GROUP BY "EpisodeMetadataId", "ArtworkKind", "Path", "SourcePath"
               HAVING COUNT(*) > 1
           ) duplicates"#,
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let actor_referenced_duplicate_extras = connection.query_row(
        r#"SELECT COALESCE(SUM(duplicate."RowCount" - 1), 0)
           FROM (
               SELECT "EpisodeMetadataId", "ArtworkKind", "Path", "SourcePath",
                      COUNT(*) AS "RowCount"
               FROM "Artwork"
               WHERE "EpisodeMetadataId" IS NOT NULL
               GROUP BY "EpisodeMetadataId", "ArtworkKind", "Path", "SourcePath"
               HAVING COUNT(*) > 1
           ) AS duplicate
           WHERE EXISTS (
               SELECT 1
               FROM "Artwork" AS member
               JOIN "Actor" AS actor ON actor."ArtworkId" = member."Id"
               WHERE member."EpisodeMetadataId" IS duplicate."EpisodeMetadataId"
                 AND member."ArtworkKind" = duplicate."ArtworkKind"
                 AND member."Path" IS duplicate."Path"
                 AND member."SourcePath" IS duplicate."SourcePath"
           )"#,
        [],
        |row| row.get(0),
    )?;
    let mut season_statement = connection.prepare(
        r#"SELECT COUNT(episode."Id")
           FROM "AddonMediaListTelevisionShow" AS projection
           JOIN "Season" AS season ON season."ShowId" = projection."ShowId"
           LEFT JOIN "Episode" AS episode ON episode."SeasonId" = season."Id"
           GROUP BY season."Id", season."SeasonNumber"
           ORDER BY season."SeasonNumber", season."Id""#,
    )?;
    let projected_season_episode_counts = season_statement
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(EpisodeAudit {
        logical_episodes,
        metadata_rows,
        joined_thumbnail_rows,
        multiplied_rows: joined_thumbnail_rows.saturating_sub(metadata_rows),
        duplicate_artwork_groups,
        duplicate_artwork_extras,
        actor_referenced_duplicate_extras,
        projected_season_episode_counts,
    })
}

fn audit_failed_builds(connection: &Connection) -> Result<i64> {
    if !table_exists(connection, "PlayoutBuildStatus")? {
        return Ok(0);
    }
    connection
        .query_row(
            r#"SELECT COUNT(*) FROM "PlayoutBuildStatus" WHERE "Success" = 0"#,
            [],
            |row| row.get(0),
        )
        .context("auditing failed playout builds")
}

fn audit_publication_history(connection: &Connection) -> Result<PublicationHistoryAudit> {
    if !table_exists(connection, "AddonMediaListPublicationEvent")? {
        return Ok(PublicationHistoryAudit::default());
    }

    let now = chrono::Utc::now().to_rfc3339();
    let (
        streams,
        correlations,
        events,
        active,
        recovery_required,
        terminal,
        truncated,
        original_failures,
        recovery_failures,
    ) = connection
        .query_row(
            r#"SELECT COUNT(DISTINCT event."PublicationJobId"),
                      COUNT(DISTINCT job."OperationId"),
                      COUNT(*),
                      COUNT(DISTINCT CASE WHEN operation."State" = 0
                                               OR (operation."State" = 1 AND EXISTS (
                                                   SELECT 1 FROM "ManagedLibraryLease" AS lease
                                                   WHERE lease."OperationId" = operation."Id"
                                                     AND lease."OwnerToken" = operation."OwnerToken"
                                                     AND julianday(lease."LeaseExpiresAt") > julianday(?1)
                                               ))
                                          THEN event."PublicationJobId" END),
                      COUNT(DISTINCT CASE WHEN operation."State" IN (2, 4)
                                               OR (operation."State" = 1 AND NOT EXISTS (
                                                   SELECT 1 FROM "ManagedLibraryLease" AS lease
                                                   WHERE lease."OperationId" = operation."Id"
                                                     AND lease."OwnerToken" = operation."OwnerToken"
                                                     AND julianday(lease."LeaseExpiresAt") > julianday(?1)
                                               ))
                                          THEN event."PublicationJobId" END),
                      COUNT(DISTINCT CASE WHEN operation."State" = 3
                                          THEN event."PublicationJobId" END),
                      COUNT(DISTINCT CASE WHEN event."Kind" = ?2
                                          THEN event."PublicationJobId" END),
                      COALESCE(SUM(CASE WHEN event."Kind" = ?3 THEN 1 ELSE 0 END), 0),
                      COALESCE(SUM(CASE WHEN event."Kind" = ?4 THEN 1 ELSE 0 END), 0)
               FROM "AddonMediaListPublicationEvent" AS event
               JOIN "AddonMediaListPublicationJob" AS job
                 ON job."Id" = event."PublicationJobId"
               JOIN "ManagedLibraryOperation" AS operation
                 ON operation."Id" = job."OperationId""#,
            params![
                now,
                PUBLICATION_EVENT_HISTORY_TRUNCATED,
                PUBLICATION_EVENT_ORIGINAL_FAILURE,
                PUBLICATION_EVENT_RECOVERY_FAILURE,
            ],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                ))
            },
        )
        .context("auditing add-on publication history aggregates")?;

    let mut statement = connection
        .prepare(
            r#"SELECT job."OperationId", operation."State",
                      EXISTS (
                          SELECT 1 FROM "ManagedLibraryLease" AS lease
                          WHERE lease."OperationId" = operation."Id"
                            AND lease."OwnerToken" = operation."OwnerToken"
                            AND julianday(lease."LeaseExpiresAt") > julianday(?1)
                      ),
                      job."Phase", job."Outcome",
                      COUNT(*), MIN(event."Sequence"), MAX(event."Sequence"),
                      COALESCE(SUM(CASE WHEN event."Kind" = ?2 THEN 1 ELSE 0 END), 0),
                      COALESCE(SUM(CASE WHEN event."Kind" = ?3 THEN 1 ELSE 0 END), 0),
                      MAX(CASE WHEN event."Kind" = ?4 THEN 1 ELSE 0 END),
                      (
                          SELECT recent_error."ErrorCategory"
                          FROM "AddonMediaListPublicationEvent" AS recent_error
                          WHERE recent_error."PublicationJobId" = job."Id"
                            AND recent_error."ErrorCategory" IS NOT NULL
                          ORDER BY recent_error."Sequence" DESC
                          LIMIT 1
                      ),
                      (
                          SELECT recent_error."OsErrorCode"
                          FROM "AddonMediaListPublicationEvent" AS recent_error
                          WHERE recent_error."PublicationJobId" = job."Id"
                            AND recent_error."ErrorCategory" IS NOT NULL
                          ORDER BY recent_error."Sequence" DESC
                          LIMIT 1
                      )
               FROM "AddonMediaListPublicationEvent" AS event
               JOIN "AddonMediaListPublicationJob" AS job
                 ON job."Id" = event."PublicationJobId"
               JOIN "ManagedLibraryOperation" AS operation
                 ON operation."Id" = job."OperationId"
               GROUP BY event."PublicationJobId", job."OperationId", operation."State",
                        operation."Id", operation."OwnerToken", job."Phase", job."Outcome",
                        job."Id"
               ORDER BY MAX(event."OccurredAt") DESC, job."Id" DESC
               LIMIT 5"#,
        )
        .context("preparing add-on publication history stream audit")?;
    let recent_streams = statement
        .query_map(
            params![
                now,
                PUBLICATION_EVENT_ORIGINAL_FAILURE,
                PUBLICATION_EVENT_RECOVERY_FAILURE,
                PUBLICATION_EVENT_HISTORY_TRUNCATED,
            ],
            |row| {
                Ok(PublicationHistoryStream {
                    correlation_id: row.get(0)?,
                    state: row.get(1)?,
                    live_lease: row.get(2)?,
                    phase: row.get(3)?,
                    outcome: row.get(4)?,
                    events: row.get(5)?,
                    first_sequence: row.get(6)?,
                    last_sequence: row.get(7)?,
                    original_failures: row.get(8)?,
                    recovery_failures: row.get(9)?,
                    truncated: row.get::<_, i64>(10)? != 0,
                    error_category: row.get(11)?,
                    os_error_code: row.get(12)?,
                })
            },
        )?
        .collect::<rusqlite::Result<Vec<_>>>()
        .context("auditing recent add-on publication history streams")?;

    Ok(PublicationHistoryAudit {
        available: true,
        streams,
        correlations,
        events,
        active,
        recovery_required,
        terminal,
        truncated,
        original_failures,
        recovery_failures,
        recent_streams,
    })
}

fn audit_addon_lifecycle(connection: &Connection) -> Result<AddonLifecycleAudit> {
    if !table_exists(connection, "AddonMediaListLifecycleJob")? {
        return Ok(AddonLifecycleAudit::default());
    }

    let now = chrono::Utc::now().to_rfc3339();
    connection
        .query_row(
            r#"SELECT
                   (SELECT COUNT(*)
                      FROM "AddonMediaListLifecycleJob" AS job
                      JOIN "ManagedLibraryOperation" AS operation
                        ON operation."Id" = job."OperationId"
                     WHERE operation."State" = 0
                        OR (operation."State" = 1 AND EXISTS (
                            SELECT 1 FROM "ManagedLibraryLease" AS lease
                             WHERE lease."OperationId" = operation."Id"
                               AND lease."OwnerToken" = operation."OwnerToken"
                               AND julianday(lease."LeaseExpiresAt") > julianday(?1)
                        ))),
                   (SELECT COUNT(*)
                      FROM "AddonMediaListLifecycleJob" AS job
                      JOIN "ManagedLibraryOperation" AS operation
                        ON operation."Id" = job."OperationId"
                     WHERE operation."State" = 2),
                   (SELECT COUNT(*)
                      FROM "AddonMediaListLifecycleJob" AS job
                      JOIN "ManagedLibraryOperation" AS operation
                        ON operation."Id" = job."OperationId"
                     WHERE operation."State" = 1
                       AND NOT EXISTS (
                           SELECT 1 FROM "ManagedLibraryLease" AS lease
                            WHERE lease."OperationId" = operation."Id"
                              AND lease."OwnerToken" = operation."OwnerToken"
                              AND julianday(lease."LeaseExpiresAt") > julianday(?1)
                       )),
                   (SELECT COUNT(*)
                      FROM "AddonMediaListLifecycleJob" AS job
                      JOIN "ManagedLibraryOperation" AS operation
                        ON operation."Id" = job."OperationId"
                     WHERE operation."State" = 4),
                   (SELECT COUNT(*) FROM "AddonMediaListImportCheckpoint"),
                   (SELECT COUNT(*) FROM "AddonMediaListRevision"),
                   (SELECT COUNT(*) FROM "AddonMediaListItemTombstone"),
                   (SELECT COUNT(*) FROM "AddonMediaListArtifact"),
                   (SELECT COUNT(*) FROM "AddonMediaListArtifact" WHERE "State" IN (0, 1)),
                   (SELECT COUNT(*) FROM "AddonMediaListArtifact" WHERE "State" = 2),
                   (SELECT COUNT(*) FROM "AddonMediaListArtifact" WHERE "State" = 3),
                   (SELECT COALESCE(SUM("ByteLength"), 0) FROM "AddonMediaListArtifact"),
                   (SELECT COALESCE(SUM("ReservedBytes"), 0) FROM "AddonMediaListArtifact"),
                   (SELECT COUNT(*) FROM "AddonToolMaintenanceJob"
                     WHERE "State" = 0
                        OR ("State" = 1 AND "LeaseOwner" IS NOT NULL
                            AND julianday("LeaseExpiresAt") > julianday(?1))),
                   (SELECT COUNT(*) FROM "AddonToolMaintenanceJob" WHERE "State" = 2),
                   (SELECT COUNT(*) FROM "AddonToolMaintenanceJob"
                     WHERE "State" = 1
                       AND ("LeaseOwner" IS NULL OR "LeaseExpiresAt" IS NULL
                            OR julianday("LeaseExpiresAt") <= julianday(?1))),
                   (SELECT COUNT(*) FROM "AddonToolMaintenanceJob" WHERE "State" = 4),
                   (SELECT COUNT(*) FROM "AddonToolMaintenanceJob" WHERE "Outcome" = 0),
                   (SELECT COUNT(*) FROM "AddonToolMaintenanceJob" WHERE "Outcome" = 1),
                   (SELECT COUNT(*) FROM "AddonToolMaintenanceJob" WHERE "Outcome" = 2),
                   (SELECT COUNT(*) FROM "AddonToolMaintenanceJob" WHERE "Outcome" = 3),
                   (SELECT COUNT(*) FROM "AddonToolMaintenanceJob" WHERE "Outcome" = 4)"#,
            params![now],
            |row| {
                Ok(AddonLifecycleAudit {
                    available: true,
                    lifecycle_active: row.get(0)?,
                    lifecycle_waiting: row.get(1)?,
                    lifecycle_expired: row.get(2)?,
                    lifecycle_failed: row.get(3)?,
                    checkpoints: row.get(4)?,
                    retained_revisions: row.get(5)?,
                    tombstones: row.get(6)?,
                    artifacts: row.get(7)?,
                    artifact_pending: row.get(8)?,
                    artifact_ready: row.get(9)?,
                    artifact_failed: row.get(10)?,
                    artifact_bytes: row.get(11)?,
                    artifact_reserved_bytes: row.get(12)?,
                    tool_active: row.get(13)?,
                    tool_waiting: row.get(14)?,
                    tool_expired: row.get(15)?,
                    tool_failed: row.get(16)?,
                    tool_updated: row.get(17)?,
                    tool_already_current: row.get(18)?,
                    tool_manual_update_required: row.get(19)?,
                    tool_rolled_back: row.get(20)?,
                    tool_failed_outcome: row.get(21)?,
                })
            },
        )
        .context("auditing add-on lifecycle foundation aggregates")
}

const fn publication_state_name(state: i64, phase: i64, live_lease: bool) -> &'static str {
    match state {
        0 => "queued",
        1 if !live_lease => "expired",
        1 if matches!(phase, 4..=6) => "restoring",
        1 => "publishing",
        2 => "waiting",
        3 => "complete",
        4 => "failed",
        _ => "unknown",
    }
}

const fn publication_phase_name(phase: i64) -> &'static str {
    match phase {
        0 => "awaiting_lease",
        1 => "staging",
        2 => "scanning",
        3 => "committing",
        4 => "restoring_files",
        5 => "restoring_projection",
        6 => "reconciling",
        7 => "complete",
        8 => "verifying",
        9 => "cleaning_up",
        _ => "unknown",
    }
}

const fn publication_outcome_name(outcome: i64) -> &'static str {
    match outcome {
        0 => "published",
        1 => "restored_after_failure",
        _ => "unknown",
    }
}

const fn publication_error_category_name(category: i64) -> &'static str {
    match category {
        0 => "filesystem_io",
        1 => "permission_denied",
        2 => "sharing_violation",
        3 => "unsafe_filesystem_state",
        4 => "invalid_publication_marker",
        5 => "publication_integrity",
        _ => "unknown",
    }
}

fn print_help() {
    eprintln!("runtime-audit — inspect an ErsatzRS SQLite database read-only");
    eprintln!();
    eprintln!("USAGE:");
    eprintln!("    cargo xtask runtime-audit --db <PATH> [--redact]");
    eprintln!();
    eprintln!("Reports aggregate integrity, Classic anchor, playout timeline,");
    eprintln!("episode/artwork cardinality, and add-on publication history evidence.");
    eprintln!("It never writes rows.");
}

#[cfg(test)]
mod tests {
    use std::{
        env,
        fs::OpenOptions as FsOpenOptions,
        sync::atomic::{AtomicU64, Ordering},
    };

    use super::*;

    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

    struct TempDatabase {
        path: PathBuf,
    }

    impl TempDatabase {
        fn new() -> Self {
            loop {
                let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
                let path = env::temp_dir().join(format!(
                    "ersatzrs-runtime-audit-{}-{id}.db",
                    std::process::id()
                ));
                match FsOpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                {
                    Ok(_) => return Self { path },
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(error) => panic!("create temporary database: {error}"),
                }
            }
        }
    }

    impl Drop for TempDatabase {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.path);
            let _ = fs::remove_file(self.path.with_extension("db-wal"));
            let _ = fs::remove_file(self.path.with_extension("db-shm"));
        }
    }

    #[test]
    fn parses_required_database_and_redaction() {
        let args = vec![
            "--db".to_owned(),
            "fixture.db".to_owned(),
            "--redact".to_owned(),
        ];
        let ParsedOptions::Run(options) = Options::parse(&args).expect("valid options") else {
            panic!("expected run options");
        };
        assert_eq!(options.database, PathBuf::from("fixture.db"));
        assert!(options.redact);
    }

    #[test]
    fn audits_known_defects_without_modifying_the_database() {
        let database = TempDatabase::new();
        seed_fixture(&database.path);
        let before = fs::read(&database.path).expect("read fixture before audit");
        let report = audit_database(&database.path).expect("audit fixture");
        let after = fs::read(&database.path).expect("read fixture after audit");

        assert_eq!(before, after, "read-only audit changed the database file");
        assert_eq!(report.anchors.total, 2);
        assert_eq!(report.anchors.dated, 1);
        assert_eq!(report.anchors.undated, 1);
        assert_eq!(report.timeline.duplicate_slot_extras, 1);
        assert_eq!(report.timeline.adjacent_overlaps, 2);
        assert_eq!(report.episodes.logical_episodes, 2);
        assert_eq!(report.episodes.metadata_rows, 2);
        assert_eq!(report.episodes.joined_thumbnail_rows, 4);
        assert_eq!(report.episodes.duplicate_artwork_extras, 1);
        assert_eq!(report.episodes.actor_referenced_duplicate_extras, 1);
        assert_eq!(report.episodes.projected_season_episode_counts, [2]);
        assert_eq!(report.failed_build_statuses, 1);
        assert!(!report.publication_history.available);
        assert!(!report.addon_lifecycle.available);

        let rendered = report.render(&database.path, true);
        assert!(rendered.contains("database=<redacted>"));
        assert!(rendered.contains("classic_playout=1"));
        assert!(!rendered.contains("playout_id=10"));
        assert!(!rendered.contains(&database.path.display().to_string()));
        assert!(rendered.contains("publication_history available=false"));
        assert!(rendered.contains("addon_lifecycle available=false"));
    }

    #[test]
    fn audits_optional_lifecycle_foundations_as_bounded_private_aggregates() {
        let database = TempDatabase::new();
        seed_fixture(&database.path);
        seed_lifecycle_foundations(&database.path);

        let report = audit_database(&database.path).expect("audit lifecycle foundations");
        assert!(report.addon_lifecycle.available);
        assert_eq!(report.addon_lifecycle.lifecycle_active, 2);
        assert_eq!(report.addon_lifecycle.lifecycle_waiting, 1);
        assert_eq!(report.addon_lifecycle.lifecycle_expired, 1);
        assert_eq!(report.addon_lifecycle.lifecycle_failed, 1);
        assert_eq!(report.addon_lifecycle.checkpoints, 2);
        assert_eq!(report.addon_lifecycle.retained_revisions, 3);
        assert_eq!(report.addon_lifecycle.tombstones, 1);
        assert_eq!(report.addon_lifecycle.artifacts, 4);
        assert_eq!(report.addon_lifecycle.artifact_pending, 2);
        assert_eq!(report.addon_lifecycle.artifact_ready, 1);
        assert_eq!(report.addon_lifecycle.artifact_failed, 1);
        assert_eq!(report.addon_lifecycle.artifact_bytes, 120);
        assert_eq!(report.addon_lifecycle.artifact_reserved_bytes, 60);
        assert_eq!(report.addon_lifecycle.tool_active, 2);
        assert_eq!(report.addon_lifecycle.tool_waiting, 1);
        assert_eq!(report.addon_lifecycle.tool_expired, 1);
        assert_eq!(report.addon_lifecycle.tool_failed, 1);

        for redact in [true, false] {
            let rendered = report.render(&database.path, redact);
            assert!(rendered.contains(
                "addon_lifecycle available=true active=2 waiting=1 expired=1 failed=1 checkpoints=2 retained_revisions=3 tombstones=1"
            ));
            assert!(rendered.contains(
                "addon_artifacts total=4 pending=2 ready=1 failed=1 bytes=120 reserved_bytes=60"
            ));
            assert!(rendered.contains(
                "addon_tool_maintenance active=2 waiting=1 expired=1 failed=1 outcomes_updated=1 outcomes_already_current=1 outcomes_manual_update_required=1 outcomes_rolled_back=1 outcomes_failed=1"
            ));
            for forbidden in [
                "/private/checkpoint.json",
                "/private/artifact.bin",
                "private-checkpoint-digest",
                "private-artifact-digest",
                "private-tool-version",
                "https://private.invalid/tool?token=secret",
            ] {
                assert!(!rendered.contains(forbidden));
            }
        }
    }

    #[test]
    fn audits_bounded_redacted_publication_history() {
        let database = TempDatabase::new();
        seed_fixture(&database.path);
        seed_publication_history(&database.path);

        let report = audit_database(&database.path).expect("audit publication history");
        assert!(report.publication_history.available);
        assert_eq!(report.publication_history.streams, 10);
        assert_eq!(report.publication_history.correlations, 10);
        assert_eq!(report.publication_history.events, 12);
        assert_eq!(report.publication_history.active, 2);
        assert_eq!(report.publication_history.recovery_required, 3);
        assert_eq!(report.publication_history.terminal, 5);
        assert_eq!(report.publication_history.truncated, 1);
        assert_eq!(report.publication_history.original_failures, 1);
        assert_eq!(report.publication_history.recovery_failures, 1);
        assert_eq!(report.publication_history.recent_streams.len(), 5);
        assert_eq!(
            report.publication_history.recent_streams[0].correlation_id,
            110
        );

        let redacted = report.render(&database.path, true);
        assert!(redacted.contains("publication_correlation=106"));
        assert!(redacted.contains("error_category=sharing_violation os_error_code=32"));
        assert!(
            redacted.contains("publication_correlation=107 events=1 sequence=1..1 state=waiting")
        );
        assert!(
            redacted.contains("publication_correlation=108 events=1 sequence=1..1 state=expired")
        );
        assert!(redacted
            .contains("publication_correlation=109 events=1 sequence=1..1 state=publishing"));
        assert!(
            redacted.contains("publication_correlation=110 events=1 sequence=1..1 state=queued")
        );

        let unredacted = report.render(&database.path, false);
        assert!(unredacted.contains("publication_correlation=106"));
        for forbidden in [
            "https://provider.invalid/list?token=secret",
            "/private/media/title.mkv",
            "cookie=session-secret",
        ] {
            assert!(!redacted.contains(forbidden));
            assert!(!unredacted.contains(forbidden));
        }
    }

    #[test]
    fn rejects_an_uninitialized_sqlite_file() {
        let database = TempDatabase::new();
        Connection::open(&database.path).expect("initialize SQLite header");
        let error = audit_database(&database.path).expect_err("missing schema must fail");
        assert!(error.to_string().contains("missing table"));
    }

    #[test]
    fn redacts_the_database_path_when_an_audit_fails() {
        let database = TempDatabase::new();
        Connection::open(&database.path).expect("initialize SQLite header");
        let options = Options {
            database: database.path.clone(),
            redact: true,
        };
        let error = audit_for_options(&options).expect_err("missing schema must fail");
        let rendered = format!("{error:#}");
        assert!(rendered.contains("redacted mode"));
        assert!(!rendered.contains(&database.path.display().to_string()));
    }

    fn seed_fixture(path: &Path) {
        let connection = Connection::open(path).expect("open fixture");
        connection
            .execute_batch(
                r#"
                CREATE TABLE "Playout" (
                    "Id" INTEGER PRIMARY KEY,
                    "ScheduleKind" INTEGER NOT NULL
                );
                CREATE TABLE "PlayoutProgramScheduleAnchor" (
                    "Id" INTEGER PRIMARY KEY,
                    "PlayoutId" INTEGER NOT NULL,
                    "AnchorDate" TEXT NULL
                );
                CREATE TABLE "PlayoutItem" (
                    "Id" INTEGER PRIMARY KEY,
                    "PlayoutId" INTEGER NOT NULL,
                    "MediaItemId" INTEGER NOT NULL,
                    "Start" TEXT NOT NULL,
                    "Finish" TEXT NOT NULL
                );
                CREATE TABLE "PlayoutBuildStatus" (
                    "PlayoutId" INTEGER PRIMARY KEY,
                    "Success" INTEGER NOT NULL
                );
                CREATE TABLE "Actor" (
                    "Id" INTEGER PRIMARY KEY,
                    "ArtworkId" INTEGER NULL
                );
                CREATE TABLE "AddonMediaListTelevisionShow" (
                    "ShowId" INTEGER NOT NULL
                );
                CREATE TABLE "Season" (
                    "Id" INTEGER PRIMARY KEY,
                    "ShowId" INTEGER NOT NULL,
                    "SeasonNumber" INTEGER NOT NULL
                );
                CREATE TABLE "Episode" (
                    "Id" INTEGER PRIMARY KEY,
                    "SeasonId" INTEGER NOT NULL
                );
                CREATE TABLE "EpisodeMetadata" (
                    "Id" INTEGER PRIMARY KEY,
                    "EpisodeId" INTEGER NOT NULL
                );
                CREATE TABLE "Artwork" (
                    "Id" INTEGER PRIMARY KEY,
                    "EpisodeMetadataId" INTEGER NULL,
                    "ArtworkKind" INTEGER NOT NULL,
                    "Path" TEXT NULL,
                    "SourcePath" TEXT NULL
                );
                CREATE TABLE _sqlx_migrations (
                    version BIGINT PRIMARY KEY,
                    success BOOLEAN NOT NULL
                );

                INSERT INTO "Playout" VALUES (10, 1), (20, 1);
                INSERT INTO "PlayoutProgramScheduleAnchor" VALUES
                    (1, 10, NULL),
                    (2, 20, '2026-08-23T00:00:00Z');
                INSERT INTO "PlayoutItem" VALUES
                    (1, 10, 100, '2026-08-23T00:00:00Z', '2026-08-23T01:00:00Z'),
                    (2, 10, 100, '2026-08-23T00:00:00Z', '2026-08-23T01:00:00Z'),
                    (3, 10, 101, '2026-08-23T00:30:00Z', '2026-08-23T01:30:00Z');
                INSERT INTO "PlayoutBuildStatus" VALUES (10, 0);
                INSERT INTO "AddonMediaListTelevisionShow" VALUES (1);
                INSERT INTO "Season" VALUES (10, 1, 1);
                INSERT INTO "Episode" VALUES (1, 10), (2, 10);
                INSERT INTO "EpisodeMetadata" VALUES (100, 1), (200, 2);
                INSERT INTO "Artwork" VALUES
                    (1, 100, 1, 'thumb-a.jpg', 'provider-a'),
                    (2, 100, 1, 'thumb-a.jpg', 'provider-a'),
                    (3, 100, 1, 'thumb-b.jpg', 'provider-b');
                INSERT INTO "Actor" VALUES (1, 2);
                INSERT INTO _sqlx_migrations VALUES (202608230001, 1);
                "#,
            )
            .expect("seed runtime audit fixture");
    }

    fn seed_publication_history(path: &Path) {
        let connection = Connection::open(path).expect("open publication history fixture");
        connection
            .execute_batch(
                r#"
                CREATE TABLE "ManagedLibraryOperation" (
                    "Id" INTEGER PRIMARY KEY,
                    "OwnerToken" TEXT NOT NULL,
                    "State" INTEGER NOT NULL
                );
                CREATE TABLE "ManagedLibraryLease" (
                    "OperationId" INTEGER NOT NULL,
                    "OwnerToken" TEXT NOT NULL,
                    "LeaseExpiresAt" TEXT NOT NULL
                );
                CREATE TABLE "AddonMediaListPublicationJob" (
                    "Id" INTEGER PRIMARY KEY,
                    "OperationId" INTEGER NOT NULL,
                    "Phase" INTEGER NOT NULL,
                    "Outcome" INTEGER NULL,
                    "OriginalFailureDiagnostic" TEXT NULL,
                    "RecoveryDiagnostic" TEXT NULL
                );
                CREATE TABLE "AddonMediaListPublicationEvent" (
                    "Id" INTEGER PRIMARY KEY,
                    "PublicationJobId" INTEGER NOT NULL,
                    "Sequence" INTEGER NOT NULL,
                    "DedupeKey" TEXT NOT NULL,
                    "Kind" INTEGER NOT NULL,
                    "Phase" INTEGER NULL,
                    "Attempt" INTEGER NULL,
                    "EnqueueTrigger" INTEGER NULL,
                    "RelatedOperationId" INTEGER NULL,
                    "FilesystemTransition" INTEGER NULL,
                    "RetryClass" INTEGER NULL,
                    "RecoveryAction" INTEGER NULL,
                    "ErrorCategory" INTEGER NULL,
                    "OsErrorCode" INTEGER NULL,
                    "PublishedItemCount" INTEGER NULL,
                    "PlaylistItemCount" INTEGER NULL,
                    "ArtworkCount" INTEGER NULL,
                    "Outcome" INTEGER NULL,
                    "OccurredAt" TEXT NOT NULL
                );

                INSERT INTO "ManagedLibraryOperation" VALUES
                    (101, 'owner-101', 3), (102, 'owner-102', 3),
                    (103, 'owner-103', 3), (104, 'owner-104', 3),
                    (105, 'owner-105', 3), (106, 'owner-106', 4),
                    (107, 'owner-107', 2), (108, 'owner-108', 1),
                    (109, 'owner-109', 1), (110, 'owner-110', 0);
                INSERT INTO "ManagedLibraryLease" VALUES
                    (108, 'stale-owner-108', '2999-01-01T00:00:00Z'),
                    (109, 'owner-109', '2999-01-01T00:00:00Z');
                INSERT INTO "AddonMediaListPublicationJob" VALUES
                    (1, 101, 7, 0, NULL, NULL),
                    (2, 102, 7, 0, NULL, NULL),
                    (3, 103, 7, 0, NULL, NULL),
                    (4, 104, 7, 0, NULL, NULL),
                    (5, 105, 7, 0, NULL, NULL),
                    (6, 106, 4, NULL,
                     'https://provider.invalid/list?token=secret',
                     '/private/media/title.mkv cookie=session-secret'),
                    (7, 107, 4, NULL, NULL, NULL),
                    (8, 108, 1, NULL, NULL, NULL),
                    (9, 109, 2, NULL, NULL, NULL),
                    (10, 110, 0, NULL, NULL, NULL);
                INSERT INTO "AddonMediaListPublicationEvent" (
                    "Id", "PublicationJobId", "Sequence", "DedupeKey", "Kind",
                    "Phase", "ErrorCategory", "OsErrorCode", "OccurredAt"
                ) VALUES
                    (1, 1, 1, 'completed-1', 13, 7, NULL, NULL, '2026-09-01T00:00:00Z'),
                    (2, 2, 1, 'completed-2', 13, 7, NULL, NULL, '2026-09-02T00:00:00Z'),
                    (3, 3, 1, 'completed-3', 13, 7, NULL, NULL, '2026-09-03T00:00:00Z'),
                    (4, 4, 1, 'completed-4', 13, 7, NULL, NULL, '2026-09-04T00:00:00Z'),
                    (5, 5, 1, 'completed-5', 13, 7, NULL, NULL, '2026-09-05T00:00:00Z'),
                    (6, 6, 1, 'failure-original', 9, 4, 2, 32, '2026-09-06T00:00:00Z'),
                    (7, 6, 2, 'failure-recovery', 10, 4, 2, 32, '2026-09-06T00:01:00Z'),
                    (8, 6, 256, 'truncated', 14, 4, NULL, NULL, '2026-09-06T00:02:00Z'),
                    (9, 7, 1, 'waiting', 11, 4, NULL, NULL, '2026-09-07T00:00:00Z'),
                    (10, 8, 1, 'expired', 5, 1, NULL, NULL, '2026-09-08T00:00:00Z'),
                    (11, 9, 1, 'publishing', 5, 2, NULL, NULL, '2026-09-09T00:00:00Z'),
                    (12, 10, 1, 'queued', 1, 0, NULL, NULL, '2026-09-10T00:00:00Z');
                "#,
            )
            .expect("seed publication history fixture");
    }

    fn seed_lifecycle_foundations(path: &Path) {
        let connection = Connection::open(path).expect("open lifecycle foundation fixture");
        connection
            .execute_batch(
                r#"
                CREATE TABLE "ManagedLibraryOperation" (
                    "Id" INTEGER PRIMARY KEY,
                    "OwnerToken" TEXT NOT NULL,
                    "State" INTEGER NOT NULL
                );
                CREATE TABLE "ManagedLibraryLease" (
                    "OperationId" INTEGER NOT NULL,
                    "OwnerToken" TEXT NOT NULL,
                    "LeaseExpiresAt" TEXT NOT NULL
                );
                CREATE TABLE "AddonMediaListLifecycleJob" (
                    "Id" INTEGER PRIMARY KEY,
                    "OperationId" INTEGER NOT NULL
                );
                CREATE TABLE "AddonMediaListImportCheckpoint" (
                    "Id" INTEGER PRIMARY KEY,
                    "RelativePath" TEXT NOT NULL,
                    "Sha256" TEXT NOT NULL
                );
                CREATE TABLE "AddonMediaListRevision" (
                    "Id" INTEGER PRIMARY KEY,
                    "OptionValuesJson" TEXT NOT NULL
                );
                CREATE TABLE "AddonMediaListItemTombstone" (
                    "AddonMediaListItemId" INTEGER PRIMARY KEY
                );
                CREATE TABLE "AddonMediaListArtifact" (
                    "Id" INTEGER PRIMARY KEY,
                    "State" INTEGER NOT NULL,
                    "RelativePath" TEXT NULL,
                    "ReservedBytes" INTEGER NOT NULL,
                    "ByteLength" INTEGER NULL,
                    "Sha256" TEXT NULL,
                    "DiagnosticCode" TEXT NULL
                );
                CREATE TABLE "AddonToolMaintenanceJob" (
                    "Id" INTEGER PRIMARY KEY,
                    "State" INTEGER NOT NULL,
                    "LeaseOwner" TEXT NULL,
                    "LeaseExpiresAt" TEXT NULL,
                    "BeforeVersion" TEXT NULL,
                    "BeforeSha256" TEXT NULL,
                    "AfterVersion" TEXT NULL,
                    "AfterSha256" TEXT NULL,
                    "Outcome" INTEGER NULL,
                    "DiagnosticCode" TEXT NULL,
                    "Diagnostic" TEXT NULL
                );

                INSERT INTO "ManagedLibraryOperation" VALUES
                    (1, 'owner-1', 0),
                    (2, 'owner-2', 1),
                    (3, 'owner-3', 2),
                    (4, 'owner-4', 1),
                    (5, 'owner-5', 4);
                INSERT INTO "ManagedLibraryLease" VALUES
                    (2, 'owner-2', '2999-01-01T00:00:00Z'),
                    (4, 'stale-owner', '2999-01-01T00:00:00Z');
                INSERT INTO "AddonMediaListLifecycleJob" VALUES
                    (1, 1), (2, 2), (3, 3), (4, 4), (5, 5);
                INSERT INTO "AddonMediaListImportCheckpoint" VALUES
                    (1, '/private/checkpoint.json', 'private-checkpoint-digest'),
                    (2, '/private/checkpoint-2.json', 'private-checkpoint-digest-2');
                INSERT INTO "AddonMediaListRevision" VALUES
                    (1, '{"secret":"private-option"}'),
                    (2, '{}'),
                    (3, '{}');
                INSERT INTO "AddonMediaListItemTombstone" VALUES (1);
                INSERT INTO "AddonMediaListArtifact" VALUES
                    (1, 0, '/private/artifact.bin', 50, NULL,
                     'private-artifact-digest', 'private-artifact-code'),
                    (2, 1, NULL, 10, NULL, NULL, NULL),
                    (3, 2, NULL, 0, 100, NULL, NULL),
                    (4, 3, NULL, 0, 20, NULL, 'private-artifact-failure');
                INSERT INTO "AddonToolMaintenanceJob" VALUES
                    (1, 0, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL),
                    (2, 1, 'worker', '2999-01-01T00:00:00Z', NULL, NULL, NULL, NULL, NULL, NULL, NULL),
                    (3, 2, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL),
                    (4, 1, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL),
                    (5, 4, NULL, NULL, NULL, NULL, NULL, NULL, 4, NULL, 'https://private.invalid/tool?token=secret'),
                    (6, 3, NULL, NULL, 'private-tool-version', 'private-before-digest', 'private-tool-version-2', 'private-after-digest', 0, NULL, NULL),
                    (7, 3, NULL, NULL, NULL, NULL, NULL, NULL, 1, NULL, NULL),
                    (8, 3, NULL, NULL, NULL, NULL, NULL, NULL, 2, NULL, NULL),
                    (9, 3, NULL, NULL, NULL, NULL, NULL, NULL, 3, NULL, NULL);
                "#,
            )
            .expect("seed lifecycle foundation fixture");
    }
}
