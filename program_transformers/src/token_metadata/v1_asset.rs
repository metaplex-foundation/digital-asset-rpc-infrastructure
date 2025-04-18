use {
    super::IsNonFungibe,
    crate::{
        asset_upserts::{upsert_assets_metadata_account_columns, AssetMetadataAccountColumns},
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
                ChainMutability, Mutability, SpecificationAssetClass, SpecificationVersions,
                V1AccountAttachments,
            },
        },
        json::ChainDataV1,
    },
    sea_orm::{
        entity::{ActiveValue, EntityTrait},
        query::JsonValue,
        sea_query::{query::OnConflict, Alias, Expr},
        Condition, ConnectionTrait, Statement, TransactionTrait,
    },
    solana_sdk::pubkey,
    solana_sdk::pubkey::Pubkey,
    tracing::warn,
};

pub async fn burn_v1_asset<T: ConnectionTrait + TransactionTrait>(
    conn: &T,
    id: pubkey::Pubkey,
    slot: u64,
) -> ProgramTransformerResult<()> {
    let slot_i = slot as i64;
    let model = asset::ActiveModel {
        id: ActiveValue::Set(id.to_bytes().to_vec()),
        slot_updated: ActiveValue::Set(Some(slot_i)),
        burnt: ActiveValue::Set(true),
        ..Default::default()
    };

    asset::Entity::insert(model)
        .on_conflict(
            OnConflict::columns([asset::Column::Id])
                .update_columns([asset::Column::SlotUpdated, asset::Column::Burnt])
                .action_cond_where(
                    Condition::all()
                        .add(
                            Expr::tbl(Alias::new("excluded"), asset::Column::Burnt)
                                .ne(Expr::tbl(asset::Entity, asset::Column::Burnt)),
                        )
                        .add(Expr::tbl(asset::Entity, asset::Column::SlotUpdated).lte(slot_i)),
                )
                .to_owned(),
        )
        .exec_without_returning(conn)
        .await?;

    Ok(())
}

