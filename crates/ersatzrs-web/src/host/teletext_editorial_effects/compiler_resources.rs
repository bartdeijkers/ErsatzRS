//! Exact compiler supplements; these private descriptors grant no editor rights.
use super::{
    check_deadline, display, error, Result, TeletextEditorialEffectsError,
    TeletextEditorialEffectsFailure,
};
use ersatzrs_domain::{TeletextPageAddress, TeletextPhysicalBaseOverlay};
use ersatzrs_domain::{TeletextResourceIdentity, TeletextResourceTransmission};
use ersatzrs_ffmpeg::teletext::source_header::{
    observe_dvb_teletext_header, TeletextHeaderObservation,
};
use ersatzrs_ffmpeg::teletext::{
    encode_teletext_compositional_links, TeletextCompilerDrcsResource,
    TeletextResourceTransmissionMode,
};
use ersatzrs_ffmpeg::teletext::{
    encode_teletext_drcs_page, project_encoded_teletext_display, EncodedTeletextAuthoredPage,
    EncodedTeletextAuthoredService, TeletextEncodedDisplayRecord, TeletextResourceFullUpdate,
};
use std::time::Instant;

fn failure() -> TeletextEditorialEffectsError {
    error(
        None,
        None,
        TeletextEditorialEffectsFailure::ProtectedStructure,
    )
}

pub(super) fn compiler_owned(
    service: &EncodedTeletextAuthoredService,
    page: &EncodedTeletextAuthoredPage,
) -> bool {
    page.resource_header()
        .is_some_and(|header| descriptor(service, header.identity()).is_some())
}

fn descriptor(
    service: &EncodedTeletextAuthoredService,
    identity: TeletextResourceIdentity,
) -> Option<&TeletextCompilerDrcsResource> {
    service
        .compiler_drcs_resources()
        .iter()
        .find(|resource| resource.table().identity() == identity)
}

pub(super) fn authored_pages(
    service: &EncodedTeletextAuthoredService,
) -> impl Iterator<Item = &EncodedTeletextAuthoredPage> {
    service
        .pages()
        .iter()
        .filter(move |page| !compiler_owned(service, page))
}

fn generated_page<'a>(
    service: &'a EncodedTeletextAuthoredService,
    resource: &TeletextCompilerDrcsResource,
) -> Result<&'a EncodedTeletextAuthoredPage> {
    let mut pages = service.pages().iter().filter(|page| {
        page.resource_header()
            .is_some_and(|header| header.identity() == resource.table().identity())
    });
    let page = pages.next().ok_or_else(failure)?;
    if pages.next().is_some() {
        return Err(failure());
    }
    Ok(page)
}

fn check_descriptor(
    service: &EncodedTeletextAuthoredService,
    resource: &TeletextCompilerDrcsResource,
    deadline: Instant,
) -> Result<()> {
    check_deadline(deadline)?;
    let identity = resource.table().identity();
    if service.declared().resource_inventory().contains(&identity)
        || resource.consumers().is_empty()
    {
        return Err(failure());
    }
    for (index, owner) in resource.consumers().iter().enumerate() {
        if resource.consumers()[..index].contains(owner)
            || !service.pages().iter().any(|page| {
                display(page).is_some_and(|(_, composition)| composition.owner() == *owner)
            })
        {
            return Err(failure());
        }
        check_links(service, *owner)?;
    }
    let EncodedTeletextAuthoredPage::Resource(page) = generated_page(service, resource)? else {
        return Err(failure());
    };
    let header = page.header();
    if page.transmission_mode() != TeletextResourceTransmissionMode::Serial
        || header.update_counter().get() != 0
        || header.transmission() != TeletextResourceTransmission::FullUnchangedFirst
    {
        return Err(failure());
    }
    let expected = encode_teletext_drcs_page(
        resource.table(),
        header.update_counter(),
        TeletextResourceFullUpdate::Unchanged,
        page.transmission_mode(),
    )
    .map_err(|_| failure())?;
    if expected.header() != header || expected.data_units() != page.data_units() {
        return Err(failure());
    }
    check_deadline(deadline)
}

fn changed_generated(
    original: &EncodedTeletextAuthoredService,
    edited: &EncodedTeletextAuthoredService,
    resource: &TeletextCompilerDrcsResource,
    rows: &[TeletextPhysicalBaseOverlay],
) -> Result<()> {
    let identity = resource.table().identity();
    let old = descriptor(original, identity);
    let new = descriptor(edited, identity);
    let changed = match (old, new) {
        (Some(old), Some(new)) => {
            let before = generated_page(original, old)?;
            let after = generated_page(edited, new)?;
            if before.resource_header() != after.resource_header() {
                return Err(failure());
            }
            old.table() != new.table()
                || old.consumers() != new.consumers()
                || before.data_units() != after.data_units()
        }
        _ => true,
    };
    if changed
        && !old.into_iter().chain(new).any(|item| {
            item.consumers()
                .iter()
                .any(|owner| rows.iter().any(|row| row.owner() == *owner))
        })
    {
        return Err(failure());
    }
    Ok(())
}

