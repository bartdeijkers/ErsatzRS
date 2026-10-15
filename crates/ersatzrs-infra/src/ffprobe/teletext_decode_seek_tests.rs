//! Actual later-source packet-window decoding, without temporal/playback authority.

use std::{fmt::Write as _, path::PathBuf, time::Duration};

use ersatzrs_core::locking::{EntityLocker, TeletextWorkGuard};
use ersatzrs_domain::{MediaSourceKind, SourceTeletextBindingIdentity, TeletextPageNumber};
use ersatzrs_ffmpeg::teletext::{
    source_observation::MAX_SOURCE_OBSERVATION_PACKETS, source_psi::SourceTeletextPsiObserver,
};
use sha2::{Digest as _, Sha256};
use tokio::time::Instant;

use super::{
    local, DecodedLocalTeletextPage, FfprobeCommand, FfprobeExtractor, LocalProbeCleanupScope,
    SourceTeletextPacketSpan, SourceTeletextPageDecodeError, SourceTeletextPageDecodeRequest,
};

const PACKET_BYTES: usize = 188;
const SELECTED_PID: u16 = 258;

fn hex_text(bytes: &[u8]) -> String {
    let mut text = String::new();
    for byte in bytes {
        write!(text, "{byte:02x}").unwrap();
    }
    text
}

struct LaterSource {
    _directory: tempfile::TempDir,
    path: PathBuf,
    bytes: Vec<u8>,
    extractor: FfprobeExtractor,
    request: SourceTeletextPageDecodeRequest,
    span: SourceTeletextPacketSpan,
}

#[allow(
    clippy::disallowed_methods,
    reason = "Explicit opt-in native test tools, never product configuration"
)]
async fn tool(scope: &LocalProbeCleanupScope, variable: &str, name: &str) -> String {
    let program = std::env::var(variable).expect("set explicit native test tool");
    let version = scope
        .scope(local::capture(
            FfprobeCommand {
                program: program.clone(),
                args: vec!["-version".into()],
            },
            "synthetic source decoder version".into(),
            Some(Instant::now() + Duration::from_secs(10)),
        ))
        .await
        .expect("actual tool version");
    let version = std::str::from_utf8(&version).expect("tool version text");
    let first_line = version.lines().next().expect("tool version line");
    let release = first_line
        .strip_prefix(&format!("{name} version "))
        .expect("actual tool version prefix");
    assert!(release
        .strip_prefix('n')
        .unwrap_or(release)
        .starts_with("9."));
    eprintln!("later-source native component: {first_line}");
    program
}

