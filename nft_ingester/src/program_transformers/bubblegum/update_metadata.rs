use crate::{
    error::IngesterError,
    program_transformers::bubblegum::{
        save_changelog_event, upsert_asset_data, upsert_asset_with_leaf_info,
        upsert_asset_with_royalty_amount, upsert_asset_with_seq,
    },
    tasks::{DownloadMetadata, IntoTaskData, TaskData},
};
use blockbuster::{
    instruction::InstructionBundle,
    programs::bubblegum::{BubblegumInstruction, LeafSchema, Payload},
    token_metadata::state::{TokenStandard, UseMethod, Uses},
};
use chrono::Utc;
use digital_asset_types::{
    dao::{
        asset_creators,
        sea_orm_active_enums::{ChainMutability, Mutability},
    },
    json::ChainDataV1,
};
use log::warn;
use num_traits::FromPrimitive;
use sea_orm::{
    entity::*, query::*, sea_query::OnConflict, ConnectionTrait, DbBackend, EntityTrait, JsonValue,
};
use std::collections::HashSet;

pub async fn update_metadata<'c, T>(
    parsing_result: &BubblegumInstruction,
    bundle: &InstructionBundle<'c>,
    txn: &'c T,
    cl_audits: bool,
) -> Result<Option<TaskData>, IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    if let (
        Some(le),
        Some(cl),
        Some(Payload::UpdateMetadata {
            current_metadata,
            update_args,
        }),
    ) = (
        &parsing_result.leaf_update,
        &parsing_result.tree_update,
        &parsing_result.payload,
    ) {
        let seq = save_changelog_event(cl, bundle.slot, bundle.txn_id, txn, cl_audits).await?;
        #[allow(unreachable_patterns)]
        return match le.schema {
            LeafSchema::V1 { id, nonce, .. } => {
                let id_bytes = id.to_bytes();
                let slot_i = bundle.slot as i64;

                let uri = if let Some(uri) = &update_args.uri {
                    uri.replace('\0', "")
                } else {
                    current_metadata.uri.replace('\0', "")
                };
                if uri.is_empty() {
                    return Err(IngesterError::DeserializationError(
                        "URI is empty".to_string(),
                    ));
                }

                let name = if let Some(name) = update_args.name.clone() {
                    name
                } else {
                    current_metadata.name.clone()
                };

                let symbol = if let Some(symbol) = update_args.symbol.clone() {
                    symbol
                } else {
                    current_metadata.symbol.clone()
                };

                let primary_sale_happened =
                    if let Some(primary_sale_happened) = update_args.primary_sale_happened {
                        primary_sale_happened
                    } else {
                        current_metadata.primary_sale_happened
                    };

                let mut chain_data = ChainDataV1 {
                    name: name.clone(),
                    symbol: symbol.clone(),
                    edition_nonce: current_metadata.edition_nonce,
                    primary_sale_happened,
                    token_standard: Some(TokenStandard::NonFungible),
                    uses: current_metadata.uses.clone().map(|u| Uses {
                        use_method: UseMethod::from_u8(u.use_method as u8).unwrap(),
                        remaining: u.remaining,
                        total: u.total,
                    }),
                };
                chain_data.sanitize();
                let chain_data_json = serde_json::to_value(chain_data)
                    .map_err(|e| IngesterError::DeserializationError(e.to_string()))?;

                let is_mutable = if let Some(is_mutable) = update_args.is_mutable {
                    is_mutable
                } else {
                    current_metadata.is_mutable
                };

                let chain_mutability = if is_mutable {
                    ChainMutability::Mutable
                } else {
                    ChainMutability::Immutable
                };

                upsert_asset_data(
                    txn,
                    id_bytes.to_vec(),
                    chain_mutability,
                    chain_data_json,
                    uri.clone(),
                    Mutability::Mutable,
                    JsonValue::String("processing".to_string()),
                    slot_i,
                    Some(true),
                    name.into_bytes().to_vec(),
                    symbol.into_bytes().to_vec(),
                    seq as i64,
                )
                .await?;

                // Partial update of asset table with just royalty amount (seller fee basis points).
                let seller_fee_basis_points =
                    if let Some(seller_fee_basis_points) = update_args.seller_fee_basis_points {
                        seller_fee_basis_points
                    } else {
                        current_metadata.seller_fee_basis_points
                    };

                upsert_asset_with_royalty_amount(
                    txn,
                    id_bytes.to_vec(),
                    seller_fee_basis_points as i32,
                    seq as i64,
                )
                .await?;

                // Partial update of asset table with just leaf.
                let tree_id = bundle.keys.get(5).unwrap().0.to_vec();
                upsert_asset_with_leaf_info(
                    txn,
                    id_bytes.to_vec(),
                    nonce as i64,
                    tree_id,
                    le.leaf_hash.to_vec(),
                    le.schema.data_hash(),
                    le.schema.creator_hash(),
                    seq as i64,
                    false,
                )
                .await?;

                upsert_asset_with_seq(txn, id_bytes.to_vec(), seq as i64).await?;

                // Update `asset_creators` table.
                if let Some(creators) = &update_args.creators {
                    // Vec to hold base creator information.
                    let mut db_creator_infos = Vec::with_capacity(creators.len());

                    // Vec to hold info on whether a creator is verified.  This info is protected by `seq` number.
                    let mut db_creator_verified_infos = Vec::with_capacity(creators.len());

                    // Set to prevent duplicates.
                    let mut creators_set = HashSet::new();

                    for (i, c) in creators.iter().enumerate() {
                        if creators_set.contains(&c.address) {
                            continue;
                        }

                        db_creator_infos.push(asset_creators::ActiveModel {
                            asset_id: Set(id_bytes.to_vec()),
                            creator: Set(c.address.to_bytes().to_vec()),
                            position: Set(i as i16),
                            share: Set(c.share as i32),
                            slot_updated: Set(Some(slot_i)),
                            ..Default::default()
                        });

                        db_creator_verified_infos.push(asset_creators::ActiveModel {
                            asset_id: Set(id_bytes.to_vec()),
                            creator: Set(c.address.to_bytes().to_vec()),
                            verified: Set(c.verified),
                            seq: Set(Some(seq as i64)),
                            ..Default::default()
                        });

                        creators_set.insert(c.address);
                    }

                    // Remove creators no longer present in creator array.
                    let db_creators_to_remove: Vec<Vec<u8>> = current_metadata
                        .creators
                        .iter()
                        .filter(|c| !creators_set.contains(&c.address))
                        .map(|c| c.address.to_bytes().to_vec())
                        .collect();

                    asset_creators::Entity::delete_many()
                        .filter(
                            Condition::all()
                                .add(asset_creators::Column::AssetId.eq(id_bytes.to_vec()))
                                .add(asset_creators::Column::Creator.is_in(db_creators_to_remove))
                                // TODO WHAT IF SEQ IS NULL
                                .add(asset_creators::Column::Seq.lt(seq as i64)),
                        )
                        .exec(txn)
                        .await?;

                    // This statement will update base information for each creator.
                    let query = asset_creators::Entity::insert_many(db_creator_infos)
                        .on_conflict(
                            OnConflict::columns([
                                asset_creators::Column::AssetId,
                                asset_creators::Column::Creator,
                            ])
                            .update_columns([
                                asset_creators::Column::Position,
                                asset_creators::Column::Share,
                                asset_creators::Column::SlotUpdated,
                            ])
                            .to_owned(),
                        )
                        .build(DbBackend::Postgres);
                    txn.execute(query).await?;

                    // This statement will update whether the creator is verified and the `seq`
                    // number.  `seq` is used to protect the `verified` field, allowing for `mint`
                    // and `verifyCreator` to be processed out of order.
                    let mut query = asset_creators::Entity::insert_many(db_creator_verified_infos)
                        .on_conflict(
                            OnConflict::columns([
                                asset_creators::Column::AssetId,
                                asset_creators::Column::Creator,
                            ])
                            .update_columns([
                                asset_creators::Column::Verified,
                                asset_creators::Column::Seq,
                            ])
                            .to_owned(),
                        )
                        .build(DbBackend::Postgres);
                    query.sql = format!(
                        "{} WHERE excluded.seq > asset_creators.seq OR asset_creators.seq IS NULL",
                        query.sql
                    );
                    txn.execute(query).await?;
                }

                if uri.is_empty() {
                    warn!(
                        "URI is empty for mint {}. Skipping background task.",
                        bs58::encode(id).into_string()
                    );
                    return Ok(None);
                }

                // TODO DEAL WITH TASKS
                let mut task = DownloadMetadata {
                    asset_data_id: id_bytes.to_vec(),
                    uri,
                    created_at: Some(Utc::now().naive_utc()),
                };
                task.sanitize();
                let t = task.into_task_data()?;
                Ok(Some(t))
            }
            _ => Err(IngesterError::NotImplemented),
        };
    }
    Err(IngesterError::ParsingError(
        "Ix not parsed correctly".to_string(),
    ))
}
