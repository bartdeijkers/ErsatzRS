use axum::{
    extract::{Form, State},
    response::Redirect,
};
use serde::{de, Deserialize, Deserializer};

use crate::{
    admin::server_fns::templates::{
        copy_template_with_pool, create_template_group_with_pool, create_template_with_pool,
        delete_template_group_with_pool, delete_template_with_pool, save_template_with_pool,
        TemplateItemForm, TemplateWithItemsForm,
    },
    host::state::AppState,
};

#[derive(Debug, Default, Deserialize)]
pub struct CreateTemplateGroupForm {
    #[serde(default)]
    name: String,
}

pub async fn create_template_group_form(
    State(state): State<AppState>,
    Form(input): Form<CreateTemplateGroupForm>,
) -> Redirect {
    match create_template_group_with_pool(&state.pool, &input.name).await {
        Ok(_) => Redirect::to("/templates"),
        Err(error) => {
            tracing::error!(%error, "failed to create template group from HTML form");
            Redirect::to("/templates?error=create-group")
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct CreateTemplateForm {
    group_id: i64,
    #[serde(default)]
    name: String,
}

pub async fn create_template_form(
    State(state): State<AppState>,
    Form(input): Form<CreateTemplateForm>,
) -> Redirect {
    match create_template_with_pool(&state.pool, input.group_id, &input.name).await {
        Ok(id) => Redirect::to(&format!("/templates/{id}")),
        Err(error) => {
            tracing::error!(%error, "failed to create template from HTML form");
            Redirect::to("/templates?error=create-template")
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct IdForm {
    id: i64,
}

pub async fn delete_template_group_form(
    State(state): State<AppState>,
    Form(input): Form<IdForm>,
) -> Redirect {
    match delete_template_group_with_pool(&state.pool, input.id).await {
        Ok(()) => Redirect::to("/templates"),
        Err(error) => {
            tracing::error!(%error, group_id = input.id, "failed to delete template group from HTML form");
            Redirect::to("/templates?error=delete-group")
        }
    }
}

pub async fn delete_template_form(
    State(state): State<AppState>,
    Form(input): Form<IdForm>,
) -> Redirect {
    match delete_template_with_pool(&state.pool, input.id).await {
        Ok(()) => Redirect::to("/templates"),
        Err(error) => {
            tracing::error!(%error, template_id = input.id, "failed to delete template from HTML form");
            Redirect::to("/templates?error=delete-template")
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct CopyTemplateForm {
    id: i64,
    group_id: i64,
    #[serde(default)]
    name: String,
}

pub async fn copy_template_form(
    State(state): State<AppState>,
    Form(input): Form<CopyTemplateForm>,
) -> Redirect {
    match copy_template_with_pool(&state.pool, input.id, input.group_id, &input.name).await {
        Ok(id) => Redirect::to(&format!("/templates/{id}")),
        Err(error) => {
            tracing::error!(%error, template_id = input.id, "failed to copy template from HTML form");
            Redirect::to("/templates?error=copy-template")
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct TemplateItemsSaveForm {
    template_id: i64,
    group_id: i64,
    #[serde(default)]
    name: String,
    #[serde(default, deserialize_with = "deserialize_string_vec")]
    item_block_id: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_string_vec")]
    item_block_name: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_string_vec")]
    item_start_minutes: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_string_vec")]
    item_duration_minutes: Vec<String>,
}

pub async fn save_template_items_form(
    State(state): State<AppState>,
    Form(input): Form<TemplateItemsSaveForm>,
) -> Redirect {
    let template_id = input.template_id;
    match save_template_with_pool(&state.pool, input.into_template_form()).await {
        Ok(()) => Redirect::to("/templates"),
        Err(error) => {
            tracing::error!(%error, template_id, "failed to save template items from HTML form");
            Redirect::to(&format!("/templates/{template_id}?error=save"))
        }
    }
}

impl TemplateItemsSaveForm {
    fn into_template_form(self) -> TemplateWithItemsForm {
        let items = self.to_items();
        TemplateWithItemsForm {
            id: self.template_id,
            group_id: self.group_id,
            name: self.name,
            items,
        }
    }

    fn to_items(&self) -> Vec<TemplateItemForm> {
        let mut items = Vec::with_capacity(self.item_block_id.len());
        for index in 0..self.item_block_id.len() {
            let block_id = parse_i64(self.item_block_id.get(index)).unwrap_or_default();
            if block_id <= 0 {
                continue;
            }
            items.push(TemplateItemForm {
                block_id,
                block_name: self.item_block_name.get(index).cloned().unwrap_or_default(),
                start_minutes: parse_i32(self.item_start_minutes.get(index)).unwrap_or_default(),
                duration_minutes: parse_i32(self.item_duration_minutes.get(index))
                    .unwrap_or_default(),
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
    value.and_then(|value| value.trim().parse().ok())
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
    fn template_items_form_aligns_repeated_fields_by_index() {
        let form = TemplateItemsSaveForm {
            template_id: 7,
            group_id: 3,
            name: "Prime".to_string(),
            item_block_id: vec!["10".to_string(), "11".to_string()],
            item_block_name: vec!["Movies".to_string(), "Shows".to_string()],
            item_start_minutes: vec!["60".to_string(), "120".to_string()],
            item_duration_minutes: vec!["30".to_string(), "45".to_string()],
        };

        let result = form.into_template_form();

        assert_eq!(result.id, 7);
        assert_eq!(result.group_id, 3);
        assert_eq!(result.name, "Prime");
        assert_eq!(result.items.len(), 2);
        assert_eq!(result.items[0].block_id, 10);
        assert_eq!(result.items[0].block_name, "Movies");
        assert_eq!(result.items[0].start_minutes, 60);
        assert_eq!(result.items[1].duration_minutes, 45);
    }

    #[test]
    fn template_items_form_drops_blank_block_ids() {
        let form = TemplateItemsSaveForm {
            item_block_id: vec![String::new(), "5".to_string()],
            item_block_name: vec!["bad".to_string(), "good".to_string()],
            item_start_minutes: vec!["0".to_string(), "15".to_string()],
            item_duration_minutes: vec!["30".to_string(), "30".to_string()],
            ..TemplateItemsSaveForm::default()
        };

        let items = form.to_items();

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].block_id, 5);
        assert_eq!(items[0].block_name, "good");
    }

    #[test]
    fn template_items_form_deserializes_single_item_fields() {
        let form: TemplateItemsSaveForm = serde_urlencoded::from_str(
            "template_id=7&group_id=3&name=Prime&item_block_id=10&item_block_name=Movies&item_start_minutes=60&item_duration_minutes=30",
        )
        .expect("single-item form should deserialize");

        let result = form.into_template_form();

        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].block_id, 10);
        assert_eq!(result.items[0].block_name, "Movies");
        assert_eq!(result.items[0].start_minutes, 60);
        assert_eq!(result.items[0].duration_minutes, 30);
    }
}
