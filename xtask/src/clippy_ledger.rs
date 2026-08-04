//! Deterministic ledger of `#[allow(clippy::...)]` exemptions across the
//! workspace, and a fixed-priority policy for the nightly clippy-cleanup
//! routine (see `docs/automation/clippy-cleanup-routine.md`).
//!
//! Regenerates `plans/clippy-exemption-ledger.html` from the checked-in
//! template in `xtask/assets/clippy_ledger_template.html` (the derived HTML
//! is gitignored, so this must work from a fresh clone every time). `--next`
//! picks exactly one safe, mechanical target so the nightly agent never has
//! to re-derive the fix-tier policy from prose.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

use anyhow::{Context, Result};
use regex::Regex;
use serde::Serialize;
use walkdir::WalkDir;

const TEMPLATE_PATH: &str = "xtask/assets/clippy_ledger_template.html";
const OUTPUT_PATH: &str = "plans/clippy-exemption-ledger.html";
const PLACEHOLDER: &str = "__CLIPPY_LEDGER_DATA__";
const GREP_COMMAND: &str = "cargo xtask clippy-ledger";

// Files whose *module-level* allow lint set is exactly this are the
// copy-pasted blanket-header pattern the ledger's headline finding calls
// out. Exact set equality (not a superset check): once a file is narrowed
// down to a targeted allow, it stops counting as "the pattern", which is how
// this list self-shrinks as the nightly routine fixes files.
const BLANKET_PATTERN_LINTS: &[&str] = &["clippy::too_many_lines", "dead_code"];

// Tier 2: single-instance, non-structural lints that don't require
// restructuring code, in priority order. Deliberately excludes every
// structural / judgment-call lint (too_many_lines, too_many_arguments,
// struct_excessive_bools, disallowed_methods, cast_*, needless_pass_by_value,
// unused_async, standalone dead_code) — those stay accepted debt, since
// "fixing" them mechanically would mean restructuring code the module-split
// survey already decided not to restructure on size alone.
const TIER2_PRIORITY: &[&str] = &[
    "clippy::needless_raw_string_hashes",
    "clippy::must_use_candidate",
    "clippy::struct_field_names",
    "clippy::similar_names",
    "clippy::match_same_arms",
    "clippy::missing_errors_doc",
    "clippy::missing_panics_doc",
];

// Correctness lints that must never be auto-selected by `--next`: a getter
// that doesn't return the field its name promises may be a real bug, not a
// style nit, so it's surfaced separately for manual review instead.
const FLAGGED_LINTS: &[&str] = &["clippy::misnamed_getters"];

// `--tune-batch-cap` bounds: the nightly routine self-tunes how many fixes it
// attempts per run by reading its own past `Clippy-cleanup-run:` commit
// trailers (see docs/automation/clippy-cleanup-routine.md), never a separate
// tracking file. Only the batch cap is tunable — the time-box below is the
// fixed backstop that actually protects the usage window, and stays a
// constant the tuner never touches.
const DEFAULT_BATCH_CAP: u32 = 4;
const MIN_BATCH_CAP: u32 = 1;
const MAX_BATCH_CAP: u32 = 6;
const BATCH_CAP_STEP: u32 = 1;
const TUNE_LOOKBACK_RUNS: usize = 3;
// Must match the time-box the nightly routine's prompt actually enforces.
const TIME_BOX_SECONDS: u64 = 2400;
const RUN_TRAILER_PREFIX: &str = "Clippy-cleanup-run:";
// Record separator between `git log` entries; not a byte that appears in
// commit message text.
const GIT_LOG_RECORD_SEP: char = '\u{1e}';

#[derive(Serialize)]
struct Meta {
    generated: String,
    grep_command: String,
    total_workspace_rs_files: usize,
    total_workspace_loc: usize,
    total_attribute_sites: usize,
    total_lint_instances: usize,
    total_files_touched: usize,
    module_level_sites: usize,
    item_level_sites: usize,
    module_level_files: usize,
    module_level_loc: usize,
    touched_loc: usize,
    pattern_files: Vec<String>,
    pattern_loc: usize,
}

