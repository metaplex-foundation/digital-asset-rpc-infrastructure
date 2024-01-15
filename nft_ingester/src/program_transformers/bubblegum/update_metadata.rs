use crate::{
    error::IngesterError,
    program_transformers::bubblegum::{
        save_changelog_event, upsert_asset_base_info, upsert_asset_creators, upsert_asset_data,
        upsert_asset_with_leaf_info, upsert_asset_with_seq,
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
    dao::sea_orm_active_enums::{
        ChainMutability, Mutability, OwnerType, RoyaltyTargetType, SpecificationAssetClass,
        SpecificationVersions,
    },
    json::ChainDataV1,
};
use log::warn;
use num_traits::FromPrimitive;
use sea_orm::{query::*, ConnectionTrait, JsonValue};

pub async fn update_metadata<'c, T>(
    parsing_result: &BubblegumInstruction,
    bundle: &InstructionBundle<'c>,
    txn: &'c T,
    instruction: &str,
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
            tree_id,
        }),
    ) = (
        &parsing_result.leaf_update,
        &parsing_result.tree_update,
        &parsing_result.payload,
    ) {
        let seq = save_changelog_event(cl, bundle.slot, bundle.txn_id, txn, instruction, cl_audits)
            .await?;

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

                // Upsert `asset` table base info.
                let seller_fee_basis_points =
                    if let Some(seller_fee_basis_points) = update_args.seller_fee_basis_points {
                        seller_fee_basis_points
                    } else {
                        current_metadata.seller_fee_basis_points
                    };

                let creators = if let Some(creators) = &update_args.creators {
                    creators
                } else {
                    &current_metadata.creators
                };

                // Begin a transaction.  If the transaction goes out of scope (i.e. one of the executions has
                // an error and this function returns it using the `?` operator), then the transaction is
                // automatically rolled back.
                let multi_txn = txn.begin().await?;

                upsert_asset_base_info(
                    txn,
                    id_bytes.to_vec(),
                    OwnerType::Single,
                    false,
                    SpecificationVersions::V1,
                    SpecificationAssetClass::Nft,
                    RoyaltyTargetType::Creators,
                    None,
                    seller_fee_basis_points as i32,
                    slot_i,
                    seq as i64,
                )
                .await?;

                // Partial update of asset table with just leaf.
                upsert_asset_with_leaf_info(
                    &multi_txn,
                    id_bytes.to_vec(),
                    nonce as i64,
                    tree_id.to_vec(),
                    le.leaf_hash.to_vec(),
                    le.schema.data_hash(),
                    le.schema.creator_hash(),
                    seq as i64,
                )
                .await?;

                upsert_asset_with_seq(&multi_txn, id_bytes.to_vec(), seq as i64).await?;

                multi_txn.commit().await?;

                // Upsert creators to `asset_creators` table.
                upsert_asset_creators(txn, id_bytes.to_vec(), creators, slot_i, seq as i64).await?;

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
