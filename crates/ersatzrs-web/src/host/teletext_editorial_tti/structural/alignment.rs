//! Canonical import correspondence, not reconstruction of editor action history.
use super::{fail, glyph_family, style, Location, Result};
use ersatzrs_domain::TeletextEnhancementCommand;
use std::time::Instant;

mod kernel;

pub(super) struct Aligned {
    pub commands: Vec<TeletextEnhancementCommand>,
    pub retained: Vec<Option<usize>>,
}

pub(super) fn align(
    old: &[(usize, TeletextEnhancementCommand)],
    new: &[TeletextEnhancementCommand],
    location: Location,
    deadline: Instant,
) -> Result<Vec<Option<usize>>> {
    Ok(kernel::solve(old, new, false, location, deadline)?.retained)
}

pub(super) fn with_anchors(
    old: &[(usize, TeletextEnhancementCommand)],
    new: &[TeletextEnhancementCommand],
    location: Location,
    deadline: Instant,
) -> Result<Aligned> {
    location.check_deadline(deadline)?;
    if old.len() > 208 || new.len() > 208 {
        return Err(fail(location));
    }

    if old
        .iter()
        .all(|(_, command)| glyph_family(command).is_some())
    {
        let retained = align(old, new, location, deadline)?;
        return Ok(Aligned {
            commands: new.to_vec(),
            retained,
        });
    }
    kernel::solve(old, new, false, location, deadline)
}

pub(super) fn with_styles(
    old: &[(usize, TeletextEnhancementCommand)],
    new: &[TeletextEnhancementCommand],
    location: Location,
    deadline: Instant,
) -> Result<Aligned> {
    kernel::solve(old, new, true, location, deadline)
}

pub(super) fn bounded(
    old: &[(usize, TeletextEnhancementCommand)],
    new: &[TeletextEnhancementCommand],
    maximum: usize,
    location: Location,
    deadline: Instant,
) -> Result<Aligned> {
    if old.len() > maximum || new.len() > maximum {
        return Err(fail(location));
    }
    let anchors = old
        .iter()
        .filter_map(|(ordinal, command)| {
            hidden_position(command).map(|column| (*ordinal, *command, column))
        })
        .collect::<Vec<_>>();
    let physical = old
        .iter()
        .filter(|(_, command)| hidden_position(command).is_none())
        .copied()
        .collect::<Vec<_>>();
    let bounds = physical
        .iter()
        .map(|(ordinal, _)| {
            let index = anchors.partition_point(|(anchor, _, _)| anchor < ordinal);
            (
                index.checked_sub(1).map_or(0, |index| anchors[index].2),
                anchors.get(index).map_or(39, |anchor| anchor.2),
            )
        })
        .collect::<Vec<_>>();
    let aligned = kernel::solve_physical(&physical, new, maximum, &bounds, location, deadline)?;
    merge_anchors(&aligned, &anchors, (maximum, false), location, deadline)
}

/// Resource-only call runs use the same bounded, exact-first correspondence.
pub(super) fn calls(
    old: &[(usize, TeletextEnhancementCommand)],
    new: &[TeletextEnhancementCommand],
    maximum: usize,
    location: Location,
    deadline: Instant,
) -> Result<Aligned> {
    kernel::solve_calls(old, new, maximum, location, deadline)
}

/// Only the resource parser supplies indivisible, authenticated invocation atoms.
pub(super) fn resource_calls(
    old: &[(usize, TeletextEnhancementCommand)],
    new: &[TeletextEnhancementCommand],
    maximum: usize,
    location: Location,
    deadline: Instant,
) -> Result<Aligned> {
    location.check_deadline(deadline)?;
    if old.len() > maximum || new.len() > maximum || new.len() > 507 {
        return Err(fail(location));
    }
    let mut anchors = Vec::new();
    let mut physical = Vec::new();
    for (ordinal, command) in old {
        location.check_deadline(deadline)?;
        if let Some(column) = hidden_position(command) {
            anchors.push((*ordinal, *command, column));
        } else {
            physical.push((*ordinal, *command));
            if physical.len() > 507 {
                return Err(fail(location));
            }
        }
    }
    if anchors
        .len()
        .checked_add(new.len())
        .is_none_or(|count| count > maximum)
    {
        return Err(fail(location));
    }
    let bounds = physical
        .iter()
        .map(|(ordinal, _)| {
            let index = anchors.partition_point(|(anchor, _, _)| anchor < ordinal);
            (
                index.checked_sub(1).map_or(0, |index| anchors[index].2),
                anchors.get(index).map_or(39, |anchor| anchor.2),
            )
        })
        .collect::<Vec<_>>();
    let aligned =
        kernel::solve_resource_calls(&physical, new, maximum, &bounds, location, deadline)?;
    merge_anchors(&aligned, &anchors, (maximum, true), location, deadline)
}

