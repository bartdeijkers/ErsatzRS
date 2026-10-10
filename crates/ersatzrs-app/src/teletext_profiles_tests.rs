use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
};

use async_trait::async_trait;
use chrono::{TimeZone as _, Utc};
use ersatzrs_domain::{TeletextInstallationSnapshot, TeletextProfileSetting, TeletextProfileValue};
use serde_json::json;

use super::*;
use crate::teletext_guide::TeletextGuideChannel;

const ADDON: &str = "org.example.teletext";
const GENERATION: &str = "supplied-generation";

fn selection(channel_id: i64) -> TeletextSelectionSnapshot {
    TeletextSelectionSnapshot {
        channel_id,
        addon_id: ADDON.to_owned(),
        binding_revision: 3,
        profile_id: channel_id + 10,
        profile_revision: 4,
        installation: TeletextInstallationSnapshot {
            addon_id: ADDON.to_owned(),
            state_revision: "state-1".to_owned(),
            version: "1.0.0".to_owned(),
            package_sha256: "package".to_owned(),
            content_sha256: "content".to_owned(),
            manifest_sha256: "manifest".to_owned(),
        },
        effective_settings: vec![TeletextProfileSetting {
            key: "THEME".to_owned(),
            value: TeletextProfileValue::Literal(
                if channel_id == 1 { "day" } else { "night" }.to_owned(),
            ),
        }],
    }
}

struct Repository {
    selections: Mutex<BTreeMap<i64, TeletextSelectionSnapshot>>,
    rejection: Mutex<Option<TeletextProfileRejection>>,
    reads: AtomicUsize,
    fail_at_read: usize,
}

impl Repository {
    fn new() -> Self {
        Self {
            selections: Mutex::new(BTreeMap::from([(1, selection(1)), (2, selection(2))])),
            rejection: Mutex::new(None),
            reads: AtomicUsize::new(0),
            fail_at_read: usize::MAX,
        }
    }
}

// Every mutation path panics: preparation must remain read-only.
#[async_trait]
impl TeletextProfileRepository for Repository {
    async fn installation_snapshot(&self, _: &str) -> RepositoryResult<TeletextInstallationRead> {
        unreachable!("unexpected unscoped read")
    }
    async fn list_profiles(&self, _: &str) -> RepositoryResult<Vec<TeletextProfileRead>> {
        unreachable!("unexpected unscoped read")
    }
    async fn channel_bindings(
        &self,
        _: EntityId,
    ) -> RepositoryResult<Vec<ChannelTeletextBindingRead>> {
        unreachable!("unexpected unscoped read")
    }
    async fn invocation_snapshot(
        &self,
        channel_id: EntityId,
        addon_id: &str,
        revision: i64,
    ) -> RepositoryResult<Mutation<TeletextSelectionSnapshot>> {
        let read = self.reads.fetch_add(1, Ordering::SeqCst);
        if read == self.fail_at_read {
            return Err(ersatzrs_core::repositories::RepositoryError::unavailable(
                "fixture",
                "private storage detail",
            ));
        }
        if let Some(reason) = *self.rejection.lock().unwrap() {
            return Ok(Mutation::Rejected(reason));
        }
        let selection = self
            .selections
            .lock()
            .unwrap()
            .get(&channel_id)
            .cloned()
            .unwrap();
        if selection.addon_id != addon_id || selection.binding_revision != revision {
            return Ok(Mutation::Rejected(TeletextProfileRejection::Conflict));
        }
        Ok(Mutation::Saved(selection))
    }
    async fn save_profile(
        &self,
        _: &TeletextProfileSave,
    ) -> RepositoryResult<Mutation<TeletextProfileSaveResult>> {
        unreachable!("preparation wrote profile")
    }
    async fn save_binding(
        &self,
        _: &ChannelTeletextBindingSave,
    ) -> RepositoryResult<Mutation<ChannelTeletextBinding>> {
        unreachable!("preparation wrote binding")
    }
    async fn disable_binding(
        &self,
        _: EntityId,
        _: &str,
        _: i64,
    ) -> RepositoryResult<Mutation<ChannelTeletextBinding>> {
        unreachable!("preparation disabled binding")
    }
    async fn tombstone_profile(
        &self,
        _: &str,
        _: EntityId,
        _: i64,
    ) -> RepositoryResult<Mutation<TeletextProfileSaveResult>> {
        unreachable!("preparation deleted profile")
    }
}

