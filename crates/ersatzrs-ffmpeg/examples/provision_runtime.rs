//! Provision the normal managed runtime for CI and append its paths to an env file.
//! Usage: `cargo run -p ersatzrs-ffmpeg --example provision_runtime -- ROOT ENV_FILE`

use std::{env, error::Error, fs::OpenOptions, io::Write, path::PathBuf};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args_os().skip(1);
    let root = PathBuf::from(args.next().ok_or("missing runtime root")?);
    let output = PathBuf::from(args.next().ok_or("missing environment output file")?);
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    let config = ersatzrs_ffmpeg::FfmpegRuntimeManagerConfig::from_env(root);
    let report = ersatzrs_ffmpeg::resolve_or_acquire_ffmpeg_runtime(&config, None, None).await?;
    if report.has_errors() {
        return Err("FFmpeg runtime capability validation failed".into());
    }
    let mut file = OpenOptions::new().create(true).append(true).open(output)?;
    for (name, value) in [
        ("ERSATZRS_NEXT_TEST_FFMPEG", &report.ffmpeg.path),
        ("ERSATZRS_NEXT_TEST_FFPROBE", &report.ffprobe.path),
        ("FFMPEG_PATH", &report.ffmpeg.path),
        ("FFPROBE_PATH", &report.ffprobe.path),
    ] {
        if value.contains(['\r', '\n']) {
            return Err("runtime path cannot be represented in an environment file".into());
        }
        writeln!(file, "{name}={value}")?;
    }
    println!("FFmpeg version: {}", report.ffmpeg.version);
    println!("FFmpeg SHA-256: {:?}", report.ffmpeg.sha256);
    println!("ffprobe SHA-256: {:?}", report.ffprobe.sha256);
    Ok(())
}
