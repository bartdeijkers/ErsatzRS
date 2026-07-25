use leptos::prelude::*;
use serde::{Deserialize, Serialize};

/// Lightweight row for the watermark list table.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct WatermarkListRow {
    pub id: i64,
    pub name: String,
    pub mode_id: i32,
    pub location_id: i32,
    pub image_path: Option<String>,
    pub image_source: i32,
}

/// Full form data for create / edit — mirrors `ChannelWatermark` domain defaults.
///
/// Enum columns use the same integer values as `ErsatzTV`'s C# enums:
/// - `ChannelWatermarkMode`: None=0, Permanent=1, Intermittent=2, OpacityExpression=3
/// - `ChannelWatermarkImageSource`: Custom=0, ChannelLogo=1, Resource=100
/// - `WatermarkLocation`: BottomRight=0, BottomLeft=1, TopRight=2, TopLeft=3,
///   TopMiddle=4, RightMiddle=5, BottomMiddle=6, LeftMiddle=7, MiddleCenter=8
/// - `WatermarkSize`: Scaled=0, ActualSize=1
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WatermarkForm {
    pub id: i64,
    pub name: String,
    pub mode: i32,
    pub image_source: i32,
    pub image: Option<String>,
    pub original_content_type: Option<String>,
    pub location: i32,
    pub size: i32,
    pub width_percent: f64,
    pub horizontal_margin_percent: f64,
    pub vertical_margin_percent: f64,
    pub frequency_minutes: i32,
    pub duration_seconds: i32,
    pub opacity: i32,
    pub place_within_source_content: bool,
    pub opacity_expression: String,
    pub z_index: i32,
}

/// One adjacent watermark in deterministic editor comparison order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatermarkNavigationItem {
    pub id: i64,
    pub name: String,
}

/// Watermark editor form plus explicit comparison boundaries.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WatermarkEditorData {
    pub form: WatermarkForm,
    pub previous: Option<WatermarkNavigationItem>,
    pub next: Option<WatermarkNavigationItem>,
}

impl Default for WatermarkForm {
    fn default() -> Self {
        Self {
            id: 0,
            name: String::new(),
            mode: 1,
            image_source: 0,
            image: None,
            original_content_type: None,
            location: 0,
            size: 0,
            width_percent: 15.0,
            horizontal_margin_percent: 5.0,
            vertical_margin_percent: 5.0,
            frequency_minutes: 15,
            duration_seconds: 15,
            opacity: 100,
            place_within_source_content: false,
            opacity_expression: String::new(),
            z_index: 0,
        }
    }
}

#[cfg(feature = "ssr")]
pub(crate) fn watermark_form_into_admin(
    form: WatermarkForm,
) -> ersatzrs_app::commands::WatermarkAdminForm {
    ersatzrs_app::commands::WatermarkAdminForm {
        id: form.id,
        name: form.name,
        mode: form.mode,
        image_source: form.image_source,
        image: form.image,
        original_content_type: form.original_content_type,
        location: form.location,
        size: form.size,
        width_percent: form.width_percent,
        horizontal_margin_percent: form.horizontal_margin_percent,
        vertical_margin_percent: form.vertical_margin_percent,
        frequency_minutes: form.frequency_minutes,
        duration_seconds: form.duration_seconds,
        opacity: form.opacity,
        place_within_source_content: form.place_within_source_content,
        opacity_expression: form.opacity_expression,
        z_index: form.z_index,
    }
}

#[cfg(feature = "ssr")]
fn watermark_form_from_admin(form: ersatzrs_app::commands::WatermarkAdminForm) -> WatermarkForm {
    WatermarkForm {
        id: form.id,
        name: form.name,
        mode: form.mode,
        image_source: form.image_source,
        image: form.image,
        original_content_type: form.original_content_type,
        location: form.location,
        size: form.size,
        width_percent: form.width_percent,
        horizontal_margin_percent: form.horizontal_margin_percent,
        vertical_margin_percent: form.vertical_margin_percent,
        frequency_minutes: form.frequency_minutes,
        duration_seconds: form.duration_seconds,
        opacity: form.opacity,
        place_within_source_content: form.place_within_source_content,
        opacity_expression: form.opacity_expression,
        z_index: form.z_index,
    }
}

