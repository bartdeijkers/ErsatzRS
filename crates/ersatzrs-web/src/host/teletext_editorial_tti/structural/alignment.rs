//! Canonical import correspondence, not reconstruction of editor action history.
use super::{fail, glyph_family, Location, Result};
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
    Ok(kernel::solve(old, new, location, deadline)?.retained)
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
    kernel::solve(old, new, location, deadline)
}

pub(super) fn hidden_identity(command: &TeletextEnhancementCommand) -> bool {
    matches!(command, TeletextEnhancementCommand::SetPosition { position }
        if position.row() == 0 && position.column() == 0)
}

fn weight(old: &TeletextEnhancementCommand, new: &TeletextEnhancementCommand) -> usize {
    if old == new {
        209
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
