//! Mandatory invisible AP identities share the bounded glyph correspondence DP.
use super::{fail, glyph_family, hidden_identity, weight, Aligned, Location, Result};
use ersatzrs_domain::TeletextEnhancementCommand;
use std::time::Instant;

pub(super) fn solve(
    old: &[(usize, TeletextEnhancementCommand)],
    new: &[TeletextEnhancementCommand],
    location: Location,
    deadline: Instant,
) -> Result<Aligned> {
    location.check_deadline(deadline)?;
    if old.len() > 208
        || new.len() > 208
        || old
            .iter()
            .any(|(_, command)| glyph_family(command).is_none() && !hidden_identity(command))
        || new.iter().any(|command| glyph_family(command).is_none())
        || old
            .iter()
            .filter(|(_, command)| hidden_identity(command))
            .count()
            .checked_add(new.len())
            .is_none_or(|count| count > 208)
    {
        return Err(fail(location));
    }
    let width = new.len() + 1;
    // None is unreachable: a mandatory anchor cannot disappear through an
    // uninitialized zero-score cell or a skip-old transition.
    let mut scores = vec![None; (old.len() + 1) * width];
    scores[0] = Some(0);
    for i in 0..=old.len() {
        location.check_deadline(deadline)?;
        for j in 0..=new.len() {
            if i == 0 && j == 0 {
                continue;
            }
            let mut score = if j == 0 {
                None
            } else {
                scores[i * width + j - 1]
            };
            if i != 0 {
                let before = &old[i - 1].1;
                if hidden_identity(before) {
                    if at_zero(new, j) {
                        score = score.max(scores[(i - 1) * width + j]);
                    }
                } else {
                    score = score.max(scores[(i - 1) * width + j]);
                    if j != 0 {
                        let weight = weight(before, &new[j - 1]);
                        if weight != 0 {
                            score = score
                                .max(scores[(i - 1) * width + j - 1].map(|value| value + weight));
                        }
                    }
                }
            }
            scores[i * width + j] = score;
        }
    }
    trace(old, new, &scores, location, deadline)
}

fn at_zero(new: &[TeletextEnhancementCommand], boundary: usize) -> bool {
    boundary == 0
        || matches!(
            new[boundary - 1],
            TeletextEnhancementCommand::AtColumn { column: 0, .. }
        )
}

fn trace(
    old: &[(usize, TeletextEnhancementCommand)],
    new: &[TeletextEnhancementCommand],
    scores: &[Option<usize>],
    location: Location,
    deadline: Instant,
) -> Result<Aligned> {
    let width = new.len() + 1;
    let (mut i, mut j) = (old.len(), new.len());
    let mut result = Aligned {
        commands: Vec::new(),
        retained: Vec::new(),
    };
    while i != 0 || j != 0 {
        location.check_deadline(deadline)?;
        let score = scores[i * width + j].ok_or_else(|| fail(location))?;
        let hidden = i != 0 && hidden_identity(&old[i - 1].1);
        let matched = if i != 0 && j != 0 && !hidden {
            let weight = weight(&old[i - 1].1, &new[j - 1]);
            weight != 0
                && scores[(i - 1) * width + j - 1].map(|value| value + weight) == Some(score)
        } else {
            false
        };
        if matched {
            result.commands.push(new[j - 1]);
            result.retained.push(Some(old[i - 1].0));
            i -= 1;
            j -= 1;
        } else if j != 0 && scores[i * width + j - 1] == Some(score) {
            // Prefer insertion over a tied hidden transition. Backtracking
            // therefore places that identity at the earliest AP00 boundary.
            result.commands.push(new[j - 1]);
            result.retained.push(None);
            j -= 1;
        } else if i != 0
            && scores[(i - 1) * width + j] == Some(score)
            && (!hidden || at_zero(new, j))
        {
            if hidden {
                result.commands.push(old[i - 1].1);
                result.retained.push(Some(old[i - 1].0));
            }
            i -= 1;
        } else {
            return Err(fail(location));
        }
        if result.commands.len() > 208 {
            return Err(fail(location));
        }
    }
    result.commands.reverse();
    result.retained.reverse();
    Ok(result)
}
