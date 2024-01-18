use crate::tasks::{DownloadMetadata, IntoTaskData};
use crate::{error::IngesterError, metric, tasks::TaskData};
use blockbuster::token_metadata::{
    pda::find_master_edition_account,
    state::{Metadata, TokenStandard, UseMethod, Uses},
};
use cadence_macros::{is_global_default_set, statsd_count};
use chrono::Utc;
use digital_asset_types::dao::{asset_authority, asset_data, asset_grouping, token_accounts};
use digital_asset_types::{
    dao::{
        asset, asset_creators, asset_v1_account_attachments,
        sea_orm_active_enums::{
            ChainMutability, Mutability, OwnerType, RoyaltyTargetType, SpecificationAssetClass,
            SpecificationVersions, V1AccountAttachments,
        },
        tokens,
    },
    json::ChainDataV1,
};
use lazy_static::lazy_static;
use log::warn;
use num_traits::FromPrimitive;
use plerkle_serialization::Pubkey as FBPubkey;
use sea_orm::{
    entity::*, query::*, sea_query::OnConflict, ActiveValue::Set, ConnectionTrait, DbBackend,
    DbErr, EntityTrait, JsonValue,
};
use solana_sdk::pubkey::Pubkey;
use std::collections::HashSet;
use std::str::FromStr;
use std::time::Duration;
use tokio::time::sleep;

