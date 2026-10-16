//! Catalogue correspondence; all invocation permissions remain an outer gate.
use super::check;
use crate::host::teletext_editorial_effects::TeletextEditorialEffectsFailure;
use ersatzrs_domain::{
    TeletextColourReference, TeletextColumnOperation, TeletextEnhancementCommand,
    TeletextEnhancementObjectCatalog, TeletextEnhancementObjectDefinition,
    TeletextEnhancementProgram, TeletextGlyphCode, TeletextLocalObjectOperandOverlay,
    TeletextObjectCatalogBudget, TeletextObjectId, TeletextObjectIdentity, TeletextObjectType,
    TeletextPageAddress, TeletextPageNumber, TeletextSubcode,
};
use std::time::{Duration, Instant};

fn owner() -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(124).unwrap(),
        subcode: TeletextSubcode::new(32).unwrap(),
    }
}
fn identity(id: u16) -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(owner(), TeletextObjectId::new(id))
}
fn g0(code: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::G0 {
        code: TeletextGlyphCode::new(code).unwrap(),
    }
}
fn change(old: u8, new: u8) -> TeletextLocalObjectOperandOverlay {
    TeletextLocalObjectOperandOverlay::new(identity(65520), 0, 0, g0(old), g0(new)).unwrap()
}
fn deadline() -> Instant {
    Instant::now().checked_add(Duration::from_secs(30)).unwrap()
}
fn definition(code: u8) -> TeletextEnhancementObjectDefinition {
    TeletextEnhancementObjectDefinition {
        identity: identity(65520),
        object_type: TeletextObjectType::Passive,
        program: TeletextEnhancementProgram::new(vec![
            TeletextEnhancementCommand::AtColumn {
                column: 0,
                operation: g0(code),
            },
            TeletextEnhancementCommand::AtColumn {
                column: 1,
                operation: TeletextColumnOperation::Foreground {
                    colour: TeletextColourReference::new(7).unwrap(),
                },
            },
        ])
        .unwrap(),
    }
}
fn catalog(
    definitions: Vec<TeletextEnhancementObjectDefinition>,
) -> TeletextEnhancementObjectCatalog {
    TeletextEnhancementObjectCatalog::new(
        definitions,
        TeletextObjectCatalogBudget {
            max_objects: 2,
            max_commands: 4,
        },
    )
    .unwrap()
}
fn service(code: u8) -> TeletextEnhancementObjectCatalog {
    catalog(vec![definition(code)])
}

#[test]
fn latest_transition_and_separate_root_qualification_accept_repeat_and_exact_undo() {
    let root = service(b'A');
    let latest = service(b'B');
    let final_service = service(b'C');
    let first = [change(b'A', b'B')];
    let incoming = [change(b'B', b'C')];
    let cumulative = [change(b'A', b'C')];
    check(&root, &latest, &first, &first, deadline()).unwrap();
    check(&latest, &final_service, &incoming, &cumulative, deadline()).unwrap();
    check(&root, &final_service, &cumulative, &cumulative, deadline()).unwrap();
    // Undo removes the cumulative root intent, while preserving the real
    // incoming C->A transition for latest-export permission qualification.
    check(
        &final_service,
        &root,
        &[change(b'C', b'A')],
        &[],
        deadline(),
    )
    .unwrap();
    check(&root, &root, &[], &[], deadline()).unwrap();
}

#[test]
fn stale_duplicate_missing_and_wrong_cumulative_operands_reject() {
    let before = service(b'A');
    let after = service(b'B');
    let valid = change(b'A', b'B');
    let missing =
        TeletextLocalObjectOperandOverlay::new(identity(65521), 0, 0, g0(b'A'), g0(b'B')).unwrap();
    let bad_command =
        TeletextLocalObjectOperandOverlay::new(identity(65520), 2, 0, g0(b'A'), g0(b'B')).unwrap();
    for incoming in [
        vec![change(b'C', b'B')],
        vec![valid, valid],
        vec![missing],
        vec![bad_command],
    ] {
        assert_eq!(
            check(&before, &after, &incoming, &[valid], deadline())
                .unwrap_err()
                .reason,
            TeletextEditorialEffectsFailure::Association
        );
    }
    for cumulative in [vec![valid, valid], vec![change(b'A', b'C')]] {
        assert_eq!(
            check(&before, &after, &[valid], &cumulative, deadline())
                .unwrap_err()
                .reason,
            TeletextEditorialEffectsFailure::Association
        );
    }
    // A forged root expectation cannot pass the independent root-to-final gate.
    let forged = [change(b'C', b'B')];
    assert_eq!(
        check(&before, &after, &forged, &forged, deadline())
            .unwrap_err()
            .reason,
        TeletextEditorialEffectsFailure::Association
    );
}

#[test]
fn identity_type_count_order_and_unapproved_commands_stay_protected() {
    let before = service(b'A');
    let valid = [change(b'A', b'B')];
    let base = definition(b'B');
    let mut wrong_type = base.clone();
    wrong_type.object_type = TeletextObjectType::Active;
    let mut extra_command = base.clone();
    let mut commands = extra_command.program.commands().to_vec();
    commands.push(commands[0]);
    extra_command.program = TeletextEnhancementProgram::new(commands).unwrap();
    let mut unapproved = base.clone();
    let mut commands = unapproved.program.commands().to_vec();
    commands[1] = TeletextEnhancementCommand::AtColumn {
        column: 1,
        operation: TeletextColumnOperation::Foreground {
            colour: TeletextColourReference::new(1).unwrap(),
        },
    };
    unapproved.program = TeletextEnhancementProgram::new(commands).unwrap();
    for changed in [wrong_type, extra_command, unapproved] {
        assert_eq!(
            check(&before, &catalog(vec![changed]), &valid, &valid, deadline())
                .unwrap_err()
                .reason,
            TeletextEditorialEffectsFailure::ProtectedStructure
        );
    }
    let mut other = definition(b'A');
    other.identity = identity(65521);
    let original = catalog(vec![definition(b'A'), other.clone()]);
    let reordered = catalog(vec![other, base]);
    assert_eq!(
        check(&original, &reordered, &valid, &valid, deadline())
            .unwrap_err()
            .reason,
        TeletextEditorialEffectsFailure::ProtectedStructure
    );
    assert!(check(&before, &catalog(vec![]), &[], &[], deadline()).is_err());
    let mut replaced = definition(b'A');
    replaced.identity = identity(65521);
    assert_eq!(
        check(&before, &catalog(vec![replaced]), &[], &[], deadline())
            .unwrap_err()
            .reason,
        TeletextEditorialEffectsFailure::ProtectedStructure
    );
}

#[test]
fn expired_deadline_rejects_before_catalogue_changes_are_authorized() {
    assert_eq!(
        check(
            &service(b'A'),
            &service(b'B'),
            &[change(b'A', b'B')],
            &[],
            Instant::now()
        )
        .unwrap_err()
        .reason,
        TeletextEditorialEffectsFailure::TimedOut
    );
}
