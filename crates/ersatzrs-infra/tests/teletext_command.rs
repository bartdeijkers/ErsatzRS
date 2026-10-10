//! Unix-only real-process stdio proof; native Windows acceptance remains separate.
#![cfg(unix)]

use std::{collections::BTreeMap, time::Duration};

use ersatzrs_infra::addons::{teletext_command, ResolvedAddonCommand};

#[tokio::test]
async fn public_owned_command_preserves_complete_request_bytes_through_real_pipes() {
    let directory = tempfile::tempdir().unwrap();
    let command = ResolvedAddonCommand {
        addon_id: "synthetic-teletext-stdio".to_owned(),
        program: "sh".to_owned(),
        args: vec!["-c".to_owned(), "cat".to_owned()],
        working_directory: directory.path().to_owned(),
        environment: BTreeMap::new(),
        state_revision: "synthetic-revision".to_owned(),
        tool_paths: Vec::new(),
    };
    // Raw protocol example deliberately independent of DTO serialization.
    // The low-level command returns bytes; page-response acceptance is separate.
    let input = br#"{
      "schema_version":1,"generation":"synthetic-epoch","locale":"nl-NL",
      "timezone":"Europe/Amsterdam","captured_at":"2026-03-28T12:00:00Z",
      "window":{"start":"2026-03-27T23:00:00Z","finish":"2026-03-30T22:00:00Z"},
      "channels":[]
    }"#
    .to_vec();
    let output = tokio::time::timeout(
        Duration::from_secs(10),
        teletext_command::run(command, input.clone(), None),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(output, input);
}
