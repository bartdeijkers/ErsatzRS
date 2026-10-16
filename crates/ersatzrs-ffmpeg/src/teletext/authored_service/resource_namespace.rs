//! Fresh physical-bank allocation from the complete original namespace,
//! including links that currently require no transmitted subpage.

use super::{error, Error, Failure};
use ersatzrs_domain::{
    TeletextDeclaredService, TeletextMotPublicLinkIndex, TeletextResourceIdentity,
    TeletextResourceKind,
};
use ersatzrs_domain::{
    TeletextDrcsTableScope, TeletextRawDataPageNumber, TeletextResourceSubpageIndex,
    TeletextX27LinkRole, TeletextX27LinkTarget,
};
use std::collections::BTreeSet;

pub(super) fn allocate(
    declared: &TeletextDeclaredService,
    scope: TeletextDrcsTableScope,
) -> Result<TeletextResourceIdentity, Error> {
    let reserved = reserved(declared)?;
    let page = choose(&reserved, scope).ok_or_else(|| {
        error(Failure::Budget {
            category: "compiler DRCS free resource owners",
            count: 1,
            maximum: 0,
        })
    })?;
    let kind = match scope {
        TeletextDrcsTableScope::Normal => TeletextResourceKind::Drcs,
        TeletextDrcsTableScope::Global => TeletextResourceKind::Gdrcs,
    };
    TeletextResourceIdentity::new(kind, page, TeletextResourceSubpageIndex::ZERO)
        .map_err(|_| error(Failure::MissingResourceBody))
}

fn reserved(
    declared: &TeletextDeclaredService,
) -> Result<BTreeSet<TeletextRawDataPageNumber>, Error> {
    let mut pages: BTreeSet<_> = declared
        .resource_inventory()
        .iter()
        .map(|resource| resource.page())
        .collect();
    for page in &declared.semantic().base().pages {
        pages.insert(
            TeletextRawDataPageNumber::new(
                page.address.page.magazine(),
                page.address.page.wire_page(),
            )
            .map_err(|_| error(Failure::MissingResourceBody))?,
        );
    }
    for metadata in declared.semantic().page_enhancements().values() {
        if let Some(links) = metadata.compositional_links {
            for role in TeletextX27LinkRole::ALL {
                reserve_target(&mut pages, links.link(role).target());
            }
        }
    }
    for mot in declared.mots() {
        for index in std::iter::once(None).chain(TeletextMotPublicLinkIndex::ALL.map(Some)) {
            reserve_target(&mut pages, mot.links().object_link(index).target());
            reserve_target(&mut pages, mot.links().drcs_link(index).target());
        }
    }
    Ok(pages)
}

fn reserve_target(pages: &mut BTreeSet<TeletextRawDataPageNumber>, target: TeletextX27LinkTarget) {
    if let TeletextX27LinkTarget::Page { page } = target {
        pages.insert(page);
    }
}

fn choose(
    reserved: &BTreeSet<TeletextRawDataPageNumber>,
    scope: TeletextDrcsTableScope,
) -> Option<TeletextRawDataPageNumber> {
    let normal = find_free(reserved, 0, None);
    match scope {
        TeletextDrcsTableScope::Normal => normal,
        // Reserve the same Normal preference even when only Global is demanded,
        // so consumer removal never renumbers another bank. With just one free
        // owner either single scope can use it; two scopes then fail atomically.
        TeletextDrcsTableScope::Global => find_free(reserved, 1, normal).or(normal),
    }
}

fn find_free(
    reserved: &BTreeSet<TeletextRawDataPageNumber>,
    parity: u8,
    exclude: Option<TeletextRawDataPageNumber>,
) -> Option<TeletextRawDataPageNumber> {
    // Non-BCD and parity are preferences, never artificial capacity limits.
    for non_bcd in [true, false] {
        for preferred in [true, false] {
            for magazine in 1..=8 {
                for byte in (0xa0..=0xfc).chain(0..0xa0) {
                    if (byte >> 4 > 9 || byte & 15 > 9) != non_bcd
                        || (byte % 2 == parity) != preferred
                    {
                        continue;
                    }
                    let page = TeletextRawDataPageNumber::new(magazine, byte).ok()?;
                    if Some(page) != exclude && !reserved.contains(&page) {
                        return Some(page);
                    }
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    fn page(byte: u8) -> TeletextRawDataPageNumber {
        TeletextRawDataPageNumber::new(1, byte).unwrap()
    }
    fn all_except(free: &[TeletextRawDataPageNumber]) -> BTreeSet<TeletextRawDataPageNumber> {
        (1..=8)
            .flat_map(|magazine| {
                (0..=0xfc).map(move |byte| TeletextRawDataPageNumber::new(magazine, byte).unwrap())
            })
            .filter(|page| !free.contains(page))
            .collect()
    }
    #[test]
    fn exhausted_preference_uses_the_remaining_legal_namespace() {
        for free in [[page(0xab), page(0xcd)], [page(0x20), page(0x21)]] {
            let reserved = all_except(&free);
            assert_eq!(
                choose(&reserved, TeletextDrcsTableScope::Normal),
                Some(free[0])
            );
            assert_eq!(
                choose(&reserved, TeletextDrcsTableScope::Global),
                Some(free[1])
            );
        }
    }
    #[test]
    fn last_free_owner_is_available_to_either_single_scope() {
        let free = page(0xab);
        let reserved = all_except(&[free]);
        for scope in [
            TeletextDrcsTableScope::Normal,
            TeletextDrcsTableScope::Global,
        ] {
            assert_eq!(choose(&reserved, scope), Some(free));
            assert_eq!(choose(&all_except(&[]), scope), None);
        }
    }
}
