//! Lints for Leptos 0.8 hydration anti-patterns that have repeatedly bitten
//! the admin UI port. The rules track TEST.md's "Hydration rules" section.
//!
//! v1 is intentionally lexical: it walks the token stream of `view! { … }`
//! invocations rather than understanding the full Leptos macro grammar.
//! False negatives are acceptable; the goal is to catch the patterns we
//! already know hurt us. False positives must be tunable from the call site
//! (no rule should ever require code restructuring that contradicts TEST.md).
//!
//! Rules implemented:
//!   * **rule-1** — `<Show>` inside `#[island]`. `Show`'s server/client
//!     branches don't always match; use `style:display`.
//!   * **rule-2** — Prop-bearing call to a known `#[island]` function inside
//!     a `<Suspense>` region. Hoist `Suspense` out of the wrapper, or pass only
//!     stable scalar props and let the island own the async boundary.
//!   * **rule-3** — The toast channel (`load_or_notify`, `ToastStore::push`)
//!     reached from a server-executing `Resource::new` / `Resource::new_blocking`
//!     loader. The store is not hydration-stable — request-scoped on the server,
//!     an empty process-wide store on the client — so a toast queued while SSR
//!     renders would serialize a `ToastContainer` child the client cannot
//!     reproduce. `ToastStore::push` is inert off-wasm precisely so that can
//!     never happen, which means the failure is simply *lost* here instead.
//!     Report failures a server-backed load can hit through `admin_result_view`,
//!     or move the load to a `LocalResource` that runs after hydration.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use proc_macro2::{Delimiter, Span, TokenStream, TokenTree};
use syn::visit::Visit;

