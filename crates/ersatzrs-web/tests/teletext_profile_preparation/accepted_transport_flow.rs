use super::*;
use ersatzrs_ffmpeg::{build_mpegts_concat_command, FfmpegCommand, OutputMetadata};
use ersatzrs_web::transport::iptv::{
    teletext_accepted_output::{
        forward_accepted_retained_teletext_output, forward_accepted_teletext_output,
        load_accepted_teletext_output_intent, AcceptedRetainedTeletextEpoch,
        AcceptedTeletextIntentLoadError, TeletextAcceptedOutputIntent,
    },
    teletext_output::TeletextPumpError,
    teletext_source_policy::{TeletextSourcePolicy, TeletextSourcePolicyLimits},
};
use std::{
    io::{self, Cursor},
    pin::Pin,
    sync::{atomic::AtomicU8, Arc},
    task::{Context, Poll},
};
use tokio::time::Instant;
use tokio::{
    io::{AsyncRead, ReadBuf},
    sync::Notify,
};

fn command() -> FfmpegCommand {
    let command = build_mpegts_concat_command(
        "http://127.0.0.1:1/synthetic.ffconcat",
        &OutputMetadata::default(),
        Some(5_000_000),
    );
    let policy = command.mpegts_output_policy().unwrap();
    assert_eq!(policy.muxrate_bps(), 5_000_000);
    assert_eq!(policy.pcr_period_ms(), 20);
    command
}

fn intent(candidate: Arc<AcceptedTeletextOutputCandidate>) -> TeletextAcceptedOutputIntent {
    // Explicit synthetic-test bounds. Programme 37 and PCR PID 901 are
    // absent: only actual source PSI may supply those identities.
    let policy = TeletextSourcePolicy::new(TeletextSourcePolicyLimits {
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
        // Immediate synthetic input can fill the entire gate before its first
        // paced write. Journal ownership therefore covers the full held bound.
        startup_maximum_bytes: 2048 * 188,
    })
    .unwrap();
    TeletextAcceptedOutputIntent::new(candidate, policy, designs::budget().max_serialized_bytes)
        .unwrap()
}

fn retained_intent(
    candidate: Arc<AcceptedTeletextOutputCandidate>,
) -> TeletextAcceptedOutputIntent {
    let mut limits = intent(Arc::clone(&candidate)).policy().limits();
    // The ordinary eight-second reserve remains intact even for a short
    // remainder. At 5 Mbps its rounded reserve/batch/250-ms EOF allowance
    // requires 27,463 packets. This explicit test bound includes that tail.
    limits.max_buffer_packets = 28_000;
    limits.startup_maximum_bytes = 28_000 * 188;
    limits.max_hold = Duration::from_secs(9);
    TeletextAcceptedOutputIntent::new(
        candidate,
        TeletextSourcePolicy::new(limits).unwrap(),
        designs::budget().max_serialized_bytes,
    )
    .unwrap()
}

