use crate::error::IngesterError;
use anchor_lang::prelude::borsh::{BorshDeserialize, BorshSerialize};
use digital_asset_types::dao::{compressed_data, compressed_data_changelog, merkle_tree};
use hpl_toolkit::prelude::*;
use log::{debug, info};
use sea_orm::{
    query::*, sea_query::OnConflict, ActiveValue::Set, ColumnTrait, DbBackend, EntityTrait,
};
use spl_account_compression::events::ApplicationDataEventV1;

async fn exec_query<'c, T: ConnectionTrait + TransactionTrait>(
    txn: &T,
    query: Statement,
) -> Result<(), IngesterError> {
    debug!(
        "Query builed successfully, {}, values {:#?}",
        query.sql, query.values
    );
    txn.execute(query)
        .await
        .map_err(|db_err| IngesterError::StorageWriteError(db_err.to_string()))?;
    debug!("Query executed successfully");
    Ok(())
}

pub async fn save_applicationdata_event<'c, T>(
    application_data: &ApplicationDataEventV1,
    txn: &T,
) -> Result<u64, IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    handle_application_data(application_data, txn).await?;
    Ok(0)
}

pub async fn handle_application_data<'c, T>(
    application_data: &ApplicationDataEventV1,
    txn: &T,
) -> Result<(), IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    debug!("Inserting AppData");
    let buf = &mut &application_data.application_data[..];
    let event = CompressedDataEvent::deserialize(buf)
        .map_err(|db_err| IngesterError::CompressedDataParseError(db_err.to_string()))?;
    debug!("Application data parsed successfully");
    match event {
        CompressedDataEvent::TreeSchemaValue {
            discriminator,
            tree_id,
            schema,
            program_id,
        } => handle_tree(txn, discriminator, tree_id, schema, program_id).await?,
        CompressedDataEvent::Leaf {
            slot,
            tree_id,
            leaf_idx,
            seq,
            stream_type,
        } => handle_leaf(txn, tree_id, leaf_idx, stream_type, seq, slot).await?,
    }
    Ok(())
}

async fn handle_tree<'c, T: ConnectionTrait + TransactionTrait>(
    txn: &T,
    discriminator: [u8; 32],
    tree_id: [u8; 32],
    schema: Schema,
    program_id: [u8; 32],
) -> Result<(), IngesterError> {
    info!("Found new tree {}", bs58::encode(tree_id).into_string());
    // @TODO: Fetch and store, maxDepth, maxBufferSize, canopyDepth, etc...
    let data_schema = schema
        .try_to_vec()
        .map_err(|db_err| IngesterError::CompressedDataParseError(db_err.to_string()))?;

    debug!("Parsed tree data schema");

    let item = merkle_tree::ActiveModel {
        id: Set(tree_id.to_vec()),
        data_schema: Set(data_schema),
        discriminator: Set(discriminator.to_vec()),
        program: Set(Some(program_id.to_vec())),
        ..Default::default()
    };

    let query = merkle_tree::Entity::insert(item)
        .on_conflict(
            OnConflict::columns([merkle_tree::Column::Id])
                .update_columns([merkle_tree::Column::DataSchema])
                .to_owned(),
        )
        .build(DbBackend::Postgres);
    exec_query(txn, query).await
}

async fn handle_leaf<'c, T: ConnectionTrait + TransactionTrait>(
    txn: &T,
    tree_id: [u8; 32],
    leaf_idx: u32,
    stream_type: CompressedDataEventStream,
    seq: u64,
    slot: u64,
) -> Result<(), IngesterError> {
    let compressed_data_id = anchor_lang::solana_program::keccak::hashv(
        &[&tree_id[..], &leaf_idx.to_le_bytes()[..]][..],
    )
    .to_bytes()
    .to_vec();
    let patch_key: Option<String>;
    let patch_data: Option<SchemaValue>;

    match stream_type {
        CompressedDataEventStream::Full { data } => {
            patch_key = None;
            patch_data = Some(data.clone());
            handle_full_leaf(
                txn,
                compressed_data_id.clone(),
                tree_id,
                leaf_idx,
                data,
                seq,
                slot,
            )
            .await?;
        }
        CompressedDataEventStream::PatchChunk { key, data } => {
            patch_key = Some(key.clone());
            patch_data = Some(data.clone());
            handle_leaf_patch(
                txn,
                compressed_data_id.clone(),
                tree_id,
                leaf_idx,
                key,
                data,
                seq,
                slot,
            )
            .await?;
        }
        CompressedDataEventStream::Empty => {
            patch_key = None;
            patch_data = None;
            handle_empty_leaf(
                txn,
                compressed_data_id.clone(),
                tree_id,
                leaf_idx,
                seq,
                slot,
            )
            .await?;
        }
    }

    if let Some(data) = patch_data {
        handle_change_log(txn, compressed_data_id, patch_key, data, seq, slot).await?;
    }

    Ok(())
}

