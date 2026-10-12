//! Synthetic port tests; successful preparation grants no publication authority.
use super::*;
use std::time::{Duration, Instant};

use crate::teletext_profile_documents::{
    prepare_input_with_ports, recheck_with_ports, BoundTeletextDocumentError,
    TeletextProfileDocument, TeletextProfileDocumentBudget,
};
use ersatzrs_addon_contract as wire;
use ersatzrs_domain::teletext as d;

fn budgets() -> TeletextProfileDocumentBudget {
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

fn authored_output() -> Vec<u8> {
    let base: serde_json::Value =
        serde_json::from_slice(&output(GENERATION, "Café <b>page</b>")).unwrap();
    serde_json::to_vec(&json!({"schema_version":2,"generation":GENERATION,"language":"nld","initial_page":200,
        "pages":base["pages"],
        "page_enhancements":[{"address":{"page":200,"subcode":0},"presentation":null,
            "compositional_links":null,"local_program":{"commands":[
                {"kind":"set_position","position":{"row":1,"column":8}},
                {"kind":"at_column","column":8,"operation":{"kind":"designate_charset","designation":8}},
                {"kind":"at_column","column":8,"operation":{"kind":"g0","code":65}}]},
            "resources":{"objects":{"public":null,"global":null},"drcs":{"normal":null,"global":null},
                "defaults":[null,null,null,null]}}],
        "magazines":[{"magazine":2,"presentation":{"primary":8,"secondary":1,
            "palette":{"clut2":vec![[1,2,3];8],"clut3":vec![[4,5,6];8]},
            "screen_colour":16,"row_colour":23,"black_background_substitution":true,
            "remapping":"fg_clut2_bg_clut3",
            "panels":{"left":true,"right":true,"status":"level25_and35","width":4}}}],
        "objects":[],"drcs":[],"mots":[],
        "mot_selections":[{"display":{"page":200,"subcode":0},"mot":null}],
        "empty_object_pages":[]})).unwrap()
}

fn authored_repository() -> Repository {
    let repo = Repository::new();
    repo.selections
        .lock()
        .unwrap()
        .get_mut(&1)
        .unwrap()
        .contract = ersatzrs_domain::TeletextPageContract::V2;
    repo
}

fn document_receipt(
    selected: TeletextSelectionSnapshot,
    stdout: Vec<u8>,
) -> TeletextProfileRuntimeReceipt {
    TeletextProfileRuntimeReceipt {
        selection: selected,
        generation: GENERATION.to_owned(),
        stdout,
    }
}

#[test]
fn validated_sources_move_exact_response_allocations_and_preserve_local_presence() {
    use crate::teletext_profile_documents::BoundTeletextDocumentInput;
    for shape in 0..4 {
        let mut selected = selection(1);
        let mut value: serde_json::Value = if shape == 0 {
            serde_json::from_slice(&output(GENERATION, "Literal <b>page</b>")).unwrap()
        } else {
            selected.contract = ersatzrs_domain::TeletextPageContract::V2;
            serde_json::from_slice(&authored_output()).unwrap()
        };
        match shape {
            1 => value["page_enhancements"][0]["local_program"] = serde_json::Value::Null,
            2 => value["page_enhancements"][0]["local_program"] = json!({"commands":[]}),
            _ => {}
        }
        let mut bytes = serde_json::to_vec_pretty(&value).unwrap();
        bytes.extend_from_slice(b"\n \t");
        let expected = bytes.clone();
        let allocation = bytes.as_ptr();
        let deadline = Instant::now().checked_add(Duration::from_secs(30)).unwrap();
        let prepared = BoundTeletextDocumentInput::from_channel_receipt(
            document_receipt(selected.clone(), bytes),
            deadline,
        )
        .validate(budgets())
        .unwrap();
        assert_eq!(prepared.source().bytes(), expected);
        assert_eq!(prepared.source().bytes().as_ptr(), allocation);
        assert_eq!(prepared.source().contract(), selected.contract);
        assert_eq!(prepared.source().generation(), GENERATION);
        let debug = format!("{:?}", prepared.source());
        assert!(!debug.contains(GENERATION));
        assert!(!debug.contains("Literal"));
        assert!(!debug.contains("Café"));
        if let TeletextProfileDocument::Authored(declared) = prepared.document() {
            let owner = declared.semantic().base().pages[0].address;
            let program = &declared.semantic().page_enhancements()[&owner].local_program;
            match shape {
                1 => assert!(program.is_none()),
                2 => assert!(program.as_ref().unwrap().commands().is_empty()),
                _ => assert_eq!(program.as_ref().unwrap().commands().len(), 3),
            }
        }
        let (observed, correlation, _, source, end) = prepared.into_parts_with_source();
        assert_eq!(observed, selected);
        assert_eq!(correlation, GENERATION);
        assert_eq!(source.bytes(), expected);
        assert_eq!(source.bytes().as_ptr(), allocation);
        assert_eq!(end, deadline);
    }
}

#[test]
fn raw_source_failures_never_create_validated_retained_records() {
    use crate::teletext_profile_documents::BoundTeletextDocumentInput;
    for contract in [
        ersatzrs_domain::TeletextPageContract::V1,
        ersatzrs_domain::TeletextPageContract::V2,
    ] {
        for bytes in [
            vec![b' '; wire::MAX_TELETEXT_PAGES_OUTPUT_BYTES + 1],
            vec![0xff],
            b"private malformed source".to_vec(),
        ] {
            let mut selected = selection(1);
            selected.contract = contract;
            let deadline = Instant::now().checked_add(Duration::from_secs(30)).unwrap();
            let result = BoundTeletextDocumentInput::from_channel_receipt(
                document_receipt(selected, bytes),
                deadline,
            )
            .validate(budgets());
            let error = result.unwrap_err();
            assert!(matches!(
                error,
                BoundTeletextDocumentError::BaseResponse(_)
                    | BoundTeletextDocumentError::AuthoredResponse(_)
            ));
            assert!(!format!("{error:?}").contains("private malformed source"));
        }
    }
}

#[tokio::test]
async fn selected_v2_input_preserves_complete_authored_fallback_and_packet_presence() {
    let repo = authored_repository();
    let before = repo.selections.lock().unwrap().clone();
    let guides = [TeletextGuideChannel {
        id: 7,
        number: "7",
        name: "Synthetic source",
        programmes: &[],
    }];
    let context = context(&guides);
    let captured_at = context.captured_at;
    let input = prepare_input_with_ports(
        &repo,
        1,
        ADDON,
        3,
        &context,
        |selected, input, deadline| async move {
            assert_eq!(selected.contract, ersatzrs_domain::TeletextPageContract::V2);
            let request = wire::TeletextPagesRequest::from_json_for_schema(&input, 2).unwrap();
            assert_eq!(request.channels[0].id, 7);
            assert_eq!(request.generation, GENERATION);
            assert_eq!(request.captured_at, captured_at);
            assert!(deadline > Instant::now());
            assert!(wire::TeletextPagesRequest::from_json(&input).is_err());
            Ok(document_receipt(selected, authored_output()))
        },
        Instant::now,
    )
    .await
    .unwrap();
    let deadline = input.deadline();
    let prepared = input.validate(budgets()).unwrap();
    assert_eq!(prepared.selection(), &before[&1]);
    assert_eq!(prepared.generation(), GENERATION);
    assert_eq!(prepared.deadline(), deadline);
    let TeletextProfileDocument::Authored(declared) = prepared.document() else {
        panic!("v2 was downgraded")
    };
    let service = declared.semantic();
    assert_eq!(
        service.base().pages[0].rows[0].runs[0].content,
        d::TeletextRunContent::Text {
            text: "Café <b>page</b>".into()
        }
    );
    let owner = service.base().pages[0].address;
    assert!(service.page_enhancements()[&owner].presentation.is_none());
    assert!(service.page_enhancements()[&owner]
        .compositional_links
        .is_none());
    let (source, program) = service.effective_program(owner).unwrap();
    assert_eq!(source, d::TeletextEffectiveProgramSource::Local);
    assert_eq!(program.commands().len(), 3);
    assert!(
        matches!(program.commands()[1], d::TeletextEnhancementCommand::AtColumn {
        operation:d::TeletextColumnOperation::DesignateCharset { designation }, .. } if designation.get() == 8)
    );
    let presentation = service.magazine_presentations()[&2];
    assert_eq!(presentation.primary.get(), 8);
    assert_eq!(presentation.secondary.unwrap().get(), 1);
    assert_eq!(presentation.palette.clut2[0].get(), [1, 2, 3]);
    assert_eq!(presentation.palette.clut3[7].get(), [4, 5, 6]);
    assert_eq!(presentation.panels.level25_widths(), (4, 12));
    assert_eq!(*repo.selections.lock().unwrap(), before);
    assert_eq!(repo.reads.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn selected_v1_document_keeps_exact_guide_bytes_and_base_draft() {
    let repo = Repository::new();
    let context = context(&[]);
    let expected = crate::teletext_guide::build_request(&context).unwrap();
    let input = prepare_input_with_ports(
        &repo,
        1,
        ADDON,
        3,
        &context,
        |selected, actual, _| async move {
            assert_eq!(actual, expected);
            Ok(receipt(selected))
        },
        Instant::now,
    )
    .await
    .unwrap();
    let prepared = input.validate(budgets()).unwrap();
    assert_eq!(prepared.selection(), &selection(1));
    let TeletextProfileDocument::Base(base) = prepared.document() else {
        panic!("v1 became authored")
    };
    assert_eq!(base.initial_page.get(), 200);
    assert_eq!(
        base.pages[0].rows[0].runs[0].content,
        d::TeletextRunContent::Text {
            text: "Literal <b>page</b>".into()
        }
    );
    let original_deadline = prepared.deadline();
    let (selected, generation, document, deadline) = prepared.into_parts();
    assert_eq!(selected, selection(1));
    assert_eq!(generation, GENERATION);
    assert_eq!(deadline, original_deadline);
    assert!(matches!(document, TeletextProfileDocument::Base(_)));
}

#[tokio::test]
async fn response_contract_and_generation_never_fall_back_or_retry() {
    for contract in [
        ersatzrs_domain::TeletextPageContract::V1,
        ersatzrs_domain::TeletextPageContract::V2,
    ] {
        for wrong_generation in [false, true] {
            let repo = Repository::new();
            repo.selections
                .lock()
                .unwrap()
                .get_mut(&1)
                .unwrap()
                .contract = contract;
            let stdout = if wrong_generation {
                let mut value: serde_json::Value = serde_json::from_slice(&if contract
                    == ersatzrs_domain::TeletextPageContract::V1
                {
                    output(GENERATION, "Synthetic")
                } else {
                    authored_output()
                })
                .unwrap();
                value["generation"] = json!("wrong private generation");
                serde_json::to_vec(&value).unwrap()
            } else if contract == ersatzrs_domain::TeletextPageContract::V1 {
                authored_output()
            } else {
                output(GENERATION, "Synthetic")
            };
            let input = prepare_input_with_ports(
                &repo,
                1,
                ADDON,
                3,
                &context(&[]),
                |selected, _, _| async move { Ok(document_receipt(selected, stdout)) },
                Instant::now,
            )
            .await
            .unwrap();
            let result = input.validate(budgets());
            assert!(matches!(
                result,
                Err(BoundTeletextDocumentError::BaseResponse(_)
                    | BoundTeletextDocumentError::AuthoredResponse(_))
            ));
            assert_eq!(repo.reads.load(Ordering::SeqCst), 2);
        }
    }
}

#[tokio::test]
async fn mismatched_receipt_contract_installation_settings_or_generation_stops_before_fresh_read() {
    for change in 0..6 {
        let repo = authored_repository();
        let result = prepare_input_with_ports(
            &repo,
            1,
            ADDON,
            3,
            &context(&[]),
            |selected, _, _| async move {
                let mut receipt = document_receipt(selected, authored_output());
                match change {
                    0 => receipt.selection.contract = ersatzrs_domain::TeletextPageContract::V1,
                    1 => receipt.selection.installation.package_sha256 = "changed".into(),
                    2 => receipt.selection.effective_settings.clear(),
                    3 => receipt.selection.profile_revision += 1,
                    4 => receipt.selection.channel_id = 2,
                    _ => receipt.generation = "changed private generation".into(),
                }
                Ok(receipt)
            },
            Instant::now,
        )
        .await;
        assert!(matches!(
            result,
            Err(BoundTeletextDocumentError::Preparation(
                BoundTeletextPagesError::ReceiptMismatch
            ))
        ));
        assert_eq!(repo.reads.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn fresh_selection_changes_and_storage_failure_reject_complete_output() {
    for change in 0..6 {
        let mut repo = authored_repository();
        if change == 5 {
            repo.fail_at_read = 1;
        }
        let result = prepare_input_with_ports(
            &repo,
            1,
            ADDON,
            3,
            &context(&[]),
            |selected, _, _| async {
                if change == 4 {
                    *repo.rejection.lock().unwrap() =
                        Some(TeletextProfileRejection::InstallationUnavailable);
                } else if change != 5 {
                    let mut selections = repo.selections.lock().unwrap();
                    let current = selections.get_mut(&1).unwrap();
                    match change {
                        0 => current.contract = ersatzrs_domain::TeletextPageContract::V1,
                        1 => current.installation.state_revision = "changed".into(),
                        2 => current.effective_settings.clear(),
                        _ => current.binding_revision += 1,
                    }
                }
                Ok(document_receipt(selected, authored_output()))
            },
            Instant::now,
        )
        .await;
        if change == 5 {
            assert!(matches!(
                result,
                Err(BoundTeletextDocumentError::Preparation(
                    BoundTeletextPagesError::StorageUnavailable
                ))
            ));
        } else {
            assert!(matches!(
                result,
                Err(BoundTeletextDocumentError::Preparation(
                    BoundTeletextPagesError::SelectionChanged
                ))
            ));
        }
        assert_eq!(repo.reads.load(Ordering::SeqCst), 2);
    }
}

#[tokio::test]
async fn unavailable_selection_bad_guide_and_runtime_failures_do_not_retry() {
    let mut repo = authored_repository();
    repo.fail_at_read = 0;
    let result = prepare_input_with_ports(
        &repo,
        1,
        ADDON,
        3,
        &context(&[]),
        |_, _, _| async { panic!("unavailable storage invoked runtime") },
        Instant::now,
    )
    .await;
    assert!(matches!(
        result,
        Err(BoundTeletextDocumentError::Preparation(
            BoundTeletextPagesError::StorageUnavailable
        ))
    ));
    for reason in [
        TeletextProfileRejection::DisabledBinding,
        TeletextProfileRejection::InstallationUnavailable,
    ] {
        let repo = authored_repository();
        *repo.rejection.lock().unwrap() = Some(reason);
        let result = prepare_input_with_ports(
            &repo,
            1,
            ADDON,
            3,
            &context(&[]),
            |_, _, _| async { panic!("unavailable selection invoked runtime") },
            Instant::now,
        )
        .await;
        assert!(
            matches!(result, Err(BoundTeletextDocumentError::Preparation(BoundTeletextPagesError::SelectionRejected(actual))) if actual == reason)
        );
    }
    let repo = authored_repository();
    let mut invalid = context(&[]);
    invalid.locale = "unsupported";
    let result = prepare_input_with_ports(
        &repo,
        1,
        ADDON,
        3,
        &invalid,
        |_, _, _| async { panic!("bad guide invoked runtime") },
        Instant::now,
    )
    .await;
    assert!(matches!(
        result,
        Err(BoundTeletextDocumentError::Preparation(
            BoundTeletextPagesError::Request(_)
        ))
    ));
    assert_eq!(repo.reads.load(Ordering::SeqCst), 1);
    for reason in [
        TeletextProfileRuntimeError::CleanupUnconfirmed,
        TeletextProfileRuntimeError::TimedOut,
    ] {
        let repo = authored_repository();
        let result = prepare_input_with_ports(
            &repo,
            1,
            ADDON,
            3,
            &context(&[]),
            |_, _, _| async move { Err(reason) },
            Instant::now,
        )
        .await;
        assert!(
            matches!(result, Err(BoundTeletextDocumentError::Preparation(BoundTeletextPagesError::Runtime(actual))) if actual == reason)
        );
        assert_eq!(repo.reads.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn raw_receipt_debug_and_validation_failures_never_expose_partial_document() {
    let mut semantic: serde_json::Value = serde_json::from_slice(&authored_output()).unwrap();
    semantic["page_enhancements"][0]["resources"]["objects"]["public"] =
        json!({"magazine":2,"page":160});
    let mut malformed = semantic.clone();
    malformed["page_enhancements"][0]["local_program"]["commands"][2]["operation"]["code"] =
        json!(31);
    for stdout in [
        serde_json::to_vec(&semantic).unwrap(),
        serde_json::to_vec(&malformed).unwrap(),
        vec![b'x'; wire::MAX_TELETEXT_PAGES_OUTPUT_BYTES + 1],
    ] {
        let repo = authored_repository();
        let input = prepare_input_with_ports(
            &repo,
            1,
            ADDON,
            3,
            &context(&[]),
            |selected, _, _| async move { Ok(document_receipt(selected, stdout)) },
            Instant::now,
        )
        .await
        .unwrap();
        let debug = format!("{input:?}");
        assert!(!debug.contains(GENERATION));
        assert!(!debug.contains("Café"));
        assert!(!debug.contains("day"));
        assert!(matches!(
            input.validate(budgets()),
            Err(BoundTeletextDocumentError::AuthoredResponse(_))
        ));
    }
    for authored in [false, true] {
        let repo = if authored {
            authored_repository()
        } else {
            Repository::new()
        };
        let input = prepare_input_with_ports(
            &repo,
            1,
            ADDON,
            3,
            &context(&[]),
            |selected, _, _| async move {
                Ok(if authored {
                    document_receipt(selected, authored_output())
                } else {
                    receipt(selected)
                })
            },
            Instant::now,
        )
        .await
        .unwrap();
        let mut budget = budgets();
        budget.service.max_records = 0;
        let result = input.validate(budget);
        assert!(matches!(
            result,
            Err(BoundTeletextDocumentError::AuthoredResponse(_)
                | BoundTeletextDocumentError::ServiceBudget(_))
        ));
    }
    let repo = authored_repository();
    let input = prepare_input_with_ports(
        &repo,
        1,
        ADDON,
        3,
        &context(&[]),
        |selected, _, _| async move { Ok(document_receipt(selected, authored_output())) },
        Instant::now,
    )
    .await
    .unwrap();
    let prepared = input.validate(budgets()).unwrap();
    let debug = format!("{prepared:?}");
    assert!(!debug.contains(GENERATION));
    assert!(!debug.contains("Café"));
    assert!(!debug.contains("day"));
}

#[tokio::test]
async fn original_deadline_is_checked_before_invocation_after_receipt_and_after_fresh_snapshot() {
    for expired_call in 1..=4 {
        let repo = authored_repository();
        let start = Instant::now();
        let invoked = AtomicUsize::new(0);
        let mut clock_calls = 0;
        let result = prepare_input_with_ports(
            &repo,
            1,
            ADDON,
            3,
            &context(&[]),
            |selected, _, deadline| {
                invoked.fetch_add(1, Ordering::SeqCst);
                assert_eq!(deadline, start + Duration::from_secs(30));
                async move { Ok(document_receipt(selected, authored_output())) }
            },
            || {
                let call = clock_calls;
                clock_calls += 1;
                if call == expired_call {
                    start + Duration::from_secs(30)
                } else {
                    start
                }
            },
        )
        .await;
        assert!(matches!(result, Err(BoundTeletextDocumentError::TimedOut)));
        assert_eq!(
            invoked.load(Ordering::SeqCst),
            usize::from(expired_call >= 3)
        );
        assert_eq!(
            repo.reads.load(Ordering::SeqCst),
            if expired_call == 4 { 2 } else { 1 }
        );
    }
}

#[tokio::test]
async fn supplied_admission_deadline_shortens_bound_document_work_without_renewal() {
    use crate::teletext_profile_documents::{prepare_input_with_ports_before, PREPARATION_TIMEOUT};

    let start = Instant::now();
    for supplied in [
        start + Duration::from_secs(15),
        start + Duration::from_mins(1),
    ] {
        let repo = authored_repository();
        let before = repo.selections.lock().unwrap().clone();
        let expected = supplied.min(start + PREPARATION_TIMEOUT);
        let invoked = AtomicUsize::new(0);
        let input = prepare_input_with_ports_before(
            &repo,
            1,
            ADDON,
            3,
            &context(&[]),
            |selected, _, deadline| {
                assert_eq!(deadline, expected);
                invoked.fetch_add(1, Ordering::SeqCst);
                async move { Ok(document_receipt(selected, authored_output())) }
            },
            (|| start, Some(supplied)),
        )
        .await
        .unwrap();
        assert_eq!(input.deadline(), expected);
        assert_eq!(invoked.load(Ordering::SeqCst), 1);
        assert_eq!(repo.reads.load(Ordering::SeqCst), 2);
        assert_eq!(*repo.selections.lock().unwrap(), before);
    }
    let repo = authored_repository();
    let mut invalid = context(&[]);
    invalid.generation = "";
    let result = prepare_input_with_ports_before(
        &repo,
        1,
        ADDON,
        3,
        &invalid,
        |_, _, _| async { panic!("expired admission invoked producer") },
        (|| start, Some(start)),
    )
    .await;
    assert!(matches!(result, Err(BoundTeletextDocumentError::TimedOut)));
    assert_eq!(repo.reads.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn synchronous_validation_rejects_expired_original_input_before_parsing_without_sleep() {
    let repo = authored_repository();
    let past = Instant::now().checked_sub(Duration::from_mins(1)).unwrap();
    let input = prepare_input_with_ports(
        &repo,
        1,
        ADDON,
        3,
        &context(&[]),
        |selected, _, _| async move {
            Ok(document_receipt(
                selected,
                b"malformed private payload".to_vec(),
            ))
        },
        || past,
    )
    .await
    .unwrap();
    assert!(input.deadline() < Instant::now());
    assert!(matches!(
        input.validate(budgets()),
        Err(BoundTeletextDocumentError::TimedOut)
    ));
    assert_eq!(repo.reads.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn post_validation_fresh_fence_rejects_forged_full_association_or_unavailable_member() {
    for change in 0..6 {
        let repo = authored_repository();
        let mut observed = repo.selections.lock().unwrap()[&1].clone();
        match change {
            0 => observed.contract = ersatzrs_domain::TeletextPageContract::V1,
            1 => observed.profile_id += 1,
            2 => observed.installation.content_sha256 = "changed".into(),
            3 => observed.effective_settings.clear(),
            4 => observed.channel_id = 2,
            _ => observed.profile_revision += 1,
        }
        let result = recheck_with_ports(
            &repo,
            &observed,
            Instant::now() + Duration::from_secs(30),
            Instant::now,
        )
        .await;
        assert!(matches!(
            result,
            Err(BoundTeletextDocumentError::Preparation(
                BoundTeletextPagesError::SelectionChanged
            ))
        ));
        assert_eq!(repo.reads.load(Ordering::SeqCst), 1);
    }
    let repo = authored_repository();
    let observed = repo.selections.lock().unwrap()[&1].clone();
    assert!(recheck_with_ports(
        &repo,
        &observed,
        Instant::now() + Duration::from_secs(30),
        Instant::now
    )
    .await
    .is_ok());
    *repo.rejection.lock().unwrap() = Some(TeletextProfileRejection::InstallationUnavailable);
    assert!(matches!(
        recheck_with_ports(
            &repo,
            &observed,
            Instant::now() + Duration::from_secs(30),
            Instant::now
        )
        .await,
        Err(BoundTeletextDocumentError::Preparation(
            BoundTeletextPagesError::SelectionChanged
        ))
    ));
    let mut repo = authored_repository();
    repo.fail_at_read = 0;
    assert!(matches!(
        recheck_with_ports(
            &repo,
            &observed,
            Instant::now() + Duration::from_secs(30),
            Instant::now
        )
        .await,
        Err(BoundTeletextDocumentError::Preparation(
            BoundTeletextPagesError::StorageUnavailable
        ))
    ));
}

#[tokio::test]
async fn post_validation_fresh_fence_retains_deadline_before_and_after_read() {
    for before_read in [true, false] {
        let repo = authored_repository();
        let observed = repo.selections.lock().unwrap()[&1].clone();
        let start = Instant::now();
        let deadline = start + Duration::from_secs(30);
        let mut calls = 0;
        let result = recheck_with_ports(&repo, &observed, deadline, || {
            calls += 1;
            if before_read || calls == 2 {
                deadline
            } else {
                start
            }
        })
        .await;
        assert!(matches!(result, Err(BoundTeletextDocumentError::TimedOut)));
        assert_eq!(repo.reads.load(Ordering::SeqCst), usize::from(!before_read));
    }
}
