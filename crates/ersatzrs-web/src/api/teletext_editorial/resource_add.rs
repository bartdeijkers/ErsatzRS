//! Explicit empty definitions reuse native Add, without uploaded program authority.
use super::{
    authoring, deadline, editorial_teletext_policy, export_error, export_original_teletext_design,
    export_owner, export_request, fields, import_error, positive, rejected, respond, AppState,
    ApplicationServices, BTreeMap, Failure, Form, IntoResponse, NativeTeletextExportPolicy,
    Redirect, Response, Result, State, TeletextDesignMutation, TeletextDesignRejection,
    MAX_TELETEXT_DESIGN_NAME_BYTES, MAX_TELETEXT_EDITOR_UPLOAD_BYTES,
};
use crate::host::teletext_editorial_designs::{
    add_teletext_editorial_resource_definitions, NativeTeletextEditorialImportError,
    NativeTeletextResourceDefinitionAddRequest,
};
use ersatzrs_domain::{
    TeletextEnhancementObjectDefinition, TeletextEnhancementProgram, TeletextObjectId,
    TeletextObjectIdentity, TeletextObjectType, TeletextResourceIdentity, TeletextResourceKind,
};

pub async fn add_object(
    State(state): State<AppState>,
    Form(pairs): Form<Vec<(String, String)>>,
) -> Response {
    let channel = pairs
        .iter()
        .find(|(key, _)| key == "channel_id")
        .and_then(|(_, value)| value.parse::<i64>().ok())
        .unwrap_or(0);
    match perform_add(&state.application, pairs).await {
        Ok(response) => response,
        Err(error) => respond(channel, &error),
    }
}

fn empty_definition(
    values: &BTreeMap<String, String>,
) -> Result<TeletextEnhancementObjectDefinition> {
    let invalid = || Failure::new("object-add-invalid");
    let resource = values
        .get("resource")
        .filter(|resource| resource.len() <= 512)
        .and_then(|resource| serde_json::from_str::<TeletextResourceIdentity>(resource).ok())
        .ok_or_else(invalid)?;
    let id = values
        .get("new_object_id")
        .and_then(|number| number.parse::<u16>().ok())
        .map(TeletextObjectId::new)
        .ok_or_else(invalid)?;
    let identity = match resource.kind() {
        TeletextResourceKind::Pop => TeletextObjectIdentity::public(resource, id),
        TeletextResourceKind::Gpop => TeletextObjectIdentity::global(resource, id),
        _ => return Err(invalid()),
    }
    .map_err(|_| invalid())?;
    let object_type = match values.get("object_type").map(String::as_str) {
        Some("active") => TeletextObjectType::Active,
        Some("adaptive") => TeletextObjectType::Adaptive,
        Some("passive") => TeletextObjectType::Passive,
        _ => return Err(invalid()),
    };
    Ok(TeletextEnhancementObjectDefinition {
        identity,
        object_type,
        program: TeletextEnhancementProgram::new(Vec::new()).map_err(|_| invalid())?,
    })
}

async fn perform_add(
    application: &ApplicationServices,
    pairs: Vec<(String, String)>,
) -> Result<Response> {
    let end = deadline()?;
    let values = fields(
        pairs,
        &[
            "channel_id",
            "design_id",
            "revision",
            "digest",
            "expected_design_head",
            "name",
            "resource",
            "object_type",
            "new_object_id",
        ],
    )?;
    let channel = positive(&values, "channel_id")?;
    let request = export_request(&values)?;
    if request.revision != request.expected_design_head {
        return Err(Failure::new("publication-conflict"));
    }
    let name = values
        .get("name")
        .filter(|name| !name.trim().is_empty() && name.len() <= MAX_TELETEXT_DESIGN_NAME_BYTES)
        .cloned()
        .ok_or_else(|| Failure::new("invalid-input"))?;
    let definition = empty_definition(&values)?;
    let (policy, end) =
        editorial_teletext_policy(application, end, MAX_TELETEXT_EDITOR_UPLOAD_BYTES)
            .await
            .map_err(|error| authoring(&error))?;
    export_owner(application, &request, channel, &policy, end).await?;
    let expected_design_head = request.expected_design_head;
    let export_policy = NativeTeletextExportPolicy {
        document: policy.document,
        encoding: policy.encoding.clone(),
        project: policy.project,
        tti: policy.tti,
    };
    let exported = match export_original_teletext_design(application, request, export_policy, end)
        .await
        .map_err(export_error)?
    {
        TeletextDesignMutation::Saved(exported) => exported,
        TeletextDesignMutation::Rejected(reason) => return Err(add_rejected(reason)),
    };
    let owner = exported.sidecar().project.entry;
    let request = NativeTeletextResourceDefinitionAddRequest {
        name,
        receipt_id: exported.metadata().id,
        receipt_digest: exported.metadata().digest.clone(),
        expected_design_head,
        definitions: vec![definition],
    };
    let saved = match add_teletext_editorial_resource_definitions(application, request, policy, end)
        .await
        .map_err(add_error)?
    {
        TeletextDesignMutation::Saved(saved) => saved,
        TeletextDesignMutation::Rejected(reason) => return Err(add_rejected(reason)),
    };
    Ok(Redirect::to(&format!(
        "/channels/{channel}/teletext?saved=draft&design_id={}&revision={}&page={}&subcode={:04x}",
        saved.head().id,
        saved.head().revision,
        owner.page,
        owner.subcode,
    ))
    .into_response())
}

fn add_error(error: NativeTeletextEditorialImportError) -> Failure {
    match error {
        NativeTeletextEditorialImportError::Association => Failure::new("object-add-invalid"),
        error => import_error(error),
    }
}

fn add_rejected(reason: TeletextDesignRejection) -> Failure {
    match reason {
        TeletextDesignRejection::Conflict => Failure::new("publication-conflict"),
        reason => rejected(reason),
    }
}
