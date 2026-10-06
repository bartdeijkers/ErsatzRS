use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use ersatzrs_domain::{TextGraphicsConfig, TextGraphicsRenderer};

#[cfg(feature = "ssr")]
use ersatzrs_core::repositories::{GraphicsRepository, RepositoryError};
#[cfg(feature = "ssr")]
use ersatzrs_domain::{GraphicsElement, GraphicsElementConfig, GraphicsElementKind};

const TEXT_GRAPHICS_KIND: i32 = 1;
pub const MIN_REFRESH_INTERVAL_MILLISECONDS: u64 = 250;
pub const MAX_REFRESH_INTERVAL_MILLISECONDS: u64 = 60_000;
const MAX_NAME_CHARS: usize = 50;
const MAX_TEMPLATE_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GraphicsTextPreviewForm {
    pub channel_id: i64,
    pub element_id: i64,
    pub render_time: String,
    pub template: String,
    pub renderer: TextGraphicsRenderer,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GraphicsTextPreviewResult {
    pub text: String,
    pub local_time: String,
    pub programme_context: String,
}

#[server(prefix = "/admin-api")]
pub async fn preview_graphics_text(
    request: GraphicsTextPreviewForm,
) -> Result<GraphicsTextPreviewResult, ServerFnError> {
    use crate::host::state::AppState;
    use ersatzrs_core::repositories::GraphicsTextPreviewRequest;
    let state = expect_context::<AppState>();
    let render_time = chrono::DateTime::parse_from_rfc3339(&request.render_time)
        .map_err(|_| ServerFnError::new("graphics-preview-invalid-time"))?
        .with_timezone(&chrono::Utc);
    let request = GraphicsTextPreviewRequest {
        channel_id: request.channel_id,
        element_id: (request.element_id > 0).then_some(request.element_id),
        render_time,
        template: request.template,
        renderer: request.renderer,
        epg_entries: 32,
        width: request.width,
        height: request.height,
    };
    let preview = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        state.application.ports().graphics().preview_text(&request),
    )
    .await
    .map_err(|_| ServerFnError::new("graphics-preview-timeout"))?
    .map_err(ServerFnError::new)?;
    Ok(GraphicsTextPreviewResult {
        text: preview.text,
        local_time: preview.local_time,
        programme_context: serde_json::to_string_pretty(&preview.programme_context)
            .map_err(ServerFnError::new)?,
    })
}

/// A graphics element shown by the admin selector.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphicsElementListRow {
    pub id: i64,
    pub name: String,
    pub kind: i32,
    pub renderer: Option<TextGraphicsRenderer>,
}

/// Text compatibility fields plus the complete typed editor definition.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GraphicsElementForm {
    #[serde(default)]
    pub definition: Option<ersatzrs_domain::GraphicsElement>,
    pub id: i64,
    pub name: String,
    pub text: String,
    pub renderer: TextGraphicsRenderer,
    pub font_file: String,
    pub refresh_interval_milliseconds: u64,
    pub reload_interval_frames: u32,
    #[serde(default)]
    pub metadata_refresh_interval_seconds: u64,
}

impl Default for GraphicsElementForm {
    fn default() -> Self {
        let config = TextGraphicsConfig::default();
        Self {
            definition: None,
            id: 0,
            name: String::new(),
            text: String::new(),
            renderer: config.renderer,
            font_file: config.drawtext.font_file.unwrap_or_default(),
            refresh_interval_milliseconds: config.drawtext.refresh_interval_milliseconds,
            reload_interval_frames: config.drawtext.reload_interval_frames,
            metadata_refresh_interval_seconds: config
                .drawtext
                .metadata_refresh_interval_seconds
                .filter(|seconds| *seconds > 0)
                .map(|seconds| seconds.clamp(5, 60))
                .unwrap_or_default(),
        }
    }
}

#[server(prefix = "/admin-api")]
pub async fn list_graphics_element_rows() -> Result<Vec<GraphicsElementListRow>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let rows = state
        .application
        .ports()
        .graphics()
        .list_elements()
        .await
        .map_err(ServerFnError::new)?;

    Ok(rows.into_iter().map(list_row_from_element).collect())
}

#[server(prefix = "/admin-api")]
pub async fn get_graphics_element_form(
    id: i64,
) -> Result<Option<GraphicsElementForm>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    get_graphics_element_form_with_repository(state.application.ports().graphics(), id)
        .await
        .map_err(ServerFnError::new)
}

