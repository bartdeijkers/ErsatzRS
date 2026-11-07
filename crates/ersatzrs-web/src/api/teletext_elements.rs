//! Native form handlers for the teletext element library (`/teletext-elements`)
//! and the channel "custom pages" links. Every handler redirects with a
//! stable `?saved=`/`?error=` code so pages work without JavaScript.

use std::collections::HashMap;

use axum::{
    extract::{Multipart, Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Redirect, Response},
    Form,
};
use ersatzrs_app::teletext_elements::{import::ElementImportWarning, TeletextElementError};
use ersatzrs_domain::{
    parse_teletext_element_page_number, TeletextElementBinding, TeletextElementBindingSource,
    MAX_TELETEXT_ELEMENT_UPLOAD_BYTES,
};

use crate::host::state::AppState;

/// Multipart body limit: the file plus a small margin for form fields.
#[must_use]
pub const fn max_upload_bytes() -> usize {
    MAX_TELETEXT_ELEMENT_UPLOAD_BYTES + 16 * 1024
}

fn editor_path(id: i64) -> String {
    format!("/teletext-elements/{id}")
}

fn error_redirect(base: &str, error: TeletextElementError) -> Redirect {
    Redirect::to(&format!("{base}?error={}", error.code()))
}

/// Stable query code for an import warning.
#[must_use]
pub const fn warning_code(warning: ElementImportWarning) -> &'static str {
    match warning {
        ElementImportWarning::ExtraPagesIgnored => "extra-pages",
        ElementImportWarning::UnreadableRecordsSkipped => "skipped-records",
        ElementImportWarning::EnhancementsNotImported => "enhancements",
        ElementImportWarning::SubpagesTruncated => "subpages",
        ElementImportWarning::UnknownLanguage => "language",
    }
}

fn warnings_query(warnings: &[ElementImportWarning]) -> String {
    if warnings.is_empty() {
        return String::new();
    }
    let codes: Vec<&str> = warnings.iter().map(|w| warning_code(*w)).collect();
    format!("&warn={}", codes.join(","))
}

struct UploadForm {
    file_name: String,
    bytes: Vec<u8>,
    fields: HashMap<String, String>,
}

async fn read_upload(mut multipart: Multipart) -> Result<UploadForm, TeletextElementError> {
    let mut form = UploadForm {
        file_name: String::new(),
        bytes: Vec::new(),
        fields: HashMap::new(),
    };
    loop {
        let field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(error) => {
                tracing::warn!(%error, "failed to parse teletext element upload");
                return Err(TeletextElementError::Import(
                    ersatzrs_app::teletext_elements::import::ElementImportError::TooLarge,
                ));
            }
        };
        let Some(name) = field.name().map(str::to_owned) else {
            continue;
        };
        if name == "file" {
            form.file_name = field.file_name().unwrap_or_default().to_owned();
            form.bytes = field
                .bytes()
                .await
                .map_err(|_| {
                    TeletextElementError::Import(
                        ersatzrs_app::teletext_elements::import::ElementImportError::TooLarge,
                    )
                })?
                .to_vec();
        } else if let Ok(value) = field.text().await {
            form.fields.insert(name, value);
        }
    }
    if form.bytes.is_empty() {
        return Err(TeletextElementError::Import(
            ersatzrs_app::teletext_elements::import::ElementImportError::Empty,
        ));
    }
    Ok(form)
}

/// `POST /admin-api/teletext-elements/upload`
pub async fn upload(State(state): State<AppState>, multipart: Multipart) -> Redirect {
    let form = match read_upload(multipart).await {
        Ok(form) => form,
        Err(error) => return error_redirect("/teletext-elements/add", error),
    };
    match state
        .application
        .upload_teletext_element(&form.file_name, &form.bytes)
        .await
    {
        Ok(upload) => Redirect::to(&format!(
            "{}?saved=uploaded{}",
            editor_path(upload.id),
            warnings_query(&upload.warnings)
        )),
        Err(error) => error_redirect("/teletext-elements/add", error),
    }
}

fn revision(fields: &HashMap<String, String>) -> i64 {
    fields
        .get("revision")
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or_default()
}

