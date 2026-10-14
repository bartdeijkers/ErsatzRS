//! Synthetic effective receipts; preparation grants no encoding or publication authority.

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ersatzrs_core::repositories::{
    ChannelTeletextBindingSave, Result as RepositoryResult, TeletextProfileSave,
    TeletextProfileSaveResult,
};
use ersatzrs_domain::{
    ChannelTeletextBinding, ChannelTeletextBindingRead, TeletextInstallationRead,
    TeletextOverrideInterval, TeletextOverrideState, TeletextProfileCandidateSnapshot,
    TeletextProfileRead, TeletextPublishedOverride,
};

use super::*;
use crate::teletext_channel_documents::tests as fixtures;

const A: &str = "org.example.a";
const B: &str = "org.example.b";
const OFF: &str = "org.example.off";

struct Repository {
    selection: Selection,
    final_selection: Mutex<Option<Mutation<Selection>>>,
    reads: AtomicUsize,
}

impl Repository {
    fn new(bindings: Vec<Binding>) -> Self {
        Self {
            selection: Selection {
                channel_id: 9,
                bindings,
                valid_until: None,
            },
            final_selection: Mutex::new(None),
            reads: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl TeletextProfileRepository for Repository {
    async fn effective_channel_snapshot(
        &self,
        channel: EntityId,
        _: DateTime<Utc>,
    ) -> RepositoryResult<Mutation<Selection>> {
        assert_eq!(channel, self.selection.channel_id);
        self.reads.fetch_add(1, Ordering::SeqCst);
        Ok(Mutation::Saved(self.selection.clone()))
    }
    async fn recheck_effective_channel_snapshot(
        &self,
        expected: &Selection,
        _: DateTime<Utc>,
    ) -> RepositoryResult<Mutation<Selection>> {
        assert_eq!(*expected, self.selection);
        self.reads.fetch_add(1, Ordering::SeqCst);
        Ok(self
            .final_selection
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| Mutation::Saved(self.selection.clone())))
    }
    async fn channel_invocation_snapshot(
        &self,
        _: EntityId,
    ) -> RepositoryResult<Mutation<ersatzrs_domain::TeletextChannelSelectionSnapshot>> {
        unreachable!("effective preparation cannot recapture base authority")
    }
    async fn invocation_snapshot(
        &self,
        _: EntityId,
        _: &str,
        _: i64,
    ) -> RepositoryResult<Mutation<ersatzrs_domain::TeletextSelectionSnapshot>> {
        unreachable!("effective candidate cannot become a base binding")
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

fn scripted(base: ersatzrs_domain::TeletextSelectionSnapshot) -> Binding {
    let candidate = TeletextProfileCandidateSnapshot {
        contract: base.contract,
        channel_id: base.channel_id,
        addon_id: base.addon_id,
        profile_id: base.profile_id,
        profile_revision: base.profile_revision,
        installation: base.installation,
        effective_settings: base.effective_settings,
    };
    Binding::Scripted(TeletextPublishedOverride {
        playout_id: 93,
        interval: TeletextOverrideInterval {
            start: Utc::now() - chrono::Duration::minutes(1),
            finish: Utc::now() + chrono::Duration::minutes(10),
            state: TeletextOverrideState::Enabled(candidate),
        },
    })
}

fn off() -> Binding {
    Binding::Scripted(TeletextPublishedOverride {
        playout_id: 93,
        interval: TeletextOverrideInterval {
            start: Utc::now() - chrono::Duration::minutes(1),
            finish: Utc::now() + chrono::Duration::minutes(10),
            state: TeletextOverrideState::Off {
                channel_id: 9,
                addon_id: OFF.to_owned(),
            },
        },
    })
}

fn receipt(
    selection: Arc<Selection>,
    addon_id: String,
    stdout: Vec<u8>,
) -> TeletextEffectiveProfileRuntimeReceipt {
    TeletextEffectiveProfileRuntimeReceipt {
        selection,
        addon_id,
        stdout,
        generation: fixtures::context().generation.to_owned(),
    }
}

fn input_budget() -> TeletextChannelDocumentInputBudget {
    TeletextChannelDocumentInputBudget {
        max_producers: 3,
        max_response_bytes: 1024 * 1024,
    }
}

const C: &str = "org.example.c";

fn authored_v3_output() -> Vec<u8> {
    let service: serde_json::Value = serde_json::from_slice(&fixtures::authored_output()).unwrap();
    let value = serde_json::json!({"schema_version":3,"service":service,"authoring":{
        "schema_version":1,"template":{"id":"synthetic-template","revision":"1"},
        "theme":{"id":"private-night-theme","overrides":[]},
        "pages":[{"page":{"page":201,"subcode":16},
            "regions":[{"id":"custom-zone","kind":"zone","area":"main",
                "row":1,"column":8,"width":16,"height":2}],"dynamic_fields":[]}]}});
    let mut output = serde_json::to_vec_pretty(&value).unwrap();
    output.extend_from_slice(b"\n \t");
    output
}

fn response(addon_id: &str) -> Vec<u8> {
    match addon_id {
        A => fixtures::output("nld", 200, &[fixtures::page(200, 0, "nld", 5)]),
        B => fixtures::authored_output(),
        C => authored_v3_output(),
        _ => panic!("Off invoked"),
    }
}

fn mixed_repository() -> Repository {
    let mut b = fixtures::binding(B, 2);
    b.contract = TeletextPageContract::V2;
    let mut c = fixtures::binding(C, 3);
    c.contract = TeletextPageContract::V3;
    Repository::new(vec![
        Binding::Base(fixtures::binding(A, 1)),
        scripted(b),
        scripted(c),
        off(),
    ])
}

fn deadline() -> Instant {
    Instant::now() + std::time::Duration::from_secs(10)
}

#[tokio::test]
async fn effective_rendered_scope_is_checked_before_preflight_and_first_producer() {
    let repo = mixed_repository();
    // A changed scope must reject even before this malformed producer context
    // is interpreted. No changed producer can execute under a rendered token.
    let context = SuppliedTeletextGuideContext {
        locale: "unsupported",
        ..fixtures::context()
    };
    for change in 0..7 {
        let mut expected = repo.selection.clone();
        match change {
            0 => {
                let Binding::Base(base) = &mut expected.bindings[0] else {
                    unreachable!()
                };
                base.profile_revision += 1;
            }
            1 => {
                let Binding::Base(base) = &mut expected.bindings[0] else {
                    unreachable!()
                };
                base.effective_settings[0].value =
                    ersatzrs_domain::TeletextProfileValue::Literal("changed theme".to_owned());
            }
            2 => {
                expected.bindings.pop();
            }
            3 => {
                let Binding::Scripted(published) = &mut expected.bindings[1] else {
                    unreachable!()
                };
                published.playout_id += 1;
            }
            4 => {
                let Binding::Scripted(published) = &mut expected.bindings[1] else {
                    unreachable!()
                };
                published.interval.start += chrono::Duration::seconds(1);
            }
            5 => expected.valid_until = Some(Utc::now() + chrono::Duration::minutes(1)),
            _ => expected.channel_id += 1,
        }
        let result = prepare_with_ports_expected(
            &repo,
            9,
            Some(C),
            &context,
            input_budget(),
            |_, _, _, _| async { panic!("changed rendered scope invoked a producer") },
            (deadline(), Some(&expected)),
        )
        .await;
        assert!(matches!(result, Err(Error::SelectionChanged)));
    }
    assert_eq!(repo.reads.load(Ordering::SeqCst), 7);
}

#[tokio::test]
async fn effective_rendered_scope_preserves_original_deadline_and_final_fence() {
    let repo = Repository::new(vec![Binding::Base(fixtures::binding(A, 1)), off()]);
    let original_deadline = deadline();
    let calls = AtomicUsize::new(0);
    let result = prepare_with_ports_expected(
        &repo,
        9,
        Some(A),
        &fixtures::context(),
        input_budget(),
        |selection, addon, _, actual_deadline| {
            assert_eq!(actual_deadline, original_deadline);
            calls.fetch_add(1, Ordering::SeqCst);
            let mut changed = (*selection).clone();
            changed.bindings.pop();
            *repo.final_selection.lock().unwrap() = Some(Mutation::Saved(changed));
            async move { Ok(receipt(selection, addon, response(A))) }
        },
        (original_deadline, Some(&repo.selection)),
    )
    .await;
    assert!(matches!(result, Err(Error::SelectionChanged)));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(repo.reads.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn effective_mixed_contracts_preserve_sources_metadata_off_and_whole_origin_association() {
    let repo = mixed_repository();
    let context = fixtures::context();
    let calls = Mutex::new(Vec::new());
    let allocations = Mutex::new(Vec::new());
    let original_deadline = deadline();
    let input = prepare_with_ports(
        &repo,
        9,
        Some(C),
        &context,
        input_budget(),
        |selection, addon_id, input, deadline| {
            let binding = selection
                .bindings
                .iter()
                .find(|b| b.addon_id() == addon_id)
                .unwrap();
            let version = contract(binding).unwrap().schema_version();
            assert_eq!(
                input,
                crate::teletext_guide::build_request_for_schema(&context, version).unwrap()
            );
            assert_eq!(deadline, original_deadline);
            calls.lock().unwrap().push(addon_id.clone());
            let stdout = response(&addon_id);
            allocations.lock().unwrap().push(stdout.as_ptr() as usize);
            async move { Ok(receipt(selection, addon_id, stdout)) }
        },
        original_deadline,
    )
    .await
    .unwrap();
    assert_eq!(input.producer_count(), 3);
    assert_eq!(input.selection().bindings, repo.selection.bindings);
    assert_eq!(*calls.lock().unwrap(), [A, B, C]);
    let prepared = input.validate(fixtures::document_budget()).unwrap();
    assert_eq!(prepared.producer_bindings, [0, 1, 2]);
    assert_eq!(prepared.entry_addon_id.as_deref(), Some(C));
    assert!(matches!(
        prepared.documents[0],
        TeletextProfileDocument::Base(_)
    ));
    assert!(matches!(
        prepared.documents[1],
        TeletextProfileDocument::Authored(_)
    ));
    assert!(matches!(
        prepared.documents[2],
        TeletextProfileDocument::Authored(_)
    ));
    for (index, addon) in [A, B, C].into_iter().enumerate() {
        assert_eq!(prepared.sources[index].bytes(), response(addon));
        assert_eq!(
            prepared.sources[index].bytes().as_ptr() as usize,
            allocations.lock().unwrap()[index]
        );
        assert_eq!(prepared.sources[index].generation(), context.generation);
    }
    assert_eq!(
        prepared.sources[2].authoring().unwrap().theme.id,
        "private-night-theme"
    );
    assert_eq!(
        prepared.sources[2].authoring().unwrap().pages[0].regions[0].id,
        "custom-zone"
    );
    assert_eq!(repo.reads.load(Ordering::SeqCst), 2);
    for debug in [
        format!("{prepared:?}"),
        format!("{:?}", prepared.sources[2]),
    ] {
        for private in [
            context.generation,
            "private-night-theme",
            "custom-zone",
            "private theme text",
        ] {
            assert!(!debug.contains(private));
        }
    }
    let parts = prepared.into_parts();
    assert_eq!(parts.selection.bindings, repo.selection.bindings);
    assert_eq!(parts.producer_bindings, [0, 1, 2]);
}

#[tokio::test]
async fn effective_off_is_retained_without_requests_or_producer_budget() {
    let repo = Repository::new(vec![off()]);
    let context = SuppliedTeletextGuideContext {
        locale: "unsupported",
        ..fixtures::context()
    };
    let input = prepare_with_ports(
        &repo,
        9,
        None,
        &context,
        TeletextChannelDocumentInputBudget {
            max_producers: 0,
            max_response_bytes: 0,
        },
        |_, _, _, _| async { panic!("Off must not run a producer") },
        deadline(),
    )
    .await
    .unwrap();
    assert_eq!(input.producer_count(), 0);
    assert_eq!(input.selection().bindings, repo.selection.bindings);
    for c in [
        TeletextPageContract::V1,
        TeletextPageContract::V2,
        TeletextPageContract::V3,
    ] {
        assert!(input.requests().request(c).is_none());
    }
    let prepared = input.validate(fixtures::document_budget()).unwrap();
    assert_eq!(prepared.selection.bindings, repo.selection.bindings);
    assert!(prepared.producer_bindings.is_empty());
    assert!(prepared.sources.is_empty());
    assert!(prepared.documents.is_empty());
    assert!(prepared.entry_addon_id.is_none());
    assert_eq!(repo.reads.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn effective_receipt_requires_whole_scope_addon_and_correlation() {
    for mode in 0..3 {
        let repo = Repository::new(vec![Binding::Base(fixtures::binding(A, 1)), off()]);
        let error = prepare_with_ports(
            &repo,
            9,
            Some(A),
            &fixtures::context(),
            input_budget(),
            |selection, addon, _, _| async move {
                let mut receipt = receipt(selection, addon, response(A));
                match mode {
                    0 => {
                        let mut changed = (*receipt.selection).clone();
                        changed.bindings.pop();
                        receipt.selection = Arc::new(changed);
                    }
                    1 => receipt.addon_id = B.to_owned(),
                    _ => receipt.generation = "wrong correlation".to_owned(),
                }
                Ok(receipt)
            },
            deadline(),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, Error::ReceiptMismatch { binding: 0 }));
        assert_eq!(repo.reads.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn effective_changed_scope_after_invocation_rejects_without_partial_success() {
    for reject in [false, true] {
        let repo = Repository::new(vec![Binding::Base(fixtures::binding(A, 1)), off()]);
        let calls = AtomicUsize::new(0);
        let error = prepare_with_ports(
            &repo,
            9,
            Some(A),
            &fixtures::context(),
            input_budget(),
            |selection, addon, _, _| {
                calls.fetch_add(1, Ordering::SeqCst);
                let mut changed = (*selection).clone();
                changed.bindings.pop();
                *repo.final_selection.lock().unwrap() = Some(if reject {
                    Mutation::Rejected(
                        ersatzrs_core::repositories::TeletextProfileRejection::Conflict,
                    )
                } else {
                    Mutation::Saved(changed)
                });
                async move { Ok(receipt(selection, addon, response(A))) }
            },
            deadline(),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, Error::SelectionChanged));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(repo.reads.load(Ordering::SeqCst), 2);
    }
}

#[tokio::test]
async fn effective_selected_contract_rejects_downgrade_and_payload_correlation_mismatch() {
    for selected_contract in [
        TeletextPageContract::V1,
        TeletextPageContract::V2,
        TeletextPageContract::V3,
    ] {
        let mut selected = fixtures::binding(A, 1);
        selected.contract = selected_contract;
        let repo = Repository::new(vec![scripted(selected)]);
        let wrong = if selected_contract == TeletextPageContract::V1 {
            response(B)
        } else {
            response(A)
        };
        let input = prepare_with_ports(
            &repo,
            9,
            Some(A),
            &fixtures::context(),
            input_budget(),
            |selection, addon, _, _| {
                let wrong = wrong.clone();
                async move { Ok(receipt(selection, addon, wrong)) }
            },
            deadline(),
        )
        .await
        .unwrap();
        assert!(matches!(
            input.validate(fixtures::document_budget()),
            Err(Error::Document { binding: 0, .. })
        ));
    }
    let repo = Repository::new(vec![Binding::Base(fixtures::binding(A, 1))]);
    let mut value: serde_json::Value = serde_json::from_slice(&response(A)).unwrap();
    value["generation"] = serde_json::json!("different output correlation");
    let output = serde_json::to_vec(&value).unwrap();
    let input = prepare_with_ports(
        &repo,
        9,
        Some(A),
        &fixtures::context(),
        input_budget(),
        |selection, addon, _, _| {
            let output = output.clone();
            async move { Ok(receipt(selection, addon, output)) }
        },
        deadline(),
    )
    .await
    .unwrap();
    assert!(matches!(
        input.validate(fixtures::document_budget()),
        Err(Error::Document { binding: 0, .. })
    ));
}

#[tokio::test]
async fn effective_producer_and_aggregate_response_budgets_are_admitted_exactly() {
    let repo = Repository::new(vec![Binding::Base(fixtures::binding(A, 1)), off()]);
    let error = prepare_with_ports(
        &repo,
        9,
        Some(A),
        &fixtures::context(),
        TeletextChannelDocumentInputBudget {
            max_producers: 0,
            ..input_budget()
        },
        |_, _, _, _| async { panic!("over-budget selection invoked") },
        deadline(),
    )
    .await
    .unwrap_err();
    assert!(matches!(
        error,
        Error::ProducerBudget {
            count: 1,
            maximum: 0
        }
    ));
    let total = response(A).len() + response(B).len();
    for maximum in [total, total - 1] {
        let mut b = fixtures::binding(B, 2);
        b.contract = TeletextPageContract::V2;
        let repo = Repository::new(vec![
            Binding::Base(fixtures::binding(A, 1)),
            scripted(b),
            off(),
        ]);
        let result = prepare_with_ports(
            &repo,
            9,
            Some(A),
            &fixtures::context(),
            TeletextChannelDocumentInputBudget {
                max_response_bytes: maximum,
                ..input_budget()
            },
            |selection, addon, _, _| {
                let output = response(&addon);
                async move { Ok(receipt(selection, addon, output)) }
            },
            deadline(),
        )
        .await;
        if maximum == total {
            assert_eq!(
                result
                    .unwrap()
                    .validate(fixtures::document_budget())
                    .unwrap()
                    .sources
                    .len(),
                2
            );
        } else {
            assert!(
                matches!(result,Err(Error::AggregateResponseBudget {binding:1,maximum:m}) if m==maximum)
            );
            assert_eq!(repo.reads.load(Ordering::SeqCst), 1);
        }
    }
}

#[tokio::test]
async fn effective_expiry_and_original_deadline_are_not_renewed() {
    let repo = Repository::new(vec![Binding::Base(fixtures::binding(A, 1))]);
    let error = prepare_with_ports(
        &repo,
        9,
        Some(A),
        &fixtures::context(),
        input_budget(),
        |_, _, _, _| async { panic!("expired request invoked") },
        Instant::now(),
    )
    .await
    .unwrap_err();
    assert!(matches!(error, Error::TimedOut));
    assert_eq!(repo.reads.load(Ordering::SeqCst), 0);
    let mut repo = Repository::new(vec![Binding::Base(fixtures::binding(A, 1))]);
    repo.selection.valid_until = Some(Utc::now() - chrono::Duration::seconds(1));
    let error = prepare_with_ports(
        &repo,
        9,
        Some(A),
        &fixtures::context(),
        input_budget(),
        |_, _, _, _| async { panic!("expired interval invoked") },
        deadline(),
    )
    .await
    .unwrap_err();
    assert!(matches!(error, Error::TimedOut));
    assert_eq!(repo.reads.load(Ordering::SeqCst), 1);
    let mut repo = Repository::new(vec![Binding::Base(fixtures::binding(A, 1))]);
    repo.selection.valid_until = Some(Utc::now() + chrono::Duration::seconds(2));
    let original = Instant::now() + std::time::Duration::from_secs(1);
    let input = prepare_with_ports(
        &repo,
        9,
        Some(A),
        &fixtures::context(),
        input_budget(),
        |selection, addon, _, deadline| {
            assert_eq!(deadline, original);
            async move { Ok(receipt(selection, addon, response(A))) }
        },
        original,
    )
    .await
    .unwrap();
    assert_eq!(input.deadline(), original);
    let mut input = input;
    input.deadline = Instant::now();
    assert!(matches!(
        input.validate(fixtures::document_budget()),
        Err(Error::TimedOut)
    ));
}

#[tokio::test]
async fn effective_same_addon_keeps_two_channels_profiles_and_metadata_independent() {
    let mut prepared = Vec::new();
    for (channel, profile, theme) in [(9, 11, "day"), (10, 22, "night")] {
        let mut target = fixtures::binding(A, profile);
        target.channel_id = channel;
        target.contract = TeletextPageContract::V3;
        target.effective_settings[0].value =
            ersatzrs_domain::TeletextProfileValue::Literal(theme.to_owned());
        let mut repo = Repository::new(vec![scripted(target)]);
        repo.selection.channel_id = channel;
        let input = prepare_with_ports(
            &repo,
            channel,
            Some(A),
            &fixtures::context(),
            input_budget(),
            |selection, addon, _, _| {
                assert_eq!(selection.channel_id, channel);
                let Binding::Scripted(published) = &selection.bindings[0] else {
                    panic!("base substituted")
                };
                let TeletextOverrideState::Enabled(candidate) = &published.interval.state else {
                    panic!("On replaced")
                };
                assert_eq!(candidate.profile_id, profile);
                assert_eq!(candidate.channel_id, channel);
                assert_eq!(
                    candidate.effective_settings[0].value,
                    ersatzrs_domain::TeletextProfileValue::Literal(theme.to_owned())
                );
                let mut value: serde_json::Value =
                    serde_json::from_slice(&authored_v3_output()).unwrap();
                value["authoring"]["theme"]["id"] = serde_json::json!(theme);
                let bytes = serde_json::to_vec(&value).unwrap();
                async move { Ok(receipt(selection, addon, bytes)) }
            },
            deadline(),
        )
        .await
        .unwrap();
        prepared.push(input.validate(fixtures::document_budget()).unwrap());
    }
    assert_eq!(prepared[0].selection.channel_id, 9);
    assert_eq!(prepared[1].selection.channel_id, 10);
    assert_eq!(prepared[0].sources[0].authoring().unwrap().theme.id, "day");
    assert_eq!(
        prepared[1].sources[0].authoring().unwrap().theme.id,
        "night"
    );
    assert_ne!(
        prepared[0].sources[0].bytes(),
        prepared[1].sources[0].bytes()
    );
}
