//! Original source-arrival admission; no route/publication or client proof.

use super::tests::{authored_set, fixture, flatten, policy};
use super::*;
use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::ReadBuf;

struct Source {
    bytes: std::io::Cursor<Vec<u8>>,
    polls: usize,
    error: Option<io::ErrorKind>,
}
impl Source {
    fn new(bytes: Vec<u8>) -> Self {
        Self {
            bytes: std::io::Cursor::new(bytes),
            polls: 0,
            error: None,
        }
    }
}
impl AsyncRead for Source {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        output: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        this.polls += 1;
        if let Some(error) = this.error {
            return Poll::Ready(Err(error.into()));
        }
        Pin::new(&mut this.bytes).poll_read(cx, output)
    }
}
#[derive(Default)]
struct Sink {
    bytes: Vec<u8>,
    times: Vec<Instant>,
}
impl AsyncWrite for Sink {
    fn poll_write(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        this.bytes.extend_from_slice(bytes);
        this.times.push(Instant::now());
        Poll::Ready(Ok(bytes.len()))
    }
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}
fn binding() -> TeletextOutputBinding {
    TeletextOutputBinding::new(policy(), authored_set(), Duration::from_millis(40)).unwrap()
}

#[tokio::test(start_paused = true)]
async fn expired_original_hold_is_rejected_without_reading_or_writing() {
    let original = flatten(&fixture(30));
    let epoch = Instant::now();
    let seed = TeletextStartupPrefix::new(original[..188 * 200].to_vec(), epoch, epoch).unwrap();
    tokio::time::advance(policy().max_hold).await;
    let mut source = Source::new(original[188 * 200..].to_vec());
    let mut sink = Sink::default();
    let result = forward_teletext_output_seeded(&mut source, &mut sink, &binding(), seed).await;
    assert!(matches!(
        result,
        Err(TeletextPumpError::Gate(TeletextOutputError::Buffer(
            TeletextBufferError::Deadline
        )))
    ));
    assert_eq!(source.polls, 0);
    assert!(sink.bytes.is_empty());
}

#[tokio::test(start_paused = true)]
async fn original_delivery_deadline_is_not_renewed_even_when_hold_age_still_fits() {
    let original = flatten(&fixture(30));
    let epoch = Instant::now();
    let seed = TeletextStartupPrefix::new(original[..188 * 200].to_vec(), epoch, epoch).unwrap();
    tokio::time::advance(policy().playout_delay + policy().max_lateness + Duration::from_nanos(1))
        .await;
    let mut source = Source::new(original[188 * 200..].to_vec());
    let mut sink = Sink::default();
    let result = forward_teletext_output_seeded(&mut source, &mut sink, &binding(), seed).await;
    assert!(matches!(
        result,
        Err(TeletextPumpError::Gate(TeletextOutputError::Deadline))
    ));
    assert_eq!(source.polls, 0);
    assert!(sink.bytes.is_empty());
}

#[tokio::test(start_paused = true)]
async fn timely_prefix_preserves_original_due_times_all_bytes_and_same_source_continuation() {
    let packets = fixture(30);
    let original = flatten(&packets);
    let mut full_source = std::io::Cursor::new(original.clone());
    let mut full_output = Vec::new();
    let baseline = forward_teletext_output(
        &mut full_source,
        &mut full_output,
        policy(),
        authored_set(),
        Duration::from_millis(40),
    )
    .await
    .unwrap();
    let epoch = Instant::now();
    let first = epoch + Duration::from_millis(10);
    tokio::time::advance(Duration::from_millis(20)).await;
    let prefix = 188 * 200 + 3;
    let seed = TeletextStartupPrefix::new(original[..prefix].to_vec(), epoch, first).unwrap();
    let mut source = Source::new(original[prefix..].to_vec());
    let mut sink = Sink::default();
    let bytes = forward_teletext_output_seeded(&mut source, &mut sink, &binding(), seed)
        .await
        .unwrap();
    assert_eq!(bytes, baseline);
    assert_eq!(sink.bytes, full_output);
    assert_eq!(sink.bytes.len(), original.len());
    assert_eq!(
        source.bytes.position(),
        u64::try_from(original.len() - prefix).unwrap()
    );
    assert_eq!(sink.times[0], first + policy().playout_delay);
    assert!(sink.times.last().unwrap().duration_since(sink.times[0]) >= Duration::from_millis(500));
    for (before, after) in packets.iter().zip(sink.bytes.chunks_exact(188)) {
        if before[1] == 1 && before[2] == 44 {
            assert_eq!(before.as_slice(), after);
        }
    }
}

