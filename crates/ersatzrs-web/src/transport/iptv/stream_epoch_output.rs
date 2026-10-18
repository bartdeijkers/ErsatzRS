//! One response-owned MPEG-TS epoch join before the existing output pump.

use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

use ersatzrs_ffmpeg::{mpegts_epoch::MpegTsEpochRewriter, FfmpegCommand, MpegTsOutputPolicy};
use tokio::io::{AsyncRead, AsyncWrite, DuplexStream, ReadBuf};

const PACKET_BYTES: usize = 188;

/// Child boundaries belong to the process supervisor, including its retries.
pub(super) trait StreamEpochWrite: Unpin {
    type Sink: AsyncWrite + Unpin;

    fn prepare_epoch(&mut self, command: &FfmpegCommand);
    fn begin_epoch(&mut self) -> io::Result<()>;
    fn finish_epoch(&mut self) -> io::Result<()>;
    fn joins_epochs(&self) -> bool;
    fn sink(&mut self) -> &mut Self::Sink;
    fn source<'a, R: AsyncRead + Unpin>(
        &'a mut self,
        source: &'a mut R,
    ) -> (EpochSource<'a, R>, &'a mut Self::Sink);
}

impl StreamEpochWrite for DuplexStream {
    type Sink = Self;

    fn prepare_epoch(&mut self, _command: &FfmpegCommand) {}

    fn begin_epoch(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn finish_epoch(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn joins_epochs(&self) -> bool {
        false
    }

    fn sink(&mut self) -> &mut Self::Sink {
        self
    }

    fn source<'a, R: AsyncRead + Unpin>(
        &'a mut self,
        source: &'a mut R,
    ) -> (EpochSource<'a, R>, &'a mut Self::Sink) {
        (
            EpochSource {
                source,
                joined: None,
            },
            self,
        )
    }
}

/// Retains one output timeline across the existing finite child processes.
/// The actual sink stays unbuffered by this adapter: a pump's sink flush can
/// acknowledge only bytes whose complete source headers have passed validation.
/// Opaque configured commands retain their existing byte-forwarding behavior.
pub(super) struct EpochStreamWriter<W> {
    inner: W,
    rewriter: MpegTsEpochRewriter,
    enabled: bool,
    active: bool,
    failed: bool,
    next_policy: Option<MpegTsOutputPolicy>,
}

impl<W: AsyncWrite + Unpin> EpochStreamWriter<W> {
    pub(super) fn new(inner: W, enabled: bool) -> Self {
        Self {
            inner,
            rewriter: MpegTsEpochRewriter::default(),
            enabled,
            active: false,
            failed: false,
            next_policy: None,
        }
    }
}

impl<W: AsyncWrite + Unpin> StreamEpochWrite for EpochStreamWriter<W> {
    type Sink = W;

    fn prepare_epoch(&mut self, command: &FfmpegCommand) {
        if self.enabled {
            self.next_policy = command.mpegts_output_policy().cloned();
        }
    }

    fn begin_epoch(&mut self) -> io::Result<()> {
        if !self.enabled {
            return Ok(());
        }
        if self.active || self.failed {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "previous MPEG-TS child epoch did not finish cleanly",
            ));
        }
        let policy = self.next_policy.take().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "MPEG-TS epoch has no sealed generated mux contract",
            )
        })?;
        self.rewriter
            .begin_epoch(policy.muxrate_bps())
            .map_err(epoch_error)?;
        self.active = true;
        Ok(())
    }

    fn finish_epoch(&mut self) -> io::Result<()> {
        if self.enabled && (self.active || self.failed) {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "MPEG-TS child source did not reach a validated EOF",
            ));
        }
        // A joined reader closes the core exactly once, at the real child EOF.
        Ok(())
    }

    fn joins_epochs(&self) -> bool {
        self.enabled
    }

    fn sink(&mut self) -> &mut Self::Sink {
        &mut self.inner
    }

    fn source<'a, R: AsyncRead + Unpin>(
        &'a mut self,
        source: &'a mut R,
    ) -> (EpochSource<'a, R>, &'a mut Self::Sink) {
        let joined = self.enabled.then_some(JoinedEpochSource {
            rewriter: &mut self.rewriter,
            active: &mut self.active,
            failed: &mut self.failed,
            partial: [0; PACKET_BYTES],
            partial_len: 0,
            pending: None,
            pending_offset: 0,
            eof: false,
        });
        (EpochSource { source, joined }, &mut self.inner)
    }
}

