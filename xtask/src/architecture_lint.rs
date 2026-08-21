//! Ratcheting architecture checks for the Phase 7 application boundary.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use syn::{parse::Parser, spanned::Spanned, visit::Visit};

const DEFAULT_BASELINE: &str = ".architecture-lint-baseline.toml";
const APP_MANIFEST: &str = "crates/ersatzrs-app/Cargo.toml";
const APP_SOURCE: &str = "crates/ersatzrs-app/src/lib.rs";
const CORE_MANIFEST: &str = "crates/ersatzrs-core/Cargo.toml";
const ADDON_CONTRACT_MANIFEST: &str = "crates/ersatzrs-addon-contract/Cargo.toml";
const ADDON_CONTRACT_REPOSITORY: &str = "https://github.com/bartdeijkers/ErsatzRS";
const SCAN_STATE_SOURCE: &str = "crates/ersatzrs-web/src/host/state.rs";
const SCAN_SERVER_FN_SOURCE: &str = "crates/ersatzrs-web/src/admin/server_fns/libraries.rs";
const SCAN_MAINTENANCE_SOURCE: &str = "crates/ersatzrs-infra/src/maintenance.rs";
const CRATES_ROOT: &str = "crates";
const ADDON_CONTRACT_ROOT: &str = "crates/ersatzrs-addon-contract";
const PROVIDER_AVAILABILITY_TOKENS: &[&str] =
    &["premium_only", "subscriber_only", "needs_auth", "unlisted"];
const ADMIN_ROOTS: &[&str] = &[
    "crates/ersatzrs-web/src/admin",
    "crates/ersatzrs-web/src/api",
];
const FORBIDDEN_APP_DEPENDENCIES: &[&str] = &[
    "ersatzrs-ffmpeg",
    "ersatzrs-healthcheck",
    "ersatzrs-hwprobe",
    "ersatzrs-infra",
    "ersatzrs-launcher",
    "ersatzrs-platform",
    "ersatzrs-scanner",
    "ersatzrs-scheduler",
    "ersatzrs-web",
];
const FORBIDDEN_CORE_DEPENDENCIES: &[&str] = &["diesel", "reqwest", "sea-orm", "sqlx"];
/// The module that defines `begin_write`; its own non-SQLite fallback is the
/// one legitimate deferred `begin()` in the workspace.
const WRITE_TRANSACTION_HELPER_SOURCE: &str = "crates/ersatzrs-infra/src/db.rs";
/// Comment marker that documents a deliberately deferred, read-only transaction.
const READ_ONLY_TRANSACTION_MARKER: &str = "read-only transaction";
/// How far above a `begin()` the read-only marker comment may sit.
const READ_ONLY_MARKER_LOOKBEHIND: usize = 8;
/// How far a transaction body is followed while looking for its commit.
const TRANSACTION_BODY_LIMIT: usize = 400;
/// SQL that proves a transaction body writes, so it must not be deferred.
const WRITE_SQL_MARKERS: &[&str] = &[
    "INSERT INTO",
    "INSERT OR",
    "REPLACE INTO",
    "DELETE FROM",
    "UPDATE \"",
];

pub fn run(args: Vec<String>) -> ExitCode {
    let opts = match Opts::parse(args) {
        Ok(opts) => opts,
        Err(ParseResult::Help) => {
            print_help();
            return ExitCode::SUCCESS;
        }
        Err(ParseResult::Error(message)) => {
            eprintln!("architecture-lint: {message}");
            print_help();
            return ExitCode::FAILURE;
        }
    };

    match run_inner(&opts) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("architecture-lint: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run_inner(opts: &Opts) -> Result<()> {
    let workspace = Path::new(".");
    check_app_crate(workspace)?;
    check_core_crate(workspace)?;
    check_addon_contract_distribution(workspace)?;
    check_scan_orchestration_boundary(workspace)?;
    check_write_transaction_begins(workspace)?;
    check_provider_availability_vocabulary_boundary(workspace)?;
    let counts = scan_admin_trees(workspace)?;

    if opts.print_baseline {
        let rendered = toml::to_string_pretty(&counts).context("render architecture baseline")?;
        print!("{rendered}");
        return Ok(());
    }

    let baseline_path = opts
        .baseline
        .as_deref()
        .unwrap_or_else(|| Path::new(DEFAULT_BASELINE));
    let baseline_source = fs::read_to_string(baseline_path)
        .with_context(|| format!("read baseline {}", baseline_path.display()))?;
    let baseline: DebtCounts = toml::from_str(&baseline_source)
        .with_context(|| format!("parse baseline {}", baseline_path.display()))?;

    let problems = compare_counts(&counts, &baseline);
    if !problems.is_empty() {
        for problem in &problems {
            eprintln!("{problem}");
        }
        anyhow::bail!(
            "{} guardrail violation(s); reduce the code and baseline together, but never raise a debt budget",
            problems.len()
        );
    }

    println!(
        "architecture-lint: passed ({} direct admin SQL call(s), {} raw admin pool access(es), {} server-error default loader(s), {} row-decode default(s) ratcheted)",
        counts.direct_admin_sql.values().sum::<usize>(),
        counts.raw_admin_pool.values().sum::<usize>(),
        counts.server_error_to_default.values().sum::<usize>(),
        counts.row_decode_default.values().sum::<usize>()
    );
    Ok(())
}

fn check_addon_contract_distribution(workspace: &Path) -> Result<()> {
    let manifest_path = workspace.join(ADDON_CONTRACT_MANIFEST);
    let manifest_source = fs::read_to_string(&manifest_path)
        .with_context(|| format!("read {}", manifest_path.display()))?;
    let manifest: toml::Value = toml::from_str(&manifest_source)
        .with_context(|| format!("parse {}", manifest_path.display()))?;
    let problems = addon_contract_distribution_problems(&manifest);
    if !problems.is_empty() {
        anyhow::bail!(
            "{ADDON_CONTRACT_MANIFEST} must remain GitHub-only: {}",
            problems.join("; ")
        );
    }
    Ok(())
}

fn addon_contract_distribution_problems(manifest: &toml::Value) -> Vec<String> {
    let Some(package) = manifest.get("package").and_then(toml::Value::as_table) else {
        return vec!["missing [package] table".to_owned()];
    };
    let mut problems = Vec::new();
    if package.get("publish").and_then(toml::Value::as_bool) != Some(false) {
        problems.push("`publish = false` is required to block registry publication".to_owned());
    }
    if package.get("repository").and_then(toml::Value::as_str) != Some(ADDON_CONTRACT_REPOSITORY) {
        problems.push(format!(
            "`repository` must be `{ADDON_CONTRACT_REPOSITORY}`"
        ));
    }
    problems
}

fn check_provider_availability_vocabulary_boundary(workspace: &Path) -> Result<()> {
    let mut pending = vec![workspace.join(CRATES_ROOT)];
    let mut problems = Vec::new();
    while let Some(path) = pending.pop() {
        let metadata = fs::symlink_metadata(&path)
            .with_context(|| format!("inspect architecture path {}", path.display()))?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            if path
                .file_name()
                .is_some_and(|name| name == "target" || name == ".git")
            {
                continue;
            }
            for entry in fs::read_dir(&path)
                .with_context(|| format!("read architecture directory {}", path.display()))?
            {
                pending.push(entry?.path());
            }
            continue;
        }
        if !metadata.is_file() {
            continue;
        }
        let contents = fs::read(&path)
            .with_context(|| format!("read architecture source {}", path.display()))?;
        for token in forbidden_provider_vocabulary(&contents) {
            problems.push(format!("{} contains `{token}`", path.display()));
        }
        if !path.starts_with(workspace.join(ADDON_CONTRACT_ROOT)) {
            if contains_ascii_identifier_token(&contents, b"CLIENT_ID") {
                problems.push(format!(
                    "{} contains add-on-owned `CLIENT_ID`",
                    path.display()
                ));
            }
            if contents
                .windows(b"Trakt client ID".len())
                .any(|window| window == b"Trakt client ID")
            {
                problems.push(format!(
                    "{} contains add-on-owned `Trakt client ID` prose",
                    path.display()
                ));
            }
        }
    }
    problems.sort();
    if !problems.is_empty() {
        anyhow::bail!(
            "provider availability vocabulary belongs to add-on packages, not host crates: {}",
            problems.join(", ")
        );
    }
    Ok(())
}

