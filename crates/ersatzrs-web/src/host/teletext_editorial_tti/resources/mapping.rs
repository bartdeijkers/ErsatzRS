//! Receipt asset order binds every editor group to its native S1, or to absence.

use super::{
    DrcsTtiBank, DrcsTtiContext, EncodedTeletextAuthoredPage, EncodedTeletextAuthoredService,
    Failure, Location, Result, TeletextResourceKind,
};
use ersatzrs_addon_contract::TeletextResourceKindWire;
use ersatzrs_app::teletext_export_project::{
    TeletextExportAssetMapping, TeletextExportAssetScope, TeletextExportProject,
};

/// Only these trusted receipt scopes enter DRCS reconciliation; malformed banks
/// fail mapping rather than falling back to an unvalidated unchanged companion.
pub(in crate::host) fn supported(companion: &TeletextExportAssetMapping) -> bool {
    matches!(
        &companion.scope,
        TeletextExportAssetScope::DrcsTti {
            editor_ordinal: 0,
            ..
        } | TeletextExportAssetScope::DrcsTtiSet { .. }
    )
}

fn invalid() -> super::super::NativeTeletextEditorialTtiError {
    Location {
        page: 0,
        subcode: 0,
        resource: None,
    }
    .error(Failure::Mapping)
}

fn resource(
    name: &str,
    project: &TeletextExportProject,
    service: &EncodedTeletextAuthoredService,
) -> Result<DrcsTtiContext> {
    let mut assets = project.assets.iter().filter(|asset| asset.name == name);
    let asset = assets.next().ok_or_else(invalid)?;
    if assets.next().is_some() {
        return Err(invalid());
    }
    let TeletextExportAssetScope::Resource {
        identity,
        physical_subcode,
    } = &asset.scope
    else {
        return Err(invalid());
    };
    let mut pages = service
        .pages()
        .iter()
        .filter_map(EncodedTeletextAuthoredPage::resource_header)
        .filter(|header| {
            let native = header.identity();
            matches!(
                (native.kind(), identity.kind),
                (TeletextResourceKind::Drcs, TeletextResourceKindWire::Drcs)
                    | (TeletextResourceKind::Gdrcs, TeletextResourceKindWire::Gdrcs)
            ) && native.page().magazine() == identity.page.magazine
                && native.page().page() == identity.page.page
                && native.logical_subpage().get() == identity.subpage
                && header.physical_subcode() == *physical_subcode
        });
    let page = pages.next().ok_or_else(invalid)?;
    if pages.next().is_some() {
        return Err(invalid());
    }
    Ok(DrcsTtiContext {
        identity: page.identity(),
        physical_subcode: *physical_subcode,
    })
}

pub(in crate::host) fn mapping(
    companion: &TeletextExportAssetMapping,
    project: &TeletextExportProject,
    service: &EncodedTeletextAuthoredService,
) -> Result<DrcsTtiBank> {
    let mut groups = [None; 16];
    let names = match &companion.scope {
        TeletextExportAssetScope::DrcsTti {
            resource_name,
            editor_ordinal: 0,
        } => {
            groups[0] = Some(resource(resource_name, project, service)?);
            return Ok(DrcsTtiBank { groups, len: 1 });
        }
        TeletextExportAssetScope::DrcsTtiSet { resource_names } => resource_names,
        _ => return Err(invalid()),
    };
    if names.is_empty() || names.len() > 16 || names.last().is_none_or(Option::is_none) {
        return Err(invalid());
    }
    let mut prototype = None;
    for (index, name) in names.iter().enumerate() {
        let Some(name) = name else {
            continue;
        };
        if names[..index]
            .iter()
            .flatten()
            .any(|previous| previous == name)
        {
            return Err(invalid());
        }
        let context = resource(name, project, service)?;
        if usize::from(context.identity.logical_subpage().get()) != index {
            return Err(invalid());
        }
        if prototype.is_some_and(|previous: DrcsTtiContext| {
            previous.identity.kind() != context.identity.kind()
                || previous.identity.page() != context.identity.page()
        }) {
            return Err(invalid());
        }
        prototype = Some(context);
        groups[index] = Some(context);
    }
    let prototype = prototype.ok_or_else(invalid)?;
    // None means the native table does not exist. A forged sparse mapping must
    // never hide an actual resource, including one outside its declared extent.
    for header in service
        .pages()
        .iter()
        .filter_map(EncodedTeletextAuthoredPage::resource_header)
    {
        let native = header.identity();
        if native.kind() == prototype.identity.kind()
            && native.page() == prototype.identity.page()
            && groups[usize::from(native.logical_subpage().get())].is_none()
        {
            return Err(invalid());
        }
    }
    Ok(DrcsTtiBank {
        groups,
        len: names.len(),
    })
}
