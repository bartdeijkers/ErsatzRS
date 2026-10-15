//! Opt-in acquisition proof using a redistributable synthetic AV/teletext file.
//! This does not establish observed pages, SQL publication or playback selection.

use std::path::PathBuf;

use ersatzrs_domain::MediaStreamKind;
use ersatzrs_infra::ffprobe::{
    teletext_probe::{
        TeletextProbeDescriptors, TeletextProbePid, TeletextProbePrograms, TeletextProbeSnapshot,
    },
    FfprobeExtractor, LocalProbeCleanupScope,
};

#[allow(
    clippy::disallowed_methods,
    reason = "Explicit opt-in native test tools and synthetic input, never product configuration"
)]
fn native_inputs() -> (String, PathBuf) {
    let probe = std::env::var("FFPROBE_PATH").expect("set pinned FFprobe 9 executable");
    let fixture = PathBuf::from(
        std::env::var_os("ERSATZRS_TELETEXT_ACQUISITION_FIXTURE")
            .expect("set synthetic AV/200/888 fixture"),
    );
    (probe, fixture)
}

#[tokio::test]
#[ignore = "requires pinned FFprobe 9 and synthetic AV/teletext fixture environment"]
async fn native_secondary_acquisition_preserves_av_and_detects_source_replacement() {
    let (probe, fixture) = native_inputs();
    assert!(fixture.metadata().unwrap().len() <= 8 * 1024 * 1024);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("synthetic.ts");
    std::fs::copy(fixture, &path).unwrap();
    let extractor = FfprobeExtractor::new(probe);
    let scope = LocalProbeCleanupScope::new();
    let ordinary = scope.scope(extractor.extract(&path)).await.unwrap();
    let acquired = scope
        .scope(extractor.extract_local_with_teletext(&path))
        .await
        .unwrap();
    let mut actual = acquired.media;
    actual.version.date_added = ordinary.version.date_added;
    actual.version.date_updated = ordinary.version.date_updated;
    assert_eq!(
        actual, ordinary,
        "auxiliary probe must not rewrite ordinary metadata"
    );
    assert_eq!(
        actual
            .streams
            .iter()
            .map(|stream| (stream.index, stream.kind))
            .collect::<Vec<_>>(),
        vec![
            (0, MediaStreamKind::Video),
            (1, MediaStreamKind::Audio),
            (2, MediaStreamKind::Subtitle)
        ]
    );
    assert_eq!(actual.streams[1].language.as_deref(), Some("eng"));
    assert_eq!(actual.streams[2].language.as_deref(), Some("eng,eng"));
    let acquired = acquired.teletext.expect("native teletext acquisition");
    assert!(acquired.is_current(&path));
    let TeletextProbeSnapshot::Available(streams) = acquired.snapshot() else {
        panic!("synthetic snapshot must be available");
    };
    assert_eq!(streams.len(), 1);
    assert_eq!(streams[0].stream_index, 2);
    assert_eq!(streams[0].pid, TeletextProbePid::Available(0x102));
    let TeletextProbePrograms::Available(programs) = &streams[0].programs else {
        panic!("native program membership must be retained");
    };
    assert_eq!(programs.len(), 1);
    assert_eq!(programs[0].program_number, 1);
    let TeletextProbeDescriptors::Available(descriptors) = &streams[0].descriptors else {
        panic!("native descriptor prefix must be usable");
    };
    assert!(descriptors.warning.is_none());
    assert_eq!(
        descriptors
            .entries
            .iter()
            .map(|entry| (
                entry.display_page.unwrap().get(),
                entry.descriptor_type,
                entry.language.as_str()
            ))
            .collect::<Vec<_>>(),
        vec![(200, 1, "eng"), (888, 2, "eng")]
    );
    let repeated = scope
        .scope(extractor.extract_local_with_teletext(&path))
        .await
        .unwrap();
    assert_eq!(acquired.revision(), repeated.teletext.unwrap().revision());
    let original = path.metadata().unwrap();
    let replacement = directory.path().join("replacement.ts");
    std::fs::copy(&path, &replacement).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&replacement)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(original.modified().unwrap()))
        .unwrap();
    assert_eq!(replacement.metadata().unwrap().len(), original.len());
    assert_eq!(
        replacement.metadata().unwrap().modified().unwrap(),
        original.modified().unwrap()
    );
    std::fs::rename(replacement, &path).unwrap();
    assert!(
        !acquired.is_current(&path),
        "same size/mtime cannot hide replacement identity"
    );
    scope.close();
    scope.wait_cleanup().await.unwrap();
    assert_eq!(scope.cleanup_status(), Ok(true));
}
