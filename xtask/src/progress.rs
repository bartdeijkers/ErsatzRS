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
const TEST_CONTRACT_PATH: &str = "TEST.md";
const ROUTES_QUERY: &str = "cargo xtask progress routes";

const STATUSES: [&str; 5] = [
    "Not started",
    "In progress",
    "Ported",
    "Deferred",
    "Dropped",
];
const HYDRATION_STATUSES: [&str; 3] = ["✅", "TODO", "⏳"];
const MILESTONE_STATUSES: [&str; 5] = [
    "Not started",
    "In progress",
    "Complete",
    "Blocked",
    "Deferred",
];
const SUMMARY_WARN_LIMIT: usize = 4_000;

pub fn run(args: Vec<String>) -> ExitCode {
    let mut it = args.into_iter();
    let sub = it.next().unwrap_or_default();
    let rest: Vec<String> = it.collect();
    let result = match sub.as_str() {
        "sync" => cmd_sync(),
        "next" => cmd_next(),
        "milestones" => cmd_milestones(&rest),
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
    /// Optional 4-digit ADR number this debt is the open part of. Lets
    /// `check` flag an ADR that claims `Implemented` while this debt is open.
    #[serde(default)]
    adr: Option<String>,
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
    #[serde(default)]
    routes: Vec<String>,
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
struct MilestonesFile {
    #[serde(default, rename = "milestone")]
    milestones: Vec<Milestone>,
}

#[derive(Deserialize)]
struct Milestone {
    id: String,
    phase: String,
    title: String,
    status: String,
    feature: String,
    plan: String,
    #[serde(default)]
    depends_on: Vec<String>,
    acceptance: String,
    #[serde(default)]
    evidence: String,
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
    /// Decision lifecycle (Proposed/Accepted/Superseded/Deprecated).
    status: String,
    /// Implementation lifecycle, independent of `status`. See `adr_impl`.
    implementation: String,
    path: String,
}

/// Recognized leading tokens for an ADR's `## Implementation` section. The
/// first matching token classifies the ADR; `Implemented` is the only one the
/// contradiction lint treats as "the code exists".
const IMPL_TOKENS: [&str; 4] = ["Not implemented", "Partial", "Implemented", "N/A"];

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

fn load_milestones() -> Result<Vec<Milestone>> {
    let path = Path::new(PROGRESS_DIR).join("milestones.toml");
    let text = fs::read_to_string(&path)?;
    let parsed: MilestonesFile =
        toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    Ok(parsed.milestones)
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
        let status = adr_section(&text, "## Status").unwrap_or_else(|| "Unknown".to_owned());
        let implementation =
            adr_section(&text, "## Implementation").unwrap_or_else(|| "Unspecified".to_owned());
        out.push(Adr {
            number,
            title,
            status,
            implementation,
            path: path.display().to_string(),
        });
    }
    Ok(out)
}

/// First non-empty, non-comment line of the named `## Heading` section.
fn adr_section(text: &str, heading: &str) -> Option<String> {
    let mut lines = text.lines();
    while let Some(l) = lines.next() {
        if l.trim() == heading {
            for next in lines.by_ref() {
                let t = next.trim();
                if t.is_empty() || t.starts_with("<!--") || t.starts_with("- `") {
                    continue;
                }
                if t.starts_with("##") {
                    break;
                }
                return Some(t.to_owned());
            }
        }
    }
    None
}

/// Short classification token for an implementation line (e.g. `Partial — …`
/// → `Partial`). Returns `Unspecified` when the line matches no known token.
fn impl_tag(implementation: &str) -> &'static str {
    IMPL_TOKENS
        .into_iter()
        .find(|tok| implementation.starts_with(tok))
        .unwrap_or("Unspecified")
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
    let milestones = load_milestones()?;
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
         CREATE TABLE parity_debt(slug TEXT, id TEXT, description TEXT, discovered TEXT, evidence TEXT, resolved INTEGER, adr TEXT);
         CREATE TABLE routes(nav TEXT, backing TEXT, routed INTEGER);
         CREATE TABLE hydration(name TEXT, status TEXT, notes TEXT);
         CREATE TABLE hydration_route(name TEXT, route TEXT);
         CREATE TABLE sessions(ord INTEGER, date TEXT, agent TEXT, summary TEXT);
         CREATE TABLE adrs(number TEXT, title TEXT, status TEXT, implementation TEXT, path TEXT);
         CREATE TABLE phase(key TEXT, value TEXT);
         CREATE TABLE phase_checklist(item TEXT, done INTEGER);
         CREATE TABLE phase_todo(note TEXT);
         CREATE TABLE phase_decision(note TEXT);
         CREATE TABLE milestones(id TEXT PRIMARY KEY, phase TEXT, title TEXT, status TEXT, feature TEXT, plan TEXT, acceptance TEXT, evidence TEXT);
         CREATE TABLE milestone_dependency(id TEXT, depends_on TEXT);
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
                "INSERT INTO parity_debt VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![
                    row.slug,
                    debt.id,
                    debt.description,
                    debt.discovered,
                    debt.evidence,
                    i32::from(debt.resolved),
                    debt.adr
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
        for route in &p.routes {
            tx.execute(
                "INSERT INTO hydration_route VALUES(?1,?2)",
                params![p.name, route],
            )?;
        }
    }
    for (i, s) in sessions.iter().enumerate() {
        tx.execute(
            "INSERT INTO sessions VALUES(?1,?2,?3,?4)",
            params![i as i64, s.date, s.agent, s.summary],
        )?;
    }
    for a in &adrs {
        tx.execute(
            "INSERT INTO adrs VALUES(?1,?2,?3,?4,?5)",
            params![a.number, a.title, a.status, a.implementation, a.path],
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
    for milestone in &milestones {
        tx.execute(
            "INSERT INTO milestones VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                milestone.id,
                milestone.phase,
                milestone.title,
                milestone.status,
                milestone.feature,
                milestone.plan,
                milestone.acceptance,
                milestone.evidence,
            ],
        )?;
        for dependency in &milestone.depends_on {
            tx.execute(
                "INSERT INTO milestone_dependency VALUES(?1,?2)",
                params![milestone.id, dependency],
            )?;
        }
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
    let m: i64 = conn.query_row("SELECT COUNT(*) FROM milestones", [], |r| r.get(0))?;
    println!("progress: synced {DB_PATH} — {n} features, {m} milestones, {s} sessions, {a} ADRs");
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

    let mut stmt = conn.prepare(
        "SELECT m.id, m.phase, m.title, m.status
         FROM milestones m
         WHERE m.status IN ('In progress', 'Not started')
           AND NOT EXISTS (
               SELECT 1
               FROM milestone_dependency d
               JOIN milestones required ON required.id = d.depends_on
               WHERE d.id = m.id AND required.status != 'Complete'
           )
         ORDER BY CASE m.status WHEN 'In progress' THEN 0 ELSE 1 END, m.id
         LIMIT 3",
    )?;
    let milestones: Vec<(String, String, String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .filter_map(std::result::Result::ok)
        .collect();
    if !milestones.is_empty() {
        println!("\nRunnable milestones:");
        for (id, phase, title, status) in milestones {
            println!("  - {id} [{status}] {title} ({phase})");
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

fn cmd_milestones(args: &[String]) -> Result<()> {
    let conn = ensure_synced()?;
    let id = flag_value(args, "--id");
    let full = args.iter().any(|arg| arg == "--full");

    if let Some(id) = id {
        let row: Option<(String, String, String, String, String, String, String)> = conn
            .query_row(
                "SELECT phase, title, status, feature, plan, acceptance, evidence
                 FROM milestones WHERE id = ?1",
                [&id],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                    ))
                },
            )
            .ok();
        let Some((phase, title, status, feature, plan, acceptance, evidence)) = row else {
            bail!("no milestone with id `{id}` (try `cargo xtask progress milestones --all`)");
        };
        let mut stmt = conn.prepare(
            "SELECT depends_on FROM milestone_dependency WHERE id = ?1 ORDER BY depends_on",
        )?;
        let dependencies: Vec<String> = stmt
            .query_map([&id], |r| r.get::<_, String>(0))?
            .filter_map(std::result::Result::ok)
            .collect();
        println!("{id}  [{status}] {title}");
        println!("  phase:   {phase}");
        println!("  feature: {feature}");
        println!("  plan:    {plan}");
        println!(
            "  depends: {}",
            if dependencies.is_empty() {
                "-".to_owned()
            } else {
                dependencies.join(", ")
            }
        );
        if full {
            println!("\nAcceptance:\n{}", acceptance.trim());
            if !evidence.trim().is_empty() {
                println!("\nEvidence:\n{}", evidence.trim());
            }
        }
        return Ok(());
    }

    let include_all = args.iter().any(|arg| arg == "--all");
    let status = flag_value(args, "--status");
    let feature = flag_value(args, "--feature");
    let phase = flag_value(args, "--phase");
    let only_runnable = args.iter().any(|arg| arg == "--runnable");
    let mut predicates = Vec::new();
    let mut bound = Vec::new();
    if !include_all && status.is_none() {
        predicates.push("status != 'Complete'".to_owned());
    }
    if only_runnable {
        predicates.push("status IN ('In progress', 'Not started')".to_owned());
        predicates.push(
            "NOT EXISTS (
                SELECT 1
                FROM milestone_dependency d
                JOIN milestones required ON required.id = d.depends_on
                WHERE d.id = milestones.id AND required.status != 'Complete'
            )"
            .to_owned(),
        );
    }
    for (column, value) in [("status", status), ("feature", feature), ("phase", phase)] {
        if let Some(value) = value {
            bound.push(value);
            predicates.push(format!("{column} = ?{}", bound.len()));
        }
    }
    let mut sql = "SELECT id, phase, title, status, feature FROM milestones".to_owned();
    if !predicates.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&predicates.join(" AND "));
    }
    sql.push_str(" ORDER BY phase, id");

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(bound.iter()), |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
        ))
    })?;
    for row in rows {
        let (id, phase, title, status, feature) = row?;
        println!("{id:<8} [{status:<11}] {title}  ({phase}; {feature})");
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
            "SELECT id, description, discovered, evidence, resolved, adr FROM parity_debt WHERE slug = ?1 ORDER BY resolved, discovered, id",
        )?;
        let debts: Vec<(String, String, String, String, i64, Option<String>)> = stmt
            .query_map([&slug], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
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
            for (id, description, discovered, evidence, resolved, adr) in debts {
                let state = if resolved == 0 { "open" } else { "resolved" };
                let adr = adr.map_or(String::new(), |a| format!(", ADR {a}"));
                println!(
                    "    - {id} [{state}, {discovered}{adr}]: {}",
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
    if !only_unrouted {
        println!();
        println!("Hydration and browser verification (docs/progress/hydration.toml):");
        let mut stmt = conn.prepare(
            "SELECT hr.route, h.status, h.name, h.notes
             FROM hydration_route hr
             JOIN hydration h ON h.name = hr.name
             ORDER BY hr.route, h.name",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?;
        for row in rows {
            let (route, status, name, notes) = row?;
            println!("{status:<5} {route:<46} {name}: {}", truncate(&notes, 72));
        }
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
            let row: Option<(String, String, String, String)> = conn
                .query_row(
                    "SELECT title, status, implementation, path FROM adrs WHERE number = ?1",
                    [&number],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )
                .ok();
            let Some((title, status, implementation, path)) = row else {
                bail!("no ADR {number}");
            };
            println!(
                "ADR {number}: {title}\n  status:   {status}\n  built:    {implementation}\n  file:     {path}"
            );
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
            let mut stmt = conn.prepare(
                "SELECT number, status, implementation, title FROM adrs ORDER BY number",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })?;
            // Two independent axes: decision Status and whether code exists.
            println!("{:<5} {:<11} {:<16} Title", "ADR", "Status", "Built");
            for row in rows {
                let (number, status, implementation, title) = row?;
                let status = status.split_once(" (").map_or(status.as_str(), |(s, _)| s);
                println!(
                    "{number:<5} {status:<11} {:<16} {title}",
                    impl_tag(&implementation)
                );
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
    let mut parity_slugs = std::collections::HashSet::new();

    // Known ADRs, keyed by number, for ref validation and the
    // Status-vs-Implementation contradiction lint below.
    let adrs_by_number: std::collections::HashMap<String, Adr> = match load_adrs() {
        Ok(adrs) => {
            for adr in &adrs {
                // Every ADR must declare a recognized Implementation token so
                // "Accepted {date}" can never be mistaken for "code exists".
                if impl_tag(&adr.implementation) == "Unspecified" {
                    errors.push(format!(
                        "ADR {}: `## Implementation` is missing or unrecognized (got {:?}; expected one of {IMPL_TOKENS:?})",
                        adr.number, adr.implementation
                    ));
                }
            }
            adrs.into_iter().map(|a| (a.number.clone(), a)).collect()
        }
        Err(e) => {
            errors.push(format!("docs/adr: {e:#}"));
            std::collections::HashMap::new()
        }
    };
    let adr_numbers: std::collections::HashSet<String> = adrs_by_number.keys().cloned().collect();

    match load_parity() {
        Ok(rows) => {
            if rows.is_empty() {
                errors.push("no parity files found under docs/progress/parity".to_owned());
            }
            for row in rows {
                parity_slugs.insert(row.slug.clone());
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
                    if let Some(adr) = &debt.adr {
                        if adr.len() != 4 || !adr.chars().all(|c| c.is_ascii_digit()) {
                            errors.push(format!(
                                "{}: debt `{}` adr `{adr}` is not a 4-digit number",
                                row.slug, debt.id
                            ));
                        } else if let Some(linked) = adrs_by_number.get(adr) {
                            // The core guard: an ADR may not claim its code is
                            // built while a debt it is linked to is still open.
                            if !debt.resolved && impl_tag(&linked.implementation) == "Implemented" {
                                errors.push(format!(
                                    "ADR {adr} declares Implementation `Implemented` but linked debt `{}` ({}) is still open — downgrade the ADR to `Partial` or resolve the debt",
                                    debt.id, row.slug
                                ));
                            }
                        } else if !adr_numbers.is_empty() {
                            errors.push(format!(
                                "{}: debt `{}` adr `{adr}` does not resolve to docs/adr/",
                                row.slug, debt.id
                            ));
                        }
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

    match load_milestones() {
        Ok(milestones) => {
            let milestone_ids: std::collections::HashSet<&str> = milestones
                .iter()
                .map(|milestone| milestone.id.as_str())
                .collect();
            let milestone_statuses: std::collections::HashMap<&str, &str> = milestones
                .iter()
                .map(|milestone| (milestone.id.as_str(), milestone.status.as_str()))
                .collect();
            let has_duplicate_ids = !milestone_ids_are_unique(&milestones);
            if has_duplicate_ids {
                errors.push("milestones.toml: milestone ids must be unique".to_owned());
            }
            for milestone in &milestones {
                if milestone.id.is_empty()
                    || !milestone
                        .id
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
                {
                    errors.push(format!(
                        "milestone `{}`: id must contain only lowercase ASCII letters, digits, and hyphens",
                        milestone.id
                    ));
                }
                if milestone.phase.trim().is_empty() || milestone.title.trim().is_empty() {
                    errors.push(format!(
                        "milestone `{}`: phase and title must not be empty",
                        milestone.id
                    ));
                }
                if !MILESTONE_STATUSES.contains(&milestone.status.as_str()) {
                    errors.push(format!(
                        "milestone `{}`: invalid status `{}` (expected one of {MILESTONE_STATUSES:?})",
                        milestone.id, milestone.status
                    ));
                }
                if !parity_slugs.contains(&milestone.feature) {
                    errors.push(format!(
                        "milestone `{}`: feature `{}` does not resolve to docs/progress/parity/",
                        milestone.id, milestone.feature
                    ));
                }
                let plan = Path::new(&milestone.plan);
                if !milestone.plan.starts_with("plans/") || !plan.is_file() {
                    errors.push(format!(
                        "milestone `{}`: plan `{}` must be an existing file under plans/",
                        milestone.id, milestone.plan
                    ));
                }
                if milestone.acceptance.trim().is_empty() {
                    errors.push(format!(
                        "milestone `{}`: acceptance must not be empty",
                        milestone.id
                    ));
                }
                if milestone.status == "Complete" && milestone.evidence.trim().is_empty() {
                    errors.push(format!(
                        "milestone `{}`: Complete milestones require evidence",
                        milestone.id
                    ));
                }
                let unique_dependencies: std::collections::HashSet<&str> =
                    milestone.depends_on.iter().map(String::as_str).collect();
                if unique_dependencies.len() != milestone.depends_on.len() {
                    errors.push(format!(
                        "milestone `{}`: dependencies must be unique",
                        milestone.id
                    ));
                }
                for dependency in &milestone.depends_on {
                    if dependency == &milestone.id {
                        errors.push(format!(
                            "milestone `{}`: milestone cannot depend on itself",
                            milestone.id
                        ));
                    } else if !milestone_ids.contains(dependency.as_str()) {
                        errors.push(format!(
                            "milestone `{}`: dependency `{dependency}` does not resolve",
                            milestone.id
                        ));
                    } else if milestone.status == "Complete"
                        && milestone_statuses.get(dependency.as_str()).copied() != Some("Complete")
                    {
                        errors.push(format!(
                            "milestone `{}`: Complete milestone depends on incomplete `{dependency}`",
                            milestone.id
                        ));
                    }
                }
            }
            // Duplicate ids make the dependency graph ambiguous. Report the
            // schema error above instead of traversing a graph whose node map
            // necessarily collapsed one or more milestone rows.
            if !has_duplicate_ids {
                if let Some(cycle) = milestone_dependency_cycle(&milestones) {
                    errors.push(format!(
                        "milestones.toml: dependency cycle contains {}",
                        cycle.join(", ")
                    ));
                }
            }
        }
        Err(e) => errors.push(format!("milestones.toml: {e:#}")),
    }

    if let Err(e) = load_phase() {
        errors.push(format!("phase.toml: {e:#}"));
    }
    if let Err(e) = load_routes() {
        errors.push(format!("routes.toml: {e:#}"));
    }
    match load_hydration() {
        Ok(pages) => {
            let mut page_names = std::collections::HashSet::new();
            let mut route_owners: std::collections::HashMap<&str, &str> =
                std::collections::HashMap::new();
            for p in &pages {
                if !page_names.insert(p.name.as_str()) {
                    errors.push(format!("hydration.toml: duplicate page name `{}`", p.name));
                }
                if !HYDRATION_STATUSES.contains(&p.status.as_str()) {
                    errors.push(format!(
                        "hydration `{}`: invalid status `{}` (expected one of {HYDRATION_STATUSES:?})",
                        p.name, p.status
                    ));
                }
                if p.routes.is_empty() && p.name != "WholeAppIslandHydrationSmokeGate" {
                    errors.push(format!(
                        "hydration `{}`: route-backed entries require at least one `routes` value",
                        p.name
                    ));
                }
                for route in &p.routes {
                    if !route.starts_with('/') || route.chars().any(char::is_whitespace) {
                        errors.push(format!(
                            "hydration `{}`: invalid route `{route}` (expected a whitespace-free absolute path)",
                            p.name
                        ));
                    }
                    if let Some(owner) = route_owners.insert(route, &p.name) {
                        errors.push(format!(
                            "hydration.toml: route `{route}` is duplicated by `{owner}` and `{}`",
                            p.name
                        ));
                    }
                }
            }
            match fs::read_to_string(TEST_CONTRACT_PATH) {
                Ok(test_contract) => {
                    errors.extend(test_route_contract_errors(&test_contract, &pages))
                }
                Err(error) => errors.push(format!("{TEST_CONTRACT_PATH}: {error}")),
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

fn milestone_dependency_cycle(milestones: &[Milestone]) -> Option<Vec<String>> {
    if !milestone_ids_are_unique(milestones) {
        return None;
    }
    let ids: std::collections::HashSet<&str> = milestones
        .iter()
        .map(|milestone| milestone.id.as_str())
        .collect();
    let mut remaining_dependencies: std::collections::HashMap<&str, usize> = milestones
        .iter()
        .map(|milestone| {
            let count = milestone
                .depends_on
                .iter()
                .filter(|dependency| ids.contains(dependency.as_str()))
                .count();
            (milestone.id.as_str(), count)
        })
        .collect();
    let mut dependents: std::collections::HashMap<&str, Vec<&str>> =
        std::collections::HashMap::new();
    for milestone in milestones {
        for dependency in &milestone.depends_on {
            if ids.contains(dependency.as_str()) {
                dependents
                    .entry(dependency.as_str())
                    .or_default()
                    .push(milestone.id.as_str());
            }
        }
    }
    let mut ready: std::collections::VecDeque<&str> = remaining_dependencies
        .iter()
        .filter_map(|(id, count)| (*count == 0).then_some(*id))
        .collect();
    let mut visited = 0;
    while let Some(id) = ready.pop_front() {
        visited += 1;
        for dependent in dependents.get(id).into_iter().flatten() {
            let Some(count) = remaining_dependencies.get_mut(dependent) else {
                continue;
            };
            *count -= 1;
            if *count == 0 {
                ready.push_back(dependent);
            }
        }
    }
    if visited == milestones.len() {
        None
    } else {
        let mut cycle: Vec<String> = remaining_dependencies
            .into_iter()
            .filter(|(_, count)| *count > 0)
            .map(|(id, _)| id.to_owned())
            .collect();
        cycle.sort();
        Some(cycle)
    }
}

fn milestone_ids_are_unique(milestones: &[Milestone]) -> bool {
    let ids: std::collections::HashSet<&str> = milestones
        .iter()
        .map(|milestone| milestone.id.as_str())
        .collect();
    ids.len() == milestones.len()
}

fn test_route_contract_errors(test_contract: &str, hydration: &[HydrationPage]) -> Vec<String> {
    let test_routes = markdown_route_rows(test_contract);
    let hydration_routes: std::collections::BTreeSet<&str> = hydration
        .iter()
        .flat_map(|page| page.routes.iter().map(String::as_str))
        .collect();
    let mut errors = Vec::new();

    if test_routes.is_empty() {
        if !test_contract.contains(ROUTES_QUERY) {
            errors.push(format!(
                "{TEST_CONTRACT_PATH}: route verification must link to `{ROUTES_QUERY}`"
            ));
        }
        return errors;
    }

    for route in test_routes.difference(&hydration_routes) {
        errors.push(format!(
            "{TEST_CONTRACT_PATH}: route row `{route}` is absent from docs/progress/hydration.toml"
        ));
    }
    for route in hydration_routes.difference(&test_routes) {
        errors.push(format!(
            "docs/progress/hydration.toml: route `{route}` is absent from the {TEST_CONTRACT_PATH} route table"
        ));
    }
    errors.push(format!(
        "{TEST_CONTRACT_PATH}: remove the duplicated per-route table and link to `{ROUTES_QUERY}`"
    ));
    errors
}

fn markdown_route_rows(markdown: &str) -> std::collections::BTreeSet<&str> {
    markdown
        .lines()
        .filter_map(|line| {
            let first_cell = line.strip_prefix('|')?.split('|').next()?.trim();
            let route = first_cell.strip_prefix('`')?.strip_suffix('`')?;
            route.starts_with('/').then_some(route)
        })
        .collect()
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
    eprintln!("    milestones [--runnable] [--all] [--status S] [--feature SLUG] [--phase P]");
    eprintln!("    milestones --id ID [--full] Show one milestone, dependencies, and acceptance");
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

#[cfg(test)]
mod tests {
    use super::{
        milestone_dependency_cycle, milestone_ids_are_unique, test_route_contract_errors,
        HydrationPage, Milestone,
    };

    fn milestone(id: &str, depends_on: &[&str]) -> Milestone {
        Milestone {
            id: id.to_owned(),
            phase: "Phase 7A".to_owned(),
            title: id.to_owned(),
            status: "Not started".to_owned(),
            feature: "feature".to_owned(),
            plan: "plans/plan.md".to_owned(),
            depends_on: depends_on.iter().map(ToString::to_string).collect(),
            acceptance: "accepted".to_owned(),
            evidence: String::new(),
        }
    }

    fn hydration(route: &str) -> HydrationPage {
        HydrationPage {
            name: format!("{route} page"),
            status: "✅".to_owned(),
            notes: "covered".to_owned(),
            routes: vec![route.to_owned()],
        }
    }

    #[test]
    fn test_route_contract_accepts_canonical_query_without_duplicate_table() {
        let errors = test_route_contract_errors(
            "Route evidence: `cargo xtask progress routes`.",
            &[hydration("/channels")],
        );

        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_route_contract_reports_mismatches_in_both_directions() {
        let errors = test_route_contract_errors(
            "| Route | Status |\n| --- | --- |\n| `/channels` | ok |\n| `/stale` | todo |",
            &[hydration("/channels"), hydration("/missing-from-test")],
        );

        assert!(errors.iter().any(|error| error.contains("`/stale`")
            && error.contains("absent from docs/progress/hydration.toml")));
        assert!(errors
            .iter()
            .any(|error| error.contains("`/missing-from-test`")
                && error.contains("absent from the TEST.md route table")));
        assert!(errors
            .iter()
            .any(|error| error.contains("remove the duplicated per-route table")));
    }

    #[test]
    fn milestone_dependencies_accept_an_acyclic_graph() {
        let milestones = [
            milestone("first", &[]),
            milestone("second", &["first"]),
            milestone("third", &["first", "second"]),
        ];

        assert!(milestone_dependency_cycle(&milestones).is_none());
    }

    #[test]
    fn milestone_dependencies_report_cycles() {
        let milestones = [
            milestone("first", &["second"]),
            milestone("second", &["first"]),
        ];

        assert_eq!(
            milestone_dependency_cycle(&milestones),
            Some(vec!["first".to_owned(), "second".to_owned()])
        );
    }

    #[test]
    fn milestone_dependency_traversal_skips_duplicate_ids() {
        let milestones = [
            milestone("root", &[]),
            milestone("duplicate", &["root"]),
            milestone("duplicate", &["root"]),
        ];

        assert!(!milestone_ids_are_unique(&milestones));
        assert!(milestone_dependency_cycle(&milestones).is_none());
    }
}
