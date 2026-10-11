//! Synthetic complete-channel documents; no publication or compilation authority.
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

fn input_budget() -> TeletextChannelDocumentInputBudget {
    TeletextChannelDocumentInputBudget {
        max_response_bytes: 1024 * 1024,
    }
}

fn document_budget() -> TeletextProfileDocumentBudget {
    use ersatzrs_domain::teletext as d;
    TeletextProfileDocumentBudget {
        service: d::TeletextServiceBudget {
            max_records: 64,
            max_objects: 64,
            max_structure_items: 1024,
            max_base_content_bytes: 1024,
            max_commands: 1024,
            max_collected_commands: 1024,
            max_collected_drcs_references: 64,
        },
        declarations: d::TeletextDeclaredServiceBudget {
            max_mots: 8,
            max_displays: 64,
            max_resources: 64,
            max_defaults: 64,
            max_objects: 64,
        },
    }
}

fn mixed_repository() -> Repository {
    let mut authored = binding(B, 2);
    authored.contract = TeletextPageContract::V2;
    Repository::new(vec![binding(A, 1), authored])
}

fn authored_output() -> Vec<u8> {
    serde_json::to_vec(&json!({"schema_version":2,"generation":GENERATION,
        "language":"eng","initial_page":201,"pages":[page(201,16,"eng",0)],
        "page_enhancements":[{"address":{"page":201,"subcode":16},
            "presentation":null,"compositional_links":null,"local_program":{"commands":[
                {"kind":"set_position","position":{"row":1,"column":8}},
                {"kind":"at_column","column":8,"operation":{"kind":"designate_charset","designation":8}},
                {"kind":"at_column","column":8,"operation":{"kind":"g0","code":65}}]},
            "resources":{"objects":{"public":null,"global":null},
                "drcs":{"normal":null,"global":null},"defaults":[null,null,null,null]}}],
        "magazines":[{"magazine":2,"presentation":{"primary":8,"secondary":1,
            "palette":{"clut2":vec![[1,2,3];8],"clut3":vec![[4,5,6];8]},
            "screen_colour":16,"row_colour":23,"black_background_substitution":true,
            "remapping":"fg_clut2_bg_clut3","panels":{"left":true,"right":true,
                "status":"level25_and35","width":4}}}],
        "objects":[],"drcs":[],"mots":[],"empty_object_pages":[],
        "mot_selections":[{"display":{"page":201,"subcode":16},"mot":null}]})).unwrap()
}

fn mixed_receipt(selected: TeletextSelectionSnapshot) -> TeletextProfileRuntimeReceipt {
    if selected.contract == TeletextPageContract::V2 {
        receipt(selected, authored_output())
    } else {
        basic(selected)
    }
}

