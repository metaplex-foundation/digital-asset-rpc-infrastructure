use super::save_changelog_event;
use crate::{
    error::IngesterError,
    program_transformers::bubblegum::{
        upsert_asset_with_compression_info, upsert_asset_with_leaf_schema,
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
        asset, asset_authority, asset_creators, asset_data, asset_grouping,
        asset_v1_account_attachments,
        sea_orm_active_enums::{ChainMutability, Mutability, OwnerType, RoyaltyTargetType},
    },
    json::ChainDataV1,
};
use num_traits::FromPrimitive;
use sea_orm::{
    entity::*, query::*, sea_query::OnConflict, ConnectionTrait, DbBackend, EntityTrait, JsonValue,
};
use std::collections::HashSet;

use digital_asset_types::dao::sea_orm_active_enums::{
    SpecificationAssetClass, SpecificationVersions, V1AccountAttachments,
};
use mpl_bubblegum::{hash_creators, hash_metadata};

// TODO -> consider moving structs into these functions to avoid clone

pub async fn mint_v1<'c, T>(
    parsing_result: &BubblegumInstruction,
    bundle: &InstructionBundle<'c>,
    txn: &'c T,
) -> Result<TaskData, IngesterError>
where
    T: ConnectionTrait + TransactionTrait,
{
    if let (Some(le), Some(cl), Some(Payload::MintV1 { args })) = (
        &parsing_result.leaf_update,
        &parsing_result.tree_update,
        &parsing_result.payload,
    ) {
        let seq = save_changelog_event(cl, bundle.slot, txn).await?;
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
                let slot_i = bundle.slot as i64;
                let uri = metadata.uri.trim().replace('\0', "");
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
                if uri.is_empty() {
                    return Err(IngesterError::DeserializationError(
                        "URI is empty".to_string(),
                    ));
                }
                let data = asset_data::ActiveModel {
                    id: Set(id_bytes.to_vec()),
                    chain_data_mutability: Set(chain_mutability),
                    chain_data: Set(chain_data_json),
                    metadata_url: Set(uri),
                    metadata: Set(JsonValue::String("processing".to_string())),
                    metadata_mutability: Set(Mutability::Mutable),
                    slot_updated: Set(slot_i),
                };

                let mut query = asset_data::Entity::insert(data)
                    .on_conflict(
                        OnConflict::columns([asset_data::Column::Id])
                            .update_columns([
                                asset_data::Column::ChainDataMutability,
                                asset_data::Column::ChainData,
                                asset_data::Column::MetadataUrl,
                                asset_data::Column::Metadata,
                                asset_data::Column::MetadataMutability,
                                asset_data::Column::SlotUpdated,
                            ])
                            .to_owned(),
                    )
                    .build(DbBackend::Postgres);
                query.sql = format!(
                    "{} WHERE excluded.slot_updated > asset_data.slot_updated",
                    query.sql
                );
                txn.execute(query).await?;
                // Insert into `asset` table.
                let delegate = if owner == delegate {
                    None
                } else {
                    Some(delegate.to_bytes().to_vec())
                };
                let data_hash = hash_metadata(args)
                    .map(|e| bs58::encode(e).into_string())
                    .unwrap_or("".to_string())
                    .trim()
                    .to_string();
                let creator_hash = hash_creators(&args.creators)
                    .map(|e| bs58::encode(e).into_string())
                    .unwrap_or("".to_string())
                    .trim()
                    .to_string();

                let asset_model = asset::ActiveModel {
                    id: Set(id_bytes.to_vec()),
                    //owner: Set(Some(owner.to_bytes().to_vec())),
                    owner_type: Set(OwnerType::Single),
                    //delegate: Set(delegate),
                    frozen: Set(false),
                    //supply: Set(1),
                    //supply_mint: Set(None),
                    //compressed: Set(true),
                    tree_id: Set(Some(bundle.keys.get(3).unwrap().0.to_vec())),
                    specification_version: Set(Some(SpecificationVersions::V1)),
                    specification_asset_class: Set(Some(SpecificationAssetClass::Nft)),
                    nonce: Set(Some(nonce as i64)),
                    //leaf: Set(Some(le.leaf_hash.to_vec())),
                    royalty_target_type: Set(RoyaltyTargetType::Creators),
                    royalty_target: Set(None),
                    royalty_amount: Set(metadata.seller_fee_basis_points as i32), //basis points
                    asset_data: Set(Some(id_bytes.to_vec())),
                    //seq: Set(seq as i64), // gummyroll seq
                    slot_updated: Set(Some(slot_i)),
                    data_hash: Set(Some(data_hash)),
                    creator_hash: Set(Some(creator_hash)),
                    ..Default::default()
                };

                // Upsert asset table base info.
                let query = asset::Entity::insert(asset_model)
                    .on_conflict(
                        OnConflict::columns([asset::Column::Id])
                            .update_columns([
                                asset::Column::OwnerType,
                                asset::Column::Frozen,
                                asset::Column::Supply,
                                asset::Column::SupplyMint,
                                asset::Column::Compressed,
                                asset::Column::TreeId,
                                asset::Column::SpecificationVersion,
                                asset::Column::SpecificationAssetClass,
                                asset::Column::Nonce,
                                asset::Column::RoyaltyTargetType,
                                asset::Column::RoyaltyTarget,
                                asset::Column::RoyaltyAmount,
                                asset::Column::AssetData,
                                //TODO maybe handle slot updated differently.
                                asset::Column::SlotUpdated,
                                asset::Column::DataHash,
                                asset::Column::CreatorHash,
                            ])
                            .to_owned(),
                    )
                    .build(DbBackend::Postgres);
                txn.execute(query).await?;

                // Partial update of asset table with just compression info elements.
                upsert_asset_with_compression_info(
                    txn,
                    id_bytes.to_vec(),
                    true,
                    false,
                    1,
                    None,
                    seq as i64,
                )
                .await?;

                // Partial update of asset table with just leaf schema elements.
                upsert_asset_with_leaf_schema(
                    txn,
                    id_bytes.to_vec(),
                    le.leaf_hash.to_vec(),
                    delegate,
                    owner.to_bytes().to_vec(),
                    seq as i64,
                )
                .await?;

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
                txn.execute(query).await?;

                // Insert into `asset_creators` table.
                let creators = &metadata.creators;
                if !creators.is_empty() {
                    let mut db_creators = Vec::with_capacity(creators.len());
                    let mut creators_set = HashSet::new();
                    for (i, c) in creators.iter().enumerate() {
                        if creators_set.contains(&c.address) {
                            continue;
                        }
                        db_creators.push(asset_creators::ActiveModel {
                            asset_id: Set(id_bytes.to_vec()),
                            creator: Set(c.address.to_bytes().to_vec()),
                            share: Set(c.share as i32),
                            verified: Set(c.verified),
                            seq: Set(seq as i64), // do we need this here @micheal-danenberg?
                            slot_updated: Set(slot_i),
                            position: Set(i as i16),
                            ..Default::default()
                        });
                        creators_set.insert(c.address);
                    }

                    let query = asset_creators::Entity::insert_many(db_creators)
                        .on_conflict(
                            OnConflict::columns([
                                asset_creators::Column::AssetId,
                                asset_creators::Column::Position,
                            ])
                            .update_columns([
                                asset_creators::Column::Creator,
                                asset_creators::Column::Share,
                                asset_creators::Column::Verified,
                                asset_creators::Column::Seq,
                                asset_creators::Column::SlotUpdated,
                            ])
                            .to_owned(),
                        )
                        .build(DbBackend::Postgres);
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
                txn.execute(query).await?;

                // Insert into `asset_grouping` table.
                if let Some(c) = &metadata.collection {
                    if c.verified {
                        let model = asset_grouping::ActiveModel {
                            asset_id: Set(id_bytes.to_vec()),
                            group_key: Set("collection".to_string()),
                            group_value: Set(c.key.to_string()),
                            seq: Set(seq as i64), // gummyroll seq
                            slot_updated: Set(slot_i),
                            ..Default::default()
                        };

                        // Do not attempt to modify any existing values:
                        // `ON CONFLICT ('asset_id') DO NOTHING`.
                        let query = asset_grouping::Entity::insert(model)
                            .on_conflict(
                                OnConflict::columns([
                                    asset_grouping::Column::AssetId,
                                    asset_grouping::Column::GroupKey,
                                ])
                                .do_nothing()
                                .to_owned(),
                            )
                            .build(DbBackend::Postgres);
                        txn.execute(query).await?;
                    }
                }
                let mut task = DownloadMetadata {
                    asset_data_id: id_bytes.to_vec(),
                    uri: metadata.uri.clone(),
                    created_at: Some(Utc::now().naive_utc()),
                };
                task.sanitize();
                return task.into_task_data();
            }
            _ => Err(IngesterError::NotImplemented),
        }?;
    }
    Err(IngesterError::ParsingError(
        "Ix not parsed correctly".to_string(),
    ))
}
