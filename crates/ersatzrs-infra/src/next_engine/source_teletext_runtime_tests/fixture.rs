use super::*;
use crate::{
    ffprobe::{
        teletext_probe::{
            TeletextProbeDescriptors, TeletextProbePid, TeletextProbePrograms,
            TeletextProbeSnapshot,
        },
        FfprobeExtractor, LocalProbeCleanupScope,
    },
    teletext_source::{load_caption_candidates, SourceTeletextParents},
};
use ersatzrs_core::repositories::{ManagedLibraryOperationRepository as _, ManagedLibraryScanSpec};
use ersatzrs_domain::{
    ManagedLibraryOperationKind, MediaSourceKind, MediaStreamKind, SourceTeletextBinding,
    SourceTeletextBindingIdentity, SourceTeletextDescriptor, SourceTeletextDescriptors,
    SourceTeletextObservation, SourceTeletextObservedPages, SourceTeletextPid,
    SourceTeletextProgram, SourceTeletextPrograms, SourceTeletextProvenance,
};

#[allow(
    clippy::disallowed_methods,
    reason = "Explicit opt-in disposable native proof root"
)]
pub(super) fn proof_root() -> PathBuf {
    let root = PathBuf::from(
        std::env::var_os("ERSATZRS_NEXT_TELETEXT_PROOF_DIR").expect("set a fresh proof directory"),
    );
    std::fs::create_dir(&root).expect("proof directory must not already exist");
    root
}

pub(super) async fn source(ffmpeg: &Path, root: &Path) -> PathBuf {
    let clip = source_inner(ffmpeg, root, None).await;
    assert!(tokio::fs::metadata(&clip).await.unwrap().len() < 32_768 * 188);
    clip
}

pub(super) async fn source_with_mux_rate(ffmpeg: &Path, root: &Path, mux_rate: u32) -> PathBuf {
    let clip = source_inner(ffmpeg, root, Some(mux_rate)).await;
    assert!(tokio::fs::metadata(&clip).await.unwrap().len() > 32_768 * 188);
    clip
}

async fn source_inner(ffmpeg: &Path, root: &Path, mux_rate: Option<u32>) -> PathBuf {
    let raw = root.join("two-captions.ts");
    let mut python = Command::new("python3");
    python
        .arg(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../scripts/calibration/generate-teletext-fixture.py"),
        )
        .arg(&raw)
        .args(["--scenario", "two-captions"]);
    tool(python).await;
    let video = root.join("source.mkv");
    let mut generate = Command::new(ffmpeg);
    generate
        .args([
            "-v",
            "error",
            "-nostdin",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x180:rate=25",
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
            "20",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-threads",
            "2",
            "-g",
            "25",
            "-sc_threshold",
            "0",
            "-c:a",
            "aac",
            "-b:a",
            "96k",
            "-metadata:s:a:0",
            "language=eng",
            "-metadata:s:a:1",
            "language=nld",
        ])
        .arg(&video);
    tool(generate).await;
    let clip = root.join("caption-source.ts");
    let mut mux = Command::new(ffmpeg);
    mux.args(["-v", "error", "-nostdin", "-i"])
        .arg(video)
        .args(["-f", "mpegts", "-i"])
        .arg(raw)
        .args([
            "-map",
            "0:v:0",
            "-map",
            "0:a:0",
            "-map",
            "1:s:0",
            "-map",
            "0:a:1",
            "-t",
            "20",
            "-c",
            "copy",
            "-pcr_period",
            "20",
            "-mpegts_start_pid",
            "256",
            "-mpegts_pmt_start_pid",
            "4096",
        ]);
    if let Some(mux_rate) = mux_rate {
        mux.args(["-muxrate", &mux_rate.to_string()]);
    }
    mux.args(["-f", "mpegts"]).arg(&clip);
    tool(mux).await;
    clip
}

pub(super) async fn independent_pages(ffmpeg: &Path, clip: &Path, root: &Path) {
    for (page, selected, rejected) in [
        ("888", "ENGLISH PAGE 888", "NEDERLANDSE PAGINA 889"),
        ("889", "NEDERLANDSE PAGINA 889", "ENGLISH PAGE 888"),
    ] {
        let bytes = decode_page(ffmpeg, clip, page).await;
        let text = std::str::from_utf8(&bytes).unwrap();
        assert!(
            text.contains(selected),
            "independent page {page} must decode its literal"
        );
        assert!(!text.contains(rejected));
        tokio::fs::write(root.join(format!("independent-{page}.srt")), bytes)
            .await
            .unwrap();
    }
}