pub fn run(args: Vec<String>) -> ExitCode {
    let opts = match parse_args(args) {
        Ok(o) => o,
        Err(msg) => {
            eprintln!("hydration-lint: {msg}");
            print_help();
            return ExitCode::FAILURE;
        }
    };
    if opts.show_help {
        print_help();
        return ExitCode::SUCCESS;
    }
    let baseline = match load_baseline(opts.baseline.as_deref()) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("hydration-lint: {e:#}");
            return ExitCode::FAILURE;
        }
    };
    match lint(&opts.roots, &baseline) {
        Ok((0, suppressed)) => {
            if suppressed == 0 {
                println!("hydration-lint: no findings");
            } else {
                println!("hydration-lint: no new findings ({suppressed} suppressed by baseline)");
            }
            ExitCode::SUCCESS
        }
        Ok((n, suppressed)) => {
            eprintln!("hydration-lint: {n} new finding(s) ({suppressed} suppressed by baseline)");
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("hydration-lint: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn print_help() {
    eprintln!("USAGE:");
    eprintln!("    cargo xtask hydration-lint -p <crate>");
    eprintln!("    cargo xtask hydration-lint --path <dir>...");
    eprintln!();
    eprintln!("OPTIONS:");
    eprintln!("    -p, --package <name>     Scan crates/<name>/src");
    eprintln!("        --path <dir>         Scan <dir> recursively (may repeat)");
    eprintln!(
        "        --baseline <file>    Suppress known findings (default: .hydration-lint-baseline)"
    );
    eprintln!("        --no-baseline        Ignore the default baseline file");
}

struct Opts {
    roots: Vec<PathBuf>,
    baseline: Option<PathBuf>,
    show_help: bool,
}

fn parse_args(args: Vec<String>) -> Result<Opts, String> {
    let mut iter = args.into_iter();
    let mut packages: Vec<String> = Vec::new();
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut show_help = false;
    let mut baseline: Option<PathBuf> = None;
    let mut no_baseline = false;
    while let Some(a) = iter.next() {
        match a.as_str() {
            "-p" | "--package" => {
                let Some(v) = iter.next() else {
                    return Err("--package requires a value".into());
                };
                packages.push(v);
            }
            "--path" => {
                let Some(v) = iter.next() else {
                    return Err("--path requires a value".into());
                };
                paths.push(PathBuf::from(v));
            }
            "--baseline" => {
                let Some(v) = iter.next() else {
                    return Err("--baseline requires a value".into());
                };
                baseline = Some(PathBuf::from(v));
            }
            "--no-baseline" => no_baseline = true,
            "-h" | "--help" => show_help = true,
            other => return Err(format!("unknown argument `{other}`")),
        }
    }
    let mut roots: Vec<PathBuf> = paths;
    for pkg in packages {
        roots.push(PathBuf::from("crates").join(pkg).join("src"));
    }
    if roots.is_empty() && !show_help {
        return Err("no targets — pass -p <crate> or --path <dir>".into());
    }
    let baseline = if no_baseline {
        None
    } else {
        baseline.or_else(|| {
            let default = PathBuf::from(".hydration-lint-baseline");
            default.exists().then_some(default)
        })
    };
    Ok(Opts {
        roots,
        baseline,
        show_help,
    })
}

/// Baseline format: one `path:rule` entry per line. `#` starts a comment.
/// A finding whose `(file, rule)` matches a baseline entry is suppressed.
fn load_baseline(path: Option<&Path>) -> Result<HashSet<(PathBuf, String)>> {
    let mut out = HashSet::new();
    let Some(p) = path else { return Ok(out) };
    let src =
        std::fs::read_to_string(p).with_context(|| format!("read baseline {}", p.display()))?;
    for raw in src.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let Some((path_part, rule)) = line.rsplit_once(':') else {
            anyhow::bail!("baseline line missing `:rule` suffix: {raw:?}");
        };
        out.insert((PathBuf::from(path_part.trim()), rule.trim().to_string()));
    }
    Ok(out)
}

fn lint(roots: &[PathBuf], baseline: &HashSet<(PathBuf, String)>) -> Result<(usize, usize)> {
    let mut files: Vec<PathBuf> = Vec::new();
    for root in roots {
        if !root.exists() {
            anyhow::bail!("path does not exist: {}", root.display());
        }
        for entry in walkdir::WalkDir::new(root) {
            let entry = entry.context("walkdir")?;
            if entry.file_type().is_file() && entry.path().extension().is_some_and(|e| e == "rs") {
                files.push(entry.into_path());
            }
        }
    }
    if files.is_empty() {
        anyhow::bail!("no .rs files under {:?}", roots);
    }

    // Pass 1: collect every `#[island]` fn name across the target tree so
    // rule-2 can recognise island invocations even when the call site lives
    // in a different file from the island declaration.
    let mut islands: HashSet<String> = HashSet::new();
    for f in &files {
        let src = std::fs::read_to_string(f).with_context(|| format!("read {}", f.display()))?;
        if let Ok(ast) = syn::parse_file(&src) {
            let mut c = IslandCollector { out: &mut islands };
            c.visit_file(&ast);
        }
    }

    // Pass 2: lint each file.
    let mut findings = 0usize;
    let mut suppressed = 0usize;
    for f in &files {
        let src = std::fs::read_to_string(f).with_context(|| format!("read {}", f.display()))?;
        if let Ok(ast) = syn::parse_file(&src) {
            let mut v = LintVisitor {
                path: f,
                islands: &islands,
                baseline,
                findings: &mut findings,
                suppressed: &mut suppressed,
            };
            v.visit_file(&ast);
        }
    }
    Ok((findings, suppressed))
}

fn has_island_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| a.path().is_ident("island"))
}

struct IslandCollector<'a> {
    out: &'a mut HashSet<String>,
}

impl<'ast> Visit<'ast> for IslandCollector<'_> {
    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        if has_island_attr(&f.attrs) {
            self.out.insert(f.sig.ident.to_string());
        }
        syn::visit::visit_item_fn(self, f);
    }
}

struct LintVisitor<'a> {
    path: &'a Path,
    islands: &'a HashSet<String>,
    baseline: &'a HashSet<(PathBuf, String)>,
    findings: &'a mut usize,
    suppressed: &'a mut usize,
}

impl<'ast> Visit<'ast> for LintVisitor<'_> {
    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        let in_island = has_island_attr(&f.attrs);
        let fn_name = f.sig.ident.to_string();
        let tokens = {
            use quote::ToTokens;
            f.block.to_token_stream()
        };
        let mut ctx = Ctx {
            path: self.path,
            fn_name: &fn_name,
            in_island,
            islands: self.islands,
            baseline: self.baseline,
            findings: self.findings,
            suppressed: self.suppressed,
        };
        walk_for_view_macros(&mut ctx, tokens.clone());
        walk_for_server_resource_loaders(&mut ctx, tokens);
        // Don't recurse into the fn body with syn::visit — token-walking covers it.
    }
}

