use crate::dao::scopes;
use crate::rpc::filter::AssetSorting;
use crate::rpc::response::TransactionSignatureList;
use sea_orm::DatabaseConnection;
use sea_orm::DbErr;
use super::common::build_transaction_signatures_response;
use super::common::{build_asset_response, create_pagination, create_sorting};


pub async fn get_signatures_for_asset(
    db: &DatabaseConnection,
    asset_id: Option<Vec<u8>>,
    tree: Option<Vec<u8>>,
    leaf_idx: Option<i64>,
    limit: u64,
    page: Option<u64>,
    before: Option<Vec<u8>>,
    after: Option<Vec<u8>>,
) -> Result<TransactionSignatureList, DbErr> {
    let pagination = create_pagination(before, after, page)?;
    let transactions = scopes::asset::get_signatures_for_asset(
        db,
        asset_id,
        tree,
        leaf_idx,
        &pagination,
        limit
    )
    .await?;
    Ok(build_transaction_signatures_response(
        transactions,
        limit,
        &pagination,
    ))
}
