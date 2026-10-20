//! Shared current-receipt fences for explicit resource controls, not uploads.
use super::{
    authoring, editorial_teletext_policy, export_error, export_original_teletext_design,
    export_owner, export_request, import_error, positive, rejected, ApplicationServices, BTreeMap,
    ExportedOriginalTeletextDesign, Failure, IntoResponse, NativeTeletextEditorialImportError,
    NativeTeletextExportPolicy, Redirect, Response, Result, TeletextDesignMutation,
    TeletextDesignRejection, TeletextPageAddress, TeletextResourceIdentity,
    MAX_TELETEXT_DESIGN_NAME_BYTES, MAX_TELETEXT_EDITOR_UPLOAD_BYTES,
};
use crate::host::teletext_editorial_designs::SavedEditorialTeletextDesign;
use ersatzrs_app::teletext_export_project::TeletextExportProject;
use std::time::Instant;

pub(super) struct CurrentResourceDraft {
    pub(super) channel: i64,
    pub(super) name: String,
    pub(super) head: i64,
    pub(super) exported: ExportedOriginalTeletextDesign,
    pub(super) policy: NativeTeletextExportPolicy,
    pub(super) end: Instant,
}

pub(super) async fn prepare(
    application: &ApplicationServices,
    values: &BTreeMap<String, String>,
    end: Instant,
) -> Result<CurrentResourceDraft> {
    let channel = positive(values, "channel_id")?;
    let request = export_request(values)?;
    if request.revision != request.expected_design_head {
        return Err(Failure::new("publication-conflict"));
    }
    let name = values
        .get("name")
        .filter(|name| !name.trim().is_empty() && name.len() <= MAX_TELETEXT_DESIGN_NAME_BYTES)
        .cloned()
        .ok_or_else(|| Failure::new("invalid-input"))?;
    let (policy, end) =
        editorial_teletext_policy(application, end, MAX_TELETEXT_EDITOR_UPLOAD_BYTES)
            .await
            .map_err(|error| authoring(&error))?;
    export_owner(application, &request, channel, &policy, end).await?;
    let head = request.expected_design_head;
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
        TeletextDesignMutation::Rejected(reason) => return Err(rejection(reason)),
    };
    Ok(CurrentResourceDraft {
        channel,
        name,
        head,
        exported,
        policy,
        end,
    })
}

/// Resolve the original producer only from the fresh, server-owned export.
/// Optional authoring metadata and submitted numeric ordinals grant no authority.
pub(super) fn source_ordinal(
    project: &TeletextExportProject,
    display: TeletextPageAddress,
    code: &'static str,
) -> Result<usize> {
    let invalid =
        || Failure::new(code).position(display.page.get(), display.subcode.get(), None, None);
    let mut pages = project.pages.iter().filter(|page| {
        page.page.page == display.page.get() && page.page.subcode == display.subcode.get()
    });
    let page = pages.next().ok_or_else(invalid)?;
    let producer = project
        .producers
        .get(page.producer_ordinal)
        .ok_or_else(invalid)?;
    if pages.next().is_some()
        || producer.ordinal != page.producer_ordinal
        || !matches!(producer.contract, 2 | 3)
    {
        return Err(invalid());
    }
    Ok(page.producer_ordinal)
}

pub(super) fn display(value: &str, code: &'static str) -> Result<TeletextPageAddress> {
    if value.len() > 512 {
        return Err(Failure::new(code));
    }
    serde_json::from_str(value).map_err(|_| Failure::new(code))
}

pub(super) fn resource(value: &str, code: &'static str) -> Result<TeletextResourceIdentity> {
    if value.len() > 512 {
        return Err(Failure::new(code));
    }
    serde_json::from_str(value).map_err(|_| Failure::new(code))
}

pub(super) fn channel(pairs: &[(String, String)]) -> i64 {
    pairs
        .iter()
        .find(|(key, _)| key == "channel_id")
        .and_then(|(_, value)| value.parse().ok())
        .unwrap_or(0)
}

pub(super) fn error(error: NativeTeletextEditorialImportError, code: &'static str) -> Failure {
    if matches!(error, NativeTeletextEditorialImportError::Association) {
        Failure::new(code)
    } else {
        import_error(error)
    }
}

pub(super) fn rejection(reason: TeletextDesignRejection) -> Failure {
    match reason {
        TeletextDesignRejection::Conflict => Failure::new("publication-conflict"),
        reason => rejected(reason),
    }
}

pub(super) fn saved(
    channel: i64,
    exported: &ExportedOriginalTeletextDesign,
    saved: &SavedEditorialTeletextDesign,
) -> Response {
    let owner = exported.sidecar().project.entry;
    Redirect::to(&format!(
        "/channels/{}/teletext?saved=draft&design_id={}&revision={}&page={}&subcode={:04x}",
        channel,
        saved.head().id,
        saved.head().revision,
        owner.page,
        owner.subcode,
    ))
    .into_response()
}
