use crate::dao::scopes;
use crate::dao::PageOptions;

use crate::rpc::response::TransactionSignatureList;
use sea_orm::DatabaseConnection;
use sea_orm::DbErr;

use super::common::{build_transaction_signatures_response, create_pagination};

pub async fn get_signatures_for_asset(
    db: &DatabaseConnection,
    asset_id: Option<Vec<u8>>,
    tree: Option<Vec<u8>>,
    leaf_idx: Option<i64>,
    page_options: PageOptions,
) -> Result<TransactionSignatureList, DbErr> {
    let pagination = create_pagination(&page_options)?;
    let transactions = scopes::asset::get_signatures_for_asset(
        db,
        asset_id,
        tree,
        leaf_idx,
        &pagination,
        page_options.limit,
    )
    .await?;
    Ok(build_transaction_signatures_response(
        transactions,
        page_options.limit,
        &pagination,
    ))
}