pub(super) struct EpochSource<'a, R> {
    source: &'a mut R,
    joined: Option<JoinedEpochSource<'a>>,
}

struct JoinedEpochSource<'a> {
    rewriter: &'a mut MpegTsEpochRewriter,
    active: &'a mut bool,
    failed: &'a mut bool,
    partial: [u8; PACKET_BYTES],
    partial_len: usize,
    pending: Option<[u8; PACKET_BYTES]>,
    pending_offset: usize,
    eof: bool,
}

impl<R: AsyncRead + Unpin> AsyncRead for EpochSource<'_, R> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        target: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        match &mut this.joined {
            None => Pin::new(&mut *this.source).poll_read(cx, target),
            Some(joined) => joined.poll_read(this.source, cx, target),
        }
    }
}

impl JoinedEpochSource<'_> {
    fn poll_read<R: AsyncRead + Unpin>(
        &mut self,
        source: &mut R,
        cx: &mut Context<'_>,
        target: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if target.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }
        if *self.failed || (!*self.active && !self.eof) {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "MPEG-TS child output has no healthy active epoch",
            )));
        }
        loop {
            if self.pending.is_none() {
                self.pending = self.rewriter.pop_ready_packet();
                self.pending_offset = 0;
            }
            if let Some(packet) = &self.pending {
                let count = target.remaining().min(PACKET_BYTES - self.pending_offset);
                target.put_slice(&packet[self.pending_offset..self.pending_offset + count]);
                self.pending_offset += count;
                if self.pending_offset == PACKET_BYTES {
                    self.pending = None;
                }
                return Poll::Ready(Ok(()));
            }
            if self.eof {
                return Poll::Ready(Ok(()));
            }
            let mut input = ReadBuf::new(&mut self.partial[self.partial_len..]);
            match std::task::ready!(Pin::new(&mut *source).poll_read(cx, &mut input)) {
                Ok(()) => {}
                Err(error) => {
                    *self.failed = true;
                    return Poll::Ready(Err(error));
                }
            }
            let count = input.filled().len();
            if count == 0 {
                if self.partial_len != 0 {
                    *self.failed = true;
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "MPEG-TS child ended inside a transport packet",
                    )));
                }
                if let Err(error) = self.rewriter.finish_epoch() {
                    *self.failed = true;
                    return Poll::Ready(Err(epoch_error(error)));
                }
                *self.active = false;
                self.eof = true;
            } else {
                self.partial_len += count;
                if self.partial_len == PACKET_BYTES {
                    if let Err(error) = self.rewriter.push_packet(self.partial) {
                        *self.failed = true;
                        return Poll::Ready(Err(epoch_error(error)));
                    }
                    self.partial_len = 0;
                }
            }
        }
    }
}