async fn decode_page(ffmpeg: &Path, clip: &Path, page: &str) -> Vec<u8> {
    let mut decode = Command::new(ffmpeg);
    decode
        .args([
            "-v",
            "error",
            "-nostdin",
            "-txt_page:2",
            page,
            "-txt_format:2",
            "text",
            "-fix_sub_duration",
            "-i",
        ])
        .arg(clip)
        .args(["-map", "0:2", "-c:s", "srt", "-f", "srt", "pipe:1"]);
    tool(decode).await
}

pub(super) async fn assert_page_spans_source_window(ffmpeg: &Path, clip: &Path, root: &Path) {
    const WINDOW_BYTES: usize = 32_768 * 188;
    let length = tokio::fs::metadata(clip).await.unwrap().len();
    assert!(length > u64::try_from(WINDOW_BYTES).unwrap());
    assert!(
        length < 32 * 1024 * 1024,
        "finite synthetic test input bound"
    );
    let bytes = tokio::fs::read(clip).await.unwrap();
    assert_eq!(bytes.len() % 188, 0);
    let prefix = root.join("first-source-window.ts");
    tokio::fs::write(&prefix, &bytes[..WINDOW_BYTES])
        .await
        .unwrap();
    let first = decode_page(ffmpeg, &prefix, "889").await;
    let first_text = std::str::from_utf8(&first).unwrap();
    assert!(first_text.contains("NEDERLANDSE PAGINA 889 EARLY"));
    assert!(!first_text.contains("889 FINAL"));
    let complete = tokio::fs::read_to_string(root.join("independent-889.srt"))
        .await
        .unwrap();
    assert!(complete.contains("NEDERLANDSE PAGINA 889 FINAL"));

    // A missing prefix decode alone could mean EOF buffering. Locate the
    // known synthetic row's odd-parity/reversed wire literal too, so FINAL's
    // actual payload bytes must lie beyond the physical packet window.
    let marker: Vec<_> = b"NEDERLANDSE PAGINA 889 FINAL"
        .iter()
        .map(|byte| {
            let parity = if byte.count_ones() % 2 == 0 { 0x80 } else { 0 };
            (byte | parity).reverse_bits()
        })
        .collect();
    let positions: Vec<_> = bytes
        .windows(marker.len())
        .enumerate()
        .filter_map(|(offset, row)| (row == marker).then_some(offset))
        .collect();
    let [final_offset] = positions.as_slice() else {
        panic!("exactly one known fixture FINAL row required");
    };
    assert!(*final_offset >= WINDOW_BYTES);
    tokio::fs::write(root.join("first-source-window-889.srt"), first)
        .await
        .unwrap();
    tokio::fs::write(
        root.join("source-window-preconditions.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "source_bytes":length,"source_packets":bytes.len()/188,
            "window_bytes":WINDOW_BYTES,"window_packets":32768,
            "selected_page":889,"prefix_has_early_caption":true,
            "full_decode_has_final_caption":true,"prefix_decode_has_final_caption":false,
            "final_fixture_wire_literal_offset":final_offset,
            "final_fixture_wire_literal_packet":final_offset/188,
            "scope":"Finite synthetic source preconditions; no Next output assertion has run yet."
        }))
        .unwrap(),
    )
    .await
    .unwrap();
}

