//! Fast gate ladder for parity slices.
//!
//! This intentionally stays small and shell-free so Codex agents and humans can
//! run the same command on every supported development platform.

use std::{
    ffi::OsStr,
    path::Path,
    process::{Command, ExitCode},
};

use crate::target_size;

pub fn run(args: Vec<String>) -> ExitCode {
    match Opts::parse(args) {
        Ok(opts) => run_inner(&opts),
        Err(ParseResult::Help) => {
            print_help();
            ExitCode::SUCCESS
        }
        Err(ParseResult::Error(msg)) => {
            eprintln!("parity-check: {msg}");
            print_help();
            ExitCode::FAILURE
        }
    }
}

fn run_inner(opts: &Opts) -> ExitCode {
    target_size::warn_if_oversized();

    let scopes = if opts.scopes.is_empty() {
        "default".to_owned()
    } else {
        opts.scopes.join(", ")
    };
    println!("parity-check: scopes: {scopes}");

    let mut steps = Vec::new();
    if !opts.skip_progress {
        steps.push(Step::new(
            "progress check",
            ".",
            ["cargo", "xtask", "progress", "check"],
        ));
    }
    if !opts.skip_fmt {
        steps.push(Step::new(
            "fmt",
            ".",
            ["cargo", "fmt", "--all", "--", "--check"],
        ));
    }
    if !opts.skip_hydration {
        steps.push(Step::new(
            "hydration lint",
            ".",
            ["cargo", "xtask", "hydration-lint", "-p", "ersatzrs-web"],
        ));
    }
    if !opts.skip_tests {
        steps.push(Step::new(
            "focused web tests",
            ".",
            ["cargo", "test", "-p", "ersatzrs-web"],
        ));
    }
    if !opts.skip_clippy {
        steps.push(Step::new(
            "focused web clippy",
            ".",
            [
                "cargo",
                "clippy",
                "-p",
                "ersatzrs-web",
                "--all-targets",
                "--",
                "-D",
                "warnings",
            ],
        ));
    }
    if opts.leptos_build {
        steps.push(Step::new(
            "leptos build",
            "crates/ersatzrs-web",
            ["cargo", "leptos", "build"],
        ));
    }

    if steps.is_empty() {
        eprintln!("parity-check: all checks were skipped");
        return ExitCode::FAILURE;
    }

    for step in steps {
        if let Err(code) = step.run() {
            return code;
        }
    }

    println!("parity-check: all selected checks passed");
    ExitCode::SUCCESS
}

struct Step {
    name: &'static str,
    cwd: &'static str,
    command: Vec<&'static str>,
}

impl Step {
    fn new<const N: usize>(
        name: &'static str,
        cwd: &'static str,
        command: [&'static str; N],
    ) -> Self {
        Self {
            name,
            cwd,
            command: command.to_vec(),
        }
    }

    fn run(&self) -> Result<(), ExitCode> {
        let Some((program, args)) = self.command.split_first() else {
            eprintln!("parity-check: empty command for {}", self.name);
            return Err(ExitCode::FAILURE);
        };

        println!(
            "parity-check: running {}: {}",
            self.name,
            self.command.join(" ")
        );
        let status = Command::new(program)
            .args(args.iter().map(OsStr::new))
            .current_dir(Path::new(self.cwd))
            .status();

        match status {
            Ok(status) if status.success() => Ok(()),
            Ok(status) => {
                let code = status
                    .code()
                    .and_then(|code| u8::try_from(code).ok())
                    .map_or(ExitCode::FAILURE, ExitCode::from);
                eprintln!("parity-check: {} failed with {status}", self.name);
                Err(code)
            }
            Err(err) => {
                eprintln!("parity-check: failed to run {}: {err}", self.name);
                Err(ExitCode::FAILURE)
            }
        }
    }
}

#[derive(Default)]
struct Opts {
    scopes: Vec<String>,
    leptos_build: bool,
    skip_progress: bool,
    skip_fmt: bool,
    skip_hydration: bool,
    skip_tests: bool,
    skip_clippy: bool,
}

impl Opts {
    fn parse(args: Vec<String>) -> Result<Self, ParseResult> {
        let mut opts = Self::default();
        for arg in args {
            match arg.as_str() {
                "-h" | "--help" => return Err(ParseResult::Help),
                "--leptos-build" => opts.leptos_build = true,
                "--skip-progress" => opts.skip_progress = true,
                "--skip-fmt" => opts.skip_fmt = true,
                "--skip-hydration" => opts.skip_hydration = true,
                "--skip-tests" => opts.skip_tests = true,
                "--skip-clippy" => opts.skip_clippy = true,
                _ if arg.starts_with('-') => {
                    return Err(ParseResult::Error(format!("unknown flag `{arg}`")));
                }
                _ => opts.scopes.push(arg),
            }
        }
        Ok(opts)
    }
}

enum ParseResult {
    Help,
    Error(String),
}

fn print_help() {
    eprintln!("parity-check — run the fast parity gate ladder");
    eprintln!();
    eprintln!("USAGE:");
    eprintln!("    cargo xtask parity-check [SCOPE]... [FLAGS]");
    eprintln!();
    eprintln!("SCOPES:");
    eprintln!("    Free-form labels for the slice, e.g. web media scanner");
    eprintln!();
    eprintln!("FLAGS:");
    eprintln!("    --leptos-build      Also run cargo leptos build in crates/ersatzrs-web");
    eprintln!("    --skip-progress     Skip cargo xtask progress check");
    eprintln!("    --skip-fmt          Skip cargo fmt --all -- --check");
    eprintln!("    --skip-hydration    Skip cargo xtask hydration-lint -p ersatzrs-web");
    eprintln!("    --skip-tests        Skip cargo test -p ersatzrs-web");
    eprintln!("    --skip-clippy       Skip cargo clippy -p ersatzrs-web --all-targets");
}
