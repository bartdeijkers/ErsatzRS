//! UCU-07 row 110 / ADR 0102: bind route surfaces before starting host work.
//! Upstream: `ErsatzTV/Startup.cs` and `Program.cs` listener isolation.

use std::{
    io,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
};

use axum::{
    extract::{ConnectInfo, Request},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Router,
};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use super::state::AppState;

pub(crate) struct BoundListeners {
    admin: TcpListener,
    streaming: Option<TcpListener>,
    internal: TcpListener,
    pub(crate) admin_addr: SocketAddr,
    pub(crate) streaming_addr: SocketAddr,
    pub(crate) internal_addr: SocketAddr,
}

impl BoundListeners {
    /// All binds precede host work. An error drops every earlier owned socket.
    pub(crate) async fn bind(
        admin_addr: SocketAddr,
        streaming_addr: Option<SocketAddr>,
    ) -> io::Result<Self> {
        let admin = TcpListener::bind(admin_addr).await?;
        let admin_addr = admin.local_addr()?;
        let streaming = match streaming_addr {
            Some(address) if address != admin_addr => Some(TcpListener::bind(address).await?),
            _ => None,
        };
        let streaming_addr = streaming
            .as_ref()
            .map_or(Ok(admin_addr), TcpListener::local_addr)?;
        let loopback = if admin_addr.is_ipv4() {
            IpAddr::V4(Ipv4Addr::LOCALHOST)
        } else {
            IpAddr::V6(Ipv6Addr::LOCALHOST)
        };
        let internal = TcpListener::bind(SocketAddr::new(loopback, 0)).await?;
        let internal_addr = internal.local_addr()?;
        Ok(Self {
            admin,
            streaming,
            internal,
            admin_addr,
            streaming_addr,
            internal_addr,
        })
    }

    pub(crate) async fn serve(
        self,
        state: AppState,
        shutdown: CancellationToken,
    ) -> io::Result<()> {
        let admin_app = super::router::build_app(state.clone());
        let streaming_app = super::router::build_streaming_app(state.clone());
        let internal_app = super::router::build_internal_app(state);
        let streaming = async {
            if let Some(listener) = self.streaming {
                serve_one(listener, streaming_app, shutdown.clone()).await
            } else {
                Ok(())
            }
        };
        let (admin, streaming, internal) = tokio::join!(
            serve_one(self.admin, admin_app, shutdown.clone()),
            streaming,
            serve_one(self.internal, internal_app, shutdown.clone()),
        );
        admin.and(streaming).and(internal)
    }
}

/// Turn wildcard binds into a reachable local origin without trusting request headers.
pub(crate) fn local_http_addr(mut address: SocketAddr) -> SocketAddr {
    let ip = match address.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(ip) if ip.is_unspecified() => IpAddr::V6(Ipv6Addr::LOCALHOST),
        ip => ip,
    };
    address.set_ip(ip);
    address
}

async fn serve_one(
    listener: TcpListener,
    app: Router,
    shutdown: CancellationToken,
) -> io::Result<()> {
    let result = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown.clone().cancelled_owned())
    .await;
    // A server error must also stop its sibling listeners and background work.
    shutdown.cancel();
    result
}

pub(crate) async fn require_loopback(request: Request, next: Next) -> Response {
    let is_loopback = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .is_some_and(|peer| match peer.0.ip() {
            IpAddr::V4(ip) => ip.is_loopback(),
            IpAddr::V6(ip) => ip
                .to_ipv4_mapped()
                .map_or_else(|| ip.is_loopback(), |ip| ip.is_loopback()),
        });
    if !is_loopback {
        return StatusCode::NOT_FOUND.into_response();
    }
    next.run(request).await
}

#[cfg(test)]
#[path = "listeners_tests.rs"]
mod tests;
