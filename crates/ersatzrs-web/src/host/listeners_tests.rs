use super::*;

fn test_state() -> AppState {
    AppState::for_tests(super::super::state::IptvAccessTokenVerifier::disabled())
}

#[tokio::test]
async fn shared_listener_configuration_does_not_bind_the_same_socket_twice() {
    let probe = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = probe.local_addr().unwrap();
    drop(probe);
    let listeners = BoundListeners::bind(address, Some(address)).await.unwrap();
    assert!(listeners.streaming.is_none());
    assert_eq!(listeners.admin_addr, listeners.streaming_addr);
    assert!(listeners.internal_addr.ip().is_loopback());
    assert_ne!(listeners.internal_addr, listeners.admin_addr);
}

#[tokio::test]
async fn failed_streaming_bind_releases_the_new_admin_socket() {
    let occupied = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let probe = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let admin = probe.local_addr().unwrap();
    drop(probe);
    let result = BoundListeners::bind(admin, Some(occupied.local_addr().unwrap())).await;
    assert!(result.is_err());
    let rebound = TcpListener::bind(admin).await.unwrap();
    assert_eq!(rebound.local_addr().unwrap(), admin);
    // The listener that caused the collision is still owned by the caller.
    assert!(
        tokio::net::TcpStream::connect(occupied.local_addr().unwrap())
            .await
            .is_ok()
    );
}

// The public SSR 404 fallback loads locale settings with block_in_place, as on
// the production multithreaded runtime. LocalSet still owns Leptos local tasks.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_ipv4_and_ipv6_listeners_isolate_routes_and_shut_down_together() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for (address, separate) in [
                ("127.0.0.1:0", false),
                ("127.0.0.1:0", true),
                ("[::1]:0", false),
                ("[::1]:0", true),
            ] {
                let address = address.parse().unwrap();
                let listeners = BoundListeners::bind(address, separate.then_some(address))
                    .await
                    .unwrap();
                let admin = listeners.admin_addr;
                let streaming = listeners.streaming_addr;
                let internal = listeners.internal_addr;
                assert_eq!(admin != streaming, separate);
                assert!(internal.ip().is_loopback());
                let state = test_state();
                state.internal_http_addr.set(internal).unwrap();
                state
                    .streaming_http_addr
                    .set(local_http_addr(streaming))
                    .unwrap();
                state.next_engine.media.configure(internal).unwrap();
                let shutdown = CancellationToken::new();
                let _guard = shutdown.clone().drop_guard();
                let server = tokio::task::spawn_local(listeners.serve(state, shutdown.clone()));
                let client = reqwest::Client::builder().no_proxy().build().unwrap();
                for (listener, path, status) in [
                    (admin, "/healthz", 200),
                    (admin, "/lineup_status.json", 200),
                    (streaming, "/healthz", if separate { 404 } else { 200 }),
                    (admin, "/changelog.md", 200),
                    (streaming, "/changelog.md", if separate { 404 } else { 200 }),
                    (streaming, "/lineup_status.json", 200),
                    (
                        streaming,
                        "/admin-api/settings/ui/save",
                        if separate { 404 } else { 405 },
                    ),
                    (streaming, "/ffmpeg/next-media/1/2/3", 404),
                    (admin, "/ffmpeg/next-media/1/2/3", 404),
                    (internal, "/healthz", 404),
                    (internal, "/lineup_status.json", 404),
                    (internal, "/ffmpeg/next-media/1/2/3", 401),
                    (
                        streaming,
                        "/ffmpeg/next-song-background?binding=invalid",
                        404,
                    ),
                    (admin, "/ffmpeg/next-song-background?binding=invalid", 404),
                    (
                        internal,
                        "/ffmpeg/next-song-background?binding=invalid",
                        401,
                    ),
                    (streaming, "/ffmpeg/external-media/1/2", 404),
                    (admin, "/ffmpeg/external-media/1/2", 404),
                    (internal, "/ffmpeg/external-media/1/2", 401),
                ] {
                    let response = client
                        .get(format!("http://{listener}{path}"))
                        .header("host", "localhost")
                        .header("forwarded", "for=127.0.0.1;host=localhost")
                        .header("x-forwarded-for", "127.0.0.1")
                        .send()
                        .await
                        .unwrap();
                    assert_eq!(response.status().as_u16(), status, "{listener}{path}");
                }
                shutdown.cancel();
                tokio::time::timeout(std::time::Duration::from_secs(5), server)
                    .await
                    .unwrap()
                    .unwrap()
                    .unwrap();
                for address in [admin, streaming, internal] {
                    assert!(
                        tokio::net::TcpStream::connect(address).await.is_err(),
                        "listener remained open: {address}"
                    );
                }
            }
        })
        .await;
}

#[test]
fn self_request_origins_preserve_ports_and_concrete_interfaces() {
    for (bound, origin) in [
        ("0.0.0.0:8410", "127.0.0.1:8410"),
        ("[::]:8411", "[::1]:8411"),
        ("192.0.2.7:8412", "192.0.2.7:8412"),
        ("[2001:db8::7]:8413", "[2001:db8::7]:8413"),
    ] {
        assert_eq!(
            super::local_http_addr(bound.parse().unwrap()),
            origin.parse().unwrap()
        );
    }
}