async fn generate(
    scope: &LocalProbeCleanupScope,
    program: String,
    fixture: &std::path::Path,
    output: &std::path::Path,
) {
    let mut args: Vec<String> = [
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostdin",
        "-f",
        "lavfi",
        "-i",
        "color=c=black:s=492x250:r=25",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:sample_rate=48000",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=660:sample_rate=48000",
        "-itsoffset",
        "40",
        "-f",
        "mpegts",
        "-i",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    args.push(fixture.to_str().expect("synthetic fixture path").into());
    args.extend(
        [
            "-t",
            "60",
            "-map",
            "0:v:0",
            "-map",
            "1:a:0",
            "-map",
            "3:s:0",
            "-map",
            "2:a:0",
            "-c:v",
            "mpeg2video",
            "-b:v",
            "200k",
            "-c:a",
            "aac",
            "-b:a",
            "64k",
            "-c:s",
            "copy",
            "-metadata:s:a:0",
            "language=eng",
            "-metadata:s:a:1",
            "language=nld",
            "-muxrate",
            "2000000",
            "-pcr_period",
            "20",
            "-mpegts_start_pid",
            "256",
            "-mpegts_pmt_start_pid",
            "4096",
            "-f",
            "mpegts",
            "-n",
        ]
        .into_iter()
        .map(str::to_owned),
    );
    args.push(output.to_str().expect("synthetic output path").into());
    let stdout = scope
        .scope(local::capture(
            FfprobeCommand { program, args },
            "synthetic delayed teletext generator".into(),
            Some(Instant::now() + Duration::from_secs(30)),
        ))
        .await
        .expect("actual delayed-source generator completes and is reaped");
    assert!(stdout.is_empty());
}

fn packet_pid(packet: &[u8]) -> u16 {
    assert_eq!(packet[0], 0x47);
    u16::from(packet[1] & 0x1f) << 8 | u16::from(packet[2])
}

fn later_span(bytes: &[u8]) -> SourceTeletextPacketSpan {
    assert!(bytes.len().is_multiple_of(PACKET_BYTES));
    let packets: Vec<_> = bytes.chunks_exact(PACKET_BYTES).collect();
    let first_selected = packets
        .iter()
        .position(|packet| packet_pid(packet) == SELECTED_PID)
        .expect("actual late teletext payload");
    assert!(first_selected > MAX_SOURCE_OBSERVATION_PACKETS);
    assert!(packets[..MAX_SOURCE_OBSERVATION_PACKETS]
        .iter()
        .all(|packet| packet_pid(packet) != SELECTED_PID));
    let pmt = packets[..first_selected]
        .iter()
        .rposition(|packet| packet_pid(packet) == 4096 && packet[1] & 0x40 != 0)
        .expect("actual preceding PMT start");
    let first = packets[..pmt]
        .iter()
        .rposition(|packet| packet_pid(packet) == 0 && packet[1] & 0x40 != 0)
        .expect("actual PAT preceding that PMT");
    assert!(first > 0);
    let span = SourceTeletextPacketSpan::new(
        u64::try_from(first).unwrap(),
        u32::try_from(packets.len() - first).unwrap(),
    )
    .expect("actual later range fits the existing bounded packet budget");
    let mut psi = SourceTeletextPsiObserver::new(SELECTED_PID).unwrap();
    for packet in &packets[first..=first_selected] {
        psi.push((*packet).try_into().unwrap()).unwrap();
    }
    let observation = psi.observation().expect("actual complete current PSI");
    assert!(observation
        .memberships()
        .any(|(program, stream)| { program.program_number == 1 && stream.stream_type == 6 }));
    assert_ne!(packets[first_selected][1] & 0x40, 0);
    span
}

impl LaterSource {
    async fn new(scope: &LocalProbeCleanupScope) -> Self {
        let ffmpeg = tool(scope, "ERSATZRS_TELETEXT_TEST_FFMPEG", "ffmpeg").await;
        let probe = tool(scope, "FFPROBE_PATH", "ffprobe").await;
        let directory = tempfile::tempdir().unwrap();
        let fixture = directory.path().join("teletext.ts");
        std::fs::write(
            &fixture,
            include_bytes!("../../../ersatzrs-ffmpeg/src/fixtures/teletext.ts"),
        )
        .unwrap();
        let path = directory.path().join("delayed-source.ts");
        generate(scope, ffmpeg, &fixture, &path).await;
        let bytes = std::fs::read(&path).unwrap();
        let span = later_span(&bytes);
        let extractor = FfprobeExtractor::new(probe);
        let acquired = scope
            .scope(extractor.extract_local_with_teletext(&path))
            .await
            .expect("actual source acquisition");
        let stream = acquired
            .media
            .streams
            .iter()
            .find(|stream| stream.codec.as_deref() == Some("dvb_teletext"))
            .expect("actual teletext metadata row");
        let acquired = acquired.teletext.expect("actual held source revision");
        let request = SourceTeletextPageDecodeRequest {
            binding: SourceTeletextBindingIdentity::new(
                1,
                MediaSourceKind::Local,
                2,
                3,
                4,
                acquired.revision().clone(),
            )
            .unwrap(),
            stream_index: stream.index,
            pid: SELECTED_PID,
            program_number: 1,
            page: TeletextPageNumber::new(888).unwrap(),
        };
        Self {
            _directory: directory,
            path,
            bytes,
            extractor,
            request,
            span,
        }
    }

    async fn decode(
        &self,
        scope: &LocalProbeCleanupScope,
        work: &TeletextWorkGuard,
        page: u16,
        program: u16,
    ) -> Result<DecodedLocalTeletextPage, SourceTeletextPageDecodeError> {
        self.decode_at(scope, work, page, program, self.span).await
    }

    async fn decode_at(
        &self,
        scope: &LocalProbeCleanupScope,
        work: &TeletextWorkGuard,
        page: u16,
        program: u16,
        span: SourceTeletextPacketSpan,
    ) -> Result<DecodedLocalTeletextPage, SourceTeletextPageDecodeError> {
        let mut request = self.request.clone();
        request.page = TeletextPageNumber::new(page).unwrap();
        request.program_number = program;
        scope
            .scope(self.extractor.decode_local_teletext_page_at_packet_span(
                request,
                self.path.clone(),
                self.path.clone(),
                span,
                Instant::now() + Duration::from_secs(15),
                work.hold_work(),
            ))
            .await
    }

    async fn assert_complete_non_eof_tail(
        &self,
        scope: &LocalProbeCleanupScope,
        work: &TeletextWorkGuard,
    ) {
        let packets: Vec<_> = self.bytes.chunks_exact(PACKET_BYTES).collect();
        let last_selected = packets
            .iter()
            .rposition(|packet| packet_pid(packet) == SELECTED_PID)
            .unwrap();
        let end = last_selected + 513;
        assert!(end < packets.len());
        let tail = &packets[last_selected + 1..end];
        assert!(tail.iter().all(|packet| packet_pid(packet) != SELECTED_PID));
        assert!(tail.iter().any(|packet| {
            packet_pid(packet) == 256
                && packet[3] & 0x20 != 0
                && packet[4] > 0
                && packet[5] & 0x10 != 0
        }));
        let first = usize::try_from(self.span.first_packet()).unwrap();
        let span = SourceTeletextPacketSpan::new(
            self.span.first_packet(),
            u32::try_from(end - first).unwrap(),
        )
        .unwrap();
        let decoded = self.decode_at(scope, work, 888, 1, span).await.unwrap();
        let window = decoded.window();
        assert!(decoded.events().count > 0);
        assert!(!window.ended_at_eof);
        assert!(!window.covers_entire_source_bytes());
        assert_eq!(window.source_packets, span.packet_count());
        assert_eq!(window.decoder_packets, span.packet_count());
        assert_eq!(window.skipped_selected_prefix_packets, 0);
        assert_eq!(window.trimmed_source_tail_packets, 0);
        let expected: [u8; 32] =
            Sha256::digest(&self.bytes[first * PACKET_BYTES..end * PACKET_BYTES]).into();
        assert_eq!(window.decoder_sha256, expected);
        eprintln!(
            "later-source non-EOF component: retained 512 actual nonselected tail packets including PCR; first={}, count={}, exact_sha256={}",
            span.first_packet(), span.packet_count(), hex_text(&expected)
        );
    }

    fn assert_exact_span(&self, decoded: &DecodedLocalTeletextPage) {
        let window = decoded.window();
        assert_eq!(window.first_source_packet, self.span.first_packet());
        assert_ne!(window.first_source_packet, 0);
        assert_eq!(window.source_packets, self.span.packet_count());
        assert!(window.ended_at_eof);
        assert!(!window.covers_entire_source_bytes());
        assert_eq!(window.skipped_selected_prefix_packets, 0);
        assert_eq!(window.trimmed_source_tail_packets, 0);
        assert_eq!(window.decoder_packets, window.source_packets);
        let first = usize::try_from(self.span.byte_offset()).unwrap();
        let actual_span = &self.bytes[first..];
        let expected: [u8; 32] = Sha256::digest(actual_span).into();
        assert_eq!(window.decoder_sha256, expected);
        let wrong_prefix: [u8; 32] = Sha256::digest(&self.bytes[..actual_span.len()]).into();
        assert_ne!(window.decoder_sha256, wrong_prefix);
        assert_eq!(
            (window.program_number, window.pmt_pid, window.pcr_pid),
            (1, 4096, 256)
        );
    }
}

#[tokio::test]
#[ignore = "requires explicit FFmpeg/FFprobe 9 with libzvbi for genuine delayed-source decoding"]
async fn native_later_packet_span_decodes_exact_pages_without_prefix_or_temporal_authority() {
    let scope = LocalProbeCleanupScope::new();
    let source = LaterSource::new(&scope).await;
    let locker = EntityLocker::new();
    let work = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
    let prefix = scope
        .scope(source.extractor.decode_local_teletext_page(
            source.request.clone(),
            source.path.clone(),
            source.path.clone(),
            Instant::now() + Duration::from_secs(15),
            work.hold_work(),
        ))
        .await;
    assert!(matches!(
        prefix,
        Err(SourceTeletextPageDecodeError::InvalidTransportWindow)
    ));
    let subtitle = source.decode(&scope, &work, 888, 1).await.unwrap();
    let guide = source.decode(&scope, &work, 200, 1).await.unwrap();
    assert!(subtitle.events().count > 0);
    assert!(guide.events().count > 0);
    assert_ne!(subtitle.request().page, guide.request().page);
    assert_ne!(
        subtitle.native_burn_input().streams[0].source_teletext_page,
        guide.native_burn_input().streams[0].source_teletext_page
    );
    source.assert_exact_span(&subtitle);
    source.assert_exact_span(&guide);
    assert_eq!(subtitle.window(), guide.window());
    eprintln!(
        "later-source component: source_packets={}, first_source_packet={}, span_packets={}, decoded888={}, decoded200={}, source_sha256={}, decoder_sha256={}",
        source.bytes.len() / PACKET_BYTES,
        source.span.first_packet(),
        source.span.packet_count(),
        subtitle.events().count,
        guide.events().count,
        hex_text(Sha256::digest(&source.bytes).as_slice()),
        hex_text(&subtitle.window().decoder_sha256)
    );
    assert!(matches!(
        source.decode(&scope, &work, 777, 1).await,
        Err(SourceTeletextPageDecodeError::NoBitmap)
    ));
    assert!(matches!(
        source.decode(&scope, &work, 888, 2).await,
        Err(SourceTeletextPageDecodeError::InvalidTransportWindow)
    ));
    source.assert_complete_non_eof_tail(&scope, &work).await;
    let after = std::fs::read(&source.path).unwrap();
    assert_eq!(Sha256::digest(&after), Sha256::digest(&source.bytes));
    assert_eq!(after, source.bytes);
    drop(subtitle);
    drop(guide);
    scope.close();
    assert_eq!(scope.wait_cleanup().await, Ok(()));
    drop(work);
    assert_eq!(locker.active_teletext_work(), 0);
    assert!(TeletextWorkGuard::try_acquire(&locker, 1).is_some());
}