/// Saves the flat HTML form used by both hydrated and no-JavaScript clients.
///
/// The server redirects after every submission. Leptos turns the same response
/// into a client navigation for hydrated calls and a regular HTTP redirect for
/// progressive enhancement.
#[server(prefix = "/admin-api")]
#[allow(clippy::too_many_arguments)]
pub async fn save_graphics_element(
    id: i64,
    name: String,
    text: String,
    renderer: String,
    font_file: String,
    refresh_interval_milliseconds: u64,
    reload_interval_frames: u32,
    metadata_refresh_interval_seconds: Option<u64>,
) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let renderer = match renderer.as_str() {
        "drawvg" => TextGraphicsRenderer::Drawvg,
        "drawtext" => TextGraphicsRenderer::Drawtext,
        _ => {
            redirect_to_editor_error(id, GraphicsElementSaveError::InvalidRenderer.code());
            return Err(ServerFnError::new(
                GraphicsElementSaveError::InvalidRenderer.code(),
            ));
        }
    };
    let form = GraphicsElementForm {
        definition: None,
        id,
        name,
        text,
        renderer,
        font_file,
        refresh_interval_milliseconds,
        reload_interval_frames,
        metadata_refresh_interval_seconds: metadata_refresh_interval_seconds.unwrap_or_default(),
    };

    match save_graphics_element_with_repository(state.application.ports().graphics(), form).await {
        Ok(saved_id) => {
            leptos_axum::redirect(&format!("/graphics-elements/{saved_id}?saved=1"));
            Ok(saved_id)
        }
        Err(error) => {
            redirect_to_editor_error(id, error.code());
            Err(ServerFnError::new(error.code()))
        }
    }
}

#[cfg(feature = "ssr")]
fn redirect_to_editor_error(id: i64, code: &str) {
    let path = if id == 0 {
        format!("/graphics-elements/add?error={code}")
    } else {
        format!("/graphics-elements/{id}?error={code}")
    };
    leptos_axum::redirect(&path);
}

#[cfg(feature = "ssr")]
#[derive(Debug, thiserror::Error)]
pub enum GraphicsElementSaveError {
    #[error("name_required")]
    NameRequired,
    #[error("name_too_long")]
    NameTooLong,
    #[error("text_required")]
    TextRequired,
    #[error("text_too_long")]
    TextTooLong,
    #[error("invalid_renderer")]
    InvalidRenderer,
    #[error("refresh_interval_out_of_range")]
    RefreshIntervalOutOfRange,
    #[error("reload_interval_out_of_range")]
    ReloadIntervalOutOfRange,
    #[error("metadata_refresh_interval_out_of_range")]
    MetadataRefreshIntervalOutOfRange,
    #[error("graphics_element_not_found")]
    NotFound,
    #[error("graphics_element_not_text")]
    NotText,
    #[error("graphics_element_config_invalid")]
    InvalidConfig,
    #[error("storage_error")]
    Storage(#[source] RepositoryError),
}

#[cfg(feature = "ssr")]
impl GraphicsElementSaveError {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::NameRequired => "name_required",
            Self::NameTooLong => "name_too_long",
            Self::TextRequired => "text_required",
            Self::TextTooLong => "text_too_long",
            Self::InvalidRenderer => "invalid_renderer",
            Self::RefreshIntervalOutOfRange => "refresh_interval_out_of_range",
            Self::ReloadIntervalOutOfRange => "reload_interval_out_of_range",
            Self::MetadataRefreshIntervalOutOfRange => "metadata_refresh_interval_out_of_range",
            Self::NotFound => "graphics_element_not_found",
            Self::NotText => "graphics_element_not_text",
            Self::InvalidConfig => "graphics_element_config_invalid",
            Self::Storage(_) => "storage_error",
        }
    }
}

#[cfg(feature = "ssr")]
fn list_row_from_element(element: GraphicsElement) -> GraphicsElementListRow {
    let renderer = match element.config {
        GraphicsElementConfig::Text(config) => Some(config.renderer),
        _ => None,
    };
    GraphicsElementListRow {
        id: element.id,
        name: element.name,
        kind: graphics_kind_id(element.kind),
        renderer,
    }
}

