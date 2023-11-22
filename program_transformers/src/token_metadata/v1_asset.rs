use {
    crate::{
        error::{ProgramTransformerError, ProgramTransformerResult},
        DownloadMetadataInfo,
    },
    blockbuster::token_metadata::{
        pda::find_master_edition_account,
        state::{Metadata, TokenStandard, UseMethod, Uses},
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
    num_traits::FromPrimitive,
    plerkle_serialization::Pubkey as FBPubkey,
    sea_orm::{
        entity::{ActiveValue, ColumnTrait, EntityTrait},
        query::{Condition, JsonValue, QueryFilter, QueryTrait},
        sea_query::query::OnConflict,
        ConnectionTrait, DbBackend, DbErr, TransactionTrait,
    },
    std::collections::HashSet,
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

pub async fn save_v1_asset<T: ConnectionTrait + TransactionTrait>(
    conn: &T,
    id: FBPubkey,
    slot: u64,
    metadata: &Metadata,
) -> ProgramTransformerResult<Option<DownloadMetadataInfo>> {
    let metadata = metadata.clone();
    let data = metadata.data;
    let meta_mint_pubkey = metadata.mint;
    let (edition_attachment_address, _) = find_master_edition_account(&meta_mint_pubkey);
    let mint = metadata.mint.to_bytes().to_vec();
    let authority = metadata.update_authority.to_bytes().to_vec();
    let id = id.0;
    let slot_i = slot as i64;
    let uri = data.uri.trim().replace('\0', "");
    let _spec = SpecificationVersions::V1;
    let class = match metadata.token_standard {
        Some(TokenStandard::NonFungible) => SpecificationAssetClass::Nft,
        Some(TokenStandard::FungibleAsset) => SpecificationAssetClass::FungibleAsset,
        Some(TokenStandard::Fungible) => SpecificationAssetClass::FungibleToken,
        _ => SpecificationAssetClass::Unknown,
    };
    let ownership_type = match class {
        SpecificationAssetClass::FungibleAsset => OwnerType::Token,
        SpecificationAssetClass::FungibleToken => OwnerType::Token,
        _ => OwnerType::Single,
    };

    // gets the token and token account for the mint to populate the asset. This is required when the token and token account are indexed, but not the metadata account. If the metadata account is indexed, then the token and ta ingester will update the asset with the correct data

    let (token, token_account): (Option<tokens::Model>, Option<token_accounts::Model>) =
        match ownership_type {
            OwnerType::Single => {
                let token: Option<tokens::Model> =
                    tokens::Entity::find_by_id(mint.clone()).one(conn).await?;
                // query for token account associated with mint with positive balance
                let token_account: Option<token_accounts::Model> = token_accounts::Entity::find()
                    .filter(token_accounts::Column::Mint.eq(mint.clone()))
                    .filter(token_accounts::Column::Amount.gt(0))
                    .one(conn)
                    .await?;
                Ok((token, token_account))
            }
            _ => {
                let token = tokens::Entity::find_by_id(mint.clone()).one(conn).await?;
                Ok((token, None))
            }
        }
        .map_err(|e: DbErr| ProgramTransformerError::DatabaseError(e.to_string()))?;

    // get supply of token, default to 1 since most cases will be NFTs. Token mint ingester will properly set supply if token_result is None
    let (supply, supply_mint) = match token {
        Some(t) => (ActiveValue::Set(t.supply), ActiveValue::Set(Some(t.mint))),
        None => (ActiveValue::Set(1), ActiveValue::NotSet),
    };

    // owner and delegate should be from the token account with the mint
    let (owner, delegate) = match token_account {
        Some(ta) => (
            ActiveValue::Set(Some(ta.owner)),
            ActiveValue::Set(ta.delegate),
        ),
        None => (ActiveValue::NotSet, ActiveValue::NotSet),
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
        id: ActiveValue::Set(id.to_vec()),
        raw_name: ActiveValue::Set(name.to_vec()),
        raw_symbol: ActiveValue::Set(symbol.to_vec()),
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
        id: ActiveValue::Set(id.to_vec()),
        owner,
        owner_type: ActiveValue::Set(ownership_type),
        delegate,
        frozen: ActiveValue::Set(false),
        supply,
        supply_mint,
        specification_version: ActiveValue::Set(Some(SpecificationVersions::V1)),
        specification_asset_class: ActiveValue::Set(Some(class)),
        tree_id: ActiveValue::Set(None),
        nonce: ActiveValue::Set(Some(0)),
        seq: ActiveValue::Set(Some(0)),
        leaf: ActiveValue::Set(None),
        compressed: ActiveValue::Set(false),
        compressible: ActiveValue::Set(false),
        royalty_target_type: ActiveValue::Set(RoyaltyTargetType::Creators),
        royalty_target: ActiveValue::Set(None),
        royalty_amount: ActiveValue::Set(data.seller_fee_basis_points as i32), //basis points
        asset_data: ActiveValue::Set(Some(id.to_vec())),
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
        "{} WHERE excluded.slot_updated > asset.slot_updated",
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
        asset_id: ActiveValue::Set(id.to_vec()),
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
            asset_id: ActiveValue::Set(id.to_vec()),
            group_key: ActiveValue::Set("collection".to_string()),
            group_value: ActiveValue::Set(Some(c.key.to_string())),
            verified: ActiveValue::Set(Some(c.verified)),
            seq: ActiveValue::Set(None),
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
                    asset_grouping::Column::GroupKey,
                    asset_grouping::Column::GroupValue,
                    asset_grouping::Column::Seq,
                    asset_grouping::Column::SlotUpdated,
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
    txn.commit().await?;
    let creators = data.creators.unwrap_or_default();
    if !creators.is_empty() {
        let mut creators_set = HashSet::new();
        let existing_creators: Vec<asset_creators::Model> = asset_creators::Entity::find()
            .filter(
                Condition::all()
                    .add(asset_creators::Column::AssetId.eq(id.to_vec()))
                    .add(asset_creators::Column::SlotUpdated.lt(slot_i)),
            )
            .all(conn)
            .await?;
        if !existing_creators.is_empty() {
            let mut db_creators = Vec::with_capacity(creators.len());
            for (i, c) in creators.into_iter().enumerate() {
                if creators_set.contains(&c.address) {
                    continue;
                }
                db_creators.push(asset_creators::ActiveModel {
                    asset_id: ActiveValue::Set(id.to_vec()),
                    creator: ActiveValue::Set(c.address.to_bytes().to_vec()),
                    share: ActiveValue::Set(c.share as i32),
                    verified: ActiveValue::Set(c.verified),
                    seq: ActiveValue::Set(Some(0)),
                    slot_updated: ActiveValue::Set(Some(slot_i)),
                    position: ActiveValue::Set(i as i16),
                    ..Default::default()
                });
                creators_set.insert(c.address);
            }
            let txn = conn.begin().await?;
            asset_creators::Entity::delete_many()
                .filter(
                    Condition::all()
                        .add(asset_creators::Column::AssetId.eq(id.to_vec()))
                        .add(asset_creators::Column::SlotUpdated.lt(slot_i)),
                )
                .exec(&txn)
                .await?;
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
                txn.execute(query).await.map_err(|db_err| {
                    ProgramTransformerError::AssetIndexError(db_err.to_string())
                })?;
            }
            txn.commit().await?;
        }
    }
    if uri.is_empty() {
        warn!(
            "URI is empty for mint {}. Skipping background task.",
            bs58::encode(mint).into_string()
        );
        return Ok(None);
    }

    Ok(Some(DownloadMetadataInfo::new(id.to_vec(), uri)))
}
