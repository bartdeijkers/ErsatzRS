use std::{
    cell::Cell,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
};

use async_trait::async_trait;
use chrono::{TimeZone as _, Utc};
use ersatzrs_core::repositories::{
    ChannelTeletextBindingSave, Result as RepositoryResult, TeletextProfileSave,
    TeletextProfileSaveResult,
};
use ersatzrs_domain::{
    ChannelTeletextBinding, ChannelTeletextBindingRead, TeletextInstallationRead,
    TeletextInstallationSnapshot, TeletextProfileRead, TeletextProfileSetting,
    TeletextProfileValue,
};
use serde_json::{json, Value};

use super::*;

const GENERATION: &str = "private-supplied-generation";
const A: &str = "org.example.a";
const B: &str = "org.example.b";
const C: &str = "org.example.c";

fn binding(addon: &str, profile_id: i64) -> TeletextSelectionSnapshot {
    TeletextSelectionSnapshot {
        contract: ersatzrs_domain::TeletextPageContract::V1,
        channel_id: 9,
        addon_id: addon.to_owned(),
        binding_revision: 2,
        profile_id,
        profile_revision: 3,
        installation: TeletextInstallationSnapshot {
            addon_id: addon.to_owned(),
            state_revision: "state".to_owned(),
            version: "1.0.0".to_owned(),
            package_sha256: "package".to_owned(),
            content_sha256: "content".to_owned(),
            manifest_sha256: "manifest".to_owned(),
        },
        effective_settings: vec![TeletextProfileSetting {
            key: "ZONE".to_owned(),
            value: TeletextProfileValue::Literal("private theme text".to_owned()),
        }],
    }
}

struct Repository {
    selection: Mutex<TeletextChannelSelectionSnapshot>,
    final_selection: Mutex<Option<Mutation<TeletextChannelSelectionSnapshot>>>,
    rejection: Option<TeletextProfileRejection>,
    fail_at_read: usize,
    reads: AtomicUsize,
}

impl Repository {
    fn new(bindings: Vec<TeletextSelectionSnapshot>) -> Self {
        Self {
            selection: Mutex::new(TeletextChannelSelectionSnapshot {
                channel_id: 9,
                bindings,
            }),
            final_selection: Mutex::new(None),
            rejection: None,
            fail_at_read: usize::MAX,
            reads: AtomicUsize::new(0),
        }
    }
    fn two() -> Self {
        Self::new(vec![binding(A, 1), binding(B, 2)])
    }
}

