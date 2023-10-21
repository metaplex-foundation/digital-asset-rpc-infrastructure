use crate::{
    error::IngesterError,
    program_transformers::bubblegum::{
        asset_was_decompressed, save_changelog_event, upsert_asset_data,
        upsert_asset_with_compression_info, upsert_asset_with_leaf_info,
        upsert_asset_with_owner_and_delegate_info, upsert_asset_with_royalty_amount,
        upsert_asset_with_seq, upsert_collection_info,
    },
    tasks::{DownloadMetadata, IntoTaskData, TaskData},
};
use blockbuster::{
    instruction::InstructionBundle,
    programs::bubblegum::{BubblegumInstruction, LeafSchema, Payload},
    token_metadata::{
        pda::find_master_edition_account,
        state::{TokenStandard, UseMethod, Uses},
    },
};
use chrono::Utc;
use digital_asset_types::{
    dao::{
        asset, asset_authority, asset_creators, asset_v1_account_attachments,
        sea_orm_active_enums::{ChainMutability, Mutability, OwnerType, RoyaltyTargetType},
    },
    json::ChainDataV1,
};
use log::warn;
use num_traits::FromPrimitive;
use sea_orm::{
    entity::*, query::*, sea_query::OnConflict, ConnectionTrait, DbBackend, EntityTrait, JsonValue,
};
use std::collections::HashSet;

use digital_asset_types::dao::sea_orm_active_enums::{
    SpecificationAssetClass, SpecificationVersions, V1AccountAttachments,
};

// TODO -> consider moving structs into these functions to avoid clone