/// `POST /admin-api/teletext-elements/{id}/replace`
pub async fn replace(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    multipart: Multipart,
) -> Redirect {
    let base = editor_path(id);
    let form = match read_upload(multipart).await {
        Ok(form) => form,
        Err(error) => return error_redirect(&base, error),
    };
    match state
        .application
        .replace_teletext_element_file(id, revision(&form.fields), &form.file_name, &form.bytes)
        .await
    {
        Ok(warnings) => Redirect::to(&format!(
            "{base}?saved=replaced{}",
            warnings_query(&warnings)
        )),
        Err(error) => error_redirect(&base, error),
    }
}

/// `POST /admin-api/teletext-elements/{id}/details`
pub async fn save_details(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(fields): Form<HashMap<String, String>>,
) -> Redirect {
    let base = editor_path(id);
    let Some(page) = fields
        .get("page_number")
        .and_then(|value| parse_teletext_element_page_number(value))
    else {
        return error_redirect(&base, TeletextElementError::InvalidPageNumber);
    };
    let Some(cycle) = fields
        .get("cycle_seconds")
        .and_then(|value| value.trim().parse::<u16>().ok())
    else {
        return error_redirect(&base, TeletextElementError::InvalidCycle);
    };
    let name = fields.get("name").map_or("", String::as_str);
    match state
        .application
        .save_teletext_element_details(id, revision(&fields), name, page, cycle)
        .await
    {
        Ok(()) => Redirect::to(&format!("{base}?saved=details")),
        Err(error) => error_redirect(&base, error),
    }
}

/// Rebuild bindings from `field_<index>_source` / `field_<index>_text`
/// against the stored list, so positions can never be forged by the form.
fn bindings_from_form(
    stored: &[TeletextElementBinding],
    fields: &HashMap<String, String>,
) -> Option<Vec<TeletextElementBinding>> {
    stored
        .iter()
        .enumerate()
        .map(|(index, binding)| {
            let source = fields
                .get(&format!("field_{index}_source"))
                .map_or(Some(binding.source), |value| {
                    TeletextElementBindingSource::from_key(value)
                })?;
            let static_text = if source == TeletextElementBindingSource::Static {
                fields
                    .get(&format!("field_{index}_text"))
                    .cloned()
                    .unwrap_or_default()
            } else {
                String::new()
            };
            Some(TeletextElementBinding {
                source,
                static_text,
                ..binding.clone()
            })
        })
        .collect()
}

/// `POST /admin-api/teletext-elements/{id}/fields`
pub async fn save_fields(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(fields): Form<HashMap<String, String>>,
) -> Redirect {
    let base = editor_path(id);
    let element = match state.application.get_teletext_element(id).await {
        Ok(Some(element)) => element,
        Ok(None) => return error_redirect(&base, TeletextElementError::NotFound),
        Err(error) => return error_redirect(&base, error),
    };
    let Some(bindings) = bindings_from_form(&element.bindings, &fields) else {
        return error_redirect(&base, TeletextElementError::InvalidBindings);
    };
    let channel = fields
        .get("preview_channel")
        .filter(|value| !value.is_empty())
        .map(|value| format!("&channel={}", urlencoding::encode(value)))
        .unwrap_or_default();
    match state
        .application
        .save_teletext_element_bindings(id, revision(&fields), bindings)
        .await
    {
        Ok(()) => Redirect::to(&format!("{base}?saved=fields{channel}")),
        Err(error) => error_redirect(&base, error),
    }
}

/// `POST /admin-api/teletext-elements/{id}/delete`
pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(fields): Form<HashMap<String, String>>,
) -> Redirect {
    if fields.get("confirm").map(String::as_str) != Some("yes") {
        return Redirect::to(&format!("{}?error=confirm", editor_path(id)));
    }
    match state.application.delete_teletext_element(id).await {
        Ok(()) => Redirect::to("/teletext-elements?saved=deleted"),
        Err(error) => error_redirect(&editor_path(id), error),
    }
}

