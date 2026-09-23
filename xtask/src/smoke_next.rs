//! Decoded evidence for the disposable Next Hybrid smoke fixture.
use super::*;

fn ffmpeg() -> Command {
    let configured = configured_ffmpeg_path().or_else(|| find_on_path("ffmpeg"));
    let mut command = Command::new(configured.as_deref().unwrap_or("ffmpeg"));
    apply_ffmpeg_library_path(&mut command, configured.as_deref());
    command.args(["-hide_banner", "-v", "error", "-nostdin", "-y"]);
    command
}

// Output goes to files, avoiding pipe backpressure while the process is polled.
fn bounded(command: &mut Command, deadline: Deadline) -> Result<()> {
    let mut child = command.stdin(Stdio::null()).stdout(Stdio::null()).spawn()?;
    loop {
        if let Some(status) = child.try_wait()? {
            if !status.success() {
                bail!("Next smoke media command failed with {status}");
            }
            return Ok(());
        }
        if deadline.expired() {
            let _ = child.kill();
            let _ = child.wait();
            bail!("Next smoke media command exceeded its deadline");
        }
        thread::sleep(Duration::from_millis(50));
    }
}

pub(super) fn generate(path: &Path, seconds: i64) -> Result<()> {
    bounded(
        ffmpeg()
            .args([
                "-f",
                "lavfi",
                "-i",
                "color=c=blue:s=320x180:r=30",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:sample_rate=48000",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=880:sample_rate=48000",
                "-map",
                "0:v",
                "-map",
                "1:a",
                "-map",
                "2:a",
                "-t",
                &seconds.to_string(),
                "-c:v",
                "mpeg4",
                "-threads",
                "2",
                "-c:a",
                "aac",
                "-ac",
                "2",
                "-metadata:s:a:0",
                "language=eng",
                "-metadata:s:a:1",
                "language=nld",
            ])
            .arg(path),
        Deadline::from_now(Duration::from_secs(30)),
    )?;
    fs::write(
        path.with_extension("nl.srt"),
        "1\n00:00:00,000 --> 00:10:00,000\nNederlandse ondertiteling\n",
    )?;
    Ok(())
}

pub(super) fn seed(db: &Path, media: &Path) -> Result<()> {
    let conn = Connection::open(db)?;
    let subtitle = media.with_extension("nl.srt").canonicalize()?;
    conn.execute(r#"INSERT INTO "MediaStream" ("Index", "Codec", "MediaStreamKind", "Language", "Channels", "Default", "Forced", "MediaVersionId") VALUES (2, 'aac', 2, 'nld', 2, 0, 0, 1)"#, [])?;
    conn.execute(r#"INSERT INTO "Subtitle" ("MovieMetadataId", "SubtitleKind", "Title", "StreamIndex", "Codec", "Default", "Forced", "SDH", "Language", "IsExtracted", "Path", "DateAdded", "DateUpdated") SELECT "Id", 1, 'Dutch', 9, 'subrip', 1, 0, 0, 'nld', 0, ?, "DateAdded", "DateUpdated" FROM "MovieMetadata" WHERE "MovieId"=1"#, [subtitle.to_string_lossy().as_ref()])?;
    conn.execute(r#"UPDATE "Channel" SET "PreferredAudioLanguageCode"='nld', "PreferredSubtitleLanguageCode"='nld', "SubtitleMode"=2 WHERE "Number"=?"#, [DEFAULT_CHANNEL])?;
    Ok(())
}

pub(super) fn verify(channel: &str, root: &Path, deadline: Deadline, convert: bool) -> Result<()> {
    let capture = root.join("next-wrapper.ts");
    let url = format!(
        "http://{}/iptv/channel/{channel}.ts?mode=ts",
        listen_addr()?
    );
    bounded(
        ffmpeg()
            .args([
                "-rw_timeout",
                "15000000",
                "-i",
                &url,
                "-t",
                "3",
                "-map",
                "0",
                "-c",
                "copy",
                "-f",
                "mpegts",
            ])
            .arg(&capture),
        deadline,
    )?;
    let audio = root.join("next-wrapper.pcm");
    bounded(
        ffmpeg()
            .arg("-i")
            .arg(&capture)
            .args([
                "-map", "0:a:0", "-t", "1", "-ac", "1", "-ar", "8000", "-f", "s16le",
            ])
            .arg(&audio),
        deadline,
    )?;
    let bytes = fs::read(audio)?;
    let samples: Vec<_> = bytes
        .chunks_exact(2)
        .map(|s| i16::from_le_bytes([s[0], s[1]]))
        .collect();
    if samples.len() < 7000 {
        bail!("Next TS audio capture is too short");
    }
    let crossings = samples.windows(2).filter(|s| s[0] <= 0 && s[1] > 0).count();
    let frequency =
        f64::from(u32::try_from(crossings)?) * 8000.0 / f64::from(u32::try_from(samples.len())?);
    if !(820.0..940.0).contains(&frequency) {
        bail!("Next TS lost selected Dutch 880 Hz audio: {frequency} Hz");
    }
    let frame = root.join("next-wrapper.rgb");
    bounded(
        ffmpeg()
            .arg("-i")
            .arg(&capture)
            .args([
                "-ss",
                "0.5",
                "-map",
                "0:v:0",
                "-frames:v",
                "1",
                "-vf",
                "crop=320:48:0:132",
                "-pix_fmt",
                "rgb24",
                "-f",
                "rawvideo",
            ])
            .arg(&frame),
        deadline,
    )?;
    let pixels = fs::read(frame)?;
    if pixels.len() != 320 * 48 * 3 {
        bail!("Next TS decoded frame has unexpected dimensions");
    }
    let white = pixels
        .chunks_exact(3)
        .filter(|p| p.iter().all(|v| *v > 180))
        .count();
    if (!convert && white <= 20) || (convert && white != 0) {
        bail!("Next TS subtitle mode mismatch: {white} white pixels, Convert={convert}");
    }
    if convert {
        let blue = pixels
            .chunks_exact(3)
            .filter(|p| p[0] < 40 && p[1] < 40 && p[2] > 180)
            .count();
        if blue * 100 < (pixels.len() / 3) * 99 {
            bail!("Next Convert TS frame is not the expected unburned blue fixture");
        }
        verify_converted_cue(channel, deadline)?;
    }
    inspect_streams(&capture, root, deadline)?;
    // The wrapper client has disconnected. The same channel must still serve HLS.
    let path = format!("/iptv/session/{channel}/hls.m3u8");
    let playlist = http_get_until_body(&path, usize::MAX, deadline.remaining()?)?;
    playlist.ensure_success_with_body(&path)?;
    fetch_first_hls_segment(
        channel,
        &String::from_utf8_lossy(&playlist.body),
        deadline.remaining()?,
        true,
    )?;
    println!("smoke evidence: Next Hybrid TS decoded Dutch {frequency:.1} Hz audio; subtitle white pixels={white}, Convert={convert}; HLS remains playable after wrapper disconnect");
    Ok(())
}

fn inspect_streams(capture: &Path, root: &Path, deadline: Deadline) -> Result<()> {
    let configured = configured_ffmpeg_path();
    #[cfg(target_os = "windows")]
    let probe_name = "ffprobe.exe";
    #[cfg(not(target_os = "windows"))]
    let probe_name = "ffprobe";
    let probe = configured
        .as_ref()
        .and_then(|p| Path::new(p).parent())
        .map(|p| p.join(probe_name));
    let probe = probe
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from("ffprobe"));
    let output = root.join("next-wrapper-streams.json");
    let mut command = Command::new(probe);
    apply_ffmpeg_library_path(&mut command, configured.as_deref());
    // Use ffprobe's output-file option because bounded() deliberately owns stdout.
    command
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_type,codec_name",
            "-of",
            "json",
            "-o",
        ])
        .arg(&output)
        .arg(capture);
    bounded(&mut command, deadline)?;
    let report: serde_json::Value = serde_json::from_slice(&fs::read(output)?)?;
    let streams = report["streams"]
        .as_array()
        .context("ffprobe omitted streams")?;
    if !streams.iter().any(|s| s["codec_type"] == "video")
        || !streams.iter().any(|s| s["codec_type"] == "audio")
    {
        bail!("Next wrapper omitted audio/video streams");
    }
    println!(
        "smoke evidence: Next Hybrid TS stream types/codecs: {}",
        report["streams"]
    );
    Ok(())
}