fn epoch_error(error: ersatzrs_ffmpeg::mpegts_epoch::MpegTsEpochError) -> io::Error {
    tracing::debug!(%error, "MPEG-TS child epoch join failed");
    io::Error::other(error)
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    // Generated PAT/PMT from the retained synthetic capacity slate plus its
    // PCR in an adaptation-only packet. Contains no media payload or paths.
    const PRELUDE: &[u8; PACKET_BYTES * 3] =
        include_bytes!("../../../tests/fixtures/mpegts-epoch-prelude.bin");

    fn command() -> FfmpegCommand {
        ersatzrs_ffmpeg::build_mpegts_concat_command(
            "http://synthetic/concat",
            &ersatzrs_ffmpeg::OutputMetadata::default(),
            Some(4_953_600),
        )
    }

    #[tokio::test]
    async fn opaque_output_keeps_non_ts_bytes_and_child_boundaries_unchanged() {
        let (sink, mut reader) = tokio::io::duplex(16);
        let mut writer = EpochStreamWriter::new(sink, false);
        for bytes in [&[1, 2, 3][..], &[4, 5][..]] {
            writer.begin_epoch().unwrap();
            let mut input = bytes;
            let (mut source, sink) = writer.source(&mut input);
            tokio::io::copy(&mut source, sink).await.unwrap();
            writer.finish_epoch().unwrap();
        }
        writer.sink().shutdown().await.unwrap();
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await.unwrap();
        assert_eq!(bytes, [1, 2, 3, 4, 5]);
        assert!(!writer.joins_epochs());
    }

    #[tokio::test]
    async fn generated_output_rejects_missing_contract_before_accepting_bytes() {
        let (sink, _reader) = tokio::io::duplex(16);
        let mut writer = EpochStreamWriter::new(sink, true);
        assert_eq!(
            writer.begin_epoch().unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        let mut input = &[0x47][..];
        let (mut source, _) = writer.source(&mut input);
        assert_eq!(
            source.read_u8().await.unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }

    #[tokio::test]
    async fn generated_output_does_not_treat_truncated_packet_as_clean_epoch() {
        let (sink, _reader) = tokio::io::duplex(16);
        let mut writer = EpochStreamWriter::new(sink, true);
        writer.prepare_epoch(&command());
        writer.begin_epoch().unwrap();
        let mut input = &[0x47, 0x40, 0][..];
        let (mut source, _) = writer.source(&mut input);
        assert_eq!(
            source.read_u8().await.unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
        assert_eq!(
            writer.finish_epoch().unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
    }

    #[tokio::test]
    async fn empty_failed_start_can_finish_and_consume_a_new_exact_contract() {
        let (sink, _reader) = tokio::io::duplex(16);
        let mut writer = EpochStreamWriter::new(sink, true);
        writer.prepare_epoch(&command());
        writer.begin_epoch().unwrap();
        let mut input = &[][..];
        let (mut source, _) = writer.source(&mut input);
        assert_eq!(
            source.read_u8().await.unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
        writer.finish_epoch().unwrap();
        assert_eq!(
            writer.begin_epoch().unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        writer.prepare_epoch(&command());
        writer.begin_epoch().unwrap();
    }

    fn packet(pid: u16, counter: u8, start: bool, payload: &[u8]) -> [u8; PACKET_BYTES] {
        assert!(payload.len() < PACKET_BYTES - 5);
        let mut packet = [0xff; PACKET_BYTES];
        packet[0] = 0x47;
        packet[1] = u8::try_from(pid >> 8).unwrap() | if start { 0x40 } else { 0 };
        packet[2] = u8::try_from(pid & 255).unwrap();
        packet[3] = 0x30 | counter;
        let adaptation_len = PACKET_BYTES - 5 - payload.len();
        packet[4] = u8::try_from(adaptation_len).unwrap();
        packet[5] = 0;
        packet[PACKET_BYTES - payload.len()..].copy_from_slice(payload);
        packet
    }

    fn split_headers() -> [([u8; PACKET_BYTES], [u8; PACKET_BYTES]); 2] {
        let pes = [0, 0, 1, 0xe0, 0, 0, 0x80, 0x80, 5, 0x21, 0, 1, 0, 1];
        let pmt = &PRELUDE[PACKET_BYTES..PACKET_BYTES * 2];
        let payload = 5 + usize::from(pmt[4]);
        let section = payload + 1 + usize::from(pmt[payload]);
        let length =
            3 + ((usize::from(pmt[section + 1] & 15) << 8) | usize::from(pmt[section + 2]));
        let mut first_psi = vec![0];
        first_psi.extend_from_slice(&pmt[section..section + 7]);
        [
            (
                packet(256, 0, true, &pes[..7]),
                packet(256, 1, false, &pes[7..]),
            ),
            (
                packet(4096, 1, true, &first_psi),
                packet(4096, 2, false, &pmt[section + 7..section + length]),
            ),
        ]
    }

    #[tokio::test]
    async fn split_pes_and_psi_wait_for_validation_before_sink_flush_and_ack() {
        for (first, last) in split_headers() {
            let (mut input_writer, mut input) = tokio::io::duplex(PRELUDE.len() + PACKET_BYTES * 2);
            input_writer.write_all(PRELUDE).await.unwrap();
            // Split both a physical read and the header across TS packets.
            input_writer.write_all(&first[..97]).await.unwrap();
            let (sink, mut observer) = tokio::io::duplex(PRELUDE.len() + PACKET_BYTES * 2);
            let mut writer = EpochStreamWriter::new(sink, true);
            writer.prepare_epoch(&command());
            writer.begin_epoch().unwrap();
            let (mut source, sink) = writer.source(&mut input);
            let mut prelude = [0; PRELUDE.len()];
            source.read_exact(&mut prelude).await.unwrap();
            sink.write_all(&prelude).await.unwrap();
            sink.flush().await.unwrap();
            observer.read_exact(&mut prelude).await.unwrap();
            let ack = Arc::new(AtomicUsize::new(0));
            let mut probe = [0; PACKET_BYTES];
            {
                let written_ack = ack.clone();
                let forward = async {
                    let mut bytes = [0; PACKET_BYTES];
                    let count = source.read(&mut bytes).await.unwrap();
                    sink.write_all(&bytes[..count]).await.unwrap();
                    sink.flush().await.unwrap();
                    written_ack.store(count, Ordering::SeqCst);
                };
                tokio::pin!(forward);
                assert!(futures_util::poll!(&mut forward).is_pending());
                input_writer.write_all(&first[97..]).await.unwrap();
                assert!(futures_util::poll!(&mut forward).is_pending());
                assert_eq!(ack.load(Ordering::SeqCst), 0);
                let mut read = ReadBuf::new(&mut probe);
                let mut context = Context::from_waker(std::task::Waker::noop());
                assert!(Pin::new(&mut observer)
                    .poll_read(&mut context, &mut read)
                    .is_pending());
                input_writer.write_all(&last).await.unwrap();
                drop(input_writer);
                forward.await;
            }
            assert_eq!(ack.load(Ordering::SeqCst), PACKET_BYTES);
            observer.read_exact(&mut probe).await.unwrap();
            let mut tail = Vec::new();
            source.read_to_end(&mut tail).await.unwrap();
            assert_eq!(tail.len(), PACKET_BYTES);
            writer.finish_epoch().unwrap();
        }
    }

    #[tokio::test]
    async fn cancelling_incomplete_header_does_not_ack_or_finish_an_epoch() {
        for (first, _) in split_headers() {
            let (mut input_writer, mut input) = tokio::io::duplex(PRELUDE.len() + PACKET_BYTES);
            input_writer.write_all(PRELUDE).await.unwrap();
            input_writer.write_all(&first).await.unwrap();
            let (sink, _observer) = tokio::io::duplex(PRELUDE.len() + PACKET_BYTES);
            let mut writer = EpochStreamWriter::new(sink, true);
            writer.prepare_epoch(&command());
            writer.begin_epoch().unwrap();
            let (mut source, sink) = writer.source(&mut input);
            let mut prelude = [0; PRELUDE.len()];
            source.read_exact(&mut prelude).await.unwrap();
            let ack = AtomicUsize::new(0);
            {
                let forward = async {
                    let mut bytes = [0; PACKET_BYTES];
                    let count = source.read(&mut bytes).await.unwrap();
                    sink.write_all(&bytes[..count]).await.unwrap();
                    sink.flush().await.unwrap();
                    ack.store(count, Ordering::SeqCst);
                };
                tokio::pin!(forward);
                assert!(futures_util::poll!(&mut forward).is_pending());
            }
            assert_eq!(ack.load(Ordering::SeqCst), 0);
            assert_eq!(
                writer.finish_epoch().unwrap_err().kind(),
                io::ErrorKind::UnexpectedEof
            );
            assert_eq!(
                writer.begin_epoch().unwrap_err().kind(),
                io::ErrorKind::InvalidData
            );
        }
    }
}
