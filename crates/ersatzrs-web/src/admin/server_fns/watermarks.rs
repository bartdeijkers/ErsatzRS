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
#[derive(Clone, Debug, Serialize, Deserialize)]
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

#[server]
pub async fn list_watermark_rows(
    page: u32,
    page_size: u32,
) -> Result<Vec<WatermarkListRow>, ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let offset = i64::from(page.saturating_sub(1)) * i64::from(page_size);

    let rows = sqlx::query(
        r#"SELECT "Id","Name","Mode","Location","Image","ImageSource"
           FROM "ChannelWatermark"
           ORDER BY "Name" LIMIT ? OFFSET ?"#,
    )
    .bind(i64::from(page_size))
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| {
            let mode_id = r.try_get::<i64, _>("Mode").unwrap_or(0);
            let location_id = r.try_get::<i64, _>("Location").unwrap_or(0);
            WatermarkListRow {
                id: r.try_get("Id").unwrap_or(0),
                name: r.try_get("Name").unwrap_or_default(),
                mode_id: i32::try_from(mode_id).unwrap_or(0),
                location_id: i32::try_from(location_id).unwrap_or(0),
                image_path: r.try_get("Image").ok(),
                image_source: i32::try_from(r.try_get::<i64, _>("ImageSource").unwrap_or(0))
                    .unwrap_or(0),
            }
        })
        .collect())
}

#[server]
pub async fn count_watermarks() -> Result<u64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "ChannelWatermark""#)
        .fetch_one(&state.pool)
        .await?;
    Ok(u64::try_from(count).unwrap_or(0))
}

#[server]
pub async fn get_watermark_form(id: i64) -> Result<WatermarkForm, ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();

    let maybe = sqlx::query(
        r#"SELECT "Id","Name","Mode","ImageSource","Image","OriginalContentType",
                  "Location","Size","WidthPercent","HorizontalMarginPercent",
                  "VerticalMarginPercent","FrequencyMinutes","DurationSeconds",
                  "Opacity","PlaceWithinSourceContent","OpacityExpression","ZIndex"
           FROM "ChannelWatermark" WHERE "Id" = ?"#,
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;

    match maybe {
        None => Err(ServerFnError::new("Watermark not found")),
        Some(r) => Ok(WatermarkForm {
            id: r.try_get("Id").unwrap_or(0),
            name: r.try_get("Name").unwrap_or_default(),
            mode: i32::try_from(r.try_get::<i64, _>("Mode").unwrap_or(0)).unwrap_or(0),
            image_source: i32::try_from(r.try_get::<i64, _>("ImageSource").unwrap_or(0))
                .unwrap_or(0),
            image: r.try_get("Image").ok(),
            original_content_type: r.try_get("OriginalContentType").ok(),
            location: i32::try_from(r.try_get::<i64, _>("Location").unwrap_or(0)).unwrap_or(0),
            size: i32::try_from(r.try_get::<i64, _>("Size").unwrap_or(0)).unwrap_or(0),
            width_percent: r.try_get("WidthPercent").unwrap_or(15.0),
            horizontal_margin_percent: r.try_get("HorizontalMarginPercent").unwrap_or(5.0),
            vertical_margin_percent: r.try_get("VerticalMarginPercent").unwrap_or(5.0),
            frequency_minutes: i32::try_from(r.try_get::<i64, _>("FrequencyMinutes").unwrap_or(15))
                .unwrap_or(15),
            duration_seconds: i32::try_from(r.try_get::<i64, _>("DurationSeconds").unwrap_or(15))
                .unwrap_or(15),
            opacity: i32::try_from(r.try_get::<i64, _>("Opacity").unwrap_or(100)).unwrap_or(100),
            place_within_source_content: r
                .try_get::<i64, _>("PlaceWithinSourceContent")
                .unwrap_or(0)
                != 0,
            opacity_expression: r.try_get("OpacityExpression").unwrap_or_default(),
            z_index: i32::try_from(r.try_get::<i64, _>("ZIndex").unwrap_or(0)).unwrap_or(0),
        }),
    }
}

#[server]
pub async fn save_watermark(form: WatermarkForm) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    save_watermark_with_pool(&state.pool, form)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))
}