static WSOL_PUBKEY: pubkey::Pubkey = pubkey!("So11111111111111111111111111111111111111112");
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

    // Wrapped Solana is a special token that has supply 0 (infinite).
    // It's a fungible token with a metadata account, but without any token standard, meaning the code above will misabel it as an NFT.
    if mint_pubkey == WSOL_PUBKEY {
        class = SpecificationAssetClass::FungibleToken;
    }

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

    let set_lock_timeout = "SET LOCAL lock_timeout = '1s';";
    let set_local_app_name =
        "SET LOCAL application_name = 'das::program_transformers::token_metadata::v1_asset';";
    let set_lock_timeout_stmt =
        Statement::from_string(txn.get_database_backend(), set_lock_timeout.to_string());
    let set_local_app_name_stmt =
        Statement::from_string(txn.get_database_backend(), set_local_app_name.to_string());
    txn.execute(set_lock_timeout_stmt).await?;
    txn.execute(set_local_app_name_stmt).await?;

    asset_data::Entity::insert(asset_data_model)
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
                .action_cond_where(
                    Condition::all()
                        .add(
                            Condition::any()
                                .add(
                                    Expr::tbl(
                                        Alias::new("excluded"),
                                        asset_data::Column::ChainDataMutability,
                                    )
                                    .ne(Expr::tbl(
                                        asset_data::Entity,
                                        asset_data::Column::ChainDataMutability,
                                    )),
                                )
                                .add(
                                    Expr::tbl(
                                        Alias::new("excluded"),
                                        asset_data::Column::ChainData,
                                    )
                                    .ne(Expr::tbl(
                                        asset_data::Entity,
                                        asset_data::Column::ChainData,
                                    )),
                                )
                                .add(
                                    Expr::tbl(
                                        Alias::new("excluded"),
                                        asset_data::Column::MetadataUrl,
                                    )
                                    .ne(Expr::tbl(
                                        asset_data::Entity,
                                        asset_data::Column::MetadataUrl,
                                    )),
                                )
                                .add(
                                    Expr::tbl(
                                        Alias::new("excluded"),
                                        asset_data::Column::MetadataMutability,
                                    )
                                    .ne(Expr::tbl(
                                        asset_data::Entity,
                                        asset_data::Column::MetadataMutability,
                                    )),
                                )
                                .add(
                                    Expr::tbl(Alias::new("excluded"), asset_data::Column::Reindex)
                                        .ne(Expr::tbl(
                                            asset_data::Entity,
                                            asset_data::Column::Reindex,
                                        )),
                                )
                                .add(
                                    Expr::tbl(Alias::new("excluded"), asset_data::Column::RawName)
                                        .ne(Expr::tbl(
                                            asset_data::Entity,
                                            asset_data::Column::RawName,
                                        )),
                                )
                                .add(
                                    Expr::tbl(
                                        Alias::new("excluded"),
                                        asset_data::Column::RawSymbol,
                                    )
                                    .ne(Expr::tbl(
                                        asset_data::Entity,
                                        asset_data::Column::RawSymbol,
                                    )),
                                )
                                .add(
                                    Expr::tbl(
                                        Alias::new("excluded"),
                                        asset_data::Column::BaseInfoSeq,
                                    )
                                    .ne(Expr::tbl(
                                        asset_data::Entity,
                                        asset_data::Column::BaseInfoSeq,
                                    )),
                                ),
                        )
                        .add(
                            Expr::tbl(asset_data::Entity, asset_data::Column::SlotUpdated)
                                .lte(slot_i),
                        ),
                )
                .to_owned(),
        )
        .exec_without_returning(&txn)
        .await
        .map_err(|db_err| ProgramTransformerError::AssetIndexError(db_err.to_string()))?;

    upsert_assets_metadata_account_columns(
        AssetMetadataAccountColumns {
            mint: mint_pubkey_vec.clone(),
            specification_asset_class: Some(class),
            royalty_amount: metadata.seller_fee_basis_points as i32,
            asset_data: Some(mint_pubkey_vec.clone()),
            slot_updated_metadata_account: slot_i as u64,
            mpl_core_plugins: None,
            mpl_core_unknown_plugins: None,
            mpl_core_collection_num_minted: None,
            mpl_core_collection_current_size: None,
            mpl_core_plugins_json_version: None,
            mpl_core_external_plugins: None,
            mpl_core_unknown_external_plugins: None,
        },
        &txn,
    )
    .await?;

    let attachment = asset_v1_account_attachments::ActiveModel {
        id: ActiveValue::Set(edition_attachment_address.to_bytes().to_vec()),
        slot_updated: ActiveValue::Set(slot_i),
        attachment_type: ActiveValue::Set(V1AccountAttachments::MasterEditionV2),
        ..Default::default()
    };

    asset_v1_account_attachments::Entity::insert(attachment)
        .on_conflict(
            OnConflict::columns([asset_v1_account_attachments::Column::Id])
                .do_nothing()
                .to_owned(),
        )
        .exec_without_returning(&txn)
        .await
        .map_err(|db_err| ProgramTransformerError::AssetIndexError(db_err.to_string()))?;

    let model = asset_authority::ActiveModel {
        asset_id: ActiveValue::Set(mint_pubkey_vec.clone()),
        authority: ActiveValue::Set(authority),
        seq: ActiveValue::Set(0),
        slot_updated: ActiveValue::Set(slot_i),
        ..Default::default()
    };

    asset_authority::Entity::insert(model)
        .on_conflict(
            OnConflict::column(asset_authority::Column::AssetId)
                .update_columns([
                    asset_authority::Column::Authority,
                    asset_authority::Column::SlotUpdated,
                ])
                .action_cond_where(
                    Condition::all()
                        .add(
                            Expr::tbl(Alias::new("excluded"), asset_authority::Column::Authority)
                                .ne(Expr::tbl(
                                    asset_authority::Entity,
                                    asset_authority::Column::Authority,
                                )),
                        )
                        .add(
                            Expr::tbl(
                                asset_authority::Entity,
                                asset_authority::Column::SlotUpdated,
                            )
                            .lte(slot_i),
                        ),
                )
                .to_owned(),
        )
        .exec_without_returning(&txn)
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

        asset_grouping::Entity::insert(model)
            .on_conflict(
                OnConflict::columns([
                    asset_grouping::Column::AssetId,
                    asset_grouping::Column::GroupKey,
                ])
                .update_columns([
                    asset_grouping::Column::GroupValue,
                    asset_grouping::Column::Verified,
                    asset_grouping::Column::SlotUpdated,
                ])
                .action_cond_where(
                    Condition::all()
                        .add(
                            Condition::any()
                                .add(
                                    Expr::tbl(
                                        Alias::new("excluded"),
                                        asset_grouping::Column::GroupValue,
                                    )
                                    .ne(Expr::tbl(
                                        asset_grouping::Entity,
                                        asset_grouping::Column::GroupValue,
                                    )),
                                )
                                .add(
                                    Expr::tbl(
                                        Alias::new("excluded"),
                                        asset_grouping::Column::Verified,
                                    )
                                    .ne(Expr::tbl(
                                        asset_grouping::Entity,
                                        asset_grouping::Column::Verified,
                                    )),
                                ),
                        )
                        .add(
                            Expr::tbl(asset_grouping::Entity, asset_grouping::Column::SlotUpdated)
                                .lte(slot_i),
                        ),
                )
                .to_owned(),
            )
            .exec_without_returning(&txn)
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
        asset_creators::Entity::insert_many(creators)
            .on_conflict(
                OnConflict::columns([
                    asset_creators::Column::AssetId,
                    asset_creators::Column::Position,
                ])
                .update_columns([
                    asset_creators::Column::Creator,
                    asset_creators::Column::Share,
                    asset_creators::Column::Seq,
                    asset_creators::Column::Verified,
                    asset_creators::Column::SlotUpdated,
                ])
                .action_cond_where(
                    Condition::any()
                        .add(
                            Condition::all().add(
                                Condition::any()
                                    .add(
                                        Expr::tbl(
                                            Alias::new("excluded"),
                                            asset_creators::Column::Creator,
                                        )
                                        .ne(Expr::tbl(
                                            asset_creators::Entity,
                                            asset_creators::Column::Creator,
                                        )),
                                    )
                                    .add(
                                        Expr::tbl(
                                            Alias::new("excluded"),
                                            asset_creators::Column::Share,
                                        )
                                        .ne(Expr::tbl(
                                            asset_creators::Entity,
                                            asset_creators::Column::Share,
                                        )),
                                    )
                                    .add(
                                        Expr::tbl(
                                            Alias::new("excluded"),
                                            asset_creators::Column::Verified,
                                        )
                                        .ne(Expr::tbl(
                                            asset_creators::Entity,
                                            asset_creators::Column::Verified,
                                        )),
                                    )
                                    .add(
                                        Expr::tbl(
                                            Alias::new("excluded"),
                                            asset_creators::Column::Seq,
                                        )
                                        .ne(Expr::tbl(
                                            asset_creators::Entity,
                                            asset_creators::Column::Seq,
                                        )),
                                    ),
                            ),
                        )
                        .add(
                            Condition::any()
                                .add(
                                    Expr::tbl(
                                        asset_creators::Entity,
                                        asset_creators::Column::SlotUpdated,
                                    )
                                    .is_null(),
                                )
                                .add(
                                    Expr::tbl(
                                        asset_creators::Entity,
                                        asset_creators::Column::SlotUpdated,
                                    )
                                    .lte(slot_i),
                                ),
                        ),
                )
                .to_owned(),
            )
            .exec_without_returning(&txn)
            .await
            .map_err(|db_err| ProgramTransformerError::AssetIndexError(db_err.to_string()))?;
    }

    // If the asset is a non-fungible token, then we need to insert to the asset_v1_account_attachments table
    if let Some(true) = metadata.token_standard.map(|t| t.is_non_fungible()) {
        upsert_asset_v1_account_attachments(&txn, &mint_pubkey, slot).await?;
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

async fn upsert_asset_v1_account_attachments<T: ConnectionTrait + TransactionTrait>(
    conn: &T,
    mint_pubkey: &Pubkey,
    slot: u64,
) -> ProgramTransformerResult<()> {
    let edition_pubkey = MasterEdition::find_pda(mint_pubkey).0;
    let mint_pubkey_vec = mint_pubkey.to_bytes().to_vec();
    let attachment = asset_v1_account_attachments::ActiveModel {
        id: ActiveValue::Set(edition_pubkey.to_bytes().to_vec()),
        asset_id: ActiveValue::Set(Some(mint_pubkey_vec.clone())),
        slot_updated: ActiveValue::Set(slot as i64),
        // by default, the attachment type is MasterEditionV2
        attachment_type: ActiveValue::Set(V1AccountAttachments::MasterEditionV2),
        ..Default::default()
    };

    asset_v1_account_attachments::Entity::insert(attachment)
        .on_conflict(
            OnConflict::columns([asset_v1_account_attachments::Column::Id])
                .update_columns([asset_v1_account_attachments::Column::AssetId])
                .action_cond_where(
                    Expr::tbl(
                        asset_v1_account_attachments::Entity,
                        asset_v1_account_attachments::Column::SlotUpdated,
                    )
                    .lte(slot as i64),
                )
                .to_owned(),
        )
        .exec_without_returning(conn)
        .await
        .map_err(|db_err| ProgramTransformerError::AssetIndexError(db_err.to_string()))?;

    Ok(())
}
