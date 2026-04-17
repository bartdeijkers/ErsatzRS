use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use tokio::net::UdpSocket;
use uuid::Uuid;

use super::protocol::{
    decode_discovery_request, derive_device_id_from_uuid, encode_discovery_reply, should_reply,
    DEVICE_TYPE_TUNER, MAX_PACKET_SIZE, UDP_DISCOVERY_PORT,
};

const BIND_ADDR: &str = "0.0.0.0";
const BIND_RETRY_DELAY: Duration = Duration::from_secs(5);

pub async fn run_udp_discovery(device_uuid: Uuid, tuner_count: i32, listen_port: u16) {
    let device_id = derive_device_id_from_uuid(device_uuid);
    let tuner_count_byte = u8::try_from(tuner_count.clamp(0, i32::from(u8::MAX))).unwrap_or(1);

    loop {
        match UdpSocket::bind((BIND_ADDR, UDP_DISCOVERY_PORT)).await {
            Ok(socket) => {
                tracing::info!(
                    port = UDP_DISCOVERY_PORT,
                    device_id = format_args!("{device_id:08x}"),
                    "HDHomeRun UDP discovery responder ready"
                );
                serve_socket(&socket, device_id, tuner_count_byte, listen_port).await;
            }
            Err(error) => {
                tracing::warn!(%error, "HDHomeRun UDP bind failed; retrying");
                tokio::time::sleep(BIND_RETRY_DELAY).await;
            }
        }
    }
}

async fn serve_socket(socket: &UdpSocket, device_id: u32, tuner_count: u8, listen_port: u16) {
    let mut buffer = vec![0u8; MAX_PACKET_SIZE];
    loop {
        let (length, peer) = match socket.recv_from(&mut buffer).await {
            Ok(pair) => pair,
            Err(error) => {
                tracing::warn!(%error, "HDHomeRun UDP recv_from failed");
                return;
            }
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
        let reply = encode_discovery_reply(
            DEVICE_TYPE_TUNER,
            device_id,
            tuner_count,
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