#[cfg(feature = "ssr")]
fn watermark_navigation_from_admin(
    item: ersatzrs_app::queries::WatermarkNavigationRead,
) -> WatermarkNavigationItem {
    WatermarkNavigationItem {
        id: item.id,
        name: item.name,
    }
}

#[cfg(feature = "ssr")]
fn watermark_editor_from_admin(
    editor: ersatzrs_app::queries::WatermarkEditorRead,
) -> WatermarkEditorData {
    WatermarkEditorData {
        form: watermark_form_from_admin(editor.form),
        previous: editor.previous.map(watermark_navigation_from_admin),
        next: editor.next.map(watermark_navigation_from_admin),
    }
}

/// Parameters for the `drawvg`-based placement preview.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WatermarkPreviewParams {
    pub location: i32,
    pub width_pct: f64,
    pub h_margin: f64,
    pub v_margin: f64,
    pub mode: i32,
    pub size: i32,
    pub opacity: i32,
}

/// Selected custom asset rendered by the truthful placement preview.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WatermarkAssetPreviewParams {
    pub placement: WatermarkPreviewParams,
    pub image_path: String,
}

/// Metadata for the real-asset placement preview.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WatermarkAssetPreview {
    pub data_url: String,
    pub asset_width: u32,
    pub asset_height: u32,
    pub rendered_width: u32,
    pub rendered_height: u32,
}

#[server]
pub async fn list_watermark_rows(
    page: u32,
    page_size: u32,
) -> Result<Vec<WatermarkListRow>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .list_watermarks(page, page_size)
        .await
        .map(|rows| {
            rows.into_iter()
                .map(|row| WatermarkListRow {
                    id: row.id,
                    name: row.name,
                    mode_id: row.mode,
                    location_id: row.location,
                    image_path: row.image,
                    image_source: row.image_source,
                })
                .collect()
        })
        .map_err(ServerFnError::new)
}

#[server]
pub async fn count_watermarks() -> Result<u64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .count_watermarks()
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn get_watermark_form(id: i64) -> Result<WatermarkForm, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .get_watermark(id)
        .await
        .map_err(ServerFnError::new)?
        .map(watermark_form_from_admin)
        .ok_or_else(|| ServerFnError::new("Watermark not found"))
}

#[server]
pub async fn get_watermark_editor_data(id: i64) -> Result<WatermarkEditorData, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .get_watermark_editor(id)
        .await
        .map_err(ServerFnError::new)?
        .map(watermark_editor_from_admin)
        .ok_or_else(|| ServerFnError::new("Watermark not found"))
}

#[server]
pub async fn save_watermark(form: WatermarkForm) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .save_watermark(ersatzrs_app::commands::SaveWatermark {
            form: watermark_form_into_admin(form),
        })
        .await
        .map_err(ServerFnError::new)
}