#[derive(Serialize, Clone)]
struct Site {
    line: usize,
    lints: Vec<String>,
    module_level: bool,
}

#[derive(Serialize)]
struct FileEntry {
    file: String,
    #[serde(rename = "crate")]
    crate_name: String,
    loc: usize,
    count: usize,
    module_level: usize,
    lints: BTreeMap<String, usize>,
    sites: Vec<Site>,
}

#[derive(Serialize)]
struct FlaggedSite {
    file: String,
    line: usize,
    lint: String,
    reason: String,
}

#[derive(Serialize)]
struct Ledger {
    meta: Meta,
    by_lint: Vec<(String, usize)>,
    by_crate: Vec<(String, usize)>,
    files: Vec<FileEntry>,
    flagged: Vec<FlaggedSite>,
}

enum NextTarget {
    Tier1 {
        file: String,
        allow: String,
        loc: usize,
    },
    Tier2 {
        lint: String,
        file: String,
        line: usize,
    },
    None,
}

#[derive(Default)]
enum Mode {
    #[default]
    Regenerate,
    Next,
    Json,
    TuneBatchCap,
}

#[derive(Default)]
struct Opts {
    mode: Mode,
    skip: BTreeSet<String>,
}

enum ParseResult {
    Help,
    Error(String),
}

impl Opts {
    fn parse(args: Vec<String>) -> std::result::Result<Self, ParseResult> {
        let mut opts = Self::default();
        let mut iter = args.into_iter();
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "-h" | "--help" => return Err(ParseResult::Help),
                "--next" => opts.mode = Mode::Next,
                "--json" => opts.mode = Mode::Json,
                "--tune-batch-cap" => opts.mode = Mode::TuneBatchCap,
                "--skip" => {
                    let value = iter.next().ok_or_else(|| {
                        ParseResult::Error("--skip requires a comma-separated file list".to_owned())
                    })?;
                    opts.skip.extend(
                        value
                            .split(',')
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .map(str::to_owned),
                    );
                }
                other => return Err(ParseResult::Error(format!("unknown argument `{other}`"))),
            }
        }
        Ok(opts)
    }
}

pub fn run(args: Vec<String>) -> ExitCode {
    let opts = match Opts::parse(args) {
        Ok(opts) => opts,
        Err(ParseResult::Help) => {
            print_help();
            return ExitCode::SUCCESS;
        }
        Err(ParseResult::Error(message)) => {
            eprintln!("clippy-ledger: {message}");
            print_help();
            return ExitCode::FAILURE;
        }
    };

    match run_inner(&opts) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("clippy-ledger: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run_inner(opts: &Opts) -> Result<()> {
    if matches!(opts.mode, Mode::TuneBatchCap) {
        return print_tuned_batch_cap();
    }

    let workspace = Path::new(".");
    let today = chrono::Utc::now().date_naive().to_string();
    let ledger = build_ledger(workspace, &today)?;

    match opts.mode {
        Mode::Next => print_next(&next_target(&ledger, &opts.skip)),
        Mode::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&ledger).context("serialize ledger")?
            );
        }
        Mode::Regenerate => {
            regenerate_html(workspace, &ledger)?;
            println!(
                "clippy-ledger: regenerated {} — {} attribute site(s) ({} lint instance(s)) across {} of {} tracked .rs files; {} file(s) still carry the copy-pasted blanket allow ({} LOC); {} site(s) flagged for manual review",
                OUTPUT_PATH,
                ledger.meta.total_attribute_sites,
                ledger.meta.total_lint_instances,
                ledger.meta.total_files_touched,
                ledger.meta.total_workspace_rs_files,
                ledger.meta.pattern_files.len(),
                ledger.meta.pattern_loc,
                ledger.flagged.len(),
            );
        }
        Mode::TuneBatchCap => unreachable!("handled above before the ledger scan"),
    }
    Ok(())
}

