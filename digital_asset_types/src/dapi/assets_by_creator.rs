use crate::dao::scopes;
use crate::dao::PageOptions;
use crate::rpc::display_options::DisplayOptions;
use crate::rpc::filter::AssetSorting;
use crate::rpc::response::AssetList;
use sea_orm::DatabaseConnection;
use sea_orm::DbErr;

use super::common::{build_asset_response, create_pagination, create_sorting};

pub async fn get_assets_by_creator(
    db: &DatabaseConnection,
    creator: Vec<u8>,
    only_verified: bool,
    sorting: AssetSorting,
    page_options: &PageOptions,
    display_options: &DisplayOptions,
) -> Result<AssetList, DbErr> {
    let pagination = create_pagination(&page_options)?;
    let (sort_direction, sort_column) = create_sorting(sorting);
    let assets = scopes::asset::get_by_creator(
        db,
        creator,
        only_verified,
        sort_column,
        sort_direction,
        &pagination,
        page_options.limit,
        display_options.show_unverified_collections,
    )
    .await?;
    Ok(build_asset_response(
        assets,
        page_options.limit,
        &pagination,
        display_options,
    ))
}
