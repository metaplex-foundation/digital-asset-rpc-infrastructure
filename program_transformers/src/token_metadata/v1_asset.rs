use {
    crate::{
        error::{ProgramTransformerError, ProgramTransformerResult},
        DownloadMetadataInfo,
    },
    blockbuster::token_metadata::{
        accounts::{MasterEdition, Metadata},
        types::TokenStandard,
    },
    digital_asset_types::{
        dao::{
            asset, asset_authority, asset_creators, asset_data, asset_grouping,
            asset_v1_account_attachments,
            sea_orm_active_enums::{
                ChainMutability, Mutability, OwnerType, RoyaltyTargetType, SpecificationAssetClass,
                SpecificationVersions, V1AccountAttachments,
            },
            token_accounts, tokens,
        },
        json::ChainDataV1,
    },
    plerkle_serialization::Pubkey as FBPubkey,
    sea_orm::{
        entity::{ActiveValue, ColumnTrait, EntityTrait},
        query::{JsonValue, Order, QueryFilter, QueryOrder, QueryTrait, Select},
        sea_query::query::OnConflict,
        ConnectionTrait, DbBackend, DbErr, TransactionTrait,
    },
    solana_sdk::{pubkey, pubkey::Pubkey},
    tokio::time::{sleep, Duration},
    tracing::warn,
};

pub async fn burn_v1_asset<T: ConnectionTrait + TransactionTrait>(
    conn: &T,
    id: FBPubkey,
    slot: u64,
) -> ProgramTransformerResult<()> {
    let (id, slot_i) = (id.0, slot as i64);
    let model = asset::ActiveModel {
        id: ActiveValue::Set(id.to_vec()),
        slot_updated: ActiveValue::Set(Some(slot_i)),
        burnt: ActiveValue::Set(true),
        ..Default::default()
    };
    let mut query = asset::Entity::insert(model)
        .on_conflict(
            OnConflict::columns([asset::Column::Id])
                .update_columns([asset::Column::SlotUpdated, asset::Column::Burnt])
                .to_owned(),
        )
        .build(DbBackend::Postgres);
    query.sql = format!(
        "{} WHERE excluded.slot_updated > asset.slot_updated",
        query.sql
    );
    conn.execute(query).await?;
    Ok(())
}

const RETRY_INTERVALS: &[u64] = &[0, 5, 10];
static WSOL_PUBKEY: Pubkey = pubkey!("So11111111111111111111111111111111111111112");

