use super::common::{asset_to_rpc, build_asset_response};
use crate::{
    dao::{scopes, Pagination},
    rpc::{display_options::DisplayOptions, Asset},
};
use sea_orm::{DatabaseConnection, DbErr};
use std::collections::HashMap;

pub async fn get_asset(
    db: &DatabaseConnection,
    id: Vec<u8>,
    display_options: &DisplayOptions,
) -> Result<Asset, DbErr> {
    let asset = scopes::asset::get_by_id(db, id, false).await?;
    asset_to_rpc(asset, display_options)
}

pub async fn get_asset_batch(
    db: &DatabaseConnection,
    ids: Vec<Vec<u8>>,
    limit: u64,
    display_options: &DisplayOptions,
) -> Result<HashMap<String, Asset>, DbErr> {
    let pagination = Pagination::Page { page: 1 };
    let assets = scopes::asset::get_asset_batch(db, ids, &pagination, limit).await?;
    let asset_list = build_asset_response(assets, limit, &pagination, display_options);
    let asset_map = asset_list
        .items
        .into_iter()
        .map(|asset| (asset.id.clone(), asset))
        .collect();
    Ok(asset_map)
}
