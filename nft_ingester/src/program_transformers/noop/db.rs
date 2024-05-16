use crate::error::IngesterError;
use anchor_lang::prelude::{
    borsh::{BorshDeserialize, BorshSerialize},
    Pubkey,
};
use digital_asset_types::dao::{character_history, compressed_data, merkle_tree};
use hpl_toolkit::{compression::*, schema::*};
use log::{debug, info};
use sea_orm::{
    query::*, sea_query::OnConflict, ActiveValue::Set, ColumnTrait, DbBackend, EntityTrait,
};
use solana_sdk::pubkey;
use spl_account_compression::events::ApplicationDataEventV1;
use std::str::FromStr;

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
    debug!("Inserting AppData");
    let buf = &mut &application_data.application_data[..];
    // @TODO: Ignore if it's not Honeycomb Compress data event instead of throwing error.
    let event = CompressedDataEvent::deserialize(buf)
        .map_err(|db_err| IngesterError::CompressedDataParseError(db_err.to_string()))?;
    debug!("Application data parsed successfully");
    match event {
        CompressedDataEvent::TreeSchemaValue {
            discriminator,
            tree_id,
            schema,
        } => {
            info!("Found new tree {}", bs58::encode(tree_id).into_string());

            let data_schema = schema
                .try_to_vec()
                .map_err(|db_err| IngesterError::CompressedDataParseError(db_err.to_string()))?;

            debug!("Parsed tree data schema");

            let item = merkle_tree::ActiveModel {
                id: Set(tree_id.to_vec()),
                data_schema: Set(data_schema),
                discriminator: Set(discriminator.to_vec()),
                ..Default::default()
            };

            let query = merkle_tree::Entity::insert(item)
                .on_conflict(
                    OnConflict::columns([merkle_tree::Column::Id])
                        .update_columns([merkle_tree::Column::DataSchema])
                        .to_owned(),
                )
                .build(DbBackend::Postgres);
            debug!("Query builed successfully");
            txn.execute(query)
                .await
                .map_err(|db_err| IngesterError::StorageWriteError(db_err.to_string()))?;
            debug!("Query executed successfully");
        }
        CompressedDataEvent::Leaf {
            slot,
            tree_id,
            leaf_idx,
            seq,
            stream_type,
        } => match stream_type {
            CompressedDataEventStream::Full { mut data } => {
                info!(
                    "Found new leaf for {} at index {}",
                    bs58::encode(tree_id).into_string(),
                    leaf_idx
                );
                let tree = merkle_tree::Entity::find_by_id(tree_id.to_vec())
                    .one(txn)
                    .await
                    .map_err(|db_err| IngesterError::StorageReadError(db_err.to_string()))?;

                debug!("Find tree query executed successfully");

                let mut schema_validated: bool = false;
                let mut program_id = None;
                if let Some(tree) = tree {
                    debug!("Parsing tree data schema");
                    let schema =
                        Schema::deserialize(&mut &tree.data_schema[..]).map_err(|db_err| {
                            IngesterError::CompressedDataParseError(db_err.to_string())
                        })?;
                    if tree.program.is_none() {
                        return Err(IngesterError::CompressedDataParseError(format!(
                            "Tree program not found"
                        )));
                    }
                    program_id = Some(Pubkey::try_from(tree.program.unwrap()).unwrap());
                    debug!("Parsed tree data schema");
                    if !schema.validate(&mut data) {
                        return Err(IngesterError::CompressedDataParseError(format!(
                            "Schema value validation failed for data: {} with schema: {}",
                            data.to_string(),
                            schema.to_string()
                        ))
                        .into());
                    }

                    schema_validated = true;
                }

                // if let SchemaValue::Object(object) = data {
                //     if let Some(used_by) = object.get("used_by") {
                //         update_character_history(used_by)?;
                //     }
                // }

                debug!("Serializing raw data");
                let raw_data = data.try_to_vec().map_err(|db_err| {
                    IngesterError::CompressedDataParseError(db_err.to_string())
                })?;
                debug!("Serialized raw data");

                let item = compressed_data::ActiveModel {
                    tree_id: Set(tree_id.to_vec()),
                    leaf_idx: Set(leaf_idx as i64),
                    seq: Set(seq as i64),
                    schema_validated: Set(schema_validated),
                    raw_data: Set(raw_data),
                    parsed_data: Set(data.clone().into()),
                    slot_updated: Set(slot as i64),
                    ..Default::default()
                };

                let query = compressed_data::Entity::insert(item)
                    .on_conflict(
                        OnConflict::columns([
                            compressed_data::Column::TreeId,
                            compressed_data::Column::LeafIdx,
                        ])
                        .update_columns([
                            compressed_data::Column::TreeId,
                            compressed_data::Column::LeafIdx,
                            compressed_data::Column::Seq,
                            compressed_data::Column::SchemaValidated,
                            compressed_data::Column::RawData,
                            compressed_data::Column::ParsedData,
                            compressed_data::Column::SlotUpdated,
                        ])
                        .to_owned(),
                    )
                    .build(DbBackend::Postgres);

                debug!("Query builed successfully");
                txn.execute(query)
                    .await
                    .map_err(|db_err| IngesterError::StorageWriteError(db_err.to_string()))?;
                debug!("Query executed successfully");

                let found_latest_entry = compressed_data::Entity::find()
                    .filter(compressed_data::Column::TreeId.eq(tree_id.to_vec()))
                    .filter(compressed_data::Column::LeafIdx.eq(leaf_idx as i64))
                    .one(txn)
                    .await
                    .map_err(|db_err| IngesterError::StorageReadError(db_err.to_string()))?;
                debug!("Find the latest entry data query executed successfully");

                if found_latest_entry.is_none() {
                    return Err(IngesterError::StorageReadError(
                        "Could not find the latest entry data in db".to_string(),
                    ));
                }
                let db_data: compressed_data::ActiveModel = found_latest_entry.unwrap().into();
                debug!("Found new_data {:?}", db_data);

                if let Some(program_id) = program_id {
                    if program_id
                        == Pubkey::from_str("ChRCtrG7X5kb9YncA4wuyD68DXXL8Szt3zBCCGiioBTg").unwrap()
                    {
                        new_character_event(
                            txn,
                            program_id,
                            db_data.id.unwrap() as u64,
                            data,
                            ("NewCharacter").to_string(),
                            slot as i64,
                            // Some(false),
                        )
                        .await?;
                    }
                }
            }
            CompressedDataEventStream::PatchChunk { key, mut data } => {
                info!(
                    "Patch leaf for {} at index {}",
                    bs58::encode(tree_id).into_string(),
                    leaf_idx
                );

                let tree = merkle_tree::Entity::find_by_id(tree_id.to_vec())
                    .one(txn)
                    .await
                    .map_err(|db_err| IngesterError::StorageReadError(db_err.to_string()))?;

                debug!("Find tree query executed successfully");

                let mut program_id: Option<Pubkey> = None;
                if let Some(tree) = tree {
                    debug!("Parsing tree data schema");
                    let schema =
                        Schema::deserialize(&mut &tree.data_schema[..]).map_err(|db_err| {
                            IngesterError::CompressedDataParseError(db_err.to_string())
                        })?;
                    program_id = Some(Pubkey::try_from(tree.program.unwrap()).unwrap());
                    debug!("Parsed tree data schema");
                    if !schema.validate(&mut data) {
                        return Err(IngesterError::CompressedDataParseError(format!(
                            "Schema value validation failed for data: {} with schema: {}",
                            data.to_string(),
                            schema.to_string()
                        ))
                        .into());
                    }
                }

                let found = compressed_data::Entity::find()
                    .filter(compressed_data::Column::TreeId.eq(tree_id.to_vec()))
                    .filter(compressed_data::Column::LeafIdx.eq(leaf_idx as i64))
                    .one(txn)
                    .await
                    .map_err(|db_err| IngesterError::StorageReadError(db_err.to_string()))?;

                debug!("Find old_data query executed successfully");

                if found.is_none() {
                    return Err(IngesterError::StorageReadError(
                        "Could not find old data in db".to_string(),
                    ));
                }

                let mut db_data: compressed_data::ActiveModel = found.unwrap().into();
                debug!("Found old_data {:?}", db_data);

                debug!("Wrapped model into ActiveModel");

                let mut parsed_data: JsonValue = db_data.parsed_data.take().unwrap();

                if let JsonValue::Object(object) = &mut parsed_data {
                    if object.contains_key(&key) {
                        debug!("Patching {}: {:?}", key, data.to_string());
                        object.insert(key, data.to_owned().into());
                    }
                }

                debug!("Complete Data After Patch: {}", parsed_data.to_string());
                db_data.parsed_data = Set(parsed_data);
                debug!("Data updated in object");

                let query: Statement = compressed_data::Entity::update(db_data.clone())
                    .filter(compressed_data::Column::TreeId.eq(tree_id.to_vec()))
                    .filter(compressed_data::Column::LeafIdx.eq(leaf_idx as i64))
                    .build(DbBackend::Postgres);

                debug!(
                    "Query builed successfully, {}, values {:#?}",
                    query.sql, query.values
                );
                txn.execute(query)
                    .await
                    .map_err(|db_err| IngesterError::StorageWriteError(db_err.to_string()))?;

                debug!("Query executed successfully");
                if let Some(program_id) = program_id {
                    let character_program_id =
                        pubkey!("ChRCtrG7X5kb9YncA4wuyD68DXXL8Szt3zBCCGiioBTg");
                    match program_id {
                        character_program_id => {
                            let mut event: Option<String> = None;
                            if let SchemaValue::Object(new_character) = &data {
                                if let Some(SchemaValue::String(new_used_by)) =
                                    new_character.get("used_by")
                                {
                                    if let SchemaValue::String(pre_used_by) =
                                        db_data.parsed_data.unwrap().into()
                                    {
                                        let staking: String = "staking".to_string();
                                        let mission: String = "mission".to_string();

                                        if pre_used_by.is_empty()
                                            && new_used_by.to_string() == staking
                                        {
                                            event = Some("staked".to_string());
                                        } else if pre_used_by == staking
                                            && new_used_by.to_string() == staking
                                        {
                                            event = Some("claimed_staking".to_string());
                                        } else if pre_used_by == staking && new_used_by.is_empty() {
                                            event = Some("unstake".to_string());
                                        } else if pre_used_by.is_empty()
                                            && new_used_by.to_string() == mission
                                        {
                                            event = Some("mission_started".to_string());
                                        } else if pre_used_by == mission
                                            && new_used_by.to_string() == mission
                                        {
                                            event = Some("claimed_mission".to_string());
                                        } else if pre_used_by == mission && new_used_by.is_empty() {
                                            event = Some("recall_from_mission".to_string());
                                        }
                                    }
                                }
                            }
                            if let Some(event) = event {
                                new_character_event(
                                    txn,
                                    program_id,
                                    db_data.id.unwrap() as u64,
                                    data,
                                    event,
                                    slot as i64,
                                    // Some(true),
                                )
                                .await?;
                            }
                        }
                    }
                }
            }
            CompressedDataEventStream::Empty => {
                info!(
                    "Remove leaf for {} at index {}",
                    bs58::encode(tree_id).into_string(),
                    leaf_idx
                );
                let found = compressed_data::Entity::find()
                    .filter(compressed_data::Column::TreeId.eq(tree_id.to_vec()))
                    .filter(compressed_data::Column::LeafIdx.eq(leaf_idx as i64))
                    .one(txn)
                    .await
                    .map_err(|db_err| IngesterError::StorageReadError(db_err.to_string()))?;

                debug!("Find old_data query executed successfully");

                if found.is_none() {
                    return Err(IngesterError::StorageReadError(
                        "Could not find old data in db".to_string(),
                    ));
                }

                let db_data: compressed_data::ActiveModel = found.unwrap().into();
                debug!("Found old_data {:?}", db_data);

                let query: Statement = compressed_data::Entity::delete(db_data)
                    .filter(compressed_data::Column::TreeId.eq(tree_id.to_vec()))
                    .filter(compressed_data::Column::LeafIdx.eq(leaf_idx as i64))
                    .build(DbBackend::Postgres);

                debug!(
                    "Query builed successfully, {}, values {:#?}",
                    query.sql, query.values
                );
                txn.execute(query)
                    .await
                    .map_err(|db_err| IngesterError::StorageWriteError(db_err.to_string()))?;
                debug!("Query executed successfully");
            }
        },
    }
    Ok(())
}