#[tokio::test(start_paused = true)]
async fn delayed_first_observation_does_not_gain_a_new_first_byte_timeout() {
    let original = flatten(&fixture(3));
    let epoch = Instant::now();
    tokio::time::advance(Duration::from_millis(40)).await;
    let seed = TeletextStartupPrefix::new(original, epoch, Instant::now()).unwrap();
    let mut source = Source::new(Vec::new());
    let mut sink = Sink::default();
    assert!(matches!(
        forward_teletext_output_seeded(&mut source, &mut sink, &binding(), seed).await,
        Err(TeletextPumpError::FirstByteTimeout)
    ));
    assert_eq!(source.polls, 0);
    assert!(sink.bytes.is_empty());
}

#[tokio::test(start_paused = true)]
async fn original_partial_packet_and_real_source_failure_remain_strict() {
    for error in [None, Some(io::ErrorKind::UnexpectedEof)] {
        let epoch = Instant::now();
        let seed = TeletextStartupPrefix::new(vec![0x47], epoch, epoch).unwrap();
        let mut source = Source::new(Vec::new());
        source.error = error;
        let mut sink = Sink::default();
        let result = forward_teletext_output_seeded(&mut source, &mut sink, &binding(), seed).await;
        match error {
            None => assert!(matches!(
                result,
                Err(TeletextPumpError::Gate(TeletextOutputError::Buffer(
                    TeletextBufferError::PartialPacket
                )))
            )),
            Some(_) => assert!(matches!(
                result,
                Err(TeletextPumpError::Read(io::ErrorKind::UnexpectedEof))
            )),
        }
        assert!(source.polls > 0);
        assert!(sink.bytes.is_empty());
    }
}

#[tokio::test(start_paused = true)]
async fn seeded_binding_keeps_initial_control_observation_and_rejected_proposal_nonterminal() {
    use super::super::teletext_binding::{
        authored_tests::authored_service_with_counter, teletext_output_control_channel,
        TeletextOutputTransitionPhase, TeletextOutputTransitionRejection, TeletextOutputUpdate,
    };
    use std::sync::Arc;
    let original = flatten(&fixture(30));
    let selected = TeletextOutputBinding::new_authored_shared(
        policy(),
        Arc::new(authored_service_with_counter(3)),
        Duration::from_millis(40),
    )
    .unwrap();
    let mut source = Source::new(original.clone());
    let mut plain = Vec::new();
    forward_teletext_output_with_content(
        &mut source,
        &mut plain,
        policy(),
        selected.content().clone(),
        Duration::from_millis(40),
    )
    .await
    .unwrap();
    let (sender, control, status) = teletext_output_control_channel();
    sender.send_replace(Some(
        TeletextOutputUpdate::replace(1, Arc::new(authored_service_with_counter(3))).unwrap(),
    ));
    let selected = selected.with_control(control);
    let epoch = Instant::now();
    let prefix = 188 * 200;
    let seed = TeletextStartupPrefix::new(original[..prefix].to_vec(), epoch, epoch).unwrap();
    let mut source = Source::new(original[prefix..].to_vec());
    let mut output = Vec::new();
    let bytes = forward_teletext_output_seeded(&mut source, &mut output, &selected, seed)
        .await
        .unwrap();
    assert_eq!(output, plain);
    assert_eq!(bytes, u64::try_from(original.len()).unwrap());
    assert_eq!(status.borrow().sequence(), Some(1));
    assert_eq!(
        status.borrow().phase(),
        TeletextOutputTransitionPhase::Rejected(TeletextOutputTransitionRejection::ResourceUpdate)
    );
}
