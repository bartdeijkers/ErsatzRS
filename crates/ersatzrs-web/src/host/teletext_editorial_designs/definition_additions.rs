//! Explicit host additions reuse retained originals and the existing concept CAS.
use super::{
    append_overlay, check_deadline, digest, fmt, import_limits, qualify_baseline, qualify_rows,
    read_editorial_baseline, repeat, save_editorial_concept, ApplicationServices,
    EditorialBaseline, EncodedStoredDesign, EntityId, Instant, NativeTeletextEditorialImportError,
    NativeTeletextExportPolicy, QualifiedBaseline, Result, SavedEditorialTeletextDesign,
    StoredDesignParts, TeletextDesignMutation, TeletextDesignRejection, TeletextDesignRevision,
    TeletextEditorialChanges, TeletextSavedEditorialProvenance, MAX_TELETEXT_DESIGN_NAME_BYTES,
};
use ersatzrs_addon_contract::TeletextResourceKindWire;
use ersatzrs_app::teletext_export_project::{TeletextExportAssetScope, TeletextExportProject};
use ersatzrs_domain::{
    TeletextEnhancementObjectDefinition, TeletextResourceIdentity, TeletextResourceKind,
    TeletextResourceObjectDefinitionAddition, TeletextServiceBudget,
};
use ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService;

/// Explicit authoring request; uploaded TTI or sidecar data never grants this action.
pub struct NativeTeletextResourceDefinitionAddRequest {
    pub name: String,
    pub receipt_id: EntityId,
    pub receipt_digest: String,
    pub expected_design_head: i64,
    pub definitions: Vec<TeletextEnhancementObjectDefinition>,
}
impl fmt::Debug for NativeTeletextResourceDefinitionAddRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeTeletextResourceDefinitionAddRequest")
            .field("receipt_id", &self.receipt_id)
            .field("expected_design_head", &self.expected_design_head)
            .field("definitions", &self.definitions.len())
            .finish_non_exhaustive()
    }
}

fn preflight(
    request: &NativeTeletextResourceDefinitionAddRequest,
    budget: TeletextServiceBudget,
    deadline: Instant,
) -> Result<()> {
    check_deadline(deadline)?;
    if request.definitions.is_empty() {
        return Err(NativeTeletextEditorialImportError::NoChanges);
    }
    if request.name.len() > MAX_TELETEXT_DESIGN_NAME_BYTES
        || request.receipt_id <= 0
        || request.expected_design_head <= 0
        || request.receipt_digest.len() != 64
        || request.definitions.len() > budget.max_objects
    {
        return Err(NativeTeletextEditorialImportError::Budget);
    }
    let mut remaining = budget.max_commands;
    for definition in &request.definitions {
        check_deadline(deadline)?;
        remaining = remaining
            .checked_sub(definition.program.commands().len())
            .ok_or(NativeTeletextEditorialImportError::Budget)?;
    }
    Ok(())
}

fn resource_asset(project: &TeletextExportProject, resource: TeletextResourceIdentity) -> bool {
    let kind = match resource.kind() {
        TeletextResourceKind::Pop => TeletextResourceKindWire::Pop,
        TeletextResourceKind::Gpop => TeletextResourceKindWire::Gpop,
        _ => return false,
    };
    project.assets.iter().any(|asset| {
        matches!(&asset.scope,
            TeletextExportAssetScope::Resource { identity, .. }
                if identity.kind == kind
                    && identity.page.magazine == resource.page().magazine()
                    && identity.page.page == resource.page().page()
                    && identity.subpage == resource.logical_subpage().get()
        )
    })
}