#[cfg(all(test, feature = "ssr"))]
#[derive(Debug, thiserror::Error)]
pub enum WatermarkSaveError {
    #[error("{0}")]
    Validation(&'static str),
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
}

#[cfg(all(test, feature = "ssr"))]
#[allow(clippy::too_many_lines)]
pub async fn save_watermark_with_pool(
    pool: &ersatzrs_infra::db::DbPool,
    form: WatermarkForm,
) -> Result<i64, WatermarkSaveError> {
    let name = form.name.trim().to_string();
    if name.is_empty() {
        return Err(WatermarkSaveError::Validation("Name is required"));
    }
    if name.len() > 50 {
        return Err(WatermarkSaveError::Validation(
            "Name must be 50 characters or fewer",
        ));
    }

    // Validate custom image is present when ImageSource == Custom
    if form.image_source == 0 && form.image.as_deref().unwrap_or("").trim().is_empty() {
        return Err(WatermarkSaveError::Validation(
            "Watermark image is required",
        ));
    }

    if form.mode != 0 {
        // Width: required to be in (0, 100] when mode != None and size == Scaled
        if form.size == 0 && (form.width_percent <= 0.0 || form.width_percent > 100.0) {
            return Err(WatermarkSaveError::Validation(
                "Width must be between 0 and 100%",
            ));
        }
        // Margins: [0, 50]
        if form.horizontal_margin_percent < 0.0 || form.horizontal_margin_percent > 50.0 {
            return Err(WatermarkSaveError::Validation(
                "Horizontal margin must be between 0 and 50%",
            ));
        }
        if form.vertical_margin_percent < 0.0 || form.vertical_margin_percent > 50.0 {
            return Err(WatermarkSaveError::Validation(
                "Vertical margin must be between 0 and 50%",
            ));
        }
        // Duration must be positive and less than frequency window
        if form.duration_seconds <= 0 || form.duration_seconds >= form.frequency_minutes * 60 {
            return Err(WatermarkSaveError::Validation(
                "Duration must be positive and less than frequency window",
            ));
        }
        // Opacity: (0, 100]
        if form.opacity <= 0 || form.opacity > 100 {
            return Err(WatermarkSaveError::Validation(
                "Opacity must be between 1 and 100",
            ));
        }
    }

    let opacity_expr: Option<String> = if form.mode == 3 && !form.opacity_expression.is_empty() {
        Some(form.opacity_expression.clone())
    } else {
        None
    };

    let (image, content_type): (Option<String>, Option<String>) = if form.image_source == 0 {
        (form.image.clone(), form.original_content_type.clone())
    } else {
        (None, None)
    };

    let place_within = i64::from(form.place_within_source_content);

    if form.id == 0 {
        let exists: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "ChannelWatermark" WHERE "Name" = ?"#)
                .bind(&name)
                .fetch_one(pool)
                .await
                .unwrap_or(0);
        if exists > 0 {
            return Err(WatermarkSaveError::Validation(
                "Watermark name must be unique",
            ));
        }

        let result = sqlx::query(
            r#"INSERT INTO "ChannelWatermark"
               ("Name","Mode","ImageSource","Image","OriginalContentType","Location","Size",
                "WidthPercent","HorizontalMarginPercent","VerticalMarginPercent",
                "FrequencyMinutes","DurationSeconds","Opacity","PlaceWithinSourceContent",
                "OpacityExpression","ZIndex")
               VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)"#,
        )
        .bind(&name)
        .bind(i64::from(form.mode))
        .bind(i64::from(form.image_source))
        .bind(&image)
        .bind(&content_type)
        .bind(i64::from(form.location))
        .bind(i64::from(form.size))
        .bind(form.width_percent)
        .bind(form.horizontal_margin_percent)
        .bind(form.vertical_margin_percent)
        .bind(i64::from(form.frequency_minutes))
        .bind(i64::from(form.duration_seconds))
        .bind(i64::from(form.opacity))
        .bind(place_within)
        .bind(&opacity_expr)
        .bind(i64::from(form.z_index))
        .execute(pool)
        .await?;
        let id = ersatzrs_infra::db::last_insert_id(result);

        Ok(id)
    } else {
        let exists: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "ChannelWatermark" WHERE "Name" = ? AND "Id" != ?"#,
        )
        .bind(&name)
        .bind(form.id)
        .fetch_one(pool)
        .await
        .unwrap_or(0);
        if exists > 0 {
            return Err(WatermarkSaveError::Validation(
                "Watermark name must be unique",
            ));
        }

        let rows = sqlx::query(
            r#"UPDATE "ChannelWatermark" SET
               "Name"=?,"Mode"=?,"ImageSource"=?,"Image"=?,"OriginalContentType"=?,
               "Location"=?,"Size"=?,"WidthPercent"=?,"HorizontalMarginPercent"=?,
               "VerticalMarginPercent"=?,"FrequencyMinutes"=?,"DurationSeconds"=?,
               "Opacity"=?,"PlaceWithinSourceContent"=?,"OpacityExpression"=?,"ZIndex"=?
               WHERE "Id"=?"#,
        )
        .bind(&name)
        .bind(i64::from(form.mode))
        .bind(i64::from(form.image_source))
        .bind(&image)
        .bind(&content_type)
        .bind(i64::from(form.location))
        .bind(i64::from(form.size))
        .bind(form.width_percent)
        .bind(form.horizontal_margin_percent)
        .bind(form.vertical_margin_percent)
        .bind(i64::from(form.frequency_minutes))
        .bind(i64::from(form.duration_seconds))
        .bind(i64::from(form.opacity))
        .bind(place_within)
        .bind(&opacity_expr)
        .bind(i64::from(form.z_index))
        .bind(form.id)
        .execute(pool)
        .await?
        .rows_affected();