#[cfg(feature = "ssr")]
const fn graphics_kind_id(kind: GraphicsElementKind) -> i32 {
    match kind {
        GraphicsElementKind::Image => 0,
        GraphicsElementKind::Text => TEXT_GRAPHICS_KIND,
        GraphicsElementKind::Subtitle => 2,
        GraphicsElementKind::Motion => 3,
        GraphicsElementKind::Script => 4,
    }
}

#[cfg(feature = "ssr")]
async fn get_graphics_element_form_with_repository(
    repository: &dyn GraphicsRepository,
    id: i64,
) -> Result<Option<GraphicsElementForm>, GraphicsElementSaveError> {
    let element = match repository.get_element(id).await {
        Ok(element) => element,
        Err(RepositoryError::NotFound { .. }) => return Ok(None),
        Err(RepositoryError::Decode { .. }) => return Err(GraphicsElementSaveError::InvalidConfig),
        Err(error) => return Err(GraphicsElementSaveError::Storage(error)),
    };
    let definition = Some(element.clone());
    let config = match &element.config {
        GraphicsElementConfig::Text(config) => config.clone(),
        _ => TextGraphicsConfig::default(),
    };

    Ok(Some(GraphicsElementForm {
        definition,
        id: element.id,
        name: element.name,
        text: config.text.unwrap_or_default(),
        renderer: config.renderer,
        font_file: config.drawtext.font_file.unwrap_or_default(),
        refresh_interval_milliseconds: config.drawtext.refresh_interval_milliseconds,
        reload_interval_frames: config.drawtext.reload_interval_frames,
        metadata_refresh_interval_seconds: config
            .drawtext
            .metadata_refresh_interval_seconds
            .filter(|seconds| *seconds > 0)
            .map(|seconds| seconds.clamp(5, 60))
            .unwrap_or_default(),
    }))
}

