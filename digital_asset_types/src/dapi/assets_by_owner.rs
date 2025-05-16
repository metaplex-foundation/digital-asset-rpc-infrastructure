use crate::dao::scopes;
use crate::dao::PageOptions;
use crate::rpc::filter::AssetSorting;
use crate::rpc::options::Options;
use crate::rpc::response::AssetList;
use sea_orm::DatabaseConnection;
use sea_orm::DbErr;

use super::common::build_asset_response;

pub async fn get_assets_by_owner(
    db: &DatabaseConnection,
    owner_address: Vec<u8>,
    sorting: AssetSorting,
    page_options: &PageOptions,
    options: &Options,
) -> Result<AssetList, DbErr> {
    let pagination = page_options.try_into()?;
    let (column, order) = sorting.into_sorting();
    let assets = scopes::asset::get_assets_by_owner(
        db,
        owner_address,
        column,
        order,
        &pagination,
        page_options.limit,
        options,
    )
    .await?;
    Ok(build_asset_response(
        assets,
        page_options.limit,
        &pagination,
        options,
    ))
}