pub(super) fn check(
    original: &EncodedTeletextAuthoredService,
    edited: &EncodedTeletextAuthoredService,
    rows: &[TeletextPhysicalBaseOverlay],
    additions: &[ersatzrs_app::teletext_design_documents::TeletextSavedObjectResourceAddition],
    deadline: Instant,
) -> Result<()> {
    for service in [original, edited] {
        if !service.inventory().iter().copied().eq(service
            .pages()
            .iter()
            .map(EncodedTeletextAuthoredPage::inventory_record))
        {
            return Err(failure());
        }
        for (index, resource) in service.compiler_drcs_resources().iter().enumerate() {
            if service.compiler_drcs_resources()[..index]
                .iter()
                .any(|previous| previous.table().identity() == resource.table().identity())
            {
                return Err(failure());
            }
            check_descriptor(service, resource, deadline)?;
            changed_generated(original, edited, resource, rows)?;
        }
    }
    check_authored_inventory(original, edited, additions)
}

fn check_authored_inventory(
    original: &EncodedTeletextAuthoredService,
    edited: &EncodedTeletextAuthoredService,
    additions: &[ersatzrs_app::teletext_design_documents::TeletextSavedObjectResourceAddition],
) -> Result<()> {
    let mut before = authored_pages(original);
    let mut after = authored_pages(edited)
        .filter(|page| !super::object_resources::is_added_page(page, additions));
    loop {
        let equal = match (before.next(), after.next()) {
            (None, None) => return Ok(()),
            (
                Some(EncodedTeletextAuthoredPage::Object(old)),
                Some(EncodedTeletextAuthoredPage::Object(new)),
            ) => {
                // Object extents are derived from the edited commands. The
                // resource check authenticates their full canonical headers.
                old.header().identity() == new.header().identity()
            }
            (Some(old), Some(new)) => old.inventory_record() == new.inventory_record(),
            _ => false,
        };
        if !equal {
            return Err(failure());
        }
    }
}

fn links(
    service: &EncodedTeletextAuthoredService,
    owner: TeletextPageAddress,
) -> Result<Option<&ersatzrs_domain::TeletextX27Level25Links>> {
    let mut expected = None;
    for resource in service.compiler_drcs_resources() {
        if resource.consumers().contains(&owner) {
            let links = resource.compositional_links(owner).ok_or_else(failure)?;
            if expected.is_some_and(|old| old != links) {
                return Err(failure());
            }
            expected = Some(links);
        }
    }
    Ok(expected.or_else(|| {
        service
            .declared()
            .semantic()
            .page_enhancements()
            .get(&owner)
            .and_then(|page| page.compositional_links.as_ref())
    }))
}

fn check_links(service: &EncodedTeletextAuthoredService, owner: TeletextPageAddress) -> Result<()> {
    let expected =
        links(service, owner)?.map(|links| encode_teletext_compositional_links(owner, links));
    let page = service
        .pages()
        .iter()
        .find(|page| display(page).is_some_and(|(_, composition)| composition.owner() == owner))
        .ok_or_else(failure)?;
    let projection = project_encoded_teletext_display(
        display(page).ok_or_else(failure)?.0,
        service.unit_count(),
    )
    .map_err(|_| failure())?;
    let count = projection
        .records
        .iter()
        .filter(|record| {
            matches!(
                record,
                TeletextEncodedDisplayRecord::Triplets {
                    row: 27,
                    designation: 4,
                    ..
                }
            )
        })
        .count();
    if count != usize::from(expected.is_some()) {
        return Err(failure());
    }
    let Some(expected) = expected else {
        return Ok(());
    };
    let mut count = 0;
    for unit in page.data_units() {
        if matches!(observe_dvb_teletext_header(unit),Ok(TeletextHeaderObservation::NotHeader(address)) if address.packet_number == 27)
            && unit[6].reverse_bits() == expected[0]
        {
            count += 1;
            if !unit[6..]
                .iter()
                .zip(expected)
                .all(|(actual, expected)| actual.reverse_bits() == expected)
            {
                return Err(failure());
            }
        }
    }
    if count != 1 {
        return Err(failure());
    }
    Ok(())
}

/// Both services have already independently matched their descriptor's exact
/// generated X27/4 body. Incoming uploads still match the old receipt strictly.
pub(super) fn links_changed(
    original: &EncodedTeletextAuthoredService,
    edited: &EncodedTeletextAuthoredService,
    owner: TeletextPageAddress,
) -> Result<bool> {
    if ![original, edited].into_iter().any(|service| {
        service
            .compiler_drcs_resources()
            .iter()
            .any(|resource| resource.consumers().contains(&owner))
    }) {
        return Ok(false);
    }
    check_links(original, owner)?;
    check_links(edited, owner)?;
    let old = links(original, owner)?;
    let new = links(edited, owner)?;
    Ok(old != new)
}