#[cfg(feature = "ssr")]
#[derive(Debug, thiserror::Error)]
pub enum WatermarkSaveError {
    #[error("{0}")]
    Validation(&'static str),
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
}

#[cfg(feature = "ssr")]
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
    sqlx::query(r#"DELETE FROM "ChannelWatermark" WHERE "Id" = ?"#)
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(())
}

#[server]
pub async fn copy_watermark(id: i64, new_name: String) -> Result<i64, ServerFnError> {
    use crate::host::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();

    let new_name = new_name.trim().to_string();
    if new_name.is_empty() {
        return Err(ServerFnError::new("Name is required"));
    }
    if new_name.len() > 50 {
        return Err(ServerFnError::new("Name must be 50 characters or fewer"));
    }

    let exists: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "ChannelWatermark" WHERE "Name" = ?"#)
            .bind(&new_name)
            .fetch_one(&state.pool)
            .await
            .unwrap_or(0);
    if exists > 0 {
        return Err(ServerFnError::new("Watermark name must be unique"));
    }

    let src = sqlx::query(
        r#"SELECT "Mode","ImageSource","Image","OriginalContentType","Location","Size",
                  "WidthPercent","HorizontalMarginPercent","VerticalMarginPercent",
                  "FrequencyMinutes","DurationSeconds","Opacity","PlaceWithinSourceContent",
                  "OpacityExpression","ZIndex"
           FROM "ChannelWatermark" WHERE "Id" = ?"#,
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;

    let Some(src) = src else {
        return Err(ServerFnError::new("Source watermark not found"));
    };

    let result = sqlx::query(
        r#"INSERT INTO "ChannelWatermark"
           ("Name","Mode","ImageSource","Image","OriginalContentType","Location","Size",
            "WidthPercent","HorizontalMarginPercent","VerticalMarginPercent",
            "FrequencyMinutes","DurationSeconds","Opacity","PlaceWithinSourceContent",
            "OpacityExpression","ZIndex")
           SELECT ?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?"#,
    )
    .bind(&new_name)
    .bind(src.try_get::<i64, _>("Mode").unwrap_or(1))
    .bind(src.try_get::<i64, _>("ImageSource").unwrap_or(0))
    .bind(src.try_get::<Option<String>, _>("Image").ok().flatten())
    .bind(
        src.try_get::<Option<String>, _>("OriginalContentType")
            .ok()
            .flatten(),
    )
    .bind(src.try_get::<i64, _>("Location").unwrap_or(0))
    .bind(src.try_get::<i64, _>("Size").unwrap_or(0))
    .bind(src.try_get::<f64, _>("WidthPercent").unwrap_or(15.0))
    .bind(
        src.try_get::<f64, _>("HorizontalMarginPercent")
            .unwrap_or(5.0),
    )
    .bind(
        src.try_get::<f64, _>("VerticalMarginPercent")
            .unwrap_or(5.0),
    )
    .bind(src.try_get::<i64, _>("FrequencyMinutes").unwrap_or(15))
    .bind(src.try_get::<i64, _>("DurationSeconds").unwrap_or(15))
    .bind(src.try_get::<i64, _>("Opacity").unwrap_or(100))
    .bind(
        src.try_get::<i64, _>("PlaceWithinSourceContent")
            .unwrap_or(0),
    )
    .bind(
        src.try_get::<Option<String>, _>("OpacityExpression")
            .ok()
            .flatten(),
    )
    .bind(src.try_get::<i64, _>("ZIndex").unwrap_or(0))
    .execute(&state.pool)
    .await?;
    let new_id = ersatzrs_infra::db::last_insert_id(result);

    Ok(new_id)
}

/// Generates a 320×180 drawvg-based preview PNG of the watermark placement.
/// Returns a base64 `data:image/png;base64,…` URL, or an empty string when
/// `mode == 0` (None).
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
    let wm_h = if params.size == 1 {
        // ActualSize — use a fixed placeholder size
        32.0_f64
    } else {
        (wm_w * 0.5).max(8.0)
    };
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

#[cfg(all(test, feature = "ssr", not(feature = "mysql")))]
mod tests {
    use super::*;
    use sqlx::Row;

    async fn fresh_pool() -> ersatzrs_infra::db::DbPool {
        let dir = tempfile::tempdir().expect("temp db dir");
        let database_url = format!("sqlite://{}", dir.path().join("watermarks.db").display());
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