fn contains_ascii_identifier_token(contents: &[u8], token: &[u8]) -> bool {
    contents
        .windows(token.len())
        .enumerate()
        .any(|(index, window)| {
            if window != token {
                return false;
            }
            let is_identifier = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
            let before = index.checked_sub(1).and_then(|before| contents.get(before));
            let after = contents.get(index + token.len());
            before.is_none_or(|byte| !is_identifier(*byte))
                && after.is_none_or(|byte| !is_identifier(*byte))
        })
}

fn forbidden_provider_vocabulary(contents: &[u8]) -> Vec<&'static str> {
    PROVIDER_AVAILABILITY_TOKENS
        .iter()
        .copied()
        .filter(|token| {
            contents
                .windows(token.len())
                .any(|window| window == token.as_bytes())
        })
        .collect()
}

fn print_help() {
    eprintln!("USAGE:");
    eprintln!("    cargo xtask architecture-lint [OPTIONS]");
    eprintln!();
    eprintln!("OPTIONS:");
    eprintln!("    --baseline <file>  Use a specific ratchet baseline");
    eprintln!("    --print-baseline   Print current production debt counts as TOML");
}

#[derive(Default)]
struct Opts {
    baseline: Option<PathBuf>,
    print_baseline: bool,
}

impl Opts {
    fn parse(args: Vec<String>) -> std::result::Result<Self, ParseResult> {
        let mut opts = Self::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "-h" | "--help" => return Err(ParseResult::Help),
                "--baseline" => {
                    let Some(path) = args.next() else {
                        return Err(ParseResult::Error(
                            "--baseline requires a file path".to_owned(),
                        ));
                    };
                    opts.baseline = Some(PathBuf::from(path));
                }
                "--print-baseline" => opts.print_baseline = true,
                _ => return Err(ParseResult::Error(format!("unknown argument `{arg}`"))),
            }
        }
        Ok(opts)
    }
}

enum ParseResult {
    Help,
    Error(String),
}

fn check_app_crate(workspace: &Path) -> Result<()> {
    let manifest_path = workspace.join(APP_MANIFEST);
    let source_path = workspace.join(APP_SOURCE);
    let manifest_source = fs::read_to_string(&manifest_path)
        .with_context(|| format!("read {}", manifest_path.display()))?;
    let manifest: toml::Value = toml::from_str(&manifest_source)
        .with_context(|| format!("parse {}", manifest_path.display()))?;
    let workspace_manifest_path = workspace.join("Cargo.toml");
    let workspace_manifest_source = fs::read_to_string(&workspace_manifest_path)
        .with_context(|| format!("read {}", workspace_manifest_path.display()))?;
    let workspace_manifest: toml::Value = toml::from_str(&workspace_manifest_source)
        .with_context(|| format!("parse {}", workspace_manifest_path.display()))?;

    let dependencies = resolve_dependencies(&manifest, &workspace_manifest);
    let forbidden_names: BTreeSet<_> = FORBIDDEN_APP_DEPENDENCIES
        .iter()
        .map(|dependency| (*dependency).to_owned())
        .collect();
    let forbidden: Vec<_> = dependencies
        .intersection(&forbidden_names)
        .cloned()
        .collect();
    if !forbidden.is_empty() {
        anyhow::bail!(
            "{APP_MANIFEST} has forbidden adapter/runtime dependencies: {}",
            forbidden.join(", ")
        );
    }

    let source = fs::read_to_string(&source_path)
        .with_context(|| format!("read {}", source_path.display()))?;
    let syntax =
        syn::parse_file(&source).with_context(|| format!("parse {}", source_path.display()))?;
    if !syntax.items.iter().any(is_public_item) {
        anyhow::bail!(
            "{APP_SOURCE} has no public application item; the app crate must not regress to a placeholder"
        );
    }

    Ok(())
}

fn check_core_crate(workspace: &Path) -> Result<()> {
    let manifest_path = workspace.join(CORE_MANIFEST);
    let manifest_source = fs::read_to_string(&manifest_path)
        .with_context(|| format!("read {}", manifest_path.display()))?;
    let manifest: toml::Value = toml::from_str(&manifest_source)
        .with_context(|| format!("parse {}", manifest_path.display()))?;
    let workspace_manifest_path = workspace.join("Cargo.toml");
    let workspace_manifest_source = fs::read_to_string(&workspace_manifest_path)
        .with_context(|| format!("read {}", workspace_manifest_path.display()))?;
    let workspace_manifest: toml::Value = toml::from_str(&workspace_manifest_source)
        .with_context(|| format!("parse {}", workspace_manifest_path.display()))?;

    let dependencies = resolve_dependencies(&manifest, &workspace_manifest);
    let forbidden_names: BTreeSet<_> = FORBIDDEN_CORE_DEPENDENCIES
        .iter()
        .map(|dependency| (*dependency).to_owned())
        .collect();
    let forbidden: Vec<_> = dependencies
        .intersection(&forbidden_names)
        .cloned()
        .collect();
    if !forbidden.is_empty() {
        anyhow::bail!(
            "{CORE_MANIFEST} has forbidden persistence/provider dependencies: {}",
            forbidden.join(", ")
        );
    }

    Ok(())
}

fn check_scan_orchestration_boundary(workspace: &Path) -> Result<()> {
    let state = read_workspace_source(workspace, SCAN_STATE_SOURCE)?;
    let server_fns = read_workspace_source(workspace, SCAN_SERVER_FN_SOURCE)?;
    let maintenance = read_workspace_source(workspace, SCAN_MAINTENANCE_SOURCE)?;
    let problems = scan_orchestration_boundary_problems(&state, &server_fns, &maintenance);
    if !problems.is_empty() {
        anyhow::bail!(
            "scan orchestration must remain application-owned: {}",
            problems.join("; ")
        );
    }
    Ok(())
}

fn read_workspace_source(workspace: &Path, relative: &str) -> Result<String> {
    let path = workspace.join(relative);
    fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))
}

fn scan_orchestration_boundary_problems(
    state: &str,
    server_fns: &str,
    maintenance: &str,
) -> Vec<String> {
    let mut problems = Vec::new();
    for forbidden in [
        "struct LibraryScanJob",
        "struct LibraryScanQueue",
        "struct ScanProgressMap",
        "library_scan_queue",
        "scan_progress:",
    ] {
        if state.contains(forbidden) {
            problems.push(format!("{SCAN_STATE_SOURCE} contains `{forbidden}`"));
        }
    }

    for forbidden in [
        "sync_library_by_id_with_progress",
        "sync_plex_show_by_id",
        "sync_jellyfin_show_by_id",
        "sync_emby_show_by_id",
        "scan_local_library_with_progress",
        "tokio::spawn",
    ] {
        if server_fns.contains(forbidden) {
            problems.push(format!(
                "{SCAN_SERVER_FN_SOURCE} bypasses the application boundary with `{forbidden}`"
            ));
        }
    }
    for required in [
        ".request_library_scan(",
        ".request_show_scan(",
        ".scan_operations(",
        ".scan_progress(",
    ] {
        if !server_fns.contains(required) {
            problems.push(format!(
                "{SCAN_SERVER_FN_SOURCE} is missing application call `{required}`"
            ));
        }
    }

    for forbidden in [
        "tokio::process::Command",
        "scanner_command",
        "ersatzrs-scanner",
    ] {
        if maintenance.contains(forbidden) {
            problems.push(format!(
                "{SCAN_MAINTENANCE_SOURCE} bypasses the application boundary with `{forbidden}`"
            ));
        }
    }
    if !maintenance.contains(".request_scheduled_library_scan(") {
        problems.push(format!(
            "{SCAN_MAINTENANCE_SOURCE} is missing scheduled application scan admission"
        ));
    }
    problems
}