fn build_ledger(workspace: &Path, today: &str) -> Result<Ledger> {
    let re =
        Regex::new(r"#(!)?\[\s*allow\((?s:(.*?))\)\]").context("compile allow-attribute regex")?;
    let paths = collect_rs_files(workspace)?;

    let mut total_workspace_loc = 0usize;
    let mut file_entries = Vec::new();
    for path in &paths {
        let content =
            fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        total_workspace_loc += content.lines().count();
        let rel = path
            .strip_prefix(workspace)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        if let Some(entry) = scan_file(&rel, &content, &re) {
            file_entries.push(entry);
        }
    }

    let mut flagged = Vec::new();
    for file in &file_entries {
        for site in &file.sites {
            for lint in &site.lints {
                if FLAGGED_LINTS.contains(&lint.as_str()) {
                    flagged.push(FlaggedSite {
                        file: file.file.clone(),
                        line: site.line,
                        lint: lint.clone(),
                        reason: "correctness lint, not style — may indicate a real bug; \
                                 requires manual review, never auto-fixed"
                            .to_owned(),
                    });
                }
            }
        }
    }

    let mut by_lint_map: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_crate_map: BTreeMap<String, usize> = BTreeMap::new();
    for file in &file_entries {
        *by_crate_map.entry(file.crate_name.clone()).or_insert(0) += file.count;
        for (lint, n) in &file.lints {
            *by_lint_map.entry(lint.clone()).or_insert(0) += n;
        }
    }
    let by_lint = sort_desc(by_lint_map);
    let by_crate = sort_desc(by_crate_map);

    let total_attribute_sites: usize = file_entries.iter().map(|f| f.count).sum();
    let total_lint_instances: usize = file_entries
        .iter()
        .map(|f| f.lints.values().sum::<usize>())
        .sum();
    let total_files_touched = file_entries.len();
    let module_level_sites: usize = file_entries.iter().map(|f| f.module_level).sum();
    let item_level_sites = total_attribute_sites - module_level_sites;
    let module_level_files: Vec<&FileEntry> =
        file_entries.iter().filter(|f| f.module_level > 0).collect();
    let module_level_loc: usize = module_level_files.iter().map(|f| f.loc).sum();
    let module_level_files_count = module_level_files.len();
    let touched_loc: usize = file_entries.iter().map(|f| f.loc).sum();

    let pattern_target: BTreeSet<&str> = BLANKET_PATTERN_LINTS.iter().copied().collect();
    let mut pattern_files: Vec<String> = file_entries
        .iter()
        .filter(|f| {
            let module_lints: BTreeSet<&str> = f
                .sites
                .iter()
                .filter(|s| s.module_level)
                .flat_map(|s| s.lints.iter().map(String::as_str))
                .collect();
            module_lints == pattern_target
        })
        .map(|f| f.file.clone())
        .collect();
    pattern_files.sort();
    let pattern_loc: usize = file_entries
        .iter()
        .filter(|f| pattern_files.contains(&f.file))
        .map(|f| f.loc)
        .sum();

    let mut files = file_entries;
    files.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.file.cmp(&b.file)));

    Ok(Ledger {
        meta: Meta {
            generated: today.to_owned(),
            grep_command: GREP_COMMAND.to_owned(),
            total_workspace_rs_files: paths.len(),
            total_workspace_loc,
            total_attribute_sites,
            total_lint_instances,
            total_files_touched,
            module_level_sites,
            item_level_sites,
            module_level_files: module_level_files_count,
            module_level_loc,
            touched_loc,
            pattern_files,
            pattern_loc,
        },
        by_lint,
        by_crate,
        files,
        flagged,
    })
}

fn sort_desc(map: BTreeMap<String, usize>) -> Vec<(String, usize)> {
    let mut entries: Vec<(String, usize)> = map.into_iter().collect();
    entries.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    entries
}

