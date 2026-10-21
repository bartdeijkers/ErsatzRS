//! Existing native callback fixture pattern, extended only for retained sources.

use super::*;
use crate::next_engine::{
    media::{ACCESS_HEADER, SESSION_HEADER, SOURCE_BINDING_HEADER},
    protocol::PlayoutItem,
};

#[derive(Default)]
pub(super) struct Events {
    pub items: Vec<PlayoutItem>,
    pub original_reads: usize,
    pub original_full_reads: usize,
    pub retained_reads: usize,
    pub failures: Vec<String>,
    pub presentation_edit_keeps_grant: Vec<bool>,
    pub source_edit_revokes_grant: Vec<bool>,
}

pub(super) struct Listener(tokio::task::JoinHandle<()>);

impl Listener {
    pub async fn stop(self) {
        self.0.abort();
        assert!(self.0.await.unwrap_err().is_cancelled());
    }
}

pub(super) async fn listen(
    host: Arc<host::NextHost>,
    events: Arc<tokio::sync::Mutex<Events>>,
) -> Listener {
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    host.media
        .configure(listener.local_addr().unwrap())
        .unwrap();
    Listener(tokio::spawn(async move {
        let mut connections = tokio::task::JoinSet::new();
        loop {
            let socket = tokio::select! {
                accepted = listener.accept() => accepted.unwrap().0,
                Some(result) = connections.join_next(), if !connections.is_empty() => {
                    result.unwrap();
                    continue;
                }
            };
            let host = host.clone();
            let events = events.clone();
            connections.spawn(async move {
                timeout(
                    Duration::from_secs(35),
                    Box::pin(connection(socket, host, events)),
                )
                .await
                .unwrap();
            });
        }
    }))
}

async fn connection(
    mut socket: tokio::net::TcpStream,
    host: Arc<host::NextHost>,
    events: Arc<tokio::sync::Mutex<Events>>,
) {
    let mut raw = Vec::new();
    while !raw.ends_with(b"\r\n\r\n") {
        assert!(raw.len() < 16 * 1024, "bounded callback request headers");
        match socket.read_u8().await {
            Ok(byte) => raw.push(byte),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::UnexpectedEof
                        | std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::ConnectionAborted
                ) =>
            {
                return
            }
            Err(error) => panic!("read callback headers: {error}"),
        }
    }
    let request = std::str::from_utf8(&raw).unwrap();
    let mut lines = request.split("\r\n");
    let parts = lines.next().unwrap().split(' ').collect::<Vec<_>>();
    assert_eq!(parts.len(), 3);
    assert!(matches!(parts[0], "GET" | "HEAD"));
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .collect::<Vec<_>>();
    let access = values(&headers, ACCESS_HEADER);
    if access.len() != 1 || !host.media.authorized(access[0]) {
        send(&mut socket, "401 Unauthorized", "", 0, &[]).await;
        return;
    }
    if parts[1] == "/ffmpeg/next-media/10/100/1" {
        original(&mut socket, &host, &headers, &events).await;
    } else if let Some(binding) = parts[1].strip_prefix("/ffmpeg/next-source/") {
        retained(&mut socket, &host, binding, parts[0], &headers, &events).await;
    } else if let Some(binding) = parts[1].strip_prefix("/ffmpeg/next-item/") {
        dynamic(&mut socket, &host, binding, &headers, &events).await;
    } else {
        panic!("unexpected native provider callback route");
    }
}

fn values<'a>(headers: &'a [(&str, &str)], name: &str) -> Vec<&'a str> {
    headers
        .iter()
        .filter(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.trim())
        .collect()
}

fn one<'a>(headers: &'a [(&str, &str)], name: &str) -> &'a str {
    let values = values(headers, name);
    assert_eq!(
        values.len(),
        1,
        "exactly one native callback header required"
    );
    values[0]
}

async fn original(
    socket: &mut tokio::net::TcpStream,
    host: &host::NextHost,
    headers: &[(&str, &str)],
    events: &tokio::sync::Mutex<Events>,
) {
    let range = values(headers, "range");
    {
        let mut events = events.lock().await;
        events.original_reads += 1;
        events.original_full_reads += usize::from(range.is_empty());
    }
    let expected = values(headers, SOURCE_BINDING_HEADER);
    let response = host
        .media
        .open_bound(
            &host.pool,
            (10, 100, 1),
            range.first().copied(),
            expected.first().copied(),
        )
        .await
        .unwrap();
    let status = response.status().as_u16();
    let mut extra = String::new();
    for name in ["content-type", "content-range", "accept-ranges"] {
        if let Some(value) = response.headers().get(name) {
            use std::fmt::Write as _;
            write!(extra, "{name}: {}\r\n", value.to_str().unwrap()).unwrap();
        }
    }
    let body = response.bytes().await.unwrap();
    assert!(body.len() <= usize::try_from(SOURCE_LIMIT).unwrap());
    send(
        socket,
        &format!("{status} Media"),
        &extra,
        u64::try_from(body.len()).unwrap(),
        &body,
    )
    .await;
}