struct Ctx<'a> {
    path: &'a Path,
    fn_name: &'a str,
    in_island: bool,
    islands: &'a HashSet<String>,
    baseline: &'a HashSet<(PathBuf, String)>,
    findings: &'a mut usize,
    suppressed: &'a mut usize,
}

fn walk_for_view_macros(ctx: &mut Ctx<'_>, tokens: TokenStream) {
    let toks: Vec<TokenTree> = tokens.into_iter().collect();
    let n = toks.len();
    let mut i = 0usize;
    while i < n {
        if let TokenTree::Ident(id) = &toks[i] {
            if id == "view" {
                if let (Some(TokenTree::Punct(p)), Some(TokenTree::Group(g))) =
                    (toks.get(i + 1), toks.get(i + 2))
                {
                    if p.as_char() == '!' && g.delimiter() == Delimiter::Brace {
                        scan_view_body(ctx, g.stream(), &mut 0);
                        i += 3;
                        continue;
                    }
                }
            }
        }
        if let TokenTree::Group(g) = &toks[i] {
            walk_for_view_macros(ctx, g.stream());
        }
        i += 1;
    }
}

/// rule-3 — find `Resource::new(…)` / `Resource::new_blocking(…)` calls whose
/// loader reaches the toast channel.
///
/// `LocalResource` is a distinct single ident, so it never matches here; that is
/// deliberate, since a `LocalResource` loader only runs on the client where the
/// toast store is real.
fn walk_for_server_resource_loaders(ctx: &mut Ctx<'_>, tokens: TokenStream) {
    let toks: Vec<TokenTree> = tokens.into_iter().collect();
    let n = toks.len();
    let mut i = 0usize;
    while i < n {
        if let TokenTree::Ident(id) = &toks[i] {
            if id == "Resource" {
                let is_path = matches!(toks.get(i + 1), Some(TokenTree::Punct(p)) if p.as_char() == ':')
                    && matches!(toks.get(i + 2), Some(TokenTree::Punct(p)) if p.as_char() == ':');
                let ctor = toks.get(i + 3).and_then(|t| match t {
                    TokenTree::Ident(name) => Some(name.to_string()),
                    _ => None,
                });
                let args = match toks.get(i + 4) {
                    Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Parenthesis => {
                        Some(g.stream())
                    }
                    _ => None,
                };
                if let (true, Some(ctor), Some(args)) = (is_path, ctor, args) {
                    if matches!(ctor.as_str(), "new" | "new_blocking") {
                        if let Some(offender) = toast_channel_call(&args) {
                            report(
                                ctx,
                                id.span(),
                                "rule-3",
                                &format!(
                                    "`{offender}` inside a server-executing `Resource::{ctor}` loader in `{}` \u{2014} the toast store is not hydration-stable, so this failure is silently dropped during SSR; render it with `admin_result_view` or move the load to a `LocalResource` (TEST.md hydration rule 5)",
                                    ctx.fn_name
                                ),
                            );
                        }
                        // Keep walking the args so a nested `Resource::new` is
                        // still reached.
                        walk_for_server_resource_loaders(ctx, args);
                        i += 5;
                        continue;
                    }
                }
            }
        }
        if let TokenTree::Group(g) = &toks[i] {
            walk_for_server_resource_loaders(ctx, g.stream());
        }
        i += 1;
    }
}