#[tokio::test]
async fn mixed_documents_keep_exact_requests_order_deadline_and_authored_metadata() {
    use ersatzrs_domain::teletext as d;
    let repo = mixed_repository();
    let before = repo.selection.lock().unwrap().clone();
    let calls = Mutex::new(Vec::new());
    let context = context();
    let input = prepare_with_ports(
        &repo,
        9,
        Some(B),
        &context,
        input_budget(),
        |selected, input, deadline| {
            let expected =
                build_request_for_schema(&context, selected.contract.schema_version()).unwrap();
            assert_eq!(input, expected);
            let request = wire::TeletextPagesRequest::from_json_for_schema(
                &input,
                selected.contract.schema_version(),
            )
            .unwrap();
            assert_eq!(request.captured_at, context.captured_at);
            assert_eq!(request.timezone, context.timezone);
            calls
                .lock()
                .unwrap()
                .push((selected.addon_id.clone(), deadline));
            async move { Ok(mixed_receipt(selected)) }
        },
        Instant::now,
    )
    .await
    .unwrap();
    let deadline = input.deadline();
    let prepared = input.validate(document_budget()).unwrap();
    assert_eq!(prepared.selection(), &before);
    assert_eq!(prepared.generation(), GENERATION);
    assert_eq!(prepared.entry_addon_id(), Some(B));
    assert_eq!(prepared.deadline(), deadline);
    assert_eq!(prepared.sources().len(), 2);
    for (index, expected) in [basic(binding(A, 1)).stdout, authored_output()]
        .into_iter()
        .enumerate()
    {
        let source = &prepared.sources()[index];
        assert_eq!(source.bytes(), expected);
        assert_eq!(source.contract(), before.bindings[index].contract);
        assert_eq!(source.generation(), GENERATION);
        assert!(!format!("{source:?}").contains(GENERATION));
    }
    let TeletextProfileDocument::Base(base) = &prepared.documents()[0] else {
        panic!("v1 changed contract")
    };
    assert_eq!(base.initial_page.get(), 200);
    assert_eq!(base.pages[0].national_option.get(), 5);
    let TeletextProfileDocument::Authored(declared) = &prepared.documents()[1] else {
        panic!("v2 downgraded")
    };
    let service = declared.semantic();
    let owner = service.base().pages[0].address;
    assert_eq!(owner.subcode.get(), 16);
    assert_eq!(service.base().language.as_str(), "eng");
    assert!(service.page_enhancements()[&owner].presentation.is_none());
    assert!(service.page_enhancements()[&owner]
        .compositional_links
        .is_none());
    let (source, program) = service.effective_program(owner).unwrap();
    assert_eq!(source, d::TeletextEffectiveProgramSource::Local);
    assert_eq!(program.commands().len(), 3);
    assert!(
        matches!(program.commands()[1], d::TeletextEnhancementCommand::AtColumn {
        operation:d::TeletextColumnOperation::DesignateCharset {designation},..} if designation.get()==8)
    );
    let presentation = service.magazine_presentations()[&2];
    assert_eq!(presentation.primary.get(), 8);
    assert_eq!(presentation.secondary.unwrap().get(), 1);
    assert_eq!(presentation.palette.clut2[0].get(), [1, 2, 3]);
    assert_eq!(presentation.palette.clut3[7].get(), [4, 5, 6]);
    assert_eq!(presentation.panels.level25_widths(), (4, 12));
    assert_eq!(
        calls.lock().unwrap().as_slice(),
        &[(A.to_owned(), deadline), (B.to_owned(), deadline)]
    );
    assert_eq!(repo.reads.load(Ordering::SeqCst), 2);
    assert_eq!(*repo.selection.lock().unwrap(), before);
    let (selected, generation, documents, sources, entry, end) = prepared.into_parts_with_sources();
    assert_eq!(selected, before);
    assert_eq!(generation, GENERATION);
    assert_eq!(documents.len(), 2);
    assert_eq!(sources.len(), 2);
    assert_eq!(sources[0].bytes(), basic(binding(A, 1)).stdout);
    assert_eq!(sources[1].bytes(), authored_output());
    assert_eq!(entry.as_deref(), Some(B));
    assert_eq!(end, deadline);
}

