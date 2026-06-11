//! `cargo xtask progress` — derived SQLite index over `docs/progress/`.
//!
//! The text files under `docs/progress/` are the source of truth. This command
//! builds a gitignored `progress.db` from them so agents and humans can pull
//! exactly the state they need cheaply, instead of loading large markdown
//! files. The DB is rebuilt automatically whenever a source file is newer than
//! the database, so reads always reflect the committed text.
//!
//! All paths are resolved relative to the current working directory, which is
//! the workspace root when invoked as `cargo xtask progress …` (same
//! convention as the other xtask subcommands).

use std::{
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection};
use serde::Deserialize;
use walkdir::WalkDir;

const PROGRESS_DIR: &str = "docs/progress";
const ADR_DIR: &str = "docs/adr";
const DB_PATH: &str = "progress.db";

const STATUSES: [&str; 5] = [
    "Not started",
    "In progress",
    "Ported",
    "Deferred",
    "Dropped",
];
const HYDRATION_STATUSES: [&str; 3] = ["✅", "TODO", "⏳"];
const SUMMARY_WARN_LIMIT: usize = 4_000;

pub fn run(args: Vec<String>) -> ExitCode {
    let mut it = args.into_iter();
    let sub = it.next().unwrap_or_default();
    let rest: Vec<String> = it.collect();
    let result = match sub.as_str() {
        "sync" => cmd_sync(),
        "next" => cmd_next(),
        "parity" => cmd_parity(&rest),
        "gaps" => cmd_gaps(),
        "routes" => cmd_routes(&rest),
        "hydration" => cmd_hydration(&rest),
        "sessions" => cmd_sessions(&rest),
        "adr" => cmd_adr(&rest),
        "add-session" => cmd_add_session(&rest),
        "check" => cmd_check(),
        "" | "-h" | "--help" | "help" => {
            print_help();
            return ExitCode::SUCCESS;
        }
        other => {
            eprintln!("progress: unknown subcommand `{other}`");
            print_help();
            return ExitCode::FAILURE;
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("progress: {err:#}");
            ExitCode::FAILURE
        }
    }
}

// ----------------------------------------------------------------- model

#[derive(Deserialize)]
struct ParityFile {
    feature: String,
    etv_source: String,
    rs_target: String,
    status: String,
    #[serde(default)]
    adr: Vec<String>,
    #[serde(default)]
    debt: Vec<Debt>,
    summary: String,
}

struct ParityRow {
    slug: String,
    file: ParityFile,
}

#[derive(Deserialize)]
struct Debt {
    id: String,
    description: String,
    discovered: String,
    evidence: String,
    #[serde(default)]
    resolved: bool,
}

impl ParityFile {
    fn open_debts(&self) -> impl Iterator<Item = &Debt> {
        self.debt.iter().filter(|debt| !debt.resolved)
    }
}

#[derive(Deserialize)]
struct RoutesFile {
    #[serde(default, rename = "route")]
    routes: Vec<Route>,
}

#[derive(Deserialize)]
struct Route {
    nav: String,
    backing: String,
    routed: bool,
}

#[derive(Deserialize)]
struct HydrationFile {
    #[serde(default, rename = "page")]
    pages: Vec<HydrationPage>,
}

#[derive(Deserialize)]
struct HydrationPage {
    name: String,
    status: String,
    notes: String,
}

#[derive(Deserialize)]
struct Phase {
    current_phase: String,
    current_focus: String,
    summary: String,
    next_steps: String,
    #[serde(default)]
    checklist: Vec<ChecklistItem>,
    #[serde(default)]
    known_todo: Vec<Note>,
    #[serde(default)]
    blocking_decision: Vec<Note>,
}

#[derive(Deserialize)]
struct ChecklistItem {
    item: String,
    done: bool,
}

#[derive(Deserialize)]
struct Note {
    note: String,
}

#[derive(Deserialize)]
struct Session {
    date: String,
    agent: String,
    summary: String,
}

struct Adr {
    number: String,
    title: String,
    status: String,
    path: String,
}

