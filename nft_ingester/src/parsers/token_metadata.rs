use {
    crate::{
        error::IngesterError,
        parsers::{InstructionBundle, ProgramHandler, ProgramHandlerConfig},
    },
    anchor_client::anchor_lang::{self, prelude::Pubkey, AnchorDeserialize},
    async_trait::async_trait,
    bubblegum::state::leaf_schema::LeafSchemaEvent,
    chrono::Utc,
    digital_asset_types::{
        dao::{
            asset, asset_authority, asset_creators, asset_data, asset_grouping,
            sea_orm_active_enums::{ChainMutability, Mutability, OwnerType, RoyaltyTargetType},
        },
        json::ChainDataV1,
    },
    lazy_static::lazy_static,
    plerkle_serialization::transaction_info_generated::transaction_info::{self},
    sea_orm::{
        entity::*, query::*, DatabaseConnection, DatabaseTransaction, JsonValue,
        SqlxPostgresConnector, TransactionTrait,
    },
    solana_sdk,
    solana_sdk::pubkeys,
    sqlx::{self, Pool, Postgres},
    std::fmt::{Display, Formatter},
};

use crate::utils::bytes_from_fb_table;
use crate::{get_gummy_roll_events, save_changelog_events};
use crate::{
    tasks::{BgTask, TaskManager},
    utils::parse_logs,
};
use bs58::alphabet::Error;
use bubblegum::state::leaf_schema::{LeafSchema, Version};
use bubblegum::state::NFTDecompressionEvent;
use digital_asset_types::adapter::{TokenStandard, UseMethod, Uses};
use mpl_token_metadata::state::{Collection, Metadata, TokenMetadataAccount, TokenStandard};
use num_traits::FromPrimitive;
use plerkle_serialization::account_info_generated::account_info::AccountInfo;
use sea_orm::{sea_query::OnConflict, DbBackend};
use serde_json;
use tokio::sync::mpsc::UnboundedSender;

pubkeys!(
    TokenMetadataProgramID,
    "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s"
);

pub struct TokenMetadataHandler {
    id: Pubkey,
    storage: DatabaseConnection,
    task_sender: UnboundedSender<Box<dyn BgTask>>,
}

#[async_trait]
impl ProgramHandler for TokenMetadataHandler {
    fn id(&self) -> Pubkey {
        self.id
    }

    fn config(&self) -> &ProgramHandlerConfig {
        lazy_static! {
            static ref CONFIG: ProgramHandlerConfig = ProgramHandlerConfig {
                responds_to_instruction: false,
                responds_to_account: true
            };
        }
        return &CONFIG;
    }

    async fn handle_account(&self, account_update: &AccountInfo) -> Result<(), IngesterError> {
        handle_token_metadata_account(&account_update, &self.storage).await
    }
}

impl TokenMetadataHandler {
    pub fn new(pool: Pool<Postgres>, task_queue: UnboundedSender<Box<dyn BgTask>>) -> Self {
        TokenMetadataHandler {
            id: TokenMetadataProgramID(),
            task_sender: task_queue,
            storage: SqlxPostgresConnector::from_sqlx_postgres_pool(pool),
        }
    }
}

async fn handle_token_metadata_account<'a, 'b, 't>(
    account_update: &AccountInfo<'_>,
    db: &DatabaseConnection,
) -> Result<(), IngesterError> {
    let metadata = if let Some(account_data) = account_update.data() {
        let data = account_data[8..].to_owned();
        let data_buf = &mut data.as_slice();
        Metadata::deserialize(data_buf)?
    } else {
        return Err(IngesterError::CompressedAssetEventMalformed);
    };

    db.transaction::<_, i64, IngesterError>(|txn| {
        Box::pin(async move {
            // Printing metadata instruction arguments for debugging
            println!(
                "\tMetadata info: {} {} {} {} {}",
                metadata.mint.to_string(),
                &metadata.data.name,
                metadata.data.seller_fee_basis_points,
                metadata.primary_sale_happened,
                metadata.is_mutable,
            );

            // Insert into `asset_data` table.  Note that if a transaction is
            // replayed, this will insert the data again resulting in a
            // duplicate entry.
            let chain_data = ChainDataV1 {
                name: metadata.data.name,
                symbol: metadata.data.symbol,
                edition_nonce: metadata.edition_nonce,
                primary_sale_happened: metadata.primary_sale_happened,
                token_standard: metadata
                    .token_standard
                    .and_then(|ts| TokenStandard::from_u8(ts as u8)),
                uses: metadata.uses.map(|u| Uses {
                    use_method: UseMethod::from_u8(u.use_method as u8).unwrap(),
                    remaining: u.remaining,
                    total: u.total,
                }),
            };
            let chain_data_json = serde_json::to_value(chain_data)
                .map_err(|e| IngesterError::DeserializationError(e.to_string()))?;
            let chain_mutability = match metadata.is_mutable {
                true => ChainMutability::Mutable,
                false => ChainMutability::Immutable,
            };

            let data = asset_data::ActiveModel {
                chain_data_mutability: Set(chain_mutability),
                schema_version: Set(1),
                chain_data: Set(chain_data_json),
                metadata_url: Set(metadata.data.uri),
                metadata: Set(JsonValue::String("processing".to_string())),
                metadata_mutability: Set(Mutability::Mutable),
                ..Default::default()
            }
            .insert(txn)
            .await?;

            let owner = if let Some(owner) = account_update.owner() {
                owner.to_vec()
            } else {
                return Err(IngesterError::CompressedAssetEventMalformed);
            };

            let model = asset::ActiveModel {
                id: Set(metadata.mint.to_bytes().to_vec()),
                owner: Set(owner),
                owner_type: Set(OwnerType::Single),
                delegate: Set(None),
                frozen: Set(false),
                supply: Set(1),
                supply_mint: Set(None),
                compressed: Set(true),
                compressible: Set(false),
                tree_id: Set(None),
                specification_version: Set(1),
                nonce: Set(nonce as i64),
                leaf: Set(None),
                royalty_target_type: Set(RoyaltyTargetType::Creators),
                royalty_target: Set(None),
                royalty_amount: Set(metadata.data.seller_fee_basis_points as i32), //basis points
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
            // todo do this better
            if metadata.data.creators.len() > 0 {
                let mut creators = Vec::with_capacity(metadata.creators.len());
                for c in metadata.data.creators? {
                    creators.push(asset_creators::ActiveModel {
                        asset_id: Set(metadata.mint.to_bytes().to_vec()),
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
                    authority: Set(metadata.update_authority.to_bytes().to_vec()),
                    seq: Set(seq as i64),
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
                if let Some(c) = metadata.collection {
                    if c.verified {
                        let model = asset_grouping::ActiveModel {
                            asset_id: Set(metadata.mint.to_bytes().to_vec()),
                            group_key: Set("collection".to_string()),
                            group_value: Set(c.key.to_string()),
                            seq: Set(seq as i64), // gummyroll seq
                            ..Default::default()
                        };

                        // Do not attempt to modify any existing values:
                        // `ON CONFLICT ('asset_id') DO NOTHING`.
                        let query = asset_grouping::Entity::insert(model)
                            .on_conflict(
                                OnConflict::columns([asset_grouping::Column::AssetId])
                                    .do_nothing()
                                    .to_owned(),
                            )
                            .build(DbBackend::Postgres);
                        txn.execute(query).await?;
                    }
                }
            }
            Ok(data.id)
        })
    })
    .await?;

    Ok(())
}