#[cfg(feature = "ssr")]
pub async fn save_graphics_element_with_repository(
    repository: &dyn GraphicsRepository,
    mut form: GraphicsElementForm,
) -> Result<i64, GraphicsElementSaveError> {
    let name = form.name.trim();
    if name.is_empty() {
        return Err(GraphicsElementSaveError::NameRequired);
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(GraphicsElementSaveError::NameTooLong);
    }
    let text = form.text.trim();
    if text.is_empty() {
        return Err(GraphicsElementSaveError::TextRequired);
    }
    if form.text.len() > MAX_TEMPLATE_BYTES || form.text.contains('\0') {
        return Err(GraphicsElementSaveError::TextTooLong);
    }
    if !(MIN_REFRESH_INTERVAL_MILLISECONDS..=MAX_REFRESH_INTERVAL_MILLISECONDS)
        .contains(&form.refresh_interval_milliseconds)
    {
        return Err(GraphicsElementSaveError::RefreshIntervalOutOfRange);
    }
    if form.reload_interval_frames == 0 {
        return Err(GraphicsElementSaveError::ReloadIntervalOutOfRange);
    }
    if form.metadata_refresh_interval_seconds != 0
        && !(5..=60).contains(&form.metadata_refresh_interval_seconds)
    {
        return Err(GraphicsElementSaveError::MetadataRefreshIntervalOutOfRange);
    }

    form.name = name.to_owned();
    let font_file = (!form.font_file.trim().is_empty()).then(|| form.font_file.trim().to_owned());

    if form.id == 0 {
        let config = TextGraphicsConfig {
            text: Some(form.text),
            renderer: form.renderer,
            drawtext: ersatzrs_domain::DrawtextGraphicsConfig {
                font_file,
                refresh_interval_milliseconds: form.refresh_interval_milliseconds,
                reload_interval_frames: form.reload_interval_frames,
                metadata_refresh_interval_seconds: (form.metadata_refresh_interval_seconds > 0)
                    .then_some(form.metadata_refresh_interval_seconds),
            },
            ..TextGraphicsConfig::default()
        };
        let element = GraphicsElement {
            name: form.name,
            kind: GraphicsElementKind::Text,
            config: GraphicsElementConfig::Text(config),
            ..GraphicsElement::default()
        };
        return repository
            .create_element(&element)
            .await
            .map_err(GraphicsElementSaveError::Storage);
    }

    let mut element = repository
        .get_element(form.id)
        .await
        .map_err(|error| match error {
            RepositoryError::NotFound { .. } => GraphicsElementSaveError::NotFound,
            RepositoryError::Decode { .. } => GraphicsElementSaveError::InvalidConfig,
            error => GraphicsElementSaveError::Storage(error),
        })?;
    if element.kind != GraphicsElementKind::Text {
        return Err(GraphicsElementSaveError::NotText);
    }
    let GraphicsElementConfig::Text(mut config) = element.config else {
        return Err(GraphicsElementSaveError::InvalidConfig);
    };
    config.text = Some(form.text);
    config.renderer = form.renderer;
    config.drawtext.font_file = font_file;
    config.drawtext.refresh_interval_milliseconds = form.refresh_interval_milliseconds;
    config.drawtext.reload_interval_frames = form.reload_interval_frames;
    config.drawtext.metadata_refresh_interval_seconds = (form.metadata_refresh_interval_seconds
        > 0)
    .then_some(form.metadata_refresh_interval_seconds);
    element.name = form.name;
    element.config = GraphicsElementConfig::Text(config);
    repository
        .upsert_element(&element)
        .await
        .map_err(GraphicsElementSaveError::Storage)?;
    Ok(form.id)
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;
    use ersatzrs_domain::{TextAlignment, TextFit};
    use sqlx::Row;

    async fn fresh_pool() -> ersatzrs_infra::db::DbPool {
        let dir = tempfile::tempdir().expect("temp db dir");
        let database_url = format!(
            "sqlite:{}",
            dir.path().join("graphics-elements.db").display()
        );
        std::mem::forget(dir);
        ersatzrs_infra::db::init(&database_url)
            .await
            .expect("initialize test database")
    }

    fn repository(
        pool: &ersatzrs_infra::db::DbPool,
    ) -> ersatzrs_infra::repositories::SqliteGraphicsRepository {
        ersatzrs_infra::repositories::SqliteGraphicsRepository(pool.clone())
    }

    fn drawtext_form(name: &str) -> GraphicsElementForm {
        GraphicsElementForm {
            definition: None,
            name: name.to_owned(),
            text: "Now: {{ title }}".to_owned(),
            renderer: TextGraphicsRenderer::Drawtext,
            font_file: String::new(),
            refresh_interval_milliseconds: 1_000,
            reload_interval_frames: 2,
            ..GraphicsElementForm::default()
        }
    }

    #[tokio::test]
    async fn creates_and_reads_drawtext_element_with_packaged_font_fallback() {
        let pool = fresh_pool().await;
        let id =
            save_graphics_element_with_repository(&repository(&pool), drawtext_form("Now playing"))
                .await
                .expect("create text graphics element");

        let saved = get_graphics_element_form_with_repository(&repository(&pool), id)
            .await
            .expect("load element")
            .expect("element exists");
        assert_eq!(saved.name, "Now playing");
        assert_eq!(saved.renderer, TextGraphicsRenderer::Drawtext);
        assert_eq!(saved.font_file, "");
        assert_eq!(saved.reload_interval_frames, 2);
    }

    #[tokio::test]
    async fn editing_preserves_unexposed_text_config_and_shared_columns() {
        let pool = fresh_pool().await;
        let original = TextGraphicsConfig {
            text: Some("Old".to_owned()),
            fit: TextFit::Wrap,
            align: TextAlignment::Center,
            width_percent: Some(72.5),
            base_style: Some("headline".to_owned()),
            ..TextGraphicsConfig::default()
        };
        let encoded =
            serde_json::to_string(&GraphicsElementConfig::Text(original)).expect("encode fixture");
        sqlx::query(
            r#"INSERT INTO "GraphicsElement"
               ("Id", "Name", "Kind", "Path", "ZIndex", "Config")
               VALUES (41, 'Imported', 1, 'text/imported.yml', 9, ?)"#,
        )
        .bind(encoded)
        .execute(&pool)
        .await
        .expect("insert fixture");

        let mut form = drawtext_form("Renamed");
        form.id = 41;
        form.font_file = " /fonts/custom.ttf ".to_owned();
        save_graphics_element_with_repository(&repository(&pool), form)
            .await
            .expect("update element");

        let row = sqlx::query(
            r#"SELECT "Name", "Kind", "Path", "ZIndex", "Config"
               FROM "GraphicsElement" WHERE "Id" = 41"#,
        )
        .fetch_one(&pool)
        .await
        .expect("fetch updated row");
        assert_eq!(row.try_get::<String, _>("Name").unwrap(), "Renamed");
        assert_eq!(row.try_get::<i32, _>("Kind").unwrap(), 1);
        assert_eq!(
            row.try_get::<String, _>("Path").unwrap(),
            "text/imported.yml"
        );
        assert_eq!(row.try_get::<i32, _>("ZIndex").unwrap(), 9);
        let config: GraphicsElementConfig =
            serde_json::from_str(&row.try_get::<String, _>("Config").unwrap())
                .expect("decode updated config");
        let GraphicsElementConfig::Text(config) = config else {
            panic!("expected text config");
        };
        assert_eq!(config.fit, TextFit::Wrap);
        assert_eq!(config.align, TextAlignment::Center);
        assert_eq!(config.width_percent, Some(72.5));
        assert_eq!(config.base_style.as_deref(), Some("headline"));
        assert_eq!(
            config.drawtext.font_file.as_deref(),
            Some("/fonts/custom.ttf")
        );
    }

    #[tokio::test]
    async fn refuses_non_text_and_malformed_config_without_mutating_rows() {
        let pool = fresh_pool().await;
        sqlx::query(
            r#"INSERT INTO "GraphicsElement" ("Id", "Name", "Kind", "Config")
               VALUES (51, 'Image', 0, '{"kind":"image"}'),
                      (52, 'Broken', 1, '{broken')"#,
        )
        .execute(&pool)
        .await
        .expect("insert fixtures");

        for (id, expected) in [
            (51, "graphics_element_not_text"),
            (52, "graphics_element_config_invalid"),
        ] {
            let mut form = drawtext_form("Do not save");
            form.id = id;
            let error = save_graphics_element_with_repository(&repository(&pool), form)
                .await
                .expect_err("reject unsafe update");
            assert_eq!(error.code(), expected);
        }

        let names: Vec<String> = sqlx::query_scalar(
            r#"SELECT "Name" FROM "GraphicsElement" WHERE "Id" IN (51, 52) ORDER BY "Id""#,
        )
        .fetch_all(&pool)
        .await
        .expect("fetch unchanged names");
        assert_eq!(names, ["Image", "Broken"]);
    }

    #[tokio::test]
    async fn null_config_defaults_existing_text_elements_to_drawvg() {
        let pool = fresh_pool().await;
        sqlx::query(
            r#"INSERT INTO "GraphicsElement" ("Id", "Name", "Kind", "Config")
               VALUES (61, 'Legacy', 1, NULL)"#,
        )
        .execute(&pool)
        .await
        .expect("insert legacy fixture");

        let form = get_graphics_element_form_with_repository(&repository(&pool), 61)
            .await
            .expect("decode legacy element")
            .expect("legacy element exists");
        assert_eq!(form.renderer, TextGraphicsRenderer::Drawvg);
        assert_eq!(form.refresh_interval_milliseconds, 1_000);
        assert_eq!(form.reload_interval_frames, 1);
    }

    #[tokio::test]
    async fn validates_required_fields_and_runtime_bounds() {
        let pool = fresh_pool().await;
        let cases = [
            (GraphicsElementForm::default(), "name_required"),
            (
                GraphicsElementForm {
                    definition: None,
                    name: "Text".to_owned(),
                    ..GraphicsElementForm::default()
                },
                "text_required",
            ),
            (
                GraphicsElementForm {
                    definition: None,
                    refresh_interval_milliseconds: 249,
                    ..drawtext_form("Fast")
                },
                "refresh_interval_out_of_range",
            ),
            (
                GraphicsElementForm {
                    definition: None,
                    refresh_interval_milliseconds: 60_001,
                    ..drawtext_form("Slow")
                },
                "refresh_interval_out_of_range",
            ),
            (
                GraphicsElementForm {
                    definition: None,
                    reload_interval_frames: 0,
                    ..drawtext_form("Reload")
                },
                "reload_interval_out_of_range",
            ),
        ];

        for (form, expected) in cases {
            let error = save_graphics_element_with_repository(&repository(&pool), form)
                .await
                .expect_err("reject invalid form");
            assert_eq!(error.code(), expected);
        }
    }
}
