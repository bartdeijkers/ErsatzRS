//! Smart-collection machine routes share validated query/scheduling operations.
use super::machine::{parse_json, MachineError};
use crate::host::state::AppState;
use axum::{
    extract::{rejection::JsonRejection, Path, State},
    Json,
};
use ersatzrs_app::{
    commands::{DeleteSmartCollection, SaveSmartCollection},
    queries::ListCollections,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SmartMutation {
    id: i64,
    name: String,
    query: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartResponse {
    id: i64,
    name: String,
    query: String,
}

pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<SmartResponse>>, MachineError> {
    let (rows, _) = state
        .application
        .list_smart_collections(ListCollections {
            search: String::new(),
            page: 0,
            page_size: u32::MAX,
        })
        .await?;
    Ok(Json(
        rows.into_iter()
            .map(|row| SmartResponse {
                id: row.id,
                name: row.name,
                query: row.query,
            })
            .collect(),
    ))
}

pub async fn create(
    State(state): State<AppState>,
    input: Result<Json<SmartMutation>, JsonRejection>,
) -> Result<Json<Value>, MachineError> {
    let input = parse_json(input)?;
    let id = state
        .application
        .save_smart_collection_for_api(
            SaveSmartCollection {
                id: 0,
                name: input.name,
                query: input.query,
            },
            false,
        )
        .await?;
    Ok(Json(json!({"id":id})))
}

pub async fn update(
    State(state): State<AppState>,
    input: Result<Json<SmartMutation>, JsonRejection>,
) -> Result<Json<Value>, MachineError> {
    let input = parse_json(input)?;
    let id = state
        .application
        .save_smart_collection_for_api(
            SaveSmartCollection {
                id: input.id,
                name: input.name,
                query: input.query,
            },
            true,
        )
        .await?;
    Ok(Json(json!({"smartCollectionId":id,"id":id})))
}

pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, MachineError> {
    if id <= 0 {
        return Err(ersatzrs_app::ApplicationError::NotFound {
            kind: "smart collection",
            id,
        }
        .into());
    }
    state
        .application
        .delete_smart_collection(DeleteSmartCollection { id })
        .await?;
    Ok(Json(json!({})))
}