fn section_packet(pid: u16, mut section: Vec<u8>) -> [u8; 188] {
    let length = u16::try_from(section.len() + 1).unwrap().to_be_bytes();
    section[1] = 0xb0 | length[0];
    section[2] = length[1];
    // MPEG-2 CRC arithmetic is independent of the transport implementation.
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

fn pcr_packet(ticks: u64) -> [u8; 188] {
    let base = ticks / 300;
    let extension = ticks % 300;
    let mut packet = [0xff; 188];
    packet[..6].copy_from_slice(&[0x47, 3, 0x85, 0x20, 183, 0x10]);
    packet[6] = u8::try_from(base >> 25).unwrap();
    packet[7] = u8::try_from((base >> 17) & 255).unwrap();
    packet[8] = u8::try_from((base >> 9) & 255).unwrap();
    packet[9] = u8::try_from((base >> 1) & 255).unwrap();
    packet[10] =
        u8::try_from((base & 1) << 7).unwrap() | 0x7e | u8::try_from(extension >> 8).unwrap();
    packet[11] = u8::try_from(extension & 255).unwrap();
    packet
}

fn source() -> Vec<u8> {
    let mut null = [0xff; 188];
    null[..4].copy_from_slice(&[0x47, 0x1f, 0xff, 0x10]);
    // 160 independent 20-ms windows at 5 Mbps, plus original PSI prefix.
    let mut packets = vec![null; 3 + 160 * 5_000_000 / (50 * 1504)];
    packets[0] = section_packet(0, vec![0, 0xb0, 0, 0, 1, 0xc1, 0, 0, 0, 37, 0xed, 9]);
    packets[1] = section_packet(
        3337,
        vec![
            2, 0xb0, 0, 0, 37, 0xc1, 0, 0, 0xe3, 0x85, 0xf0, 0, 0x1b, 0xe3, 0x85, 0xf0, 0, 0x0f,
            0xe3, 0x86, 0xf0, 0,
        ],
    );
    // Each five-packet distance represents exactly 40,608 ticks. This
    // independently chosen physical byte/PCR relation proves the rate
    // rather than taking a fixture's clock configuration as authority.
    for (sample, position) in (2..packets.len()).step_by(5).enumerate() {
        packets[position] = pcr_packet(900_000 + u64::try_from(sample).unwrap() * 40_608);
    }
    // Opaque synthetic elementary payload, not a real audio acceptance clip.
    for (sample, position) in (4..packets.len()).step_by(10).enumerate() {
        let mut payload = [0x5a; 188];
        payload[..4].copy_from_slice(&[0x47, 3, 0x86, 0x10 | u8::try_from(sample & 15).unwrap()]);
        packets[position] = payload;
    }
    packets.into_iter().flatten().collect()
}

fn pid(packet: &[u8]) -> u16 {
    u16::from_be_bytes([packet[1] & 0x1f, packet[2]])
}

fn observe_units(bytes: &[u8]) -> Vec<[u8; 46]> {
    assert_eq!(bytes.len() % 188, 0);
    let mut pes = Vec::new();
    let mut units = Vec::new();
    let mut previous_counter = None;
    for packet in bytes.chunks_exact(188).filter(|packet| pid(packet) == 512) {
        assert_eq!(packet[0], 0x47);
        assert_eq!(packet[3] & 0x30, 0x10);
        let counter = packet[3] & 15;
        if let Some(previous) = previous_counter {
            assert_eq!(counter, (previous + 1) & 15);
        }
        previous_counter = Some(counter);
        if packet[1] & 0x40 != 0 {
            assert!(pes.is_empty(), "new PES before the old declared extent");
        } else {
            assert!(!pes.is_empty(), "continuation without a PES start");
        }
        pes.extend_from_slice(&packet[4..]);
        assert_eq!(&pes[..4], &[0, 0, 1, 0xbd]);
        let extent = 6 + usize::from(u16::from_be_bytes([pes[4], pes[5]]));
        assert!(pes.len() <= extent);
        if pes.len() == extent {
            assert_eq!(pes[8], 0x24);
            assert_eq!(pes[45], 0x10);
            assert_eq!((pes.len() - 46) % 46, 0);
            for unit in pes[46..].chunks_exact(46) {
                assert_eq!(unit[1], 44);
                match unit[0] {
                    2 | 3 => units.push(unit.try_into().unwrap()),
                    0xff => assert!(unit[2..].iter().all(|&byte| byte == 0xff)),
                    other => panic!("unexpected DVB teletext unit {other}"),
                }
            }
            pes.clear();
        }
    }
    assert!(pes.is_empty(), "incomplete final teletext PES");
    assert!(!units.is_empty());
    units
}

async fn forward(
    fixture: &Fixture,
    selected: &TeletextAcceptedOutputIntent,
    command: &FfmpegCommand,
    bytes: Vec<u8>,
) -> (Vec<u8>, Option<TeletextPumpError>) {
    let length = u64::try_from(bytes.len()).unwrap();
    let mut source = Cursor::new(bytes);
    let mut output = Vec::new();
    let result = forward_accepted_teletext_output(
        &mut source,
        &mut output,
        command,
        selected,
        &fixture.state.application,
        Instant::now() + Duration::from_secs(10),
    )
    .await
    .unwrap();
    assert_eq!(result.bytes, length);
    assert_eq!(source.position(), length);
    assert_eq!(u64::try_from(output.len()).unwrap(), length);
    (output, result.rejection)
}

#[tokio::test]
async fn frozen_accepted_generation_reaches_synthetic_wire_and_reopens_without_reinvocation() {
    let (mut fixture, saved, pending) = prepared().await;
    let config = designs::config(&fixture).await;
    let invoked = calls(&fixture);
    let bindings = fixture
        .state
        .application
        .channel_teletext_bindings(1)
        .await
        .unwrap();
    assert!(
        recover(&fixture).await.is_none(),
        "pending is not output authority"
    );
    let candidate = Arc::new(accepted(accept(&fixture, &pending, 1, None).await));
    let selected = intent(Arc::clone(&candidate));
    assert!(Arc::ptr_eq(selected.candidate(), &candidate));
    assert!(Arc::ptr_eq(
        selected.candidate().service(),
        candidate.service()
    ));
    assert_eq!(
        preview(candidate.service()).raster(),
        preview(saved.candidate().encoded().unwrap()).raster()
    );
    let original = source();
    let generated = command();
    let (first, rejected) = forward(&fixture, &selected, &generated, original.clone()).await;
    assert!(rejected.is_none(), "first output rejection: {rejected:?}");
    let actual = observe_units(&first);
    for expected in designs::units(candidate.service())
        .into_iter()
        .flatten()
        .chain(candidate.service().magazine_units().iter().copied())
    {
        assert!(
            actual.contains(&expected),
            "frozen authored unit absent from independently reassembled PES"
        );
    }
    for (before, after) in original.chunks_exact(188).zip(first.chunks_exact(188)) {
        if matches!(pid(before), 901 | 902) {
            assert_eq!(before, after, "source PCR/elementary packet changed");
        }
    }
    fixture = fixture.reopen().await;
    let recovered = Arc::new(recover(&fixture).await.unwrap());
    assert_same_output(&recovered, &candidate);
    let (second, rejected) = forward(&fixture, &intent(recovered), &generated, original).await;
    assert!(
        rejected.is_none(),
        "reopened output rejection: {rejected:?}"
    );
    assert_eq!(second, first);
    assert_eq!(calls(&fixture), invoked);
    assert_eq!(designs::config(&fixture).await, config);
    assert_eq!(
        fixture
            .state
            .application
            .channel_teletext_bindings(1)
            .await
            .unwrap(),
        bindings
    );
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn accepted_native_content_cannot_override_collision_clock_or_opaque_command() {
    let (fixture, _, pending) = prepared().await;
    let selected = intent(Arc::new(accepted(
        accept(&fixture, &pending, 1, None).await,
    )));
    let invoked = calls(&fixture);
    let config = designs::config(&fixture).await;
    for mode in 0..4 {
        let mut bytes = source();
        let mut generated = command();
        let undersized;
        let selected = if mode == 3 {
            let mut limits = selected.policy().limits();
            limits.startup_maximum_bytes = 1024 * 188;
            undersized = TeletextAcceptedOutputIntent::new(
                Arc::clone(selected.candidate()),
                TeletextSourcePolicy::new(limits).unwrap(),
                designs::budget().max_serialized_bytes,
            )
            .unwrap();
            &undersized
        } else {
            &selected
        };
        match mode {
            0 => bytes[20 * 188 + 1..20 * 188 + 3].copy_from_slice(&[2, 0]),
            1 => {
                for (sample, packet) in bytes
                    .chunks_exact_mut(188)
                    .filter(|packet| pid(packet) == 901)
                    .enumerate()
                {
                    packet.copy_from_slice(&pcr_packet(
                        900_000 + u64::try_from(sample).unwrap() * 81_216,
                    ));
                }
            }
            2 => {
                generated = FfmpegCommand::new_with_executable(
                    generated.executable,
                    generated.environment,
                    generated.arguments,
                );
            }
            _ => {}
        }
        let (output, rejected) = forward(&fixture, selected, &generated, bytes.clone()).await;
        assert!(
            rejected.is_some(),
            "mode {mode} unexpectedly gained authored authority"
        );
        if mode == 3 {
            assert!(
                matches!(
                    rejected,
                    Some(TeletextPumpError::Read(io::ErrorKind::Other))
                ),
                "undersized raw journal must report its actual tee-read rejection: {rejected:?}"
            );
        }
        assert_eq!(output, bytes, "mode {mode} damaged ordinary raw source");
    }
    assert_eq!(calls(&fixture), invoked);
    assert_eq!(designs::config(&fixture).await, config);
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn cached_accepted_intent_rechecks_off_and_generation_replacement_before_authored_delivery() {
    for off in [true, false] {
        let (fixture, saved, pending) = prepared().await;
        let candidate = Arc::new(accepted(accept(&fixture, &pending, 1, None).await));
        let selected = intent(Arc::clone(&candidate));
        if off {
            let original = &saved.candidate().selection().bindings[0];
            let result = fixture
                .state
                .application
                .save_channel_teletext_binding(&ChannelTeletextBindingSave {
                    channel_id: 1,
                    addon_id: original.addon_id.clone(),
                    profile_id: original.profile_id,
                    enabled: false,
                    expected_installation: original.installation.clone(),
                    expected_profile_revision: original.profile_revision,
                    expected_binding_revision: Some(original.binding_revision),
                })
                .await
                .unwrap();
            assert!(matches!(result, TeletextProfileMutation::Saved(_)));
        } else {
            let next = channel_documents::candidate(&fixture, channel_documents::policy())
                .await
                .unwrap();
            let next = designs::save(&fixture, next, Some((saved.head().id, 1))).await;
            let pending = request(&fixture, &next, Some(pending.id)).await;
            let changed =
                accepted(accept(&fixture, &pending, 2, Some(candidate.generation().id)).await);
            assert_ne!(changed.generation().id, candidate.generation().id);
        }
        let invoked = calls(&fixture);
        let config = designs::config(&fixture).await;
        let bytes = source();
        let (output, rejected) = forward(&fixture, &selected, &command(), bytes.clone()).await;
        assert!(
            rejected.is_some(),
            "stale cached candidate was forwarded (off={off})"
        );
        assert_eq!(output, bytes);
        assert_eq!(calls(&fixture), invoked);
        assert_eq!(designs::config(&fixture).await, config);
        assert_eq!(count(&fixture).await, if off { 1 } else { 2 });
        fixture.state.pool.close().await;
    }
}
#[tokio::test]
async fn accepted_retained_output_keeps_full_reserve_and_lossless_optional_rejection() {
    let (fixture, _, pending) = prepared().await;
    let candidate = Arc::new(accepted(accept(&fixture, &pending, 1, None).await));
    let invoked = calls(&fixture);
    let config = designs::config(&fixture).await;
    let generated = command();
    for mode in 0..3 {
        let selected = if mode == 1 {
            intent(Arc::clone(&candidate)) // Insufficient ordinary-reserve capacity.
        } else {
            retained_intent(Arc::clone(&candidate))
        };
        let mut bytes = source();
        if mode == 2 {
            bytes[20 * 188 + 1..20 * 188 + 3].copy_from_slice(&[2, 0]);
        }
        let mut reader = Cursor::new(bytes.clone());
        let mut output = Vec::new();
        // Values are the existing producer owner's terminal protocol:
        // successful exit=1, failure=2. The adapter does not create an owner.
        let terminal = AtomicU8::new(1);
        let cancel = Notify::new();
        let epoch = AcceptedRetainedTeletextEpoch::new(
            5_000_000,
            Some(Duration::from_millis(3200)),
            &terminal,
            Instant::now() + Duration::from_secs(2),
            &cancel,
        )
        .unwrap();
        let result = forward_accepted_retained_teletext_output(
            &mut reader,
            &mut output,
            &generated,
            &selected,
            &fixture.state.application,
            epoch,
        )
        .await
        .unwrap();
        assert_eq!(result.bytes, u64::try_from(bytes.len()).unwrap());
        assert_eq!(reader.position(), result.bytes);
        assert_eq!(output.len(), bytes.len());
        if mode == 0 {
            assert!(
                result.rejection.is_none(),
                "retained output rejection: {:?}",
                result.rejection
            );
            let actual = observe_units(&output);
            for expected in designs::units(candidate.service())
                .into_iter()
                .flatten()
                .chain(candidate.service().magazine_units().iter().copied())
            {
                assert!(actual.contains(&expected), "retained frozen unit missing");
            }
            for (before, after) in bytes.chunks_exact(188).zip(output.chunks_exact(188)) {
                if matches!(pid(before), 901 | 902) {
                    assert_eq!(before, after);
                }
            }
        } else {
            assert!(
                result.rejection.is_some(),
                "mode {mode} must reject authored output"
            );
            assert_eq!(output, bytes, "mode {mode} lost original input");
        }
    }
    assert_eq!(calls(&fixture), invoked);
    assert_eq!(designs::config(&fixture).await, config);
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn retained_cached_intent_honours_off_without_reinvoking_or_losing_source() {
    let (fixture, saved, pending) = prepared().await;
    let selected = retained_intent(Arc::new(accepted(
        accept(&fixture, &pending, 1, None).await,
    )));
    let original = &saved.candidate().selection().bindings[0];
    let disabled = fixture
        .state
        .application
        .save_channel_teletext_binding(&ChannelTeletextBindingSave {
            channel_id: 1,
            addon_id: original.addon_id.clone(),
            profile_id: original.profile_id,
            enabled: false,
            expected_installation: original.installation.clone(),
            expected_profile_revision: original.profile_revision,
            expected_binding_revision: Some(original.binding_revision),
        })
        .await
        .unwrap();
    assert!(matches!(disabled, TeletextProfileMutation::Saved(_)));
    let invoked = calls(&fixture);
    let config = designs::config(&fixture).await;
    let bytes = source();
    let mut reader = Cursor::new(bytes.clone());
    let mut output = Vec::new();
    let terminal = AtomicU8::new(1);
    let cancel = Notify::new();
    let epoch = AcceptedRetainedTeletextEpoch::new(
        5_000_000,
        Some(Duration::from_millis(3200)),
        &terminal,
        Instant::now() + Duration::from_secs(2),
        &cancel,
    )
    .unwrap();
    let result = forward_accepted_retained_teletext_output(
        &mut reader,
        &mut output,
        &command(),
        &selected,
        &fixture.state.application,
        epoch,
    )
    .await
    .unwrap();
    assert!(result.rejection.is_some());
    assert_eq!(output, bytes);
    assert_eq!(reader.position(), result.bytes);
    assert_eq!(result.bytes, u64::try_from(bytes.len()).unwrap());
    assert_eq!(calls(&fixture), invoked);
    assert_eq!(designs::config(&fixture).await, config);
    assert_eq!(count(&fixture).await, 1);
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn retained_entry_cancellation_and_expiry_do_not_read_or_write() {
    let (fixture, _, pending) = prepared().await;
    let selected = retained_intent(Arc::new(accepted(
        accept(&fixture, &pending, 1, None).await,
    )));
    for cancelled in [true, false] {
        let mut reader = Cursor::new(source());
        let mut output = Vec::new();
        let terminal = AtomicU8::new(1);
        let cancel = Notify::new();
        if cancelled {
            cancel.notify_one();
        }
        let startup = if cancelled {
            Instant::now() + Duration::from_secs(2)
        } else {
            Instant::now()
        };
        let epoch = AcceptedRetainedTeletextEpoch::new(
            5_000_000,
            Some(Duration::from_millis(3200)),
            &terminal,
            startup,
            &cancel,
        )
        .unwrap();
        let error = forward_accepted_retained_teletext_output(
            &mut reader,
            &mut output,
            &command(),
            &selected,
            &fixture.state.application,
            epoch,
        )
        .await
        .unwrap_err();
        assert!(error
            .to_string()
            .contains(if cancelled { "cancelled" } else { "deadline" }));
        assert_eq!(reader.position(), 0);
        assert!(output.is_empty());
    }
    fixture.state.pool.close().await;
}

enum SourceTail {
    Pending,
    Error,
}
struct ObservedSource<'a> {
    prefix: Cursor<Vec<u8>>,
    entered: &'a Notify,
    polls: usize,
    tail: SourceTail,
}
impl AsyncRead for ObservedSource<'_> {
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        output: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        this.polls += 1;
        this.entered.notify_one();
        if this.prefix.position() < u64::try_from(this.prefix.get_ref().len()).unwrap() {
            return Pin::new(&mut this.prefix).poll_read(context, output);
        }
        match this.tail {
            SourceTail::Pending => Poll::Pending,
            SourceTail::Error => Poll::Ready(Err(io::ErrorKind::Other.into())),
        }
    }
}

#[tokio::test]
async fn retained_cancellation_deadline_and_read_fault_remain_terminal_after_fresh_check() {
    let (fixture, _, pending) = prepared().await;
    let selected = retained_intent(Arc::new(accepted(
        accept(&fixture, &pending, 1, None).await,
    )));
    let invoked = calls(&fixture);
    let config = designs::config(&fixture).await;
    let generated = command();
    for mode in 0..3 {
        let entered = Notify::new();
        let mut reader = ObservedSource {
            prefix: Cursor::new(if mode == 2 {
                source()[..3 * 188].to_vec()
            } else {
                Vec::new()
            }),
            entered: &entered,
            polls: 0,
            tail: if mode == 2 {
                SourceTail::Error
            } else {
                SourceTail::Pending
            },
        };
        let mut output = Vec::new();
        let terminal = AtomicU8::new(1);
        let cancel = Notify::new();
        let startup = Instant::now()
            + if mode == 1 {
                Duration::from_millis(500)
            } else {
                Duration::from_secs(2)
            };
        let epoch = AcceptedRetainedTeletextEpoch::new(
            5_000_000,
            Some(Duration::from_millis(3200)),
            &terminal,
            startup,
            &cancel,
        )
        .unwrap();
        let operation = forward_accepted_retained_teletext_output(
            &mut reader,
            &mut output,
            &generated,
            &selected,
            &fixture.state.application,
            epoch,
        );
        let control = async {
            // This read cannot occur until the fresh SQLite confirmation
            // finishes; cancellation is exercised after that boundary.
            tokio::time::timeout(Duration::from_secs(1), entered.notified())
                .await
                .unwrap();
            if mode == 0 {
                cancel.notify_one();
            }
        };
        let (result, ()) = tokio::join!(operation, control);
        let error = result.unwrap_err();
        if mode != 2 {
            assert!(error
                .to_string()
                .contains(if mode == 0 { "cancelled" } else { "deadline" }));
        }
        assert!(
            reader.polls > 0,
            "mode {mode} did not reach actual source polling"
        );
        if mode == 2 {
            assert_eq!(reader.prefix.position(), 3 * 188);
        }
        assert!(
            output.is_empty(),
            "mode {mode} hid terminal failure behind delivery"
        );
    }
    assert_eq!(calls(&fixture), invoked);
    assert_eq!(designs::config(&fixture).await, config);
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn retained_unsuccessful_and_premature_eof_do_not_release_authored_bytes() {
    let (fixture, _, pending) = prepared().await;
    let selected = retained_intent(Arc::new(accepted(
        accept(&fixture, &pending, 1, None).await,
    )));
    for failed in [true, false] {
        let mut reader = Cursor::new(source());
        let mut output = Vec::new();
        let terminal = AtomicU8::new(if failed { 2 } else { 1 });
        let cancel = Notify::new();
        let remaining = Duration::from_millis(if failed { 3200 } else { 10_000 });
        let epoch = AcceptedRetainedTeletextEpoch::new(
            5_000_000,
            Some(remaining),
            &terminal,
            Instant::now() + Duration::from_secs(2),
            &cancel,
        )
        .unwrap();
        assert!(forward_accepted_retained_teletext_output(
            &mut reader,
            &mut output,
            &command(),
            &selected,
            &fixture.state.application,
            epoch,
        )
        .await
        .is_err());
        assert!(
            output.is_empty(),
            "unsuccessful/premature EOF released data"
        );
        assert_eq!(reader.position(), u64::try_from(source().len()).unwrap());
    }
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn accepted_intent_loader_bounds_waiting_sqlite_and_recovers_unchanged_generation() {
    let (fixture, _, pending) = prepared().await;
    let candidate = Arc::new(accepted(accept(&fixture, &pending, 1, None).await));
    let source_policy = intent(Arc::clone(&candidate)).policy();
    let invoked = calls(&fixture);
    let config = designs::config(&fixture).await;
    // The fixture intentionally has one pool connection. Holding it proves
    // the loader bounds the awaited repository call, not only entry checks.
    let held = fixture.state.pool.acquire().await.unwrap();
    let result = tokio::time::timeout(
        Duration::from_millis(500),
        load_accepted_teletext_output_intent(
            &fixture.state.application,
            1,
            designs::budget(),
            channel_documents::policy(),
            source_policy,
            std::time::Instant::now() + Duration::from_millis(20),
        ),
    )
    .await
    .expect("loader waited beyond its absolute bound");
    assert!(matches!(
        result,
        Err(AcceptedTeletextIntentLoadError::TimedOut)
    ));
    drop(held);
    let recovered = load_accepted_teletext_output_intent(
        &fixture.state.application,
        1,
        designs::budget(),
        channel_documents::policy(),
        source_policy,
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_same_output(recovered.candidate(), &candidate);
    assert_eq!(calls(&fixture), invoked);
    assert_eq!(designs::config(&fixture).await, config);
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn opaque_direct_command_preserves_raw_playback_without_waiting_for_sqlite() {
    let (fixture, _, pending) = prepared().await;
    let selected = intent(Arc::new(accepted(
        accept(&fixture, &pending, 1, None).await,
    )));
    let generated = command();
    let opaque = FfmpegCommand::new_with_executable(
        generated.executable,
        generated.environment,
        generated.arguments,
    );
    assert!(opaque.mpegts_output_policy().is_none());
    let invoked = calls(&fixture);
    let config = designs::config(&fixture).await;
    let bytes = source();
    let mut reader = Cursor::new(bytes.clone());
    let mut output = Vec::new();
    // Unqualified commands cannot carry authored output. Holding the sole pool
    // connection exposes any needless current-generation query on this path.
    let held = fixture.state.pool.acquire().await.unwrap();
    let result = tokio::time::timeout(
        Duration::from_millis(500),
        forward_accepted_teletext_output(
            &mut reader,
            &mut output,
            &opaque,
            &selected,
            &fixture.state.application,
            Instant::now() + Duration::from_secs(2),
        ),
    )
    .await
    .expect("opaque ordinary playback waited for unavailable SQLite")
    .unwrap();
    assert!(result.rejection.is_some());
    assert_eq!(result.bytes, u64::try_from(bytes.len()).unwrap());
    assert_eq!(reader.position(), result.bytes);
    assert_eq!(output, bytes);
    drop(held);
    assert_eq!(calls(&fixture), invoked);
    assert_eq!(designs::config(&fixture).await, config);
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn retained_unqualified_command_or_policy_reaches_source_without_waiting_for_sqlite() {
    let (fixture, _, pending) = prepared().await;
    let candidate = Arc::new(accepted(accept(&fixture, &pending, 1, None).await));
    let invoked = calls(&fixture);
    let config = designs::config(&fixture).await;
    let held = fixture.state.pool.acquire().await.unwrap();
    for opaque in [true, false] {
        let selected = if opaque {
            retained_intent(Arc::clone(&candidate))
        } else {
            intent(Arc::clone(&candidate)) // Too small for the unchanged reserve.
        };
        let generated = command();
        let generated = if opaque {
            FfmpegCommand::new_with_executable(
                generated.executable,
                generated.environment,
                generated.arguments,
            )
        } else {
            generated
        };
        let entered = Notify::new();
        let mut reader = ObservedSource {
            prefix: Cursor::new(Vec::new()),
            entered: &entered,
            polls: 0,
            tail: SourceTail::Pending,
        };
        let mut output = Vec::new();
        let terminal = AtomicU8::new(1);
        let cancel = Notify::new();
        let epoch = AcceptedRetainedTeletextEpoch::new(
            5_000_000,
            Some(Duration::from_millis(3200)),
            &terminal,
            Instant::now() + Duration::from_secs(2),
            &cancel,
        )
        .unwrap();
        let operation = forward_accepted_retained_teletext_output(
            &mut reader,
            &mut output,
            &generated,
            &selected,
            &fixture.state.application,
            epoch,
        );
        let control = async {
            tokio::time::timeout(Duration::from_millis(500), entered.notified())
                .await
                .expect("unqualified ordinary reserve waited for unavailable SQLite");
            cancel.notify_one();
        };
        let (result, ()) = tokio::join!(operation, control);
        assert!(result.unwrap_err().to_string().contains("cancelled"));
        assert!(
            reader.polls > 0,
            "opaque={opaque} never reached ordinary source"
        );
        assert!(output.is_empty());
    }
    drop(held);
    assert_eq!(calls(&fixture), invoked);
    assert_eq!(designs::config(&fixture).await, config);
    fixture.state.pool.close().await;
}
