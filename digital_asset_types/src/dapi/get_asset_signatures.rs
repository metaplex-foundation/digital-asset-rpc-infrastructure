use crate::dao::scopes;
use crate::dao::PageOptions;

use crate::rpc::filter::AssetSortDirection;
use crate::rpc::response::TransactionSignatureList;
use sea_orm::DatabaseConnection;
use sea_orm::DbErr;

use super::common::{build_transaction_signatures_response, create_pagination};

pub async fn get_asset_signatures(
    db: &DatabaseConnection,
    asset_id: Option<Vec<u8>>,
    tree: Option<Vec<u8>>,
    leaf_idx: Option<i64>,
    page_options: PageOptions,
    sort_direction: Option<AssetSortDirection>,
) -> Result<TransactionSignatureList, DbErr> {
    let pagination = create_pagination(&page_options)?;
    let transactions = scopes::signature::get_asset_signatures(
        db,
        asset_id,
        tree,
        leaf_idx,
        &pagination,
        page_options.limit,
        sort_direction,
    )
    .await?;
    Ok(build_transaction_signatures_response(
        transactions,
        page_options.limit,
        &pagination,
    ))
}
