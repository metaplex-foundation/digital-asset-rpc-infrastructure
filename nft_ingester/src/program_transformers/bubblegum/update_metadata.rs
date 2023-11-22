use crate::{
    error::IngesterError,
    program_transformers::bubblegum::{
        asset_should_be_updated, save_changelog_event, unprotected_upsert_asset_authority,
        unprotected_upsert_asset_base_info, unprotected_upsert_asset_data,
        unprotected_upsert_asset_v1_account_attachments, unprotected_upsert_creators,
        upsert_asset_with_compression_info, upsert_asset_with_leaf_info,
        upsert_asset_with_owner_and_delegate_info, upsert_asset_with_seq,
        upsert_asset_with_update_metadata_seq, upsert_collection_info,
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
use digital_asset_types::dao::sea_orm_active_enums::{
    SpecificationAssetClass, SpecificationVersions,
};
use digital_asset_types::{
    dao::{
        asset_creators,
        sea_orm_active_enums::{ChainMutability, Mutability, OwnerType, RoyaltyTargetType},
    },
    json::ChainDataV1,
};
use log::warn;
use num_traits::FromPrimitive;
use sea_orm::{entity::*, query::*, ConnectionTrait, EntityTrait, JsonValue};

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
            LeafSchema::V1 {
                id,
                delegate,
                owner,
                nonce,
                ..
            } => {
                let id_bytes = id.to_bytes();

                // First check to see if this asset has been decompressed or updated by
                // `update_metadata`.
                if !asset_should_be_updated(txn, id_bytes.to_vec(), Some(seq as i64)).await? {
                    return Ok(None);
                }

                // Upsert into `asset_data` table.

                let slot_i = bundle.slot as i64;

                let uri = if let Some(uri) = &update_args.uri {
                    uri.replace('\0', "")
                } else {
                    current_metadata.uri.replace('\0', "")
                };

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

                unprotected_upsert_asset_data(
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
                )
                .await?;

                // Upsert into `asset` table.
                // Start a db transaction.
                let multi_txn = txn.begin().await?;

                // Set base mint info.
                let tree_id = bundle.keys.get(5).unwrap().0.to_vec();
                let seller_fee_basis_points =
                    if let Some(seller_fee_basis_points) = update_args.seller_fee_basis_points {
                        seller_fee_basis_points
                    } else {
                        current_metadata.seller_fee_basis_points
                    };

                unprotected_upsert_asset_base_info(
                    &multi_txn,
                    id_bytes.to_vec(),
                    OwnerType::Single,
                    false,
                    SpecificationVersions::V1,
                    SpecificationAssetClass::Nft,
                    RoyaltyTargetType::Creators,
                    None,
                    seller_fee_basis_points as i32,
                    slot_i,
                )
                .await?;

                // Partial update of asset table with just compression info elements.
                upsert_asset_with_compression_info(
                    &multi_txn,
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
                    &multi_txn,
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
                let delegate = if owner == delegate || delegate.to_bytes() == [0; 32] {
                    None
                } else {
                    Some(delegate.to_bytes().to_vec())
                };
                upsert_asset_with_owner_and_delegate_info(
                    &multi_txn,
                    id_bytes.to_vec(),
                    owner.to_bytes().to_vec(),
                    delegate,
                    seq as i64,
                )
                .await?;

                upsert_asset_with_seq(&multi_txn, id_bytes.to_vec(), seq as i64).await?;

                upsert_asset_with_update_metadata_seq(&multi_txn, id_bytes.to_vec(), seq as i64)
                    .await?;

                // Close out transaction and relinqish the lock.
                multi_txn.commit().await?;

                // Upsert into `asset_v1_account_attachments` table.
                let (edition_attachment_address, _) = find_master_edition_account(&id);
                unprotected_upsert_asset_v1_account_attachments(
                    txn,
                    edition_attachment_address.to_bytes().to_vec(),
                    slot_i,
                )
                .await?;

                // Update `asset_creators` table.

                // Delete any existing creators.
                asset_creators::Entity::delete_many()
                    .filter(
                        Condition::all().add(asset_creators::Column::AssetId.eq(id_bytes.to_vec())),
                    )
                    .exec(txn)
                    .await?;

                let creators = if let Some(creators) = &update_args.creators {
                    creators
                } else {
                    &current_metadata.creators
                };

                // Upsert into `asset_creators` table.
                unprotected_upsert_creators(txn, id_bytes.to_vec(), creators, slot_i, seq as i64)
                    .await?;

                // Insert into `asset_authority` table.
                //TODO - we need to remove the optional bubblegum signer logic
                let authority = bundle.keys.get(0).unwrap().0.to_vec();
                unprotected_upsert_asset_authority(
                    txn,
                    id_bytes.to_vec(),
                    authority,
                    seq as i64,
                    slot_i,
                )
                .await?;

                // Upsert into `asset_grouping` table with base collection info.
                upsert_collection_info(
                    txn,
                    id_bytes.to_vec(),
                    current_metadata.collection.clone(),
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
                    uri,
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