fn collect_rs_files(workspace: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in WalkDir::new(workspace).into_iter().filter_entry(|entry| {
        let name = entry.file_name().to_string_lossy();
        !(entry.file_type().is_dir() && (name == "target" || name == ".git"))
    }) {
        let entry = entry.context("walk workspace for .rs files")?;
        if entry.file_type().is_file() && entry.path().extension().is_some_and(|ext| ext == "rs") {
            files.push(entry.into_path());
        }
    }
    files.sort();
    Ok(files)
}

fn scan_file(rel: &str, content: &str, re: &Regex) -> Option<FileEntry> {
    let loc = content.lines().count();
    let mut sites = Vec::new();
    let mut lints: BTreeMap<String, usize> = BTreeMap::new();

    for caps in re.captures_iter(content) {
        let is_module_level = caps.get(1).is_some();
        let inner = caps.get(2).map_or("", |m| m.as_str());
        let lint_names: Vec<String> = inner
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();
        if !lint_names.iter().any(|l| l.starts_with("clippy::")) {
            continue;
        }

        let whole = caps.get(0).expect("capture group 0 always matches");
        let line = content[..whole.start()].matches('\n').count() + 1;
        for lint in &lint_names {
            *lints.entry(lint.clone()).or_insert(0) += 1;
        }
        sites.push(Site {
            line,
            lints: lint_names,
            module_level: is_module_level,
        });
    }

    if sites.is_empty() {
        return None;
    }
    sites.sort_by_key(|s| s.line);
    let module_level = sites.iter().filter(|s| s.module_level).count();

    Some(FileEntry {
        file: rel.to_owned(),
        crate_name: crate_for_path(rel),
        loc,
        count: sites.len(),
        module_level,
        lints,
        sites,
    })
}

fn crate_for_path(rel: &str) -> String {
    let mut parts = rel.split('/');
    match parts.next() {
        Some("crates") => parts.next().unwrap_or("unknown").to_owned(),
        Some(first) => first.to_owned(),
        None => "unknown".to_owned(),
    }
}

fn regenerate_html(workspace: &Path, ledger: &Ledger) -> Result<()> {
    let template_path = workspace.join(TEMPLATE_PATH);
    let template = fs::read_to_string(&template_path)
        .with_context(|| format!("read {}", template_path.display()))?;
    if !template.contains(PLACEHOLDER) {
        anyhow::bail!(
            "{} is missing the {} placeholder",
            template_path.display(),
            PLACEHOLDER
        );
    }
    let json = serde_json::to_string(ledger).context("serialize ledger")?;
    let rendered = template.replacen(PLACEHOLDER, &json, 1);

    let output_path = workspace.join(OUTPUT_PATH);
    fs::write(&output_path, rendered)
        .with_context(|| format!("write {}", output_path.display()))?;
    Ok(())
}

fn next_target(ledger: &Ledger, skip: &BTreeSet<String>) -> NextTarget {
    if let Some(file) = ledger
        .meta
        .pattern_files
        .iter()
        .find(|file| !skip.contains(file.as_str()))
    {
        let loc = ledger
            .files
            .iter()
            .find(|f| &f.file == file)
            .map_or(0, |f| f.loc);
        return NextTarget::Tier1 {
            file: file.clone(),
            allow: format!("#![allow({})]", BLANKET_PATTERN_LINTS.join(", ")),
            loc,
        };
    }

    for lint in TIER2_PRIORITY {
        let mut candidates: Vec<(&str, usize)> = ledger
            .files
            .iter()
            .filter(|f| !skip.contains(f.file.as_str()))
            .flat_map(|f| {
                f.sites
                    .iter()
                    .filter(|s| s.lints.iter().any(|l| l == lint))
                    .map(move |s| (f.file.as_str(), s.line))
            })
            .collect();
        candidates.sort_unstable();
        if let Some((file, line)) = candidates.into_iter().next() {
            return NextTarget::Tier2 {
                lint: (*lint).to_owned(),
                file: file.to_owned(),
                line,
            };
        }
    }

    NextTarget::None
}

