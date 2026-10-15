//! Opt-in exact-page decode component proof with the pinned native decoder.
//! Durable parent ownership/selection and ordinary-route use are separate gates.

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use ersatzrs_core::locking::{EntityLocker, TeletextWorkGuard};
use ersatzrs_domain::{MediaSourceKind, SourceTeletextBindingIdentity, TeletextPageNumber};
use ersatzrs_infra::ffprobe::{
    teletext_decode::{SourceTeletextPageDecodeError, SourceTeletextPageDecodeRequest},
    FfprobeExtractor, LocalProbeCleanupScope,
};
use tokio::time::Instant;

#[allow(
    clippy::disallowed_methods,
    reason = "Explicit opt-in native tools and synthetic test input, never product configuration"
)]
fn inputs() -> (String, PathBuf) {
    (
        std::env::var("FFPROBE_PATH").expect("set pinned FFprobe 9 executable"),
        std::env::var_os("ERSATZRS_TELETEXT_ACQUISITION_FIXTURE")
            .map(PathBuf::from)
            .expect("set synthetic two-audio AV/200/888 fixture"),
    )
}

fn replace_source_preserving_size_and_mtime(path: &Path) {
    let replacement = path.with_extension("replacement.ts");
    std::fs::copy(path, &replacement).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&replacement)
        .unwrap()
        .set_times(
            std::fs::FileTimes::new().set_modified(path.metadata().unwrap().modified().unwrap()),
        )
        .unwrap();
    std::fs::rename(path, path.with_extension("retired.ts")).unwrap();
    std::fs::rename(replacement, path).unwrap();
}

fn request_for(
    revision: ersatzrs_domain::SourceTeletextRevision,
) -> SourceTeletextPageDecodeRequest {
    // IDs label this component request only. No database ownership is claimed.
    SourceTeletextPageDecodeRequest {
        binding: SourceTeletextBindingIdentity::new(1, MediaSourceKind::Local, 2, 3, 4, revision)
            .unwrap(),
        stream_index: 3,
        pid: 0x103,
        program_number: 1,
        page: TeletextPageNumber::new(888).unwrap(),
    }
}

fn assert_exact_source_window(
    page: &ersatzrs_infra::ffprobe::teletext_decode::DecodedLocalTeletextPage,
    path: &Path,
) {
    assert_eq!(page.window().source_packets, 303);
    assert_eq!(page.window().decoder_packets, 303);
    assert_eq!(page.window().skipped_selected_prefix_packets, 0);
    assert!(page.window().ended_at_eof);
    assert_eq!(
        (
            page.window().program_number,
            page.window().pmt_pid,
            page.window().pcr_pid
        ),
        (1, 0x1000, 0x100)
    );
    assert_eq!(
        page.window().decoder_sha256,
        <[u8; 32]>::from(<sha2::Sha256 as sha2::Digest>::digest(
            std::fs::read(path).unwrap()
        ))
    );
}

#[tokio::test]
#[ignore = "requires pinned FFprobe 9 and redistributable two-audio source fixture"]
async fn native_exact_page_decode_requires_actual_bitmaps_and_current_source() {
    let (probe, source) = inputs();
    assert!(source.metadata().unwrap().len() <= 8 * 1024 * 1024);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("synthetic-source.ts");
    std::fs::copy(source, &path).unwrap();
    let extractor = FfprobeExtractor::new(probe);
    let scope = LocalProbeCleanupScope::new();
    let acquired = scope
        .scope(extractor.extract_local_with_teletext(&path))
        .await
        .unwrap();
    let acquisition = acquired.teletext.unwrap();
    assert_eq!(acquired.media.streams[1].language.as_deref(), Some("eng"));
    assert_eq!(acquired.media.streams[2].language.as_deref(), Some("nld"));
    assert_eq!(
        acquired.media.streams[3].codec.as_deref(),
        Some("dvb_teletext")
    );
    let before = acquisition.headers().for_stream(3).unwrap().clone();
    let locker = EntityLocker::new();
    let work = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
    let mut request = request_for(acquisition.revision().clone());
    let decode = |request| {
        scope.scope(extractor.decode_local_teletext_page(
            request,
            path.clone(),
            path.clone(),
            Instant::now() + Duration::from_secs(15),
            work.hold_work(),
        ))
    };
    let caption = decode(request.clone()).await.unwrap();
    assert_exact_source_window(&caption, &path);
    assert_eq!(caption.events().count, 2);
    assert_eq!(caption.events().first_pts_microseconds, Some(1_661_333));
    assert_eq!(caption.events().last_pts_microseconds, Some(2_701_333));
    assert_eq!(caption.request(), &request);
    let input = caption.native_burn_input();
    assert!(input.is_image_based_subtitle);
    assert_eq!(input.streams[0].index, 3);
    assert_eq!(input.streams[0].source_teletext_page, Some(request.page));
    assert!(!format!("{caption:?}").contains("synthetic-source.ts"));
    request.page = TeletextPageNumber::new(200).unwrap();
    let guide = decode(request.clone()).await.unwrap();
    assert!(guide.events().count > 0);
    assert_eq!(guide.events().first_pts_microseconds, None);
    assert_ne!(guide.events(), caption.events());
    request.page = TeletextPageNumber::new(777).unwrap();
    assert_eq!(
        decode(request.clone()).await.unwrap_err(),
        SourceTeletextPageDecodeError::NoBitmap
    );
    request.page = TeletextPageNumber::new(888).unwrap();
    request.pid = 0x104;
    assert_eq!(
        decode(request.clone()).await.unwrap_err(),
        SourceTeletextPageDecodeError::InvalidTransportWindow
    );
    request.pid = 0x103;
    request.program_number = 2;
    assert_eq!(
        decode(request).await.unwrap_err(),
        SourceTeletextPageDecodeError::InvalidTransportWindow
    );
    assert_eq!(
        acquisition.headers().for_stream(3),
        Some(&before),
        "decoding must not promote or rewrite report-only header metadata"
    );

    replace_source_preserving_size_and_mtime(&path);
    assert_eq!(
        caption
            .check_source(Instant::now() + Duration::from_secs(15), work.hold_work())
            .await
            .unwrap_err(),
        SourceTeletextPageDecodeError::NoCleanupScope,
        "postcheck outside its owner scope must never silently accept a replaced source"
    );
    assert_eq!(
        scope
            .scope(caption.check_source(Instant::now() + Duration::from_secs(15), work.hold_work()))
            .await
            .unwrap_err(),
        SourceTeletextPageDecodeError::SourceChanged
    );
    scope.close();
    scope.wait_cleanup().await.unwrap();
    drop(work);
    assert_eq!(locker.active_teletext_work(), 0);
}