        if rows == 0 {
            return Err(WatermarkSaveError::Validation("Watermark not found"));
        }
        Ok(form.id)
    }
}

#[server]
pub async fn delete_watermark(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .delete_watermark(ersatzrs_app::commands::DeleteWatermark { id })
        .await
        .map_err(ServerFnError::new)
}

#[server]
pub async fn copy_watermark(id: i64, new_name: String) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .copy_watermark(ersatzrs_app::commands::CopyWatermark { id, name: new_name })
        .await
        .map_err(ServerFnError::new)
}

/// Generates the legacy schematic placement preview.
///
/// The editor uses [`preview_watermark_asset_placement`] so the visible
/// preview contains the selected image and can represent `ActualSize`
/// truthfully. This server function remains available for matched older
/// clients, but no longer claims a fixed-height box is the asset's real size.
#[server]
pub async fn preview_watermark_placement(
    params: WatermarkPreviewParams,
) -> Result<String, ServerFnError> {
    use base64::Engine;
    use std::io::Write;
    use tokio::process::Command;

    const W: f64 = 320.0;
    const H: f64 = 180.0;

    if params.mode == 0 {
        return Ok(String::new());
    }

    let wm_w = W * params.width_pct / 100.0;
    let wm_h = (wm_w * 0.5).max(8.0);
    let h_px = W * params.h_margin / 100.0;
    let v_px = H * params.v_margin / 100.0;
    let cx = (W - wm_w) / 2.0;
    let cy = (H - wm_h) / 2.0;

    let (rx, ry) = match params.location {
        0 => (W - wm_w - h_px, H - wm_h - v_px), // BottomRight
        1 => (h_px, H - wm_h - v_px),            // BottomLeft
        2 => (W - wm_w - h_px, v_px),            // TopRight
        3 => (h_px, v_px),                       // TopLeft
        4 => (cx, v_px),                         // TopMiddle
        5 => (W - wm_w - h_px, cy),              // RightMiddle
        6 => (cx, H - wm_h - v_px),              // BottomMiddle
        7 => (h_px, cy),                         // LeftMiddle
        _ => (cx, cy),                           // MiddleCenter
    };

    // Clamp to canvas bounds
    let rx = rx.clamp(0.0, W - wm_w.max(1.0));
    let ry = ry.clamp(0.0, H - wm_h.max(1.0));
    let opacity = (f64::from(params.opacity) / 100.0).clamp(0.0, 1.0);

    let drawvg = format!(
        "\
setcolor #1a1a1a
rect 0 0 {W} {H} fill
setcolor #4488ff@{opacity:.2}
rect {rx:.1} {ry:.1} {wm_w:.1} {wm_h:.1} fill
"
    );

    let mut tmp = tempfile::NamedTempFile::new()
        .map_err(|e| ServerFnError::new(format!("temp file: {e}")))?;
    tmp.write_all(drawvg.as_bytes())
        .map_err(|e| ServerFnError::new(format!("write drawvg: {e}")))?;
    let script_dir = tmp
        .path()
        .parent()
        .ok_or_else(|| ServerFnError::new("drawvg temp file has no parent directory"))?
        .to_path_buf();
    let script_name = tmp
        .path()
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| ServerFnError::new("drawvg temp file name is not valid UTF-8"))?
        .to_owned();

    let out = Command::new(ersatzrs_ffmpeg::ffmpeg_path())
        .current_dir(script_dir)
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            &format!("color=c=0x1a1a1a:s={W}x{H}:r=1"),
            "-vframes",
            "1",
            "-vf",
            &format!("drawvg=file={script_name}"),
            "-f",
            "image2pipe",
            "-c:v",
            "png",
            "pipe:1",
        ])
        .output()
        .await
        .map_err(|e| ServerFnError::new(format!("ffmpeg spawn: {e}")))?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(ServerFnError::new(format!("ffmpeg: {stderr}")));
    }

    let encoded = base64::engine::general_purpose::STANDARD.encode(&out.stdout);
    Ok(format!("data:image/png;base64,{encoded}"))
}