fn print_next(target: &NextTarget) {
    match target {
        NextTarget::Tier1 { file, allow, loc } => {
            println!("tier=1");
            println!("file={file}");
            println!("allow={allow}");
            println!("loc={loc}");
        }
        NextTarget::Tier2 { lint, file, line } => {
            println!("tier=2");
            println!("lint={lint}");
            println!("file={file}");
            println!("line={line}");
        }
        NextTarget::None => println!("tier=none"),
    }
}

/// One night's outcome, parsed back out of a `Clippy-cleanup-run:` commit
/// trailer (see docs/automation/clippy-cleanup-routine.md). This is the
/// tuner's only state, and it already lives in git history for other
/// reasons — no separate tracking file.
struct RunRecord {
    fixes: u32,
    skipped: u32,
    duration_s: u64,
    cap: u32,
}

fn parse_run_trailer(line: &str) -> Option<RunRecord> {
    let rest = line.trim().strip_prefix(RUN_TRAILER_PREFIX)?.trim();
    let mut fixes = None;
    let mut skipped = None;
    let mut duration_s = None;
    let mut cap = None;
    for field in rest.split_whitespace() {
        let (key, value) = field.split_once('=')?;
        match key {
            "fixes" => fixes = value.parse().ok(),
            "skipped" => skipped = value.parse().ok(),
            "duration_s" => duration_s = value.parse().ok(),
            "cap" => cap = value.parse().ok(),
            _ => {}
        }
    }
    Some(RunRecord {
        fixes: fixes?,
        skipped: skipped?,
        duration_s: duration_s?,
        cap: cap?,
    })
}