/// Reject transactions that write but start in `SQLite`'s deferred mode.
///
/// A deferred transaction takes a read snapshot on its first statement and only
/// tries to take the write lock when it first writes. If another connection
/// commits in between, that upgrade fails immediately with `SQLITE_BUSY` and
/// `busy_timeout` does not apply, because waiting could never resolve it. Almost
/// every write path here reads before it writes, so the default `begin()` is a
/// latent 500 under concurrency; `ersatzrs_infra::db::begin_write` issues
/// `BEGIN IMMEDIATE` instead. Genuinely read-only transactions must stay
/// deferred so `WAL` can still run them concurrently, and say so in a comment.
fn check_write_transaction_begins(workspace: &Path) -> Result<()> {
    let root = workspace.join(CRATES_ROOT);
    if !root.is_dir() {
        anyhow::bail!("crates root does not exist: {}", root.display());
    }

    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(&root) {
        let entry = entry.context("walk crate source tree")?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if path.extension().is_none_or(|extension| extension != "rs") {
            continue;
        }
        // Only crate `src/` trees are production code; `tests/` is test code.
        if !path
            .components()
            .any(|component| component.as_os_str() == "src")
        {
            continue;
        }
        files.push(path.to_path_buf());
    }
    files.sort();

    let mut problems = Vec::new();
    for path in files {
        let source =
            fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        if !source.contains(".begin()") {
            continue;
        }
        let display_path = path
            .strip_prefix(".")
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if display_path == WRITE_TRANSACTION_HELPER_SOURCE {
            // `begin_write` itself falls back to a plain `begin()` on backends
            // where `BEGIN IMMEDIATE` is not valid syntax.
            continue;
        }
        problems.extend(
            write_transaction_problems(&display_path, &source)
                .with_context(|| format!("parse {}", path.display()))?,
        );
    }

    if !problems.is_empty() {
        for problem in &problems {
            eprintln!("{problem}");
        }
        anyhow::bail!(
            "{} deferred write transaction(s); a transaction that writes must start with \
             `begin_write` (BEGIN IMMEDIATE), because `SQLite` fails a deferred \
             read-then-write transaction immediately and `busy_timeout` cannot help",
            problems.len()
        );
    }
    Ok(())
}

fn write_transaction_problems(display_path: &str, source: &str) -> Result<Vec<String>> {
    let syntax = syn::parse_file(source)?;
    let mut visitor = DeferredBeginVisitor::default();
    visitor.visit_file(&syntax);

    let lines: Vec<&str> = source.lines().collect();
    let mut problems = Vec::new();
    for line_number in visitor.sites {
        let Some(index) = line_number.checked_sub(1) else {
            continue;
        };
        if !has_read_only_marker(&lines, index) {
            problems.push(format!(
                "{display_path}:{line_number} starts a transaction with `begin()`, which \
                 SQLite defers; use `begin_write` if it writes, or document a genuinely \
                 read-only transaction with a `{READ_ONLY_TRANSACTION_MARKER}` comment"
            ));
            continue;
        }
        if let Some(marker) = transaction_body_write_marker(&lines, index) {
            problems.push(format!(
                "{display_path}:{line_number} is documented as a \
                 `{READ_ONLY_TRANSACTION_MARKER}` but its body runs `{marker}`; use \
                 `begin_write` instead"
            ));
        }
    }
    Ok(problems)
}

/// Look for the read-only justification in the comment block above the site.
fn has_read_only_marker(lines: &[&str], index: usize) -> bool {
    let start = index.saturating_sub(READ_ONLY_MARKER_LOOKBEHIND);
    for line in lines[start..index].iter().rev() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !trimmed.starts_with("//") {
            // A statement, not a comment: the comment block has ended.
            return false;
        }
        if trimmed
            .to_ascii_lowercase()
            .contains(READ_ONLY_TRANSACTION_MARKER)
        {
            return true;
        }
    }
    false
}

/// Follow the transaction from its `begin()` to its commit, looking for writes.
fn transaction_body_write_marker(lines: &[&str], index: usize) -> Option<&'static str> {
    let binding = transaction_binding(lines[index]);
    let end = lines.len().min(index + TRANSACTION_BODY_LIMIT);
    for line in &lines[index..end] {
        if let Some(marker) = WRITE_SQL_MARKERS
            .iter()
            .find(|marker| line.contains(**marker))
        {
            return Some(marker);
        }
        if let Some(binding) = binding.as_deref() {
            if line.contains(&format!("{binding}.commit()"))
                || line.contains(&format!("{binding}.rollback()"))
            {
                break;
            }
        }
        if line.starts_with('}') {
            break;
        }
    }
    None
}

/// Recover the transaction variable name from a `let mut tx = …begin()` line.
fn transaction_binding(line: &str) -> Option<String> {
    let rest = line.trim_start().strip_prefix("let mut ")?;
    let name: String = rest
        .chars()
        .take_while(|character| character.is_alphanumeric() || *character == '_')
        .collect();
    (!name.is_empty()).then_some(name)
}

/// Collect the source lines of `…begin().await` calls in production code.
#[derive(Default)]
struct DeferredBeginVisitor {
    sites: Vec<usize>,
}

impl<'ast> Visit<'ast> for DeferredBeginVisitor {
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        if !is_test_only(&item.attrs) {
            syn::visit::visit_item_fn(self, item);
        }
    }

    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        if !is_test_only(&item.attrs) {
            syn::visit::visit_item_mod(self, item);
        }
    }

    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        if !is_test_only(&item.attrs) {
            syn::visit::visit_item_impl(self, item);
        }
    }

    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        if !is_test_only(&item.attrs) {
            syn::visit::visit_impl_item_fn(self, item);
        }
    }

    fn visit_expr_await(&mut self, expression: &'ast syn::ExprAwait) {
        if let syn::Expr::MethodCall(call) = expression.base.as_ref() {
            if call.method == "begin" && call.args.is_empty() {
                self.sites.push(call.span().start().line);
            }
        }
        syn::visit::visit_expr_await(self, expression);
    }
}

fn is_public_item(item: &syn::Item) -> bool {
    let attributes = match item {
        syn::Item::Const(item) => &item.attrs,
        syn::Item::Enum(item) => &item.attrs,
        syn::Item::Fn(item) => &item.attrs,
        syn::Item::Mod(item) => &item.attrs,
        syn::Item::Static(item) => &item.attrs,
        syn::Item::Struct(item) => &item.attrs,
        syn::Item::Trait(item) => &item.attrs,
        syn::Item::Type(item) => &item.attrs,
        syn::Item::Union(item) => &item.attrs,
        syn::Item::Use(item) => &item.attrs,
        _ => return false,
    };
    if is_test_only(attributes) {
        return false;
    }

    match item {
        syn::Item::Const(item) => matches!(item.vis, syn::Visibility::Public(_)),
        syn::Item::Enum(item) => matches!(item.vis, syn::Visibility::Public(_)),
        syn::Item::Fn(item) => matches!(item.vis, syn::Visibility::Public(_)),
        syn::Item::Mod(item) => matches!(item.vis, syn::Visibility::Public(_)),
        syn::Item::Static(item) => matches!(item.vis, syn::Visibility::Public(_)),
        syn::Item::Struct(item) => matches!(item.vis, syn::Visibility::Public(_)),
        syn::Item::Trait(item) => matches!(item.vis, syn::Visibility::Public(_)),
        syn::Item::Type(item) => matches!(item.vis, syn::Visibility::Public(_)),
        syn::Item::Union(item) => matches!(item.vis, syn::Visibility::Public(_)),
        syn::Item::Use(item) => matches!(item.vis, syn::Visibility::Public(_)),
        _ => false,
    }
}

fn collect_dependencies(manifest: &toml::Value) -> BTreeSet<String> {
    let mut dependencies = BTreeSet::new();
    collect_dependency_tables(manifest, None, &mut dependencies);
    dependencies
}

