//! Exact contiguous X26 stream, derived markers and Local invocation pointers.
use super::{fail, Location, Page, Result};
use std::time::Instant;
const FINAL: u32 = 63 | (31 << 6) | (5 << 11);
pub(super) struct Image {
    pub words: Vec<u32>,
    pub markers: Vec<usize>,
    pub end: usize,
}
impl Image {
    pub fn read(page: &Page<'_>, location: Location, deadline: Instant) -> Result<Self> {
        let mut words = Vec::new();
        let mut absent = false;
        for packet in page.packets[0] {
            location.check_deadline(deadline)?;
            match packet {
                Some(packet) if !absent => words.extend_from_slice(packet),
                None => absent = true,
                _ => return Err(fail(location)),
            }
        }
        if words.is_empty() || words.len() > 208 {
            return Err(fail(location));
        }
        let end = words
            .iter()
            .position(|word| *word == FINAL)
            .ok_or_else(|| fail(location))?
            + 1;
        if words[end..].iter().any(|word| *word != FINAL) || words.len() != end.div_ceil(13) * 13 {
            return Err(fail(location));
        }
        let markers = words[..end]
            .iter()
            .enumerate()
            .filter_map(|(i, word)| {
                let mode = (word >> 6) & 31;
                (word & 63 >= 40 && (21..=23).contains(&mode)).then_some(i)
            })
            .collect::<Vec<_>>();
        if markers.is_empty() {
            return Err(fail(location));
        }
        Ok(Self {
            words,
            markers,
            end,
        })
    }
    pub fn body(&self, definition: usize) -> &[u32] {
        let start = self.markers[definition] + 1;
        let end = self
            .markers
            .get(definition + 1)
            .copied()
            .unwrap_or(self.end - 1);
        &self.words[start..end]
    }
}
pub(super) fn relocate(word: u32, old: &Image, new: &Image, location: Location) -> Result<u32> {
    let address = word & 63;
    let mode = (word >> 6) & 31;
    if address >= 40 && (17..=19).contains(&mode) && address & 0x18 == 8 {
        let data = (word >> 11) & 127;
        let index = (((address & 1) * 8 + (data >> 4)) * 13) + (data & 15);
        let index = usize::try_from(index).map_err(|_| fail(location))?;
        let target = old
            .markers
            .iter()
            .position(|value| *value == index)
            .ok_or_else(|| fail(location))?;
        let number = new.markers[target];
        let address = (address & 0x38) | u32::from(number >= 104);
        let data =
            u32::try_from(((number / 13) & 7) * 16 + number % 13).map_err(|_| fail(location))?;
        Ok(address | (mode << 6) | (data << 11))
    } else {
        Ok(word)
    }
}
pub(super) fn marker(word: u32, index: usize, location: Location) -> Result<u32> {
    let address = (word & 0x38) | u32::from(index >= 104);
    let data = u32::try_from(((index / 13) & 7) * 16 + index % 13).map_err(|_| fail(location))?;
    Ok(address | (word & (31 << 6)) | (data << 11))
}
