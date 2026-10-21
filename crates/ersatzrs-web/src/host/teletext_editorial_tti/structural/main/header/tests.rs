//! Carrier hypotheses cannot create authored commands or scalar permissions.
use super::{carrier_colour, prospective, Command, Location, ProspectiveCarrier, Unit};
use crate::host::teletext_editorial_tti::structural::{main::correspondence, stream};
use ersatzrs_domain::{
    TeletextColourReference, TeletextColumnOperation, TeletextEnhancementPosition,
    TeletextEnhancementProgram, TeletextObjectId, TeletextObjectIdentity, TeletextObjectType,
    TeletextPageAddress, TeletextPageNumber, TeletextSubcode,
};
use std::time::{Duration, Instant};

fn owner() -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(124).unwrap(),
        subcode: TeletextSubcode::new(32).unwrap(),
    }
}
fn location() -> Location {
    Location {
        page: 124,
        subcode: 32,
        resource: None,
    }
}
fn position() -> TeletextEnhancementPosition {
    TeletextEnhancementPosition::new(0, 8).unwrap()
}
fn ap() -> Command {
    Command::SetPosition {
        position: position(),
    }
}
fn call() -> Command {
    Command::InvokeObject {
        target: TeletextObjectIdentity::local(owner(), TeletextObjectId::new(65520)),
        object_type: TeletextObjectType::Passive,
        offset: None,
    }
}
fn image(words: Vec<u32>) -> stream::Image {
    let end = words.len();
    stream::Image {
        words,
        markers: vec![end],
        end,
    }
}
fn unit(first: usize, call: Option<Command>) -> Unit {
    Unit {
        first,
        last: first,
        original: None,
        call,
        editable: false,
        scalar: None,
    }
}

#[test]
fn main_carrier_hypothesis_preserves_the_retained_leading_ap_and_only_local_calls() {
    let original = TeletextEnhancementProgram::new(vec![ap()]).unwrap();
    let carrier = ProspectiveCarrier {
        position: position(),
    };
    assert!(carrier
        .check_candidate(
            &original,
            &[ap(), call()],
            &[Some(0), None],
            owner(),
            location()
        )
        .is_ok());
    for (commands, retained) in [
        (vec![call()], vec![None]),
        (vec![ap(), call()], vec![None, None]),
        (vec![ap()], vec![Some(0)]),
        (vec![ap(), ap(), call()], vec![Some(0), None, None]),
    ] {
        assert!(carrier
            .check_candidate(&original, &commands, &retained, owner(), location())
            .is_err());
    }
    let other = TeletextPageAddress {
        page: TeletextPageNumber::new(125).unwrap(),
        ..owner()
    };
    assert!(carrier
        .check_candidate(
            &original,
            &[ap(), call()],
            &[Some(0), None],
            other,
            location()
        )
        .is_err());
}

#[test]
fn main_carrier_hypothesis_needs_one_known_background_word_then_a_local_call() {
    let original = TeletextEnhancementProgram::new(vec![ap()]).unwrap();
    let before = image(vec![0x3f]);
    let bg = 8 | (3 << 6);
    let after = image(vec![bg, 0x28]);
    let mut valid = vec![unit(0, None), unit(1, Some(call()))];
    assert!(prospective(
        &original,
        owner(),
        &mut valid,
        (&before, &after),
        None,
        location()
    )
    .unwrap()
    .is_some());
    assert_eq!(valid.len(), 1);
    for (words, mut units) in [
        (
            vec![bg | 1, 0x28],
            vec![unit(0, None), unit(1, Some(call()))],
        ),
        (
            vec![bg, bg, 0x28],
            vec![unit(0, None), unit(1, None), unit(2, Some(call()))],
        ),
        (vec![bg], vec![unit(0, None)]),
    ] {
        let after = image(words);
        let count = units.len();
        assert!(prospective(
            &original,
            owner(),
            &mut units,
            (&before, &after),
            None,
            location()
        )
        .unwrap()
        .is_none());
        assert_eq!(units.len(), count);
    }
    assert!(!carrier_colour(bg | (0x20 << 11), 8));
    assert!(!carrier_colour(bg | (1 << 18), 8));
}

#[test]
fn main_authored_background_remains_mandatory_and_keeps_existing_scalar_edits() {
    let before = image(vec![8 | (3 << 6)]);
    let mut authored = unit(0, None);
    authored.original = Some(1);
    authored.scalar = Some((
        8,
        TeletextColumnOperation::Background {
            colour: TeletextColourReference::new(0).unwrap(),
        },
    ));
    let units = [authored];
    let deadline = Instant::now() + Duration::from_secs(5);
    let empty = image(Vec::new());
    assert!(correspondence::align((&units, &[]), (&before, &empty), location(), deadline).is_err());
    let changed = image(vec![8 | (3 << 6) | (1 << 11)]);
    let uploaded = [unit(0, None)];
    assert!(correspondence::align(
        (&units, &uploaded),
        (&before, &changed),
        location(),
        deadline
    )
    .is_ok());
}

fn no_main_native() -> ersatzrs_ffmpeg::teletext::EncodedTeletextLocalObjectProgram {
    use ersatzrs_domain::{
        TeletextEnhancementObjectCatalog, TeletextObjectCatalogBudget,
        TeletextResolvedObjectPageLinks,
    };
    use ersatzrs_ffmpeg::teletext::{
        allocate_teletext_resource_objects, compile_teletext_local_program_with_objects,
    };
    let budget = TeletextObjectCatalogBudget {
        max_objects: 4,
        max_commands: 208,
    };
    let catalog = TeletextEnhancementObjectCatalog::new(Vec::new(), budget).unwrap();
    let allocations = allocate_teletext_resource_objects(&catalog, &[], 4).unwrap();
    compile_teletext_local_program_with_objects(
        owner(),
        &TeletextEnhancementProgram::new(Vec::new()).unwrap(),
        &catalog,
        &allocations,
        TeletextResolvedObjectPageLinks {
            public: None,
            global: None,
        },
        budget,
    )
    .unwrap()
}