fn owned_subtitle_path(channel: &str, uri: &str) -> Result<String> {
    let origin = format!("http://{}", listen_addr()?);
    let uri = uri.strip_prefix(&origin).unwrap_or(uri);
    let prefix = format!("/iptv/session/{channel}/");
    let path = if uri.starts_with('/') {
        uri.to_owned()
    } else {
        format!("{prefix}{uri}")
    };
    let file = path
        .strip_prefix(&prefix)
        .context("subtitle URI escaped the smoke channel")?;
    let file = file.split('?').next().unwrap_or(file);
    if file.is_empty()
        || file.contains("..")
        || !file
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-.".contains(&c))
        || path.bytes().any(|c| c.is_ascii_control())
    {
        bail!("unsupported smoke subtitle URI");
    }
    Ok(path)
}

fn verify_converted_cue(channel: &str, deadline: Deadline) -> Result<()> {
    let master_path = format!("/iptv/channel/{channel}.m3u8?mode=segmenter");
    let master = http_get_until_body(&master_path, usize::MAX, deadline.remaining()?)?;
    master.ensure_success_with_body("Next Convert master")?;
    let body = String::from_utf8(master.body)?;
    let uri = body
        .lines()
        .find(|line| line.starts_with("#EXT-X-MEDIA:TYPE=SUBTITLES,"))
        .and_then(|line| line.split("URI=\"").nth(1))
        .and_then(|tail| tail.split('"').next())
        .context("Next Convert master omitted the subtitle rendition")?;
    let path = owned_subtitle_path(channel, uri)?;
    let playlist = http_get_until_body(&path, usize::MAX, deadline.remaining()?)?;
    playlist.ensure_success_with_body("Next Convert subtitle playlist")?;
    let playlist = String::from_utf8(playlist.body)?;
    if !playlist.contains("#EXTM3U") {
        bail!("Next Convert subtitle playlist is invalid");
    }
    for uri in playlist
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .take(8)
    {
        let path = owned_subtitle_path(channel, uri)?;
        let cue = http_get_until_body(&path, usize::MAX, deadline.remaining()?)?;
        cue.ensure_success_with_body("Next Convert WebVTT segment")?;
        let cue = String::from_utf8(cue.body)?;
        if cue.starts_with("WEBVTT") && cue.contains("Nederlandse ondertiteling") {
            println!(
                "smoke evidence: Next Convert master rendition serves selected Dutch WebVTT cue"
            );
            return Ok(());
        }
    }
    bail!("Next Convert rendition did not contain the selected Dutch WebVTT cue");
}
