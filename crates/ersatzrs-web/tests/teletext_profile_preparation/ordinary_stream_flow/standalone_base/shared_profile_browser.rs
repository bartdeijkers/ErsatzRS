//! Verify actual headed-browser captures with the existing complete wire/AV oracles.
use super::*;

#[tokio::test]
#[ignore = "requires actual headed shared-profile captures and pinned FFmpeg9/TSDuck"]
async fn shared_profile_browser_publication_retains_exact_model_and_source_av() {
    let tools = Tools::load().await;
    let root = client_capture::selected("ERSATZRS_TELETEXT_SHARED_PROFILE_CAPTURE");
    let (fixture, _) = installed_profiles_with(&tools, response, 12).await;
    let day = accept_base(&fixture, 1).await;
    let night = accept_base(&fixture, 2).await;
    let source = tokio::fs::read(format!("{root}/shared-source.ts"))
        .await
        .unwrap();
    for (stage, expected) in [
        ("initial", [Some(&day), Some(&day)]),
        ("invalidated", [None, None]),
        ("pending", [None, None]),
        ("first", [Some(&night), None]),
        ("final", [Some(&night), Some(&night)]),
    ] {
        for (index, accepted) in expected.into_iter().enumerate() {
            let bytes = tokio::fs::read(format!("{root}/shared-{stage}-{index}.ts"))
                .await
                .unwrap();
            if let Some(accepted) = accepted {
                super::super::super::editorial_imports::live_updates::live_tcp::assert_uninterrupted_source_clock(
                    &bytes,
                );
                client_capture::verify_transport_with_qualified_authored_pts_gaps(
                    &tools.tsanalyze,
                    &tools.ffprobe,
                    &bytes,
                    0,
                    Some(&PathBuf::from(format!(
                        "{root}/shared-{stage}-{index}.analyze.json"
                    ))),
                    false,
                )
                .await;
                client_capture::verify_av(&tools.ffmpeg, &source, &bytes).await;
                assert_emitted_model(&bytes, accepted.service());
                scoped_restart::export_units(
                    &format!("{root}/wire"),
                    &format!("{stage}-{index}"),
                    &bytes,
                )
                .await;
            } else {
                // Off has no authored clock. Keep the existing strict plain
                // PMT/PCR, absent-PID, TSDuck and source-AV oracle instead.
                super::super::super::editorial_imports::live_updates::live_tcp::assert_plain_source_clock(
                    &bytes,
                );
                verify_plain(&tools, &source, &bytes).await;
            }
        }
    }
    fixture.state.pool.close().await;
}
