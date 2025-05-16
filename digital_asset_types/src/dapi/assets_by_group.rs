use crate::dao::scopes;
use crate::dao::PageOptions;
use crate::rpc::filter::AssetSorting;
use crate::rpc::options::Options;
use crate::rpc::response::AssetList;
use sea_orm::DatabaseConnection;
use sea_orm::DbErr;

use super::common::build_asset_response;

#[allow(clippy::too_many_arguments)]
pub async fn get_assets_by_group(
    db: &DatabaseConnection,
    group_key: String,
    group_value: String,
    sorting: AssetSorting,
    page_options: &PageOptions,
    options: &Options,
) -> Result<AssetList, DbErr> {
    let pagination = page_options.try_into()?;
    let (column, order) = sorting.into_sorting();
    let assets = scopes::asset::get_by_grouping(
        db,
        group_key,
        group_value,
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