async fn handle_full_leaf<'c, T: ConnectionTrait + TransactionTrait>(
    txn: &T,
    id: Vec<u8>,
    tree_id: [u8; 32],
    leaf_idx: u32,
    mut data: SchemaValue,
    seq: u64,
    slot: u64,
) -> Result<(), IngesterError> {
    let tree = merkle_tree::Entity::find_by_id(tree_id.to_vec())
        .one(txn)
        .await
        .map_err(|db_err| IngesterError::StorageReadError(db_err.to_string()))?;

    debug!("Find tree query executed successfully");

    let mut schema_validated: bool = false;
    if let Some(tree) = tree {
        debug!("Parsing tree data schema");
        let schema = Schema::deserialize(&mut &tree.data_schema[..])
            .map_err(|db_err| IngesterError::CompressedDataParseError(db_err.to_string()))?;

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

    debug!("Serializing raw data");
    let raw_data = data
        .try_to_vec()
        .map_err(|db_err| IngesterError::CompressedDataParseError(db_err.to_string()))?;
    debug!("Serialized raw data");

    let item = compressed_data::ActiveModel {
        id: Set(id),
        tree_id: Set(tree_id.to_vec()),
        leaf_idx: Set(leaf_idx as i64),
        seq: Set(seq as i64),
        schema_validated: Set(schema_validated),
        raw_data: Set(raw_data),
        parsed_data: Set(data.into()),
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
    exec_query(txn, query).await
}

async fn handle_leaf_patch<'c, T: ConnectionTrait + TransactionTrait>(
    txn: &T,
    id: Vec<u8>,
    tree_id: [u8; 32],
    leaf_idx: u32,
    key: String,
    data: SchemaValue,
    _seq: u64,
    _slot: u64,
) -> Result<(), IngesterError> {
    info!(
        "Patch leaf for {} at index {}",
        bs58::encode(tree_id).into_string(),
        leaf_idx
    );
    let found = compressed_data::Entity::find()
        .filter(compressed_data::Column::Id.eq(id.to_owned()))
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
            object.insert(key.clone(), data.clone().into());
        }
    }

    debug!("Complete Data After Patch: {}", parsed_data.to_string());
    db_data.parsed_data = Set(parsed_data);
    debug!("Data updated in object");

    let query: Statement = compressed_data::Entity::update(db_data)
        .filter(compressed_data::Column::Id.eq(id))
        .build(DbBackend::Postgres);
    exec_query(txn, query).await
}

async fn handle_empty_leaf<'c, T: ConnectionTrait + TransactionTrait>(
    txn: &T,
    id: Vec<u8>,
    tree_id: [u8; 32],
    leaf_idx: u32,
    _seq: u64,
    _slot: u64,
) -> Result<(), IngesterError> {
    info!(
        "Remove leaf for {} at index {}",
        bs58::encode(tree_id).into_string(),
        leaf_idx
    );
    let found = compressed_data::Entity::find()
        .filter(compressed_data::Column::Id.eq(id.clone()))
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
        .filter(compressed_data::Column::Id.eq(id))
        .build(DbBackend::Postgres);
    exec_query(txn, query).await
}

async fn handle_change_log<'c, T: ConnectionTrait + TransactionTrait>(
    txn: &T,
    compressed_data_id: Vec<u8>,
    key: Option<String>,
    data: SchemaValue,
    seq: u64,
    slot: u64,
) -> Result<(), IngesterError> {
    let change_log = compressed_data_changelog::ActiveModel {
        compressed_data_id: Set(compressed_data_id),
        key: Set(key),
        data: Set(data.into()),
        seq: Set(seq),
        slot_updated: Set(slot as i64),
        ..Default::default()
    };

    let query = compressed_data_changelog::Entity::insert(change_log).build(DbBackend::Postgres);
    exec_query(txn, query).await
}
