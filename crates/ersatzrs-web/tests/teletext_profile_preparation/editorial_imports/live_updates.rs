//! Same-response accepted-generation updates, observed on synthetic wire bytes.
//! Literal source tests are not real playback or suitable-client acceptance.

#[path = "live_updates/live_http.rs"]
mod live_http;
#[path = "live_updates/live_tcp.rs"]
pub(in super::super) mod live_tcp;
#[path = "live_updates/live_tcp_clock.rs"]
mod live_tcp_clock;

use super::*;
use ersatzrs_core::repositories::{
    TeletextProfileMutation, TeletextProfileRejection, TeletextPublicationMutation,
};
use ersatzrs_ffmpeg::{build_mpegts_concat_command, OutputMetadata};
use ersatzrs_web::transport::iptv::{
    teletext_live_output::{
        forward_live_teletext_output, TeletextAcceptedUpdatePolicy, TeletextLiveOutputIntent,
    },
    teletext_source_policy::{TeletextSourcePolicy, TeletextSourcePolicyLimits},
};
use std::{
    io,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};
use tokio::{io::AsyncWrite, sync::Notify, task::JoinHandle, time::Instant};

const MAX_CAPTURE: usize = 48 * 1024 * 1024;

fn nibble(byte: u8) -> u8 {
    const HAMMING: [u8; 16] = [
        0x15, 0x02, 0x49, 0x5e, 0x64, 0x73, 0x38, 0x2f, 0xd0, 0xc7, 0x8c, 0x9b, 0xa1, 0xb6, 0xfd,
        0xea,
    ];
    u8::try_from(
        HAMMING
            .iter()
            .position(|value| *value == byte.reverse_bits())
            .unwrap(),
    )
    .unwrap()
}

fn same_unit(expected: &[u8; 46], actual: &[u8; 46]) -> bool {
    if expected == actual {
        return true;
    }
    // Repeated headers clear only C4 (erase). These resource-free fixtures
    // permit that temporal control change while preserving every identity,
    // subcode, other header control and authored content byte exactly.
    if (nibble(expected[4]) | (nibble(expected[5]) << 4)) >> 3 != 0 {
        return false;
    }
    if (nibble(actual[4]) | (nibble(actual[5]) << 4)) >> 3 != 0 {
        return false;
    }
    let mut expected = *expected;
    let mut actual = *actual;
    expected[9] = nibble(expected[9]) & 7;
    actual[9] = nibble(actual[9]) & 7;
    expected == actual
}

#[derive(Default)]
pub(in super::super) struct Wire {
    bytes: Vec<u8>,
    parsed: usize,
    pmt: Option<bool>,
    pes: Vec<u8>,
    counter: Option<u8>,
    units: Vec<(usize, [u8; 46])>,
}

impl Wire {
    fn append(&mut self, bytes: &[u8]) {
        assert!(self.bytes.len().checked_add(bytes.len()).unwrap() <= MAX_CAPTURE);
        self.bytes.extend_from_slice(bytes);
        while self.bytes.len() - self.parsed >= 188 {
            let packet: [u8; 188] = self.bytes[self.parsed..self.parsed + 188]
                .try_into()
                .unwrap();
            self.parsed += 188;
            assert_eq!(packet[0], 0x47);
            let pid = u16::from_be_bytes([packet[1] & 31, packet[2]]);
            let payload = match packet[3] & 0x30 {
                0x10 => &packet[4..],
                0x30 => &packet[5 + usize::from(packet[4])..],
                _ => continue,
            };
            if packet[1] & 0x40 != 0 && !payload.is_empty() {
                let start = usize::from(payload[0]) + 1;
                if start + 12 <= payload.len()
                    && payload[start] == 2
                    && payload[start + 1] & 0xf0 == 0xb0
                {
                    let section = &payload[start..];
                    let extent = 3 + usize::from(u16::from_be_bytes([section[1] & 15, section[2]]));
                    assert!(extent <= section.len());
                    let mut pos =
                        12 + usize::from(u16::from_be_bytes([section[10] & 15, section[11]]));
                    let mut present = false;
                    while pos + 5 <= extent - 4 {
                        present |=
                            u16::from_be_bytes([section[pos + 1] & 31, section[pos + 2]]) == 512;
                        pos += 5 + usize::from(u16::from_be_bytes([
                            section[pos + 3] & 15,
                            section[pos + 4],
                        ]));
                    }
                    assert_eq!(pos, extent - 4);
                    self.pmt = Some(present);
                }
            }
            if pid != 512 {
                continue;
            }
            let counter = packet[3] & 15;
            if let Some(previous) = self.counter {
                assert_eq!(counter, (previous + 1) & 15);
            }
            self.counter = Some(counter);
            if packet[1] & 0x40 != 0 {
                assert!(self.pes.is_empty());
            } else {
                assert!(!self.pes.is_empty());
            }
            self.pes.extend_from_slice(payload);
            assert_eq!(&self.pes[..4], &[0, 0, 1, 0xbd]);
            let extent = 6 + usize::from(u16::from_be_bytes([self.pes[4], self.pes[5]]));
            assert!(self.pes.len() <= extent);
            if self.pes.len() == extent {
                assert_eq!(self.pes[45], 0x10);
                assert_eq!((extent - 46) % 46, 0);
                for unit in self.pes[46..].chunks_exact(46) {
                    assert_eq!(unit[1], 44);
                    if matches!(unit[0], 2 | 3) {
                        assert!(self.units.len() < 65_536);
                        self.units.push((self.parsed, unit.try_into().unwrap()));
                    } else {
                        assert_eq!(unit[0], 0xff);
                    }
                }
                self.pes.clear();
            }
        }
    }
}

