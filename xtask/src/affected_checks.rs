//! Recommend the narrowest faithful ErsatzRS verification commands for a diff.

use std::{
    collections::BTreeSet,
    env, fs,
    path::{Component, Path, PathBuf},
    process::{Command, ExitCode},
};

use anyhow::{bail, Context, Result};

pub fn run(args: Vec<String>) -> ExitCode {
    match execute(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("affected-checks: {error:#}");
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
    let root = find_workspace_root(&env::current_dir().context("reading current directory")?)?;
    let paths = changed_paths(&root, &options)?;
    let recommendations = Recommendations::from_paths(paths);
    print!("{}", recommendations.render(cfg!(windows)));
    Ok(())
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Options {
    base: Option<String>,
    paths: Vec<String>,
}

enum ParsedOptions {
    Help,
    Run(Options),
}

impl Options {
    fn parse(args: &[String]) -> Result<ParsedOptions> {
        let mut options = Self::default();
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "-h" | "--help" => return Ok(ParsedOptions::Help),
                "--base" => {
                    let value = args
                        .get(index + 1)
                        .context("--base requires a Git revision")?;
                    if value.is_empty() {
                        bail!("--base requires a non-empty Git revision");
                    }
                    options.base = Some(value.clone());
                    index += 2;
                }
                option if option.starts_with('-') => bail!("unknown option `{option}`"),
                path => {
                    options.paths.push(path.to_owned());
                    index += 1;
                }
            }
        }
        Ok(ParsedOptions::Run(options))
    }
}

fn find_workspace_root(start: &Path) -> Result<PathBuf> {
    for candidate in start.ancestors() {
        let manifest = candidate.join("Cargo.toml");
        if candidate.join("xtask/Cargo.toml").is_file()
            && fs::read_to_string(&manifest).is_ok_and(|contents| contents.contains("[workspace]"))
        {
            return Ok(candidate.to_path_buf());
        }
    }
    bail!(
        "could not find the ErsatzRS workspace root from {}",
        start.display()
    )
}

fn changed_paths(root: &Path, options: &Options) -> Result<BTreeSet<String>> {
    let mut paths = BTreeSet::new();
    for path in &options.paths {
        paths.insert(normalize_explicit_path(root, path)?);
    }

    if options.paths.is_empty() || options.base.is_some() {
        if let Some(base) = &options.base {
            collect_git_paths(
                root,
                &[
                    "diff",
                    "--name-only",
                    "--relative",
                    &format!("{base}...HEAD"),
                ],
                &mut paths,
            )?;
        }
        collect_git_paths(
            root,
            &["diff", "--name-only", "--relative", "HEAD"],
            &mut paths,
        )?;
        collect_git_paths(
            root,
            &["ls-files", "--others", "--exclude-standard"],
            &mut paths,
        )?;
    }
    Ok(paths)
}

fn collect_git_paths(root: &Path, args: &[&str], paths: &mut BTreeSet<String>) -> Result<()> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .with_context(|| format!("running git {}", args.join(" ")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        bail!("git {} failed: {stderr}", args.join(" "));
    }
    let stdout = String::from_utf8(output.stdout).context("Git returned non-UTF-8 paths")?;
    paths.extend(stdout.lines().filter_map(normalize_git_path));
    Ok(())
}

fn normalize_explicit_path(root: &Path, value: &str) -> Result<String> {
    let raw = Path::new(value);
    let relative = if raw.is_absolute() {
        raw.strip_prefix(root).with_context(|| {
            format!(
                "explicit path {} is outside workspace {}",
                raw.display(),
                root.display()
            )
        })?
    } else {
        raw
    };
    let mut parts = Vec::new();
    for component in relative.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                bail!("explicit path `{value}` is not workspace-relative")
            }
        }
    }
    if parts.is_empty() {
        bail!("explicit path `{value}` does not identify a workspace file");
    }
    Ok(parts.join("/"))
}

