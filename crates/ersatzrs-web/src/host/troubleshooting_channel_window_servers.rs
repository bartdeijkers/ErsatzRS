//! Exact existing bounded MediaMTX/publisher runtime fixture; only crate namespaces differ.
use super::*;
use tokio::net::{TcpListener, TcpStream};

pub(super) struct Rtsp {
    pub uri: String,
    address: std::net::SocketAddr,
    server: ManagedProcess,
    publisher: ManagedProcess,
}

impl Rtsp {
    pub async fn start(ffmpeg: &Path, source: &Path, root: &Path) -> Self {
        let reserved = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = reserved.local_addr().unwrap();
        let config = root.join("mediamtx.yml");
        tokio::fs::write(&config, format!("logLevel: info\nlogDestinations: [stdout]\nrtsp: true\nrtspTransports: [tcp]\nrtspAddress: {address}\nrtmp: false\nhls: false\nwebrtc: false\nsrt: false\nmoq: false\napi: false\nmetrics: false\npprof: false\nplayback: false\npaths:\n  fixture:\n    source: publisher\n")).await.unwrap();
        let log = root.join("mediamtx.log");
        let mut command = Command::new(runtime_path("ERSATZRS_NEXT_TEST_RTSP_SERVER"));
        command
            .arg(config)
            .current_dir(root)
            .stdin(Stdio::null())
            .stdout(std::fs::File::create(&log).unwrap())
            .stderr(Stdio::null());
        drop(reserved);
        let mut server = ManagedProcess::spawn(command).unwrap();
        if timeout(Duration::from_secs(10), async {
            loop {
                if TcpStream::connect(address).await.is_ok() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .is_err()
        {
            reap(&mut server).await;
            panic!("isolated RTSP listener did not become ready");
        }
        let uri = format!("rtsp://{address}/fixture");
        let mut command = Command::new(ffmpeg);
        command
            .args(["-v", "error", "-nostdin", "-re", "-stream_loop", "-1", "-i"])
            .arg(source)
            .args([
                "-map",
                "0",
                "-c",
                "copy",
                "-f",
                "rtsp",
                "-rtsp_transport",
                "tcp",
            ])
            .arg(&uri)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(std::fs::File::create(root.join("publisher.log")).unwrap());
        let mut publisher = match ManagedProcess::spawn(command) {
            Ok(publisher) => publisher,
            Err(error) => {
                reap(&mut server).await;
                panic!("RTSP publisher spawn failed: {error}");
            }
        };
        // Pinned MediaMTX announces actual publisher admission and track count.
        let ready = timeout(Duration::from_secs(10), async {
            loop {
                let bytes = tokio::fs::read(&log).await.unwrap();
                assert!(bytes.len() < 64 * 1024);
                let log = String::from_utf8_lossy(&bytes);
                if log.contains("[path fixture] stream is available and online, 3 tracks (H264, MPEG-4 Audio, MPEG-4 Audio)")
                    && log.contains("is publishing to path 'fixture'") {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await;
        if ready.is_err() {
            reap(&mut publisher).await;
            reap(&mut server).await;
            let publisher_log = tokio::fs::read_to_string(root.join("publisher.log"))
                .await
                .unwrap();
            let server_log = tokio::fs::read_to_string(&log).await.unwrap();
            assert!(publisher_log.len() < 64 * 1024 && server_log.len() < 64 * 1024);
            panic!("isolated RTSP publisher did not announce three tracks: {publisher_log}; {server_log}");
        }
        Self {
            uri,
            address,
            server,
            publisher,
        }
    }

    pub async fn stop(mut self) {
        reap(&mut self.publisher).await;
        reap(&mut self.server).await;
        assert!(
            TcpStream::connect(self.address).await.is_err(),
            "RTSP listener must close after cleanup"
        );
    }
}

async fn reap(child: &mut ManagedProcess) {
    timeout(Duration::from_secs(10), child.kill())
        .await
        .unwrap()
        .unwrap();
}
