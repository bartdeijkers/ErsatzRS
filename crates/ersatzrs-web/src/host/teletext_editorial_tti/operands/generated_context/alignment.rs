//! Exact scalar-signature interning; ambiguous authored assignments stay rejected.
use super::{located_word, model, operand, Atom, Location, Result, CAPACITY};
use std::collections::BTreeMap;
use std::time::Instant;

#[derive(Clone, Copy)]
struct State {
    signature: usize,
    ambiguous: bool,
    context_changed: bool,
}

struct Signatures {
    nodes: Vec<(usize, u32)>,
    interned: BTreeMap<(usize, u32), usize>,
    maximum: usize,
}

impl Signatures {
    fn append(&mut self, previous: usize, word: u32, location: Location) -> Result<usize> {
        if let Some(found) = self.interned.get(&(previous, word)) {
            return Ok(*found);
        }
        if self.nodes.len() >= self.maximum {
            return Err(located_word(location, 0));
        }
        self.nodes.push((previous, word));
        let id = self.nodes.len();
        self.interned.insert((previous, word), id);
        Ok(id)
    }

    fn words(
        &self,
        mut signature: usize,
        location: Location,
        deadline: Instant,
    ) -> Result<Vec<u32>> {
        let mut result = Vec::new();
        while signature != 0 {
            location.check_deadline(deadline)?;
            let (previous, word) = *self
                .nodes
                .get(signature - 1)
                .ok_or_else(|| located_word(location, 0))?;
            if previous >= signature || result.len() >= CAPACITY {
                return Err(located_word(location, 0));
            }
            result.push(word);
            signature = previous;
        }
        result.reverse();
        Ok(result)
    }
}

pub(super) fn assign(
    atoms: &[Atom<'_>],
    uploaded: &[u32],
    location: Location,
    deadline: Instant,
) -> Result<Vec<u32>> {
    location.check_deadline(deadline)?;
    if atoms.len() > CAPACITY || uploaded.len() > CAPACITY {
        return Err(located_word(location, 0));
    }
    let width = uploaded.len() + 1;
    let count = (atoms.len() + 1)
        .checked_mul(width)
        .filter(|count| *count <= (CAPACITY + 1) * (CAPACITY + 1))
        .ok_or_else(|| located_word(location, 0))?;
    let mut states = Vec::new();
    states
        .try_reserve_exact(count)
        .map_err(|_| located_word(location, 0))?;
    states.resize(count, None);
    states[0] = Some(State {
        signature: 0,
        ambiguous: false,
        context_changed: false,
    });
    let mut signatures = Signatures {
        nodes: Vec::new(),
        interned: BTreeMap::new(),
        maximum: count,
    };
    for (index, atom) in atoms.iter().enumerate() {
        for cursor in 0..width {
            location.check_deadline(deadline)?;
            let Some(mut state) = states[index * width + cursor] else {
                continue;
            };
            let target = &mut states[(index + 1) * width..(index + 2) * width];
            match atom {
                Atom::Fixed(words) => {
                    if uploaded.get(cursor..cursor + words.len()) == Some(*words) {
                        merge(&mut target[cursor + words.len()], state);
                    }
                }
                Atom::Scalar {
                    column,
                    expected,
                    before,
                    ..
                } => {
                    if let Some(after) = uploaded
                        .get(cursor)
                        .filter(|after| operand(*column, *expected, *before, **after).is_some())
                    {
                        state.signature = signatures.append(state.signature, *after, location)?;
                        merge(&mut target[cursor + 1], state);
                    }
                }
                Atom::Glyph {
                    words,
                    original,
                    column,
                } => {
                    glyph(target, uploaded, cursor, (words, original, *column), state);
                }
            }
        }
    }
    let final_state = states[count - 1]
        .filter(|state| !state.ambiguous && state.context_changed)
        .ok_or_else(|| located_word(location, 0))?;
    signatures.words(final_state.signature, location, deadline)
}

fn merge(target: &mut Option<State>, incoming: State) {
    if let Some(previous) = target {
        previous.ambiguous |= incoming.ambiguous || previous.signature != incoming.signature;
        // Every surviving interpretation must witness a real generated change.
        previous.context_changed &= incoming.context_changed;
    } else {
        *target = Some(incoming);
    }
}

fn glyph(
    target: &mut [Option<State>],
    uploaded: &[u32],
    cursor: usize,
    group: (&[u32], &[u32], u8),
    state: State,
) {
    let (words, original, column) = group;
    for prefix in 0..=1 {
        if prefix != 0
            && !uploaded
                .get(cursor)
                .is_some_and(|word| model::charset_word(*word, column))
        {
            continue;
        }
        let first = cursor + prefix;
        let end = first + words.len();
        if uploaded.get(first..end) != Some(words) {
            continue;
        }
        let mut changed = state;
        changed.context_changed |= uploaded.get(cursor..end) != Some(original);
        merge(&mut target[end], changed);
        if uploaded
            .get(end)
            .is_some_and(|word| model::charset_word(*word, column))
        {
            let mut changed = state;
            changed.context_changed |= uploaded.get(cursor..end + 1) != Some(original);
            merge(&mut target[end + 1], changed);
        }
    }
}
