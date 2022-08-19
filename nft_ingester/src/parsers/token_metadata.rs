use std::time::SystemTime;
use {
    crate::{
        error::IngesterError,
        events::handle_event,
        parsers::{InstructionBundle, ProgramHandler, ProgramHandlerConfig},
        utils::filter_events_from_logs,
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
    flatbuffers::{ForwardsUOffset, Vector},
    lazy_static::lazy_static,
    num_traits::FromPrimitive,
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
use plerkle_serialization::account_info_generated::account_info::AccountInfo;
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
        handle_token_metadata_account(&account_update).await
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
) -> Result<(), IngesterError> {
    Ok(())
}
