use crate::dao::{asset, asset_authority, asset_creators, asset_data, asset_grouping};

use super::asset_v1_account_attachments;

#[derive(Clone, Debug, PartialEq)]
pub struct FullAsset {
    pub asset: asset::Model,
    pub data: Option<asset_data::Model>,
    pub authorities: Vec<asset_authority::Model>,
    pub creators: Vec<asset_creators::Model>,
    pub groups: Vec<asset_grouping::Model>,
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
