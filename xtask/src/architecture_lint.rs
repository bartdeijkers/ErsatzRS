//! Ratcheting architecture checks for the Phase 7 application boundary.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use syn::{parse::Parser, visit::Visit};

const DEFAULT_BASELINE: &str = ".architecture-lint-baseline.toml";
const APP_MANIFEST: &str = "crates/ersatzrs-app/Cargo.toml";
const APP_SOURCE: &str = "crates/ersatzrs-app/src/lib.rs";
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
    "ersatzrs-scanner",
    "ersatzrs-scheduler",
    "ersatzrs-web",
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
        "architecture-lint: passed ({} direct admin SQL call(s), {} server-error default loader(s) ratcheted)",
        counts.direct_admin_sql.values().sum::<usize>(),
        counts.server_error_to_default.values().sum::<usize>()
    );
    Ok(())
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
struct DebtCounts {
    direct_admin_sql: BTreeMap<String, usize>,
    server_error_to_default: BTreeMap<String, usize>,
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
        if visitor.server_error_to_default > 0 {
            counts
                .server_error_to_default
                .insert(display_path, visitor.server_error_to_default);
        }
    }
    Ok(counts)
}

struct AdminDebtVisitor {
    direct_admin_sql: usize,
    server_error_to_default: usize,
    imports: SqlxImports,
}

impl AdminDebtVisitor {
    fn new(imports: SqlxImports) -> Self {
        Self {
            direct_admin_sql: 0,
            server_error_to_default: 0,
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

    fn visit_macro(&mut self, expression: &'ast syn::Macro) {
        if is_sqlx_query_path(&expression.path, &self.imports) {
            self.direct_admin_sql += 1;
        }
        syn::visit::visit_macro(self, expression);
    }

    fn visit_expr_method_call(&mut self, expression: &'ast syn::ExprMethodCall) {
        if is_raw_executor_sql_call(expression, &self.imports) {
            self.direct_admin_sql += 1;
        }
        if expression.method == "unwrap_or_default" && is_await(&expression.receiver) {
            self.server_error_to_default += 1;
        }
        syn::visit::visit_expr_method_call(self, expression);
    }
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
        "server-error-to-default",
        &current.server_error_to_default,
        &baseline.server_error_to_default,
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
        assert_eq!(visitor.server_error_to_default, 1);
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