// pub async fn update_character_history(
//     tree_id: [u8; 32],
//     leaf_idx: u32,
//     used_by: SchemaValue,
// ) -> Result<(), IngesterError> {
//     Ok(())
// }

pub async fn new_character_event<T>(
    txn: &T,
    program_id: Pubkey,
    character_id: u64,
    data: SchemaValue,
    event: String,
    slot: i64,
    // fetch_history: Option<bool>,
) -> Result<(), IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    // let fetch_history = fetch_history.unwrap_or(true);

    // if fetch_history {}
    // IF the Program ID is character id
    if let SchemaValue::Object(character) = data {
        if let Some(used_by) = character.get("used_by") {
            let new_history = character_history::ActiveModel {
                event: Set(event), //Set(("NewCharacter").to_string()),
                event_data: Set(used_by.clone().into()),
                character_id: Set(character_id),
                slot_updated: Set(slot),
                ..Default::default()
            };
            let query = character_history::Entity::insert(new_history)
                .on_conflict(
                    OnConflict::columns([character_history::Column::Id])
                        .update_columns([character_history::Column::CharacterId])
                        .to_owned(),
                )
                .build(DbBackend::Postgres);

            debug!("Query builed successfully for character_history");
            txn.execute(query)
                .await
                .map_err(|db_err| IngesterError::StorageWriteError(db_err.to_string()))?;
            debug!("Query executed successfully for character_history");
        }
    }

    Ok(())
}
