//! Explicit host additions reuse retained originals and the existing concept CAS.
use super::{
    check_deadline, digest, fmt, import_limits, qualify_baseline, qualify_rows,
    read_editorial_baseline, repeat, save_editorial_concept, ApplicationServices,
    EditorialBaseline, EncodedStoredDesign, EntityId, Instant, NativeTeletextEditorialImportError,
    NativeTeletextExportPolicy, QualifiedBaseline, Result, SavedEditorialTeletextDesign,
    StoredDesignParts, TeletextDesignMutation, TeletextDesignRejection, TeletextDesignRevision,
    TeletextEditorialChanges, TeletextSavedEditorialProvenance, MAX_TELETEXT_DESIGN_NAME_BYTES,
};
use ersatzrs_addon_contract::TeletextResourceKindWire;
use ersatzrs_app::teletext_export_project::{TeletextExportAssetScope, TeletextExportProject};
use ersatzrs_domain::{
    TeletextEnhancementObjectDefinition, TeletextObjectId, TeletextObjectIdentity,
    TeletextResourceIdentity, TeletextResourceKind, TeletextResourceObjectDefinitionAddition,
    TeletextServiceBudget,
};
use ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService;

#[path = "definition_additions/retained_banks.rs"]
mod retained_banks;

#[path = "definition_additions/announcements.rs"]
mod announcements;
pub use announcements::{
    announce_teletext_editorial_object_subpages, NativeTeletextObjectSubpagesAnnounceRequest,
};

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

/// Copy one qualified latest native definition into a new ID in the same resource.
/// No caller-supplied program or object type participates in the copy.
pub struct NativeTeletextResourceDefinitionCopyRequest {
    pub name: String,
    pub receipt_id: EntityId,
    pub receipt_digest: String,
    pub expected_design_head: i64,
    pub source: TeletextObjectIdentity,
    pub new_object_id: TeletextObjectId,
}
impl fmt::Debug for NativeTeletextResourceDefinitionCopyRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeTeletextResourceDefinitionCopyRequest")
            .field("receipt_id", &self.receipt_id)
            .field("expected_design_head", &self.expected_design_head)
            .field("source", &self.source)
            .field("new_object_id", &self.new_object_id)
            .finish_non_exhaustive()
    }
}

/// Explicit complete new banks and optional definitions in original banks.
/// Source ordinal and every lifecycle value are supplied, never derived from TTI.
pub struct NativeTeletextObjectResourcesAddRequest {
    pub name: String,
    pub receipt_id: EntityId,
    pub receipt_digest: String,
    pub expected_design_head: i64,
    pub banks: Vec<ersatzrs_app::teletext_design_documents::TeletextSavedObjectResourceAddition>,
    pub definitions: Vec<TeletextEnhancementObjectDefinition>,
}
impl fmt::Debug for NativeTeletextObjectResourcesAddRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeTeletextObjectResourcesAddRequest")
            .field("receipt_id", &self.receipt_id)
            .field("expected_design_head", &self.expected_design_head)
            .field("banks", &self.banks.len())
            .field("definitions", &self.definitions.len())
            .finish_non_exhaustive()
    }
}

#[path = "definition_additions/page_selections.rs"]
mod page_selections;
pub use page_selections::{
    select_teletext_editorial_object_pages, NativeTeletextObjectPagesSelectRequest,
};

enum Action {
    Selections {
        selections:
            Vec<ersatzrs_app::teletext_design_documents::TeletextSavedObjectPageSelectionChange>,
        programs: Vec<ersatzrs_domain::TeletextResourceObjectProgramOverlay>,
    },
    Announcements(
        Vec<ersatzrs_app::teletext_design_documents::TeletextSavedObjectSubpageAnnouncement>,
    ),
    Banks {
        banks: Vec<ersatzrs_app::teletext_design_documents::TeletextSavedObjectResourceAddition>,
        definitions: Vec<TeletextEnhancementObjectDefinition>,
    },
    Definitions(Vec<TeletextEnhancementObjectDefinition>),
    Copy {
        source: TeletextObjectIdentity,
        new_object_id: TeletextObjectId,
    },
}
struct Request {
    name: String,
    receipt_id: EntityId,
    receipt_digest: String,
    expected_design_head: i64,
    action: Action,
}