#[tokio::test]
async fn off_is_fenced_empty_and_on_requires_selected_entry_without_writes() {
    let repo = Repository::new(vec![]);
    let input = prepare_with_ports(
        &repo,
        9,
        Some("obsolete"),
        &context(),
        TeletextChannelDocumentInputBudget {
            max_response_bytes: 0,
        },
        |_, _, _| async { panic!("Off invoked producer") },
        Instant::now,
    )
    .await
    .unwrap();
    let prepared = input.validate(document_budget()).unwrap();
    assert!(prepared.documents().is_empty());
    assert!(prepared.sources().is_empty());
    assert!(prepared.entry_addon_id().is_none());
    assert_eq!(repo.reads.load(Ordering::SeqCst), 2);
    for (entry, missing) in [(None, true), (Some(C), false)] {
        let result = prepare_with_ports(
            &Repository::two(),
            9,
            entry,
            &context(),
            input_budget(),
            |_, _, _| async { panic!("invalid entry invoked") },
            Instant::now,
        )
        .await;
        assert!(matches!(
            (&result, missing),
            (Err(ChannelTeletextDocumentError::EntryAddonRequired), true)
                | (
                    Err(ChannelTeletextDocumentError::EntryAddonNotSelected),
                    false
                )
        ));
    }
    let repo = mixed_repository();
    let mut invalid = context();
    invalid.locale = "xx";
    assert!(matches!(
        prepare_with_ports(
            &repo,
            9,
            Some(A),
            &invalid,
            input_budget(),
            |_, _, _| async { panic!("guide preflight failed after invocation") },
            Instant::now
        )
        .await,
        Err(ChannelTeletextDocumentError::Request(_))
    ));
    assert_eq!(repo.reads.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn off_correlation_retention_obeys_generation_byte_bound_without_building_guide() {
    for generation in [String::new(), "g".repeat(1025), "é".repeat(513)] {
        for enabled in [false, true] {
            let repo = if enabled {
                Repository::two()
            } else {
                Repository::new(vec![])
            };
            let mut context = context();
            context.generation = &generation;
            let result = prepare_with_ports(
                &repo,
                9,
                Some(A),
                &context,
                input_budget(),
                |_, _, _| async { panic!("invalid generation invoked") },
                Instant::now,
            )
            .await;
            assert!(matches!(
                result,
                Err(ChannelTeletextDocumentError::Request(
                    TeletextGuideRequestError::Contract(
                        wire::TeletextPagesRequestError::InvalidGeneration
                    )
                ))
            ));
            assert_eq!(repo.reads.load(Ordering::SeqCst), 1);
        }
    }
    for generation in ["g".repeat(1024), "é".repeat(512)] {
        let mut context = context();
        context.generation = &generation;
        context.locale = "";
        context.timezone = "not a guide timezone";
        let repo = Repository::new(vec![]);
        let input = prepare_with_ports(
            &repo,
            9,
            None,
            &context,
            TeletextChannelDocumentInputBudget {
                max_response_bytes: 0,
            },
            |_, _, _| async { panic!("Off invoked producer") },
            Instant::now,
        )
        .await
        .unwrap();
        let prepared = input.validate(document_budget()).unwrap();
        assert_eq!(prepared.generation(), generation);
        assert!(prepared.documents().is_empty());
        assert_eq!(repo.reads.load(Ordering::SeqCst), 2);
    }
}

#[tokio::test]
async fn complete_membership_and_every_selection_dimension_are_fenced_after_producers() {
    for dimension in 0..8 {
        let repo = mixed_repository();
        let mut changed = repo.selection.lock().unwrap().clone();
        match dimension {
            0 => {
                changed.bindings.pop();
            }
            1 => changed.bindings.swap(0, 1),
            2 => changed.bindings[1].binding_revision += 1,
            3 => changed.bindings[1].profile_revision += 1,
            4 => changed.bindings[1].profile_id += 1,
            5 => changed.bindings[1].installation.content_sha256.push('x'),
            6 => changed.bindings[1].effective_settings.clear(),
            _ => changed.bindings[1].contract = TeletextPageContract::V1,
        }
        *repo.final_selection.lock().unwrap() = Some(Mutation::Saved(changed));
        let calls = Cell::new(0);
        let result = prepare_with_ports(
            &repo,
            9,
            Some(A),
            &context(),
            input_budget(),
            |selected, _, _| {
                calls.set(calls.get() + 1);
                async move { Ok(mixed_receipt(selected)) }
            },
            Instant::now,
        )
        .await;
        assert!(matches!(
            result,
            Err(ChannelTeletextDocumentError::SelectionChanged)
        ));
        assert_eq!(calls.get(), 2);
    }
    let repo = mixed_repository();
    *repo.final_selection.lock().unwrap() = Some(Mutation::Rejected(
        TeletextProfileRejection::InstallationUnavailable,
    ));
    assert!(matches!(
        prepare_with_ports(
            &repo,
            9,
            Some(A),
            &context(),
            input_budget(),
            |selected, _, _| async move { Ok(mixed_receipt(selected)) },
            Instant::now
        )
        .await,
        Err(ChannelTeletextDocumentError::SelectionChanged)
    ));
}

#[tokio::test]
async fn receipt_mismatch_or_runtime_failure_stops_before_later_binding_without_retry() {
    for defect in 0..5 {
        let calls = Cell::new(0);
        let result = prepare_with_ports(
            &mixed_repository(),
            9,
            Some(A),
            &context(),
            input_budget(),
            |selected, _, _| {
                calls.set(calls.get() + 1);
                let mut response = mixed_receipt(selected);
                match defect {
                    0 => response.selection.contract = TeletextPageContract::V2,
                    1 => response.selection.installation.version.push('x'),
                    2 => response.selection.effective_settings.clear(),
                    3 => response.generation.push('x'),
                    _ => {}
                }
                async move {
                    if defect == 4 {
                        Err(TeletextProfileRuntimeError::Failed)
                    } else {
                        Ok(response)
                    }
                }
            },
            Instant::now,
        )
        .await;
        assert!(matches!(
            result,
            Err(ChannelTeletextDocumentError::ReceiptMismatch { binding: 0 }
                | ChannelTeletextDocumentError::Runtime { binding: 0, .. })
        ));
        assert_eq!(calls.get(), 1);
    }
}

#[tokio::test]
async fn aggregate_cutoff_is_exact_and_individual_wire_limit_is_independent() {
    let repo = mixed_repository();
    let exact = basic(binding(A, 1)).stdout.len() + authored_output().len();
    let input = prepare_with_ports(
        &repo,
        9,
        Some(A),
        &context(),
        TeletextChannelDocumentInputBudget {
            max_response_bytes: exact,
        },
        |selected, _, _| async move { Ok(mixed_receipt(selected)) },
        Instant::now,
    )
    .await
    .unwrap();
    assert_eq!(
        input.validate(document_budget()).unwrap().documents().len(),
        2
    );
    for maximum in [0, exact - 1] {
        let calls = Cell::new(0);
        let result = prepare_with_ports(
            &mixed_repository(),
            9,
            Some(A),
            &context(),
            TeletextChannelDocumentInputBudget {
                max_response_bytes: maximum,
            },
            |selected, _, _| {
                calls.set(calls.get() + 1);
                async move { Ok(mixed_receipt(selected)) }
            },
            Instant::now,
        )
        .await;
        let offender = usize::from(maximum != 0);
        assert!(
            matches!(result,Err(ChannelTeletextDocumentError::AggregateResponseBudget {binding,maximum:limit})
            if binding==offender && limit==maximum)
        );
        assert_eq!(calls.get(), offender + 1);
    }
    assert!(matches!(
        prepare_with_ports(
            &Repository::two(),
            9,
            Some(A),
            &context(),
            TeletextChannelDocumentInputBudget {
                max_response_bytes: usize::MAX
            },
            |selected, _, _| async move {
                Ok(receipt(
                    selected,
                    vec![b' '; wire::MAX_TELETEXT_PAGES_OUTPUT_BYTES + 1],
                ))
            },
            Instant::now
        )
        .await,
        Err(ChannelTeletextDocumentError::ResponseTooLarge { binding: 0 })
    ));
}

#[tokio::test]
async fn validation_has_no_partial_result_contract_fallback_or_raw_debug_output() {
    for defect in 0..3 {
        let input = prepare_with_ports(
            &mixed_repository(),
            9,
            Some(A),
            &context(),
            input_budget(),
            |selected, _, _| async move {
                let mut response = mixed_receipt(selected);
                if response.selection.contract == TeletextPageContract::V2 {
                    if defect == 0 {
                        response.stdout = output("eng", 201, &[page(201, 0, "eng", 0)]);
                    }
                    if defect == 1 {
                        response.stdout = b"private malformed producer text".to_vec();
                    }
                }
                Ok(response)
            },
            Instant::now,
        )
        .await
        .unwrap();
        let debug = format!("{input:?}");
        assert!(!debug.contains(GENERATION));
        assert!(!debug.contains("private malformed producer text"));
        let mut budget = document_budget();
        if defect == 2 {
            budget.declarations.max_displays = 0;
        }
        let error = input.validate(budget).unwrap_err();
        assert!(matches!(
            error,
            ChannelTeletextDocumentError::Document { binding: 1, .. }
        ));
        assert!(!format!("{error:?}").contains("private malformed producer text"));
    }
}

#[tokio::test]
async fn one_original_deadline_expires_at_each_snapshot_and_invocation_boundary() {
    let base = Instant::now();
    for expired_tick in 1..=8 {
        let repo = mixed_repository();
        let ticks = Cell::new(0);
        let calls = Cell::new(0);
        let result = prepare_with_ports(
            &repo,
            9,
            Some(A),
            &context(),
            input_budget(),
            |selected, _, deadline| {
                assert_eq!(deadline, base + PREPARATION_TIMEOUT);
                calls.set(calls.get() + 1);
                async move { Ok(mixed_receipt(selected)) }
            },
            || {
                let tick = ticks.get();
                ticks.set(tick + 1);
                if tick == expired_tick {
                    base + PREPARATION_TIMEOUT
                } else {
                    base
                }
            },
        )
        .await;
        assert!(
            matches!(result, Err(ChannelTeletextDocumentError::TimedOut)),
            "tick {expired_tick}"
        );
        assert_eq!(
            calls.get(),
            match expired_tick {
                1..=3 => 0,
                4 | 5 => 1,
                _ => 2,
            }
        );
        assert_eq!(
            repo.reads.load(Ordering::SeqCst),
            if expired_tick == 8 { 2 } else { 1 }
        );
    }
    let past = Instant::now().checked_sub(Duration::from_mins(1)).unwrap();
    let input = prepare_with_ports(
        &Repository::new(vec![binding(A, 1)]),
        9,
        Some(A),
        &context(),
        input_budget(),
        |selected, _, _| async move { Ok(receipt(selected, b"not JSON".to_vec())) },
        || past,
    )
    .await
    .unwrap();
    assert!(matches!(
        input.validate(document_budget()),
        Err(ChannelTeletextDocumentError::TimedOut)
    ));
}

#[tokio::test]
async fn post_cpu_recheck_uses_whole_channel_and_original_deadline() {
    let repo = mixed_repository();
    let input = prepare_with_ports(
        &repo,
        9,
        Some(A),
        &context(),
        input_budget(),
        |selected, _, _| async move { Ok(mixed_receipt(selected)) },
        Instant::now,
    )
    .await
    .unwrap();
    let prepared = input.validate(document_budget()).unwrap();
    recheck_with_ports(
        &repo,
        prepared.selection(),
        prepared.deadline(),
        Instant::now,
    )
    .await
    .unwrap();
    repo.selection.lock().unwrap().bindings[1]
        .installation
        .manifest_sha256
        .push('x');
    assert!(matches!(
        recheck_with_ports(
            &repo,
            prepared.selection(),
            prepared.deadline(),
            Instant::now
        )
        .await,
        Err(ChannelTeletextDocumentError::SelectionChanged)
    ));
    for after_read in [false, true] {
        let reads = repo.reads.load(Ordering::SeqCst);
        let ticks = Cell::new(0);
        let now = Instant::now();
        assert!(matches!(
            recheck_with_ports(&repo, prepared.selection(), prepared.deadline(), || {
                let tick = ticks.get();
                ticks.set(tick + 1);
                if !after_read || tick == 1 {
                    prepared.deadline()
                } else {
                    now
                }
            })
            .await,
            Err(ChannelTeletextDocumentError::TimedOut)
        ));
        assert_eq!(
            repo.reads.load(Ordering::SeqCst),
            reads + usize::from(after_read)
        );
    }
}

#[tokio::test]
async fn bounded_selection_shape_and_storage_fail_before_invocation() {
    let mut unavailable = Repository::two();
    unavailable.fail_at_read = 0;
    assert!(matches!(
        prepare_with_ports(
            &unavailable,
            9,
            Some(A),
            &context(),
            input_budget(),
            |_, _, _| async { panic!("unavailable invoked") },
            Instant::now
        )
        .await,
        Err(ChannelTeletextDocumentError::StorageUnavailable)
    ));
    for defect in 0..4 {
        let bindings = if defect == 3 {
            vec![binding(A, 1); MAX_BINDINGS + 1]
        } else {
            vec![binding(A, 1), binding(B, 2)]
        };
        let repo = Repository::new(bindings);
        match defect {
            0 => repo.selection.lock().unwrap().channel_id = 10,
            1 => repo.selection.lock().unwrap().bindings[1].channel_id = 10,
            2 => repo.selection.lock().unwrap().bindings[1].addon_id = A.to_owned(),
            _ => {}
        }
        let result = prepare_with_ports(
            &repo,
            9,
            Some(A),
            &context(),
            input_budget(),
            |_, _, _| async { panic!("invalid selection invoked") },
            Instant::now,
        )
        .await;
        assert!(matches!(
            result,
            Err(ChannelTeletextDocumentError::SelectionShape
                | ChannelTeletextDocumentError::BindingCount)
        ));
    }
}