fn resolve_dependencies(
    manifest: &toml::Value,
    workspace_manifest: &toml::Value,
) -> BTreeSet<String> {
    let mut dependencies = collect_dependencies(manifest);
    let aliases = dependencies.clone();
    let workspace_dependencies = workspace_manifest
        .get("workspace")
        .and_then(|workspace| workspace.get("dependencies"))
        .and_then(toml::Value::as_table);
    if let Some(workspace_dependencies) = workspace_dependencies {
        for alias in aliases {
            let Some(specification) = workspace_dependencies.get(&alias) else {
                continue;
            };
            if let Some(package) = specification
                .as_table()
                .and_then(|specification| specification.get("package"))
                .and_then(toml::Value::as_str)
            {
                dependencies.insert(package.to_owned());
            }
        }
    }
    dependencies
}

fn collect_dependency_tables(
    value: &toml::Value,
    key: Option<&str>,
    dependencies: &mut BTreeSet<String>,
) {
    let Some(table) = value.as_table() else {
        return;
    };
    if key.is_some_and(|key| {
        matches!(
            key,
            "dependencies" | "dev-dependencies" | "build-dependencies"
        )
    }) {
        for (dependency_name, specification) in table {
            dependencies.insert(dependency_name.to_owned());
            if let Some(package) = specification
                .as_table()
                .and_then(|specification| specification.get("package"))
                .and_then(toml::Value::as_str)
            {
                dependencies.insert(package.to_owned());
            }
        }
        return;
    }

    for (child_key, child) in table {
        collect_dependency_tables(child, Some(child_key), dependencies);
    }
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default)]
struct DebtCounts {
    direct_admin_sql: BTreeMap<String, usize>,
    raw_admin_pool: BTreeMap<String, usize>,
    server_error_to_default: BTreeMap<String, usize>,
    row_decode_default: BTreeMap<String, usize>,
}

fn scan_admin_trees(workspace: &Path) -> Result<DebtCounts> {
    let mut files = Vec::new();
    for root in ADMIN_ROOTS {
        let root = workspace.join(root);
        if !root.is_dir() {
            anyhow::bail!("admin source root does not exist: {}", root.display());
        }
        for entry in walkdir::WalkDir::new(root) {
            let entry = entry.context("walk admin source tree")?;
            if entry.file_type().is_file()
                && entry.path().extension().is_some_and(|ext| ext == "rs")
            {
                files.push(entry.into_path());
            }
        }
    }
    files.sort();

    let mut counts = DebtCounts::default();
    for path in files {
        let source =
            fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        let syntax =
            syn::parse_file(&source).with_context(|| format!("parse {}", path.display()))?;
        let imports = SqlxImports::from_file(&syntax);
        let mut visitor = AdminDebtVisitor::new(imports);
        visitor.visit_file(&syntax);
        let display_path = path
            .strip_prefix(".")
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if visitor.direct_admin_sql > 0 {
            counts
                .direct_admin_sql
                .insert(display_path.clone(), visitor.direct_admin_sql);
        }
        if visitor.raw_admin_pool > 0 {
            counts
                .raw_admin_pool
                .insert(display_path.clone(), visitor.raw_admin_pool);
        }
        if visitor.server_error_to_default > 0 {
            counts
                .server_error_to_default
                .insert(display_path.clone(), visitor.server_error_to_default);
        }
        if visitor.row_decode_default > 0 {
            counts
                .row_decode_default
                .insert(display_path, visitor.row_decode_default);
        }
    }
    Ok(counts)
}

struct AdminDebtVisitor {
    direct_admin_sql: usize,
    raw_admin_pool: usize,
    server_error_to_default: usize,
    row_decode_default: usize,
    imports: SqlxImports,
}

impl AdminDebtVisitor {
    fn new(imports: SqlxImports) -> Self {
        Self {
            direct_admin_sql: 0,
            raw_admin_pool: 0,
            server_error_to_default: 0,
            row_decode_default: 0,
            imports,
        }
    }
}

impl<'ast> Visit<'ast> for AdminDebtVisitor {
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        if !is_test_only(&item.attrs) {
            syn::visit::visit_item_fn(self, item);
        }
    }

    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        if !is_test_only(&item.attrs) {
            syn::visit::visit_item_mod(self, item);
        }
    }

    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        if !is_test_only(&item.attrs) {
            syn::visit::visit_item_impl(self, item);
        }
    }

    fn visit_item_type(&mut self, _item: &'ast syn::ItemType) {
        // A module may retain an unconditionally imported alias solely for
        // cfg(test) compatibility helpers. Count concrete production uses,
        // not the alias declaration itself.
    }

    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        if !is_test_only(&item.attrs) {
            syn::visit::visit_impl_item_fn(self, item);
        }
    }

    fn visit_expr_call(&mut self, expression: &'ast syn::ExprCall) {
        if is_sqlx_query_expression(&expression.func, &self.imports)
            || is_sqlx_executor_function(&expression.func, &self.imports)
        {
            self.direct_admin_sql += 1;
        }
        syn::visit::visit_expr_call(self, expression);
    }

    fn visit_expr_field(&mut self, expression: &'ast syn::ExprField) {
        if matches!(&expression.member, syn::Member::Named(member) if member == "pool") {
            self.raw_admin_pool += 1;
        }
        syn::visit::visit_expr_field(self, expression);
    }

    fn visit_type_path(&mut self, ty: &'ast syn::TypePath) {
        if is_raw_pool_type_path(&ty.path) {
            self.raw_admin_pool += 1;
        }
        syn::visit::visit_type_path(self, ty);
    }

    fn visit_macro(&mut self, expression: &'ast syn::Macro) {
        if is_sqlx_query_path(&expression.path, &self.imports) {
            self.direct_admin_sql += 1;
        }
        if suspense_fallback_renders_form(expression) {
            self.server_error_to_default += 1;
        }
        syn::visit::visit_macro(self, expression);
    }

    fn visit_expr_method_call(&mut self, expression: &'ast syn::ExprMethodCall) {
        if is_raw_executor_sql_call(expression, &self.imports) {
            self.direct_admin_sql += 1;
        }
        if is_result_default_method(&expression.method.to_string())
            && is_await(&expression.receiver)
        {
            self.server_error_to_default += 1;
        }
        if is_row_decode_default(expression) {
            self.row_decode_default += 1;
        }
        syn::visit::visit_expr_method_call(self, expression);
    }

    fn visit_expr_if(&mut self, expression: &'ast syn::ExprIf) {
        if is_silent_awaited_ok_branch(expression) {
            self.server_error_to_default += 1;
        }
        syn::visit::visit_expr_if(self, expression);
    }
}

fn is_raw_pool_type_path(path: &syn::Path) -> bool {
    let Some(last) = path.segments.last() else {
        return false;
    };
    let name = last.ident.to_string();
    if matches!(
        name.as_str(),
        "DbPool" | "SqlitePool" | "MySqlPool" | "PgPool"
    ) {
        return true;
    }
    name == "Pool"
        && path
            .segments
            .first()
            .is_some_and(|segment| segment.ident == "sqlx")
}

fn is_result_default_method(name: &str) -> bool {
    matches!(
        name,
        "unwrap_or_default" | "unwrap_or" | "unwrap_or_else" | "ok"
    )
}

fn is_row_decode_default(expression: &syn::ExprMethodCall) -> bool {
    matches!(
        expression.method.to_string().as_str(),
        "unwrap_or_default" | "unwrap_or" | "unwrap_or_else"
    ) && is_try_get_call(&expression.receiver)
}

fn is_try_get_call(expression: &syn::Expr) -> bool {
    match expression {
        syn::Expr::MethodCall(call) => call.method == "try_get",
        syn::Expr::Group(group) => is_try_get_call(&group.expr),
        syn::Expr::Paren(paren) => is_try_get_call(&paren.expr),
        syn::Expr::Try(expression) => is_try_get_call(&expression.expr),
        _ => false,
    }
}

fn is_silent_awaited_ok_branch(expression: &syn::ExprIf) -> bool {
    let syn::Expr::Let(condition) = expression.cond.as_ref() else {
        return false;
    };
    if !pattern_is_ok(&condition.pat) || !contains_await(&condition.expr) {
        return false;
    }

    match &expression.else_branch {
        None => true,
        Some((_, branch)) => {
            matches!(branch.as_ref(), syn::Expr::Block(block) if block.block.stmts.is_empty())
        }
    }
}

