use axum::{
    body::Bytes,
    extract::{Form, State},
    response::Redirect,
};
use serde::{de, Deserialize, Deserializer};

use crate::{
    admin::server_fns::decos::{
        create_deco_group_with_pool, create_deco_template_group_with_pool,
        create_deco_template_with_pool, create_deco_with_pool, delete_deco_group_with_pool,
        delete_deco_template_group_with_pool, delete_deco_template_with_pool,
        delete_deco_with_pool, save_deco_template_with_pool, save_deco_with_pool,
        DecoBreakContentForm, DecoForm, DecoTemplateItemForm, DecoTemplateWithItemsForm,
    },
    host::state::AppState,
};

#[derive(Debug, Default, Deserialize)]
pub struct CreateNameForm {
    #[serde(default)]
    name: String,
}

pub async fn create_deco_group_form(
    State(state): State<AppState>,
    Form(input): Form<CreateNameForm>,
) -> Redirect {
    match create_deco_group_with_pool(&state.pool, &input.name).await {
        Ok(_) => Redirect::to("/decos"),
        Err(error) => {
            tracing::error!(%error, "failed to create deco group from HTML form");
            Redirect::to("/decos?error=create-group")
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct CreateGroupedNameForm {
    group_id: i64,
    #[serde(default)]
    name: String,
}

pub async fn create_deco_form(
    State(state): State<AppState>,
    Form(input): Form<CreateGroupedNameForm>,
) -> Redirect {
    match create_deco_with_pool(&state.pool, input.group_id, &input.name).await {
        Ok(id) => Redirect::to(&format!("/decos/{id}")),
        Err(error) => {
            tracing::error!(%error, "failed to create deco from HTML form");
            Redirect::to("/decos?error=create-deco")
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct IdForm {
    id: i64,
}

pub async fn delete_deco_group_form(
    State(state): State<AppState>,
    Form(input): Form<IdForm>,
) -> Redirect {
    match delete_deco_group_with_pool(&state.pool, input.id).await {
        Ok(()) => Redirect::to("/decos"),
        Err(error) => {
            tracing::error!(%error, group_id = input.id, "failed to delete deco group from HTML form");
            Redirect::to("/decos?error=delete-group")
        }
    }
}

pub async fn delete_deco_form(
    State(state): State<AppState>,
    Form(input): Form<IdForm>,
) -> Redirect {
    match delete_deco_with_pool(&state.pool, input.id).await {
        Ok(()) => Redirect::to("/decos"),
        Err(error) => {
            tracing::error!(%error, deco_id = input.id, "failed to delete deco from HTML form");
            Redirect::to("/decos?error=delete-deco")
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct DecoSaveForm {
    id: i64,
    group_id: i64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    watermark_mode: i32,
    #[serde(default, deserialize_with = "deserialize_string_vec")]
    watermark_ids: Vec<String>,
    #[serde(default)]
    use_watermark_during_filler: String,
    #[serde(default)]
    graphics_elements_mode: i32,
    #[serde(default, deserialize_with = "deserialize_string_vec")]
    graphics_element_ids: Vec<String>,
    #[serde(default)]
    use_graphics_elements_during_filler: String,
    #[serde(default)]
    break_content_mode: i32,
    #[serde(default)]
    default_filler_mode: i32,
    #[serde(default)]
    default_filler_collection_type: i32,
    #[serde(default)]
    default_filler_collection_id: String,
    #[serde(default, deserialize_with = "deserialize_string_vec")]
    default_filler_media_item_id: Vec<String>,
    #[serde(default)]
    default_filler_multi_collection_id: String,
    #[serde(default)]
    default_filler_smart_collection_id: String,
    #[serde(default)]
    default_filler_trim_to_fit: String,
    #[serde(default)]
    dead_air_fallback_mode: i32,
    #[serde(default)]
    dead_air_fallback_collection_type: i32,
    #[serde(default)]
    dead_air_fallback_collection_id: String,
    #[serde(default, deserialize_with = "deserialize_string_vec")]
    dead_air_fallback_media_item_id: Vec<String>,
    #[serde(default)]
    dead_air_fallback_multi_collection_id: String,
    #[serde(default)]
    dead_air_fallback_smart_collection_id: String,
    #[serde(default, deserialize_with = "deserialize_string_vec")]
    break_content_id: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_string_vec")]
    break_content_collection_type: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_string_vec")]
    break_content_collection_id: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_string_vec")]
    break_content_media_item_id: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_string_vec")]
    break_content_multi_collection_id: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_string_vec")]
    break_content_smart_collection_id: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_string_vec")]
    break_content_playlist_id: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_string_vec")]
    break_content_placement: Vec<String>,
}

pub async fn save_deco_form(State(state): State<AppState>, body: Bytes) -> Redirect {
    let input = DecoSaveForm::from_urlencoded_bytes(&body);
    let deco_id = input.id;
    match save_deco_with_pool(&state.pool, input.into_deco_form()).await {
        Ok(()) => Redirect::to("/decos"),
        Err(error) => {
            tracing::error!(%error, deco_id, "failed to save deco from HTML form");
            Redirect::to(&format!("/decos/{deco_id}?error=save"))
        }
    }
}

impl DecoSaveForm {
    fn from_urlencoded_bytes(body: &[u8]) -> Self {
        let mut form = Self::default();
        let body = String::from_utf8_lossy(body);
        for pair in body.split('&').filter(|pair| !pair.is_empty()) {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            let value = decode_form_value(value);
            match key {
                "id" => form.id = value.trim().parse().unwrap_or_default(),
                "group_id" => form.group_id = value.trim().parse().unwrap_or_default(),
                "name" => form.name = value,
                "watermark_mode" => form.watermark_mode = parse_i32_string(&value),
                "watermark_ids" => form.watermark_ids.push(value),
                "use_watermark_during_filler" => form.use_watermark_during_filler = value,
                "graphics_elements_mode" => form.graphics_elements_mode = parse_i32_string(&value),
                "graphics_element_ids" => form.graphics_element_ids.push(value),
                "use_graphics_elements_during_filler" => {
                    form.use_graphics_elements_during_filler = value;
                }
                "break_content_mode" => form.break_content_mode = parse_i32_string(&value),
                "default_filler_mode" => form.default_filler_mode = parse_i32_string(&value),
                "default_filler_collection_type" => {
                    form.default_filler_collection_type = parse_i32_string(&value);
                }
                "default_filler_collection_id" => form.default_filler_collection_id = value,
                "default_filler_media_item_id" => form.default_filler_media_item_id.push(value),
                "default_filler_multi_collection_id" => {
                    form.default_filler_multi_collection_id = value;
                }
                "default_filler_smart_collection_id" => {
                    form.default_filler_smart_collection_id = value;
                }
                "default_filler_trim_to_fit" => form.default_filler_trim_to_fit = value,
                "dead_air_fallback_mode" => form.dead_air_fallback_mode = parse_i32_string(&value),
                "dead_air_fallback_collection_type" => {
                    form.dead_air_fallback_collection_type = parse_i32_string(&value);
                }
                "dead_air_fallback_collection_id" => form.dead_air_fallback_collection_id = value,
                "dead_air_fallback_media_item_id" => {
                    form.dead_air_fallback_media_item_id.push(value);
                }
                "dead_air_fallback_multi_collection_id" => {
                    form.dead_air_fallback_multi_collection_id = value;
                }
                "dead_air_fallback_smart_collection_id" => {
                    form.dead_air_fallback_smart_collection_id = value;
                }
                "break_content_id" => form.break_content_id.push(value),
                "break_content_collection_type" => form.break_content_collection_type.push(value),
                "break_content_collection_id" => form.break_content_collection_id.push(value),
                "break_content_media_item_id" => form.break_content_media_item_id.push(value),
                "break_content_multi_collection_id" => {
                    form.break_content_multi_collection_id.push(value);
                }
                "break_content_smart_collection_id" => {
                    form.break_content_smart_collection_id.push(value);
                }
                "break_content_playlist_id" => form.break_content_playlist_id.push(value),
                "break_content_placement" => form.break_content_placement.push(value),
                _ => {}
            }
        }
        form
    }

    fn into_deco_form(self) -> DecoForm {
        let break_content = self.to_break_content();
        DecoForm {
            id: self.id,
            group_id: self.group_id,
            name: self.name,
            watermark_mode: self.watermark_mode,
            watermark_ids: parse_i64_vec(&self.watermark_ids),
            use_watermark_during_filler: parse_bool(&self.use_watermark_during_filler),
            graphics_elements_mode: self.graphics_elements_mode,
            graphics_element_ids: parse_i64_vec(&self.graphics_element_ids),
            use_graphics_elements_during_filler: parse_bool(
                &self.use_graphics_elements_during_filler,
            ),
            break_content_mode: self.break_content_mode,
            break_content,
            default_filler_mode: self.default_filler_mode,
            default_filler_collection_type: self.default_filler_collection_type,
            default_filler_collection_id: parse_i64_string(&self.default_filler_collection_id),
            default_filler_media_item_id: parse_first_i64(&self.default_filler_media_item_id),
            default_filler_multi_collection_id: parse_i64_string(
                &self.default_filler_multi_collection_id,
            ),
            default_filler_smart_collection_id: parse_i64_string(
                &self.default_filler_smart_collection_id,
            ),
            default_filler_trim_to_fit: parse_bool(&self.default_filler_trim_to_fit),
            dead_air_fallback_mode: self.dead_air_fallback_mode,
            dead_air_fallback_collection_type: self.dead_air_fallback_collection_type,
            dead_air_fallback_collection_id: parse_i64_string(
                &self.dead_air_fallback_collection_id,
            ),
            dead_air_fallback_media_item_id: parse_first_i64(&self.dead_air_fallback_media_item_id),
            dead_air_fallback_multi_collection_id: parse_i64_string(
                &self.dead_air_fallback_multi_collection_id,
            ),
            dead_air_fallback_smart_collection_id: parse_i64_string(
                &self.dead_air_fallback_smart_collection_id,
            ),
            ..DecoForm::default()
        }
    }

    fn to_break_content(&self) -> Vec<DecoBreakContentForm> {
        let mut items = Vec::with_capacity(self.break_content_collection_type.len());
        for index in 0..self.break_content_collection_type.len() {
            items.push(DecoBreakContentForm {
                id: parse_i64(self.break_content_id.get(index)).unwrap_or_default(),
                collection_type: parse_i32(self.break_content_collection_type.get(index))
                    .unwrap_or_default(),
                collection_id: parse_i64(self.break_content_collection_id.get(index)),
                media_item_id: parse_i64(self.break_content_media_item_id.get(index)),
                multi_collection_id: parse_i64(self.break_content_multi_collection_id.get(index)),
                smart_collection_id: parse_i64(self.break_content_smart_collection_id.get(index)),
                playlist_id: parse_i64(self.break_content_playlist_id.get(index)),
                placement: parse_i32(self.break_content_placement.get(index)).unwrap_or_default(),
                ..DecoBreakContentForm::default()
            });
        }
        items
    }
}

pub async fn create_deco_template_group_form(
    State(state): State<AppState>,
    Form(input): Form<CreateNameForm>,
) -> Redirect {
    match create_deco_template_group_with_pool(&state.pool, &input.name).await {
        Ok(_) => Redirect::to("/deco-templates"),
        Err(error) => {
            tracing::error!(%error, "failed to create deco template group from HTML form");
            Redirect::to("/deco-templates?error=create-group")
        }
    }
}

pub async fn create_deco_template_form(
    State(state): State<AppState>,
    Form(input): Form<CreateGroupedNameForm>,
) -> Redirect {
    match create_deco_template_with_pool(&state.pool, input.group_id, &input.name).await {
        Ok(id) => Redirect::to(&format!("/deco-templates/{id}")),
        Err(error) => {
            tracing::error!(%error, "failed to create deco template from HTML form");
            Redirect::to("/deco-templates?error=create-template")
        }
    }
}

pub async fn delete_deco_template_group_form(
    State(state): State<AppState>,
    Form(input): Form<IdForm>,
) -> Redirect {
    match delete_deco_template_group_with_pool(&state.pool, input.id).await {
        Ok(()) => Redirect::to("/deco-templates"),
        Err(error) => {
            tracing::error!(%error, group_id = input.id, "failed to delete deco template group from HTML form");
            Redirect::to("/deco-templates?error=delete-group")
        }
    }
}

pub async fn delete_deco_template_form(
    State(state): State<AppState>,
    Form(input): Form<IdForm>,
) -> Redirect {
    match delete_deco_template_with_pool(&state.pool, input.id).await {
        Ok(()) => Redirect::to("/deco-templates"),
        Err(error) => {
            tracing::error!(%error, template_id = input.id, "failed to delete deco template from HTML form");
            Redirect::to("/deco-templates?error=delete-template")
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct DecoTemplateItemsSaveForm {
    template_id: i64,
    group_id: i64,
    #[serde(default)]
    name: String,
    #[serde(default, deserialize_with = "deserialize_string_vec")]
    item_deco_id: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_string_vec")]
    item_deco_name: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_string_vec")]
    item_start_minutes: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_string_vec")]
    item_end_minutes: Vec<String>,
}

pub async fn save_deco_template_items_form(
    State(state): State<AppState>,
    Form(input): Form<DecoTemplateItemsSaveForm>,
) -> Redirect {
    let template_id = input.template_id;
    match save_deco_template_with_pool(&state.pool, input.into_template_form()).await {
        Ok(()) => Redirect::to("/deco-templates"),
        Err(error) => {
            tracing::error!(%error, template_id, "failed to save deco template items from HTML form");
            Redirect::to(&format!("/deco-templates/{template_id}?error=save"))
        }
    }
}

impl DecoTemplateItemsSaveForm {
    fn into_template_form(self) -> DecoTemplateWithItemsForm {
        let items = self.to_items();
        DecoTemplateWithItemsForm {
            id: self.template_id,
            group_id: self.group_id,
            name: self.name,
            items,
        }
    }

    fn to_items(&self) -> Vec<DecoTemplateItemForm> {
        let mut items = Vec::with_capacity(self.item_deco_id.len());
        for index in 0..self.item_deco_id.len() {
            let deco_id = parse_i64(self.item_deco_id.get(index)).unwrap_or_default();
            if deco_id <= 0 {
                continue;
            }
            items.push(DecoTemplateItemForm {
                deco_id,
                deco_name: self.item_deco_name.get(index).cloned().unwrap_or_default(),
                start_minutes: parse_i32(self.item_start_minutes.get(index)).unwrap_or_default(),
                end_minutes: parse_i32(self.item_end_minutes.get(index)).unwrap_or_default(),
            });
        }
        items.sort_by_key(|item| item.start_minutes);
        items
    }
}

fn parse_i32(value: Option<&String>) -> Option<i32> {
    value.and_then(|value| value.trim().parse().ok())
}

fn parse_i64(value: Option<&String>) -> Option<i64> {
    value.and_then(|value| parse_i64_string(value))
}

fn parse_i64_string(value: &str) -> Option<i64> {
    value.trim().parse::<i64>().ok().filter(|value| *value > 0)
}

fn parse_i32_string(value: &str) -> i32 {
    value.trim().parse().unwrap_or_default()
}

fn parse_i64_vec(values: &[String]) -> Vec<i64> {
    values
        .iter()
        .filter_map(|value| parse_i64_string(value))
        .collect()
}

fn parse_first_i64(values: &[String]) -> Option<i64> {
    values.iter().find_map(|value| parse_i64_string(value))
}

fn parse_bool(value: &str) -> bool {
    matches!(value.trim(), "true" | "1" | "on")
}

fn decode_form_value(value: &str) -> String {
    let value = value.replace('+', " ");
    urlencoding::decode(&value)
        .map(std::borrow::Cow::into_owned)
        .unwrap_or(value)
}

fn deserialize_string_vec<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    struct StringVecVisitor;

    impl<'de> de::Visitor<'de> for StringVecVisitor {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a string or a sequence of strings")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(vec![value.to_string()])
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(vec![value])
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: de::SeqAccess<'de>,
        {
            let mut values = Vec::new();
            while let Some(value) = seq.next_element::<String>()? {
                values.push(value);
            }
            Ok(values)
        }
    }

    deserializer.deserialize_any(StringVecVisitor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deco_template_items_form_aligns_repeated_fields_by_index() {
        let form = DecoTemplateItemsSaveForm {
            template_id: 7,
            group_id: 3,
            name: "Evening".to_string(),
            item_deco_id: vec!["10".to_string(), "11".to_string()],
            item_deco_name: vec!["Bugs".to_string(), "Credits".to_string()],
            item_start_minutes: vec!["60".to_string(), "120".to_string()],
            item_end_minutes: vec!["90".to_string(), "150".to_string()],
        };

        let result = form.into_template_form();

        assert_eq!(result.id, 7);
        assert_eq!(result.group_id, 3);
        assert_eq!(result.name, "Evening");
        assert_eq!(result.items.len(), 2);
        assert_eq!(result.items[0].deco_id, 10);
        assert_eq!(result.items[0].deco_name, "Bugs");
        assert_eq!(result.items[0].start_minutes, 60);
        assert_eq!(result.items[1].end_minutes, 150);
    }

    #[test]
    fn deco_template_items_form_drops_blank_deco_ids() {
        let form = DecoTemplateItemsSaveForm {
            item_deco_id: vec![String::new(), "5".to_string()],
            item_deco_name: vec!["bad".to_string(), "good".to_string()],
            item_start_minutes: vec!["0".to_string(), "15".to_string()],
            item_end_minutes: vec!["30".to_string(), "45".to_string()],
            ..DecoTemplateItemsSaveForm::default()
        };

        let items = form.to_items();

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].deco_id, 5);
        assert_eq!(items[0].deco_name, "good");
        assert_eq!(items[0].start_minutes, 15);
    }

    #[test]
    fn deco_save_form_aligns_break_content_and_selected_ids() {
        let form = DecoSaveForm {
            id: 9,
            group_id: 4,
            name: "Prime Deco".to_string(),
            watermark_mode: 2,
            watermark_ids: vec!["7".to_string(), String::new(), "8".to_string()],
            use_watermark_during_filler: "true".to_string(),
            graphics_elements_mode: 3,
            graphics_element_ids: vec!["11".to_string()],
            use_graphics_elements_during_filler: "false".to_string(),
            break_content_mode: 2,
            default_filler_mode: 2,
            default_filler_collection_type: 0,
            default_filler_collection_id: "13".to_string(),
            default_filler_trim_to_fit: "true".to_string(),
            dead_air_fallback_mode: 2,
            dead_air_fallback_collection_type: 6,
            break_content_id: vec!["1".to_string(), "0".to_string()],
            break_content_collection_type: vec!["6".to_string(), "0".to_string()],
            break_content_collection_id: vec![String::new(), "20".to_string()],
            break_content_playlist_id: vec!["30".to_string(), String::new()],
            break_content_placement: vec!["0".to_string(), "1".to_string()],
            ..DecoSaveForm::default()
        };

        let result = form.into_deco_form();

        assert_eq!(result.id, 9);
        assert_eq!(result.group_id, 4);
        assert_eq!(result.name, "Prime Deco");
        assert_eq!(result.watermark_ids, vec![7, 8]);
        assert!(result.use_watermark_during_filler);
        assert_eq!(result.graphics_element_ids, vec![11]);
        assert_eq!(result.default_filler_collection_id, Some(13));
        assert!(result.default_filler_trim_to_fit);
        assert_eq!(result.break_content.len(), 2);
        assert_eq!(result.break_content[0].playlist_id, Some(30));
        assert_eq!(result.break_content[1].collection_id, Some(20));
        assert_eq!(result.break_content[1].placement, 1);
    }

    #[test]
    fn deco_save_form_accepts_repeated_media_item_fields_from_hidden_controls() {
        let form = DecoSaveForm::from_urlencoded_bytes(
            b"id=1&group_id=2&name=Plain+Deco&default_filler_media_item_id=0&default_filler_media_item_id=42&dead_air_fallback_media_item_id=0&dead_air_fallback_media_item_id=43",
        );

        let result = form.into_deco_form();

        assert_eq!(result.id, 1);
        assert_eq!(result.group_id, 2);
        assert_eq!(result.name, "Plain Deco");
        assert_eq!(result.default_filler_media_item_id, Some(42));
        assert_eq!(result.dead_air_fallback_media_item_id, Some(43));
    }
}