#[tokio::test]
async fn idle_tcp_preconnection_does_not_block_graceful_shutdown() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let shutdown = CancellationToken::new();
    let _guard = shutdown.clone().drop_guard();
    let app = Router::new().route("/healthz", axum::routing::get(|| async { "ok" }));
    let mut server = tokio::spawn(serve_one(listener, app, shutdown.clone()));
    // A second actual response fences acceptance of the earlier established
    // connection without inventing an HTTP request on that quiet socket.
    let quiet = tokio::net::TcpStream::connect(address).await.unwrap();
    let response = reqwest::Client::builder()
        .no_proxy()
        .build()
        .unwrap()
        .get(format!("http://{address}/healthz"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.text().await.unwrap(), "ok");
    shutdown.cancel();
    let stopped = tokio::time::timeout(std::time::Duration::from_secs(5), &mut server).await;
    // Even the negative path closes our socket and joins its server before
    // reporting failure; no detached test owner or lingering listener remains.
    drop(quiet);
    if let Ok(result) = stopped {
        result.unwrap().unwrap();
    } else {
        tokio::time::timeout(std::time::Duration::from_secs(5), server)
            .await
            .expect("server must settle after its owned quiet client closes")
            .unwrap()
            .unwrap();
        panic!("quiet TCP preconnection blocked graceful shutdown");
    }
}

#[tokio::test]
async fn unread_response_does_not_hold_host_shutdown_indefinitely() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let shutdown = CancellationToken::new();
    let _guard = shutdown.clone().drop_guard();
    let app = Router::new().route(
        "/asset",
        axum::routing::get(|| async { vec![b'x'; 32 * 1024 * 1024] }),
    );
    let mut server = tokio::spawn(serve_one(listener, app, shutdown.clone()));
    let mut client = tokio::net::TcpStream::connect(address).await.unwrap();
    client
        .write_all(b"GET /asset HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .await
        .unwrap();
    let mut first = [0; 128];
    let count = client.read(&mut first).await.unwrap();
    assert!(first[..count].starts_with(b"HTTP/1.1 200"));
    shutdown.cancel();
    let stopped = tokio::time::timeout(std::time::Duration::from_secs(15), &mut server).await;
    drop(client);
    if let Ok(result) = stopped {
        result.unwrap().unwrap();
    } else {
        tokio::time::timeout(std::time::Duration::from_secs(5), server)
            .await
            .expect("server must settle after its owned unread client closes")
            .unwrap()
            .unwrap();
        panic!("unread response blocked bounded host shutdown");
    }
}

#[tokio::test]
async fn complete_response_preserves_bytes_and_real_peer_before_shutdown() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    for address in ["127.0.0.1:0", "[::1]:0"] {
        let listener = TcpListener::bind(address).await.unwrap();
        let address = listener.local_addr().unwrap();
        let shutdown = CancellationToken::new();
        let _guard = shutdown.clone().drop_guard();
        let app = Router::new().route(
            "/asset",
            axum::routing::get(|ConnectInfo(peer): ConnectInfo<SocketAddr>| async move {
                ([("x-peer", peer.to_string())], vec![0xab; 512 * 1024])
            }),
        );
        let server = tokio::spawn(serve_one(listener, app, shutdown.clone()));
        let mut client = tokio::net::TcpStream::connect(address).await.unwrap();
        let peer = client.local_addr().unwrap();
        client
            .write_all(b"GET /asset HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nX-Forwarded-For: 192.0.2.7\r\n\r\n")
            .await
            .unwrap();
        let mut response = Vec::new();
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            client.read_to_end(&mut response),
        )
        .await
        .unwrap()
        .unwrap();
        let split = response.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
        let headers = std::str::from_utf8(&response[..split]).unwrap();
        assert!(headers.starts_with("HTTP/1.1 200"));
        assert!(headers
            .lines()
            .any(|line| line == format!("x-peer: {peer}")));
        assert_eq!(&response[split + 4..], vec![0xab; 512 * 1024]);
        shutdown.cancel();
        tokio::time::timeout(std::time::Duration::from_secs(5), server)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
}

#[tokio::test]
async fn pending_response_producer_does_not_hold_host_shutdown_indefinitely() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let shutdown = CancellationToken::new();
    let _guard = shutdown.clone().drop_guard();
    let release = CancellationToken::new();
    let producer_release = release.clone();
    let app = Router::new().route(
        "/pending",
        axum::routing::get(move || {
            let release = producer_release.clone();
            async move {
                axum::body::Body::from_stream(futures_util::stream::once(async move {
                    release.cancelled().await;
                    Ok::<_, std::convert::Infallible>(axum::body::Bytes::from_static(b"released"))
                }))
            }
        }),
    );
    let mut server = tokio::spawn(serve_one(listener, app, shutdown.clone()));
    let mut client = tokio::net::TcpStream::connect(address).await.unwrap();
    client
        .write_all(b"GET /pending HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .await
        .unwrap();
    let mut first = [0; 13];
    client.read_exact(&mut first).await.unwrap();
    assert_eq!(&first, b"HTTP/1.1 200 ");
    shutdown.cancel();
    let stopped = tokio::time::timeout(std::time::Duration::from_secs(15), &mut server).await;
    let timed_out = stopped.is_err();
    if timed_out {
        release.cancel();
    }
    drop(client);
    if let Ok(result) = stopped {
        result.unwrap().unwrap();
    } else {
        tokio::time::timeout(std::time::Duration::from_secs(5), server)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        panic!("pending response producer blocked bounded host shutdown");
    }
    assert!(!release.is_cancelled());
}