const PREVIEW_WIDTH: f64 = 320.0;
const PREVIEW_HEIGHT: f64 = 180.0;
const REFERENCE_WIDTH: f64 = 1920.0;
const REFERENCE_HEIGHT: f64 = 1080.0;

#[derive(Clone, Copy, Debug, PartialEq)]
struct PreviewLayout {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

fn preview_asset_size(
    params: &WatermarkPreviewParams,
    asset_width: u32,
    asset_height: u32,
) -> Result<(f64, f64), &'static str> {
    if asset_width == 0 || asset_height == 0 {
        return Err("watermark image dimensions must be greater than zero");
    }

    if params.size == 1 {
        return Ok((
            f64::from(asset_width) * PREVIEW_WIDTH / REFERENCE_WIDTH,
            f64::from(asset_height) * PREVIEW_HEIGHT / REFERENCE_HEIGHT,
        ));
    }

    let width = (PREVIEW_WIDTH * params.width_pct / 100.0).max(1.0);
    let height = width * f64::from(asset_height) / f64::from(asset_width);
    Ok((width, height.max(1.0)))
}

fn preview_layout(params: &WatermarkPreviewParams, width: f64, height: f64) -> PreviewLayout {
    let horizontal_margin = PREVIEW_WIDTH * params.h_margin / 100.0;
    let vertical_margin = PREVIEW_HEIGHT * params.v_margin / 100.0;
    let center_x = (PREVIEW_WIDTH - width) / 2.0;
    let center_y = (PREVIEW_HEIGHT - height) / 2.0;
    let (x, y) = match params.location {
        0 => (
            PREVIEW_WIDTH - width - horizontal_margin,
            PREVIEW_HEIGHT - height - vertical_margin,
        ),
        1 => (horizontal_margin, PREVIEW_HEIGHT - height - vertical_margin),
        2 => (PREVIEW_WIDTH - width - horizontal_margin, vertical_margin),
        3 => (horizontal_margin, vertical_margin),
        4 => (center_x, vertical_margin),
        5 => (PREVIEW_WIDTH - width - horizontal_margin, center_y),
        6 => (center_x, PREVIEW_HEIGHT - height - vertical_margin),
        7 => (horizontal_margin, center_y),
        _ => (center_x, center_y),
    };

    PreviewLayout {
        x: x.clamp(0.0, (PREVIEW_WIDTH - width).max(0.0)),
        y: y.clamp(0.0, (PREVIEW_HEIGHT - height).max(0.0)),
        width,
        height,
    }
}

fn rounded_render_dimension(value: f64) -> u32 {
    let bounded = value.round().clamp(1.0, f64::from(u32::MAX));
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        bounded as u32
    }
}

#[cfg(feature = "ssr")]
fn watermark_asset_path(image_path: &str) -> Result<std::path::PathBuf, ServerFnError> {
    use std::path::{Component, Path};

    let path = Path::new(image_path);
    let mut components = path.components();
    let Some(Component::Normal(file_name)) = components.next() else {
        return Err(ServerFnError::new("Watermark image path is invalid"));
    };
    if components.next().is_some() {
        return Err(ServerFnError::new("Watermark image path is invalid"));
    }

    Ok(ersatzrs_infra::file_system_layout::data_path("watermarks").join(file_name))
}

#[cfg(feature = "ssr")]
fn parse_image_dimensions(output: &str) -> Result<(u32, u32), ServerFnError> {
    let dimensions = output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .ok_or_else(|| ServerFnError::new("FFprobe returned no watermark dimensions"))?;
    let (width, height) = dimensions
        .split_once('x')
        .ok_or_else(|| ServerFnError::new("FFprobe returned invalid watermark dimensions"))?;
    let width = width
        .parse::<u32>()
        .map_err(|_| ServerFnError::new("FFprobe returned invalid watermark width"))?;
    let height = height
        .parse::<u32>()
        .map_err(|_| ServerFnError::new("FFprobe returned invalid watermark height"))?;
    if width == 0 || height == 0 {
        return Err(ServerFnError::new(
            "Watermark image dimensions must be greater than zero",
        ));
    }
    Ok((width, height))
}

