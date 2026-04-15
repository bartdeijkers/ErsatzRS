use serde::{Deserialize, Serialize};

use crate::EntityId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum GraphicsElementKind {
    #[default]
    Image,
    Text,
    Subtitle,
    Motion,
    Script,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GraphicsElement {
    pub id: EntityId,
    pub name: String,
    pub kind: GraphicsElementKind,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub z_index: i32,
    pub path: Option<String>,
    pub text: Option<String>,
    pub script: Option<String>,
    pub variables: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PlayoutItemGraphicsElement {
    pub playout_item_id: EntityId,
    pub graphics_element_id: Option<EntityId>,
    pub variables: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProgramScheduleItemGraphicsElement {
    pub program_schedule_item_id: EntityId,
    pub graphics_element_id: EntityId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BlockItemGraphicsElement {
    pub block_item_id: EntityId,
    pub graphics_element_id: EntityId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DecoGraphicsElement {
    pub deco_id: EntityId,
    pub graphics_element_id: EntityId,
}
