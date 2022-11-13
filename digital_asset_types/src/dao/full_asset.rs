use crate::dao::generated::{asset, asset_authority, asset_creators, asset_data, asset_grouping};

pub struct FullAsset {
    pub asset: asset::Model,
    pub data: asset_data::Model,
    pub authorities: Vec<asset_authority::Model>,
    pub creators: Vec<asset_creators::Model>,
    pub groups: Vec<asset_grouping::Model>,
}

pub struct FullAssetList {
    pub list: Vec<FullAsset>,
}
