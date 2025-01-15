use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, JsonSchema, Default)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Options {
    #[serde(default)]
    pub show_unverified_collections: bool,
    #[serde(default)]
    pub show_collection_metadata: bool,
    #[serde(default)]
    pub show_zero_balance: bool,
    #[serde(default)]
    pub show_inscription: bool,
    #[serde(default)]
    pub show_fungible: bool,
}
