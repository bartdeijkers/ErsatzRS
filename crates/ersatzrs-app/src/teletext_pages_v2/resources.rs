use super::{
    d, located, presentation, program, wire, TeletextAuthoredLocation as Location,
    TeletextAuthoredPagesError, TeletextAuthoredSection as Section,
    TeletextAuthoredValueError as Error,
};

pub(super) fn address(
    value: wire::TeletextPageAddressWire,
) -> Result<d::TeletextPageAddress, Error> {
    Ok(d::TeletextPageAddress {
        page: d::TeletextPageNumber::new(value.page)?,
        subcode: d::TeletextSubcode::new(value.subcode)?,
    })
}

fn raw_page(
    value: &wire::TeletextRawDataPageNumberWire,
) -> Result<d::TeletextRawDataPageNumber, Error> {
    Ok(d::TeletextRawDataPageNumber::new(
        value.magazine,
        value.page,
    )?)
}

pub(super) fn identity(
    value: &wire::TeletextResourceIdentityWire,
) -> Result<d::TeletextResourceIdentity, Error> {
    let kind = match value.kind {
        wire::TeletextResourceKindWire::Pop => d::TeletextResourceKind::Pop,
        wire::TeletextResourceKindWire::Gpop => d::TeletextResourceKind::Gpop,
        wire::TeletextResourceKindWire::Drcs => d::TeletextResourceKind::Drcs,
        wire::TeletextResourceKindWire::Gdrcs => d::TeletextResourceKind::Gdrcs,
        wire::TeletextResourceKindWire::Mot => d::TeletextResourceKind::Mot,
        wire::TeletextResourceKindWire::Mip => d::TeletextResourceKind::Mip,
    };
    Ok(d::TeletextResourceIdentity::new(
        kind,
        raw_page(&value.page)?,
        d::TeletextResourceSubpageIndex::new(value.subpage)?,
    )?)
}

pub(super) fn object_identity(
    value: wire::TeletextObjectIdentityWire,
) -> Result<d::TeletextObjectIdentity, Error> {
    Ok(match value {
        wire::TeletextObjectIdentityWire::Local { owner, object_id } => {
            d::TeletextObjectIdentity::local(address(owner)?, d::TeletextObjectId::new(object_id))
        }
        wire::TeletextObjectIdentityWire::Public {
            resource,
            object_id,
        } => d::TeletextObjectIdentity::public(
            identity(&resource)?,
            d::TeletextObjectId::new(object_id),
        )?,
        wire::TeletextObjectIdentityWire::Global {
            resource,
            object_id,
        } => d::TeletextObjectIdentity::global(
            identity(&resource)?,
            d::TeletextObjectId::new(object_id),
        )?,
    })
}

pub(super) fn reference(
    value: &wire::TeletextDrcsGlyphReferenceWire,
) -> Result<d::TeletextDrcsGlyphReference, Error> {
    Ok(d::TeletextDrcsGlyphReference {
        table: match value.table {
            wire::TeletextDrcsTableScopeWire::Normal => d::TeletextDrcsTableScope::Normal,
            wire::TeletextDrcsTableScopeWire::Global => d::TeletextDrcsTableScope::Global,
        },
        subtable: d::TeletextDrcsSubtableIndex::new(value.subtable)?,
        slot: d::TeletextDrcsGlyphSlot::new(value.slot)?,
    })
}

pub(super) fn drcs(
    value: wire::TeletextDrcsSubtableWire,
) -> Result<d::TeletextDrcsSubtable, Error> {
    let glyphs = value
        .glyphs
        .into_iter()
        .map(|glyph| {
            Ok(d::TeletextDrcsGlyphDefinition {
                slot: d::TeletextDrcsGlyphSlot::new(glyph.slot)?,
                bitmap: d::TeletextDrcsBitmap::new(glyph.bitmap)?,
            })
        })
        .collect::<Result<_, Error>>()?;
    Ok(d::TeletextDrcsSubtable::new(
        identity(&value.identity)?,
        glyphs,
    )?)
}

pub(super) fn selection(
    value: &wire::TeletextDisplayMotSelectionWire,
) -> Result<d::TeletextDisplayMotSelection, Error> {
    Ok(d::TeletextDisplayMotSelection {
        display: address(value.display)?,
        mot: value.mot.as_ref().map(identity).transpose()?,
    })
}

pub(super) fn page(
    value: wire::TeletextPageEnhancementWire,
    index: usize,
) -> Result<d::TeletextResolvedPageEnhancementDraft, TeletextAuthoredPagesError> {
    let location = Location::new(Section::Pages, index);
    Ok(d::TeletextResolvedPageEnhancementDraft {
        address: located(location, address(value.address))?,
        presentation: located(
            location,
            value
                .presentation
                .as_ref()
                .map(presentation::convert)
                .transpose(),
        )?,
        compositional_links: located(location, value.compositional_links.map(x27).transpose())?,
        local_program: value
            .local_program
            .map(|value| program::convert(value, location))
            .transpose()?,
        resources: located(location, resolved(value.resources))?,
    })
}

