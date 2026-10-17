//! Mandatory invisible positions share the bounded glyph correspondence DP.
use super::{editable, fail, hidden_position, weight, Aligned, Location, Result};
use ersatzrs_domain::TeletextEnhancementCommand;
use std::time::Instant;

#[derive(Clone, Copy)]
enum Mode {
    Glyphs,
    Styles,
    Calls,
    ResourceCalls,
}
impl Mode {
    fn editable(self, command: &TeletextEnhancementCommand) -> bool {
        match self {
            Self::Calls => matches!(command, TeletextEnhancementCommand::InvokeObject { .. }),
            Self::ResourceCalls => {
                editable(command, true)
                    || matches!(command, TeletextEnhancementCommand::InvokeObject { .. })
            }
            Self::Glyphs | Self::Styles => editable(command, matches!(self, Self::Styles)),
        }
    }
}

pub(super) fn solve_resource_calls(
    old: &[(usize, TeletextEnhancementCommand)],
    new: &[TeletextEnhancementCommand],
    maximum: usize,
    bounds: &[(u8, u8)],
    location: Location,
    deadline: Instant,
) -> Result<Aligned> {
    solve_matched(
        old,
        new,
        (Mode::ResourceCalls, maximum, bounds),
        location,
        deadline,
    )
}

pub(super) fn solve_calls(
    old: &[(usize, TeletextEnhancementCommand)],
    new: &[TeletextEnhancementCommand],
    maximum: usize,
    location: Location,
    deadline: Instant,
) -> Result<Aligned> {
    solve_matched(old, new, (Mode::Calls, maximum, &[]), location, deadline)
}

pub(super) fn solve(
    old: &[(usize, TeletextEnhancementCommand)],
    new: &[TeletextEnhancementCommand],
    styles: bool,
    location: Location,
    deadline: Instant,
) -> Result<Aligned> {
    solve_bounded(old, new, styles, 208, location, deadline)
}

pub(super) fn solve_bounded(
    old: &[(usize, TeletextEnhancementCommand)],
    new: &[TeletextEnhancementCommand],
    styles: bool,
    maximum: usize,
    location: Location,
    deadline: Instant,
) -> Result<Aligned> {
    let mode = if styles { Mode::Styles } else { Mode::Glyphs };
    solve_matched(old, new, (mode, maximum, &[]), location, deadline)
}

pub(super) fn solve_physical(
    old: &[(usize, TeletextEnhancementCommand)],
    new: &[TeletextEnhancementCommand],
    maximum: usize,
    bounds: &[(u8, u8)],
    location: Location,
    deadline: Instant,
) -> Result<Aligned> {
    solve_matched(
        old,
        new,
        (Mode::Styles, maximum, bounds),
        location,
        deadline,
    )
}

fn solve_matched(
    old: &[(usize, TeletextEnhancementCommand)],
    new: &[TeletextEnhancementCommand],
    policy: (Mode, usize, &[(u8, u8)]),
    location: Location,
    deadline: Instant,
) -> Result<Aligned> {
    let (mode, maximum, bounds) = policy;
    location.check_deadline(deadline)?;
    if (!bounds.is_empty() && bounds.len() != old.len())
        || old.len() > maximum
        || new.len() > maximum
        || old
            .iter()
            .any(|(_, command)| !mode.editable(command) && hidden_position(command).is_none())
        || new.iter().any(|command| !mode.editable(command))
        || old
            .iter()
            .filter(|(_, command)| hidden_position(command).is_some())
            .count()
            .checked_add(new.len())
            .is_none_or(|count| count > maximum)
    {
        return Err(fail(location));
    }
    let width = new.len() + 1;
    // None is unreachable: a mandatory anchor cannot disappear through an
    // uninitialized zero-score cell or a skip-old transition.
    let cells = old
        .len()
        .checked_add(1)
        .and_then(|height| height.checked_mul(width))
        .ok_or_else(|| fail(location))?;
    let mut scores = Vec::<Option<usize>>::new();
    scores
        .try_reserve_exact(cells)
        .map_err(|_| fail(location))?;
    scores.resize(cells, None);
    let exact = maximum.checked_add(1).ok_or_else(|| fail(location))?;
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
                        let weight = if permits(i - 1, &new[j - 1], bounds) {
                            weight(before, &new[j - 1], exact)
                        } else {
                            0
                        };
                        if weight != 0 {
                            score = score.max(
                                scores[(i - 1) * width + j - 1]
                                    .and_then(|value| value.checked_add(weight)),
                            );
                        }
                    }
                }
            }
            scores[i * width + j] = score;
        }
    }
    trace(old, new, &scores, (maximum, bounds), location, deadline)
}

fn permits(index: usize, new: &TeletextEnhancementCommand, bounds: &[(u8, u8)]) -> bool {
    bounds.get(index).is_none_or(|(start, end)| {
        matches!(new, TeletextEnhancementCommand::InvokeObject { .. })
            || matches!(new,
        TeletextEnhancementCommand::AtColumn { column, .. } if start <= column && column <= end)
    })
}

/// Only resource calls are authenticated atoms that leave the caller AP intact.
/// A hidden position beside one must be an identity of a known emitted AP.
pub(super) fn at_resource_anchor(
    anchor: &TeletextEnhancementCommand,
    new: &[TeletextEnhancementCommand],
    boundary: usize,
) -> bool {
    if at_anchor(anchor, new, boundary) {
        return true;
    }
    let Some(column) = hidden_position(anchor) else {
        return false;
    };
    let Some((before, after)) = new.split_at_checked(boundary) else {
        return false;
    };
    let call = |command: &TeletextEnhancementCommand| {
        matches!(command, TeletextEnhancementCommand::InvokeObject { .. })
    };
    if !before.last().is_some_and(call) && !after.first().is_some_and(call) {
        return false;
    }
    matches!(before.iter().rev().find(|command| !call(command)),
        Some(TeletextEnhancementCommand::AtColumn { column: previous, .. }) if *previous == column)
        && after
            .iter()
            .find(|command| !call(command))
            .is_none_or(|command| {
                matches!(command,
                TeletextEnhancementCommand::AtColumn { column: next, .. } if column <= *next)
            })
}

pub(super) fn at_anchor(
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
    policy: (usize, &[(u8, u8)]),
    location: Location,
    deadline: Instant,
) -> Result<Aligned> {
    let (maximum, bounds) = policy;
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
            let weight = weight(&old[i - 1].1, &new[j - 1], maximum + 1);
            permits(i - 1, &new[j - 1], bounds)
                && weight != 0
                && scores[(i - 1) * width + j - 1].and_then(|value| value.checked_add(weight))
                    == Some(score)
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
        if result.commands.len() > maximum {
            return Err(fail(location));
        }
    }
    result.commands.reverse();
    result.retained.reverse();
    Ok(result)
}
