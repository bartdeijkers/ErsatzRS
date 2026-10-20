//! Optional Main lifecycle preserves the complete service and genuine fallback.
use super::*;
use crate::teletext::TeletextMainProgramPresenceOverlay as Overlay;

fn original() -> TeletextSemanticService {
    let mut input = draft(&[address(200, 0), address(200, 1)]);
    input.pages[0].resources.objects.global = Some(raw(0xb0));
    input.pages[0].resources.defaults[0] =
        Some(selection(global(0xb0, 1), TeletextObjectType::Passive));
    input.objects.push(definition(
        global(0xb0, 1),
        TeletextObjectType::Passive,
        vec![],
    ));
    input.pages[1].local_program = Some(program(vec![]));
    input.objects.push(definition(
        TeletextObjectIdentity::local(address(200, 1), TeletextObjectId::new(3)),
        TeletextObjectType::Passive,
        vec![],
    ));
    TeletextSemanticService::new(input, budget()).unwrap()
}

#[test]
fn main_program_presence_keeps_none_distinct_from_empty_and_restores_defaults_exactly() {
    let source = original();
    assert!(matches!(
        source.effective_program(address(200, 0)).unwrap().0,
        TeletextEffectiveProgramSource::Defaults { .. }
    ));
    let empty = program(vec![]);
    let change = Overlay::new(address(200, 0), None, Some(empty.clone())).unwrap();
    let changed = source
        .clone()
        .with_main_program_presence_overlays(&[change], budget())
        .unwrap();
    assert_eq!(
        changed.page_enhancements()[&address(200, 0)].local_program,
        Some(empty.clone())
    );
    assert_eq!(
        changed.effective_program(address(200, 0)).unwrap().0,
        TeletextEffectiveProgramSource::Local
    );
    assert_eq!(changed.base(), source.base());
    assert_eq!(changed.objects(), source.objects());
    assert_eq!(changed.drcs(), source.drcs());
    assert_eq!(
        changed.page_enhancements()[&address(200, 1)],
        source.page_enhancements()[&address(200, 1)]
    );
    let restored = changed
        .with_main_program_presence_overlays(
            &[Overlay::new(address(200, 0), Some(empty), None).unwrap()],
            budget(),
        )
        .unwrap();
    assert_eq!(restored, source);
    assert!(Overlay::new(address(200, 0), None, None).is_err());
    assert!(Overlay::new(
        address(200, 0),
        Some(program(vec![])),
        Some(program(vec![]))
    )
    .is_err());
}

#[test]
fn main_program_presence_rejects_duplicate_stale_unknown_wrong_owner_and_missing_calls() {
    let source = original();
    let empty = Overlay::new(address(200, 0), None, Some(program(vec![]))).unwrap();
    let apply = |values: &[Overlay]| {
        source
            .clone()
            .with_main_program_presence_overlays(values, budget())
    };
    assert!(apply(&[]).is_err());
    assert!(apply(&[empty.clone(), empty.clone()]).is_err());
    assert!(apply(&[Overlay::new(address(200, 0), Some(program(vec![])), None).unwrap()]).is_err());
    assert!(apply(&[Overlay::new(address(201, 0), None, Some(program(vec![]))).unwrap()]).is_err());
    for target in [
        TeletextObjectIdentity::local(address(200, 0), TeletextObjectId::new(99)),
        TeletextObjectIdentity::local(address(200, 1), TeletextObjectId::new(3)),
    ] {
        assert!(apply(&[Overlay::new(
            address(200, 0),
            None,
            Some(program(vec![invoke(target, TeletextObjectType::Passive)]))
        )
        .unwrap()])
        .is_err());
    }
    let known = global(0xb0, 1);
    let changed = apply(&[Overlay::new(
        address(200, 0),
        None,
        Some(program(vec![invoke(known, TeletextObjectType::Passive)])),
    )
    .unwrap()])
    .unwrap();
    assert_eq!(changed.objects(), source.objects());
    let mut tiny = budget();
    tiny.max_records = 0;
    assert!(source
        .clone()
        .with_main_program_presence_overlays(std::slice::from_ref(&empty), tiny)
        .is_err());
    tiny = budget();
    tiny.max_structure_items = 0;
    assert!(source
        .clone()
        .with_main_program_presence_overlays(&[empty], tiny)
        .is_err());
    tiny = budget();
    tiny.max_commands = 0;
    let call = Overlay::new(
        address(200, 0),
        None,
        Some(program(vec![invoke(known, TeletextObjectType::Passive)])),
    )
    .unwrap();
    assert!(source
        .with_main_program_presence_overlays(&[call], tiny)
        .is_err());
}