#[derive(Clone)]
pub(in super::super) struct Capture {
    wire: Arc<Mutex<Wire>>,
    changed: Arc<Notify>,
}

impl Capture {
    fn new() -> Self {
        Self {
            wire: Arc::new(Mutex::new(Wire::default())),
            changed: Arc::new(Notify::new()),
        }
    }
    pub(in super::super) fn position(&self) -> usize {
        self.wire.lock().unwrap().parsed
    }
    pub(in super::super) fn bytes(&self) -> Vec<u8> {
        self.wire.lock().unwrap().bytes.clone()
    }
    async fn wait(&self, predicate: impl Fn(&Wire) -> bool) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let notified = self.changed.notified();
                if predicate(&self.wire.lock().unwrap()) {
                    return;
                }
                notified.await;
            }
        })
        .await
        .expect("same output did not reach its required wire state");
    }
    pub(in super::super) async fn off(&self, after: usize) {
        self.wait(|wire| wire.parsed >= after + 188 * 200 && wire.pmt == Some(false))
            .await;
    }
    async fn service(&self, after: usize, candidate: &AcceptedTeletextOutputCandidate) {
        self.service_encoded(after, candidate.service()).await;
    }
    pub(in super::super) async fn service_encoded(
        &self,
        after: usize,
        service: &ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService,
    ) {
        let expected: Vec<_> = designs::units(service)
            .into_iter()
            .flatten()
            .chain(service.magazine_units().iter().copied())
            .collect();
        assert!(!expected.is_empty());
        self.wait(|wire| {
            wire.pmt == Some(true)
                && expected.iter().all(|expected| {
                    wire.units
                        .iter()
                        .any(|(offset, actual)| *offset > after && same_unit(expected, actual))
                })
        })
        .await;
    }
}

impl AsyncWrite for Capture {
    fn poll_write(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.wire.lock().unwrap().append(bytes);
        self.changed.notify_one();
        Poll::Ready(Ok(bytes.len()))
    }
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

fn source_policy() -> TeletextSourcePolicy {
    TeletextSourcePolicy::new(TeletextSourcePolicyLimits {
        elementary_pid: 512,
        rate_tolerance_ppm: 0,
        pcr_jitter_ticks: 0,
        pcr_gap_slack_ticks: 540_000,
        pcr_gap_slack_packets: 1,
        min_pcr_intervals: 2,
        max_window_packets: 1024,
        max_buffer_packets: 2048,
        max_hold: Duration::from_secs(1),
        playout_delay: Duration::from_millis(100),
        max_lateness: Duration::from_millis(200),
        first_byte_timeout: Duration::from_secs(1),
        probe_maximum_bytes: 1024 * 188,
        probe_maximum_time: Duration::from_secs(1),
        startup_maximum_bytes: 2048 * 188,
    })
    .unwrap()
}

fn section_packet(pid: u16, mut section: Vec<u8>) -> [u8; 188] {
    let length = u16::try_from(section.len() + 1).unwrap().to_be_bytes();
    section[1] = 0xb0 | length[0];
    section[2] = length[1];
    let mut crc = u32::MAX;
    for &byte in &section {
        crc ^= u32::from(byte) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 == 0 {
                crc << 1
            } else {
                (crc << 1) ^ 0x04c1_1db7
            };
        }
    }
    section.extend_from_slice(&crc.to_be_bytes());
    let mut packet = [0xff; 188];
    let [high, low] = pid.to_be_bytes();
    packet[..5].copy_from_slice(&[0x47, 0x40 | high, low, 0x10, 0]);
    packet[5..5 + section.len()].copy_from_slice(&section);
    packet
}

