use super::common::{build_asset_response, create_pagination, create_sorting};
use crate::{
    dao::{scopes, SearchAssetsQuery},
    rpc::{filter::AssetSorting, response::AssetList},
};
use sea_orm::{DatabaseConnection, DbErr};

pub async fn search_assets(
    db: &DatabaseConnection,
    search_assets_query: SearchAssetsQuery,
    sorting: AssetSorting,
    limit: u64,
    page: Option<u64>,
    before: Option<Vec<u8>>,
    after: Option<Vec<u8>>,
) -> Result<AssetList, DbErr> {
    let pagination = create_pagination(before, after, page)?;
    let (sort_direction, sort_column) = create_sorting(sorting);
    let (condition, joins) = search_assets_query.conditions()?;
    let assets = scopes::asset::get_assets_by_condition(
        db,
        condition,
        joins,
        sort_column,
        sort_direction,
        &pagination,
        limit,
    )
    .await?;
    Ok(build_asset_response(assets, limit, &pagination))
}
