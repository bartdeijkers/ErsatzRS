//! Mandatory invisible positions share the bounded glyph correspondence DP.
use super::{editable, fail, hidden_position, weight, Aligned, Location, Result};
use ersatzrs_domain::TeletextEnhancementCommand;
use std::time::Instant;

pub(super) fn solve(
    old: &[(usize, TeletextEnhancementCommand)],
    new: &[TeletextEnhancementCommand],
    styles: bool,
    location: Location,
    deadline: Instant,
) -> Result<Aligned> {
    location.check_deadline(deadline)?;
    if old.len() > 208
        || new.len() > 208
        || old
            .iter()
            .any(|(_, command)| !editable(command, styles) && hidden_position(command).is_none())
        || new.iter().any(|command| !editable(command, styles))
        || old
            .iter()
            .filter(|(_, command)| hidden_position(command).is_some())
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
                if hidden_position(before).is_some() {
                    if at_anchor(before, new, j) {
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

fn at_anchor(
    anchor: &TeletextEnhancementCommand,
    new: &[TeletextEnhancementCommand],
    boundary: usize,
) -> bool {
    let Some(column) = hidden_position(anchor) else {
        return false;
    };
    if boundary > new.len() {
        return false;
    }
    let previous = boundary.checked_sub(1).and_then(|index| new.get(index));
    let next = new.get(boundary);
    // The original anchor changes no field and cannot be dropped or moved
    // past a glyph that would make its retained AP transition go backwards.
    previous.is_none_or(|command| {
        matches!(command,
        TeletextEnhancementCommand::AtColumn { column: previous, .. } if *previous <= column)
    }) && next.is_none_or(|command| {
        matches!(command,
            TeletextEnhancementCommand::AtColumn { column: next, .. } if column <= *next)
    })
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
        let hidden = i != 0 && hidden_position(&old[i - 1].1).is_some();
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
            // therefore places that position at its earliest ordered boundary.
            result.commands.push(new[j - 1]);
            result.retained.push(None);
            j -= 1;
        } else if i != 0
            && scores[(i - 1) * width + j] == Some(score)
            && (!hidden || at_anchor(&old[i - 1].1, new, j))
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
