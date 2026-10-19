//! Bounded exact-first alignment keeps noncalls and generated units mandatory.
use super::{fail, stream, Command, Location, Result, TeletextEnhancementProgram, Unit};
use std::time::Instant;

pub(super) enum Step {
    Retained(usize),
    Omitted(usize),
    Inserted(usize),
}

fn matches(
    units: (&Unit, &Unit),
    images: (&stream::Image, &stream::Image),
    location: Location,
) -> Result<bool> {
    let (old, new) = units;
    if old.editable {
        return Ok(old.call == new.call);
    }
    let a = &images.0.words[old.first..=old.last];
    let b = &images.1.words[new.first..=new.last];
    if a.len() != b.len() {
        return Ok(false);
    }
    for (a, b) in a.iter().zip(b) {
        if stream::relocate(*a, images.0, images.1, location)? != *b {
            return Ok(false);
        }
    }
    Ok(true)
}

pub(super) fn align(
    units: (&[Unit], &[Unit]),
    images: (&stream::Image, &stream::Image),
    location: Location,
    deadline: Instant,
) -> Result<Vec<Step>> {
    let (old, new) = units;
    location.check_deadline(deadline)?;
    if old.len() > 208 || new.len() > 208 {
        return Err(fail(location));
    }
    let width = new.len() + 1;
    let cells = (old.len() + 1)
        .checked_mul(width)
        .ok_or_else(|| fail(location))?;
    let mut scores = Vec::<Option<usize>>::new();
    scores
        .try_reserve_exact(cells)
        .map_err(|_| fail(location))?;
    scores.resize(cells, None);
    scores[0] = Some(0);
    for i in 0..=old.len() {
        location.check_deadline(deadline)?;
        for j in 0..=new.len() {
            if i == 0 && j == 0 {
                continue;
            }
            let mut score = if j != 0 && new[j - 1].call.is_some() {
                scores[i * width + j - 1]
            } else {
                None
            };
            if i != 0 && old[i - 1].editable {
                score = score.max(scores[(i - 1) * width + j]);
            }
            if i != 0 && j != 0 && matches((&old[i - 1], &new[j - 1]), images, location)? {
                let weight = if old[i - 1].editable { 209 } else { 1 };
                score = score.max(
                    scores[(i - 1) * width + j - 1].and_then(|score| score.checked_add(weight)),
                );
            }
            scores[i * width + j] = score;
        }
    }
    let (mut i, mut j) = (old.len(), new.len());
    let mut result = Vec::new();
    while i != 0 || j != 0 {
        location.check_deadline(deadline)?;
        let score = scores[i * width + j].ok_or_else(|| fail(location))?;
        let weight = i
            .checked_sub(1)
            .map_or(1, |i| if old[i].editable { 209 } else { 1 });
        if i != 0
            && j != 0
            && matches((&old[i - 1], &new[j - 1]), images, location)?
            && scores[(i - 1) * width + j - 1].and_then(|value| value.checked_add(weight))
                == Some(score)
        {
            result.push(Step::Retained(i - 1));
            i -= 1;
            j -= 1;
        } else if j != 0 && new[j - 1].call.is_some() && scores[i * width + j - 1] == Some(score) {
            result.push(Step::Inserted(j - 1));
            j -= 1;
        } else if i != 0 && old[i - 1].editable && scores[(i - 1) * width + j] == Some(score) {
            result.push(Step::Omitted(i - 1));
            i -= 1;
        } else {
            return Err(fail(location));
        }
    }
    result.reverse();
    Ok(result)
}

pub(super) fn assemble(
    original: &TeletextEnhancementProgram,
    units: (&[Unit], &[Unit]),
    steps: &[Step],
    location: Location,
    deadline: Instant,
) -> Result<(Vec<Command>, Vec<Option<usize>>)> {
    let (old, new) = units;
    let mut output = (Vec::new(), Vec::new());
    let mut next = 0;
    let mut physical = 0;
    for step in steps {
        location.check_deadline(deadline)?;
        match *step {
            Step::Inserted(index) => {
                let bound = old[physical..]
                    .iter()
                    .find_map(|unit| unit.original)
                    .unwrap_or(original.commands().len());
                flush(original, &mut next, bound, &mut output, location)?;
                output
                    .0
                    .push(new[index].call.ok_or_else(|| fail(location))?);
                output.1.push(None);
            }
            Step::Retained(index) | Step::Omitted(index) => {
                if let Some(ordinal) = old[index].original.filter(|ordinal| *ordinal >= next) {
                    flush(original, &mut next, ordinal, &mut output, location)?;
                    if matches!(step, Step::Retained(_)) {
                        output.0.push(original.commands()[ordinal]);
                        output.1.push(Some(ordinal));
                    }
                    next = ordinal + 1;
                }
                physical = index + 1;
            }
        }
        if output.0.len() > 208 {
            return Err(fail(location));
        }
    }
    flush(
        original,
        &mut next,
        original.commands().len(),
        &mut output,
        location,
    )?;
    Ok(output)
}

fn flush(
    original: &TeletextEnhancementProgram,
    next: &mut usize,
    bound: usize,
    output: &mut (Vec<Command>, Vec<Option<usize>>),
    location: Location,
) -> Result<()> {
    if bound < *next || bound > original.commands().len() {
        return Err(fail(location));
    }
    for ordinal in *next..bound {
        if !matches!(original.commands()[ordinal], Command::SetPosition { position } if position.row() == 0)
            || output.0.len() >= 208
        {
            return Err(fail(location));
        }
        output.0.push(original.commands()[ordinal]);
        output.1.push(Some(ordinal));
    }
    *next = bound;
    Ok(())
}
