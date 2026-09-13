//! Validate portable sparse options without resolving defaults or executable authority.

use ersatzrs_addon_contract::{AddonCapability, AddonManifest, AddonMediaListOptionValueScope};

use crate::addon_media_list_options::{wire_values, ResolvedAddonItemOptions};
use crate::addon_media_list_saved::model::SavedOptions;

use super::SavedContentError;

pub(super) fn validate_options(
    saved: &SavedOptions,
    addon_id: &str,
    installed: &AddonManifest,
) -> Result<(), SavedContentError> {
    let Some(schema) = &saved.schema else {
        return if saved.list_values.is_empty()
            && saved
                .item_values
                .values()
                .all(std::collections::BTreeMap::is_empty)
        {
            Ok(())
        } else {
            Err(SavedContentError::Options)
        };
    };
    let definition: AddonCapability = serde_json::from_value(schema.definition.clone())
        .map_err(|_| SavedContentError::Options)?;
    if schema.identity.addon_id != addon_id
        || schema.identity.capability_id != definition.id
        || installed.media_list_option_schema() != Some(&definition)
    {
        return Err(SavedContentError::Options);
    }
    let list_values = wire_values(&schema.identity.capability_id, &saved.list_values);
    definition
        .validate_stored_option_values(&list_values, AddonMediaListOptionValueScope::ListDefault)
        .map_err(|_| SavedContentError::Options)?;
    for values in saved.item_values.values() {
        definition
            .validate_stored_option_values(
                &wire_values(&schema.identity.capability_id, values),
                AddonMediaListOptionValueScope::ItemOverride,
            )
            .map_err(|_| SavedContentError::Options)?;
    }

    // The saved Value may reorder object keys. The runtime compares the digest
    // of the installed typed snapshot, never a reserialized generic JSON Value.
    ResolvedAddonItemOptions {
        schema: schema.identity.clone(),
        definition,
        values: list_values,
    }
    .validate_installed_schema(installed)
    .map_err(|_| SavedContentError::Options)
}

#[cfg(test)]
mod tests;
