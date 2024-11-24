use super::common::{asset_to_rpc, build_asset_response};
use crate::{
    dao::{scopes, Pagination},
    rpc::{options::Options, Asset},
};
use sea_orm::{DatabaseConnection, DbErr};
use std::collections::HashMap;

pub async fn get_asset(
    db: &DatabaseConnection,
    id: Vec<u8>,
    options: &Options,
) -> Result<Asset, DbErr> {
    let asset = scopes::asset::get_by_id(db, id, false,options).await?;
    asset_to_rpc(asset, options)
}

pub async fn get_assets(
    db: &DatabaseConnection,
    ids: Vec<Vec<u8>>,
    limit: u64,
    options: &Options,
) -> Result<HashMap<String, Asset>, DbErr> {
    let pagination = Pagination::Page { page: 1 };
    let assets = scopes::asset::get_assets(db, ids, &pagination, limit,options).await?;
    let asset_list = build_asset_response(assets, limit, &pagination, options);
    let asset_map = asset_list
        .items
        .into_iter()
        .map(|asset| (asset.id.clone(), asset))
        .collect();
    Ok(asset_map)
}