fn preflight(request: &Request, budget: TeletextServiceBudget, deadline: Instant) -> Result<()> {
    check_deadline(deadline)?;
    if matches!(&request.action, Action::Definitions(definitions) if definitions.is_empty()) {
        return Err(NativeTeletextEditorialImportError::NoChanges);
    }
    if request.name.len() > MAX_TELETEXT_DESIGN_NAME_BYTES
        || request.receipt_id <= 0
        || request.expected_design_head <= 0
        || request.receipt_digest.len() != 64
    {
        return Err(NativeTeletextEditorialImportError::Budget);
    }
    let definitions = match &request.action {
        Action::Selections {
            selections,
            programs,
        } => return page_selections::preflight(selections, programs, budget, deadline),
        Action::Announcements(values) => {
            if values.is_empty() {
                return Err(NativeTeletextEditorialImportError::NoChanges);
            }
            if values.len() > budget.max_records.min(budget.max_structure_items) {
                return Err(NativeTeletextEditorialImportError::Budget);
            }
            return Ok(());
        }
        Action::Definitions(definitions) => definitions,
        Action::Banks { banks, definitions } => {
            return preflight_banks(banks, definitions, budget, deadline);
        }
        Action::Copy {
            source,
            new_object_id,
        } => {
            if budget.max_objects == 0 || budget.max_structure_items == 0 {
                return Err(NativeTeletextEditorialImportError::Budget);
            }
            if source.resource().is_none() || source.object_id() == *new_object_id {
                return Err(NativeTeletextEditorialImportError::Association);
            }
            return Ok(());
        }
    };
    if definitions.len() > budget.max_objects {
        return Err(NativeTeletextEditorialImportError::Budget);
    }
    let mut remaining = budget.max_commands;
    for definition in definitions {
        check_deadline(deadline)?;
        remaining = remaining
            .checked_sub(definition.program.commands().len())
            .ok_or(NativeTeletextEditorialImportError::Budget)?;
    }
    Ok(())
}

fn preflight_banks(
    banks: &[ersatzrs_app::teletext_design_documents::TeletextSavedObjectResourceAddition],
    definitions: &[TeletextEnhancementObjectDefinition],
    budget: TeletextServiceBudget,
    deadline: Instant,
) -> Result<()> {
    if banks.is_empty() {
        return Err(NativeTeletextEditorialImportError::NoChanges);
    }
    if banks.len() > budget.max_records {
        return Err(NativeTeletextEditorialImportError::Budget);
    }
    let mut objects = budget.max_objects;
    let mut commands = budget.max_commands;
    for bank in banks {
        check_deadline(deadline)?;
        objects = objects
            .checked_sub(bank.addition().definitions().len())
            .ok_or(NativeTeletextEditorialImportError::Budget)?;
        for definition in bank.addition().definitions() {
            commands = commands
                .checked_sub(definition.program.commands().len())
                .ok_or(NativeTeletextEditorialImportError::Budget)?;
        }
    }
    objects = objects
        .checked_sub(definitions.len())
        .ok_or(NativeTeletextEditorialImportError::Budget)?;
    for definition in definitions {
        commands = commands
            .checked_sub(definition.program.commands().len())
            .ok_or(NativeTeletextEditorialImportError::Budget)?;
    }
    let _ = (objects, commands);
    Ok(())
}