fn resolved(
    value: wire::TeletextResolvedPageResourceSelectionWire,
) -> Result<d::TeletextResolvedPageResourceSelection, Error> {
    let mut defaults = [None; 4];
    for (destination, source) in defaults.iter_mut().zip(value.defaults) {
        *destination = source
            .map(|value| -> Result<_, Error> {
                Ok(d::TeletextDefaultObjectSelection {
                    target: object_identity(value.target)?,
                    object_type: program::object_type(value.object_type),
                })
            })
            .transpose()?;
    }
    Ok(d::TeletextResolvedPageResourceSelection {
        objects: d::TeletextResolvedObjectPageLinks {
            public: value.objects.public.as_ref().map(raw_page).transpose()?,
            global: value.objects.global.as_ref().map(raw_page).transpose()?,
        },
        drcs: d::TeletextDrcsPageLinks::new(
            value.drcs.normal.as_ref().map(raw_page).transpose()?,
            value.drcs.global.as_ref().map(raw_page).transpose()?,
        )?,
        defaults,
    })
}

fn target(value: wire::TeletextX27LinkTargetWire) -> Result<d::TeletextX27LinkTarget, Error> {
    Ok(match value {
        wire::TeletextX27LinkTargetWire::Unspecified { magazine } => {
            d::TeletextX27LinkTarget::Unspecified {
                magazine: d::TeletextX27Magazine::new(magazine)?,
            }
        }
        wire::TeletextX27LinkTargetWire::Page { page } => d::TeletextX27LinkTarget::Page {
            page: raw_page(&page)?,
        },
    })
}

fn x27_link(
    value: wire::TeletextX27LinkDeclarationWire,
) -> Result<d::TeletextX27LinkDeclaration, Error> {
    let validity = match value.validity {
        wire::TeletextX27LinkValidityWire::Level25Only => d::TeletextX27LinkValidity::Level25Only,
        wire::TeletextX27LinkValidityWire::Level35Only => d::TeletextX27LinkValidity::Level35Only,
        wire::TeletextX27LinkValidityWire::Level25And35 => d::TeletextX27LinkValidity::Level25And35,
    };
    Ok(d::TeletextX27LinkDeclaration::new(
        target(value.target)?,
        validity,
        d::TeletextSubpageRequirementMask::new(value.required_subpages),
    )?)
}

fn x27(value: wire::TeletextX27Level25LinksWire) -> Result<d::TeletextX27Level25Links, Error> {
    Ok(d::TeletextX27Level25Links::new(
        x27_link(value.gpop)?,
        x27_link(value.pop)?,
        x27_link(value.gdrcs)?,
        x27_link(value.drcs)?,
    )?)
}

fn association(
    value: &wire::TeletextMotAssociationSelectionWire,
) -> Result<d::TeletextMotAssociationSelection, Error> {
    Ok(d::TeletextMotAssociationSelection::new(
        value.global,
        value
            .public
            .map(d::TeletextMotPublicLinkIndex::new)
            .transpose()?,
    ))
}

fn default(
    value: wire::TeletextMotDefaultObjectWire,
) -> Result<d::TeletextMotDefaultObject, Error> {
    Ok(match value {
        wire::TeletextMotDefaultObjectWire::NoObject { raw_number } => {
            d::TeletextMotDefaultObject::NoObject { raw_number }
        }
        wire::TeletextMotDefaultObjectWire::Dormant {
            object_type,
            raw_number,
        } => d::TeletextMotDefaultObject::Dormant {
            object_type: program::object_type(object_type),
            raw_number,
        },
        wire::TeletextMotDefaultObjectWire::Object {
            target,
            object_type,
        } => d::TeletextMotDefaultObject::Object {
            target: object_identity(target)?,
            object_type: program::object_type(object_type),
        },
    })
}

fn object_link(value: wire::TeletextMotObjectLinkWire) -> Result<d::TeletextMotObjectLink, Error> {
    let [first, second] = value.defaults;
    Ok(d::TeletextMotObjectLink::new(
        target(value.target)?,
        d::TeletextMotSubpageExtent::new(value.extent)?,
        value.supersedable,
        d::TeletextMotFallback::new(value.raw_fallback)?,
        [default(first)?, default(second)?],
    ))
}

fn drcs_link(value: wire::TeletextMotDrcsLinkWire) -> Result<d::TeletextMotDrcsLink, Error> {
    Ok(d::TeletextMotDrcsLink::new(
        target(value.target)?,
        d::TeletextMotSubpageExtent::new(value.extent)?,
        value.supersedable,
    ))
}

pub(super) fn mot(
    value: wire::TeletextMotLevel25DeclarationWire,
) -> Result<d::TeletextMotLevel25Declaration, Error> {
    let links = value.links;
    let mut public_objects = [object_link(links.global_object)?; 7];
    let global_object = public_objects[0];
    for (destination, source) in public_objects.iter_mut().zip(links.public_objects) {
        *destination = object_link(source)?;
    }
    let mut normal_drcs = [drcs_link(links.global_drcs)?; 7];
    let global_drcs = normal_drcs[0];
    for (destination, source) in normal_drcs.iter_mut().zip(links.normal_drcs) {
        *destination = drcs_link(source)?;
    }
    let links = d::TeletextMotLevel25Links::new(
        global_object,
        public_objects,
        global_drcs,
        normal_drcs,
        d::TeletextMotEnhancementCounts::new(
            links.counts.magazine_mask,
            links.counts.object_pages,
            links.counts.drcs_pages,
        ),
    )?;
    let associations = value
        .associations
        .into_iter()
        .map(|value| {
            Ok(d::TeletextMotPageAssociation::new(
                value.page,
                association(&value.objects)?,
                association(&value.drcs)?,
            ))
        })
        .collect::<Result<_, Error>>()?;
    Ok(d::TeletextMotLevel25Declaration::new(
        identity(&value.identity)?,
        associations,
        links,
    )?)
}