// ------------------------------------------------------------- text load

fn load_parity() -> Result<Vec<ParityRow>> {
    let dir = Path::new(PROGRESS_DIR).join("parity");
    let mut rows = Vec::new();
    let mut entries: Vec<PathBuf> = fs::read_dir(&dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .collect();
    entries.sort();
    for path in entries {
        let text = fs::read_to_string(&path)?;
        let file: ParityFile =
            toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        let slug = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_owned();
        rows.push(ParityRow { slug, file });
    }
    Ok(rows)
}

fn load_phase() -> Result<Phase> {
    let path = Path::new(PROGRESS_DIR).join("phase.toml");
    let text = fs::read_to_string(&path)?;
    toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

fn load_routes() -> Result<Vec<Route>> {
    let path = Path::new(PROGRESS_DIR).join("routes.toml");
    let text = fs::read_to_string(&path)?;
    let parsed: RoutesFile =
        toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    Ok(parsed.routes)
}

fn load_hydration() -> Result<Vec<HydrationPage>> {
    let path = Path::new(PROGRESS_DIR).join("hydration.toml");
    let text = fs::read_to_string(&path)?;
    let parsed: HydrationFile =
        toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    Ok(parsed.pages)
}

fn load_sessions() -> Result<Vec<Session>> {
    let path = Path::new(PROGRESS_DIR).join("sessions.ndjson");
    let text = fs::read_to_string(&path)?;
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let s: Session = serde_json::from_str(line)
            .with_context(|| format!("{}:{}: invalid NDJSON", path.display(), i + 1))?;
        out.push(s);
    }
    Ok(out)
}

fn load_adrs() -> Result<Vec<Adr>> {
    let mut out = Vec::new();
    let mut entries: Vec<PathBuf> = fs::read_dir(ADR_DIR)
        .with_context(|| format!("reading {ADR_DIR}"))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .filter(|p| {
            p.file_name().and_then(|s| s.to_str()).is_some_and(|n| {
                n.starts_with(|c: char| c.is_ascii_digit()) && n != "0000-template.md"
            })
        })
        .collect();
    entries.sort();
    for path in entries {
        let text = fs::read_to_string(&path)?;
        let number = path
            .file_name()
            .and_then(|s| s.to_str())
            .and_then(|n| n.split('-').next())
            .unwrap_or_default()
            .to_owned();
        let title = text
            .lines()
            .find_map(|l| l.strip_prefix("# ADR "))
            .map(|t| t.split_once(':').map_or(t, |x| x.1).trim().to_owned())
            .unwrap_or_default();
        let status = adr_status(&text);
        out.push(Adr {
            number,
            title,
            status,
            path: path.display().to_string(),
        });
    }
    Ok(out)
}

fn adr_status(text: &str) -> String {
    let mut lines = text.lines();
    while let Some(l) = lines.next() {
        if l.trim() == "## Status" {
            for next in lines.by_ref() {
                let t = next.trim();
                if !t.is_empty() {
                    return t.to_owned();
                }
            }
        }
    }
    "Unknown".to_owned()
}

// -------------------------------------------------------------- staleness

fn max_source_mtime() -> Result<i64> {
    let mut max = 0i64;
    for root in [PROGRESS_DIR, ADR_DIR] {
        for entry in WalkDir::new(root)
            .into_iter()
            .filter_map(std::result::Result::ok)
        {
            if entry.file_type().is_file() {
                if let Ok(meta) = entry.metadata() {
                    if let Ok(modified) = meta.modified() {
                        let secs = modified
                            .duration_since(UNIX_EPOCH)
                            .map(|d| d.as_secs() as i64)
                            .unwrap_or(0);
                        max = max.max(secs);
                    }
                }
            }
        }
    }
    Ok(max)
}

/// Rebuild the DB if it is missing or older than any source file.
fn ensure_synced() -> Result<Connection> {
    let source_mtime = max_source_mtime()?;
    if Path::new(DB_PATH).exists() {
        let conn = Connection::open(DB_PATH)?;
        let stored: Option<i64> = conn
            .query_row("SELECT source_max_mtime FROM meta LIMIT 1", [], |r| {
                r.get(0)
            })
            .ok();
        if stored == Some(source_mtime) {
            return Ok(conn);
        }
    }
    sync()
}

// ------------------------------------------------------------------- sync

fn sync() -> Result<Connection> {
    let parity = load_parity()?;
    let phase = load_phase()?;
    let routes = load_routes()?;
    let hydration = load_hydration()?;
    let sessions = load_sessions()?;
    let adrs = load_adrs()?;
    let source_mtime = max_source_mtime()?;

    // Rebuild from scratch: the DB is a disposable index, so no migrations.
    let _ = fs::remove_file(DB_PATH);
    let mut conn = Connection::open(DB_PATH)?;
    conn.execute_batch(
        "CREATE TABLE parity(slug TEXT PRIMARY KEY, feature TEXT, etv_source TEXT, rs_target TEXT, status TEXT, summary TEXT);
         CREATE TABLE parity_adr(slug TEXT, adr TEXT);
         CREATE TABLE parity_debt(slug TEXT, id TEXT, description TEXT, discovered TEXT, evidence TEXT, resolved INTEGER);
         CREATE TABLE routes(nav TEXT, backing TEXT, routed INTEGER);
         CREATE TABLE hydration(name TEXT, status TEXT, notes TEXT);
         CREATE TABLE sessions(ord INTEGER, date TEXT, agent TEXT, summary TEXT);
         CREATE TABLE adrs(number TEXT, title TEXT, status TEXT, path TEXT);
         CREATE TABLE phase(key TEXT, value TEXT);
         CREATE TABLE phase_checklist(item TEXT, done INTEGER);
         CREATE TABLE phase_todo(note TEXT);
         CREATE TABLE phase_decision(note TEXT);
         CREATE TABLE meta(synced_at TEXT, source_max_mtime INTEGER);",
    )?;

    let tx = conn.transaction()?;
    for row in &parity {
        tx.execute(
            "INSERT INTO parity VALUES(?1,?2,?3,?4,?5,?6)",
            params![
                row.slug,
                row.file.feature,
                row.file.etv_source,
                row.file.rs_target,
                row.file.status,
                row.file.summary
            ],
        )?;
        for adr in &row.file.adr {
            tx.execute(
                "INSERT INTO parity_adr VALUES(?1,?2)",
                params![row.slug, adr],
            )?;
        }
        for debt in &row.file.debt {
            tx.execute(
                "INSERT INTO parity_debt VALUES(?1,?2,?3,?4,?5,?6)",
                params![
                    row.slug,
                    debt.id,
                    debt.description,
                    debt.discovered,
                    debt.evidence,
                    i32::from(debt.resolved)
                ],
            )?;
        }
    }
    for r in &routes {
        tx.execute(
            "INSERT INTO routes VALUES(?1,?2,?3)",
            params![r.nav, r.backing, i32::from(r.routed)],
        )?;
    }
    for p in &hydration {
        tx.execute(
            "INSERT INTO hydration VALUES(?1,?2,?3)",
            params![p.name, p.status, p.notes],
        )?;
    }
    for (i, s) in sessions.iter().enumerate() {
        tx.execute(
            "INSERT INTO sessions VALUES(?1,?2,?3,?4)",
            params![i as i64, s.date, s.agent, s.summary],
        )?;
    }
    for a in &adrs {
        tx.execute(
            "INSERT INTO adrs VALUES(?1,?2,?3,?4)",
            params![a.number, a.title, a.status, a.path],
        )?;
    }
    for (key, value) in [
        ("current_phase", &phase.current_phase),
        ("current_focus", &phase.current_focus),
        ("summary", &phase.summary),
        ("next_steps", &phase.next_steps),
    ] {
        tx.execute("INSERT INTO phase VALUES(?1,?2)", params![key, value])?;
    }
    for c in &phase.checklist {
        tx.execute(
            "INSERT INTO phase_checklist VALUES(?1,?2)",
            params![c.item, i32::from(c.done)],
        )?;
    }
    for n in &phase.known_todo {
        tx.execute("INSERT INTO phase_todo VALUES(?1)", params![n.note])?;
    }
    for n in &phase.blocking_decision {
        tx.execute("INSERT INTO phase_decision VALUES(?1)", params![n.note])?;
    }
    tx.execute(
        "INSERT INTO meta VALUES(?1,?2)",
        params![today_iso(), source_mtime],
    )?;
    tx.commit()?;
    Ok(conn)
}

fn cmd_sync() -> Result<()> {
    let conn = sync()?;
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM parity", [], |r| r.get(0))?;
    let s: i64 = conn.query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))?;
    let a: i64 = conn.query_row("SELECT COUNT(*) FROM adrs", [], |r| r.get(0))?;
    println!("progress: synced {DB_PATH} — {n} features, {s} sessions, {a} ADRs");
    Ok(())
}