fn resolve(
    action: Action,
    latest: &QualifiedBaseline,
    budget: TeletextServiceBudget,
    deadline: Instant,
) -> Result<(
    Vec<TeletextEnhancementObjectDefinition>,
    Vec<ersatzrs_app::teletext_design_documents::TeletextSavedObjectResourceAddition>,
    Vec<ersatzrs_app::teletext_design_documents::TeletextSavedObjectSubpageAnnouncement>,
)> {
    check_deadline(deadline)?;
    let (source, new_object_id) = match action {
        Action::Selections { .. } => return Err(NativeTeletextEditorialImportError::Association),
        Action::Announcements(values) => return Ok((vec![], vec![], values)),
        Action::Definitions(definitions) => return Ok((definitions, vec![], vec![])),
        Action::Banks { banks, definitions } => return Ok((definitions, banks, vec![])),
        Action::Copy {
            source,
            new_object_id,
        } => (source, new_object_id),
    };
    let resource = source
        .resource()
        .ok_or(NativeTeletextEditorialImportError::Association)?;
    let definition = latest
        .original
        .view()
        .encoded()
        .ok_or(NativeTeletextEditorialImportError::Association)?
        .declared()
        .semantic()
        .objects()
        .definition(source)
        .ok_or(NativeTeletextEditorialImportError::Association)?;
    if definition.program.commands().len() > budget.max_commands {
        return Err(NativeTeletextEditorialImportError::Budget);
    }
    let identity = match resource.kind() {
        TeletextResourceKind::Pop => TeletextObjectIdentity::public(resource, new_object_id),
        TeletextResourceKind::Gpop => TeletextObjectIdentity::global(resource, new_object_id),
        _ => return Err(NativeTeletextEditorialImportError::Association),
    }
    .map_err(|_| NativeTeletextEditorialImportError::Association)?;
    let mut copied = definition.clone();
    copied.identity = identity;
    check_deadline(deadline)?;
    Ok((vec![copied], vec![], vec![]))
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
    action: Action,
    revision: i64,
    policy: &NativeTeletextExportPolicy,
    deadline: Instant,
) -> Result<(EncodedStoredDesign, String)> {
    check_deadline(deadline)?;
    if let Action::Selections {
        selections,
        programs,
    } = action
    {
        return page_selections::encode(
            baselines, selections, programs, revision, policy, deadline,
        );
    }
    let latest = baselines.latest.as_ref().unwrap_or(&baselines.root);
    let (definitions, banks, announcements) =
        resolve(action, latest, policy.document.validation.service, deadline)?;
    super::object_resources::authorize(latest, &banks, &policy.encoding, deadline)?;
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
    authorize(
        &latest.body.project,
        if banks.is_empty() {
            latest
        } else {
            &baselines.root
        }
        .original
        .view()
        .encoded()
        .ok_or(NativeTeletextEditorialImportError::Association)?,
        &additions,
        policy.encoding.composition.service,
        deadline,
    )?;
    let incoming = TeletextEditorialChanges {
        object_resource_additions: banks,
        object_subpage_announcements: announcements,
        object_page_selections: Vec::new(),
        base_text_edits: Vec::new(),
        resource_additions: additions,
        ..Default::default()
    };
    announcements::append(
        &mut baselines.cumulative,
        &incoming,
        policy.document.validation.service,
        deadline,
    )?;
    retained_banks::append(
        &mut baselines.cumulative,
        &incoming,
        policy.document.validation.service,
        deadline,
    )?;
    finish_encode(&baselines, &incoming, revision, policy, deadline)
}

fn finish_encode(
    baselines: &Baselines,
    incoming: &TeletextEditorialChanges,
    revision: i64,
    policy: &NativeTeletextExportPolicy,
    deadline: Instant,
) -> Result<(EncodedStoredDesign, String)> {
    let latest = baselines.latest.as_ref().unwrap_or(&baselines.root);
    let document = repeat::encode_cumulative_document(
        &baselines.root,
        &baselines.cumulative,
        &baselines.provenance,
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
    let edited = if parts.overlay().is_some() {
        qualify_rows(
            stored,
            parts,
            &baselines.root,
            policy.encoding.clone(),
            deadline,
        )?
    } else {
        parts
            .compile(stored, policy.encoding.clone(), deadline, Vec::new())
            .map_err(|error| NativeTeletextEditorialImportError::Design(Box::new(error)))?
    };
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

/// Add complete banks through the existing retained-receipt worker and head CAS.
/// No publication or installed producer state changes. All original effects,
/// explicit lifecycle, complete graph and source-owner checks remain mandatory.
/// # Errors
/// Rejects empty/invalid banks, stale receipts/heads, occupied owners, bounds,
/// protected effects, native encoding failures and deadlines atomically.
pub async fn add_teletext_editorial_object_resources(
    application: &ApplicationServices,
    request: NativeTeletextObjectResourcesAddRequest,
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
            action: Action::Banks {
                banks: request.banks,
                definitions: request.definitions,
            },
        },
        policy,
        deadline,
    )
    .await
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
    apply(
        application,
        Request {
            name: request.name,
            receipt_id: request.receipt_id,
            receipt_digest: request.receipt_digest,
            expected_design_head: request.expected_design_head,
            action: Action::Definitions(request.definitions),
        },
        policy,
        deadline,
    )
    .await
}

/// Copy a complete latest native resource definition into an unpublished concept.
///
/// Resolves the source after receipt/current-head qualification in the same
/// admitted blocking worker used by Add. Resource/S1, type and complete program
/// are preserved; the existing unused-ID, native, permission and CAS gates apply.
/// # Errors
/// Rejects Local/missing sources, identical or occupied IDs, unavailable/stale
/// receipts, bounds, dependencies, protected effects and expired deadlines.
pub async fn copy_teletext_editorial_resource_definition(
    application: &ApplicationServices,
    request: NativeTeletextResourceDefinitionCopyRequest,
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
            action: Action::Copy {
                source: request.source,
                new_object_id: request.new_object_id,
            },
        },
        policy,
        deadline,
    )
    .await
}

async fn apply(
    application: &ApplicationServices,
    request: Request,
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
            encode(baselines, request.action, revision, &policy, deadline)
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