fn normalize_git_path(value: &str) -> Option<String> {
    let normalized = value.trim().replace('\\', "/");
    (!normalized.is_empty()).then_some(normalized)
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Recommendations {
    paths: BTreeSet<String>,
    packages: BTreeSet<String>,
    rust_or_build: bool,
    workspace_wide: bool,
    progress: bool,
    migration: bool,
    hydration: bool,
    playback: bool,
    architecture: bool,
    skills: bool,
}

impl Recommendations {
    fn from_paths(paths: BTreeSet<String>) -> Self {
        let mut result = Self {
            paths,
            ..Self::default()
        };
        let classified = result.paths.iter().cloned().collect::<Vec<_>>();
        for path in classified {
            result.classify(&path);
        }
        result
    }

    fn classify(&mut self, path: &str) {
        let lower = path.to_ascii_lowercase();
        if let Some(rest) = path.strip_prefix("crates/") {
            if let Some(package) = rest.split('/').next().filter(|value| !value.is_empty()) {
                self.packages.insert(package.to_owned());
                self.rust_or_build |= is_rust_or_manifest(path);
                self.architecture |= path.ends_with("Cargo.toml") || package == "ersatzrs-app";
                self.hydration |= package == "ersatzrs-web"
                    && (lower.contains("/src/admin/")
                        || lower.ends_with(".css")
                        || lower.ends_with("cargo.toml"));
                self.playback |= matches!(
                    package,
                    "ersatzrs-ffmpeg" | "ersatzrs-scheduler" | "ersatzrs-scanner"
                ) || lower.contains("hls")
                    || lower.contains("iptv")
                    || lower.contains("playout")
                    || lower.contains("stream");
            }
        } else if path.starts_with("xtask/") {
            self.packages.insert("xtask".to_owned());
            self.rust_or_build |= is_rust_or_manifest(path);
        }

        if path.starts_with("migrations/") || path.starts_with(".sqlx/") {
            self.packages.insert("ersatzrs-infra".to_owned());
            self.migration = true;
        }
        self.progress |= path.starts_with("docs/progress/")
            || path.starts_with("docs/adr/")
            || path.starts_with("plans/")
            || matches!(path, "STATUS.md" | "CHANGELOG.md");
        self.skills |= path.starts_with("skills/")
            || path.starts_with(".codex/skills/")
            || path.starts_with(".claude/skills/");

        if matches!(
            path,
            "Cargo.toml" | "Cargo.lock" | "rust-toolchain.toml" | "run.sh" | "run.ps1" | "run.cmd"
        ) || path.starts_with(".cargo/")
        {
            self.workspace_wide = true;
            self.rust_or_build = true;
        }
        self.architecture |= path == "ARCHITECTURE.md";
    }

    fn render(&self, windows: bool) -> String {
        let wrapper = if windows { ".\\run.cmd" } else { "./run.sh" };
        let mut output = String::new();
        output.push_str(&format!(
            "affected-checks: {} changed path(s)\n",
            self.paths.len()
        ));
        if self.paths.is_empty() {
            output.push_str(
                "affected-checks: no checks recommended; pass paths or change the working tree\n",
            );
            return output;
        }
        for path in &self.paths {
            output.push_str(&format!("  {path}\n"));
        }

        output.push_str("\nFocused loop (run while iterating):\n");
        if self.workspace_wide {
            output.push_str(&format!("  {wrapper} check --workspace\n"));
        } else if !self.packages.is_empty() {
            let selectors = package_selectors(&self.packages);
            output.push_str(&format!("  {wrapper} check {selectors}\n"));
            output.push_str(&format!("  {wrapper} test {selectors} [test-filter]\n"));
        } else {
            output.push_str("  No Rust compile selected (documentation/contract-only diff).\n");
        }
        if self.migration {
            output.push_str(&format!("  {wrapper} test -p ersatzrs-infra migration\n"));
        }
        if self.hydration {
            output.push_str("  cargo xtask hydration-lint -p ersatzrs-web\n");
            output.push_str("  cargo xtask parity-check web\n");
        }
        if self.architecture {
            output.push_str("  cargo xtask architecture-lint\n");
        }
        if self.progress {
            output.push_str("  cargo xtask progress check\n");
        }
        if self.skills {
            output.push_str(
                "  Verify each .codex/.claude skill pointer matches its canonical skills/ file.\n",
            );
        }

        if self.rust_or_build || self.migration {
            output.push_str("\nCloseout (once, after the focused loop is green):\n");
            output.push_str("  cargo fmt --all -- --check\n");
            output.push_str("  cargo clippy --workspace --all-targets -- -D warnings\n");
            output.push_str(&format!("  {wrapper} test-closeout\n"));
            if self.hydration {
                output.push_str("  (cd crates/ersatzrs-web && cargo leptos build)\n");
                output.push_str("  cargo xtask parity-check web\n");
            }
            if self.playback {
                output.push_str("  cargo xtask smoke\n");
            }
        }
        output
    }
}

fn is_rust_or_manifest(path: &str) -> bool {
    path.ends_with(".rs") || path.ends_with("Cargo.toml") || path.ends_with("build.rs")
}

fn package_selectors(packages: &BTreeSet<String>) -> String {
    packages
        .iter()
        .map(|package| format!("-p {package}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn print_help() {
    eprintln!("affected-checks — recommend focused checks for changed files");
    eprintln!();
    eprintln!("USAGE:");
    eprintln!("    cargo xtask affected-checks [--base <REV>] [PATH]...");
    eprintln!();
    eprintln!("With no PATH, inspect tracked and untracked working-tree changes.");
    eprintln!("--base also includes committed changes from REV...HEAD.");
    eprintln!("The command prints recommendations and never executes them.");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_base_and_explicit_paths() {
        let args = vec![
            "--base".to_owned(),
            "origin/main".to_owned(),
            "xtask/src/main.rs".to_owned(),
        ];
        let ParsedOptions::Run(options) = Options::parse(&args).expect("valid options") else {
            panic!("expected run options");
        };
        assert_eq!(options.base.as_deref(), Some("origin/main"));
        assert_eq!(options.paths, ["xtask/src/main.rs"]);
    }

    #[test]
    fn recommends_one_windows_command_graph_for_xtask_and_progress() {
        let recommendations = Recommendations::from_paths(BTreeSet::from([
            "docs/progress/phase.toml".to_owned(),
            "xtask/src/main.rs".to_owned(),
        ]));
        let rendered = recommendations.render(true);
        assert!(rendered.contains(".\\run.cmd check -p xtask"));
        assert!(rendered.contains(".\\run.cmd test -p xtask [test-filter]"));
        assert!(rendered.contains("cargo xtask progress check"));
        assert!(!rendered.contains("cargo xtask smoke"));
    }

    #[test]
    fn classifies_migration_hydration_and_playback_contracts() {
        let recommendations = Recommendations::from_paths(BTreeSet::from([
            "crates/ersatzrs-web/src/admin/media.rs".to_owned(),
            "crates/ersatzrs-ffmpeg/src/pipeline.rs".to_owned(),
            "migrations/202608230001_example.sql".to_owned(),
        ]));
        let rendered = recommendations.render(false);
        assert!(rendered.contains("-p ersatzrs-ffmpeg"));
        assert!(rendered.contains("-p ersatzrs-infra"));
        assert!(rendered.contains("-p ersatzrs-web"));
        assert!(rendered.contains("hydration-lint"));
        assert!(rendered.contains("test -p ersatzrs-infra migration"));
        assert!(!rendered.contains("--features mysql"));
        assert!(rendered.contains("cargo xtask smoke"));
    }

    #[test]
    fn canonical_skills_have_valid_frontmatter_and_exact_mirror_pointers() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask has workspace parent");
        let mut skill_count = 0;
        for entry in fs::read_dir(root.join("skills")).expect("read skills") {
            let entry = entry.expect("skill entry");
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("md") {
                continue;
            }
            skill_count += 1;
            let name = path
                .file_stem()
                .and_then(|value| value.to_str())
                .expect("UTF-8 skill name");
            let contents = fs::read_to_string(&path).expect("read canonical skill");
            assert!(contents.starts_with("---\nname: ") || contents.starts_with("---\r\nname: "));
            assert!(contents.contains(&format!("name: {name}")));
            assert!(contents.contains("\ndescription: ") || contents.contains("\r\ndescription: "));

            let expected = format!("../../skills/{name}.md");
            for mirror in [".codex/skills", ".claude/skills"] {
                let pointer = fs::read_to_string(root.join(mirror).join(format!("{name}.md")))
                    .unwrap_or_else(|error| panic!("missing {mirror}/{name}.md: {error}"));
                assert_eq!(pointer.trim(), expected);
            }
        }
        assert!(skill_count >= 17, "expected canonical skill catalog");
    }
}
