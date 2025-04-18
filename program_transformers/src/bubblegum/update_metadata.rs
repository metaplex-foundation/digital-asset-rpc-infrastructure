use {
    crate::{
        bubblegum::{
            bgum_use_method_to_token_metadata_use_method,
            db::{
                save_changelog_event, upsert_asset_base_info, upsert_asset_creators,
                upsert_asset_data, upsert_asset_with_leaf_info, upsert_asset_with_seq,
            },
            NormalizedLeafFields,
        },
        error::{ProgramTransformerError, ProgramTransformerResult},
        DownloadMetadataInfo,
    },
    blockbuster::{
        instruction::InstructionBundle,
        programs::bubblegum::{BubblegumInstruction, Payload},
        token_metadata::types::{TokenStandard, Uses},
    },
    digital_asset_types::{
        dao::sea_orm_active_enums::{
            ChainMutability, Mutability, OwnerType, RoyaltyTargetType, SpecificationAssetClass,
            SpecificationVersions,
        },
        json::ChainDataV1,
    },
    sea_orm::{query::*, ConnectionTrait, JsonValue},
    tracing::warn,
};

pub async fn update_metadata<'c, T>(
    parsing_result: &BubblegumInstruction,
    bundle: &InstructionBundle<'c>,
    txn: &'c T,
    instruction: &str,
) -> ProgramTransformerResult<Option<DownloadMetadataInfo>>
where
    T: ConnectionTrait + TransactionTrait,
{
    // Begin a transaction.  If the transaction goes out of scope (i.e. one of the executions has
    // an error and this function returns it using the `?` operator), then the transaction is
    // automatically rolled back.
    let multi_txn = txn.begin().await?;

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
        let seq = save_changelog_event(cl, bundle.slot, bundle.txn_id, txn, instruction).await?;

        let leaf = NormalizedLeafFields::from(&le.schema);

        let id_bytes = leaf.id.to_bytes();
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
                use_method: bgum_use_method_to_token_metadata_use_method(u.use_method),
                remaining: u.remaining,
                total: u.total,
            }),
        };
        chain_data.sanitize();
        let chain_data_json = serde_json::to_value(chain_data)
            .map_err(|e| ProgramTransformerError::DeserializationError(e.to_string()))?;

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
            &multi_txn,
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

        upsert_asset_base_info(
            &multi_txn,
            id_bytes.to_vec(),
            OwnerType::Single,
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
            leaf.nonce as i64,
            tree_id.to_bytes().to_vec(),
            le.leaf_hash.to_vec(),
            leaf.data_hash,
            leaf.creator_hash,
            leaf.collection_hash,
            leaf.asset_data_hash,
            leaf.flags,
            seq as i64,
        )
        .await?;

        upsert_asset_with_seq(&multi_txn, id_bytes.to_vec(), seq as i64).await?;

        // Upsert creators to `asset_creators` table.
        upsert_asset_creators(&multi_txn, id_bytes.to_vec(), creators, slot_i, seq as i64).await?;

        multi_txn.commit().await?;

        if uri.is_empty() {
            warn!(
                "URI is empty for mint {}. Skipping background task.",
                bs58::encode(leaf.id).into_string()
            );
            return Ok(None);
        }

        return Ok(Some(DownloadMetadataInfo::new(id_bytes.to_vec(), uri)));
    }
    Err(ProgramTransformerError::ParsingError(
        "Ix not parsed correctly".to_string(),
    ))
}