fn merge_anchors(
    aligned: &Aligned,
    anchors: &[(usize, TeletextEnhancementCommand, u8)],
    policy: (usize, bool),
    location: Location,
    deadline: Instant,
) -> Result<Aligned> {
    let (maximum, calls) = policy;
    if aligned
        .commands
        .len()
        .checked_add(anchors.len())
        .is_none_or(|count| count > maximum)
    {
        return Err(fail(location));
    }
    let mut insertions = Vec::new();
    let mut previous = 0;
    for (ordinal, command, _) in anchors {
        location.check_deadline(deadline)?;
        let lower = aligned
            .retained
            .iter()
            .rposition(|source| source.is_some_and(|source| source < *ordinal))
            .map_or(0, |index| index + 1)
            .max(previous);
        let upper = aligned
            .retained
            .iter()
            .position(|source| source.is_some_and(|source| source > *ordinal))
            .unwrap_or(aligned.commands.len());
        let boundary = (lower..=upper)
            .find(|boundary| {
                if calls {
                    kernel::at_resource_anchor(command, &aligned.commands, *boundary)
                } else {
                    kernel::at_anchor(command, &aligned.commands, *boundary)
                }
            })
            .ok_or_else(|| fail(location))?;
        insertions.push((boundary, *ordinal, *command));
        previous = boundary;
    }
    let mut result = Aligned {
        commands: Vec::new(),
        retained: Vec::new(),
    };
    let mut inserted = insertions.into_iter().peekable();
    for boundary in 0..=aligned.commands.len() {
        while inserted
            .peek()
            .is_some_and(|(index, _, _)| *index == boundary)
        {
            location.check_deadline(deadline)?;
            let (_, ordinal, command) = inserted.next().ok_or_else(|| fail(location))?;
            result.commands.push(command);
            result.retained.push(Some(ordinal));
        }
        if let Some(command) = aligned.commands.get(boundary) {
            result.commands.push(*command);
            result.retained.push(aligned.retained[boundary]);
        }
    }
    Ok(result)
}

fn editable(command: &TeletextEnhancementCommand, styles: bool) -> bool {
    glyph_family(command).is_some() || (styles && style::kind(command).is_some())
}

/// A possible invisible position; only the sealed baseline's absent physical
/// span can authenticate it as an anchor in the parser.
pub(super) fn hidden_position(command: &TeletextEnhancementCommand) -> Option<u8> {
    match command {
        TeletextEnhancementCommand::SetPosition { position } if position.row() == 0 => {
            Some(position.column())
        }
        _ => None,
    }
}

fn weight(
    old: &TeletextEnhancementCommand,
    new: &TeletextEnhancementCommand,
    exact: usize,
) -> usize {
    if old == new {
        exact
    } else if matches!(
        (old, new),
        (
            TeletextEnhancementCommand::InvokeObject { .. },
            TeletextEnhancementCommand::InvokeObject { .. }
        )
    ) {
        1
    } else {
        usize::from(glyph_family(old) == glyph_family(new) && glyph_family(old).is_some())
    }
}
#[cfg(test)]
mod tests {
    use super::{align, Instant, Location};
    use ersatzrs_domain::{
        TeletextColumnOperation, TeletextEnhancementCommand, TeletextGlyphCode, TeletextMosaic,
    };
    use std::time::Duration;
    fn glyph(column: u8, code: u8) -> TeletextEnhancementCommand {
        TeletextEnhancementCommand::AtColumn {
            column,
            operation: TeletextColumnOperation::G0 {
                code: TeletextGlyphCode::new(code).unwrap(),
            },
        }
    }
    fn mosaic(bits: u8) -> TeletextEnhancementCommand {
        TeletextEnhancementCommand::AtColumn {
            column: 1,
            operation: TeletextColumnOperation::G1 {
                mosaic: TeletextMosaic::new(bits).unwrap(),
            },
        }
    }
    fn location() -> Location {
        Location {
            page: 124,
            subcode: 32,
            resource: None,
        }
    }
    #[test]
    fn duplicate_same_column_survivors_are_right_biased_not_rejected() {
        let a = glyph(0, b'A');
        let end = Instant::now() + Duration::from_secs(30);
        assert_eq!(
            align(&[(3, a), (4, a)], &[a], location(), end).unwrap(),
            vec![Some(4)]
        );
        assert_eq!(
            align(&[(3, a)], &[a, a], location(), end).unwrap(),
            vec![None, Some(3)]
        );
    }
    #[test]
    fn inserted_other_family_suffix_does_not_discard_the_rightmost_duplicate() {
        let a = glyph(0, b'A');
        let old = [(3, a), (4, a), (5, mosaic(1))];
        let new = [
            a,
            mosaic(2),
            mosaic(3),
            mosaic(4),
            mosaic(5),
            mosaic(1),
            mosaic(6),
            mosaic(7),
        ];
        assert_eq!(
            align(
                &old,
                &new,
                location(),
                Instant::now() + Duration::from_secs(30)
            )
            .unwrap(),
            vec![Some(4), None, None, None, None, Some(5), None, None],
        );
    }
    #[test]
    fn exact_survivors_dominate_family_fallback_and_deadlines_stay_bounded() {
        let old = [(3, glyph(0, b'A')), (4, glyph(1, b'B'))];
        let new = [glyph(0, b'A'), glyph(2, b'Q'), glyph(3, b'B')];
        let map = align(
            &old,
            &new,
            location(),
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
        assert_eq!(map, vec![Some(3), None, Some(4)]);
        assert!(align(&old, &new, location(), Instant::now()).is_err());
    }
}

#[cfg(test)]
#[path = "alignment/anchor_tests.rs"]
mod anchor_tests;
