use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use thiserror::Error;
use tokio::process::Command;

pub const DEFAULT_SAMPLE_DURATION_SECONDS: u64 = 20;
pub const MAX_SAMPLE_DURATION_SECONDS: u64 = 120;
pub const MAX_SAMPLE_BYTES: u64 = 512 * 1024 * 1024;
const TSP_PROGRAM: &str = "tsp";
const TSBITRATE_PROGRAM: &str = "tsbitrate";
const CONTINUITY_PREFIX: &str = "ERSATZRS_CONTINUITY";
const PROCESS_TIMEOUT_SECONDS: u64 = 120;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TsduckAvailability {
    pub available: bool,
    pub version: Option<String>,
    pub error: Option<String>,
}

impl Default for TsduckAvailability {
    fn default() -> Self {
        Self::unavailable("not checked")
    }
}

impl TsduckAvailability {
    #[must_use]
    pub fn unavailable(error: impl Into<String>) -> Self {
        Self {
            available: false,
            version: None,
            error: Some(redact_sensitive_text(&error.into())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TsduckCommandSpec {
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TsduckAnalysis {
    pub tsp_command: TsduckCommandSpec,
    pub tsbitrate_command: TsduckCommandSpec,
    pub status: TsduckAnalysisStatus,
    pub summary: String,
    pub analysis_json_excerpt: String,
    pub continuity_findings: Vec<String>,
    pub pcr_findings: Vec<String>,
    pub services: Vec<String>,
    pub pids: Vec<String>,
    pub bitrate_bps: Option<u64>,
    pub stdout_excerpt: String,
    pub stderr_excerpt: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TsduckAnalysisStatus {
    Completed,
    Failed,
    NotAvailable,
}

#[derive(Debug, Error)]
pub enum TsduckError {
    #[error("TSDuck command timed out")]
    Timeout,
    #[error("TSDuck command failed: {0}")]
    Process(String),
    #[error("TSDuck analysis file failed: {0}")]
    File(String),
}

pub async fn detect_tsduck() -> TsduckAvailability {
    detect_tsduck_with_program(TSP_PROGRAM).await
}

pub async fn detect_tsduck_with_program(program: &str) -> TsduckAvailability {
    let output = tokio::time::timeout(
        Duration::from_secs(5),
        Command::new(program)
            .arg("--version")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .output(),
    )
    .await;

    match output {
        Ok(Ok(output)) if output.status.success() => {
            let version = first_non_empty_line(&String::from_utf8_lossy(&output.stdout))
                .or_else(|| first_non_empty_line(&String::from_utf8_lossy(&output.stderr)));
            TsduckAvailability {
                available: true,
                version,
                error: None,
            }
        }
        Ok(Ok(output)) => TsduckAvailability::unavailable(format!(
            "{} --version exited with {}: {}",
            program,
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )),
        Ok(Err(error)) => TsduckAvailability::unavailable(error.to_string()),
        Err(_) => TsduckAvailability::unavailable(format!("{program} --version timed out")),
    }
}

#[must_use]
pub fn normalize_sample_duration_seconds(value: u64) -> u64 {
    if value == 0 {
        DEFAULT_SAMPLE_DURATION_SECONDS
    } else {
        value.min(MAX_SAMPLE_DURATION_SECONDS)
    }
}

#[must_use]
pub fn build_tsp_analysis_command(
    sample_path: &Path,
    analysis_json_path: &Path,
) -> TsduckCommandSpec {
    TsduckCommandSpec {
        program: TSP_PROGRAM.to_owned(),
        args: vec![
            "--realtime=off".to_owned(),
            "-I".to_owned(),
            "file".to_owned(),
            sample_path.display().to_string(),
            "-P".to_owned(),
            "continuity".to_owned(),
            format!("--json-line={CONTINUITY_PREFIX}"),
            "-P".to_owned(),
            "pcrverify".to_owned(),
            "-P".to_owned(),
            "analyze".to_owned(),
            "--json".to_owned(),
            "--output-file".to_owned(),
            analysis_json_path.display().to_string(),
            "--error-analysis".to_owned(),
            "-O".to_owned(),
            "drop".to_owned(),
        ],
    }
}

#[must_use]
pub fn build_tsbitrate_command(sample_path: &Path) -> TsduckCommandSpec {
    TsduckCommandSpec {
        program: TSBITRATE_PROGRAM.to_owned(),
        args: vec!["--value-only".to_owned(), sample_path.display().to_string()],
    }
}

/// Run the `TSDuck` analysis commands for a captured `MPEG-TS` sample.
///
/// # Errors
///
/// Returns an error when the run directory cannot be created, the primary
/// `tsp` process cannot be spawned, or the primary process times out.
pub async fn analyze_sample(
    sample_path: &Path,
    run_dir: &Path,
) -> Result<TsduckAnalysis, TsduckError> {
    tokio::fs::create_dir_all(run_dir)
        .await
        .map_err(|error| TsduckError::File(error.to_string()))?;
    let analysis_json_path = run_dir.join("analysis.json");
    let tsp_command = build_tsp_analysis_command(sample_path, &analysis_json_path);
    let tsbitrate_command = build_tsbitrate_command(sample_path);

    let tsp = run_command(&tsp_command).await?;
    let analysis_json = tokio::fs::read_to_string(&analysis_json_path)
        .await
        .unwrap_or_default();
    let bitrate = match run_command(&tsbitrate_command).await {
        Ok(output) if output.status.success() => parse_bitrate(&output.stdout),
        _ => None,
    };

    let status = if tsp.status.success() {
        TsduckAnalysisStatus::Completed
    } else {
        TsduckAnalysisStatus::Failed
    };
    let continuity_findings = parse_continuity_findings(&tsp.stdout);
    let pcr_findings = parse_pcr_findings(&tsp.stderr);
    let (services, pids) = parse_analysis_summaries(&analysis_json);
    let summary = analysis_summary(
        status,
        &continuity_findings,
        &pcr_findings,
        &services,
        &pids,
    );

    Ok(TsduckAnalysis {
        tsp_command,
        tsbitrate_command,
        status,
        summary,
        analysis_json_excerpt: bounded_excerpt(&analysis_json, 8_000),
        continuity_findings,
        pcr_findings,
        services,
        pids,
        bitrate_bps: bitrate,
        stdout_excerpt: bounded_excerpt(&redact_sensitive_text(&tsp.stdout), 8_000),
        stderr_excerpt: bounded_excerpt(&redact_sensitive_text(&tsp.stderr), 8_000),
    })
}

#[derive(Debug)]
struct CommandOutput {
    status: std::process::ExitStatus,
    stdout: String,
    stderr: String,
}

async fn run_command(spec: &TsduckCommandSpec) -> Result<CommandOutput, TsduckError> {
    let output = tokio::time::timeout(
        Duration::from_secs(PROCESS_TIMEOUT_SECONDS),
        Command::new(&spec.program)
            .args(&spec.args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| TsduckError::Timeout)?
    .map_err(|error| TsduckError::Process(error.to_string()))?;

    Ok(CommandOutput {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

fn first_non_empty_line(value: &str) -> Option<String> {
    value
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToOwned::to_owned)
}

#[must_use]
pub fn parse_continuity_findings(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let payload = line.strip_prefix(CONTINUITY_PREFIX).unwrap_or(line).trim();
            (!payload.is_empty()
                && (payload.contains("error")
                    || payload.contains("Error")
                    || payload.contains("missing")
                    || payload.contains("discontinuity")))
            .then(|| bounded_excerpt(&redact_sensitive_text(payload), 500))
        })
        .collect()
}

#[must_use]
pub fn parse_pcr_findings(output: &str) -> Vec<String> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            !line.is_empty()
                && (lower.contains("pcr")
                    || lower.contains("pts")
                    || lower.contains("error")
                    || lower.contains("warning"))
        })
        .map(|line| bounded_excerpt(&redact_sensitive_text(line), 500))
        .collect()
}

#[must_use]
pub fn parse_analysis_summaries(json: &str) -> (Vec<String>, Vec<String>) {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return (Vec::new(), Vec::new());
    };
    let mut services = Vec::new();
    let mut pids = Vec::new();
    collect_analysis_value(&value, &mut services, &mut pids);
    services.sort();
    services.dedup();
    pids.sort();
    pids.dedup();
    (
        services.into_iter().take(20).collect(),
        pids.into_iter().take(50).collect(),
    )
}

fn collect_analysis_value(
    value: &serde_json::Value,
    services: &mut Vec<String>,
    pids: &mut Vec<String>,
) {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(pid) = json_number_or_string(map.get("pid").or_else(|| map.get("PID"))) {
                let label = json_string(
                    map.get("description")
                        .or_else(|| map.get("Description"))
                        .or_else(|| map.get("stream_type"))
                        .or_else(|| map.get("StreamType")),
                )
                .unwrap_or_else(|| "PID".to_owned());
                pids.push(format!("{pid}: {label}"));
            }
            if let Some(service_id) = json_number_or_string(
                map.get("service_id")
                    .or_else(|| map.get("ServiceId"))
                    .or_else(|| map.get("serviceId")),
            ) {
                let name = json_string(
                    map.get("name")
                        .or_else(|| map.get("Name"))
                        .or_else(|| map.get("service_name"))
                        .or_else(|| map.get("ServiceName")),
                )
                .unwrap_or_else(|| "unnamed".to_owned());
                services.push(format!("{service_id}: {name}"));
            }
            for child in map.values() {
                collect_analysis_value(child, services, pids);
            }
        }
        serde_json::Value::Array(items) => {
            for child in items {
                collect_analysis_value(child, services, pids);
            }
        }
        _ => {}
    }
}

fn json_number_or_string(value: Option<&serde_json::Value>) -> Option<String> {
    match value? {
        serde_json::Value::Number(number) => Some(number.to_string()),
        serde_json::Value::String(value) if !value.trim().is_empty() => Some(value.clone()),
        _ => None,
    }
}

fn json_string(value: Option<&serde_json::Value>) -> Option<String> {
    match value? {
        serde_json::Value::String(value) if !value.trim().is_empty() => Some(value.clone()),
        _ => None,
    }
}

fn parse_bitrate(output: &str) -> Option<u64> {
    output
        .split_whitespace()
        .find_map(|token| token.trim().parse::<u64>().ok())
}

fn analysis_summary(
    status: TsduckAnalysisStatus,
    continuity: &[String],
    pcr: &[String],
    services: &[String],
    pids: &[String],
) -> String {
    match status {
        TsduckAnalysisStatus::Completed if continuity.is_empty() && pcr.is_empty() => {
            format!(
                "TSDuck completed without continuity/PCR findings; {} services and {} PIDs summarized.",
                services.len(),
                pids.len()
            )
        }
        TsduckAnalysisStatus::Completed => format!(
            "TSDuck completed with {} continuity findings and {} PCR findings.",
            continuity.len(),
            pcr.len()
        ),
        TsduckAnalysisStatus::Failed => "TSDuck analysis exited with a non-zero status.".to_owned(),
        TsduckAnalysisStatus::NotAvailable => "TSDuck is not available.".to_owned(),
    }
}

#[must_use]
pub fn bounded_excerpt(value: &str, max_chars: usize) -> String {
    let value = value.trim();
    if value.chars().count() <= max_chars {
        value.to_owned()
    } else {
        let mut excerpt = value.chars().take(max_chars).collect::<String>();
        excerpt.push_str("\n...");
        excerpt
    }
}

#[must_use]
pub fn redact_sensitive_text(value: &str) -> String {
    value
        .split_whitespace()
        .map(redact_token)
        .collect::<Vec<_>>()
        .join(" ")
}

fn redact_token(token: &str) -> String {
    let mut redacted = token.to_owned();
    for needle in [
        "access_token=",
        "token=",
        "apikey=",
        "api_key=",
        "key=",
        "secret=",
    ] {
        if let Some(index) = redacted.to_ascii_lowercase().find(needle) {
            let value_start = index + needle.len();
            let value_end = redacted[value_start..]
                .find(['&', '"', '\'', ')', ']'])
                .map_or(redacted.len(), |offset| value_start + offset);
            redacted.replace_range(value_start..value_end, "<redacted>");
        }
    }
    redacted
}

#[must_use]
pub fn report_archive_dir(run_id: uuid::Uuid) -> PathBuf {
    crate::file_system_layout::data_path(
        Path::new("troubleshooting")
            .join("mpeg")
            .join(run_id.to_string()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_tsp_analysis_argv_without_shell() {
        let command = build_tsp_analysis_command(
            Path::new("/tmp/sample.ts"),
            Path::new("/tmp/analysis.json"),
        );

        assert_eq!(command.program, "tsp");
        assert_eq!(
            command.args,
            vec![
                "--realtime=off",
                "-I",
                "file",
                "/tmp/sample.ts",
                "-P",
                "continuity",
                "--json-line=ERSATZRS_CONTINUITY",
                "-P",
                "pcrverify",
                "-P",
                "analyze",
                "--json",
                "--output-file",
                "/tmp/analysis.json",
                "--error-analysis",
                "-O",
                "drop",
            ]
        );
    }

    #[test]
    fn parses_analysis_services_and_pids_recursively() {
        let json = r#"{
          "transport_stream": {
            "services": [{"service_id": 7, "name": "Channel 7"}],
            "pids": [{"pid": 256, "description": "H.264 video"}]
          }
        }"#;

        let (services, pids) = parse_analysis_summaries(json);

        assert_eq!(services, vec!["7: Channel 7"]);
        assert_eq!(pids, vec!["256: H.264 video"]);
    }

    #[test]
    fn parses_continuity_and_pcr_findings() {
        let continuity = parse_continuity_findings(
            r#"ERSATZRS_CONTINUITY {"pid":256,"error":"missing packet"}"#,
        );
        let pcr = parse_pcr_findings("PCR repetition error on PID 256");

        assert_eq!(continuity.len(), 1);
        assert_eq!(pcr, vec!["PCR repetition error on PID 256"]);
    }

    #[test]
    fn redacts_sensitive_query_values() {
        let redacted = redact_sensitive_text(
            "http://127.0.0.1/iptv/channel/1.ts?access_token=secret&mode=ts token=abc",
        );

        assert!(redacted.contains("access_token=<redacted>&mode=ts"));
        assert!(redacted.contains("token=<redacted>"));
        assert!(!redacted.contains("secret"));
        assert!(!redacted.contains("abc"));
    }
}
