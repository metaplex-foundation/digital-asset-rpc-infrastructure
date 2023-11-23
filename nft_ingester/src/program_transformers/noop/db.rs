use crate::error::IngesterError;
use anchor_lang::prelude::borsh::{BorshDeserialize, BorshSerialize};
use digital_asset_types::dao::{compressed_data, merkle_tree};
use hpl_compression::{CompressedDataEvent, CompressedDataEventStream, Schema, SchemaValue};
// use log::{debug, info};
use sea_orm::{
    query::*, sea_query::OnConflict, ActiveValue::Set, ColumnTrait, DbBackend, EntityTrait,
};
use spl_account_compression::events::ApplicationDataEventV1;

pub async fn save_applicationdata_event<'c, T>(
    application_data: &ApplicationDataEventV1,
    txn: &T,
) -> Result<u64, IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    insert_application_data(application_data, txn).await?;
    Ok(0)
}

pub async fn insert_application_data<'c, T>(
    application_data: &ApplicationDataEventV1,
    txn: &T,
) -> Result<(), IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    let event = CompressedDataEvent::deserialize(&mut &application_data.application_data[..])?;
    match event {
        CompressedDataEvent::TreeSchemaValue { tree_id, schema } => {
            let item = merkle_tree::ActiveModel {
                id: Set(tree_id.to_vec()),
                data_schema: Set(schema.try_to_vec().unwrap()),
                ..Default::default()
            };

            let query = merkle_tree::Entity::insert(item)
                .on_conflict(
                    OnConflict::columns([merkle_tree::Column::Id, merkle_tree::Column::DataSchema])
                        .update_columns([merkle_tree::Column::Id, merkle_tree::Column::DataSchema])
                        .to_owned(),
                )
                .build(DbBackend::Postgres);
            txn.execute(query)
                .await
                .map_err(|db_err| IngesterError::StorageWriteError(db_err.to_string()))?;
        }
        CompressedDataEvent::Leaf {
            slot,
            tree_id,
            leaf_idx,
            seq,
            stream_type,
        } => match stream_type {
            CompressedDataEventStream::Full { data } => {
                let tree = merkle_tree::Entity::find_by_id(tree_id.to_vec())
                    .one(txn)
                    .await?
                    .unwrap();
                let schema = Schema::deserialize(&mut &tree.data_schema[..])?;

                if !schema.validate(&data) {
                    return Err(IngesterError::CompressedDataParseError.into());
                }

                let item = compressed_data::ActiveModel {
                    tree_id: Set(tree_id.to_vec()),
                    leaf_idx: Set(leaf_idx as i64),
                    seq: Set(seq as i64),
                    raw_data: Set(data.try_to_vec().unwrap()),
                    parsed_data: Set(data.into()),
                    slot_updated: Set(slot as i64),
                    ..Default::default()
                };

                let query = compressed_data::Entity::insert(item)
                    .on_conflict(
                        OnConflict::columns([
                            compressed_data::Column::TreeId,
                            compressed_data::Column::LeafIdx,
                            compressed_data::Column::Seq,
                            compressed_data::Column::RawData,
                            compressed_data::Column::ParsedData,
                            compressed_data::Column::SlotUpdated,
                        ])
                        .update_columns([
                            compressed_data::Column::TreeId,
                            compressed_data::Column::LeafIdx,
                            compressed_data::Column::Seq,
                            compressed_data::Column::RawData,
                            compressed_data::Column::ParsedData,
                            compressed_data::Column::SlotUpdated,
                        ])
                        .to_owned(),
                    )
                    .build(DbBackend::Postgres);
                txn.execute(query)
                    .await
                    .map_err(|db_err| IngesterError::StorageWriteError(db_err.to_string()))?;
            }
            CompressedDataEventStream::PatchChunk { key, data } => {
                let mut db_data: compressed_data::ActiveModel = compressed_data::Entity::find()
                    .filter(compressed_data::Column::TreeId.eq(tree_id.to_vec()))
                    .filter(compressed_data::Column::LeafIdx.eq(leaf_idx as i64))
                    .one(txn)
                    .await?
                    .unwrap()
                    .into();

                let mut schema_value: SchemaValue = db_data.parsed_data.take().unwrap().into();
                if let SchemaValue::Object(object) = &mut schema_value {
                    if let Some(SchemaValue::Object(v1_map)) = object.get_mut("V1") {
                        v1_map.insert(key, data);
                    }
                }

                db_data.raw_data = Set(schema_value.try_to_vec()?);
                db_data.parsed_data = Set(schema_value.into());

                let query = compressed_data::Entity::update(db_data)
                    .filter(compressed_data::Column::TreeId.eq(tree_id.to_vec()))
                    .filter(compressed_data::Column::LeafIdx.eq(leaf_idx as i64))
                    .build(DbBackend::Postgres);
                txn.execute(query)
                    .await
                    .map_err(|db_err| IngesterError::StorageWriteError(db_err.to_string()))?;
            }
        },
    }
    Ok(())
}