fn source() -> Vec<u8> {
    let mut null = [0xff; 188];
    null[..4].copy_from_slice(&[0x47, 31, 255, 0x10]);
    let mut packets = vec![null; 3 + 400 * 5_000_000 / (50 * 1504)];
    packets[0] = section_packet(0, vec![0, 0xb0, 0, 0, 1, 0xc1, 0, 0, 0, 37, 0xed, 9]);
    packets[1] = section_packet(
        3337,
        vec![
            2, 0xb0, 0, 0, 37, 0xc1, 0, 0, 0xe3, 0x85, 0xf0, 0, 0x1b, 0xe3, 0x85, 0xf0, 0, 0x0f,
            0xe3, 0x86, 0xf0, 0,
        ],
    );
    for (sample, pos) in (2..packets.len()).step_by(5).enumerate() {
        // Literal physical relation: five TS packets at 5 Mbps = 40,608 PCR ticks.
        let ticks = 900_000 + u64::try_from(sample).unwrap() * 40_608;
        let base = ticks / 300;
        let extension = ticks % 300;
        let mut p = [0xff; 188];
        p[..6].copy_from_slice(&[0x47, 3, 0x85, 0x20, 183, 0x10]);
        p[6] = u8::try_from(base >> 25).unwrap();
        p[7] = u8::try_from((base >> 17) & 255).unwrap();
        p[8] = u8::try_from((base >> 9) & 255).unwrap();
        p[9] = u8::try_from((base >> 1) & 255).unwrap();
        p[10] =
            u8::try_from((base & 1) << 7).unwrap() | 0x7e | u8::try_from(extension >> 8).unwrap();
        p[11] = u8::try_from(extension & 255).unwrap();
        packets[pos] = p;
    }
    for (sample, pos) in (4..packets.len()).step_by(10).enumerate() {
        let mut p = [0x5a; 188];
        p[..4].copy_from_slice(&[0x47, 3, 0x86, 0x10 | u8::try_from(sample & 15).unwrap()]);
        packets[pos] = p;
    }
    packets.into_iter().flatten().collect()
}

struct Current {
    design_id: i64,
    revision: i64,
    request: TeletextPublicationRequest,
    accepted: Arc<AcceptedTeletextOutputCandidate>,
}

async fn pending_exact(
    fixture: &Fixture,
    design_id: i64,
    revision: i64,
    digest: &str,
    previous: i64,
) -> TeletextPublicationRequest {
    let result =
        ersatzrs_web::host::teletext_designs::request_original_teletext_design_publication(
            &fixture.state.application,
            design_id,
            revision,
            digest.to_owned(),
            revision,
            Some(previous),
            designs::budget(),
            channel_documents::policy(),
            designs::deadline(),
        )
        .await
        .unwrap();
    let TeletextPublicationMutation::Requested(pending) = result else {
        panic!("explicit publication rejected");
    };
    pending.request().clone()
}

async fn accept_exact(
    fixture: &Fixture,
    request: TeletextPublicationRequest,
    previous: i64,
) -> Current {
    let candidate = generations::accepted(
        generations::accept(fixture, &request, request.revision, Some(previous)).await,
    );
    Current {
        design_id: request.design_id,
        revision: request.revision,
        request,
        accepted: Arc::new(candidate),
    }
}

async fn enable(fixture: &Fixture, flow: &Flow, revision: i64) -> i64 {
    let selected = &flow.saved[0].candidate().selection().bindings[0];
    let result = fixture
        .state
        .application
        .save_channel_teletext_binding(&ChannelTeletextBindingSave {
            channel_id: 1,
            addon_id: selected.addon_id.clone(),
            profile_id: selected.profile_id,
            enabled: true,
            expected_installation: selected.installation.clone(),
            expected_profile_revision: selected.profile_revision,
            expected_binding_revision: Some(revision),
        })
        .await
        .unwrap();
    let TeletextProfileMutation::Saved(binding) = result else {
        panic!("existing profile re-enable rejected");
    };
    binding.revision
}

async fn disable(fixture: &Fixture, revision: i64) -> i64 {
    let result = fixture
        .state
        .application
        .disable_channel_teletext_binding(1, ADDON, revision)
        .await
        .unwrap();
    let TeletextProfileMutation::Saved(binding) = result else {
        panic!("Off rejected");
    };
    binding.revision
}