pub async fn burn_v1_asset<T: ConnectionTrait + TransactionTrait>(
    conn: &T,
    id: FBPubkey,
    slot: u64,
) -> Result<(), IngesterError> {
    let (id, slot_i) = (id.0, slot as i64);
    let model = asset::ActiveModel {
        id: Set(id.to_vec()),
        slot_updated: Set(Some(slot_i)),
        burnt: Set(true),
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
const WSOL_ADDRESS: &str = "So11111111111111111111111111111111111111112";

lazy_static! {
    static ref WSOL_PUBKEY: Pubkey =
        Pubkey::from_str(WSOL_ADDRESS).expect("Invalid public key format");
}

pub async fn save_v1_asset<T: ConnectionTrait + TransactionTrait>(
    conn: &T,
    metadata: &Metadata,
    slot: u64,
) -> Result<Option<TaskData>, IngesterError> {
    let metadata = metadata.clone();
    let data = metadata.data;
    let mint_pubkey = metadata.mint;
    let mint_pubkey_array = mint_pubkey.to_bytes();
    let mint_pubkey_vec = mint_pubkey_array.to_vec();

    let (edition_attachment_address, _) = find_master_edition_account(&mint_pubkey);

    let authority = metadata.update_authority.to_bytes().to_vec();
    let slot_i = slot as i64;
    let uri = data.uri.trim().replace('\0', "");
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
    if mint_pubkey == *WSOL_PUBKEY {
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
            .map_err(|e: DbErr| IngesterError::DatabaseError(e.to_string()))?;

            token_account
        }
        _ => None,
    };

    // owner and delegate should be from the token account with the mint
    let (owner, delegate) = match token_account {
        Some(ta) => (Set(Some(ta.owner)), Set(ta.delegate)),
        None => {
            if supply == 1 && ownership_type == OwnerType::Single {
                warn!(
                    target: "Account not found",
                    "Token acc not found in 'token_accounts' table for mint {}",
                    bs58::encode(&mint_pubkey_vec).into_string()
                );
            }
            (NotSet, NotSet)
        }
    };

    let name = data.name.clone().into_bytes();
    let symbol = data.symbol.clone().into_bytes();
    let mut chain_data = ChainDataV1 {
        name: data.name.clone(),
        symbol: data.symbol.clone(),
        edition_nonce: metadata.edition_nonce,
        primary_sale_happened: metadata.primary_sale_happened,
        token_standard: metadata.token_standard,
        uses: metadata.uses.map(|u| Uses {
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
    let asset_data_model = asset_data::ActiveModel {
        chain_data_mutability: Set(chain_mutability),
        chain_data: Set(chain_data_json),
        metadata_url: Set(uri.clone()),
        metadata: Set(JsonValue::String("processing".to_string())),
        metadata_mutability: Set(Mutability::Mutable),
        slot_updated: Set(slot_i),
        reindex: Set(Some(true)),
        id: Set(mint_pubkey_vec.clone()),
        raw_name: Set(Some(name.to_vec())),
        raw_symbol: Set(Some(symbol.to_vec())),
        base_info_seq: Set(Some(0)),
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
        .map_err(|db_err| IngesterError::AssetIndexError(db_err.to_string()))?;

    let model = asset::ActiveModel {
        id: Set(mint_pubkey_vec.clone()),
        owner,
        owner_type: Set(ownership_type),
        delegate,
        frozen: Set(false),
        supply: Set(supply),
        supply_mint: Set(supply_mint),
        specification_version: Set(Some(SpecificationVersions::V1)),
        specification_asset_class: Set(Some(class)),
        tree_id: Set(None),
        nonce: Set(Some(0)),
        seq: Set(Some(0)),
        leaf: Set(None),
        data_hash: Set(None),
        creator_hash: Set(None),
        compressed: Set(false),
        compressible: Set(false),
        royalty_target_type: Set(RoyaltyTargetType::Creators),
        royalty_target: Set(None),
        royalty_amount: Set(data.seller_fee_basis_points as i32), //basis points
        asset_data: Set(Some(mint_pubkey_vec.clone())),
        slot_updated: Set(Some(slot_i)),
        burnt: Set(false),
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
        .map_err(|db_err| IngesterError::AssetIndexError(db_err.to_string()))?;

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

    let model = asset_authority::ActiveModel {
        asset_id: Set(mint_pubkey_vec.clone()),
        authority: Set(authority),
        seq: Set(0),
        slot_updated: Set(slot_i),
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
        .map_err(|db_err| IngesterError::AssetIndexError(db_err.to_string()))?;

    if let Some(c) = &metadata.collection {
        let model = asset_grouping::ActiveModel {
            asset_id: Set(mint_pubkey_vec.clone()),
            group_key: Set("collection".to_string()),
            group_value: Set(Some(c.key.to_string())),
            verified: Set(c.verified),
            group_info_seq: Set(Some(0)),
            slot_updated: Set(Some(slot_i)),
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
            .map_err(|db_err| IngesterError::AssetIndexError(db_err.to_string()))?;
    }

    let creators = data.creators.unwrap_or_default();
    if !creators.is_empty() {
        let mut creators_set = HashSet::new();
        let mut db_creators = Vec::with_capacity(creators.len());
        for (i, c) in creators.into_iter().enumerate() {
            if creators_set.contains(&c.address) {
                continue;
            }
            db_creators.push(asset_creators::ActiveModel {
                asset_id: Set(mint_pubkey_vec.clone()),
                position: Set(i as i16),
                creator: Set(c.address.to_bytes().to_vec()),
                share: Set(c.share as i32),
                verified: Set(c.verified),
                slot_updated: Set(Some(slot_i)),
                seq: Set(Some(0)),
                ..Default::default()
            });
            creators_set.insert(c.address);
        }

        if !db_creators.is_empty() {
            let mut query = asset_creators::Entity::insert_many(db_creators)
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
                "{} WHERE excluded.slot_updated > asset_creators.slot_updated",
                query.sql
            );
            txn.execute(query)
                .await
                .map_err(|db_err| IngesterError::AssetIndexError(db_err.to_string()))?;
        }
    }
    txn.commit().await?;

    if uri.is_empty() {
        warn!(
            "URI is empty for mint {}. Skipping background task.",
            bs58::encode(mint_pubkey_vec).into_string()
        );
        return Ok(None);
    }

    let mut task = DownloadMetadata {
        asset_data_id: mint_pubkey_vec.clone(),
        uri,
        created_at: Some(Utc::now().naive_utc()),
    };
    task.sanitize();
    let t = task.into_task_data()?;
    Ok(Some(t))
}

async fn find_model_with_retry<T: ConnectionTrait + TransactionTrait, K: EntityTrait>(
    conn: &T,
    model_name: &str,
    select: &Select<K>,
    retry_intervals: &[u64],
) -> Result<Option<K::Model>, DbErr> {
    let mut retries = 0;
    let metric_name = format!("{}_found", model_name);

    for interval in retry_intervals {
        let interval_duration = Duration::from_millis(interval.to_owned());
        sleep(interval_duration).await;

        let model = select.clone().one(conn).await?;
        if let Some(m) = model {
            record_metric(&metric_name, true, retries);
            return Ok(Some(m));
        }
        retries += 1;
    }

    record_metric(&metric_name, false, retries - 1);
    Ok(None)
}

fn record_metric(metric_name: &str, success: bool, retries: u32) {
    let retry_count = &retries.to_string();
    let success = if success { "true" } else { "false" };
    metric! {
        statsd_count!(metric_name, 1, "success" => success, "retry_count" => retry_count);
    }
}
