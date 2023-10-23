use crate::{
    error::IngesterError,
    program_transformers::bubblegum::{
        asset_was_decompressed, save_changelog_event, upsert_asset_data,
        upsert_asset_with_leaf_info, upsert_asset_with_royalty_amount, upsert_asset_with_seq,
        upsert_creators,
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
            LeafSchema::V1 { id, nonce, .. } => {
                let id_bytes = id.to_bytes();

                // First check to see if this asset has been decompressed and if so do not update.
                if asset_was_decompressed(txn, id_bytes.to_vec()).await? {
                    return Ok(None);
                }

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
                )
                .await?;

                upsert_asset_with_seq(txn, id_bytes.to_vec(), seq as i64).await?;

                // Update `asset_creators` table.

                // Delete any existing creators.
                asset_creators::Entity::delete_many()
                    .filter(
                        Condition::all().add(asset_creators::Column::AssetId.eq(id_bytes.to_vec())),
                    )
                    .exec(txn)
                    .await?;

                // Upsert into `asset_creators` table.
                let creators = if let Some(creators) = &update_args.creators {
                    creators
                } else {
                    &current_metadata.creators
                };
                upsert_creators(txn, id_bytes.to_vec(), creators, slot_i, seq as i64).await?;

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