/// Only retained complete native resource assets authorize adding a new ID.
pub(super) fn authorize(
    project: &TeletextExportProject,
    service: &EncodedTeletextAuthoredService,
    additions: &[TeletextResourceObjectDefinitionAddition],
    budget: TeletextServiceBudget,
    deadline: Instant,
) -> Result<()> {
    check_deadline(deadline)?;
    if additions.len() > budget.max_objects {
        return Err(NativeTeletextEditorialImportError::Budget);
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut commands = budget.max_commands;
    for addition in additions {
        check_deadline(deadline)?;
        let identity = addition.identity();
        let resource = identity
            .resource()
            .ok_or(NativeTeletextEditorialImportError::Association)?;
        commands = commands
            .checked_sub(addition.definition().program.commands().len())
            .ok_or(NativeTeletextEditorialImportError::Budget)?;
        if !seen.insert(identity)
            || service
                .declared()
                .semantic()
                .objects()
                .definition(identity)
                .is_some()
            || !service.declared().resource_inventory().contains(&resource)
            || !resource_asset(project, resource)
        {
            return Err(NativeTeletextEditorialImportError::Association);
        }
    }
    Ok(())
}

struct Baselines {
    root: QualifiedBaseline,
    latest: Option<QualifiedBaseline>,
    cumulative: TeletextEditorialChanges,
    provenance: TeletextSavedEditorialProvenance,
}
fn qualify(
    baseline: EditorialBaseline,
    root: Option<EditorialBaseline>,
    policy: &NativeTeletextExportPolicy,
    deadline: Instant,
) -> Result<Baselines> {
    let (project, tti) = import_limits(policy);
    let provenance = repeat::provenance(root.as_ref().unwrap_or(&baseline));
    let (root, latest, cumulative) = match root {
        Some(root) => {
            let root = qualify_baseline(
                root,
                policy.document,
                policy.encoding.clone(),
                project,
                tti,
                deadline,
            )?;
            let (latest, changes) = repeat::qualify_latest(baseline, &root, policy, deadline)?;
            (root, Some(latest), changes)
        }
        None => (
            qualify_baseline(
                baseline,
                policy.document,
                policy.encoding.clone(),
                project,
                tti,
                deadline,
            )?,
            None,
            TeletextEditorialChanges::default(),
        ),
    };
    Ok(Baselines {
        root,
        latest,
        cumulative,
        provenance,
    })
}

fn encode(
    mut baselines: Baselines,
    definitions: Vec<TeletextEnhancementObjectDefinition>,
    revision: i64,
    policy: &NativeTeletextExportPolicy,
    deadline: Instant,
) -> Result<(EncodedStoredDesign, String)> {
    check_deadline(deadline)?;
    let additions = definitions
        .into_iter()
        .map(|definition| {
            TeletextResourceObjectDefinitionAddition::new(
                definition,
                policy.document.validation.service,
            )
            .map_err(|error| match error {
                ersatzrs_domain::TeletextResourceObjectDefinitionAdditionError::Budget {
                    ..
                } => NativeTeletextEditorialImportError::Budget,
                ersatzrs_domain::TeletextResourceObjectDefinitionAdditionError::Scope => {
                    NativeTeletextEditorialImportError::Association
                }
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let latest = baselines.latest.as_ref().unwrap_or(&baselines.root);
    authorize(
        &latest.body.project,
        latest
            .original
            .view()
            .encoded()
            .ok_or(NativeTeletextEditorialImportError::Association)?,
        &additions,
        policy.encoding.composition.service,
        deadline,
    )?;
    let incoming = TeletextEditorialChanges {
        resource_additions: additions,
        ..Default::default()
    };
    baselines
        .cumulative
        .resource_additions
        .extend_from_slice(&incoming.resource_additions);
    let overlay = repeat::cumulative_overlay(&baselines.provenance, &baselines.cumulative)?;
    let document = append_overlay(
        baselines.root.original.view().document_version(),
        &baselines.root.original_document,
        &overlay,
        policy.document,
    )?;
    let document_digest = digest(document.bytes());
    let stored = TeletextDesignRevision {
        design_id: baselines.root.original.view().design_id(),
        revision,
        document_version: document.version(),
        digest: document_digest.clone(),
        document: document.bytes().to_vec(),
    };
    let parts = StoredDesignParts::decode(&stored, policy.document)?;
    let edited = qualify_rows(
        stored,
        parts,
        &baselines.root,
        policy.encoding.clone(),
        deadline,
    )?;
    super::super::teletext_editorial_effects::validate_teletext_editorial_transition_with_operands(
        latest
            .original
            .view()
            .encoded()
            .ok_or(NativeTeletextEditorialImportError::Association)?,
        edited
            .view()
            .encoded()
            .ok_or(NativeTeletextEditorialImportError::Association)?,
        &latest.body.project,
        incoming.edits(),
        baselines.cumulative.edits(),
        &policy.encoding.encoding,
        deadline,
    )
    .map_err(|error| NativeTeletextEditorialImportError::Effects(Box::new(error)))?;
    check_deadline(deadline)?;
    Ok((document, document_digest))
}

/// Add complete definitions as a separate unpublished concept revision.
///
/// The captured resource and current head are authenticated before native
/// compilation. Existing admission owns every worker through caller cancellation;
/// package, profile and pending/accepted publication state remain unchanged.
/// # Errors
/// Rejects unavailable/tampered receipts, unsupported additions, bounds, invalid
/// dependencies, protected shared effects and deadlines; stale/deleted heads
/// return the existing explicit mutation rejection.
pub async fn add_teletext_editorial_resource_definitions(
    application: &ApplicationServices,
    request: NativeTeletextResourceDefinitionAddRequest,
    policy: NativeTeletextExportPolicy,
    deadline: Instant,
) -> Result<TeletextDesignMutation<SavedEditorialTeletextDesign>> {
    preflight(&request, policy.document.validation.service, deadline)?;
    let guard = super::super::teletext_work::begin(application, Some(deadline))
        .await
        .map_err(|error| NativeTeletextEditorialImportError::Design(Box::new(error.into())))?;
    let (limits, _) = import_limits(&policy);
    let receipt = application
        .teletext_design_export_receipt(
            request.receipt_id,
            limits.max_bytes,
            policy.document.max_serialized_bytes,
        )
        .await?
        .ok_or(NativeTeletextEditorialImportError::MissingReceipt)?;
    if receipt.metadata.digest != request.receipt_digest {
        return Err(NativeTeletextEditorialImportError::Association);
    }
    let Some(head) = application
        .teletext_design_head(receipt.metadata.design_id)
        .await?
    else {
        return Ok(TeletextDesignMutation::Rejected(
            TeletextDesignRejection::MissingDesign,
        ));
    };
    if head.deleted {
        return Ok(TeletextDesignMutation::Rejected(
            TeletextDesignRejection::DeletedDesign,
        ));
    }
    if head.revision != request.expected_design_head {
        return Ok(TeletextDesignMutation::Rejected(
            TeletextDesignRejection::Conflict,
        ));
    }
    // Add extends the current concept. A historical receipt must never replace
    // newer body intents merely because its caller knows the current head.
    if receipt.metadata.revision != head.revision {
        return Ok(TeletextDesignMutation::Rejected(
            TeletextDesignRejection::Conflict,
        ));
    }
    let provenance = TeletextSavedEditorialProvenance {
        export_receipt_id: receipt.metadata.id,
        export_receipt_digest: receipt.metadata.digest,
        original_design_id: receipt.metadata.design_id,
        original_revision: receipt.metadata.revision,
        original_digest: receipt.metadata.design_digest,
    };
    let baseline = read_editorial_baseline(
        application,
        &provenance,
        head.id,
        None,
        policy.document,
        &policy.encoding,
        deadline,
    )
    .await?;
    let root = repeat::read_root(
        application,
        &baseline,
        &policy,
        (deadline, guard.hold_work()),
    )
    .await?;
    let revision = head
        .revision
        .checked_add(1)
        .ok_or(NativeTeletextEditorialImportError::Association)?;
    let maximum = policy.document.max_serialized_bytes;
    let (document, digest) =
        super::super::teletext_work::spawn_blocking(guard.hold_work(), move || {
            let baselines = qualify(baseline, root, &policy, deadline)?;
            encode(baselines, request.definitions, revision, &policy, deadline)
        })
        .await
        .map_err(|_| NativeTeletextEditorialImportError::WorkerFailed)??;
    save_editorial_concept(
        application,
        request.name,
        (head.id, head.revision),
        document,
        digest,
        maximum,
        (deadline, guard.hold_work()),
    )
    .await
}
