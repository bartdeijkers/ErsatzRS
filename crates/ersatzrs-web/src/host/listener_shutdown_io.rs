//! Cancel owned TCP I/O after the existing host shutdown grace, without changing
//! HTTP frames or normal streaming. No detached connection tasks or socket clones.
use std::{
    future::Future,
    io,
    net::SocketAddr,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::{TcpListener, TcpStream},
};
use tokio_util::sync::{CancellationToken, WaitForCancellationFutureOwned};

pub(super) struct ShutdownListener {
    listener: TcpListener,
    force: CancellationToken,
}

impl ShutdownListener {
    pub(super) fn new(listener: TcpListener, force: CancellationToken) -> Self {
        Self { listener, force }
    }
}

impl axum::serve::Listener for ShutdownListener {
    type Io = ShutdownStream;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        // Preserve Axum's existing accept-error logging/retry behavior.
        let (stream, address) = axum::serve::Listener::accept(&mut self.listener).await;
        let io = ShutdownStream {
            stream,
            force: Box::pin(self.force.clone().cancelled_owned()),
            closed: false,
        };
        (io, address)
    }

    fn local_addr(&self) -> io::Result<Self::Addr> {
        self.listener.local_addr()
    }
}

pub(super) struct ShutdownStream {
    stream: TcpStream,
    force: Pin<Box<WaitForCancellationFutureOwned>>,
    closed: bool,
}

impl ShutdownStream {
    fn check(&mut self, cx: &mut Context<'_>) -> io::Result<()> {
        // Register the actual connection waker even when I/O is ready. A peer
        // that stops reading must not strand Hyper in a blocked write/flush.
        if !self.closed && self.force.as_mut().poll(cx).is_ready() {
            self.closed = true;
        }
        if self.closed {
            Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "HTTP connection exceeded the host shutdown grace",
            ))
        } else {
            Ok(())
        }
    }
}

impl AsyncRead for ShutdownStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        self.check(cx)?;
        Pin::new(&mut self.stream).poll_read(cx, buf)
    }
}

impl AsyncWrite for ShutdownStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.check(cx)?;
        Pin::new(&mut self.stream).poll_write(cx, buf)
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        self.check(cx)?;
        Pin::new(&mut self.stream).poll_write_vectored(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        self.stream.is_write_vectored()
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.check(cx)?;
        Pin::new(&mut self.stream).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.check(cx)?;
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}
