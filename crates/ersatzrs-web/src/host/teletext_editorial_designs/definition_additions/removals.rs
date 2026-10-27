//! Explicit added-copy deletion; ordinary TTI import retains its complete catalogue.
use super::{
    apply, check_deadline, compile_cumulative, fmt, resource_asset, Action, ApplicationServices,
    Baselines, EncodedStoredDesign, EntityId, Instant, NativeTeletextEditorialImportError,
    NativeTeletextExportPolicy, Request, Result, SavedEditorialTeletextDesign,
    TeletextDesignMutation, TeletextObjectIdentity,
};
use ersatzrs_app::teletext_design_documents::TeletextSavedObjectResourceAddition;
use ersatzrs_domain::TeletextObjectResourceAddition;

enum AddedDefinition {
    Standalone(usize),
    RetainedBank { bank: usize, definition: usize },
}

/// Delete one current, unused added definition, never an immutable original.
pub struct NativeTeletextResourceDefinitionRemoveRequest {
    pub name: String,
    pub receipt_id: EntityId,
    pub receipt_digest: String,
    pub expected_design_head: i64,
    pub identity: TeletextObjectIdentity,
}
impl fmt::Debug for NativeTeletextResourceDefinitionRemoveRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeTeletextResourceDefinitionRemoveRequest")
            .field("receipt_id", &self.receipt_id)
            .field("expected_design_head", &self.expected_design_head)
            .field("identity", &self.identity)
            .finish_non_exhaustive()
    }
}

/// Save an explicit unused-copy removal through the existing receipt/worker/head CAS.
///
/// The immutable original catalogue, complete declared dependency graph and
/// exact latest-minus-one service are checked before concept storage. No
/// installed package, active selection or published generation is changed.
/// # Errors
/// Rejects original, Local, absent, referenced or unauthorised targets, stale
/// receipts/heads, limits, native model changes and expired deadlines atomically.
pub async fn remove_unused_teletext_editorial_resource_definition(
    application: &ApplicationServices,
    request: NativeTeletextResourceDefinitionRemoveRequest,
    policy: NativeTeletextExportPolicy,
    deadline: Instant,
) -> Result<TeletextDesignMutation<SavedEditorialTeletextDesign>> {
    apply(
        application,
        Request {
            name: request.name,
            receipt_id: request.receipt_id,
            receipt_digest: request.receipt_digest,
            expected_design_head: request.expected_design_head,
            action: Action::Remove(request.identity),
        },
        policy,
        deadline,
    )
    .await
}

pub(super) fn encode(
    mut baselines: Baselines,
    identity: TeletextObjectIdentity,
    revision: i64,
    policy: &NativeTeletextExportPolicy,
    deadline: Instant,
) -> Result<(EncodedStoredDesign, String)> {
    check_deadline(deadline)?;
    let fail = || NativeTeletextEditorialImportError::Association;
    let latest = baselines.latest.as_ref().ok_or_else(fail)?;
    let source = latest.original.view().encoded().ok_or_else(fail)?;
    let original = baselines.root.original.view().encoded().ok_or_else(fail)?;
    let resource = identity.resource().ok_or_else(fail)?;
    if original
        .declared()
        .semantic()
        .objects()
        .definition(identity)
        .is_some()
        || !resource_asset(&latest.body.project, resource)
    {
        return Err(fail());
    }
    let added = baselines
        .cumulative
        .resource_additions
        .iter()
        .position(|added| added.identity() == identity)
        .map(AddedDefinition::Standalone)
        .or_else(|| {
            baselines
                .cumulative
                .object_resource_additions
                .iter()
                .enumerate()
                .find_map(|(bank, value)| {
                    value
                        .addition()
                        .definitions()
                        .iter()
                        .position(|definition| definition.identity == identity)
                        .map(|definition| AddedDefinition::RetainedBank { bank, definition })
                })
        })
        .ok_or_else(fail)?;
    let definition = match added {
        AddedDefinition::Standalone(index) => {
            baselines.cumulative.resource_additions[index].definition()
        }
        AddedDefinition::RetainedBank { bank, definition } => {
            &baselines.cumulative.object_resource_additions[bank]
                .addition()
                .definitions()[definition]
        }
    };
    if source.declared().semantic().objects().definition(identity) != Some(definition) {
        return Err(fail());
    }
    let expected = source
        .declared()
        .clone()
        .without_unused_resource_object_definitions(&[identity], policy.document.validation.service)
        .map_err(|_| fail())?;
    check_deadline(deadline)?;
    // Body changes to added definitions are already folded into this exact
    // cumulative definition. Do not create uploaded deletion authority or drop
    // any sibling edit, bank, selection, original document or default lineage.
    match added {
        AddedDefinition::Standalone(index) => {
            baselines.cumulative.resource_additions.remove(index);
        }
        AddedDefinition::RetainedBank { bank, definition } => {
            let previous = &baselines.cumulative.object_resource_additions[bank];
            let mut definitions = previous.addition().definitions().to_vec();
            definitions.remove(definition);
            let addition = TeletextObjectResourceAddition::new(
                previous.addition().resource(),
                definitions,
                policy.document.validation.service,
            )
            .map_err(|_| fail())?;
            baselines.cumulative.object_resource_additions[bank] =
                TeletextSavedObjectResourceAddition::new(
                    previous.document_index(),
                    addition,
                    previous.transmission(),
                )?;
        }
    }
    let (document, digest, edited) = compile_cumulative(&baselines, revision, policy, deadline)?;
    let actual = edited.view().encoded().ok_or_else(fail)?;
    if actual.declared() != &expected {
        return Err(fail());
    }
    check_deadline(deadline)?;
    Ok((document, digest))
}
