use super::*;

pub(in crate::next_engine) fn description() -> serde_json::Value {
    serde_json::json!({
        "describe_version":1,
        "contract": {
            "channel_config_schema":{"uri":"https://ersatztv.org/channel/version/0.1.4","breaking":1,"compatible":4},
            "playout_schema":{"uri":"https://ersatztv.org/playout/version/0.0.8","breaking":0,"compatible":8},
            "playout_layouts":["folder","ersatzrs.playout-index.v1"],
            "output_files":{"playlist":"live.m3u8","subtitle_playlist":"live_sub.m3u8","ready":".ready","heartbeat":".heartbeat"}
        },
        "future_additive_field":"ignored"
    })
}

#[test]
fn indexed_contract_requires_explicit_mode_and_consistent_supported_schemas() {
    assert_eq!(
        validate(&serde_json::to_vec(&description()).unwrap()),
        Ok(())
    );
    for (pointer, value) in [
        ("/describe_version", serde_json::json!(2)),
        (
            "/contract/output_files/playlist",
            serde_json::json!("wrong.m3u8"),
        ),
        (
            "/contract/output_files/subtitle_playlist",
            serde_json::json!("wrong.vtt"),
        ),
        (
            "/contract/output_files/ready",
            serde_json::json!("wrong-ready"),
        ),
        (
            "/contract/output_files/heartbeat",
            serde_json::json!("wrong-heartbeat"),
        ),
        ("/contract/playout_layouts", serde_json::json!(["folder"])),
        (
            "/contract/channel_config_schema/compatible",
            serde_json::json!(3),
        ),
        (
            "/contract/channel_config_schema/uri",
            serde_json::json!("private-invalid-uri"),
        ),
        ("/contract/playout_schema/breaking", serde_json::json!(1)),
    ] {
        let mut input = description();
        *input.pointer_mut(pointer).unwrap() = value;
        assert_eq!(
            validate(&serde_json::to_vec(&input).unwrap()),
            Err(IndexedRuntimeError::Contract)
        );
    }
    for input in [b"not json".as_slice(), b"{\"describe_version\":1}", b"\xff"] {
        assert_eq!(validate(input), Err(IndexedRuntimeError::Contract));
    }
    assert_eq!(
        validate(&vec![b' '; OUTPUT_LIMIT + 1]),
        Err(IndexedRuntimeError::Contract)
    );
    assert!(!format!("{:?}", IndexedReaderPermit::fixture()).contains("synthetic-indexed-reader"));
}

#[tokio::test]
async fn indexed_identity_is_bounded_and_refuses_missing_or_nonregular_inputs() {
    let folder = tempfile::tempdir().unwrap();
    assert_eq!(
        identity(folder.path()).await,
        Err(IndexedRuntimeError::Identity)
    );
    let file = folder.path().join("binary");
    assert_eq!(identity(&file).await, Err(IndexedRuntimeError::Identity));
    std::fs::File::create(&file)
        .unwrap()
        .set_len(BINARY_LIMIT + 1)
        .unwrap();
    assert_eq!(identity(&file).await, Err(IndexedRuntimeError::Identity));
}

// Unix shell fixtures exercise process-tree ownership. Native Windows runs the
// same managed Job Object implementation; actual reader qualification stays open.
#[cfg(unix)]
mod processes {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn write(path: &Path, body: &str) {
        std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }

    fn good(path: &Path) {
        let json = description().to_string();
        write(
            path,
            &format!("[ \"$1\" = describe ] || exit 3\nprintf '%s' '{json}'"),
        );
    }

    #[tokio::test]
    async fn indexed_permit_binds_success_to_the_same_binary_and_detects_changed_bytes() {
        let folder = tempfile::tempdir().unwrap();
        let binary = folder.path().join("next");
        good(&binary);
        let permit = IndexedReaderPermit::qualify(&binary, None).await.unwrap();
        permit.authorize(&binary).await.unwrap();
        permit.health().await.unwrap();
        let other = folder.path().join("other");
        std::fs::copy(&binary, &other).unwrap();
        assert_eq!(
            permit.authorize(&other).await,
            Err(IndexedRuntimeError::Identity)
        );
        write(&binary, "printf 'same version, no indexed reader'");
        assert_eq!(
            permit.authorize(&binary).await,
            Err(IndexedRuntimeError::Identity)
        );
        assert_eq!(permit.health().await, Err(IndexedRuntimeError::Identity));
        assert!(IndexedReaderPermit::qualify(&binary, None).await.is_err());
    }

    #[tokio::test]
    async fn indexed_probe_refuses_bad_status_invalid_output_flood_and_hanging_descendants() {
        let folder = tempfile::tempdir().unwrap();
        let binary = folder.path().join("next");
        for body in [
            "printf '{}'; exit 1",
            "printf '{malformed'",
            "while :; do printf '0123456789'; done",
            "sleep 30 & wait",
        ] {
            write(&binary, body);
            let result = tokio::time::timeout(
                Duration::from_secs(6),
                IndexedReaderPermit::probe(&binary, Duration::from_millis(100), None),
            )
            .await
            .expect("probe and managed descendant cleanup stay bounded");
            assert!(result.is_err());
        }
    }

    #[tokio::test]
    async fn indexed_pinned_identity_mismatch_precedes_every_describe_launch() {
        let folder = tempfile::tempdir().unwrap();
        let binary = folder.path().join("next");
        let marker = folder.path().join("must-not-run");
        write(&binary, &format!("touch '{}'", marker.display()));
        assert_eq!(
            IndexedReaderPermit::qualify(&binary, Some(&"0".repeat(64)))
                .await
                .unwrap_err(),
            IndexedRuntimeError::Identity
        );
        assert!(!marker.exists());
    }

    #[tokio::test]
    async fn indexed_probe_refuses_a_binary_replaced_while_describe_is_running() {
        let folder = tempfile::tempdir().unwrap();
        let binary = folder.path().join("next");
        let marker = folder.path().join("describing");
        let json = description().to_string();
        write(
            &binary,
            &format!(
                "touch '{}'; sleep 0.15; printf '%s' '{json}'",
                marker.display()
            ),
        );
        let mutate = async {
            tokio::time::timeout(Duration::from_secs(2), async {
                while !marker.exists() {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .unwrap();
            std::fs::write(&binary, b"replaced after successful describe started").unwrap();
        };
        let (result, ()) = tokio::join!(IndexedReaderPermit::qualify(&binary, None), mutate);
        assert_eq!(result.unwrap_err(), IndexedRuntimeError::Identity);
    }
}