async fn edited(fixture: &Fixture, current: &Current) -> SavedEditorialTeletextDesign {
    let exported = export_original_teletext_design(
        &fixture.state.application,
        NativeTeletextExportRequest {
            design_id: current.design_id,
            revision: current.revision,
            design_digest: current.accepted.generation().request.digest.clone(),
            expected_design_head: current.revision,
            pages: None,
        },
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    let TeletextDesignMutation::Saved(exported) = exported else {
        panic!("fresh concept export rejected");
    };
    let mut files = exported.files().to_vec();
    edit_cell(
        &mut files,
        &exported.sidecar().project.pages[0].name,
        2,
        8,
        b'Q',
    );
    import(
        fixture,
        NativeTeletextEditorialImportRequest {
            name: "Live replacement concept".to_owned(),
            receipt_id: exported.metadata().id,
            receipt_digest: exported.metadata().digest.clone(),
            expected_design_head: current.revision,
            sidecar: write_teletext_export_sidecar(exported.sidecar(), policy().project).unwrap(),
            files,
        },
    )
    .await
}

fn calls(fixture: &Fixture) -> Vec<u8> {
    std::fs::read(fixture.root.path().join("authoring-calls")).unwrap()
}

async fn fresh_acceptance(
    fixture: &Fixture,
    views: &[Capture; 3],
    previous: &TeletextPublicationRequest,
    previous_generation: i64,
) -> Current {
    let saved = designs::save(
        fixture,
        authoring_metadata::candidate(fixture, 1).await,
        Some((previous.design_id, previous.revision)),
    )
    .await;
    let pending = pending_exact(
        fixture,
        saved.head().id,
        saved.head().revision,
        saved.digest(),
        previous.id,
    )
    .await;
    // Pending output remains physically Off until independent acceptance.
    for view in &views[..2] {
        view.off(view.position()).await;
    }
    let current = accept_exact(fixture, pending, previous_generation).await;
    let invoked = calls(fixture);
    for view in &views[..2] {
        view.service(view.position(), &current.accepted).await;
    }
    assert_eq!(
        calls(fixture),
        invoked,
        "monitor invoked an installed producer"
    );
    current
}

async fn replace_concept(fixture: &Fixture, views: &[Capture; 3], current: &Current) -> Current {
    let invoked = calls(fixture);
    let concept = edited(fixture, current).await;
    assert_eq!(concept.head().revision, 3);
    let pending = pending_exact(
        fixture,
        concept.head().id,
        3,
        concept.digest(),
        current.request.id,
    )
    .await;
    let current = accept_exact(fixture, pending, current.accepted.generation().id).await;
    for view in &views[..2] {
        view.service(view.position(), &current.accepted).await;
    }
    assert_eq!(
        calls(fixture),
        invoked,
        "native concept replacement invoked producer"
    );
    current
}

async fn missing_profile(
    fixture: &Fixture,
    views: &[Capture; 3],
    current: &Current,
    revision: i64,
) {
    let invoked = calls(fixture);
    let selected = &current.accepted.selection().bindings[0];
    let invalid = fixture
        .state
        .application
        .save_channel_teletext_binding(&ChannelTeletextBindingSave {
            channel_id: 1,
            addon_id: selected.addon_id.clone(),
            profile_id: i64::MAX,
            enabled: true,
            expected_installation: selected.installation.clone(),
            expected_profile_revision: selected.profile_revision,
            expected_binding_revision: Some(revision),
        })
        .await
        .unwrap();
    assert!(matches!(
        invalid,
        TeletextProfileMutation::Rejected(TeletextProfileRejection::MissingProfile)
    ));
    for view in &views[..2] {
        view.service(view.position(), &current.accepted).await;
    }
    let missing = fixture
        .state
        .application
        .tombstone_teletext_profile(
            &selected.addon_id,
            selected.profile_id,
            selected.profile_revision,
        )
        .await
        .unwrap();
    assert!(matches!(missing, TeletextProfileMutation::Saved(_)));
    for view in &views[..2] {
        view.off(view.position()).await;
    }
    assert_eq!(calls(fixture), invoked);
}

async fn scenario(
    fixture: &Fixture,
    flow: &Flow,
    views: &[Capture; 3],
    off_revision: i64,
) -> Current {
    for view in &views[..2] {
        view.off(0).await;
    }
    views[2].service(0, &flow.accepted[1]).await;
    let on_revision = enable(fixture, flow, off_revision).await;
    let current = fresh_acceptance(
        fixture,
        views,
        &flow.pending[0],
        flow.accepted[0].generation().id,
    )
    .await;
    assert_eq!(current.revision, 2);
    let current = replace_concept(fixture, views, &current).await;
    let off = disable(fixture, on_revision).await;
    for view in &views[..2] {
        view.off(view.position()).await;
    }
    let on_revision = enable(fixture, flow, off).await;
    let current = fresh_acceptance(
        fixture,
        views,
        &current.request,
        current.accepted.generation().id,
    )
    .await;
    assert_eq!(current.revision, 4);
    missing_profile(fixture, views, &current, on_revision).await;
    views[2]
        .service(views[2].position(), &flow.accepted[1])
        .await;
    assert_eq!(designs::config(fixture).await, flow.config);
    assert_eq!(
        recover(fixture, 2).await.generation(),
        flow.accepted[1].generation()
    );
    for index in 0..2 {
        assert_eq!(
            original(fixture, &flow.saved[index]).await,
            flow.originals[index]
        );
        assert_eq!(
            receipt(fixture, &flow.exported[index]).await,
            flow.receipts[index]
        );
    }
    current
}

struct Pumps(Vec<JoinHandle<(u64, bool)>>);
impl Drop for Pumps {
    fn drop(&mut self) {
        for task in &self.0 {
            task.abort();
        }
    }
}

#[tokio::test]
async fn same_sources_monitor_on_replacement_off_reenable_and_missing_profile_for_isolated_viewers()
{
    let fixture = Fixture::new_persistent().await;
    // Three response-owned recoveries and this explicit publication operator
    // share the existing bounded admission. Preserve its product default;
    // declare this four-owner test budget before the configuration baseline.
    fixture
        .state
        .application
        .set_admin_setting(ersatzrs_app::commands::SetAdminSetting {
            key: ersatzrs_web::host::teletext_work::MAX_CONCURRENT_SETTING,
            value: "4".to_owned(),
        })
        .await
        .unwrap();
    let (mut fixture, flow) =
        Box::pin(setup_with_fixture(authoring_metadata::response, fixture)).await;
    let off = disable(&fixture, 1).await;
    let views = [Capture::new(), Capture::new(), Capture::new()];
    let raw = source();
    let mut pumps = Pumps(Vec::new());
    for (index, view) in views.iter().enumerate() {
        let channel = if index == 2 { 2 } else { 1 };
        let initial = if index == 2 {
            Some(Arc::new(recover(&fixture, 2).await))
        } else {
            None
        };
        let intent = TeletextLiveOutputIntent::new(
            Arc::clone(&fixture.state.application),
            channel,
            designs::budget(),
            channel_documents::policy(),
            source_policy(),
            TeletextAcceptedUpdatePolicy::new(Duration::from_millis(25), Duration::from_secs(2))
                .unwrap(),
            initial,
        )
        .unwrap();
        let bytes = raw.clone();
        let mut sink = view.clone();
        pumps.0.push(tokio::spawn(async move {
            let command = build_mpegts_concat_command(
                "http://127.0.0.1:1/synthetic.ffconcat",
                &OutputMetadata::default(),
                Some(5_000_000),
            );
            let mut source = std::io::Cursor::new(bytes);
            let result = forward_live_teletext_output(
                &mut source,
                &mut sink,
                &command,
                &intent,
                Instant::now() + Duration::from_secs(2),
            )
            .await
            .unwrap();
            assert!(
                result.rejection.is_none(),
                "live source rejection: {:?}",
                result.rejection
            );
            (result.bytes, source.position() == result.bytes)
        }));
    }
    scenario(&fixture, &flow, &views, off).await;
    for task in &mut pumps.0 {
        let (bytes, consumed) = task.await.unwrap();
        assert!(consumed);
        assert_eq!(bytes, u64::try_from(raw.len()).unwrap());
    }
    for view in &views {
        let actual = view.bytes();
        assert_eq!(actual.len(), raw.len());
        for (before, after) in raw.chunks_exact(188).zip(actual.chunks_exact(188)) {
            if matches!(u16::from_be_bytes([before[1] & 31, before[2]]), 901 | 902) {
                assert_eq!(before, after, "same source AV/PCR packet changed");
            }
        }
        let wire = view.wire.lock().unwrap();
        assert!(wire.pes.is_empty());
        assert_eq!(wire.parsed, actual.len());
    }
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}
