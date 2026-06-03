use std::net::{IpAddr, SocketAddr};
use std::sync::{
    atomic::{AtomicI32, Ordering},
    Arc,
};
use std::time::Duration;

use tokio::net::UdpSocket;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::protocol::{
    decode_discovery_request, derive_device_id_from_uuid, encode_discovery_reply, should_reply,
    DEVICE_TYPE_TUNER, MAX_PACKET_SIZE, UDP_DISCOVERY_PORT,
};

const BIND_ADDR: &str = "0.0.0.0";
const BIND_RETRY_DELAY: Duration = Duration::from_secs(5);

pub async fn run_udp_discovery(
    device_uuid: Uuid,
    tuner_count: Arc<AtomicI32>,
    listen_port: u16,
    shutdown: CancellationToken,
) {
    let device_id = derive_device_id_from_uuid(device_uuid);

    while !shutdown.is_cancelled() {
        match UdpSocket::bind((BIND_ADDR, UDP_DISCOVERY_PORT)).await {
            Ok(socket) => {
                tracing::info!(
                    port = UDP_DISCOVERY_PORT,
                    device_id = format_args!("{device_id:08x}"),
                    "HDHomeRun UDP discovery responder ready"
                );
                serve_socket(
                    &socket,
                    device_id,
                    tuner_count.clone(),
                    listen_port,
                    &shutdown,
                )
                .await;
            }
            Err(error) => {
                tracing::warn!(%error, "HDHomeRun UDP bind failed; retrying");
                tokio::select! {
                    () = shutdown.cancelled() => break,
                    () = tokio::time::sleep(BIND_RETRY_DELAY) => {}
                }
            }
        }
    }

    tracing::debug!("HDHomeRun UDP discovery responder stopped");
}

async fn serve_socket(
    socket: &UdpSocket,
    device_id: u32,
    tuner_count: Arc<AtomicI32>,
    listen_port: u16,
    shutdown: &CancellationToken,
) {
    let mut buffer = vec![0u8; MAX_PACKET_SIZE];
    loop {
        let (length, peer) = tokio::select! {
            () = shutdown.cancelled() => return,
            result = socket.recv_from(&mut buffer) => match result {
                Ok(pair) => pair,
                Err(error) => {
                    tracing::warn!(%error, "HDHomeRun UDP recv_from failed");
                    return;
                }
            },
        };

        let request = match decode_discovery_request(&buffer[..length]) {
            Ok(request) => request,
            Err(error) => {
                tracing::debug!(%error, %peer, "discarding HDHomeRun UDP packet");
                continue;
            }
        };

        if !should_reply(&request, device_id, DEVICE_TYPE_TUNER) {
            continue;
        }

        let our_ip = local_ip_toward(peer)
            .await
            .unwrap_or(IpAddr::from([0, 0, 0, 0]));
        let base_url = format!("http://{our_ip}:{listen_port}");
        let lineup_url = format!("{base_url}/lineup.json");
        let tuner_count_byte = u8::try_from(
            tuner_count
                .load(Ordering::Relaxed)
                .clamp(1, i32::from(u8::MAX)),
        )
        .unwrap_or(1);
        let reply = encode_discovery_reply(
            DEVICE_TYPE_TUNER,
            device_id,
            tuner_count_byte,
            &base_url,
            &lineup_url,
        );

        if let Err(error) = socket.send_to(&reply, peer).await {
            tracing::warn!(%error, %peer, "HDHomeRun UDP reply failed");
        } else {
            tracing::debug!(%peer, "answered HDHomeRun discovery");
        }
    }
}

async fn local_ip_toward(peer: SocketAddr) -> Option<IpAddr> {
    let probe = UdpSocket::bind("0.0.0.0:0").await.ok()?;
    probe.connect(peer).await.ok()?;
    probe.local_addr().ok().map(|addr| addr.ip())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// An already-cancelled token must make the responder return before it even
    /// binds the socket (the outer-loop guard).
    #[tokio::test]
    async fn stops_when_token_already_cancelled() {
        let token = CancellationToken::new();
        token.cancel();

        let result = tokio::time::timeout(
            Duration::from_secs(1),
            run_udp_discovery(Uuid::new_v4(), Arc::new(AtomicI32::new(2)), 8409, token),
        )
        .await;

        assert!(
            result.is_ok(),
            "discovery loop did not exit when the token was already cancelled"
        );
    }

    /// Cancelling after the responder is running must unblock the `recv_from`
    /// (or bind-retry) await so the task exits promptly. Robust even if the
    /// discovery port is unavailable: the bind-retry branch also selects on
    /// cancellation.
    #[tokio::test]
    async fn stops_when_token_cancelled_mid_run() {
        let token = CancellationToken::new();
        let task = tokio::spawn(run_udp_discovery(
            Uuid::new_v4(),
            Arc::new(AtomicI32::new(2)),
            8409,
            token.clone(),
        ));

        tokio::time::sleep(Duration::from_millis(100)).await;
        token.cancel();

        let joined = tokio::time::timeout(Duration::from_secs(2), task).await;
        assert!(
            joined.is_ok(),
            "discovery loop did not exit promptly after cancellation"
        );
        assert!(joined.unwrap().is_ok(), "discovery task panicked");
    }
}
