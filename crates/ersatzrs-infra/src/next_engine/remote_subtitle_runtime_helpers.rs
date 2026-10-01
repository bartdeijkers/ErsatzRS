//! Portable sidecar setup and converted-caption capture for remote runtime fixtures.
use super::*;

pub(super) async fn configure_sidecar(
    pool: &crate::db::DbPool,
    root: &Path,
    sidecar: Option<bool>,
    relative: bool,
) {
    if let Some(convert) = sidecar {
        let path = root.join("captions.nl.srt");
        tokio::fs::write(&path, CUES).await.unwrap();
        if relative {
            // The native sidecar base is the stored local manifest, not the
            // remote public URL or the one-shot producer transport.
            sqlx::query(r#"UPDATE "MediaFile" SET "Path"=? WHERE "Id"=1"#)
                .bind(root.join("manifest.yml").to_str().unwrap())
                .execute(pool)
                .await
                .unwrap();
        }

        sqlx::query(r#"INSERT INTO "Subtitle" ("RemoteStreamMetadataId","SubtitleKind","Title","StreamIndex","Codec","Default","Forced","SDH","Language","IsExtracted","Path","DateAdded","DateUpdated") VALUES (30,1,'Dutch',0,'subrip',1,0,0,'nld',0,?,'2026-09-23','2026-09-23')"#)
            .bind(if relative {"captions.nl.srt"} else {path.to_str().unwrap()}).execute(pool).await.unwrap();
        sqlx::query(
            r#"UPDATE "Channel" SET "SubtitleMode"=2,"NextEngineTextSubtitleMode"=? WHERE "Id"=1"#,
        )
        .bind(i32::from(convert))
        .execute(pool)
        .await
        .unwrap();
    }
}

pub(super) async fn capture_vtt(
    handle: &dyn ersatzrs_core::hls::HlsSessionHandle,
    output: &Path,
) -> Result<(String, f64), String> {
    let playlist = handle.trim_playlist().await.ok_or("missing playlist")?;
    let segment = playlist
        .lines()
        .find(|line| {
            !line.starts_with('#')
                && Path::new(line)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("ts"))
        })
        .ok_or("missing segment")?;
    let duration = playlist
        .lines()
        .find_map(|line| line.strip_prefix("#EXTINF:"))
        .ok_or("missing duration")?
        .trim_end_matches(',')
        .parse::<f64>()
        .map_err(|error| error.to_string())?;
    timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(body) =
                tokio::fs::read_to_string(output.join(segment).with_extension("vtt")).await
            {
                return (body, duration);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .map_err(|_| "converted VTT timed out".into())
}