async fn retained(
    socket: &mut tokio::net::TcpStream,
    host: &host::NextHost,
    binding: &str,
    method: &str,
    headers: &[(&str, &str)],
    events: &tokio::sync::Mutex<Events>,
) {
    let parts = binding.split('/').collect::<Vec<_>>();
    assert_eq!(parts.len(), 2);
    let instance = parts[0].parse().unwrap();
    let source = parts[1].parse().unwrap();
    let total = host.retained_source_length(instance, source).await.unwrap();
    assert!(total <= SOURCE_LIMIT);
    let ranges = values(headers, "range");
    assert!(ranges.len() <= 1);
    let (offset, length, partial) = range(ranges.first().copied(), total);
    let mut body = Vec::new();
    if method == "GET" {
        let mut reader = host
            .open_retained_source(instance, source, offset, length)
            .await
            .unwrap();
        events.lock().await.retained_reads += 1;
        reader.read_to_end(&mut body).await.unwrap();
        assert_eq!(u64::try_from(body.len()).unwrap(), length);
    }
    let mut extra = "Content-Type: application/octet-stream\r\nAccept-Ranges: bytes\r\n".to_owned();
    if partial {
        use std::fmt::Write as _;
        write!(
            extra,
            "Content-Range: bytes {}-{}/{total}\r\n",
            offset,
            offset + length - 1
        )
        .unwrap();
    }
    send(
        socket,
        if partial {
            "206 Partial Content"
        } else {
            "200 OK"
        },
        &extra,
        length,
        &body,
    )
    .await;
}

fn range(value: Option<&str>, total: u64) -> (u64, u64, bool) {
    let Some(value) = value else {
        return (0, total, false);
    };
    let (start, end) = value
        .strip_prefix("bytes=")
        .unwrap()
        .split_once('-')
        .unwrap();
    let start = start.parse::<u64>().unwrap();
    let end = if end.is_empty() {
        total - 1
    } else {
        end.parse::<u64>().unwrap().min(total - 1)
    };
    assert!(start <= end && end < total);
    (start, end - start + 1, true)
}

async fn dynamic(
    socket: &mut tokio::net::TcpStream,
    host: &host::NextHost,
    binding: &str,
    headers: &[(&str, &str)],
    events: &tokio::sync::Mutex<Events>,
) {
    let parts = binding.split('/').collect::<Vec<_>>();
    assert_eq!(parts.len(), 4);
    let now = chrono::DateTime::parse_from_rfc3339(one(headers, "x-etv-now"))
        .unwrap()
        .with_timezone(&chrono::Utc);
    let until = chrono::DateTime::parse_from_rfc3339(one(headers, "x-etv-until"))
        .unwrap()
        .with_timezone(&chrono::Utc);
    let item = host
        .resolve_dynamic(
            parts[0].parse().unwrap(),
            parts[1].parse().unwrap(),
            parts[2],
            if parts[3] == "off" {
                None
            } else {
                Some(parts[3].parse().unwrap())
            },
            one(headers, "x-etv-channel"),
            one(headers, SESSION_HEADER).parse().unwrap(),
            now,
            until,
        )
        .await;
    match item {
        Ok(item) => {
            check_presentation_edit(host, &item, events).await;
            let body = serde_json::to_vec(&item).unwrap();
            events.lock().await.items.push(item);
            send(
                socket,
                "200 OK",
                "Content-Type: application/json\r\n",
                u64::try_from(body.len()).unwrap(),
                &body,
            )
            .await;
        }
        Err(error) => {
            events.lock().await.failures.push(error.to_string());
            send(socket, "503 Service Unavailable", "", 0, &[]).await;
        }
    }
}

async fn check_presentation_edit(
    host: &host::NextHost,
    item: &PlayoutItem,
    events: &tokio::sync::Mutex<Events>,
) {
    let uri = item
        .tracks
        .as_ref()
        .and_then(|tracks| tracks.video.as_ref())
        .and_then(|track| track.source.as_ref())
        .and_then(|source| match source {
            crate::next_engine::protocol::Source::Http { uri, .. } => Some(uri),
            _ => None,
        });
    let Some(path) =
        uri.and_then(|uri| uri.split_once("/ffmpeg/next-source/").map(|(_, path)| path))
    else {
        return;
    };
    let (instance, source) = path.split_once('/').unwrap();
    let instance = instance.parse().unwrap();
    let source = source.parse().unwrap();
    let original: String = sqlx::query_scalar(r#"SELECT "Name" FROM "Channel" WHERE "Id"=1"#)
        .fetch_one(&host.pool)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "Channel" SET "Name"='Presentation-only edit' WHERE "Id"=1"#)
        .execute(&host.pool)
        .await
        .unwrap();
    let described = host.retained_source_length(instance, source).await.is_ok();
    let opened = match host.open_retained_source(instance, source, 0, 188).await {
        Ok(mut reader) => {
            let mut bytes = Vec::new();
            reader.read_to_end(&mut bytes).await.is_ok() && bytes.len() == 188
        }
        Err(_) => false,
    };
    sqlx::query(r#"UPDATE "Channel" SET "Name"=? WHERE "Id"=1"#)
        .bind(original)
        .execute(&host.pool)
        .await
        .unwrap();
    events
        .lock()
        .await
        .presentation_edit_keeps_grant
        .push(described && opened);
    let original: String = sqlx::query_scalar(r#"SELECT "Path" FROM "MediaFile" WHERE "Id"=1"#)
        .fetch_one(&host.pool)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "MediaFile" SET "Path"='server-only/changed-source.ts' WHERE "Id"=1"#)
        .execute(&host.pool)
        .await
        .unwrap();
    let denied = host.retained_source_length(instance, source).await.is_err()
        && host
            .open_retained_source(instance, source, 0, 188)
            .await
            .is_err();
    sqlx::query(r#"UPDATE "MediaFile" SET "Path"=? WHERE "Id"=1"#)
        .bind(original)
        .execute(&host.pool)
        .await
        .unwrap();
    events.lock().await.source_edit_revokes_grant.push(denied);
}

async fn send(
    socket: &mut tokio::net::TcpStream,
    status: &str,
    extra: &str,
    length: u64,
    body: &[u8],
) {
    let headers = format!("HTTP/1.1 {status}\r\n{extra}Content-Length: {length}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n");
    if socket.write_all(headers.as_bytes()).await.is_ok() {
        let _ = socket.write_all(body).await;
    }
    let _ = socket.shutdown().await;
}
