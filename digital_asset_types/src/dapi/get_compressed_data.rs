use crate::{dao::compressed_data, rpc::CompressedData};
use sea_orm::DatabaseConnection;
use sea_orm::{entity::*, query::*, DbErr};

pub async fn get_compressed_data(
    db: &DatabaseConnection,
    tree_id: Vec<u8>,
    leaf_idx: u32,
) -> Result<CompressedData, DbErr> {
    // let sel = compressed_data::Entity::find()
    //     .filter(Expr::cust_with_values(
    //         "compressed_data.tree_id = $1::bytea",
    //         vec![tree_id],
    //     ))
    //     .filter(Expr::cust_with_values(
    //         "compressed_data.leaf_idx = $1::bigint",
    //         vec![leaf_idx],
    //     ));
    // let leaf: Option<compressed_data::ActiveModel> = sel.one(db).await?;
    // if leaf.is_none() {
    //     return Err(DbErr::RecordNotFound(
    //         "compressed_data Proof Not Found".to_string(),
    //     ));
    // }
    // Ok(leaf.unwrap())

    let found = compressed_data::Entity::find()
        .filter(compressed_data::Column::TreeId.eq(tree_id.to_vec()))
        .filter(compressed_data::Column::LeafIdx.eq(leaf_idx as i64))
        .one(db)
        .await?;

    if found.is_none() {
        return Err(DbErr::RecordNotFound(
            "compressed_data Not Found".to_string(),
        ));
    }

    let db_data = found.unwrap();

    Ok(CompressedData {
        id: db_data.id,
        tree_id: bs58::encode(db_data.tree_id).into_string(),
        leaf_idx: db_data.leaf_idx,
        parsed_data: db_data.parsed_data,
        slot_updated: db_data.slot_updated,
    })
}
