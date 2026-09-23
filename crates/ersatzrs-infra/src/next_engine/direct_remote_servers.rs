//! Bounded synthetic HTTP origin and pinned external RTSP fixture ownership.
use super::*;
use tokio::{
    io::AsyncSeekExt as _,
    net::{TcpListener, TcpStream},
    sync::oneshot,
};

pub(super) struct Http {
    pub uri: String,
    cancel: Option<oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<()>,
}

impl Http {
    pub async fn start(path: PathBuf) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let uri = format!("http://{}/source.mkv", listener.local_addr().unwrap());
        let (cancel, mut cancelled) = oneshot::channel();
        let task = tokio::spawn(async move {
            let mut requests = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    _ = &mut cancelled => break,
                    accepted = listener.accept(), if requests.len() < 8 => {
                        let Ok((socket, _)) = accepted else { break; };
                        let path = path.clone();
                        requests.spawn(async move { let _ = timeout(Duration::from_secs(30), serve(socket, path)).await; });
                    }
                    _ = requests.join_next(), if !requests.is_empty() => {}
                }
            }
            requests.abort_all();
            while requests.join_next().await.is_some() {}
        });
        Self {
            uri,
            cancel: Some(cancel),
            task,
        }
    }

    pub async fn stop(mut self) {
        let _ = self.cancel.take().unwrap().send(());
        timeout(Duration::from_secs(5), &mut self.task)
            .await
            .unwrap()
            .unwrap();
    }
}

impl Drop for Http {
    fn drop(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            let _ = cancel.send(());
        }
        self.task.abort();
    }
}

async fn serve(mut socket: TcpStream, path: PathBuf) -> std::io::Result<()> {
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
        if header.len() >= 8192 {
            return Err(std::io::ErrorKind::InvalidData.into());
        }
        header.push(socket.read_u8().await?);
    }
    let request = std::str::from_utf8(&header).map_err(|_| std::io::ErrorKind::InvalidData)?;
    let head = request.starts_with("HEAD /source.mkv HTTP/");
    if !head && !request.starts_with("GET /source.mkv HTTP/") {
        return Ok(());
    }
    let mut file = tokio::fs::File::open(path).await?;
    let size = file.metadata().await?.len();
    let range = request
        .split("\r\n")
        .filter_map(|line| line.split_once(':'))
        .find(|(key, _)| key.eq_ignore_ascii_case("range"))
        .map(|(_, value)| value.trim());
    let (start, end) = if let Some(value) = range {
        let (start, end) = value
            .strip_prefix("bytes=")
            .and_then(|v| v.split_once('-'))
            .ok_or(std::io::ErrorKind::InvalidData)?;
        let start = start
            .parse::<u64>()
            .map_err(|_| std::io::ErrorKind::InvalidData)?;
        let end = if end.is_empty() {
            size - 1
        } else {
            end.parse::<u64>()
                .map_err(|_| std::io::ErrorKind::InvalidData)?
                .min(size - 1)
        };
        if start > end {
            return Err(std::io::ErrorKind::InvalidData.into());
        }
        (start, end)
    } else {
        (0, size - 1)
    };
    let status = if range.is_some() {
        "206 Partial Content"
    } else {
        "200 OK"
    };
    let range_header = if range.is_some() {
        format!("Content-Range: bytes {start}-{end}/{size}\r\n")
    } else {
        String::new()
    };
    socket.write_all(format!("HTTP/1.1 {status}\r\nContent-Type: video/x-matroska\r\nAccept-Ranges: bytes\r\n{range_header}Content-Length: {}\r\nConnection: close\r\n\r\n",end-start+1).as_bytes()).await?;
    if !head {
        file.seek(std::io::SeekFrom::Start(start)).await?;
        tokio::io::copy(&mut file.take(end - start + 1), &mut socket).await?;
    }
    socket.shutdown().await
}

pub(super) struct Rtsp {
    pub uri: String,
    address: std::net::SocketAddr,
    server: crate::addons::ManagedAddonChild,
    publisher: crate::addons::ManagedAddonChild,
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
        let mut server = spawn_managed_addon_process(command).unwrap();
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
        let mut publisher = spawn_managed_addon_process(command).unwrap();
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

async fn reap(child: &mut crate::addons::ManagedAddonChild) {
    child.start_kill().unwrap();
    timeout(Duration::from_secs(10), child.wait())
        .await
        .unwrap()
        .unwrap();
}