fn pattern_is_ok(pattern: &syn::Pat) -> bool {
    match pattern {
        syn::Pat::TupleStruct(pattern) => pattern
            .path
            .segments
            .last()
            .is_some_and(|part| part.ident == "Ok"),
        syn::Pat::Paren(pattern) => pattern_is_ok(&pattern.pat),
        _ => false,
    }
}

fn contains_await(expression: &syn::Expr) -> bool {
    struct AwaitVisitor(bool);

    impl<'ast> Visit<'ast> for AwaitVisitor {
        fn visit_expr_await(&mut self, _expression: &'ast syn::ExprAwait) {
            self.0 = true;
        }
    }

    let mut visitor = AwaitVisitor(false);
    visitor.visit_expr(expression);
    visitor.0
}

fn suspense_fallback_renders_form(expression: &syn::Macro) -> bool {
    if !expression
        .path
        .segments
        .last()
        .is_some_and(|part| part.ident == "view")
    {
        return false;
    }

    let tokens: Vec<_> = expression.tokens.clone().into_iter().collect();
    let has_suspense = tokens.windows(2).any(|pair| {
        matches!(&pair[0], proc_macro2::TokenTree::Punct(punct) if punct.as_char() == '<')
            && matches!(&pair[1], proc_macro2::TokenTree::Ident(ident) if ident == "Suspense")
    });
    if !has_suspense {
        return false;
    }

    let Some(fallback_index) = tokens.iter().position(
        |token| matches!(token, proc_macro2::TokenTree::Ident(ident) if ident == "fallback"),
    ) else {
        return false;
    };
    let fallback: Vec<_> = tokens
        .into_iter()
        .skip(fallback_index + 1)
        .skip_while(|token| {
            matches!(token, proc_macro2::TokenTree::Punct(punct) if punct.as_char() == '=')
        })
        .take_while(|token| {
            !matches!(token, proc_macro2::TokenTree::Punct(punct) if punct.as_char() == '>')
        })
        .collect();

    token_trees_render_form(&fallback)
}

fn token_trees_render_form(tokens: &[proc_macro2::TokenTree]) -> bool {
    if tokens.windows(2).any(|pair| {
        matches!(&pair[0], proc_macro2::TokenTree::Punct(punct) if punct.as_char() == '<')
            && matches!(&pair[1], proc_macro2::TokenTree::Ident(ident) if {
                let name = ident.to_string();
                name == "form" || name.ends_with("Form")
            })
    }) {
        return true;
    }

    tokens.iter().any(|token| match token {
        proc_macro2::TokenTree::Ident(ident) => {
            let name = ident.to_string();
            name.starts_with("render_") || name == "form" || name.ends_with("_form")
        }
        proc_macro2::TokenTree::Group(group) => {
            token_trees_render_form(&group.stream().into_iter().collect::<Vec<_>>())
        }
        _ => false,
    })
}

fn is_test_only(attributes: &[syn::Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        attribute.path().is_ident("test")
            || (attribute.path().is_ident("cfg")
                && attribute
                    .parse_args::<syn::Meta>()
                    .is_ok_and(|cfg| evaluate_cfg_without_test(&cfg) == CfgValue::False))
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CfgValue {
    True,
    False,
    Unknown,
}

fn evaluate_cfg_without_test(meta: &syn::Meta) -> CfgValue {
    match meta {
        syn::Meta::Path(path) if path.is_ident("test") => CfgValue::False,
        syn::Meta::Path(_) | syn::Meta::NameValue(_) => CfgValue::Unknown,
        syn::Meta::List(list) => {
            let parser = syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated;
            let Ok(arguments) = parser.parse2(list.tokens.clone()) else {
                return CfgValue::Unknown;
            };
            let values: Vec<_> = arguments.iter().map(evaluate_cfg_without_test).collect();
            if list.path.is_ident("all") {
                evaluate_cfg_all(&values)
            } else if list.path.is_ident("any") {
                evaluate_cfg_any(&values)
            } else if list.path.is_ident("not") && values.len() == 1 {
                match values[0] {
                    CfgValue::True => CfgValue::False,
                    CfgValue::False => CfgValue::True,
                    CfgValue::Unknown => CfgValue::Unknown,
                }
            } else {
                CfgValue::Unknown
            }
        }
    }
}

fn evaluate_cfg_all(values: &[CfgValue]) -> CfgValue {
    if values.contains(&CfgValue::False) {
        CfgValue::False
    } else if values.iter().all(|value| *value == CfgValue::True) {
        CfgValue::True
    } else {
        CfgValue::Unknown
    }
}

fn evaluate_cfg_any(values: &[CfgValue]) -> CfgValue {
    if values.contains(&CfgValue::True) {
        CfgValue::True
    } else if values.iter().all(|value| *value == CfgValue::False) {
        CfgValue::False
    } else {
        CfgValue::Unknown
    }
}

#[derive(Default)]
struct SqlxImports {
    crate_names: BTreeSet<String>,
    query_names: BTreeSet<String>,
    query_builder_names: BTreeSet<String>,
    executor_names: BTreeSet<String>,
    glob_imported: bool,
}

impl SqlxImports {
    fn from_file(file: &syn::File) -> Self {
        let mut collector = SqlxImportCollector {
            imports: Self::default(),
        };
        collector.imports.crate_names.insert("sqlx".to_owned());
        collector.visit_file(file);
        collector.imports
    }
}

struct SqlxImportCollector {
    imports: SqlxImports,
}

impl<'ast> Visit<'ast> for SqlxImportCollector {
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        if !is_test_only(&item.attrs) {
            syn::visit::visit_item_fn(self, item);
        }
    }

    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        if !is_test_only(&item.attrs) {
            syn::visit::visit_item_mod(self, item);
        }
    }

    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        if !is_test_only(&item.attrs) {
            syn::visit::visit_impl_item_fn(self, item);
        }
    }

    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        if !is_test_only(&item.attrs) {
            collect_sqlx_use_tree(&item.tree, None, &mut self.imports);
        }
    }
}

fn collect_sqlx_use_tree(
    tree: &syn::UseTree,
    sqlx_depth: Option<usize>,
    imports: &mut SqlxImports,
) {
    match tree {
        syn::UseTree::Path(path) => {
            let next_depth = match sqlx_depth {
                Some(depth) => Some(depth + 1),
                None if path.ident == "sqlx" => Some(0),
                None => None,
            };
            collect_sqlx_use_tree(&path.tree, next_depth, imports);
        }
        syn::UseTree::Name(name)
            if sqlx_depth == Some(0) && is_query_name(&name.ident.to_string()) =>
        {
            imports.query_names.insert(name.ident.to_string());
        }
        syn::UseTree::Name(name) if sqlx_depth == Some(0) && name.ident == "QueryBuilder" => {
            imports.query_builder_names.insert(name.ident.to_string());
        }
        syn::UseTree::Name(name) if sqlx_depth == Some(0) && name.ident == "Executor" => {
            imports.executor_names.insert(name.ident.to_string());
        }
        syn::UseTree::Rename(rename) if sqlx_depth.is_none() && rename.ident == "sqlx" => {
            imports.crate_names.insert(rename.rename.to_string());
        }
        syn::UseTree::Rename(rename)
            if sqlx_depth == Some(0) && is_query_name(&rename.ident.to_string()) =>
        {
            imports.query_names.insert(rename.rename.to_string());
        }
        syn::UseTree::Rename(rename) if sqlx_depth == Some(0) && rename.ident == "QueryBuilder" => {
            imports
                .query_builder_names
                .insert(rename.rename.to_string());
        }
        syn::UseTree::Rename(rename) if sqlx_depth == Some(0) && rename.ident == "Executor" => {
            imports.executor_names.insert(rename.rename.to_string());
        }
        syn::UseTree::Rename(rename) if sqlx_depth == Some(0) && rename.ident == "self" => {
            imports.crate_names.insert(rename.rename.to_string());
        }
        syn::UseTree::Group(group) => {
            for item in &group.items {
                collect_sqlx_use_tree(item, sqlx_depth, imports);
            }
        }
        syn::UseTree::Glob(_) if sqlx_depth == Some(0) => imports.glob_imported = true,
        _ => {}
    }
}