// --------------------------------------------------------------- commands

fn cmd_next() -> Result<()> {
    let conn = ensure_synced()?;
    let phase = phase_value(&conn, "current_phase")?;
    let focus = phase_value(&conn, "current_focus")?;
    let summary = phase_value(&conn, "summary")?;
    let next_steps = phase_value(&conn, "next_steps")?;
    println!("{phase}");
    println!("Focus: {focus}");
    if !summary.trim().is_empty() {
        println!("\n{}", summary.trim());
    }

    for status in ["In progress", "Not started"] {
        let mut stmt =
            conn.prepare("SELECT feature FROM parity WHERE status = ?1 ORDER BY feature")?;
        let features: Vec<String> = stmt
            .query_map([status], |r| r.get::<_, String>(0))?
            .filter_map(std::result::Result::ok)
            .collect();
        if !features.is_empty() {
            println!("\n{} ({}):", status, features.len());
            for f in features {
                println!("  - {f}");
            }
        }
    }

    if !next_steps.trim().is_empty() {
        println!("\nNext steps:\n{}", next_steps.trim());
    }

    let latest: Option<(String, String, String)> = conn
        .query_row(
            "SELECT date, agent, summary FROM sessions ORDER BY ord LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .ok();
    if let Some((date, agent, summary)) = latest {
        println!(
            "\nLatest session ({date}, {agent}):\n  {}",
            truncate(&summary, 280)
        );
    }
    Ok(())
}

fn cmd_parity(args: &[String]) -> Result<()> {
    let conn = ensure_synced()?;
    let status = flag_value(args, "--status");
    let feature = flag_value(args, "--feature");
    let full = args.iter().any(|a| a == "--full");

    if let Some(slug) = feature {
        let row: Option<(String, String, String, String, String)> = conn
            .query_row(
                "SELECT feature, etv_source, rs_target, status, summary FROM parity WHERE slug = ?1",
                [&slug],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .ok();
        let Some((feature, source, target, status, summary)) = row else {
            bail!("no parity feature with slug `{slug}` (try `cargo xtask progress parity`)");
        };
        let mut stmt = conn.prepare("SELECT adr FROM parity_adr WHERE slug = ?1 ORDER BY adr")?;
        let adrs: Vec<String> = stmt
            .query_map([&slug], |r| r.get::<_, String>(0))?
            .filter_map(std::result::Result::ok)
            .collect();
        let mut stmt = conn.prepare(
            "SELECT id, description, discovered, evidence, resolved FROM parity_debt WHERE slug = ?1 ORDER BY resolved, discovered, id",
        )?;
        let debts: Vec<(String, String, String, String, i64)> = stmt
            .query_map([&slug], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })?
            .filter_map(std::result::Result::ok)
            .collect();
        println!("{feature}  [{status}]");
        println!("  slug:   {slug}");
        println!("  source: {source}");
        println!("  target: {target}");
        println!(
            "  adr:    {}",
            if adrs.is_empty() {
                "-".to_owned()
            } else {
                adrs.join(", ")
            }
        );
        if !debts.is_empty() {
            println!("  debt:");
            for (id, description, discovered, evidence, resolved) in debts {
                let state = if resolved == 0 { "open" } else { "resolved" };
                println!(
                    "    - {id} [{state}, {discovered}]: {}",
                    truncate(&description, 100)
                );
                println!("      evidence: {}", truncate(&evidence, 120));
            }
        }
        if full {
            println!("\n{}", summary.trim());
        }
        return Ok(());
    }

    let (sql, bound): (String, Vec<String>) = match &status {
        Some(s) => (
            "SELECT feature, status FROM parity WHERE status = ?1 ORDER BY feature".to_owned(),
            vec![s.clone()],
        ),
        None => (
            "SELECT feature, status FROM parity ORDER BY status, feature".to_owned(),
            vec![],
        ),
    };
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(bound.iter()), |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (feature, status) = row?;
        println!("{:<14} {feature}", format!("[{status}]"));
    }
    Ok(())
}