/// Name the toast-channel call inside `tokens`, if any.
///
/// Lexical on purpose, like the rest of this lint: `load_or_notify` by ident,
/// and `.push` / `.push_with_action` only when the receiver reads as a toast
/// store, so unrelated `Vec::push` calls in a loader stay quiet.
fn toast_channel_call(tokens: &TokenStream) -> Option<String> {
    let toks: Vec<TokenTree> = tokens.clone().into_iter().collect();
    for (i, tok) in toks.iter().enumerate() {
        match tok {
            TokenTree::Ident(id) if id == "load_or_notify" => {
                return Some("load_or_notify".to_owned())
            }
            TokenTree::Ident(id) if id == "push" || id == "push_with_action" => {
                let is_method = matches!(toks.get(i.wrapping_sub(1)), Some(TokenTree::Punct(p)) if p.as_char() == '.');
                let receiver_is_toast = matches!(
                    toks.get(i.wrapping_sub(2)),
                    Some(TokenTree::Ident(recv)) if recv.to_string().contains("toast")
                );
                if i >= 2 && is_method && receiver_is_toast {
                    return Some(id.to_string());
                }
            }
            TokenTree::Group(g) => {
                if let Some(found) = toast_channel_call(&g.stream()) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

fn scan_view_body(ctx: &mut Ctx<'_>, body: TokenStream, suspense_depth: &mut i32) {
    let toks: Vec<TokenTree> = body.into_iter().collect();
    let n = toks.len();
    let mut i = 0usize;
    while i < n {
        if let TokenTree::Punct(p) = &toks[i] {
            if p.as_char() == '<' {
                let mut j = i + 1;
                let mut is_closing = false;
                if matches!(toks.get(j), Some(TokenTree::Punct(q)) if q.as_char() == '/') {
                    is_closing = true;
                    j += 1;
                }
                if let Some(TokenTree::Ident(name_id)) = toks.get(j) {
                    let name = name_id.to_string();
                    let span = name_id.span();

                    let mut k = j + 1;
                    let mut self_closing = false;
                    let mut attr_count = 0usize;
                    while k < n {
                        match &toks[k] {
                            TokenTree::Punct(pp) if pp.as_char() == '>' => break,
                            TokenTree::Punct(pp) if pp.as_char() == '/' => {
                                if matches!(toks.get(k + 1), Some(TokenTree::Punct(p2)) if p2.as_char() == '>')
                                {
                                    self_closing = true;
                                    k += 1;
                                    break;
                                }
                            }
                            TokenTree::Group(g) => {
                                // Attribute values can contain nested `view! { … }`
                                // (typical for `fallback=move || view!{…}`). Scan
                                // recursively; suspense depth doesn't propagate
                                // into the closure body.
                                scan_view_body(ctx, g.stream(), &mut 0);
                                attr_count += 1;
                            }
                            TokenTree::Ident(_) => attr_count += 1,
                            _ => {}
                        }
                        k += 1;
                    }

                    if is_closing {
                        if name == "Suspense" {
                            *suspense_depth = suspense_depth.saturating_sub(1);
                        }
                    } else {
                        if name == "Suspense" && !self_closing {
                            *suspense_depth += 1;
                        }
                        if name == "Show" && ctx.in_island {
                            report(
                                ctx,
                                span,
                                "rule-1",
                                &format!(
                                    "`<Show>` inside `#[island] {}` \u{2014} use `style:display` instead (TEST.md hydration rule 2)",
                                    ctx.fn_name
                                ),
                            );
                        }
                        if ctx.islands.contains(&name) && *suspense_depth > 0 && attr_count > 0 {
                            report(
                                ctx,
                                span,
                                "rule-2",
                                &format!(
                                    "prop-bearing island `<{name}>` inside `<Suspense>` \u{2014} hoist Suspense or pass only stable scalar props and let the island own the async boundary (TEST.md hydration rule 3)"
                                ),
                            );
                        }
                    }

                    i = k + 1;
                    continue;
                }
            }
        }
        if let TokenTree::Group(g) = &toks[i] {
            scan_view_body(ctx, g.stream(), suspense_depth);
        }
        i += 1;
    }
}

fn report(ctx: &mut Ctx<'_>, span: Span, rule: &str, msg: &str) {
    let key = (ctx.path.to_path_buf(), rule.to_string());
    if ctx.baseline.contains(&key) {
        *ctx.suppressed += 1;
        return;
    }
    let loc = span.start();
    let line = loc.line;
    let col = loc.column + 1;
    eprintln!(
        "{}:{}:{}: hydration-lint({}): {}",
        ctx.path.display(),
        line,
        col,
        rule,
        msg
    );
    *ctx.findings += 1;
}
