use crate::{dao::compressed_data, rpc::CompressedData};
use sea_orm::{entity::*, query::*, DbErr};
use sea_orm::{DatabaseConnection, DbBackend};

pub async fn get_characters(
    db: &DatabaseConnection,
    wallet: String,
    merkle_tree: Option<Vec<u8>>,
) -> Result<Vec<CompressedData>, DbErr> {
    let mut query_builder = compressed_data::Entity::find();

    let merkle_tree_is_some = merkle_tree.is_some();
    if merkle_tree_is_some {
        query_builder =
            query_builder.filter(compressed_data::Column::TreeId.eq(merkle_tree.unwrap()));
    }

    let mut query = query_builder.build(DbBackend::Postgres);
    query.sql = format!(
        "{} {} parsed_data->>'owner' = 'pubkey:{}'",
        query.sql,
        if !merkle_tree_is_some { "WHERE" } else { "AND" },
        wallet
    );

    let models = compressed_data::Entity::find()
        .from_raw_sql(query)
        .all(db)
        .await?;

    let mut characters = Vec::new();

    for model in models {
        characters.push(CompressedData {
            id: model.id,
            tree_id: bs58::encode(model.tree_id).into_string(),
            leaf_idx: model.leaf_idx,
            schema_validated: model.schema_validated,
            parsed_data: model.parsed_data,
            slot_updated: model.slot_updated,
        })
    }

    Ok(characters)
}
