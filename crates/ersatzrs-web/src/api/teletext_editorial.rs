//! Bounded native editor transfers; uploads never select broadcast output.

use crate::host::{
    state::AppState,
    teletext_authoring::{
        design_error, editorial_teletext_policy, editorial_transfer_work, TeletextAuthoringError,
    },
    teletext_design_exports::{
        export_original_teletext_design, ExportedOriginalTeletextDesign, NativeTeletextExportError,
        NativeTeletextExportPolicy, NativeTeletextExportRequest,
    },
    teletext_editorial_designs::{
        import_teletext_editorial_design, NativeTeletextEditorialImportError,
        NativeTeletextEditorialImportRequest, NativeTeletextEditorialTtiFailure,
    },
    teletext_work::{self, TeletextWorkError},
};
use axum::{
    body::Body,
    extract::{Form, Multipart, Query, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Redirect, Response},
};
use ersatzrs_app::{
    teletext_design_documents::{
        decode_teletext_design_documents, decode_teletext_effective_design_documents,
        TeletextDesignDocumentBudget,
    },
    teletext_export_project::{
        parse_teletext_export_sidecar, write_teletext_export_sidecar, TeletextExportFile,
    },
    ApplicationServices,
};
use ersatzrs_core::repositories::{TeletextDesignMutation, TeletextDesignRejection};
use ersatzrs_domain::{
    TeletextDesignRevision, TeletextPageAddress, TeletextPageNumber, TeletextResourceIdentity,
    TeletextSubcode, MAX_TELETEXT_DESIGN_NAME_BYTES, TELETEXT_DESIGN_DOCUMENT_VERSION,
    TELETEXT_EFFECTIVE_DESIGN_DOCUMENT_VERSION,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    future::Future,
    io::{Cursor, Write as _},
    time::Instant,
};

mod resource_add;
mod resource_announcements;
mod resource_banks;
mod resource_controls;
mod resource_copy;
mod resource_remove;
mod resource_selection;
mod secondary_defaults;
pub use resource_add::add_object;
pub use resource_announcements::announce_object_subpages;
pub use resource_banks::add_resource_bank;
pub use resource_copy::copy_object;
pub use resource_remove::remove_unused_object;
pub use resource_selection::{attach_object_page, choose_object_pages, restore_page_attachment};
pub use secondary_defaults::choose_secondary_default;

pub const MAX_TELETEXT_EDITOR_UPLOAD_BYTES: usize = 8 * 1024 * 1024;
const MAX_PARTS: usize = 4096;
const MAX_ZIP_BYTES: usize = 32 * 1024 * 1024;
type Result<T> = std::result::Result<T, Failure>;

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorContext {
    channel_id: Option<i64>,
}