fn context<'a>(channels: &'a [TeletextGuideChannel<'a>]) -> SuppliedTeletextGuideContext<'a> {
    SuppliedTeletextGuideContext {
        generation: GENERATION,
        locale: "nl",
        supported_locales: &["nl", "en-us"],
        timezone: "Europe/Amsterdam",
        captured_at: Utc.with_ymd_and_hms(2026, 10, 11, 12, 0, 0).unwrap(),
        channels,
    }
}

fn output(generation: &str, text: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({"schema_version":1,"generation":generation,"language":"nld","initial_page":200,
        "pages":[{"address":{"page":200,"subcode":0},"kind":"full_page","language":"nld","national_option":5,
            "rows":[{"row":1,"runs":[{"column":0,"style":{"foreground":"white","background":"black","height":"normal","boxed":false,"flash":false,"conceal":false},
                "content":{"kind":"text","text":text}}]}],"navigation":[]}]})).unwrap()
}

fn receipt(selection: TeletextSelectionSnapshot) -> TeletextProfileRuntimeReceipt {
    TeletextProfileRuntimeReceipt {
        stdout: output(GENERATION, "Literal <b>page</b>"),
        selection,
        generation: GENERATION.to_owned(),
    }
}

#[tokio::test]
async fn invalid_guide_never_reads_selection_or_invokes_runtime() {
    let repo = Repository::new();
    let mut context = context(&[]);
    context.locale = "unsupported";
    let result = prepare_with_ports(&repo, 1, ADDON, 3, &context, |_, _| async {
        panic!("invalid guide invoked runtime")
    })
    .await;
    assert!(matches!(result, Err(BoundTeletextPagesError::Request(_))));
    assert_eq!(repo.reads.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn disabled_or_unavailable_selection_never_invokes_runtime() {
    let repo = Repository::new();
    for reason in [
        TeletextProfileRejection::DisabledBinding,
        TeletextProfileRejection::InstallationUnavailable,
        TeletextProfileRejection::DeletedProfile,
    ] {
        *repo.rejection.lock().unwrap() = Some(reason);
        let result = prepare_with_ports(&repo, 1, ADDON, 3, &context(&[]), |_, _| async {
            panic!("rejected selection invoked runtime")
        })
        .await;
        assert!(
            matches!(result, Err(BoundTeletextPagesError::SelectionRejected(actual)) if actual == reason)
        );
    }
    assert_eq!(repo.reads.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn exact_output_bindings_route_separately_with_same_multi_source_guide_and_no_writes() {
    let repo = Repository::new();
    let before = repo.selections.lock().unwrap().clone();
    let guides = [
        TeletextGuideChannel {
            id: 7,
            number: "7",
            name: "Source",
            programmes: &[],
        },
        TeletextGuideChannel {
            id: 8,
            number: "8",
            name: "Mirror",
            programmes: &[],
        },
    ];
    let context = context(&guides);
    for channel in [1, 2] {
        let prepared = prepare_with_ports(
            &repo,
            channel,
            ADDON,
            3,
            &context,
            |selected, input| async move {
                assert_eq!(selected, selection(channel));
                let request =
                    ersatzrs_addon_contract::TeletextPagesRequest::from_json(&input).unwrap();
                assert_eq!(
                    request
                        .channels
                        .iter()
                        .map(|channel| channel.id)
                        .collect::<Vec<_>>(),
                    vec![7, 8]
                );
                assert_eq!(request.generation, GENERATION);
                Ok(TeletextProfileRuntimeReceipt {
                    stdout: output(GENERATION, if channel == 1 { "day" } else { "night" }),
                    selection: selected,
                    generation: GENERATION.to_owned(),
                })
            },
        )
        .await
        .unwrap();
        assert_eq!(prepared.selection, selection(channel));
        assert_eq!(prepared.generation, GENERATION);
        assert_eq!(prepared.draft.pages.len(), 1);
        assert_eq!(
            prepared.draft.pages[0].rows[0].runs[0].content,
            ersatzrs_domain::teletext::TeletextRunContent::Text {
                text: if channel == 1 { "day" } else { "night" }.to_owned()
            }
        );
    }
    assert_eq!(*repo.selections.lock().unwrap(), before);
    assert_eq!(repo.reads.load(Ordering::SeqCst), 4);
}

#[tokio::test]
async fn changed_receipt_selection_or_generation_is_rejected_before_final_read() {
    for change in 0..4 {
        let repo = Repository::new();
        let result = prepare_with_ports(
            &repo,
            1,
            ADDON,
            3,
            &context(&[]),
            |selection, _| async move {
                let mut receipt = receipt(selection);
                match change {
                    0 => receipt.selection.channel_id = 2,
                    1 => receipt.selection.profile_revision += 1,
                    2 => receipt.selection.effective_settings.clear(),
                    _ => receipt.generation = "wrong private generation".to_owned(),
                }
                Ok(receipt)
            },
        )
        .await;
        assert!(matches!(
            result,
            Err(BoundTeletextPagesError::ReceiptMismatch)
        ));
        assert_eq!(repo.reads.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn in_flight_edits_or_unavailability_reject_completed_producer_output() {
    for change in 0..4 {
        let repo = Repository::new();
        let result = prepare_with_ports(&repo, 1, ADDON, 3, &context(&[]), |selection, _| async {
            if change == 3 {
                *repo.rejection.lock().unwrap() =
                    Some(TeletextProfileRejection::InstallationUnavailable);
            } else {
                let mut selections = repo.selections.lock().unwrap();
                let current = selections.get_mut(&1).unwrap();
                match change {
                    0 => current.binding_revision += 1,
                    1 => current.profile_revision += 1,
                    _ => current.effective_settings.clear(),
                }
            }
            Ok(receipt(selection))
        })
        .await;
        assert!(matches!(
            result,
            Err(BoundTeletextPagesError::SelectionChanged)
        ));
        assert_eq!(repo.reads.load(Ordering::SeqCst), 2);
    }
}

#[tokio::test]
async fn malformed_partial_or_wrong_generation_response_never_returns_a_draft() {
    let mut late_bad: serde_json::Value =
        serde_json::from_slice(&output(GENERATION, "private producer text")).unwrap();
    late_bad["pages"]
        .as_array_mut()
        .unwrap()
        .push(json!({"address":{"page":201}}));
    for stdout in [
        b"{private malformed".to_vec(),
        output("wrong", "private producer text"),
        serde_json::to_vec(&late_bad).unwrap(),
    ] {
        let repo = Repository::new();
        let result = prepare_with_ports(
            &repo,
            1,
            ADDON,
            3,
            &context(&[]),
            |selection, _| async move {
                Ok(TeletextProfileRuntimeReceipt {
                    stdout,
                    selection,
                    generation: GENERATION.to_owned(),
                })
            },
        )
        .await;
        assert!(matches!(result, Err(BoundTeletextPagesError::Response(_))));
        let diagnostic = format!("{:?}", result.unwrap_err());
        assert!(!diagnostic.contains("private producer text"));
        assert!(!diagnostic.contains("private malformed"));
    }
}

#[tokio::test]
async fn runtime_failures_preserve_exact_cleanup_uncertainty_without_retry_or_final_read() {
    for error in [
        TeletextProfileRuntimeError::Unsupported,
        TeletextProfileRuntimeError::InvalidRequest,
        TeletextProfileRuntimeError::AdmissionRejected,
        TeletextProfileRuntimeError::Cancelled,
        TeletextProfileRuntimeError::TimedOut,
        TeletextProfileRuntimeError::CleanupUnconfirmed,
        TeletextProfileRuntimeError::SpawnFailed,
        TeletextProfileRuntimeError::IoFailed,
        TeletextProfileRuntimeError::OutputTooLarge,
        TeletextProfileRuntimeError::StderrTooLarge,
        TeletextProfileRuntimeError::Failed,
    ] {
        let repo = Repository::new();
        let result = prepare_with_ports(&repo, 1, ADDON, 3, &context(&[]), |_, _| async move {
            Err(error)
        })
        .await;
        assert!(matches!(result,Err(BoundTeletextPagesError::Runtime(actual)) if actual==error));
        assert_eq!(repo.reads.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn storage_failure_is_static_and_initial_failure_never_invokes_runtime() {
    for read in [0, 1] {
        let mut repo = Repository::new();
        repo.fail_at_read = read;
        let result = prepare_with_ports(
            &repo,
            1,
            ADDON,
            3,
            &context(&[]),
            |selection, _| async move {
                assert_eq!(read, 1, "initial storage failure invoked runtime");
                Ok(receipt(selection))
            },
        )
        .await;
        assert!(matches!(
            result,
            Err(BoundTeletextPagesError::StorageUnavailable)
        ));
        assert!(!format!("{:?}", result.unwrap_err()).contains("private storage detail"));
    }
}

#[tokio::test]
async fn receipt_and_prepared_debug_omit_raw_content_and_correlation_values() {
    let repo = Repository::new();
    let raw = receipt(selection(1));
    let debug = format!("{raw:?}");
    assert!(!debug.contains(GENERATION));
    assert!(!debug.contains("Literal <b>page</b>"));
    assert!(!debug.contains("day"));
    let prepared = prepare_with_ports(
        &repo,
        1,
        ADDON,
        3,
        &context(&[]),
        |selection, _| async move { Ok(receipt(selection)) },
    )
    .await
    .unwrap();
    let debug = format!("{prepared:?}");
    assert!(!debug.contains(GENERATION));
    assert!(!debug.contains("Literal <b>page</b>"));
    assert!(!debug.contains("day"));
}
