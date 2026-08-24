//! Read-only aggregate audit for an ErsatzRS SQLite runtime database.

use std::{
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
    time::Duration,
};

use anyhow::{bail, Context, Result};
use rusqlite::{Connection, OpenFlags};

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

fn print_help() {
    eprintln!("runtime-audit — inspect an ErsatzRS SQLite database read-only");
    eprintln!();
    eprintln!("USAGE:");
    eprintln!("    cargo xtask runtime-audit --db <PATH> [--redact]");
    eprintln!();
    eprintln!("Reports aggregate integrity, Classic anchor, playout timeline,");
    eprintln!("and episode/artwork cardinality evidence. It never writes rows.");
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

        let rendered = report.render(&database.path, true);
        assert!(rendered.contains("database=<redacted>"));
        assert!(rendered.contains("classic_playout=1"));
        assert!(!rendered.contains("playout_id=10"));
        assert!(!rendered.contains(&database.path.display().to_string()));
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
}
