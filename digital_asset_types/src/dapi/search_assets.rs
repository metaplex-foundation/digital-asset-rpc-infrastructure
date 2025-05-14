use super::common::{build_asset_response, create_pagination, create_sorting};
use crate::{
    dao::{scopes, PageOptions, SearchAssetsQuery},
    rpc::{filter::AssetSorting, options::Options, response::AssetList},
};
use sea_orm::{DatabaseConnection, DbErr};

pub async fn search_assets(
    db: &DatabaseConnection,
    search_assets_query: SearchAssetsQuery,
    sorting: AssetSorting,
    page_options: &PageOptions,
    options: &Options,
) -> Result<AssetList, DbErr> {
    let pagination = create_pagination(page_options)?;
    let (sort_direction, sort_column) = create_sorting(sorting);
    search_assets_query.validate()?;

    let assets = scopes::asset::search_assets(
        db,
        &search_assets_query,
        sort_column,
        sort_direction,
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