#[test]
fn main_prepare_preserves_authored_background_through_real_call_growth_alignment() {
    let bg = 8 | (3 << 6);
    let style = TeletextColumnOperation::Background {
        colour: TeletextColourReference::new(0).unwrap(),
    };
    let authored = Command::AtColumn {
        column: 8,
        operation: style,
    };
    let original = TeletextEnhancementProgram::new(vec![ap(), authored, call()]).unwrap();
    let native = no_main_native();
    let before = image(vec![bg, 0x28, 0x28, 0x3f]);
    let after = image(vec![bg, 0x28, 0x28, 0x28, 0x28, 0x3f]);
    let mut old = vec![unit(0, None), unit(1, Some(call())), unit(3, None)];
    old[0].original = Some(1);
    old[0].scalar = Some((8, style));
    old[1].original = Some(2);
    old[1].last = 2;
    old[1].editable = true;
    let mut new = vec![
        unit(0, None),
        unit(1, Some(call())),
        unit(3, Some(call())),
        unit(5, None),
    ];
    new[1].last = 2;
    new[2].last = 4;
    let deadline = Instant::now() + Duration::from_secs(5);
    assert!(super::prepare(
        &original,
        &native,
        (&mut old, &mut new),
        (&before, &after),
        location(),
        deadline
    )
    .unwrap()
    .is_none());
    assert_eq!(old[0].original, Some(1));
    assert_eq!(new[0].first, 0);
    let steps =
        correspondence::align((&old, &new), (&before, &after), location(), deadline).unwrap();
    let (replacement, retained) =
        correspondence::assemble(&original, (&old, &new), &steps, location(), deadline).unwrap();
    assert_eq!(replacement, [ap(), authored, call(), call()]);
    assert_eq!(&retained[..2], &[Some(0), Some(1)]);
}

#[test]
fn main_prepare_leaves_unqualified_nonleading_writers_mandatory() {
    let original = TeletextEnhancementProgram::new(vec![ap(), ap()]).unwrap();
    let native = no_main_native();
    let before = image(vec![8, 8 | (3 << 6)]);
    let after = image(vec![8]);
    let mut old = vec![unit(0, None), unit(1, None)];
    let mut new = vec![unit(0, None)];
    let deadline = Instant::now() + Duration::from_secs(5);
    assert!(super::prepare(
        &original,
        &native,
        (&mut old, &mut new),
        (&before, &after),
        location(),
        deadline
    )
    .unwrap()
    .is_none());
    assert_eq!(old.len(), 2);
    assert!(correspondence::align((&old, &new), (&before, &after), location(), deadline).is_err());
    assert!(super::prepare(
        &original,
        &native,
        (&mut old, &mut new),
        (&before, &after),
        location(),
        Instant::now().checked_sub(Duration::from_secs(1)).unwrap()
    )
    .is_err());
}

#[test]
fn main_carrier_hypothesis_covers_the_complete_compatibility_selector_background_span() {
    let original = TeletextEnhancementProgram::new(vec![ap()]).unwrap();
    let selector = 63 | (7 << 6) | (5 << 11);
    let bg = 8 | (3 << 6);
    let before = image(vec![selector, bg, 0x28]);
    let mut valid = vec![unit(0, None), unit(1, None), unit(2, Some(call()))];
    assert!(prospective(
        &original,
        owner(),
        &mut valid,
        (&before, &before),
        Some(2),
        location()
    )
    .unwrap()
    .is_some());
    assert_eq!(valid.len(), 1);
    assert_eq!(valid[0].first, 2);
    let foreground = image(vec![selector, 8 | (7 << 11), 0x28]);
    let mut foreground_units = vec![unit(0, None), unit(1, None), unit(2, Some(call()))];
    assert!(prospective(
        &original,
        owner(),
        &mut foreground_units,
        (&foreground, &foreground),
        Some(2),
        location()
    )
    .unwrap()
    .is_some());
    assert_eq!(foreground_units.len(), 1);
    assert!(!carrier_colour(8 | (1 << 6) | (7 << 11), 8));
    for words in [
        vec![selector ^ (1 << 11), bg, 0x28],
        vec![selector, bg | (1 << 11), 0x28],
    ] {
        let changed = image(words);
        let mut new = vec![unit(0, None), unit(1, None), unit(2, Some(call()))];
        assert!(prospective(
            &original,
            owner(),
            &mut new,
            (&before, &changed),
            Some(2),
            location()
        )
        .is_err());
        assert_eq!(new.len(), 3);
    }
    for words in [
        vec![selector | (0x20 << 11), bg, 0x28],
        vec![selector ^ (1 << 6), bg, 0x28],
        vec![selector, bg | 1, 0x28],
    ] {
        let changed = image(words);
        let mut new = vec![unit(0, None), unit(1, None), unit(2, Some(call()))];
        assert!(prospective(
            &original,
            owner(),
            &mut new,
            (&before, &changed),
            None,
            location()
        )
        .unwrap()
        .is_none());
        assert_eq!(new.len(), 3);
    }
}