#[async_trait]
impl TeletextProfileRepository for Repository {
    async fn channel_invocation_snapshot(
        &self,
        channel_id: EntityId,
    ) -> RepositoryResult<Mutation<TeletextChannelSelectionSnapshot>> {
        assert_eq!(channel_id, 9);
        let read = self.reads.fetch_add(1, Ordering::SeqCst);
        if read == self.fail_at_read {
            return Err(ersatzrs_core::repositories::RepositoryError::unavailable(
                "fixture",
                "private storage detail",
            ));
        }
        if let Some(reason) = self.rejection {
            return Ok(Mutation::Rejected(reason));
        }
        if read == 1 {
            if let Some(snapshot) = self.final_selection.lock().unwrap().clone() {
                return Ok(snapshot);
            }
        }
        Ok(Mutation::Saved(self.selection.lock().unwrap().clone()))
    }
    async fn invocation_snapshot(
        &self,
        _: EntityId,
        _: &str,
        _: i64,
    ) -> RepositoryResult<Mutation<TeletextSelectionSnapshot>> {
        unreachable!("per-binding read misses membership fence")
    }
    async fn installation_snapshot(&self, _: &str) -> RepositoryResult<TeletextInstallationRead> {
        unreachable!("unscoped read")
    }
    async fn list_profiles(&self, _: &str) -> RepositoryResult<Vec<TeletextProfileRead>> {
        unreachable!("unscoped read")
    }
    async fn channel_bindings(
        &self,
        _: EntityId,
    ) -> RepositoryResult<Vec<ChannelTeletextBindingRead>> {
        unreachable!("unscoped read")
    }
    async fn save_profile(
        &self,
        _: &TeletextProfileSave,
    ) -> RepositoryResult<Mutation<TeletextProfileSaveResult>> {
        unreachable!("preparation mutated profile")
    }
    async fn save_binding(
        &self,
        _: &ChannelTeletextBindingSave,
    ) -> RepositoryResult<Mutation<ChannelTeletextBinding>> {
        unreachable!("preparation mutated binding")
    }
    async fn disable_binding(
        &self,
        _: EntityId,
        _: &str,
        _: i64,
    ) -> RepositoryResult<Mutation<ChannelTeletextBinding>> {
        unreachable!("preparation changed base")
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

fn context() -> SuppliedTeletextGuideContext<'static> {
    SuppliedTeletextGuideContext {
        generation: GENERATION,
        locale: "nl",
        supported_locales: &["nl"],
        timezone: "Europe/Amsterdam",
        captured_at: Utc.with_ymd_and_hms(2026, 10, 11, 12, 0, 0).unwrap(),
        channels: &[],
    }
}

fn page(number: u16, subcode: u16, language: &str, national_option: u8) -> Value {
    json!({"address":{"page":number,"subcode":subcode},"kind":"full_page","language":language,"national_option":national_option,
        "rows":[],"navigation":[]})
}

fn output(language: &str, initial_page: u16, pages: &[Value]) -> Vec<u8> {
    serde_json::to_vec(&json!({"schema_version":1,"generation":GENERATION,"language":language,"initial_page":initial_page,"pages":pages})).unwrap()
}

fn receipt(selection: TeletextSelectionSnapshot, stdout: Vec<u8>) -> TeletextProfileRuntimeReceipt {
    TeletextProfileRuntimeReceipt {
        selection,
        stdout,
        generation: GENERATION.to_owned(),
    }
}

fn basic(selection: TeletextSelectionSnapshot) -> TeletextProfileRuntimeReceipt {
    let number = if selection.addon_id == A { 200 } else { 201 };
    receipt(
        selection,
        output("nld", number, &[page(number, 0, "nld", 5)]),
    )
}

#[tokio::test]
async fn multiple_producers_preserve_explicit_entry_languages_subcodes_and_navigation() {
    let repo = Repository::two();
    let before = repo.selection.lock().unwrap().clone();
    let inputs = Mutex::new(Vec::new());
    let calls = Mutex::new(Vec::new());
    let prepared = prepare_with_ports(
        &repo,
        9,
        Some(B),
        &context(),
        |selection, input, deadline| {
            inputs.lock().unwrap().push(input);
            calls
                .lock()
                .unwrap()
                .push((selection.addon_id.clone(), deadline));
            async move {
                let stdout = if selection.addon_id == A {
                    output("nld", 200, &[page(200, 0, "nld", 5)])
                } else {
                    let mut linked = page(201, 0, "eng", 0);
                    linked["navigation"] =
                        json!([{"role":"green","target":{"page":200,"subcode":16}}]);
                    output("eng", 201, &[linked, page(200, 16, "eng", 0)])
                };
                Ok(receipt(selection, stdout))
            }
        },
        Instant::now,
    )
    .await
    .unwrap();
    let draft = prepared.draft().unwrap();
    assert_eq!(draft.pages.len(), 3);
    assert_eq!(draft.initial_page.get(), 201);
    assert_eq!(draft.language.as_str(), "eng");
    assert_eq!(draft.pages[0].language.as_str(), "nld");
    assert_eq!(draft.pages[0].national_option.get(), 5);
    assert_eq!(draft.pages[2].address.subcode.get(), 16);
    assert_eq!(
        draft.pages[1].navigation[0].target.subcode.unwrap().get(),
        16
    );
    assert_eq!(prepared.entry_addon_id(), Some(B));
    assert_eq!(prepared.selection(), &before);
    assert_eq!(prepared.generation(), GENERATION);
    assert_eq!(*repo.selection.lock().unwrap(), before);
    let inputs = inputs.lock().unwrap();
    assert_eq!(inputs[0], inputs[1]);
    let calls = calls.lock().unwrap();
    assert_eq!(
        calls.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(),
        vec![A, B]
    );
    assert_eq!(calls[0].1, calls[1].1);
    assert_eq!(prepared.deadline(), calls[0].1);
}

#[tokio::test]
async fn nonempty_selection_requires_selected_entry_without_sorted_first_fallback() {
    for entry in [None, Some(C)] {
        let repo = Repository::two();
        let result = prepare_with_ports(
            &repo,
            9,
            entry,
            &context(),
            |_, _, _| async { panic!("invalid entry invoked runtime") },
            Instant::now,
        )
        .await;
        match entry {
            None => assert!(matches!(
                result,
                Err(ChannelTeletextPagesError::EntryAddonRequired)
            )),
            _ => assert!(matches!(
                result,
                Err(ChannelTeletextPagesError::EntryAddonNotSelected)
            )),
        }
        assert_eq!(repo.reads.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn off_is_explicit_and_fenced_without_invocation_or_obsolete_entry() {
    let repo = Repository::new(vec![]);
    let prepared = prepare_with_ports(
        &repo,
        9,
        Some("obsolete"),
        &context(),
        |_, _, _| async { panic!("off invoked runtime") },
        Instant::now,
    )
    .await
    .unwrap();
    assert!(prepared.draft().is_none());
    assert!(prepared.selection().bindings.is_empty());
    assert_eq!(prepared.entry_addon_id(), None);
    assert_eq!(repo.reads.load(Ordering::SeqCst), 2);
    *repo.final_selection.lock().unwrap() =
        Some(Mutation::Saved(TeletextChannelSelectionSnapshot {
            channel_id: 9,
            bindings: vec![binding(A, 1)],
        }));
    repo.reads.store(0, Ordering::SeqCst);
    let result = prepare_with_ports(
        &repo,
        9,
        None,
        &context(),
        |_, _, _| async { panic!("off invoked runtime") },
        Instant::now,
    )
    .await;
    assert!(matches!(
        result,
        Err(ChannelTeletextPagesError::SelectionChanged)
    ));
}

#[tokio::test]
async fn invalid_guide_and_unavailable_selection_do_not_invoke_runtime() {
    let mut repo = Repository::two();
    let mut invalid = context();
    invalid.locale = "unsupported";
    let result = prepare_with_ports(
        &repo,
        9,
        Some(A),
        &invalid,
        |_, _, _| async { panic!("invalid guide invoked runtime") },
        Instant::now,
    )
    .await;
    assert!(matches!(result, Err(ChannelTeletextPagesError::Request(_))));
    assert_eq!(repo.reads.load(Ordering::SeqCst), 0);
    repo.rejection = Some(TeletextProfileRejection::InstallationUnavailable);
    let result = prepare_with_ports(
        &repo,
        9,
        Some(A),
        &context(),
        |_, _, _| async { panic!("unavailable selection invoked runtime") },
        Instant::now,
    )
    .await;
    assert!(matches!(
        result,
        Err(ChannelTeletextPagesError::SelectionRejected(
            TeletextProfileRejection::InstallationUnavailable
        ))
    ));
}

#[tokio::test]
async fn mixed_or_forged_later_receipt_never_returns_an_aggregate() {
    for change in 0..4 {
        let repo = Repository::two();
        let result = prepare_with_ports(
            &repo,
            9,
            Some(A),
            &context(),
            |selection, _, _| async move {
                let later = selection.addon_id == B;
                let mut receipt = basic(selection);
                if later {
                    match change {
                        0 => receipt.selection = binding(A, 1),
                        1 => receipt.selection.effective_settings.clear(),
                        2 => receipt.selection.contract = ersatzrs_domain::TeletextPageContract::V2,
                        _ => receipt.generation = "wrong".to_owned(),
                    }
                }
                Ok(receipt)
            },
            Instant::now,
        )
        .await;
        assert!(matches!(
            result,
            Err(ChannelTeletextPagesError::ReceiptMismatch)
        ));
        assert_eq!(repo.reads.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn colliding_addresses_fail_without_overwrite_or_renumbering() {
    let repo = Repository::two();
    let result = prepare_with_ports(
        &repo,
        9,
        Some(A),
        &context(),
        |selection, _, _| async move {
            Ok(receipt(
                selection,
                output("nld", 200, &[page(200, 0, "nld", 5)]),
            ))
        },
        Instant::now,
    )
    .await;
    assert!(matches!(
        result,
        Err(ChannelTeletextPagesError::AddressCollision)
    ));
}

#[tokio::test]
async fn aggregate_accepts_exact_1024_and_rejects_1025_distinct_addresses() {
    for last_count in [512, 513] {
        let repo = Repository::two();
        let result = prepare_with_ports(
            &repo,
            9,
            Some(A),
            &context(),
            |selection, _, _| async move {
                let (first, count) = if selection.addon_id == A {
                    (0, 512)
                } else {
                    (512, last_count)
                };
                let pages = (first..first + count)
                    .map(|i| page(200, (i % 128) | ((i / 128) << 8), "nld", 5))
                    .collect::<Vec<_>>();
                Ok(receipt(selection, output("nld", 200, &pages)))
            },
            Instant::now,
        )
        .await;
        if last_count == 512 {
            assert_eq!(result.unwrap().draft().unwrap().pages.len(), 1024);
        } else {
            assert!(matches!(
                result,
                Err(ChannelTeletextPagesError::RecordCount)
            ));
        }
    }
}

#[tokio::test]
async fn late_failure_or_malformed_response_is_atomic_and_preserves_cleanup_uncertainty() {
    for failure in 0..3 {
        let repo = Repository::two();
        let result = prepare_with_ports(
            &repo,
            9,
            Some(A),
            &context(),
            |selection, _, _| async move {
                if selection.addon_id == B {
                    match failure {
                        0 => Err(TeletextProfileRuntimeError::CleanupUnconfirmed),
                        1 => Err(TeletextProfileRuntimeError::Failed),
                        _ => Ok(receipt(
                            selection,
                            b"{private invalid producer payload".to_vec(),
                        )),
                    }
                } else {
                    Ok(basic(selection))
                }
            },
            Instant::now,
        )
        .await;
        match failure {
            0 => assert!(matches!(
                result,
                Err(ChannelTeletextPagesError::Runtime(
                    TeletextProfileRuntimeError::CleanupUnconfirmed
                ))
            )),
            1 => assert!(matches!(
                result,
                Err(ChannelTeletextPagesError::Runtime(
                    TeletextProfileRuntimeError::Failed
                ))
            )),
            _ => assert!(matches!(
                result,
                Err(ChannelTeletextPagesError::Response(_))
            )),
        }
        assert!(!format!("{:?}", result.unwrap_err()).contains("private invalid producer payload"));
        assert_eq!(repo.reads.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn final_whole_snapshot_catches_early_producer_edits_and_enabled_membership_changes() {
    for change in 0..5 {
        let repo = Repository::two();
        let result = prepare_with_ports(
            &repo,
            9,
            Some(A),
            &context(),
            |selection, _, _| {
                if selection.addon_id == B {
                    let mut snapshot = repo.selection.lock().unwrap();
                    match change {
                        0 => snapshot.bindings[0].profile_revision += 1,
                        1 => {
                            snapshot.bindings.remove(0);
                        }
                        2 => snapshot.bindings.push(binding(C, 3)),
                        3 => snapshot.bindings[0].effective_settings.clear(),
                        _ => {
                            snapshot.bindings[0].contract =
                                ersatzrs_domain::TeletextPageContract::V2;
                        }
                    }
                }
                async move { Ok(basic(selection)) }
            },
            Instant::now,
        )
        .await;
        assert!(matches!(
            result,
            Err(ChannelTeletextPagesError::SelectionChanged)
        ));
        assert_eq!(repo.reads.load(Ordering::SeqCst), 2);
    }
}

#[tokio::test]
async fn shared_deadline_does_not_reset_or_start_a_later_producer_after_expiry() {
    let repo = Repository::new(vec![binding(A, 1), binding(B, 2), binding(C, 3)]);
    let start = Instant::now();
    let clock = Cell::new(start);
    let calls = AtomicUsize::new(0);
    let result = prepare_with_ports(
        &repo,
        9,
        Some(A),
        &context(),
        |selection, _, deadline| {
            assert_eq!(deadline, start + Duration::from_secs(30));
            let call = calls.fetch_add(1, Ordering::SeqCst);
            assert!(call < 2, "expired budget started later producer");
            clock.set(clock.get() + Duration::from_secs(if call == 0 { 20 } else { 10 }));
            async move { Ok(basic(selection)) }
        },
        || clock.get(),
    )
    .await;
    assert!(matches!(result, Err(ChannelTeletextPagesError::TimedOut)));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(repo.reads.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn bounded_binding_count_and_storage_failures_do_not_expose_diagnostics() {
    let repo = Repository::new(
        (0..1025)
            .map(|i| binding(&format!("org.example.producer{i}"), i + 1))
            .collect(),
    );
    let result = prepare_with_ports(
        &repo,
        9,
        Some(A),
        &context(),
        |_, _, _| async { panic!("oversized selection invoked runtime") },
        Instant::now,
    )
    .await;
    assert!(matches!(
        result,
        Err(ChannelTeletextPagesError::BindingCount)
    ));
    for read in [0, 1] {
        let mut repo = Repository::two();
        repo.fail_at_read = read;
        let result = prepare_with_ports(
            &repo,
            9,
            Some(A),
            &context(),
            |selection, _, _| async move {
                assert_eq!(read, 1, "initial storage failure invoked runtime");
                Ok(basic(selection))
            },
            Instant::now,
        )
        .await;
        assert!(matches!(
            result,
            Err(ChannelTeletextPagesError::StorageUnavailable)
        ));
        assert!(!format!("{:?}", result.unwrap_err()).contains("private storage detail"));
    }
}

#[tokio::test]
async fn prepared_debug_redacts_generation_and_effective_setting_values() {
    let repo = Repository::two();
    let prepared = prepare_with_ports(
        &repo,
        9,
        Some(A),
        &context(),
        |selection, _, _| async move { Ok(basic(selection)) },
        Instant::now,
    )
    .await
    .unwrap();
    let debug = format!("{prepared:?}");
    assert!(!debug.contains(GENERATION));
    assert!(!debug.contains("private theme text"));
}

#[tokio::test]
async fn any_authored_binding_rejects_base_composition_before_the_first_producer() {
    for authored_index in 0..2 {
        let repo = Repository::two();
        repo.selection.lock().unwrap().bindings[authored_index].contract =
            ersatzrs_domain::TeletextPageContract::V2;
        let result = prepare_with_ports(
            &repo,
            9,
            Some(A),
            &context(),
            |_, _, _| async { panic!("no producer may run before complete contract admission") },
            Instant::now,
        )
        .await;
        assert!(matches!(
            result,
            Err(ChannelTeletextPagesError::EnhancedCompositionRequired)
        ));
        assert_eq!(repo.reads.load(Ordering::SeqCst), 1);
    }
}
