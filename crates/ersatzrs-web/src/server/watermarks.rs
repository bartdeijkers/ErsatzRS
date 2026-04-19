use leptos::prelude::*;
use serde::{Deserialize, Serialize};

/// Lightweight row for the watermark list table.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct WatermarkListRow {
    pub id: i64,
    pub name: String,
    pub mode: String,
    pub location: String,
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
            width_percent: 10.0,
            horizontal_margin_percent: 2.0,
            vertical_margin_percent: 2.0,
            frequency_minutes: 5,
            duration_seconds: 15,
            opacity: 100,
            place_within_source_content: false,
            opacity_expression: String::new(),
            z_index: 0,
        }
    }
}

fn mode_label(mode: i64) -> &'static str {
    match mode {
        0 => "None",
        1 => "Permanent",
        2 => "Intermittent",
        3 => "Opacity Expression",
        _ => "Unknown",
    }
}

fn location_label(loc: i64) -> &'static str {
    match loc {
        0 => "Bottom Right",
        1 => "Bottom Left",
        2 => "Top Right",
        3 => "Top Left",
        4 => "Top Middle",
        5 => "Right Middle",
        6 => "Bottom Middle",
        7 => "Left Middle",
        8 => "Middle Center",
        _ => "Unknown",
    }
}

#[server]
pub async fn list_watermark_rows(
    page: u32,
    page_size: u32,
) -> Result<Vec<WatermarkListRow>, ServerFnError> {
    use crate::state::AppState;
    use sqlx::Row;

    let state = expect_context::<AppState>();
    let offset = i64::from(page.saturating_sub(1)) * i64::from(page_size);

    let rows = sqlx::query(
        r#"SELECT "Id", "Name", "Mode", "Location" FROM "ChannelWatermark"
           ORDER BY "Name" COLLATE NOCASE LIMIT ? OFFSET ?"#,
    )
    .bind(i64::from(page_size))
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| WatermarkListRow {
            id: r.try_get("Id").unwrap_or(0),
            name: r.try_get("Name").unwrap_or_default(),
            mode: mode_label(r.try_get("Mode").unwrap_or(0)).to_string(),
            location: location_label(r.try_get("Location").unwrap_or(0)).to_string(),
        })
        .collect())
}

#[server]
pub async fn count_watermarks() -> Result<u64, ServerFnError> {
    use crate::state::AppState;

    let state = expect_context::<AppState>();
    let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "ChannelWatermark""#)
        .fetch_one(&state.pool)
        .await?;
    Ok(u64::try_from(count).unwrap_or(0))
}

#[server]
pub async fn get_watermark_form(id: i64) -> Result<WatermarkForm, ServerFnError> {
    use crate::state::AppState;
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
            width_percent: r.try_get("WidthPercent").unwrap_or(10.0),
            horizontal_margin_percent: r.try_get("HorizontalMarginPercent").unwrap_or(2.0),
            vertical_margin_percent: r.try_get("VerticalMarginPercent").unwrap_or(2.0),
            frequency_minutes: i32::try_from(
                r.try_get::<i64, _>("FrequencyMinutes").unwrap_or(5),
            )
            .unwrap_or(5),
            duration_seconds: i32::try_from(
                r.try_get::<i64, _>("DurationSeconds").unwrap_or(15),
            )
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

#[allow(clippy::too_many_lines)]
#[server]
pub async fn save_watermark(form: WatermarkForm) -> Result<i64, ServerFnError> {
    use crate::state::AppState;

    let state = expect_context::<AppState>();

    let name = form.name.trim().to_string();
    if name.is_empty() {
        return Err(ServerFnError::new("Name is required"));
    }
    if name.len() > 50 {
        return Err(ServerFnError::new("Name must be 50 characters or fewer"));
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
        let exists: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "ChannelWatermark" WHERE "Name" = ? COLLATE NOCASE"#,
        )
        .bind(&name)
        .fetch_one(&state.pool)
        .await
        .unwrap_or(0);
        if exists > 0 {
            return Err(ServerFnError::new("Watermark name must be unique"));
        }

        let id = sqlx::query(
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
        .execute(&state.pool)
        .await?
        .last_insert_rowid();

        Ok(id)
    } else {
        let exists: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "ChannelWatermark" WHERE "Name" = ? COLLATE NOCASE AND "Id" != ?"#,
        )
        .bind(&name)
        .bind(form.id)
        .fetch_one(&state.pool)
        .await
        .unwrap_or(0);
        if exists > 0 {
            return Err(ServerFnError::new("Watermark name must be unique"));
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
        .execute(&state.pool)
        .await?
        .rows_affected();

        if rows == 0 {
            return Err(ServerFnError::new("Watermark not found"));
        }
        Ok(form.id)
    }
}

#[server]
pub async fn delete_watermark(id: i64) -> Result<(), ServerFnError> {
    use crate::state::AppState;

    let state = expect_context::<AppState>();
    sqlx::query(r#"DELETE FROM "ChannelWatermark" WHERE "Id" = ?"#)
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(())
}