#[cfg(feature = "ssr")]
async fn probe_image_dimensions(path: &std::path::Path) -> Result<(u32, u32), ServerFnError> {
    let output = tokio::process::Command::new(ersatzrs_ffmpeg::ffprobe_path())
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
            "-of",
            "csv=s=x:p=0",
        ])
        .arg(path)
        .output()
        .await
        .map_err(|error| ServerFnError::new(format!("ffprobe spawn: {error}")))?;
    if !output.status.success() {
        return Err(ServerFnError::new(format!(
            "ffprobe: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    parse_image_dimensions(&String::from_utf8_lossy(&output.stdout))
}

/// Render the selected custom watermark into a 1920×1080 reference frame.
///
/// The returned 320×180 PNG is exactly one sixth of that reference in each
/// dimension. `ActualSize` therefore uses the selected asset's native pixel
/// dimensions at a truthful 1:6 scale; `Scaled` uses the configured percentage
/// of the reference output width while preserving the asset aspect ratio.
#[server]
pub async fn preview_watermark_asset_placement(
    params: WatermarkAssetPreviewParams,
) -> Result<WatermarkAssetPreview, ServerFnError> {
    use base64::Engine;
    use std::io::Write;
    use tokio::process::Command;

    if params.placement.mode == 0 {
        return Err(ServerFnError::new(
            "Watermark preview is unavailable while mode is None",
        ));
    }

    let image_path = watermark_asset_path(&params.image_path)?;
    let (asset_width, asset_height) = probe_image_dimensions(&image_path).await?;
    let (width, height) = preview_asset_size(&params.placement, asset_width, asset_height)
        .map_err(ServerFnError::new)?;
    let layout = preview_layout(&params.placement, width, height);
    let rendered_width = rounded_render_dimension(layout.width);
    let rendered_height = rounded_render_dimension(layout.height);
    let opacity = (f64::from(params.placement.opacity) / 100.0).clamp(0.0, 1.0);

    // A 4:3 frame inside 320x180 is exactly 240x180, centered at x=40.
    // The outer 16:9 edge is intentionally left unmarked so the preview has
    // one unambiguous positioning guide.
    let drawvg = "\
setcolor #111827
rect 0 0 320 180 fill
setcolor #f59e0b@0.90
rect 40 0 2 180 fill
rect 278 0 2 180 fill
rect 40 0 240 2 fill
rect 40 178 240 2 fill
";
    let mut script = tempfile::NamedTempFile::new()
        .map_err(|error| ServerFnError::new(format!("temp file: {error}")))?;
    script
        .write_all(drawvg.as_bytes())
        .map_err(|error| ServerFnError::new(format!("write drawvg: {error}")))?;
    let script_dir = script
        .path()
        .parent()
        .ok_or_else(|| ServerFnError::new("drawvg temp file has no parent directory"))?
        .to_path_buf();
    let script_name = script
        .path()
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| ServerFnError::new("drawvg temp file name is not valid UTF-8"))?
        .to_owned();
    let filter = format!(
        "[0:v]drawvg=file={script_name}[frame];\
         [1:v]scale={rendered_width}:{rendered_height},format=rgba,\
         colorchannelmixer=aa={opacity:.2}[watermark];\
         [frame][watermark]overlay=x={x:.1}:y={y:.1}:format=auto,format=rgba[out]",
        x = layout.x,
        y = layout.y,
    );

    let output = Command::new(ersatzrs_ffmpeg::ffmpeg_path())
        .current_dir(script_dir)
        .args(["-y", "-f", "lavfi", "-i", "color=c=0x111827:s=320x180:r=1"])
        .arg("-i")
        .arg(&image_path)
        .args([
            "-filter_complex",
            &filter,
            "-map",
            "[out]",
            "-frames:v",
            "1",
            "-f",
            "image2pipe",
            "-c:v",
            "png",
            "pipe:1",
        ])
        .output()
        .await
        .map_err(|error| ServerFnError::new(format!("ffmpeg spawn: {error}")))?;
    if !output.status.success() {
        return Err(ServerFnError::new(format!(
            "ffmpeg: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }

    let encoded = base64::engine::general_purpose::STANDARD.encode(&output.stdout);
    Ok(WatermarkAssetPreview {
        data_url: format!("data:image/png;base64,{encoded}"),
        asset_width,
        asset_height,
        rendered_width,
        rendered_height,
    })
}

#[cfg(all(test, feature = "ssr", not(feature = "mysql")))]
mod tests {
    use super::*;
    use sqlx::Row;

    async fn fresh_pool() -> ersatzrs_infra::db::DbPool {
        let dir = tempfile::tempdir().expect("temp db dir");
        let database_url = format!("sqlite:{}", dir.path().join("watermarks.db").display());
        std::mem::forget(dir);
        ersatzrs_infra::db::init(&database_url)
            .await
            .expect("initialize test database")
    }

    fn custom_form(name: &str) -> WatermarkForm {
        WatermarkForm {
            name: name.to_owned(),
            image: Some("bug.png".to_owned()),
            original_content_type: Some("image/png".to_owned()),
            ..WatermarkForm::default()
        }
    }

    #[test]
    fn actual_size_preview_uses_native_asset_dimensions_at_reference_scale() {
        let params = WatermarkPreviewParams {
            location: 0,
            width_pct: 15.0,
            h_margin: 5.0,
            v_margin: 5.0,
            mode: 1,
            size: 1,
            opacity: 100,
        };

        let (width, height) =
            preview_asset_size(&params, 600, 300).expect("valid asset dimensions");
        let layout = preview_layout(&params, width, height);

        assert_eq!((width, height), (100.0, 50.0));
        assert_eq!(
            layout,
            PreviewLayout {
                x: 204.0,
                y: 121.0,
                width: 100.0,
                height: 50.0,
            }
        );
    }

    #[test]
    fn scaled_preview_preserves_asset_aspect_ratio_and_placement() {
        let params = WatermarkPreviewParams {
            location: 3,
            width_pct: 20.0,
            h_margin: 5.0,
            v_margin: 5.0,
            mode: 1,
            size: 0,
            opacity: 75,
        };

        let (width, height) =
            preview_asset_size(&params, 800, 400).expect("valid asset dimensions");
        let layout = preview_layout(&params, width, height);

        assert_eq!((width, height), (64.0, 32.0));
        assert_eq!(
            layout,
            PreviewLayout {
                x: 16.0,
                y: 9.0,
                width: 64.0,
                height: 32.0,
            }
        );
    }

    #[test]
    fn preview_layout_clamps_each_edge_and_keeps_oversized_assets_contained_by_cropping() {
        let params = WatermarkPreviewParams {
            location: 0,
            width_pct: 15.0,
            h_margin: -20.0,
            v_margin: 200.0,
            mode: 1,
            size: 1,
            opacity: 100,
        };

        assert_eq!(
            preview_layout(&params, 40.0, 20.0),
            PreviewLayout {
                x: 280.0,
                y: 0.0,
                width: 40.0,
                height: 20.0,
            }
        );
        assert_eq!(
            preview_layout(&params, 640.0, 360.0),
            PreviewLayout {
                x: 0.0,
                y: 0.0,
                width: 640.0,
                height: 360.0,
            }
        );
    }

    #[test]
    fn preview_dimension_parsing_and_asset_paths_reject_invalid_input() {
        assert_eq!(parse_image_dimensions("600x300\n").unwrap(), (600, 300));
        assert!(parse_image_dimensions("600").is_err());
        assert!(parse_image_dimensions("0x300").is_err());
        assert!(watermark_asset_path("station.png").is_ok());
        assert!(watermark_asset_path("../station.png").is_err());
        assert!(watermark_asset_path("nested/station.png").is_err());
    }

    #[tokio::test]
    async fn save_watermark_with_pool_creates_custom_image_watermark() {
        let pool = fresh_pool().await;

        let id = save_watermark_with_pool(&pool, custom_form("bug"))
            .await
            .expect("save watermark");

        let row = sqlx::query(
            r#"SELECT "Name", "ImageSource", "Image", "OriginalContentType"
               FROM "ChannelWatermark" WHERE "Id" = ?"#,
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .expect("fetch watermark");

        assert_eq!(row.try_get::<String, _>("Name").unwrap(), "bug");
        assert_eq!(row.try_get::<i64, _>("ImageSource").unwrap(), 0);
        assert_eq!(
            row.try_get::<Option<String>, _>("Image")
                .unwrap()
                .as_deref(),
            Some("bug.png")
        );
        assert_eq!(
            row.try_get::<Option<String>, _>("OriginalContentType")
                .unwrap()
                .as_deref(),
            Some("image/png")
        );
    }

    #[tokio::test]
    async fn save_watermark_with_pool_allows_channel_logo_without_image() {
        let pool = fresh_pool().await;
        let form = WatermarkForm {
            name: "logo".to_owned(),
            image_source: 1,
            image: Some("ignored.png".to_owned()),
            original_content_type: Some("image/png".to_owned()),
            ..WatermarkForm::default()
        };

        let id = save_watermark_with_pool(&pool, form)
            .await
            .expect("save watermark");

        let row = sqlx::query(
            r#"SELECT "ImageSource", "Image", "OriginalContentType"
               FROM "ChannelWatermark" WHERE "Id" = ?"#,
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .expect("fetch watermark");

        assert_eq!(row.try_get::<i64, _>("ImageSource").unwrap(), 1);
        assert!(row.try_get::<Option<String>, _>("Image").unwrap().is_none());
        assert!(row
            .try_get::<Option<String>, _>("OriginalContentType")
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn save_watermark_with_pool_rejects_duplicate_name() {
        let pool = fresh_pool().await;
        save_watermark_with_pool(&pool, custom_form("dupe"))
            .await
            .expect("save first watermark");

        let error = save_watermark_with_pool(&pool, custom_form("dupe"))
            .await
            .expect_err("reject duplicate");

        assert_eq!(error.to_string(), "Watermark name must be unique");
    }

    #[tokio::test]
    async fn save_watermark_with_pool_clears_opacity_expression_outside_expression_mode() {
        let pool = fresh_pool().await;
        let create = WatermarkForm {
            mode: 3,
            opacity_expression: "if(lt(t,1),0,100)".to_owned(),
            ..custom_form("fade")
        };
        let id = save_watermark_with_pool(&pool, create)
            .await
            .expect("save expression watermark");

        let update = WatermarkForm {
            id,
            mode: 1,
            opacity_expression: "if(lt(t,1),0,100)".to_owned(),
            ..custom_form("fade")
        };
        save_watermark_with_pool(&pool, update)
            .await
            .expect("update watermark");

        let opacity_expression: Option<String> = sqlx::query_scalar(
            r#"SELECT "OpacityExpression" FROM "ChannelWatermark" WHERE "Id" = ?"#,
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .expect("fetch opacity expression");

        assert!(opacity_expression.is_none());
    }

    #[tokio::test]
    async fn save_watermark_with_pool_rejects_validation_failures() {
        let cases = [
            (
                "missing image",
                WatermarkForm {
                    name: "missing image".to_owned(),
                    image: None,
                    ..WatermarkForm::default()
                },
                "Watermark image is required",
            ),
            (
                "bad width",
                WatermarkForm {
                    width_percent: 0.0,
                    ..custom_form("bad width")
                },
                "Width must be between 0 and 100%",
            ),
            (
                "bad margin",
                WatermarkForm {
                    horizontal_margin_percent: 51.0,
                    ..custom_form("bad margin")
                },
                "Horizontal margin must be between 0 and 50%",
            ),
            (
                "bad duration",
                WatermarkForm {
                    duration_seconds: 900,
                    ..custom_form("bad duration")
                },
                "Duration must be positive and less than frequency window",
            ),
            (
                "bad opacity",
                WatermarkForm {
                    opacity: 0,
                    ..custom_form("bad opacity")
                },
                "Opacity must be between 1 and 100",
            ),
        ];

        for (label, form, expected) in cases {
            let pool = fresh_pool().await;
            let Err(error) = save_watermark_with_pool(&pool, form).await else {
                panic!("expected validation error for {label}");
            };
            assert_eq!(error.to_string(), expected, "{label}");
        }
    }
}
