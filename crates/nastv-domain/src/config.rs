use serde::{Deserialize, Serialize};

use crate::EntityId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ConfigElement {
    pub id: EntityId,
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConfigElementKey(pub String);
