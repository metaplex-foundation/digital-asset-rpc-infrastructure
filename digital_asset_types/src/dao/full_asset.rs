use crate::dao::{asset, asset_authority, asset_creators, asset_data, asset_grouping};

use super::asset_v1_account_attachments;

use super::tokens;

pub struct FullAssetGroup {
    pub id: i64,
    pub asset_id: Vec<u8>,
    pub group_key: String,
    pub group_value: Option<String>,
    pub seq: Option<i64>,
    pub slot_updated: Option<i64>,
    pub verified: bool,
    pub group_info_seq: Option<i64>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct FullAsset {
    pub asset: asset::Model,
    pub data: asset_data::Model,
    pub token_info: Option<tokens::Model>,
    pub authorities: Vec<asset_authority::Model>,
    pub creators: Vec<asset_creators::Model>,
    pub groups: Vec<(asset_grouping::Model, Option<asset_data::Model>)>,
    pub inscription: Option<asset_v1_account_attachments::Model>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct AssetRelated {
    pub authorities: Vec<asset_authority::Model>,
    pub creators: Vec<asset_creators::Model>,
    pub groups: Vec<asset_grouping::Model>,
}

pub struct FullAssetList {
    pub list: Vec<FullAsset>,
}