/// The most recent `limit` past runs, newest first, read straight from `git
/// log` rather than any persisted file.
fn recent_run_records(limit: usize) -> Result<Vec<RunRecord>> {
    let output = Command::new("git")
        .args([
            "log",
            &format!("--grep=^{RUN_TRAILER_PREFIX}"),
            "-n",
            &limit.to_string(),
            &format!("--format=%B{GIT_LOG_RECORD_SEP}"),
        ])
        .output()
        .context("run `git log` for clippy-cleanup-run history")?;
    if !output.status.success() {
        anyhow::bail!(
            "git log exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let records = stdout
        .split(GIT_LOG_RECORD_SEP)
        .map(str::trim)
        .filter(|block| !block.is_empty())
        .filter_map(|block| block.lines().find_map(parse_run_trailer))
        .collect();
    Ok(records)
}

/// Decide tonight's batch cap from the last `TUNE_LOOKBACK_RUNS` runs.
/// Only ever moves the cap by `BATCH_CAP_STEP`, clamped to
/// `[MIN_BATCH_CAP, MAX_BATCH_CAP]` — never the time-box, never the lint
/// selection policy.
fn decide_batch_cap(window: &[RunRecord], last_cap: u32) -> (u32, String) {
    let should_decrease = window
        .iter()
        .any(|r| r.duration_s * 10 >= TIME_BOX_SECONDS * 9 || r.skipped > 0);
    if should_decrease {
        let cap = last_cap.saturating_sub(BATCH_CAP_STEP).max(MIN_BATCH_CAP);
        let reason = "a recent run used >=90% of the time-box or skipped a target";
        return if cap == last_cap {
            (cap, format!("held at minimum ({MIN_BATCH_CAP}): {reason}"))
        } else {
            (cap, format!("decreased: {reason}"))
        };
    }

    let should_increase = window.len() >= TUNE_LOOKBACK_RUNS
        && window.iter().all(|r| r.fixes == r.cap)
        && window
            .iter()
            .all(|r| r.duration_s * 10 <= TIME_BOX_SECONDS * 6);
    if should_increase {
        let cap = (last_cap + BATCH_CAP_STEP).min(MAX_BATCH_CAP);
        let reason = "recent runs filled the cap well under the time-box";
        return if cap == last_cap {
            (cap, format!("held at maximum ({MAX_BATCH_CAP}): {reason}"))
        } else {
            (cap, format!("increased: {reason}"))
        };
    }

    if window.len() < TUNE_LOOKBACK_RUNS {
        return (
            last_cap,
            format!(
                "insufficient-history: only {}/{TUNE_LOOKBACK_RUNS} run(s) recorded",
                window.len()
            ),
        );
    }

    (
        last_cap,
        "unchanged: no consistent signal to raise or lower the cap".to_owned(),
    )
}

fn tuned_batch_cap() -> Result<(u32, String)> {
    let records = recent_run_records(TUNE_LOOKBACK_RUNS)?;
    let Some(latest) = records.first() else {
        return Ok((DEFAULT_BATCH_CAP, "no-history".to_owned()));
    };
    let last_cap = latest.cap.clamp(MIN_BATCH_CAP, MAX_BATCH_CAP);
    Ok(decide_batch_cap(&records, last_cap))
}

fn print_tuned_batch_cap() -> Result<()> {
    let (cap, reason) = tuned_batch_cap()?;
    println!("batch_cap={cap}");
    println!("reason={reason}");
    Ok(())
}

fn print_help() {
    eprintln!("clippy-ledger — regenerate the clippy exemption ledger and pick the next safe fix");
    eprintln!();
    eprintln!("USAGE:");
    eprintln!("    cargo xtask clippy-ledger [--next [--skip <file>[,<file>...]] | --json | --tune-batch-cap]");
    eprintln!();
    eprintln!("    (no flags)        regenerate plans/clippy-exemption-ledger.html from the");
    eprintln!("                      checked-in template and print a one-line summary");
    eprintln!(
        "    --next            print the next deterministic, safe-to-fix target (or `tier=none`)"
    );
    eprintln!(
        "    --skip <files>    with --next, exclude these files (comma-separated) from selection —"
    );
    eprintln!("                      for skipping a target that just failed verification within the same run");
    eprintln!("    --json            print the full ledger as JSON instead of writing the HTML");
    eprintln!("    --tune-batch-cap  print tonight's self-tuned batch cap, derived from past");
    eprintln!("                      `Clippy-cleanup-run:` commit trailers in `git log` (no tracking file)");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_record(fixes: u32, skipped: u32, duration_s: u64, cap: u32) -> RunRecord {
        RunRecord {
            fixes,
            skipped,
            duration_s,
            cap,
        }
    }

    #[test]
    fn parses_empty_noop_commit_trailer_for_tuning() {
        let record = parse_run_trailer(
            "Clippy-cleanup-run: fixes=0 skipped=3 duration_s=2400 cap=4 timebox_s=2400",
        )
        .expect("valid no-op run trailer");

        assert_eq!(record.fixes, 0);
        assert_eq!(record.skipped, 3);
        assert_eq!(record.duration_s, 2400);
        assert_eq!(record.cap, 4);
    }

    #[test]
    fn decreases_cap_after_all_skipped_run() {
        let window = [run_record(0, 4, 300, 4)];

        let (cap, reason) = decide_batch_cap(&window, 4);

        assert_eq!(cap, 3);
        assert!(reason.starts_with("decreased:"));
    }

    #[test]
    fn decreases_cap_after_pre_success_time_box_run() {
        let window = [run_record(0, 0, TIME_BOX_SECONDS, 4)];

        let (cap, reason) = decide_batch_cap(&window, 4);

        assert_eq!(cap, 3);
        assert!(reason.starts_with("decreased:"));
    }

    #[test]
    fn does_not_increase_cap_without_full_history() {
        let window = [run_record(4, 0, 500, 4)];

        let (cap, reason) = decide_batch_cap(&window, 4);

        assert_eq!(cap, 4);
        assert_eq!(reason, "insufficient-history: only 1/3 run(s) recorded");
    }
}
