//! Native authoring uses the same domain, YAML and rendering repository ports.
use crate::{
    admin::{
        i18n::{text, TextKey, UiLanguage},
        pages::graphics_elements::{authoring::error_text, editor::escape_html},
        server_fns::{
            graphics_authoring::{default_element, from_fields, kind, Fields},
            settings::load_ui_language_for_api,
        },
    },
    host::state::AppState,
};
use axum::{
    extract::{Form, State},
    http::{header, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use ersatzrs_core::repositories::{GraphicsRenderPreviewRequest, RepositoryError};

fn invalid() -> RepositoryError {
    RepositoryError::Validation {
        field: "graphics authoring",
        message: "graphics-authoring-invalid".into(),
    }
}
fn code(error: &RepositoryError) -> &'static str {
    if let RepositoryError::Validation { message, .. } = error {
        for code in [
            "graphics-authoring-missing-font",
            "graphics-authoring-missing-media",
            "graphics-authoring-missing-variable",
            "graphics-authoring-preview-failed",
        ] {
            if message == code {
                return code;
            }
        }
    }
    if matches!(error, RepositoryError::Decode { kind: "graphics template", value } if value.starts_with("unknown variable "))
    {
        "graphics-authoring-missing-variable"
    } else {
        "graphics-authoring-invalid"
    }
}

pub async fn author(State(state): State<AppState>, Form(fields): Form<Fields>) -> Response {
    let id = fields
        .get("id")
        .and_then(|id| id.parse::<i64>().ok())
        .unwrap_or(-1);
    let intent = fields.get("intent").map_or("save", String::as_str);
    let language = match load_ui_language_for_api(state.application.ports().admin_settings()).await
    {
        Ok(language) => language,
        Err(error) => {
            tracing::warn!(%error, "failed to load UI language for graphics authoring");
            UiLanguage::English
        }
    };
    let result = process(&state, &fields, id, intent, language).await;
    match result {
        Ok(response) => response,
        Err(error) if intent == "preview" => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Html(format!(
                "<p role=\"alert\">{}</p>",
                error_text(language, code(&error))
            )),
        )
            .into_response(),
        Err(error) => {
            let path = if id == 0 {
                "/graphics-elements/add".to_owned()
            } else {
                format!("/graphics-elements/{id}")
            };
            Redirect::to(&format!("{path}?error={}", code(&error))).into_response()
        }
    }
}

async fn process(
    state: &AppState,
    fields: &Fields,
    id: i64,
    intent: &str,
    language: UiLanguage,
) -> Result<Response, RepositoryError> {
    if id < 0 || fields.values().map(String::len).sum::<usize>() > 512 * 1024 {
        return Err(invalid());
    }
    let repository = state.application.ports().graphics();
    let previous = if id == 0 {
        default_element()
    } else {
        repository.get_element(id).await?
    };
    let mut element = if intent == "import" {
        let selected = kind(fields.get("kind").ok_or_else(invalid)?).map_err(|_| invalid())?;
        let mut imported = repository
            .parse_yaml(selected, fields.get("yaml").ok_or_else(invalid)?)
            .await?;
        imported.path.clone_from(&previous.path);
        imported
    } else {
        from_fields(fields, previous).map_err(|_| invalid())?
    };
    element.id = id;
    if intent == "export" {
        let yaml = repository.export_yaml(&element).await?;
        return Ok((
            [
                (header::CONTENT_TYPE, "application/yaml; charset=utf-8"),
                (
                    header::CONTENT_DISPOSITION,
                    "attachment; filename=graphics-element.yml",
                ),
            ],
            yaml,
        )
            .into_response());
    }
    let number = |key: &str, default: &str| {
        fields
            .get(key)
            .filter(|v| !v.trim().is_empty())
            .map_or(default, String::as_str)
            .parse::<u32>()
            .map_err(|_| invalid())
    };
    let request = GraphicsRenderPreviewRequest {
        element,
        channel_id: fields
            .get("preview_channel")
            .filter(|v| !v.trim().is_empty())
            .map(|v| v.parse::<i64>().map_err(|_| invalid()))
            .transpose()?,
        render_time: fields
            .get("preview_time")
            .filter(|v| !v.trim().is_empty())
            .map(|value| {
                chrono::DateTime::parse_from_rfc3339(value)
                    .map(|time| time.with_timezone(&chrono::Utc))
                    .map_err(|_| invalid())
            })
            .transpose()?
            .unwrap_or_else(chrono::Utc::now),
        seek_seconds: fields
            .get("preview_seek")
            .filter(|v| !v.trim().is_empty())
            .map_or("0", String::as_str)
            .parse()
            .map_err(|_| invalid())?,
        width: number("preview_width", "1920")?,
        height: number("preview_height", "1080")?,
        variables: fields
            .get("preview_variables")
            .filter(|v| !v.trim().is_empty())
            .cloned(),
    };
    if intent == "preview" {
        let preview = repository.render_preview(&request).await?;
        return Ok(Html(format!(r#"<!doctype html><html><head><meta name="viewport" content="width=device-width"><title>{}</title></head><body><p data-graphics-render-time>{}</p><img style="max-width:100%;height:auto;background:repeating-conic-gradient(#bbb 0% 25%,#ddd 0% 50%) 0/16px 16px" alt="{}" src="data:image/png;base64,{}" data-graphics-render-output></body></html>"#,text(language,TextKey::GraphicsRealPreview),escape_html(&preview.local_time),text(language,TextKey::GraphicsRealPreview),STANDARD.encode(preview.png))).into_response());
    }
    if intent != "save" && intent != "import" {
        return Err(invalid());
    }
    repository.validate_authoring(&request).await?;
    let saved_id = if id == 0 {
        repository.create_element(&request.element).await?
    } else {
        repository.upsert_element(&request.element).await?;
        id
    };
    Ok(Redirect::to(&format!("/graphics-elements/{saved_id}?saved=1")).into_response())
}
