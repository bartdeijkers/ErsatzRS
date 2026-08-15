use std::env;
use std::io::{self, Write as _};
use std::process::ExitCode;

use ersatzrs_addon_contract::{
    AddonCheckResult, AddonCheckStatus, AddonMediaListContentKind, AddonMediaListItemAvailability,
    AddonMediaListItemKind, AddonMediaListRecord,
};
use serde::Serialize;

fn emit(output: &mut impl io::Write, value: &impl Serialize) -> io::Result<()> {
    serde_json::to_writer(&mut *output, value)?;
    output.write_all(b"\n")
}

fn run(output: &mut impl io::Write) -> io::Result<()> {
    match env::args().nth(1).as_deref() {
        Some("check") => emit(
            output,
            &AddonCheckResult {
                status: AddonCheckStatus::Ready,
                code: "ready".to_owned(),
                message: "Reference add-on is ready.".to_owned(),
            },
        ),
        Some("list") => {
            emit(
                output,
                &AddonMediaListRecord::list("reference/list", "Reference list"),
            )?;
            emit(
                output,
                &AddonMediaListRecord::Item {
                    provider_id: "movie:1".to_owned(),
                    rank: 0,
                    display_title: "Reference movie (2026)".to_owned(),
                    title: "Reference movie".to_owned(),
                    year: Some(2026),
                    season: None,
                    episode: None,
                    kind: AddonMediaListItemKind::Movie,
                    guids: vec!["tmdb://1".to_owned()],
                    source_url: None,
                    availability: AddonMediaListItemAvailability::Available,
                    availability_reason: None,
                    content_kind: AddonMediaListContentKind::Movie,
                },
            )
        }
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "expected check or list operation",
        )),
    }
}

fn main() -> ExitCode {
    let stdout = io::stdout();
    match run(&mut stdout.lock()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = writeln!(io::stderr().lock(), "{error}");
            ExitCode::FAILURE
        }
    }
}
