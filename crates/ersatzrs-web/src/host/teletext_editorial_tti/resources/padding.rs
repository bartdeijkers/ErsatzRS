//! Receipt None groups retain all-undefined modes and never define native glyphs.

use super::{compare_packets, DrcsTtiBank, Failure, Page, Result, TeletextTtiCycleUnit};

pub(super) fn validate(
    page: &Page<'_>,
    bank: DrcsTtiBank,
    index: usize,
    edited: bool,
) -> Result<()> {
    let location = bank.group_location(index);
    // Pinned cac2167 PageBase defaults have no controls; SaveTTIFormat's
    // writeSubPageStart adds PS8000, RE0 and CT20,T to this empty editor group.
    // These editor-only defaults cannot create a native resource or glyph.
    if page.status.is_some() && (!edited || page.status != Some(0x8000)) {
        return Err(location.error(Failure::Status));
    }
    if page.region.is_some_and(|region| !edited || region != 0) {
        return Err(location.error(Failure::Region));
    }
    if page
        .cycle
        .is_some_and(|cycle| !edited || cycle != (20, TeletextTtiCycleUnit::Time))
    {
        return Err(location.error(Failure::Cycle));
    }
    if page.description.is_some() || page.links.is_some() || page.rows.iter().any(Option::is_some) {
        return Err(location.error(Failure::Structure));
    }
    let prototype = bank
        .groups
        .iter()
        .flatten()
        .next()
        .ok_or_else(|| location.error(Failure::Mapping))?;
    let mut undefined = [0_u32; 13];
    undefined[0] = u32::from(prototype.identity.kind().function_code());
    undefined[1..11].fill(0x3ffff);
    undefined[11] = 0xfff;
    let mut packets = [[None; 16]; 4];
    packets[2][3] = Some(&undefined);
    compare_packets(&page.packets, &packets, location)
}
