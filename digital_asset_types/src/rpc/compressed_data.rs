use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct CompressedData {
    pub id: i64,
    pub tree_id: String,
    pub leaf_idx: i64,
    pub schema_validated: bool,
    pub parsed_data: serde_json::Value,
    pub slot_updated: i64,
}
