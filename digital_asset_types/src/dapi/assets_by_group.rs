use crate::dao::scopes;
use crate::rpc::display_options::DisplayOptions;
use crate::rpc::filter::AssetSorting;
use crate::rpc::response::AssetList;
use sea_orm::DatabaseConnection;
use sea_orm::DbErr;

use super::common::{build_asset_response, create_pagination, create_sorting};
pub async fn get_assets_by_group(
    db: &DatabaseConnection,
    group_key: String,
    group_value: String,
    sorting: AssetSorting,
    limit: u64,
    page: Option<u64>,
    before: Option<Vec<u8>>,
    after: Option<Vec<u8>>,
    display_options: &DisplayOptions,
) -> Result<AssetList, DbErr> {
    let pagination = create_pagination(before, after, page)?;
    let (sort_direction, sort_column) = create_sorting(sorting);
    let assets = scopes::asset::get_by_grouping(
        db,
        group_key,
        group_value,
        sort_column,
        sort_direction,
        &pagination,
        limit,
        display_options.show_unverified_collections,
    )
    .await?;
    Ok(build_asset_response(
        assets,
        limit,
        &pagination,
        display_options,
    ))
}
