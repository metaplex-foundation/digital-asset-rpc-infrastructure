use {
    crate::{
        bubblegum::{
            bgum_use_method_to_token_metadata_use_method,
            db::{
                save_changelog_event, upsert_asset_authority, upsert_asset_base_info,
                upsert_asset_creators, upsert_asset_data, upsert_asset_with_compression_info,
                upsert_asset_with_leaf_info, upsert_asset_with_owner_and_delegate_info,
                upsert_asset_with_seq, upsert_collection_info,
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
    sea_orm::{ConnectionTrait, TransactionTrait},
    tracing::warn,
};

pub async fn mint<'c, T>(
    parsing_result: &BubblegumInstruction,
    bundle: &InstructionBundle<'c>,
    txn: &'c T,
    instruction: &str,
) -> ProgramTransformerResult<Option<DownloadMetadataInfo>>
where
    T: ConnectionTrait + TransactionTrait,
{
    if let (
        Some(le),
        Some(cl),
        Some(Payload::Mint {
            args,
            authority,
            tree_id,
        }),
    ) = (
        &parsing_result.leaf_update,
        &parsing_result.tree_update,
        &parsing_result.payload,
    ) {
        let seq = save_changelog_event(cl, bundle.slot, bundle.txn_id, txn, instruction).await?;
        let metadata = args;

        let leaf = NormalizedLeafFields::from(&le.schema);

        let id_bytes = leaf.id.to_bytes();
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
                use_method: bgum_use_method_to_token_metadata_use_method(u.use_method),
                remaining: u.remaining,
                total: u.total,
            }),
        };
        chain_data.sanitize();
        let chain_data_json = serde_json::to_value(chain_data)
            .map_err(|e| ProgramTransformerError::DeserializationError(e.to_string()))?;
        let chain_mutability = match metadata.is_mutable {
            true => ChainMutability::Mutable,
            false => ChainMutability::Immutable,
        };

        // Begin a transaction.  If the transaction goes out of scope (i.e. one of the executions has
        // an error and this function returns it using the `?` operator), then the transaction is
        // automatically rolled back.
        let multi_txn = txn.begin().await?;

        upsert_asset_data(
            &multi_txn,
            id_bytes.to_vec(),
            chain_mutability,
            chain_data_json,
            uri.clone(),
            Mutability::Mutable,
            slot_i,
            name.to_vec(),
            symbol.to_vec(),
            seq as i64,
        )
        .await?;

        // Upsert `asset` table base info.
        let delegate = if leaf.owner == leaf.delegate || leaf.delegate.to_bytes() == [0; 32] {
            None
        } else {
            Some(leaf.delegate.to_bytes().to_vec())
        };

        // Upsert `asset` table base info and `asset_creators` table.
        upsert_asset_base_info(
            &multi_txn,
            id_bytes.to_vec(),
            OwnerType::Single,
            SpecificationVersions::V1,
            SpecificationAssetClass::Nft,
            RoyaltyTargetType::Creators,
            None,
            metadata.seller_fee_basis_points as i32,
            slot_i,
            seq as i64,
        )
        .await?;

        // Partial update of asset table with just compression info elements.
        upsert_asset_with_compression_info(&multi_txn, id_bytes.to_vec(), true, false, 1, None)
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

        // Partial update of asset table with just leaf owner and delegate.
        upsert_asset_with_owner_and_delegate_info(
            &multi_txn,
            id_bytes.to_vec(),
            leaf.owner.to_bytes().to_vec(),
            delegate,
            seq as i64,
        )
        .await?;

        upsert_asset_with_seq(&multi_txn, id_bytes.to_vec(), seq as i64).await?;

        // Upsert creators to `asset_creators` table.
        upsert_asset_creators(
            &multi_txn,
            id_bytes.to_vec(),
            &metadata.creators,
            slot_i,
            seq as i64,
        )
        .await?;

        // Insert into `asset_authority` table.
        //TODO - we need to remove the optional bubblegum signer logic
        upsert_asset_authority(
            &multi_txn,
            id_bytes.to_vec(),
            authority.to_bytes().to_vec(),
            seq as i64,
            slot_i,
        )
        .await?;

        // Upsert into `asset_grouping` table with base collection info.
        upsert_collection_info(
            &multi_txn,
            id_bytes.to_vec(),
            metadata.collection.clone(),
            slot_i,
            seq as i64,
        )
        .await?;

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