struct Failure {
    code: &'static str,
    location: String,
}
impl Failure {
    fn new(code: &'static str) -> Self {
        Self {
            code,
            location: String::new(),
        }
    }
    fn position(mut self, page: u16, subcode: u16, row: Option<u8>, column: Option<usize>) -> Self {
        self.location = format!("&error_page={page}&error_subcode={subcode:04x}");
        self.grid_position(row, column)
    }
    fn resource_position(
        mut self,
        resource: TeletextResourceIdentity,
        row: Option<u8>,
        column: Option<usize>,
    ) -> Self {
        if let Ok(raw) = serde_json::to_string(&resource) {
            self.location = format!("&error_resource={}", urlencoding::encode(&raw));
        }
        self.grid_position(row, column)
    }
    fn grid_position(mut self, row: Option<u8>, column: Option<usize>) -> Self {
        if let Some(row) = row {
            let _ = write!(self.location, "&error_row={row}");
        }
        if let Some(column) = column {
            let _ = write!(self.location, "&error_column={column}");
        }
        self
    }
}
fn authoring(error: &TeletextAuthoringError) -> Failure {
    Failure {
        code: if error.code() == "limit" {
            "editorial-limit"
        } else {
            error.code()
        },
        location: super::teletext::authoring_error_location(error),
    }
}
fn work(error: &TeletextWorkError) -> Failure {
    Failure::new(match error {
        TeletextWorkError::TimedOut => "expired",
        TeletextWorkError::Saturated => "busy",
        _ => "unavailable",
    })
}
fn rejected(reason: TeletextDesignRejection) -> Failure {
    Failure::new(match reason {
        TeletextDesignRejection::Conflict => "conflict",
        TeletextDesignRejection::MissingDesign => "missing-design",
        _ => "invalid-input",
    })
}
fn respond(channel: i64, error: &Failure) -> Response {
    let path = if channel > 0 {
        format!("/channels/{channel}/teletext")
    } else {
        "/channels".to_owned()
    };
    Redirect::to(&format!("{path}?error={}{}", error.code, error.location)).into_response()
}
fn deadline() -> Result<Instant> {
    Instant::now()
        .checked_add(ersatzrs_app::teletext_channel_documents::PREPARATION_TIMEOUT)
        .ok_or_else(|| Failure::new("expired"))
}
async fn before<T>(end: Instant, future: impl Future<Output = T>) -> Result<T> {
    if Instant::now() >= end {
        return Err(Failure::new("expired"));
    }
    tokio::time::timeout_at(tokio::time::Instant::from_std(end), future)
        .await
        .map_err(|_| Failure::new("expired"))
}
fn fields(pairs: Vec<(String, String)>, allowed: &[&str]) -> Result<BTreeMap<String, String>> {
    let mut values = BTreeMap::new();
    for (key, value) in pairs {
        if !allowed.contains(&key.as_str()) || values.insert(key, value).is_some() {
            return Err(Failure::new("invalid-input"));
        }
    }
    Ok(values)
}
fn positive(values: &BTreeMap<String, String>, key: &str) -> Result<i64> {
    values
        .get(key)
        .and_then(|value| value.parse().ok())
        .filter(|id| *id > 0)
        .ok_or_else(|| Failure::new("invalid-input"))
}
fn export_request(values: &BTreeMap<String, String>) -> Result<NativeTeletextExportRequest> {
    let pages = match (values.get("export_page"), values.get("export_subcode")) {
        (None, None) => None,
        (Some(page), Some(subcode))
            if subcode.len() == 4 && subcode.bytes().all(|b| b.is_ascii_hexdigit()) =>
        {
            Some(vec![TeletextPageAddress {
                page: TeletextPageNumber::new(
                    page.parse().map_err(|_| Failure::new("invalid-input"))?,
                )
                .map_err(|_| Failure::new("invalid-input"))?,
                subcode: TeletextSubcode::new(
                    u16::from_str_radix(subcode, 16).map_err(|_| Failure::new("invalid-input"))?,
                )
                .map_err(|_| Failure::new("invalid-input"))?,
            }])
        }
        _ => return Err(Failure::new("invalid-input")),
    };
    Ok(NativeTeletextExportRequest {
        design_id: positive(values, "design_id")?,
        revision: positive(values, "revision")?,
        design_digest: values
            .get("digest")
            .cloned()
            .ok_or_else(|| Failure::new("invalid-input"))?,
        expected_design_head: positive(values, "expected_design_head")?,
        pages,
    })
}

async fn check_revision_owner(
    original: TeletextDesignRevision,
    channel: i64,
    budget: TeletextDesignDocumentBudget,
    end: Instant,
    hold: ersatzrs_core::locking::TeletextWorkHold,
) -> Result<()> {
    editorial_transfer_work(hold, end, move || {
        // The repository-read role determines the decoder. Uploaded metadata
        // cannot borrow a base binding or change an effective original's role.
        let owner = match original.document_version {
            TELETEXT_DESIGN_DOCUMENT_VERSION => {
                decode_teletext_design_documents(&original.document, budget)
                    .map_err(|_| Failure::new("encoding"))?
                    .selection()
                    .channel_id
            }
            TELETEXT_EFFECTIVE_DESIGN_DOCUMENT_VERSION => {
                decode_teletext_effective_design_documents(&original.document, budget)
                    .map_err(|_| Failure::new("encoding"))?
                    .selection()
                    .channel_id
            }
            _ => return Err(Failure::new("encoding")),
        };
        if owner != channel {
            return Err(Failure::new("foreign-channel"));
        }
        Ok(())
    })
    .await
    .map_err(|error| authoring(&error))?
}
async fn export_owner(
    application: &ApplicationServices,
    request: &NativeTeletextExportRequest,
    channel: i64,
    policy: &NativeTeletextExportPolicy,
    end: Instant,
) -> Result<()> {
    let guard = teletext_work::begin(application, Some(end))
        .await
        .map_err(|error| work(&error))?;
    let original = before(
        end,
        application.teletext_design_revision(
            request.design_id,
            request.revision,
            policy.document.max_serialized_bytes,
        ),
    )
    .await?
    .map_err(|_| Failure::new("unavailable"))?
    .ok_or_else(|| Failure::new("missing-design"))?;
    check_revision_owner(original, channel, policy.document, end, guard.hold_work()).await
}

