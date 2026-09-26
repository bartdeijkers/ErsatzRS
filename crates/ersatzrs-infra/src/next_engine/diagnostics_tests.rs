use super::*;
use tokio::io::AsyncWriteExt as _;

fn diagnostics(redactions: Vec<String>) -> Diagnostics {
    Diagnostics {
        readers: JoinSet::new(),
        tail: Arc::default(),
        redactions: Arc::new(redactions),
    }
}

#[test]
fn upstream_levels_and_targets_are_preserved_with_debug_fallback() {
    for level in ["TRACE", "DEBUG", "INFO", "WARN", "ERROR"] {
        let line = format!("[2026-09-26T12:00:00Z {level} next::worker] message");
        assert_eq!(parse(&line), (level, "next::worker", "message"));
    }
    for line in [
        "plain output",
        "[timestamp UNKNOWN target] message",
        "[ WARN target] message",
        "[timestamp INFO ] message",
    ] {
        assert_eq!(parse(line), ("DEBUG", "next", line));
    }
}

#[tokio::test]
async fn readers_handle_chunk_boundaries_redact_and_discard_oversized_lines() {
    let mut log = diagnostics(vec![
        "fixture-session-value".into(),
        "X:\\fixture media\\input.mkv".into(),
    ]);
    let (reader, mut writer) = tokio::io::duplex(7);
    log.read(reader);
    writer.write_all(b"[time WARN next::worker] source https://user:placeholder@example.test/private?token=placeholder Authorization: Bearer placeholder X:\\fixture media\\input.mkv fixture-session-value\r\n").await.unwrap();
    writer
        .write_all(&vec![b'x'; MAX_LINE_BYTES * 3])
        .await
        .unwrap();
    writer
        .write_all(b"\n[time INFO next::worker] completed\nunterminated \xff")
        .await
        .unwrap();
    drop(writer);
    log.finish().await;
    let text = log.text();
    assert!(text.contains("WARN] [next::worker]"));
    assert!(text.contains("https://example.test"));
    for private in [
        "placeholder",
        "private",
        "fixture-session-value",
        "fixture media",
        "input.mkv",
        "xxxxx",
    ] {
        assert!(
            !text.contains(private),
            "private or oversized text was retained"
        );
    }
    assert!(text.contains("Oversized Next diagnostic line discarded"));
    assert!(text.contains("INFO] [next::worker] completed"));
    assert!(text.contains("unterminated �"));
    assert!(log.readers.is_empty());
}

#[tokio::test]
async fn retained_output_is_bounded_and_both_streams_keep_draining() {
    let mut log = diagnostics(Vec::new());
    let (first, mut first_writer) = tokio::io::duplex(64);
    let (second, mut second_writer) = tokio::io::duplex(64);
    log.read(first);
    log.read(second);
    let repeated = format!("[time DEBUG worker] {}\n", "diagnostic ".repeat(100)).repeat(400);
    let (a, b) = tokio::join!(
        first_writer.write_all(repeated.as_bytes()),
        second_writer.write_all(repeated.as_bytes())
    );
    a.unwrap();
    b.unwrap();
    first_writer
        .write_all(b"[time INFO worker] first stream finished\n")
        .await
        .unwrap();
    second_writer
        .write_all(b"[time ERROR worker] second stream finished\n")
        .await
        .unwrap();
    drop((first_writer, second_writer));
    log.finish().await;
    let text = log.text();
    assert!(text.len() <= MAX_LOG_BYTES);
    assert!(text.starts_with(OMITTED));
    assert!(text.contains("first stream finished"));
    assert!(text.contains("second stream finished"));
}

#[tokio::test]
async fn finish_and_drop_cancel_readers_that_never_reach_eof() {
    let mut log = diagnostics(Vec::new());
    let (reader, mut writer) = tokio::io::duplex(8);
    log.read(reader);
    tokio::time::timeout(Duration::from_secs(2), log.finish())
        .await
        .unwrap();
    assert!(writer.write_all(b"closed").await.is_err());
    let (reader, mut writer) = tokio::io::duplex(8);
    log.read(reader);
    drop(log);
    tokio::time::timeout(Duration::from_secs(1), async {
        while writer.write_all(b"closed").await.is_ok() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
