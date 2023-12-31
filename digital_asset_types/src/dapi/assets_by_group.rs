use crate::dao::scopes;
use crate::dao::PageOptions;
use crate::rpc::filter::AssetSorting;
use crate::rpc::options::Options;
use crate::rpc::response::AssetList;
use sea_orm::DatabaseConnection;
use sea_orm::DbErr;

use super::common::{build_asset_response, create_pagination, create_sorting};

#[allow(clippy::too_many_arguments)]
pub async fn get_assets_by_group(
    db: &DatabaseConnection,
    group_key: String,
    group_value: String,
    sorting: AssetSorting,
    page_options: &PageOptions,
    options: &Options,
) -> Result<AssetList, DbErr> {
    let pagination = create_pagination(page_options)?;
    let (sort_direction, sort_column) = create_sorting(sorting);
    let assets = scopes::asset::get_by_grouping(
        db,
        group_key,
        group_value,
        sort_column,
        sort_direction,
        &pagination,
        page_options.limit,
        options.show_unverified_collections,
    )
    .await?;
    Ok(build_asset_response(
        assets,
        page_options.limit,
        &pagination,
        options,
    ))
}