/// `GET /admin-api/teletext-elements/{id}/download` — canonical TTI.
pub async fn download(State(state): State<AppState>, Path(id): Path<i64>) -> Response {
    match state.application.get_teletext_element(id).await {
        Ok(Some(element)) => {
            let stem: String = element
                .name
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '-' {
                        c
                    } else {
                        '_'
                    }
                })
                .take(60)
                .collect();
            (
                [
                    (header::CONTENT_TYPE, "text/plain; charset=utf-8".to_owned()),
                    (
                        header::CONTENT_DISPOSITION,
                        format!("attachment; filename=\"{stem}.tti\""),
                    ),
                ],
                element.document,
            )
                .into_response()
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

fn channel_return(channel_id: i64, fields: &HashMap<String, String>) -> String {
    // Only same-site admin paths are accepted as return targets.
    fields
        .get("return")
        .filter(|path| path.starts_with('/') && !path.starts_with("//"))
        .cloned()
        .unwrap_or_else(|| format!("/channels/{channel_id}/teletext"))
}

fn with_query(path: &str, query: &str) -> String {
    let (path, fragment) = path.split_once('#').map_or((path, ""), |(p, f)| (p, f));
    let separator = if path.contains('?') { '&' } else { '?' };
    if fragment.is_empty() {
        format!("{path}{separator}{query}")
    } else {
        format!("{path}{separator}{query}#{fragment}")
    }
}

/// `POST /admin-api/channels/{id}/teletext-elements/save`
pub async fn save_channel_link(
    State(state): State<AppState>,
    Path(channel_id): Path<i64>,
    Form(fields): Form<HashMap<String, String>>,
) -> Redirect {
    let target = channel_return(channel_id, &fields);
    let Some(element_id) = fields
        .get("element_id")
        .and_then(|value| value.trim().parse::<i64>().ok())
    else {
        return Redirect::to(&with_query(&target, "error=custom-page-element"));
    };
    let page = match fields.get("page_number").map(|value| value.trim()) {
        None | Some("") => None,
        Some(value) => match parse_teletext_element_page_number(value) {
            Some(page) => Some(page),
            None => return Redirect::to(&with_query(&target, "error=custom-page-page-number")),
        },
    };
    let enabled = fields.get("enabled").map_or(true, |value| value != "0");
    match state
        .application
        .save_channel_teletext_element(channel_id, element_id, page, enabled)
        .await
    {
        Ok(_) => Redirect::to(&with_query(&target, "saved=custom-page")),
        Err(error) => Redirect::to(&with_query(
            &target,
            &format!("error=custom-page-{}", error.code()),
        )),
    }
}

/// `POST /admin-api/channels/{id}/teletext-elements/{link_id}/remove`
pub async fn remove_channel_link(
    State(state): State<AppState>,
    Path((channel_id, link_id)): Path<(i64, i64)>,
    Form(fields): Form<HashMap<String, String>>,
) -> Redirect {
    let target = channel_return(channel_id, &fields);
    match state
        .application
        .remove_channel_teletext_element(channel_id, link_id)
        .await
    {
        Ok(()) => Redirect::to(&with_query(&target, "saved=custom-page-removed")),
        Err(error) => Redirect::to(&with_query(
            &target,
            &format!("error=custom-page-{}", error.code()),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding(source: TeletextElementBindingSource) -> TeletextElementBinding {
        TeletextElementBinding {
            field: "NOW".to_owned(),
            subpage: 0,
            row: 4,
            column: 1,
            length: 10,
            source,
            static_text: String::new(),
        }
    }

    #[test]
    fn form_bindings_keep_stored_positions_and_only_change_the_source() {
        let stored = vec![binding(TeletextElementBindingSource::Unset)];
        let mut fields = HashMap::new();
        fields.insert("field_0_source".to_owned(), "static".to_owned());
        fields.insert("field_0_text".to_owned(), "Hallo".to_owned());
        fields.insert("field_0_row".to_owned(), "1".to_owned());
        let bindings = bindings_from_form(&stored, &fields).expect("valid");
        assert_eq!(bindings[0].row, 4);
        assert_eq!(bindings[0].static_text, "Hallo");
        fields.insert("field_0_source".to_owned(), "bogus".to_owned());
        assert!(bindings_from_form(&stored, &fields).is_none());
    }

    #[test]
    fn return_targets_stay_on_site_and_keep_fragments() {
        let mut fields = HashMap::new();
        fields.insert("return".to_owned(), "//evil.example".to_owned());
        assert_eq!(channel_return(3, &fields), "/channels/3/teletext");
        assert_eq!(
            with_query("/channels/3/teletext#custom-pages", "saved=x"),
            "/channels/3/teletext?saved=x#custom-pages"
        );
    }
}
