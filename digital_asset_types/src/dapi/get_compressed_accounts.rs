use crate::{
    dao::{compressed_data, merkle_tree},
    rpc::CompressedData,
};
use sea_orm::{entity::*, query::*, DbErr};
use sea_orm::{DatabaseConnection, DbBackend};

pub async fn get_compressed_accounts(
    db: &DatabaseConnection,
    discriminator: Vec<u8>,
) -> Result<Vec<CompressedData>, DbErr> {
    // let discriminator_trees_query = format!(
    //     "SELECT id from merkle_tree WHERE encode(discriminator, 'base64') = '{}'",
    //     "kcvQ4LSL08LVfZ12sfjlGZjXLQR5h9dVRPQ2XhwbnAA="
    // );

    let query = compressed_data::Entity::find()
        .filter(
            compressed_data::Column::TreeId.in_subquery(
                merkle_tree::Entity::find()
                    .filter(merkle_tree::Column::Discriminator.eq(discriminator))
                    .into_query(),
            ),
        )
        .build(DbBackend::Postgres);

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