pub async fn mint_v1<'c, T>(
    parsing_result: &BubblegumInstruction,
    bundle: &InstructionBundle<'c>,
    txn: &'c T,
    cl_audits: bool,
) -> Result<Option<TaskData>, IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    if let (Some(le), Some(cl), Some(Payload::MintV1 { args })) = (
        &parsing_result.leaf_update,
        &parsing_result.tree_update,
        &parsing_result.payload,
    ) {
        let seq = save_changelog_event(cl, bundle.slot, bundle.txn_id, txn, cl_audits).await?;
        let metadata = args;
        #[allow(unreachable_patterns)]
        return match le.schema {
            LeafSchema::V1 {
                id,
                delegate,
                owner,
                nonce,
                ..
            } => {
                let (edition_attachment_address, _) = find_master_edition_account(&id);
                let id_bytes = id.to_bytes();

                // First check to see if this asset has been decompressed and if so do not update.
                if asset_was_decompressed(txn, id_bytes.to_vec()).await? {
                    return Ok(None);
                }

                let slot_i = bundle.slot as i64;
                let uri = metadata.uri.replace('\0', "");
                let name = metadata.name.clone().into_bytes();
                let symbol = metadata.symbol.clone().into_bytes();
                let mut chain_data = ChainDataV1 {
                    name: metadata.name.clone(),
                    symbol: metadata.symbol.clone(),
                    edition_nonce: metadata.edition_nonce,
                    primary_sale_happened: metadata.primary_sale_happened,
                    token_standard: Some(TokenStandard::NonFungible),
                    uses: metadata.uses.clone().map(|u| Uses {
                        use_method: UseMethod::from_u8(u.use_method as u8).unwrap(),
                        remaining: u.remaining,
                        total: u.total,
                    }),
                };
                chain_data.sanitize();
                let chain_data_json = serde_json::to_value(chain_data)
                    .map_err(|e| IngesterError::DeserializationError(e.to_string()))?;
                let chain_mutability = match metadata.is_mutable {
                    true => ChainMutability::Mutable,
                    false => ChainMutability::Immutable,
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
                    name.to_vec(),
                    symbol.to_vec(),
                    seq as i64,
                )
                .await?;

                // Insert into `asset` table.
                let delegate = if owner == delegate || delegate.to_bytes() == [0; 32] {
                    None
                } else {
                    Some(delegate.to_bytes().to_vec())
                };
                let tree_id = bundle.keys.get(3).unwrap().0.to_vec();

                // Set initial mint info.
                let asset_model = asset::ActiveModel {
                    id: Set(id_bytes.to_vec()),
                    owner_type: Set(OwnerType::Single),
                    frozen: Set(false),
                    tree_id: Set(Some(tree_id.clone())),
                    specification_version: Set(Some(SpecificationVersions::V1)),
                    specification_asset_class: Set(Some(SpecificationAssetClass::Nft)),
                    nonce: Set(Some(nonce as i64)),
                    royalty_target_type: Set(RoyaltyTargetType::Creators),
                    royalty_target: Set(None),
                    asset_data: Set(Some(id_bytes.to_vec())),
                    slot_updated: Set(Some(slot_i)),
                    ..Default::default()
                };

                // Upsert asset table base info.
                let query = asset::Entity::insert(asset_model)
                    .on_conflict(
                        OnConflict::columns([asset::Column::Id])
                            .update_columns([
                                asset::Column::OwnerType,
                                asset::Column::Frozen,
                                asset::Column::SpecificationVersion,
                                asset::Column::SpecificationAssetClass,
                                asset::Column::RoyaltyTargetType,
                                asset::Column::RoyaltyTarget,
                                asset::Column::AssetData,
                            ])
                            .to_owned(),
                    )
                    .build(DbBackend::Postgres);

                txn.execute(query)
                    .await
                    .map_err(|db_err| IngesterError::AssetIndexError(db_err.to_string()))?;

                upsert_asset_with_royalty_amount(
                    txn,
                    id_bytes.to_vec(),
                    metadata.seller_fee_basis_points as i32,
                    seq as i64,
                )
                .await?;

                // Partial update of asset table with just compression info elements.
                upsert_asset_with_compression_info(
                    txn,
                    id_bytes.to_vec(),
                    true,
                    false,
                    1,
                    None,
                    false,
                )
                .await?;

                // Partial update of asset table with just leaf.
                upsert_asset_with_leaf_info(
                    txn,
                    id_bytes.to_vec(),
                    nonce as i64,
                    tree_id,
                    le.leaf_hash.to_vec(),
                    le.schema.data_hash(),
                    le.schema.creator_hash(),
                    seq as i64,
                )
                .await?;

                // Partial update of asset table with just leaf owner and delegate.
                upsert_asset_with_owner_and_delegate_info(
                    txn,
                    id_bytes.to_vec(),
                    owner.to_bytes().to_vec(),
                    delegate,
                    seq as i64,
                )
                .await?;

                upsert_asset_with_seq(txn, id_bytes.to_vec(), seq as i64).await?;

                let attachment = asset_v1_account_attachments::ActiveModel {
                    id: Set(edition_attachment_address.to_bytes().to_vec()),
                    slot_updated: Set(slot_i),
                    attachment_type: Set(V1AccountAttachments::MasterEditionV2),
                    ..Default::default()
                };

                let query = asset_v1_account_attachments::Entity::insert(attachment)
                    .on_conflict(
                        OnConflict::columns([asset_v1_account_attachments::Column::Id])
                            .do_nothing()
                            .to_owned(),
                    )
                    .build(DbBackend::Postgres);
                txn.execute(query)
                    .await
                    .map_err(|db_err| IngesterError::AssetIndexError(db_err.to_string()))?;

                // Insert into `asset_creators` table.
                let creators = &metadata.creators;
                if !creators.is_empty() {
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
                            base_info_seq: Set(Some(seq as i64)),
                            ..Default::default()
                        });

                        db_creator_verified_infos.push(asset_creators::ActiveModel {
                            asset_id: Set(id_bytes.to_vec()),
                            creator: Set(c.address.to_bytes().to_vec()),
                            verified: Set(c.verified),
                            verified_seq: Set(Some(seq as i64)),
                            ..Default::default()
                        });

                        creators_set.insert(c.address);
                    }

                    // This statement will update base information for each creator.
                    let mut query = asset_creators::Entity::insert_many(db_creator_infos)
                        .on_conflict(
                            OnConflict::columns([
                                asset_creators::Column::AssetId,
                                asset_creators::Column::Creator,
                            ])
                            .update_columns([
                                asset_creators::Column::Position,
                                asset_creators::Column::Share,
                                asset_creators::Column::SlotUpdated,
                                asset_creators::Column::BaseInfoSeq,
                            ])
                            .to_owned(),
                        )
                        .build(DbBackend::Postgres);
                    query.sql = format!(
                        "{} WHERE excluded.base_info_seq >= asset_creators.base_info_seq OR asset_creators.base_info_seq IS NULL",
                        query.sql
                    );
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
                                asset_creators::Column::VerifiedSeq,
                            ])
                            .to_owned(),
                        )
                        .build(DbBackend::Postgres);
                    query.sql = format!(
                        "{} WHERE excluded.verified_seq >= asset_creators.verified_seq OR asset_creators.verified_seq IS NULL",
                        query.sql
                    );
                    txn.execute(query).await?;
                }

                // Insert into `asset_authority` table.
                let model = asset_authority::ActiveModel {
                    asset_id: Set(id_bytes.to_vec()),
                    authority: Set(bundle.keys.get(0).unwrap().0.to_vec()), //TODO - we need to rem,ove the optional bubblegum signer logic
                    seq: Set(seq as i64),
                    slot_updated: Set(slot_i),
                    ..Default::default()
                };

                // Do not attempt to modify any existing values:
                // `ON CONFLICT ('asset_id') DO NOTHING`.
                let query = asset_authority::Entity::insert(model)
                    .on_conflict(
                        OnConflict::columns([asset_authority::Column::AssetId])
                            .do_nothing()
                            .to_owned(),
                    )
                    .build(DbBackend::Postgres);
                txn.execute(query)
                    .await
                    .map_err(|db_err| IngesterError::AssetIndexError(db_err.to_string()))?;

                // Upsert into `asset_grouping` table with base collection info.
                upsert_collection_info(
                    txn,
                    id_bytes.to_vec(),
                    metadata.collection.clone(),
                    slot_i,
                    seq as i64,
                )
                .await?;

                if uri.is_empty() {
                    warn!(
                        "URI is empty for mint {}. Skipping background task.",
                        bs58::encode(id).into_string()
                    );
                    return Ok(None);
                }

                let mut task = DownloadMetadata {
                    asset_data_id: id_bytes.to_vec(),
                    uri: metadata.uri.clone(),
                    seq: seq as i64,
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