fn cmd_gaps() -> Result<()> {
    let conn = ensure_synced()?;
    let mut stmt = conn.prepare(
        "SELECT status, feature, slug FROM parity WHERE status != 'Ported' ORDER BY status, feature",
    )?;
    let rows: Vec<(String, String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .filter_map(std::result::Result::ok)
        .collect();
    if rows.is_empty() {
        println!("No gaps: every tracked feature is Ported. 🎉");
        return Ok(());
    }
    println!(
        "{} feature(s) remaining for a feature-complete port:\n",
        rows.len()
    );
    for (status, feature, slug) in rows {
        println!("  [{status:<11}] {feature}  ({slug})");
        let mut debts = conn.prepare(
            "SELECT id, description, discovered FROM parity_debt WHERE slug = ?1 AND resolved = 0 ORDER BY discovered, id",
        )?;
        let debts: Vec<(String, String, String)> = debts
            .query_map([&slug], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .filter_map(std::result::Result::ok)
            .collect();
        for (id, description, discovered) in debts {
            println!(
                "      debt {id} ({discovered}): {}",
                truncate(&description, 120)
            );
        }
    }
    Ok(())
}

fn cmd_routes(args: &[String]) -> Result<()> {
    let conn = ensure_synced()?;
    let only_unrouted = args.iter().any(|a| a == "--unrouted");
    let sql = if only_unrouted {
        "SELECT nav, backing, routed FROM routes WHERE routed = 0 ORDER BY nav"
    } else {
        "SELECT nav, backing, routed FROM routes ORDER BY routed DESC, nav"
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
        ))
    })?;
    for row in rows {
        let (nav, backing, routed) = row?;
        let mark = if routed == 1 { "✓" } else { "404" };
        println!("{mark:<4} {nav}  ->  {}", truncate(&backing, 100));
    }
    Ok(())
}

