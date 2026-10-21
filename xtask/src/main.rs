#[cfg(test)]
#[macro_use]
#[path = "../../tests/support/manifest_dir.rs"]
mod test_manifest;

use std::process::ExitCode;

mod affected_checks;
mod architecture_lint;
mod clippy_ledger;
mod hydration_lint;
mod parity_check;
mod progress;
mod runtime_audit;
mod smoke;
mod target_size;
mod validate_addon_manifest;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(sub) = args.next() else {
        print_help();
        return ExitCode::FAILURE;
    };
    match sub.as_str() {
        "affected-checks" => affected_checks::run(args.collect()),
        "architecture-lint" => architecture_lint::run(args.collect()),
        "clippy-ledger" => clippy_ledger::run(args.collect()),
        "hydration-lint" => hydration_lint::run(args.collect()),
        "parity-check" => parity_check::run(args.collect()),
        "progress" => progress::run(args.collect()),
        "runtime-audit" => runtime_audit::run(args.collect()),
        "smoke" => smoke::run(args.collect()),
        "target-size" => {
            target_size::warn_if_oversized();
            ExitCode::SUCCESS
        }
        "validate-addon-manifest" => validate_addon_manifest::run(args.collect()),
        "help" | "--help" | "-h" => {
            print_help();
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("xtask: unknown subcommand `{other}`");
            print_help();
            ExitCode::FAILURE
        }
    }
}

fn print_help() {
    eprintln!("xtask — workspace task runner");
    eprintln!();
    eprintln!("USAGE:");
    eprintln!("    cargo xtask <SUBCOMMAND>");
    eprintln!();
    eprintln!("SUBCOMMANDS:");
    eprintln!("    affected-checks   Recommend focused checks for changed files");
    eprintln!("    architecture-lint Check crate-boundary and admin-debt guardrails");
    eprintln!("    clippy-ledger     Regenerate the clippy exemption ledger / pick next fix");
    eprintln!("    hydration-lint    Check Leptos islands for hydration anti-patterns");
    eprintln!("    parity-check      Run the fast parity gate ladder");
    eprintln!("    progress          Query/maintain the porting progress index (docs/progress/)");
    eprintln!("    runtime-audit     Audit an ErsatzRS SQLite database read-only");
    eprintln!("    smoke             Boot the web host and probe IPTV/HLS runtime playback");
    eprintln!("    target-size       Warn when target/ exceeds the cargo-clean threshold");
    eprintln!("    validate-addon-manifest Validate add-on contract documents");
}
