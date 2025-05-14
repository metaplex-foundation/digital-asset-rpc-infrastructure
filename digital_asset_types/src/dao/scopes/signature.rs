use super::asset::paginate;
use crate::{
    dao::{
        asset, cl_audits_v2, extensions::instruction::PascalCase,
        sea_orm_active_enums::Instruction, Pagination,
    },
    rpc::filter::AssetSortDirection,
};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DbErr, EntityTrait, Order, QueryFilter, QueryOrder, QuerySelect,
};

pub async fn fetch_transactions(
    conn: &impl ConnectionTrait,
    tree: Vec<u8>,
    leaf_idx: i64,
    pagination: &Pagination,
    limit: u64,
    sort_direction: Option<AssetSortDirection>,
) -> Result<Vec<(String, String)>, DbErr> {
    // Default sort direction is Desc
    // Similar to GetSignaturesForAddress in the Solana API
    let sort_direction = sort_direction.unwrap_or(AssetSortDirection::Desc);
    let sort_order = match sort_direction {
        AssetSortDirection::Asc => sea_orm::Order::Asc,
        AssetSortDirection::Desc => sea_orm::Order::Desc,
    };

    let mut stmt = cl_audits_v2::Entity::find().filter(cl_audits_v2::Column::Tree.eq(tree));
    stmt = stmt.filter(cl_audits_v2::Column::LeafIdx.eq(leaf_idx));
    stmt = stmt.order_by(cl_audits_v2::Column::Seq, sort_order.clone());

    stmt = paginate(
        pagination,
        limit,
        stmt,
        sort_order,
        cl_audits_v2::Column::Seq,
    );
    let transactions = stmt.all(conn).await?;
    let transaction_list = transactions
        .into_iter()
        .map(|transaction| {
            let tx = bs58::encode(transaction.tx).into_string();
            let ix = Instruction::to_pascal_case(&transaction.instruction).to_string();
            (tx, ix)
        })
        .collect();

    Ok(transaction_list)
}

pub async fn get_asset_signatures(
    conn: &impl ConnectionTrait,
    asset_id: Option<Vec<u8>>,
    tree_id: Option<Vec<u8>>,
    leaf_idx: Option<i64>,
    pagination: &Pagination,
    limit: u64,
    sort_direction: Option<AssetSortDirection>,
) -> Result<Vec<(String, String)>, DbErr> {
    // if tree_id and leaf_idx are provided, use them directly to fetch transactions
    if let (Some(tree_id), Some(leaf_idx)) = (tree_id, leaf_idx) {
        let transactions =
            fetch_transactions(conn, tree_id, leaf_idx, pagination, limit, sort_direction).await?;
        return Ok(transactions);
    }

    if asset_id.is_none() {
        return Err(DbErr::Custom(
            "Either 'id' or both 'tree' and 'leafIndex' must be provided".to_string(),
        ));
    }

    // if only asset_id is provided, fetch the latest tree and leaf_idx (asset.nonce) for the asset
    // and use them to fetch transactions
    let stmt = asset::Entity::find()
        .distinct_on([(asset::Entity, asset::Column::Id)])
        .filter(asset::Column::Id.eq(asset_id))
        .order_by(asset::Column::Id, Order::Desc)
        .limit(1);
    let asset = stmt.one(conn).await?;
    if let Some(asset) = asset {
        let tree = asset
            .tree_id
            .ok_or(DbErr::RecordNotFound("Tree not found".to_string()))?;
        if tree.is_empty() {
            return Err(DbErr::Custom("Empty tree for asset".to_string()));
        }
        let leaf_idx = asset
            .nonce
            .ok_or(DbErr::RecordNotFound("Leaf ID does not exist".to_string()))?;
        let transactions =
            fetch_transactions(conn, tree, leaf_idx, pagination, limit, sort_direction).await?;
        Ok(transactions)
    } else {
        Ok(Vec::new())
    }
}
