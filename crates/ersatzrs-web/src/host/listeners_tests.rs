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

#[tokio::test(flavor = "current_thread")]
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