pub async fn export_design(
    State(state): State<AppState>,
    Form(pairs): Form<Vec<(String, String)>>,
) -> Response {
    let channel = pairs
        .iter()
        .find(|(key, _)| key == "channel_id")
        .and_then(|(_, value)| value.parse::<i64>().ok())
        .unwrap_or(0);
    match perform_export(&state.application, pairs).await {
        Ok(response) => response,
        Err(error) => respond(channel, &error),
    }
}
async fn perform_export(
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
            "export_page",
            "export_subcode",
        ],
    )?;
    let channel = positive(&values, "channel_id")?;
    let request = export_request(&values)?;
    let filename = format!("teletext-{}-{}.zip", request.design_id, request.revision);
    let (policy, end) =
        editorial_teletext_policy(application, end, MAX_TELETEXT_EDITOR_UPLOAD_BYTES)
            .await
            .map_err(|error| authoring(&error))?;
    export_owner(application, &request, channel, &policy, end).await?;
    let project_budget = policy.project;
    let exported = match export_original_teletext_design(application, request, policy, end)
        .await
        .map_err(export_error)?
    {
        TeletextDesignMutation::Saved(exported) => exported,
        TeletextDesignMutation::Rejected(reason) => return Err(rejected(reason)),
    };
    let guard = teletext_work::begin(application, Some(end))
        .await
        .map_err(|error| work(&error))?;
    let bytes = editorial_transfer_work(guard.hold_work(), end, move || {
        archive(&exported, project_budget)
    })
    .await
    .map_err(|error| authoring(&error))??;
    let mut response = Body::from(bytes).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/zip"),
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
            .map_err(|_| Failure::new("unavailable"))?,
    );
    Ok(response)
}
fn archive(
    exported: &ExportedOriginalTeletextDesign,
    budget: ersatzrs_app::teletext_export_project::TeletextExportProjectBudget,
) -> Result<Vec<u8>> {
    let sidecar = write_teletext_export_sidecar(exported.sidecar(), budget)
        .map_err(|_| Failure::new("editorial-limit"))?;
    let estimate = exported
        .files()
        .iter()
        .try_fold(sidecar.len() + 4096, |total, file| {
            total
                .checked_add(file.bytes.len())
                .and_then(|n| n.checked_add(1024 + 2 * file.name.len()))
        })
        .ok_or_else(|| Failure::new("editorial-limit"))?;
    if estimate > MAX_ZIP_BYTES || exported.files().len() >= MAX_PARTS {
        return Err(Failure::new("editorial-limit"));
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(estimate)
        .map_err(|_| Failure::new("editorial-limit"))?;
    let mut writer = zip::ZipWriter::new(Cursor::new(bytes));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for file in exported.files() {
        if !flat_name(&file.name) || file.name == "project.ersatzrs.json" {
            return Err(Failure::new("editorial-mismatch"));
        }
        writer
            .start_file(&file.name, options)
            .map_err(|_| Failure::new("unavailable"))?;
        writer
            .write_all(&file.bytes)
            .map_err(|_| Failure::new("unavailable"))?;
    }
    writer
        .start_file("project.ersatzrs.json", options)
        .map_err(|_| Failure::new("unavailable"))?;
    writer
        .write_all(&sidecar)
        .map_err(|_| Failure::new("unavailable"))?;
    let bytes = writer
        .finish()
        .map_err(|_| Failure::new("unavailable"))?
        .into_inner();
    if bytes.len() > estimate {
        return Err(Failure::new("editorial-limit"));
    }
    Ok(bytes)
}
fn export_error(error: NativeTeletextExportError) -> Failure {
    match error {
        NativeTeletextExportError::Design(error) => authoring(&design_error(error)),
        NativeTeletextExportError::TimedOut => Failure::new("expired"),
        NativeTeletextExportError::WorkerFailed => Failure::new("worker"),
        NativeTeletextExportError::Mapping(error)
            if matches!(
                *error,
                crate::host::teletext_design_exports::NativeTeletextExportProjectError::NoDisplay
            ) =>
        {
            Failure::new("editorial-no-pages")
        }
        _ => Failure::new("editorial-mismatch"),
    }
}

struct Upload {
    values: BTreeMap<String, String>,
    sidecar: Option<Vec<u8>>,
    files: Vec<TeletextExportFile>,
}
fn flat_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name != "."
        && name != ".."
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
}
async fn read_upload(
    mut multipart: Multipart,
    end: Instant,
    channel: &mut i64,
    expected_channel: Option<i64>,
) -> Result<Upload> {
    let mut upload = Upload {
        values: BTreeMap::new(),
        sidecar: None,
        files: Vec::new(),
    };
    let mut names = BTreeSet::new();
    let mut total = 0usize;
    let mut count = 0usize;
    while let Some(mut field) = before(end, multipart.next_field())
        .await?
        .map_err(|error| {
            Failure::new(if error.status() == StatusCode::PAYLOAD_TOO_LARGE {
                "editorial-limit"
            } else {
                "invalid-input"
            })
        })?
    {
        count += 1;
        if count > MAX_PARTS {
            return Err(Failure::new("editorial-limit"));
        }
        let key = field
            .name()
            .ok_or_else(|| Failure::new("invalid-input"))?
            .to_owned();
        let filename = field.file_name().map(str::to_owned);
        let limit = match key.as_str() {
            "channel_id" | "expected_design_head" => 32,
            "name" => MAX_TELETEXT_DESIGN_NAME_BYTES,
            "sidecar" | "files" => MAX_TELETEXT_EDITOR_UPLOAD_BYTES,
            _ => return Err(Failure::new("invalid-input")),
        };
        if key == "files" && !filename.as_deref().is_some_and(flat_name) {
            return Err(Failure::new("invalid-input"));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = before(end, field.chunk()).await?.map_err(|error| {
            Failure::new(if error.status() == StatusCode::PAYLOAD_TOO_LARGE {
                "editorial-limit"
            } else {
                "invalid-input"
            })
        })? {
            total = total
                .checked_add(chunk.len())
                .ok_or_else(|| Failure::new("editorial-limit"))?;
            if total > MAX_TELETEXT_EDITOR_UPLOAD_BYTES
                || bytes.len().saturating_add(chunk.len()) > limit
            {
                return Err(Failure::new("editorial-limit"));
            }
            bytes.extend_from_slice(&chunk);
        }
        match key.as_str() {
            "sidecar" if upload.sidecar.is_none() => upload.sidecar = Some(bytes),
            "files" => {
                let name = filename.ok_or_else(|| Failure::new("invalid-input"))?;
                if name == "project.ersatzrs.json" || !names.insert(name.clone()) {
                    return Err(Failure::new("invalid-input"));
                }
                upload.files.push(TeletextExportFile { name, bytes });
            }
            "channel_id" | "expected_design_head" | "name" if filename.is_none() => {
                let text = String::from_utf8(bytes).map_err(|_| Failure::new("invalid-input"))?;
                if key == "channel_id" {
                    let body_channel = text
                        .parse::<i64>()
                        .ok()
                        .filter(|id| *id > 0)
                        .ok_or_else(|| Failure::new("invalid-input"))?;
                    if expected_channel.is_some_and(|id| id != body_channel) {
                        return Err(Failure::new("foreign-channel"));
                    }
                    *channel = body_channel;
                }
                if upload.values.insert(key, text).is_some() {
                    return Err(Failure::new("invalid-input"));
                }
            }
            _ => return Err(Failure::new("invalid-input")),
        }
    }
    if upload.sidecar.is_none() || upload.files.is_empty() {
        return Err(Failure::new("invalid-input"));
    }
    Ok(upload)
}

pub async fn import_design(
    State(state): State<AppState>,
    Query(context): Query<EditorContext>,
    multipart: Multipart,
) -> Response {
    let expected_channel = context.channel_id;
    let mut channel = expected_channel.unwrap_or(0);
    if expected_channel.is_some_and(|id| id <= 0) {
        return respond(0, &Failure::new("invalid-input"));
    }
    match perform_import(
        &state.application,
        multipart,
        &mut channel,
        expected_channel,
    )
    .await
    {
        Ok(response) => response,
        Err(error) => respond(channel, &error),
    }
}
async fn perform_import(
    application: &ApplicationServices,
    multipart: Multipart,
    channel: &mut i64,
    expected_channel: Option<i64>,
) -> Result<Response> {
    let (policy, end) =
        editorial_teletext_policy(application, deadline()?, MAX_TELETEXT_EDITOR_UPLOAD_BYTES)
            .await
            .map_err(|error| authoring(&error))?;
    let upload = read_upload(multipart, end, channel, expected_channel).await?;
    let channel = positive(&upload.values, "channel_id")?;
    let expected_design_head = positive(&upload.values, "expected_design_head")?;
    let name = upload
        .values
        .get("name")
        .cloned()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| Failure::new("invalid-input"))?;
    let sidecar = upload
        .sidecar
        .ok_or_else(|| Failure::new("invalid-input"))?;
    let guard = teletext_work::begin(application, Some(end))
        .await
        .map_err(|error| work(&error))?;
    let budget = policy.project;
    let parse_bytes = sidecar.clone();
    let parsed = editorial_transfer_work(guard.hold_work(), end, move || {
        parse_teletext_export_sidecar(&parse_bytes, budget)
    })
    .await
    .map_err(|error| authoring(&error))?
    .map_err(|_| Failure::new("invalid-input"))?;
    drop(guard);
    receipt_owner(
        application,
        parsed.receipt_id,
        &parsed.receipt_digest,
        channel,
        &policy,
        end,
    )
    .await?;
    let owner = parsed
        .project
        .pages
        .first()
        .ok_or_else(|| Failure::new("invalid-input"))?
        .page;
    let request = NativeTeletextEditorialImportRequest {
        name,
        receipt_id: parsed.receipt_id,
        receipt_digest: parsed.receipt_digest,
        expected_design_head,
        sidecar,
        files: upload.files,
    };
    let saved = match import_teletext_editorial_design(application, request, policy, end)
        .await
        .map_err(import_error)?
    {
        TeletextDesignMutation::Saved(saved) => saved,
        TeletextDesignMutation::Rejected(reason) => return Err(rejected(reason)),
    };
    Ok(Redirect::to(&format!(
        "/channels/{channel}/teletext?saved=draft&design_id={}&revision={}&page={}&subcode={:04x}",
        saved.head().id,
        saved.head().revision,
        owner.page,
        owner.subcode
    ))
    .into_response())
}
async fn receipt_owner(
    application: &ApplicationServices,
    receipt_id: i64,
    digest: &str,
    channel: i64,
    policy: &NativeTeletextExportPolicy,
    end: Instant,
) -> Result<()> {
    let guard = teletext_work::begin(application, Some(end))
        .await
        .map_err(|error| work(&error))?;
    let receipt = before(
        end,
        application.teletext_design_export_receipt(
            receipt_id,
            policy.project.max_bytes,
            policy.document.max_serialized_bytes,
        ),
    )
    .await?
    .map_err(|_| Failure::new("unavailable"))?
    .ok_or_else(|| Failure::new("editorial-missing-receipt"))?;
    if receipt.metadata.digest != digest {
        return Err(Failure::new("editorial-mismatch"));
    }
    let original = before(
        end,
        application.teletext_design_revision(
            receipt.metadata.design_id,
            receipt.metadata.revision,
            policy.document.max_serialized_bytes,
        ),
    )
    .await?
    .map_err(|_| Failure::new("unavailable"))?
    .ok_or_else(|| Failure::new("missing-design"))?;
    check_revision_owner(original, channel, policy.document, end, guard.hold_work()).await
}
fn import_error(error: NativeTeletextEditorialImportError) -> Failure {
    match error {
        NativeTeletextEditorialImportError::TimedOut => Failure::new("expired"),
        NativeTeletextEditorialImportError::WorkerFailed => Failure::new("worker"),
        NativeTeletextEditorialImportError::Budget => Failure::new("editorial-limit"),
        NativeTeletextEditorialImportError::MissingReceipt => {
            Failure::new("editorial-missing-receipt")
        }
        NativeTeletextEditorialImportError::NoChanges => Failure::new("editorial-no-changes"),
        NativeTeletextEditorialImportError::Design(error) => authoring(&design_error(*error)),
        NativeTeletextEditorialImportError::Tti(error) => {
            let result = Failure::new(
                if error.failure == NativeTeletextEditorialTtiFailure::Syntax {
                    "editorial-syntax"
                } else {
                    "editorial-protected"
                },
            );
            let column = error
                .column
                .or(error.cause.as_ref().and_then(|cause| cause.column));
            let mut result = match error.resource {
                Some(resource) => result.resource_position(resource, error.row, column),
                None => result.position(error.page, error.subcode, error.row, column),
            };
            if let Some(cause) = error.cause {
                let _ = write!(result.location, "&error_line={}", cause.line);
            }
            if let Some(designation) = error.designation {
                let _ = write!(result.location, "&error_designation={designation}");
            }
            result
        }
        NativeTeletextEditorialImportError::Effects(error) => match error.page {
            Some(owner) => Failure::new("editorial-protected").position(
                owner.page.get(),
                owner.subcode.get(),
                error.position.map(|(row, _)| row),
                error.position.map(|(_, column)| usize::from(column)),
            ),
            None => Failure::new("editorial-protected"),
        },
        _ => Failure::new("editorial-mismatch"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::teletext_editorial_designs::NativeTeletextEditorialTtiError;
    use ersatzrs_app::teletext_tti::{TeletextTtiError, TeletextTtiErrorKind};
    use ersatzrs_domain::{
        TeletextRawDataPageNumber, TeletextResourceKind, TeletextResourceSubpageIndex,
    };

    fn query(error: NativeTeletextEditorialTtiError) -> BTreeMap<String, String> {
        let response = respond(
            7,
            &import_error(NativeTeletextEditorialImportError::Tti(error)),
        );
        let location = response.headers()[header::LOCATION].to_str().unwrap();
        let (path, query) = location.split_once('?').unwrap();
        assert_eq!(path, "/channels/7/teletext");
        serde_urlencoded::from_str(query).unwrap()
    }
    fn located(resource: Option<TeletextResourceIdentity>) -> NativeTeletextEditorialTtiError {
        NativeTeletextEditorialTtiError {
            failure: NativeTeletextEditorialTtiFailure::Packet,
            resource,
            page: resource.map_or(200, |_| 0x1c0),
            subcode: resource.map_or(16, |identity| u16::from(identity.logical_subpage().get())),
            row: Some(28),
            column: Some(5),
            designation: Some(3),
            cause: None,
        }
    }

    #[test]
    fn display_editor_diagnostic_retains_decimal_page_and_subcode() {
        let values = query(located(None));
        assert_eq!(values["error"], "editorial-protected");
        assert_eq!(values["error_page"], "200");
        assert_eq!(values["error_subcode"], "0010");
        assert_eq!(values["error_row"], "28");
        assert_eq!(values["error_column"], "5");
        assert_eq!(values["error_designation"], "3");
        assert!(!values.contains_key("error_resource"));
    }

    #[test]
    fn resource_editor_diagnostic_preserves_kind_raw_page_and_native_s1() {
        for kind in [TeletextResourceKind::Drcs, TeletextResourceKind::Gdrcs] {
            let resource = TeletextResourceIdentity::new(
                kind,
                TeletextRawDataPageNumber::new(1, 0xc0).unwrap(),
                TeletextResourceSubpageIndex::new(1).unwrap(),
            )
            .unwrap();
            let values = query(located(Some(resource)));
            let decoded: TeletextResourceIdentity =
                serde_json::from_str(&values["error_resource"]).unwrap();
            assert_eq!(decoded, resource);
            assert_eq!(values["error_row"], "28");
            assert_eq!(values["error_column"], "5");
            assert_eq!(values["error_designation"], "3");
            assert!(!values.contains_key("error_page"));
            assert!(!values.contains_key("error_subcode"));
            assert!(!values.values().any(|value| value == "448"));

            let mut syntax = located(Some(resource));
            syntax.failure = NativeTeletextEditorialTtiFailure::Syntax;
            syntax.column = None;
            syntax.cause = Some(TeletextTtiError {
                kind: TeletextTtiErrorKind::InvalidPacket,
                line: 9,
                row: Some(28),
                column: Some(17),
            });
            let values = query(syntax);
            assert_eq!(values["error"], "editorial-syntax");
            assert_eq!(values["error_line"], "9");
            assert_eq!(values["error_column"], "17");
            assert_eq!(values["error_designation"], "3");
            assert_eq!(
                serde_json::from_str::<TeletextResourceIdentity>(&values["error_resource"])
                    .unwrap(),
                resource
            );
            assert!(!values.contains_key("error_page"));
        }
    }
}