#[allow(
    clippy::too_many_lines,
    reason = "Opt-in proof keeps actual probe acquisition, managed association and exact source/page assertions together"
)]
pub(super) async fn acquire_and_store(
    pool: &crate::db::DbPool,
    ffprobe: &Path,
    clip: &Path,
    root: &Path,
) {
    let scope = LocalProbeCleanupScope::new();
    let probe = scope
        .scope(FfprobeExtractor::new(ffprobe.to_str().unwrap()).extract_local_with_teletext(clip))
        .await
        .unwrap();
    scope.close();
    scope.wait_cleanup().await.unwrap();
    let acquisition = probe.teletext.unwrap();
    assert!(acquisition.is_current(clip));
    let TeletextProbeSnapshot::Available(streams) = acquisition.snapshot() else {
        panic!("actual teletext metadata required")
    };
    assert_eq!(streams.len(), 1);
    let stream = &streams[0];
    assert_eq!(stream.stream_index, 2);
    assert_eq!(stream.pid, TeletextProbePid::Available(258));
    sqlx::query(r#"DELETE FROM "Subtitle""#)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(r#"DELETE FROM "MediaStream""#)
        .execute(pool)
        .await
        .unwrap();
    for item in &probe.media.streams {
        let kind = match item.kind {
            MediaStreamKind::Video => 1,
            MediaStreamKind::Audio => 2,
            MediaStreamKind::Subtitle => 3,
            _ => panic!("unexpected generated fixture stream"),
        };
        sqlx::query(r#"INSERT INTO "MediaStream" ("Id","MediaVersionId","Index","Codec","MediaStreamKind","Language","Channels","Title","Default","Forced") VALUES (?,100,?,?,?,?,?,?,?,?)"#)
            .bind(i64::from(item.index) + 1).bind(item.index).bind(&item.codec).bind(kind)
            .bind(&item.language).bind(item.channels.unwrap_or(0)).bind(&item.title).bind(item.default).bind(item.forced)
            .execute(pool).await.unwrap();
        if item.kind == MediaStreamKind::Subtitle {
            sqlx::query(r#"INSERT INTO "Subtitle" ("MovieMetadataId","SubtitleKind","Title","StreamIndex","Codec","Default","Forced","SDH","Language","IsExtracted","DateAdded","DateUpdated") VALUES (20,0,?,?,?,?,?,0,?,0,'2026-10-18','2026-10-18')"#)
                .bind(&item.title).bind(item.index).bind(&item.codec).bind(item.default).bind(item.forced).bind(&item.language)
                .execute(pool).await.unwrap();
        }
    }
    sqlx::query(r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (1)"#)
        .execute(pool)
        .await
        .unwrap();
    let repository = crate::repositories::SqliteManagedLibraryOperationRepository(pool.clone());
    let operation = repository
        .enqueue_scan(
            ManagedLibraryOperationKind::ManualScan,
            1,
            ManagedLibraryScanSpec {
                provider: MediaSourceKind::Local,
                show_id: None,
                deep_scan: true,
            },
            false,
        )
        .await
        .unwrap()
        .unwrap();
    let claim = repository
        .claim_operation(operation.id, chrono::Utc::now())
        .await
        .unwrap()
        .unwrap();
    let TeletextProbePrograms::Available(programs) = &stream.programs else {
        panic!("actual program required")
    };
    let TeletextProbeDescriptors::Available(descriptors) = &stream.descriptors else {
        panic!("actual descriptors required")
    };
    assert_eq!(descriptors.entries.len(), 2);
    assert!(descriptors.warning.is_none());
    let mut report = SourceTeletextObservation::new(
        SourceTeletextBinding::Bound(
            SourceTeletextBindingIdentity::new(
                1,
                MediaSourceKind::Local,
                100,
                1,
                3,
                acquisition.revision().clone(),
            )
            .unwrap(),
        ),
        stream.stream_index,
        SourceTeletextProvenance::Ffprobe9PackedDescriptorProjection,
        SourceTeletextPid::Reported(258),
        SourceTeletextPrograms::Reported(
            programs
                .iter()
                .map(|p| SourceTeletextProgram {
                    program_id: p.program_id,
                    program_number: p.program_number,
                    pmt_pid: p.pmt_pid,
                    pcr_pid: p.pcr_pid,
                })
                .collect(),
        ),
        SourceTeletextDescriptors::Reported {
            entries: descriptors
                .entries
                .iter()
                .map(|entry| {
                    SourceTeletextDescriptor::new(
                        u32::try_from(entry.projection_ordinal).unwrap(),
                        entry.language.clone(),
                        entry.descriptor_type,
                        entry.wire_magazine,
                        entry.wire_page,
                    )
                    .unwrap()
                })
                .collect(),
            trailing_unpaired_pairs: None,
        },
    )
    .unwrap();
    report.observed_pages = SourceTeletextObservedPages::HeadersObserved(Box::new(
        acquisition.headers().for_stream(2).unwrap().clone(),
    ));
    let mut tx = crate::db::begin_write(pool).await.unwrap();
    assert!(crate::teletext_source::replace(
        &mut tx,
        &claim,
        SourceTeletextParents {
            media_version_id: 100,
            media_stream_id: 3,
            media_file_id: 1,
        },
        &report
    )
    .await
    .unwrap());
    tx.commit().await.unwrap();
    assert!(repository.complete_operation(&claim).await.unwrap());
    let candidates = load_caption_candidates(pool, 10, 100, 1, clip.to_str().unwrap())
        .await
        .unwrap();
    assert_eq!(
        candidates
            .iter()
            .map(|c| (c.request.page.get(), c.language.as_str(), c.descriptor_type))
            .collect::<Vec<_>>(),
        [(888, "eng", 2), (889, "nld", 2)]
    );
    assert_eq!(candidates[0].request.binding, candidates[1].request.binding);
    assert_eq!(candidates[0].request.pid, 258);
    assert_eq!(candidates[0].request.stream_index, 2);
    assert_eq!(candidates[0].request.program_number, 1);
    tokio::fs::write(root.join("acquired-page-choices.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "scanner_executed":false,"source_revision":acquisition.revision(),"pid":258,"index":2,
        "choices":[{"page":888,"language":"eng","role":2},{"page":889,"language":"nld","role":2}],
        "actual_program_and_psi":true,"independent_page_decodes":true,
    })).unwrap()).await.unwrap();
}
