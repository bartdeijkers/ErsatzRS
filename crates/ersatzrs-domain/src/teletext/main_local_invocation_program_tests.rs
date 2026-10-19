//! Main call structure retains complete service state and exact original anchors.
use super::*;
use crate::teletext::{
    TeletextMainLocalInvocationProgramOverlay as Overlay,
    TeletextMainLocalInvocationProgramOverlayError as Error,
};

fn local() -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(address(200, 0), TeletextObjectId::new(65520))
}
fn call() -> TeletextEnhancementCommand {
    invoke(local(), TeletextObjectType::Passive)
}
fn position() -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(1, 0).unwrap(),
    }
}
fn intent(
    before: Vec<TeletextEnhancementCommand>,
    after: Vec<TeletextEnhancementCommand>,
    map: Vec<Option<usize>>,
) -> Overlay {
    Overlay::new(address(200, 0), program(before), program(after), map).unwrap()
}
fn input() -> TeletextResolvedServiceDraft {
    let mut input = draft(&[address(200, 0), address(200, 1)]);
    input.pages[0].local_program = Some(program(vec![call()]));
    input.pages[0].resources.objects.global = Some(raw(0xb0));
    input.pages[0].resources.defaults[0] =
        Some(selection(global(0xb0, 1), TeletextObjectType::Passive));
    input.objects.push(definition(
        global(0xb0, 1),
        TeletextObjectType::Passive,
        vec![],
    ));
    input
        .objects
        .push(definition(local(), TeletextObjectType::Passive, vec![]));
    input
}
#[test]
fn main_call_structure_insertion_deletion_roundtrip_keeps_explicit_empty_and_service_siblings() {
    let original = TeletextSemanticService::new(input(), budget()).unwrap();
    let deleted = original
        .clone()
        .with_main_local_invocation_program_overlays(
            &[intent(vec![call()], vec![], vec![])],
            budget(),
        )
        .unwrap();
    assert_eq!(
        deleted.page_enhancements()[&address(200, 0)].local_program,
        Some(program(vec![]))
    );
    assert_eq!(
        deleted.effective_program(address(200, 0)).unwrap().0,
        TeletextEffectiveProgramSource::Local
    );
    let restored = deleted
        .with_main_local_invocation_program_overlays(
            &[intent(vec![], vec![call()], vec![None])],
            budget(),
        )
        .unwrap();
    assert_eq!(restored, original);
    let inserted = intent(vec![call()], vec![call(), call()], vec![Some(0), None]);
    assert_eq!(inserted.original_command(0), Some(0));
    assert_eq!(inserted.original_command(1), None);
    assert_eq!(inserted.replacement_command(0), Some(0));
    assert_eq!(inserted.replacement_command(1), None);
    assert_eq!(inserted.retained_commands(), [Some(0), None]);
    let changed = original
        .clone()
        .with_main_local_invocation_program_overlays(&[inserted], budget())
        .unwrap();
    assert_eq!(changed.base(), original.base());
    assert_eq!(changed.objects(), original.objects());
    assert_eq!(
        changed.page_enhancements()[&address(200, 1)],
        original.page_enhancements()[&address(200, 1)]
    );
    assert_eq!(
        changed.page_enhancements()[&address(200, 0)].resources,
        original.page_enhancements()[&address(200, 0)].resources
    );
}
#[test]
fn main_call_structure_keeps_all_retained_commands_exact_and_maps_strict() {
    let original = program(vec![position(), call()]);
    for (after, map, error) in [
        (vec![call()], vec![Some(1)], Error::ProtectedCommand),
        (
            vec![position(), call(), position()],
            vec![Some(0), Some(1), None],
            Error::ProtectedCommand,
        ),
        (
            vec![position(), call(), call()],
            vec![Some(0), Some(0), None],
            Error::Correspondence,
        ),
        (
            vec![position(), call(), call()],
            vec![Some(0), Some(2), None],
            Error::Correspondence,
        ),
        (
            vec![position(), call(), call()],
            vec![Some(0), Some(1)],
            Error::Correspondence,
        ),
    ] {
        assert_eq!(
            Overlay::new(address(200, 0), original.clone(), program(after), map),
            Err(error)
        );
    }
    assert_eq!(
        Overlay::new(
            address(200, 0),
            original.clone(),
            original,
            vec![Some(0), None]
        ),
        Err(Error::Unchanged)
    );
    let mut altered = call();
    if let TeletextEnhancementCommand::InvokeObject { offset, .. } = &mut altered {
        *offset = Some(TeletextObjectOffset::new(0, 1).unwrap());
    }
    assert_eq!(
        Overlay::new(
            address(200, 0),
            program(vec![call()]),
            program(vec![altered, call()]),
            vec![Some(0), None]
        ),
        Err(Error::ProtectedCommand)
    );
}
#[test]
fn main_call_structure_protects_resource_calls_and_foreign_local_owners() {
    for protected in [
        invoke(public(0xa0, 0, 1), TeletextObjectType::Passive),
        invoke(
            TeletextObjectIdentity::local(address(200, 1), TeletextObjectId::new(65520)),
            TeletextObjectType::Passive,
        ),
    ] {
        assert_eq!(
            Overlay::new(
                address(200, 0),
                program(vec![]),
                program(vec![protected]),
                vec![None]
            ),
            Err(Error::ProtectedCommand)
        );
        assert_eq!(
            Overlay::new(
                address(200, 0),
                program(vec![protected]),
                program(vec![]),
                vec![]
            ),
            Err(Error::ProtectedCommand)
        );
        assert!(Overlay::new(
            address(200, 0),
            program(vec![protected]),
            program(vec![protected, call()]),
            vec![Some(0), None]
        )
        .is_ok());
    }
}
#[test]
fn main_call_structure_rejects_missing_targets_wrong_types_stale_or_absent_programs() {
    let original = TeletextSemanticService::new(input(), budget()).unwrap();
    let add = |command| intent(vec![call()], vec![call(), command], vec![Some(0), None]);
    for command in [
        invoke(
            TeletextObjectIdentity::local(address(200, 0), TeletextObjectId::new(65519)),
            TeletextObjectType::Passive,
        ),
        invoke(local(), TeletextObjectType::Active),
    ] {
        assert!(matches!(
            original
                .clone()
                .with_main_local_invocation_program_overlays(&[add(command)], budget()),
            Err(TeletextServiceError::MainLocalInvocationProgramOverlay {
                reason: TeletextMainLocalInvocationProgramFailure::Target,
                ..
            })
        ));
    }
    let stale = intent(vec![], vec![call()], vec![None]);
    assert!(original
        .clone()
        .with_main_local_invocation_program_overlays(&[stale], budget())
        .is_err());
    let good = add(call());
    assert!(original
        .clone()
        .with_main_local_invocation_program_overlays(&[good.clone(), good], budget())
        .is_err());
    let mut absent = input();
    absent.pages[0].local_program = None;
    absent
        .objects
        .retain(|definition| definition.identity != local());
    let absent = TeletextSemanticService::new(absent, budget()).unwrap();
    assert!(absent
        .with_main_local_invocation_program_overlays(
            &[intent(vec![], vec![call()], vec![None])],
            budget()
        )
        .is_err());
}
#[test]
fn main_call_structure_capacity_and_raw_result_budgets_remain_closed() {
    let before = vec![call(); 207];
    let mut after = before.clone();
    after.push(call());
    let mut retained = (0..207).map(Some).collect::<Vec<_>>();
    retained.push(None);
    assert!(Overlay::new(
        address(200, 0),
        program(before.clone()),
        program(after.clone()),
        retained.clone()
    )
    .is_ok());
    after.push(call());
    retained.push(None);
    assert_eq!(
        Overlay::new(address(200, 0), program(before), program(after), retained),
        Err(Error::Capacity)
    );
    let original = TeletextSemanticService::new(input(), budget()).unwrap();
    for which in 0..3 {
        let mut small = budget();
        match which {
            0 => small.max_records = 0,
            1 => small.max_commands = 2,
            _ => small.max_structure_items = 1,
        }
        assert!(original
            .clone()
            .with_main_local_invocation_program_overlays(
                &[intent(
                    vec![call()],
                    vec![call(), call()],
                    vec![Some(0), None]
                )],
                small
            )
            .is_err());
    }
}
