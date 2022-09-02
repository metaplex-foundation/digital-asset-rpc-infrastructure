use std::future::Future;
use std::pin::Pin;
use std::process::Output;
use sea_orm::{entity::*, query::*, DbErr};
use sea_orm::{entity::*, query::*, ConnectionTrait, DatabaseTransaction, DbBackend, JsonValue};
use sea_orm::sea_query::OnConflict;
use blockbuster::instruction::InstructionBundle;
use blockbuster::programs::bubblegum::{BubblegumInstruction, LeafSchema, Payload};
use digital_asset_types::adapter::{TokenStandard, UseMethod, Uses};
use digital_asset_types::dao::{asset, asset_authority, asset_creators, asset_data, asset_grouping};
use digital_asset_types::dao::sea_orm_active_enums::{ChainMutability, Mutability, OwnerType, RoyaltyTargetType};
use digital_asset_types::json::ChainDataV1;
use crate::IngesterError;
use crate::program_transformers::bubblegum::task::DownloadMetadata;
use crate::program_transformers::common::save_changelog_event;

// TODO -> consider moving structs into these functions to avoid clone

pub fn mint_v1<'c>(parsing_result: &'c BubblegumInstruction, bundle: &'c InstructionBundle, txn: &'c DatabaseTransaction) -> Pin<Box<dyn Future<Output=Result<DownloadMetadata, IngesterError>> + Send + 'c>> {
    Box::pin(async move {
        if let (Some(le), Some(cl), Some(Payload::MintV1 { args })) = (&parsing_result.leaf_update, &parsing_result.tree_update, &parsing_result.payload) {
            let seq = save_changelog_event(&cl, bundle.slot, txn)
                .await?;
            let metadata = args;
            return match le.schema {
                LeafSchema::V1 {
                    id,
                    delegate,
                    owner,
                    nonce,
                    ..
                } => {
                    let chain_data = ChainDataV1 {
                        name: metadata.name.clone(),
                        symbol: metadata.symbol.clone(),
                        edition_nonce: metadata.edition_nonce,
                        primary_sale_happened: metadata.primary_sale_happened,
                        token_standard: Some(TokenStandard::NonFungible),
                        uses: metadata.uses.map(|u| Uses {
                            use_method: UseMethod::from_u8(u.use_method as u8).unwrap(),
                            remaining: u.remaining,
                            total: u.total,
                        }),
                    };
                    let chain_data_json =
                        serde_json::to_value(chain_data).map_err(|e| {
                            IngesterError::DeserializationError(e.to_string())
                        })?;
                    let chain_mutability = match metadata.is_mutable {
                        true => ChainMutability::Mutable,
                        false => ChainMutability::Immutable,
                    };

                    let data = asset_data::ActiveModel {
                        chain_data_mutability: Set(chain_mutability),
                        schema_version: Set(1),
                        chain_data: Set(chain_data_json),
                        metadata_url: Set(metadata.uri.clone()),
                        metadata: Set(JsonValue::String("processing".to_string())),
                        metadata_mutability: Set(Mutability::Mutable),
                        ..Default::default()
                    }
                        .insert(txn)
                        .await?;

                    // Insert into `asset` table.
                    let delegate = if owner == delegate {
                        None
                    } else {
                        Some(delegate.to_bytes().to_vec())
                    };
                    let model = asset::ActiveModel {
                        id: Set(id.to_bytes().to_vec()),
                        owner: Set(owner.to_bytes().to_vec()),
                        owner_type: Set(OwnerType::Single),
                        delegate: Set(delegate),
                        frozen: Set(false),
                        supply: Set(1),
                        supply_mint: Set(None),
                        compressed: Set(true),
                        compressible: Set(false),
                        tree_id: Set(Some(bundle.keys[7].0.to_vec())), //will change when we remove requests
                        specification_version: Set(1),
                        nonce: Set(nonce as i64),
                        leaf: Set(Some(le.leaf_hash.to_vec())),
                        royalty_target_type: Set(RoyaltyTargetType::Creators),
                        royalty_target: Set(None),
                        royalty_amount: Set(metadata.seller_fee_basis_points as i32), //basis points
                        chain_data_id: Set(Some(data.id)),
                        seq: Set(seq as i64), // gummyroll seq
                        ..Default::default()
                    };

                    // Do not attempt to modify any existing values:
                    // `ON CONFLICT ('id') DO NOTHING`.
                    let query = asset::Entity::insert(model)
                        .on_conflict(
                            OnConflict::columns([asset::Column::Id])
                                .do_nothing()
                                .to_owned(),
                        )
                        .build(DbBackend::Postgres);
                    txn.execute(query).await?;

                    // Insert into `asset_creators` table.
                    if metadata.creators.len() > 0 {
                        let mut creators = Vec::with_capacity(metadata.creators.len());
                        for c in metadata.creators {
                            creators.push(asset_creators::ActiveModel {
                                asset_id: Set(id.to_bytes().to_vec()),
                                creator: Set(c.address.to_bytes().to_vec()),
                                share: Set(c.share as i32),
                                verified: Set(c.verified),
                                seq: Set(seq as i64), // gummyroll seq
                                ..Default::default()
                            });
                        }

                        // Do not attempt to modify any existing values:
                        // `ON CONFLICT ('asset_id') DO NOTHING`.
                        let query = asset_creators::Entity::insert_many(creators)
                            .on_conflict(
                                OnConflict::columns([asset_creators::Column::AssetId])
                                    .do_nothing()
                                    .to_owned(),
                            )
                            .build(DbBackend::Postgres);
                        txn.execute(query).await?;

                        // Insert into `asset_authority` table.
                        let model = asset_authority::ActiveModel {
                            asset_id: Set(id.to_bytes().to_vec()),
                            authority: Set(bundle.keys[0].0.to_vec()), //TODO - we need to rem,ove the optional bubblegum signer logic
                            seq: Set(seq as i64), // gummyroll seq
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
                                    asset_id: Set(id.to_bytes().to_vec()),
                                    group_key: Set("collection".to_string()),
                                    group_value: Set(c.key.to_string()),
                                    seq: Set(seq as i64), // gummyroll seq
                                    ..Default::default()
                                };

                                // Do not attempt to modify any existing values:
                                // `ON CONFLICT ('asset_id') DO NOTHING`.
                                let query = asset_grouping::Entity::insert(model)
                                    .on_conflict(
                                        OnConflict::columns([
                                            asset_grouping::Column::AssetId,
                                        ])
                                            .do_nothing()
                                            .to_owned(),
                                    )
                                    .build(DbBackend::Postgres);
                                txn.execute(query).await?;
                            }
                        }
                    }
                    return Ok(DownloadMetadata {
                        asset_data_id: data.id,
                        uri: metadata.uri.clone(),
                    });
                }
                _ => Err(IngesterError::NotImplemented),
            }?;
        }
        Err(IngesterError::ParsingError("Ix not parsed correctly".to_string()))
    })
}
