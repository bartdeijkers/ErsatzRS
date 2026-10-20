//! A failed Body must retain the literal incomplete prefix and remain a failure.

use super::*;

#[tokio::test]
async fn body_error_and_early_eof_keep_exact_unaligned_prefix_without_becoming_success() {
    let directory = tempfile::tempdir().unwrap();
    let expected: Vec<u8> = (0..191_u8).collect();
    for body_error in [true, false] {
        let name = if body_error {
            "body-error"
        } else {
            "early-eof"
        };
        let prefix = directory.path().join(name);
        let mut chunks = vec![
            Ok(axum::body::Bytes::copy_from_slice(&expected[..137])),
            Ok(axum::body::Bytes::copy_from_slice(&expected[137..])),
        ];
        if body_error {
            chunks.push(Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "fixed failure",
            )));
        }
        let failed = AssertUnwindSafe(capture_chunks(
            futures_util::stream::iter(chunks),
            Some(&prefix),
        ))
        .catch_unwind()
        .await;
        assert!(
            failed.is_err(),
            "retention must not turn a failed capture green"
        );
        let retained = std::fs::read(prefix.with_extension("ts")).unwrap();
        assert_eq!(retained, expected);
        assert_eq!(
            retained.len() % 188,
            3,
            "do not trim to complete TS packets"
        );
        let marker: Value =
            serde_json::from_slice(&std::fs::read(prefix.with_extension("json")).unwrap()).unwrap();
        assert_eq!(marker["bytes"], 191);
        assert_eq!(marker["maximum_bytes"], CAPTURE_MAXIMUM);
        assert_eq!(marker["sha256"], live_client::hash(&expected));
        assert_eq!(marker["complete_capture"], false);
        assert_eq!(
            marker["reason"],
            if body_error {
                "body_error"
            } else {
                "early_eof"
            }
        );
    }
}