pub async fn save_v1_asset<T: ConnectionTrait + TransactionTrait>(
    conn: &T,
    metadata: &Metadata,
    slot: u64,
) -> ProgramTransformerResult<Option<DownloadMetadataInfo>> {
    let metadata = metadata.clone();
    let mint_pubkey = metadata.mint;
    let mint_pubkey_array = mint_pubkey.to_bytes();
    let mint_pubkey_vec = mint_pubkey_array.to_vec();

    let (edition_attachment_address, _) = MasterEdition::find_pda(&mint_pubkey);

    let authority = metadata.update_authority.to_bytes().to_vec();
    let slot_i = slot as i64;
    let uri = metadata.uri.trim().replace('\0', "");
    let _spec = SpecificationVersions::V1;
    let mut class = match metadata.token_standard {
        Some(TokenStandard::NonFungible) => SpecificationAssetClass::Nft,
        Some(TokenStandard::FungibleAsset) => SpecificationAssetClass::FungibleAsset,
        Some(TokenStandard::Fungible) => SpecificationAssetClass::FungibleToken,
        Some(TokenStandard::NonFungibleEdition) => SpecificationAssetClass::Nft,
        Some(TokenStandard::ProgrammableNonFungible) => SpecificationAssetClass::ProgrammableNft,
        Some(TokenStandard::ProgrammableNonFungibleEdition) => {
            SpecificationAssetClass::ProgrammableNft
        }
        _ => SpecificationAssetClass::Unknown,
    };
    let mut ownership_type = match class {
        SpecificationAssetClass::FungibleAsset => OwnerType::Token,
        SpecificationAssetClass::FungibleToken => OwnerType::Token,
        SpecificationAssetClass::Nft | SpecificationAssetClass::ProgrammableNft => {
            OwnerType::Single
        }
        _ => OwnerType::Unknown,
    };

    // Wrapped Solana is a special token that has supply 0 (infinite).
    // It's a fungible token with a metadata account, but without any token standard, meaning the code above will misabel it as an NFT.
    if mint_pubkey == WSOL_PUBKEY {
        ownership_type = OwnerType::Token;
        class = SpecificationAssetClass::FungibleToken;
    }

    // Gets the token and token account for the mint to populate the asset.
    // This is required when the token and token account are indexed, but not the metadata account.
    // If the metadata account is indexed, then the token and ta ingester will update the asset with the correct data.
    let token: Option<tokens::Model> = find_model_with_retry(
        conn,
        "token",
        &tokens::Entity::find_by_id(mint_pubkey_vec.clone()),
        RETRY_INTERVALS,
    )
    .await?;

    // get supply of token, default to 1 since most cases will be NFTs. Token mint ingester will properly set supply if token_result is None
    let (supply, supply_mint) = match token {
        Some(t) => (t.supply, Some(t.mint)),
        None => {
            warn!(
                target: "Account not found",
                "Token/Mint not found in 'tokens' table for mint {}",
                bs58::encode(&mint_pubkey_vec).into_string()
            );
            (1, None)
        }
    };

    // Map unknown ownership types based on the supply.
    if ownership_type == OwnerType::Unknown {
        ownership_type = match supply.cmp(&1) {
            std::cmp::Ordering::Equal => OwnerType::Single,
            std::cmp::Ordering::Greater => OwnerType::Token,
            _ => OwnerType::Unknown,
        }
    }

    let token_account: Option<token_accounts::Model> = match ownership_type {
        OwnerType::Single | OwnerType::Unknown => {
            // query for token account associated with mint with positive balance with latest slot
            let token_account: Option<token_accounts::Model> = find_model_with_retry(
                conn,
                "token_accounts",
                &token_accounts::Entity::find()
                    .filter(token_accounts::Column::Mint.eq(mint_pubkey_vec.clone()))
                    .filter(token_accounts::Column::Amount.gt(0))
                    .order_by(token_accounts::Column::SlotUpdated, Order::Desc),
                RETRY_INTERVALS,
            )
            .await
            .map_err(|e: DbErr| ProgramTransformerError::DatabaseError(e.to_string()))?;

            token_account
        }
        _ => None,
    };

    // owner and delegate should be from the token account with the mint
    let (owner, delegate) = match token_account {
        Some(ta) => (
            ActiveValue::Set(Some(ta.owner)),
            ActiveValue::Set(ta.delegate),
        ),
        None => {
            if supply == 1 && ownership_type == OwnerType::Single {
                warn!(
                    target: "Account not found",
                    "Token acc not found in 'token_accounts' table for mint {}",
                    bs58::encode(&mint_pubkey_vec).into_string()
                );
            }
            (ActiveValue::NotSet, ActiveValue::NotSet)
        }
    };

    let name = metadata.name.clone().into_bytes();
    let symbol = metadata.symbol.clone().into_bytes();
    let mut chain_data = ChainDataV1 {
        name: metadata.name.clone(),
        symbol: metadata.symbol.clone(),
        edition_nonce: metadata.edition_nonce,
        primary_sale_happened: metadata.primary_sale_happened,
        token_standard: metadata.token_standard,
        uses: metadata.uses,
    };
    chain_data.sanitize();
    let chain_data_json = serde_json::to_value(chain_data)
        .map_err(|e| ProgramTransformerError::DeserializationError(e.to_string()))?;
    let chain_mutability = match metadata.is_mutable {
        true => ChainMutability::Mutable,
        false => ChainMutability::Immutable,
    };
    let asset_data_model = asset_data::ActiveModel {
        chain_data_mutability: ActiveValue::Set(chain_mutability),
        chain_data: ActiveValue::Set(chain_data_json),
        metadata_url: ActiveValue::Set(uri.clone()),
        metadata: ActiveValue::Set(JsonValue::String("processing".to_string())),
        metadata_mutability: ActiveValue::Set(Mutability::Mutable),
        slot_updated: ActiveValue::Set(slot_i),
        reindex: ActiveValue::Set(Some(true)),
        id: ActiveValue::Set(mint_pubkey_vec.clone()),
        raw_name: ActiveValue::Set(Some(name.to_vec())),
        raw_symbol: ActiveValue::Set(Some(symbol.to_vec())),
        base_info_seq: ActiveValue::Set(Some(0)),
    };
    let txn = conn.begin().await?;
    let mut query = asset_data::Entity::insert(asset_data_model)
        .on_conflict(
            OnConflict::columns([asset_data::Column::Id])
                .update_columns([
                    asset_data::Column::ChainDataMutability,
                    asset_data::Column::ChainData,
                    asset_data::Column::MetadataUrl,
                    asset_data::Column::MetadataMutability,
                    asset_data::Column::SlotUpdated,
                    asset_data::Column::Reindex,
                    asset_data::Column::RawName,
                    asset_data::Column::RawSymbol,
                    asset_data::Column::BaseInfoSeq,
                ])
                .to_owned(),
        )
        .build(DbBackend::Postgres);
    query.sql = format!(
        "{} WHERE excluded.slot_updated > asset_data.slot_updated",
        query.sql
    );
    txn.execute(query)
        .await
        .map_err(|db_err| ProgramTransformerError::AssetIndexError(db_err.to_string()))?;

    let model = asset::ActiveModel {
        id: ActiveValue::Set(mint_pubkey_vec.clone()),
        owner,
        owner_type: ActiveValue::Set(ownership_type),
        delegate,
        frozen: ActiveValue::Set(false),
        supply: ActiveValue::Set(supply),
        supply_mint: ActiveValue::Set(supply_mint),
        specification_version: ActiveValue::Set(Some(SpecificationVersions::V1)),
        specification_asset_class: ActiveValue::Set(Some(class)),
        tree_id: ActiveValue::Set(None),
        nonce: ActiveValue::Set(Some(0)),
        seq: ActiveValue::Set(Some(0)),
        leaf: ActiveValue::Set(None),
        data_hash: ActiveValue::Set(None),
        creator_hash: ActiveValue::Set(None),
        compressed: ActiveValue::Set(false),
        compressible: ActiveValue::Set(false),
        royalty_target_type: ActiveValue::Set(RoyaltyTargetType::Creators),
        royalty_target: ActiveValue::Set(None),
        royalty_amount: ActiveValue::Set(metadata.seller_fee_basis_points as i32), //basis points
        asset_data: ActiveValue::Set(Some(mint_pubkey_vec.clone())),
        slot_updated: ActiveValue::Set(Some(slot_i)),
        burnt: ActiveValue::Set(false),
        ..Default::default()
    };
    let mut query = asset::Entity::insert(model)
        .on_conflict(
            OnConflict::columns([asset::Column::Id])
                .update_columns([
                    asset::Column::Owner,
                    asset::Column::OwnerType,
                    asset::Column::Delegate,
                    asset::Column::Frozen,
                    asset::Column::Supply,
                    asset::Column::SupplyMint,
                    asset::Column::SpecificationVersion,
                    asset::Column::SpecificationAssetClass,
                    asset::Column::TreeId,
                    asset::Column::Nonce,
                    asset::Column::Seq,
                    asset::Column::Leaf,
                    asset::Column::DataHash,
                    asset::Column::CreatorHash,
                    asset::Column::Compressed,
                    asset::Column::Compressible,
                    asset::Column::RoyaltyTargetType,
                    asset::Column::RoyaltyTarget,
                    asset::Column::RoyaltyAmount,
                    asset::Column::AssetData,
                    asset::Column::SlotUpdated,
                    asset::Column::Burnt,
                ])
                .to_owned(),
        )
        .build(DbBackend::Postgres);
    query.sql = format!(
        "{} WHERE excluded.slot_updated >= asset.slot_updated OR asset.slot_updated IS NULL",
        query.sql
    );
    txn.execute(query)
        .await
        .map_err(|db_err| ProgramTransformerError::AssetIndexError(db_err.to_string()))?;

    let attachment = asset_v1_account_attachments::ActiveModel {
        id: ActiveValue::Set(edition_attachment_address.to_bytes().to_vec()),
        slot_updated: ActiveValue::Set(slot_i),
        attachment_type: ActiveValue::Set(V1AccountAttachments::MasterEditionV2),
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
        .map_err(|db_err| ProgramTransformerError::AssetIndexError(db_err.to_string()))?;

    let model = asset_authority::ActiveModel {
        asset_id: ActiveValue::Set(mint_pubkey_vec.clone()),
        authority: ActiveValue::Set(authority),
        seq: ActiveValue::Set(0),
        slot_updated: ActiveValue::Set(slot_i),
        ..Default::default()
    };
    let mut query = asset_authority::Entity::insert(model)
        .on_conflict(
            OnConflict::columns([asset_authority::Column::AssetId])
                .update_columns([
                    asset_authority::Column::Authority,
                    asset_authority::Column::Seq,
                    asset_authority::Column::SlotUpdated,
                ])
                .to_owned(),
        )
        .build(DbBackend::Postgres);
    query.sql = format!(
        "{} WHERE excluded.slot_updated > asset_authority.slot_updated",
        query.sql
    );
    txn.execute(query)
        .await
        .map_err(|db_err| ProgramTransformerError::AssetIndexError(db_err.to_string()))?;

    if let Some(c) = &metadata.collection {
        let model = asset_grouping::ActiveModel {
            asset_id: ActiveValue::Set(mint_pubkey_vec.clone()),
            group_key: ActiveValue::Set("collection".to_string()),
            group_value: ActiveValue::Set(Some(c.key.to_string())),
            verified: ActiveValue::Set(c.verified),
            group_info_seq: ActiveValue::Set(Some(0)),
            slot_updated: ActiveValue::Set(Some(slot_i)),
            ..Default::default()
        };
        let mut query = asset_grouping::Entity::insert(model)
            .on_conflict(
                OnConflict::columns([
                    asset_grouping::Column::AssetId,
                    asset_grouping::Column::GroupKey,
                ])
                .update_columns([
                    asset_grouping::Column::GroupValue,
                    asset_grouping::Column::Verified,
                    asset_grouping::Column::SlotUpdated,
                    asset_grouping::Column::GroupInfoSeq,
                ])
                .to_owned(),
            )
            .build(DbBackend::Postgres);
        query.sql = format!(
            "{} WHERE excluded.slot_updated > asset_grouping.slot_updated",
            query.sql
        );
        txn.execute(query)
            .await
            .map_err(|db_err| ProgramTransformerError::AssetIndexError(db_err.to_string()))?;
    }

    let creators = metadata
        .creators
        .unwrap_or_default()
        .iter()
        .enumerate()
        .map(|(i, creator)| asset_creators::ActiveModel {
            asset_id: ActiveValue::Set(mint_pubkey_vec.clone()),
            position: ActiveValue::Set(i as i16),
            creator: ActiveValue::Set(creator.address.to_bytes().to_vec()),
            share: ActiveValue::Set(creator.share as i32),
            verified: ActiveValue::Set(creator.verified),
            slot_updated: ActiveValue::Set(Some(slot_i)),
            seq: ActiveValue::Set(Some(0)),
            ..Default::default()
        })
        .collect::<Vec<_>>();

    if !creators.is_empty() {
        let mut query = asset_creators::Entity::insert_many(creators)
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
        query.sql = format!(
                "{} WHERE excluded.slot_updated >= asset_creators.slot_updated OR asset_creators.slot_updated is NULL",
                query.sql
            );
        txn.execute(query)
            .await
            .map_err(|db_err| ProgramTransformerError::AssetIndexError(db_err.to_string()))?;
    }
    txn.commit().await?;

    if uri.is_empty() {
        warn!(
            "URI is empty for mint {}. Skipping background task.",
            bs58::encode(mint_pubkey_vec).into_string()
        );
        return Ok(None);
    }

    Ok(Some(DownloadMetadataInfo::new(mint_pubkey_vec, uri)))
}

async fn find_model_with_retry<T: ConnectionTrait + TransactionTrait, K: EntityTrait>(
    conn: &T,
    _model_name: &str,
    select: &Select<K>,
    retry_intervals: &[u64],
) -> Result<Option<K::Model>, DbErr> {
    // let mut retries = 0;
    // let metric_name = format!("{}_found", model_name);

    for interval in retry_intervals {
        let interval_duration = Duration::from_millis(*interval);
        sleep(interval_duration).await;

        let model = select.clone().one(conn).await?;
        if let Some(m) = model {
            // record_metric(&metric_name, true, retries);
            return Ok(Some(m));
        }
        // retries += 1;
    }

    // record_metric(&metric_name, false, retries - 1);
    Ok(None)
}

// fn record_metric(metric_name: &str, success: bool, retries: u32) {
//     let retry_count = &retries.to_string();
//     let success = if success { "true" } else { "false" };
//     metric! {
//         statsd_count!(metric_name, 1, "success" => success, "retry_count" => retry_count);
//     }
// }