fn is_sqlx_query_expression(expression: &syn::Expr, imports: &SqlxImports) -> bool {
    match expression {
        syn::Expr::Path(path) => is_sqlx_query_path(&path.path, imports),
        syn::Expr::Paren(paren) => is_sqlx_query_expression(&paren.expr, imports),
        _ => false,
    }
}

fn is_sqlx_query_path(path: &syn::Path, imports: &SqlxImports) -> bool {
    let Some(first) = path.segments.first() else {
        return false;
    };
    let Some(last) = path.segments.last() else {
        return false;
    };
    let first = first.ident.to_string();
    let last = last.ident.to_string();
    (path.segments.len() == 1
        && (imports.query_names.contains(&last) || (imports.glob_imported && is_query_name(&last))))
        || (imports.crate_names.contains(&first) && is_query_name(&last))
        || (last == "new"
            && ((imports.crate_names.contains(&first)
                && path
                    .segments
                    .iter()
                    .any(|segment| segment.ident == "QueryBuilder"))
                || imports.query_builder_names.contains(&first)
                || (imports.glob_imported && first == "QueryBuilder")))
}

fn is_raw_executor_sql_call(expression: &syn::ExprMethodCall, imports: &SqlxImports) -> bool {
    is_executor_sql_method(&expression.method.to_string())
        && !expression.args.is_empty()
        && !is_sqlx_query_chain(&expression.receiver, imports)
        && (imports.glob_imported
            || !imports.executor_names.is_empty()
            || expression.args.first().is_some_and(is_obvious_sql_source))
}

fn is_sqlx_executor_function(expression: &syn::Expr, imports: &SqlxImports) -> bool {
    let syn::Expr::Path(path) = expression else {
        return false;
    };
    let segments: Vec<_> = path.path.segments.iter().collect();
    let Some(method) = segments.last() else {
        return false;
    };
    if !is_executor_sql_method(&method.ident.to_string()) || segments.len() < 2 {
        return false;
    }
    let executor = &segments[segments.len() - 2].ident;
    if segments.len() == 2 {
        return imports.executor_names.contains(&executor.to_string())
            || (imports.glob_imported && executor == "Executor");
    }

    let crate_name = &segments[0].ident.to_string();
    imports.crate_names.contains(crate_name) && executor == "Executor"
}

fn is_sqlx_query_chain(expression: &syn::Expr, imports: &SqlxImports) -> bool {
    match expression {
        syn::Expr::Call(call) => is_sqlx_query_expression(&call.func, imports),
        syn::Expr::Macro(expression) => is_sqlx_query_path(&expression.mac.path, imports),
        syn::Expr::MethodCall(call) => is_sqlx_query_chain(&call.receiver, imports),
        syn::Expr::Await(expression) => is_sqlx_query_chain(&expression.base, imports),
        syn::Expr::Group(expression) => is_sqlx_query_chain(&expression.expr, imports),
        syn::Expr::Paren(expression) => is_sqlx_query_chain(&expression.expr, imports),
        syn::Expr::Try(expression) => is_sqlx_query_chain(&expression.expr, imports),
        _ => false,
    }
}

fn is_obvious_sql_source(expression: &syn::Expr) -> bool {
    match expression {
        syn::Expr::Lit(literal) => matches!(literal.lit, syn::Lit::Str(_)),
        syn::Expr::Macro(_) | syn::Expr::Binary(_) => true,
        syn::Expr::Group(expression) => is_obvious_sql_source(&expression.expr),
        syn::Expr::Paren(expression) => is_obvious_sql_source(&expression.expr),
        syn::Expr::Reference(expression) => is_obvious_sql_source(&expression.expr),
        _ => false,
    }
}

fn is_executor_sql_method(name: &str) -> bool {
    matches!(
        name,
        "execute"
            | "execute_many"
            | "fetch"
            | "fetch_many"
            | "fetch_all"
            | "fetch_one"
            | "fetch_optional"
            | "prepare"
            | "prepare_with"
            | "describe"
    )
}

fn is_query_name(name: &str) -> bool {
    matches!(
        name,
        "query"
            | "raw_sql"
            | "query_as"
            | "query_as_with"
            | "query_file"
            | "query_file_as"
            | "query_scalar"
            | "query_scalar_with"
            | "query_with"
    )
}

fn is_await(expression: &syn::Expr) -> bool {
    match expression {
        syn::Expr::Await(_) => true,
        syn::Expr::Group(group) => is_await(&group.expr),
        syn::Expr::Paren(paren) => is_await(&paren.expr),
        _ => false,
    }
}

fn compare_counts(current: &DebtCounts, baseline: &DebtCounts) -> Vec<String> {
    let mut problems = Vec::new();
    compare_rule(
        "direct-admin-sql",
        &current.direct_admin_sql,
        &baseline.direct_admin_sql,
        &mut problems,
    );
    compare_rule(
        "raw-admin-pool",
        &current.raw_admin_pool,
        &baseline.raw_admin_pool,
        &mut problems,
    );
    compare_rule(
        "server-error-to-default",
        &current.server_error_to_default,
        &baseline.server_error_to_default,
        &mut problems,
    );
    compare_rule(
        "row-decode-default",
        &current.row_decode_default,
        &baseline.row_decode_default,
        &mut problems,
    );
    problems
}

