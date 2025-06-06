use crate::{
    dao::{
        asset,
        cl_audits_v2::{self, Column},
        extensions::instruction::PascalCase,
        sea_orm_active_enums::Instruction,
        Cursor, Pagination,
    },
    rpc::filter::AssetSortDirection,
};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DbErr, EntityTrait, Order, QueryFilter, QueryOrder, QuerySelect,
    Select,
};

pub trait TransactionSelectExt {
    fn sort_by(self, column: Column, direction: &Order) -> Self;

    fn page_by(
        self,
        pagination: &Pagination,
        limit: u64,
        sort_direction: &Order,
        col: Column,
    ) -> Self;
}

impl TransactionSelectExt for Select<cl_audits_v2::Entity> {
    fn sort_by(self, col: Column, direction: &Order) -> Self {
        match col {
            Column::Tx => self.order_by(col, direction.clone()).to_owned(),
            _ => self
                .order_by(col, direction.clone())
                .order_by(Column::Tx, Order::Asc)
                .to_owned(),
        }
    }

    fn page_by(
        mut self,
        pagination: &Pagination,
        limit: u64,
        order: &Order,
        column: Column,
    ) -> Self {
        match pagination {
            Pagination::Keyset { before, after } => {
                if let Some(b) = before {
                    self = self.filter(column.lt(b.clone())).to_owned();
                }
                if let Some(a) = after {
                    self = self.filter(column.gt(a.clone())).to_owned();
                }
            }
            Pagination::Page { page } => {
                if *page > 0 {
                    self = self.offset((page - 1) * limit).to_owned();
                }
            }
            Pagination::Cursor(cursor) => {
                if *cursor != Cursor::default() {
                    if order == &Order::Asc {
                        self = self.filter(column.gt(cursor.id.clone())).to_owned();
                    } else {
                        self = self.filter(column.lt(cursor.id.clone())).to_owned();
                    }
                }
            }
        }
        self.limit(limit).to_owned()
    }
}

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

    let mut stmt = cl_audits_v2::Entity::find()
        .filter(cl_audits_v2::Column::Tree.eq(tree))
        .filter(cl_audits_v2::Column::LeafIdx.eq(leaf_idx));

    stmt = stmt
        .sort_by(cl_audits_v2::Column::Seq, &sort_order)
        .page_by(pagination, limit, &sort_order, cl_audits_v2::Column::Seq);

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
