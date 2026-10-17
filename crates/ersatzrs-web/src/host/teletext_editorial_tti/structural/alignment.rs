//! Canonical import correspondence, not reconstruction of editor action history.
use super::{fail, glyph_family, Location, Result};
use ersatzrs_domain::TeletextEnhancementCommand;
use std::time::Instant;

pub(super) fn align(
    old: &[(usize, TeletextEnhancementCommand)],
    new: &[TeletextEnhancementCommand],
    location: Location,
    deadline: Instant,
) -> Result<Vec<Option<usize>>> {
    if old.len() > 208 || new.len() > 208 {
        return Err(fail(location));
    }
    let width = new.len() + 1;
    let mut scores = vec![0_usize; (old.len() + 1) * width];
    for i in 1..=old.len() {
        location.check_deadline(deadline)?;
        for j in 1..=new.len() {
            let skip = scores[(i - 1) * width + j].max(scores[i * width + j - 1]);
            let weight = weight(&old[i - 1].1, &new[j - 1]);
            scores[i * width + j] = if weight == 0 {
                skip
            } else {
                skip.max(scores[(i - 1) * width + j - 1] + weight)
            };
        }
    }
    let mut result = vec![None; new.len()];
    let (mut i, mut j) = (old.len(), new.len());
    while i != 0 && j != 0 {
        location.check_deadline(deadline)?;
        let weight = weight(&old[i - 1].1, &new[j - 1]);
        if weight != 0 && scores[i * width + j] == scores[(i - 1) * width + j - 1] + weight {
            result[j - 1] = Some(old[i - 1].0);
            i -= 1;
            j -= 1;
        } else if scores[(i - 1) * width + j] > scores[i * width + j - 1] {
            i -= 1;
        } else {
            j -= 1;
        }
    }
    Ok(result)
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
