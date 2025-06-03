use super::common::build_asset_response;
use crate::{
    dao::{scopes, PageOptions, SearchAssetsQuery},
    rpc::{filter::AssetSorting, options::Options, response::AssetList},
};
use sea_orm::{DatabaseConnection, DbErr};

#[tracing::instrument(name = "db::searchAssets", skip_all)]
pub async fn search_assets(
    db: &DatabaseConnection,
    search_assets_query: SearchAssetsQuery,
    sorting: AssetSorting,
    page_options: &PageOptions,
    options: &Options,
) -> Result<AssetList, DbErr> {
    let pagination = page_options.try_into()?;
    let (column, order) = sorting.into_sorting();
    search_assets_query.validate()?;

    let assets = scopes::asset::search_assets(
        db,
        &search_assets_query,
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
