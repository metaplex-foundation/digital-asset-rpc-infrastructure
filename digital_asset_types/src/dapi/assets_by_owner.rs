use crate::dao::{scopes};
use crate::rpc::filter::AssetSorting;
use crate::rpc::response::AssetList;
use sea_orm::DatabaseConnection;
use sea_orm::{DbErr};

use super::common::{create_pagination, create_sorting, build_asset_response};


pub async fn get_assets_by_owner(
    db: &DatabaseConnection,
    owner_address: Vec<u8>,
    sort_by: AssetSorting,
    limit: u64,
    page: Option<u64>,
    before: Option<Vec<u8>>,
    after: Option<Vec<u8>>,
) -> Result<AssetList, DbErr> {
    let pagination = create_pagination(before, after, page)?;
    let (sort_direction,sort_column) = create_sorting(sort_by);
    let assets = scopes::asset::get_assets_by_owner(
        db, owner_address, sort_column, sort_direction, &pagination, limit).await?;
    Ok(build_asset_response(assets, limit,&pagination))
}