fn cmd_hydration(args: &[String]) -> Result<()> {
    let conn = ensure_synced()?;
    let only_pending = args.iter().any(|a| a == "--pending");
    let sql = if only_pending {
        "SELECT name, status, notes FROM hydration WHERE status != '✅' ORDER BY name"
    } else {
        "SELECT name, status, notes FROM hydration ORDER BY name"
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })?;
    for row in rows {
        let (name, status, notes) = row?;
        println!("{status:<5} {name:<28} {}", truncate(&notes, 80));
    }
    Ok(())
}

fn cmd_sessions(args: &[String]) -> Result<()> {
    let conn = ensure_synced()?;
    let limit: i64 = flag_value(args, "--limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);
    let mut stmt =
        conn.prepare("SELECT date, agent, summary FROM sessions ORDER BY ord LIMIT ?1")?;
    let rows = stmt.query_map([limit], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })?;
    for row in rows {
        let (date, agent, summary) = row?;
        println!("{date}  {agent}\n  {}\n", truncate(&summary, 280));
    }
    Ok(())
}

fn cmd_adr(args: &[String]) -> Result<()> {
    let conn = ensure_synced()?;
    match args.first().map(String::as_str) {
        Some("show") => {
            let Some(number) = args.get(1) else {
                bail!("usage: cargo xtask progress adr show <number>");
            };
            let number = format!("{:0>4}", number.trim_start_matches('0'));
            let row: Option<(String, String, String)> = conn
                .query_row(
                    "SELECT title, status, path FROM adrs WHERE number = ?1",
                    [&number],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .ok();
            let Some((title, status, path)) = row else {
                bail!("no ADR {number}");
            };
            println!("ADR {number}: {title}\n  status: {status}\n  file:   {path}");
            let mut stmt = conn.prepare(
                "SELECT p.feature FROM parity p JOIN parity_adr a ON a.slug = p.slug WHERE a.adr = ?1 ORDER BY p.feature",
            )?;
            let features: Vec<String> = stmt
                .query_map([&number], |r| r.get::<_, String>(0))?
                .filter_map(std::result::Result::ok)
                .collect();
            if !features.is_empty() {
                println!("  features: {}", features.join(", "));
            }
        }
        _ => {
            let mut stmt =
                conn.prepare("SELECT number, status, title FROM adrs ORDER BY number")?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?;
            for row in rows {
                let (number, status, title) = row?;
                println!("{number}  {status:<10} {title}");
            }
        }
    }
    Ok(())
}

fn cmd_add_session(args: &[String]) -> Result<()> {
    let agent = flag_value(args, "--agent")
        .context("missing --agent (e.g. --agent \"Claude (Opus 4.8)\")")?;
    let summary = flag_value(args, "--summary").context("missing --summary \"…\"")?;
    let date = flag_value(args, "--date").unwrap_or_else(today_iso);

    let session = Session {
        date,
        agent,
        summary,
    };
    let line = serde_json::to_string(&serde_json::json!({
        "date": session.date,
        "agent": session.agent,
        "summary": session.summary,
    }))?;

    let path = Path::new(PROGRESS_DIR).join("sessions.ndjson");
    let existing = fs::read_to_string(&path).unwrap_or_default();
    // Newest-first, matching the existing ordering.
    let new_contents = format!("{line}\n{existing}");
    fs::write(&path, new_contents)?;
    println!(
        "progress: prepended session ({}) to {}",
        session.date,
        path.display()
    );
    sync()?;
    Ok(())
}

fn cmd_check() -> Result<()> {
    let mut errors: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    // Build the set of known ADR numbers from filenames for ref validation.
    let adr_numbers: std::collections::HashSet<String> = match load_adrs() {
        Ok(adrs) => adrs.into_iter().map(|a| a.number).collect(),
        Err(e) => {
            errors.push(format!("docs/adr: {e:#}"));
            std::collections::HashSet::new()
        }
    };

    match load_parity() {
        Ok(rows) => {
            if rows.is_empty() {
                errors.push("no parity files found under docs/progress/parity".to_owned());
            }
            for row in rows {
                if !STATUSES.contains(&row.file.status.as_str()) {
                    errors.push(format!(
                        "{}: invalid status `{}` (expected one of {STATUSES:?})",
                        row.slug, row.file.status
                    ));
                }
                if row.file.summary.chars().count() > SUMMARY_WARN_LIMIT {
                    warnings.push(format!(
                        "{}: summary is {} characters (budget: {SUMMARY_WARN_LIMIT})",
                        row.slug,
                        row.file.summary.chars().count()
                    ));
                }
                for adr in &row.file.adr {
                    if adr.len() != 4 || !adr.chars().all(|c| c.is_ascii_digit()) {
                        errors.push(format!(
                            "{}: ADR ref `{adr}` is not a 4-digit number",
                            row.slug
                        ));
                    } else if !adr_numbers.is_empty() && !adr_numbers.contains(adr) {
                        errors.push(format!(
                            "{}: ADR ref `{adr}` does not resolve to docs/adr/",
                            row.slug
                        ));
                    }
                }
                for debt in &row.file.debt {
                    if debt.id.trim().is_empty() {
                        errors.push(format!("{}: debt id must not be empty", row.slug));
                    }
                    if debt.description.trim().is_empty() {
                        errors.push(format!(
                            "{}: debt `{}` description must not be empty",
                            row.slug, debt.id
                        ));
                    }
                    if debt.discovered.trim().is_empty() {
                        errors.push(format!(
                            "{}: debt `{}` discovered must not be empty",
                            row.slug, debt.id
                        ));
                    }
                    if debt.evidence.trim().is_empty() {
                        errors.push(format!(
                            "{}: debt `{}` evidence must not be empty",
                            row.slug, debt.id
                        ));
                    }
                }
                let open_debt_count = row.file.open_debts().count();
                if row.file.status == "Ported" && open_debt_count > 0 {
                    errors.push(format!(
                        "{}: status is Ported but {open_debt_count} open structured debt item(s) remain",
                        row.slug
                    ));
                }
            }
        }
        Err(e) => errors.push(format!("parity: {e:#}")),
    }

    if let Err(e) = load_phase() {
        errors.push(format!("phase.toml: {e:#}"));
    }
    if let Err(e) = load_routes() {
        errors.push(format!("routes.toml: {e:#}"));
    }
    match load_hydration() {
        Ok(pages) => {
            for p in pages {
                if !HYDRATION_STATUSES.contains(&p.status.as_str()) {
                    errors.push(format!(
                        "hydration `{}`: invalid status `{}` (expected one of {HYDRATION_STATUSES:?})",
                        p.name, p.status
                    ));
                }
            }
        }
        Err(e) => errors.push(format!("hydration.toml: {e:#}")),
    }
    if let Err(e) = load_sessions() {
        errors.push(format!("sessions.ndjson: {e:#}"));
    }

    for warning in &warnings {
        eprintln!("  ! {warning}");
    }

    if errors.is_empty() {
        println!("progress: check passed");
        Ok(())
    } else {
        for e in &errors {
            eprintln!("  ✗ {e}");
        }
        bail!("{} problem(s) found", errors.len());
    }
}

// ---------------------------------------------------------------- helpers

fn phase_value(conn: &Connection, key: &str) -> Result<String> {
    Ok(conn
        .query_row("SELECT value FROM phase WHERE key = ?1", [key], |r| {
            r.get(0)
        })
        .unwrap_or_default())
}

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn truncate(s: &str, max: usize) -> String {
    let s = s.replace('\n', " ");
    if s.chars().count() <= max {
        s
    } else {
        let cut: String = s.chars().take(max).collect();
        format!("{cut}…")
    }
}

/// Current UTC date as `YYYY-MM-DD`, dependency-free and cross-platform.
fn today_iso() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    format!("{y:04}-{m:02}-{d:02}")
}

/// Howard Hinnant's days-from-civil inverse (proleptic Gregorian).
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 }.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn print_help() {
    eprintln!("progress — query/maintain the porting progress index (docs/progress/)");
    eprintln!();
    eprintln!("USAGE:");
    eprintln!("    cargo xtask progress <SUBCOMMAND>");
    eprintln!();
    eprintln!("SUBCOMMANDS:");
    eprintln!("    next                         Current phase, focus, in-progress features, latest session");
    eprintln!(
        "    gaps                         Features not yet Ported (the \"what's left\" view)"
    );
    eprintln!("    parity [--status S]          List features (compact)");
    eprintln!("    parity --feature SLUG [--full]   Show one feature (--full adds the summary)");
    eprintln!("    routes [--unrouted]          Admin UI route coverage");
    eprintln!("    hydration [--pending]        Leptos hydration parity per page");
    eprintln!("    sessions [--limit N]         Recent session log entries (default 10)");
    eprintln!("    adr [list|show <number>]     Derived ADR index");
    eprintln!("    add-session --agent A --summary S [--date D]   Append a session entry");
    eprintln!("    sync                         Rebuild progress.db from the text sources");
    eprintln!("    check                        Validate the text sources (CI gate)");
}