fn compare_rule(
    rule: &str,
    current: &BTreeMap<String, usize>,
    baseline: &BTreeMap<String, usize>,
    problems: &mut Vec<String>,
) {
    let paths: BTreeSet<_> = current.keys().chain(baseline.keys()).collect();
    for path in paths {
        let current_count = current.get(path).copied().unwrap_or_default();
        let baseline_count = baseline.get(path).copied().unwrap_or_default();
        match current_count.cmp(&baseline_count) {
            std::cmp::Ordering::Greater => problems.push(format!(
                "{path}: architecture-lint({rule}): {current_count} occurrence(s), baseline allows {baseline_count}"
            )),
            std::cmp::Ordering::Less => problems.push(format!(
                "{path}: architecture-lint({rule}): debt fell from {baseline_count} to {current_count}; shrink the baseline"
            )),
            std::cmp::Ordering::Equal => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_on_contract_distribution_stays_github_only() {
        let valid: toml::Value = toml::from_str(
            r#"
            [package]
            publish = false
            repository = "https://github.com/bartdeijkers/ErsatzRS"
            "#,
        )
        .expect("fixture should parse");
        assert!(addon_contract_distribution_problems(&valid).is_empty());

        let publishable: toml::Value = toml::from_str(
            r#"
            [package]
            repository = "https://example.invalid/fork"
            "#,
        )
        .expect("fixture should parse");
        let problems = addon_contract_distribution_problems(&publishable);
        assert!(problems
            .iter()
            .any(|problem| problem.contains("publish = false")));
        assert!(problems
            .iter()
            .any(|problem| problem.contains("repository")));
    }

    #[test]
    fn provider_availability_vocabulary_cannot_reenter_host_crates() {
        assert!(forbidden_provider_vocabulary(b"available unavailable unknown").is_empty());
        assert_eq!(
            forbidden_provider_vocabulary(b"premium_only and needs_auth"),
            ["premium_only", "needs_auth"]
        );
    }

    #[test]
    fn add_on_owned_setting_keys_are_detected_without_matching_host_identifiers() {
        assert!(contains_ascii_identifier_token(
            b"required setting CLIENT_ID is absent",
            b"CLIENT_ID"
        ));
        assert!(!contains_ascii_identifier_token(
            b"PLEX_CLIENT_IDENTIFIER_KEY",
            b"CLIENT_ID"
        ));
    }

    #[test]
    fn writing_transactions_must_not_start_deferred() {
        let source = r###"
            async fn save(pool: &DbPool) -> Result<()> {
                let mut tx = pool.begin().await?;
                sqlx::query(r##"INSERT INTO "Block" ("Name") VALUES (?)"##)
                    .execute(&mut *tx)
                    .await?;
                tx.commit().await
            }
        "###;
        let problems =
            write_transaction_problems("crates/demo/src/lib.rs", source).expect("fixture parses");
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("crates/demo/src/lib.rs:3"));
        assert!(problems[0].contains("begin_write"));
    }

    #[test]
    fn begin_write_transactions_are_accepted() {
        let source = r###"
            async fn save(pool: &DbPool) -> Result<()> {
                let mut tx = crate::db::begin_write(pool).await?;
                sqlx::query(r##"INSERT INTO "Block" ("Name") VALUES (?)"##)
                    .execute(&mut *tx)
                    .await?;
                tx.commit().await
            }
        "###;
        assert!(write_transaction_problems("crates/demo/src/lib.rs", source)
            .expect("fixture parses")
            .is_empty());
    }

    #[test]
    fn documented_read_only_transactions_stay_deferred() {
        let source = r###"
            async fn load(pool: &DbPool) -> Result<Vec<i64>> {
                // Read-only transaction: a consistent snapshot, nothing is written,
                // so keep the deferred begin and let WAL run readers concurrently.
                let mut tx = pool.begin().await?;
                let ids = sqlx::query_scalar(r##"SELECT "Id" FROM "Block""##)
                    .fetch_all(&mut *tx)
                    .await?;
                tx.commit().await?;
                Ok(ids)
            }
        "###;
        assert!(write_transaction_problems("crates/demo/src/lib.rs", source)
            .expect("fixture parses")
            .is_empty());
    }

    #[test]
    fn read_only_annotation_cannot_cover_a_writing_transaction() {
        let source = r###"
            async fn save(pool: &DbPool) -> Result<()> {
                // Read-only transaction: this claim is wrong.
                let mut tx = pool.begin().await?;
                sqlx::query(r##"DELETE FROM "Block" WHERE "Id" = ?"##)
                    .execute(&mut *tx)
                    .await?;
                tx.commit().await
            }
        "###;
        let problems =
            write_transaction_problems("crates/demo/src/lib.rs", source).expect("fixture parses");
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("DELETE FROM"));
    }

    #[test]
    fn test_only_transactions_are_not_linted() {
        let source = r###"
            #[cfg(all(test, feature = "ssr"))]
            async fn seed(pool: &DbPool) -> Result<()> {
                let mut tx = pool.begin().await?;
                sqlx::query(r##"INSERT INTO "Block" ("Name") VALUES (?)"##)
                    .execute(&mut *tx)
                    .await?;
                tx.commit().await
            }
        "###;
        assert!(write_transaction_problems("crates/demo/src/lib.rs", source)
            .expect("fixture parses")
            .is_empty());
    }

    #[test]
    fn a_read_only_marker_only_counts_in_the_adjacent_comment_block() {
        let source = r###"
            async fn save(pool: &DbPool) -> Result<()> {
                // Read-only transaction: this comment belongs to the statement below.
                let existing = count(pool).await?;
                let mut tx = pool.begin().await?;
                sqlx::query(r##"INSERT INTO "Block" ("Name") VALUES (?)"##)
                    .execute(&mut *tx)
                    .await?;
                tx.commit().await
            }
        "###;
        let problems =
            write_transaction_problems("crates/demo/src/lib.rs", source).expect("fixture parses");
        assert_eq!(problems.len(), 1);
    }

    #[test]
    fn scan_transport_owners_cannot_reintroduce_web_or_subprocess_orchestration() {
        let state = "struct LibraryScanQueue; struct ScanProgressMap;";
        let server_fns = "tokio::spawn(sync_plex_show_by_id());";
        let maintenance = "tokio::process::Command::new(\"ersatzrs-scanner\");";

        let problems = scan_orchestration_boundary_problems(state, server_fns, maintenance);
        assert!(problems
            .iter()
            .any(|problem| problem.contains("LibraryScanQueue")));
        assert!(problems
            .iter()
            .any(|problem| problem.contains("sync_plex_show_by_id")));
        assert!(problems
            .iter()
            .any(|problem| problem.contains("tokio::process::Command")));
    }

    #[test]
    fn application_scan_commands_and_queries_satisfy_transport_boundary() {
        let server_fns = "
            app.request_library_scan();
            app.request_show_scan();
            app.scan_operations();
            app.scan_progress();
        ";
        let maintenance = "application.request_scheduled_library_scan();";

        assert!(scan_orchestration_boundary_problems(
            "pub struct AppState;",
            server_fns,
            maintenance
        )
        .is_empty());
    }

    #[test]
    fn admin_debt_visitor_counts_production_patterns_only() {
        let syntax = syn::parse_file(
            r#"
            async fn production(pool: &sqlx::SqlitePool) {
                let _ = sqlx::query("SELECT 1").fetch_one(pool).await;
                let _ = load_rows().await.unwrap_or_default();
            }

            #[cfg(test)]
            mod tests {
                async fn seed(pool: &sqlx::SqlitePool) {
                    sqlx::query!("INSERT INTO Test VALUES (1)");
                    let _ = load_rows().await.unwrap_or_default();
                }
            }
            "#,
        )
        .expect("fixture should parse");
        let mut visitor = AdminDebtVisitor::new(SqlxImports::from_file(&syntax));
        visitor.visit_file(&syntax);
        assert_eq!(visitor.direct_admin_sql, 1);
        assert_eq!(visitor.raw_admin_pool, 1);
        assert_eq!(visitor.server_error_to_default, 1);
    }

    #[test]
    fn raw_admin_pool_access_cannot_bypass_the_guard() {
        let syntax = syn::parse_file(
            r#"
            type DbPool = sqlx::Pool<sqlx::Sqlite>;

            async fn production(pool: &DbPool, state: &AppState) {
                consume(pool);
                consume(&state.pool);
            }

            #[cfg(test)]
            async fn test_helper(pool: &DbPool, state: &AppState) {
                consume(pool);
                consume(&state.pool);
            }
            "#,
        )
        .expect("fixture should parse");
        let mut visitor = AdminDebtVisitor::new(SqlxImports::from_file(&syntax));
        visitor.visit_file(&syntax);
        assert_eq!(visitor.raw_admin_pool, 2);
    }

    #[test]
    fn server_failure_fallback_variants_cannot_bypass_the_guard() {
        let syntax = syn::parse_file(
            r#"
            async fn load() {
                let _ = first().await.unwrap_or(Vec::new());
                let _ = second().await.unwrap_or_else(|_| Vec::new());
                let _ = third().await.ok();
                if let Ok(rows) = fourth().await {
                    consume(rows);
                }
                if let Ok(rows) = fifth().await {
                    consume(rows);
                } else {}

                if let Ok(value) = "1".parse::<u32>() {
                    consume(value);
                }
                if let Ok(rows) = sixth().await {
                    consume(rows);
                } else {
                    report_failure();
                }
            }
            "#,
        )
        .expect("fixture should parse");
        let mut visitor = AdminDebtVisitor::new(SqlxImports::from_file(&syntax));
        visitor.visit_file(&syntax);
        assert_eq!(visitor.server_error_to_default, 5);
    }

    #[test]
    fn form_rendering_suspense_fallback_cannot_bypass_the_guard() {
        let syntax = syn::parse_file(
            r#"
            fn page() {
                view! {
                    <Suspense fallback=move || render_settings(empty_form())>
                        {move || data.get().map(render_settings)}
                    </Suspense>
                };
                view! {
                    <Suspense fallback=move || view! { <form method="post"></form> }>
                        <p>"ready"</p>
                    </Suspense>
                };
                view! {
                    <Suspense fallback=move || settings_form()>
                        <p>"ready"</p>
                    </Suspense>
                };
                view! {
                    <Suspense fallback=move || view! { <SettingsForm/> }>
                        <p>"ready"</p>
                    </Suspense>
                };
                view! {
                    <Suspense fallback=move || view! { <p>"loading"</p> }>
                        <form method="post"></form>
                    </Suspense>
                };
            }
            "#,
        )
        .expect("fixture should parse");
        let mut visitor = AdminDebtVisitor::new(SqlxImports::from_file(&syntax));
        visitor.visit_file(&syntax);
        assert_eq!(visitor.server_error_to_default, 4);
    }

    #[test]
    fn row_decode_defaults_cannot_bypass_the_guard() {
        let syntax = syn::parse_file(
            r#"
            fn decode(row: &sqlx::sqlite::SqliteRow) {
                let _ = row.try_get::<i64, _>("Id").unwrap_or(0);
                let _ = row.try_get::<String, _>("Name").unwrap_or_default();
                let _ = row.try_get::<String, _>("Title").unwrap_or_else(|_| String::new());
                let _ = row.try_get::<Option<String>, _>("Path").ok().flatten();

                let _ = "1".parse::<u32>().unwrap_or(0);
                let _ = row.try_get::<i64, _>("Required")?;
            }
            "#,
        )
        .expect("fixture should parse");
        let mut visitor = AdminDebtVisitor::new(SqlxImports::from_file(&syntax));
        visitor.visit_file(&syntax);
        assert_eq!(visitor.row_decode_default, 3);
    }

    #[test]
    fn raw_executor_sql_cannot_bypass_the_guard() {
        let syntax = syn::parse_file(
            r#"
            use sqlx::Executor;

            async fn run(pool: &sqlx::SqlitePool, statement: &str) {
                pool.execute("DELETE FROM MediaItem").await;
                pool.fetch_one(statement).await;
                sqlx::Executor::execute(pool, "PRAGMA foreign_keys = ON").await;
                sqlx::query("SELECT 1").execute(pool).await;
            }
            "#,
        )
        .expect("fixture should parse");
        let mut visitor = AdminDebtVisitor::new(SqlxImports::from_file(&syntax));
        visitor.visit_file(&syntax);
        assert_eq!(visitor.direct_admin_sql, 4);
    }

    #[test]
    fn cfg_negation_and_disjunction_remain_production_visible() {
        let syntax = syn::parse_file(
            r#"
            #[cfg(test)]
            fn test_only() {
                sqlx::query("SELECT 1");
            }

            #[cfg(all(test, feature = "ssr"))]
            fn also_test_only() {
                sqlx::query("SELECT 2");
            }

            #[cfg(not(test))]
            fn production_only() {
                sqlx::query("SELECT 3");
            }

            #[cfg(any(test, feature = "ssr"))]
            fn production_capable() {
                sqlx::query("SELECT 4");
            }
            "#,
        )
        .expect("fixture should parse");
        let mut visitor = AdminDebtVisitor::new(SqlxImports::from_file(&syntax));
        visitor.visit_file(&syntax);
        assert_eq!(visitor.direct_admin_sql, 2);
    }

    #[test]
    fn debt_baseline_is_an_exact_downward_ratchet() {
        let mut current = DebtCounts::default();
        current.direct_admin_sql.insert("admin.rs".to_owned(), 2);
        let mut baseline = DebtCounts::default();
        baseline.direct_admin_sql.insert("admin.rs".to_owned(), 3);

        let problems = compare_counts(&current, &baseline);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("shrink the baseline"));

        baseline.direct_admin_sql.insert("admin.rs".to_owned(), 2);
        assert!(compare_counts(&current, &baseline).is_empty());

        current.direct_admin_sql.insert("admin.rs".to_owned(), 3);
        assert!(compare_counts(&current, &baseline)[0].contains("baseline allows 2"));
    }

    #[test]
    fn renamed_forbidden_dependencies_are_still_detected() {
        let manifest: toml::Value = toml::from_str(
            r#"
            [dependencies]
            storage = { package = "ersatzrs-infra", path = "../ersatzrs-infra" }
            ersatzrs-core = { path = "../ersatzrs-core" }
            "#,
        )
        .expect("fixture should parse");
        let dependencies = collect_dependencies(&manifest);
        assert!(dependencies.contains("storage"));
        assert!(dependencies.contains("ersatzrs-infra"));
        assert!(dependencies.contains("ersatzrs-core"));
        assert!(dependencies
            .intersection(
                &FORBIDDEN_APP_DEPENDENCIES
                    .iter()
                    .map(|dependency| (*dependency).to_owned())
                    .collect()
            )
            .any(|dependency| dependency == "ersatzrs-infra"));
    }

    #[test]
    fn workspace_renamed_forbidden_dependencies_are_detected() {
        let manifest: toml::Value = toml::from_str(
            r#"
            [dependencies]
            storage.workspace = true
            "#,
        )
        .expect("fixture should parse");
        let workspace: toml::Value = toml::from_str(
            r#"
            [workspace.dependencies]
            storage = { package = "ersatzrs-infra", path = "crates/ersatzrs-infra" }
            "#,
        )
        .expect("fixture should parse");
        assert!(resolve_dependencies(&manifest, &workspace).contains("ersatzrs-infra"));
    }

    #[test]
    fn renamed_storage_dependencies_cannot_reenter_core() {
        let manifest: toml::Value = toml::from_str(
            r#"
            [dependencies]
            database = { package = "sqlx", version = "0.9" }
            ersatzrs-domain = { path = "../ersatzrs-domain" }
            "#,
        )
        .expect("fixture should parse");
        let dependencies = collect_dependencies(&manifest);
        let forbidden: BTreeSet<_> = FORBIDDEN_CORE_DEPENDENCIES
            .iter()
            .map(|dependency| (*dependency).to_owned())
            .collect();

        assert!(dependencies.contains("database"));
        assert!(dependencies.contains("sqlx"));
        assert!(dependencies
            .intersection(&forbidden)
            .any(|name| name == "sqlx"));
    }

    #[test]
    fn sqlx_query_aliases_cannot_bypass_the_guard() {
        let syntax = syn::parse_file(
            r#"
            use sqlx::{query as run_query, query_scalar};
            use sqlx::QueryBuilder as SqlBuilder;
            use sqlx as database;

            fn load() {
                run_query("SELECT 1");
                query_scalar("SELECT 2");
                database::query("SELECT 3");
                SqlBuilder::<Database>::new("SELECT 4");
                database::QueryBuilder::<Database>::new("SELECT 5");
                database::raw_sql("SELECT 6");
            }
            "#,
        )
        .expect("fixture should parse");
        let mut visitor = AdminDebtVisitor::new(SqlxImports::from_file(&syntax));
        visitor.visit_file(&syntax);
        assert_eq!(visitor.direct_admin_sql, 6);
    }

    #[test]
    fn sqlx_glob_imports_cannot_bypass_the_guard() {
        for (label, expression) in [
            ("function", r#"query("SELECT 1")"#),
            ("macro", r#"query_as!(MediaItem, "SELECT 2")"#),
            (
                "query builder",
                r#"QueryBuilder::<Database>::new("SELECT 3")"#,
            ),
            ("raw sql", r#"raw_sql("SELECT 4")"#),
            ("executor", "pool.execute(statement)"),
        ] {
            let source = format!(
                "use sqlx::*; fn load(pool: &SqlitePool, statement: &str) {{ {expression}; }}"
            );
            let syntax = syn::parse_file(&source).expect("fixture should parse");
            let mut visitor = AdminDebtVisitor::new(SqlxImports::from_file(&syntax));
            visitor.visit_file(&syntax);
            assert_eq!(visitor.direct_admin_sql, 1, "missed {label}");
        }
    }

    #[test]
    fn app_crate_requires_a_public_non_test_item() {
        let placeholder = syn::parse_file("// placeholder").expect("fixture should parse");
        assert!(!placeholder.items.iter().any(is_public_item));

        let test_only = syn::parse_file("#[cfg(test)] pub struct FakeApplicationServices;")
            .expect("fixture should parse");
        assert!(!test_only.items.iter().any(is_public_item));

        let application =
            syn::parse_file("pub struct ApplicationServices;").expect("fixture should parse");
        assert!(application.items.iter().any(is_public_item));
    }
}
