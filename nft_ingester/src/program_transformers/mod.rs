use blockbuster::{
    instruction::InstructionBundle,
    program_handler::ProgramParser,
    programs::{
        bubblegum::BubblegumParser, token_account::TokenAccountParser,
        token_metadata::TokenMetadataParser, ProgramParseResult,
    },
};

use crate::{error::IngesterError, BgTask};
use blockbuster::instruction::IxPair;
use plerkle_serialization::{AccountInfo, Pubkey as FBPubkey, TransactionInfo};
use sea_orm::{DatabaseConnection, SqlxPostgresConnector};
use solana_sdk::pubkey::Pubkey;
use sqlx::PgPool;
use std::collections::{HashMap, HashSet, VecDeque};
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    order_instructions,
    program_transformers::{
        bubblegum::handle_bubblegum_instruction, token::handle_token_program_account,
        token_metadata::handle_token_metadata_account,
    },
};

use digital_asset_types::dao::{raw_account_update, raw_transaction_update};
use plerkle_serialization::{root_as_account_info, root_as_transaction_info};
use sea_orm::{
    entity::*, query::*, sea_query::OnConflict, ActiveValue::Set, ConnectionTrait,
    DatabaseTransaction, DbBackend, DbErr, EntityTrait, JsonValue,
};

mod bubblegum;
mod common;
mod token;
mod token_metadata;

pub struct ProgramTransformer {
    storage: DatabaseConnection,
    task_sender: UnboundedSender<Box<dyn BgTask>>,
    matchers: HashMap<Pubkey, Box<dyn ProgramParser>>,
    key_set: HashSet<Pubkey>,
}

impl ProgramTransformer {
    pub fn new(pool: PgPool, task_sender: UnboundedSender<Box<dyn BgTask>>) -> Self {
        let mut matchers: HashMap<Pubkey, Box<dyn ProgramParser>> = HashMap::with_capacity(1);
        let bgum = BubblegumParser {};
        let token_metadata = TokenMetadataParser {};
        let token = TokenAccountParser {};
        matchers.insert(bgum.key(), Box::new(bgum));
        matchers.insert(token_metadata.key(), Box::new(token_metadata));
        matchers.insert(token.key(), Box::new(token));
        let hs = matchers.iter().fold(HashSet::new(), |mut acc, (k, _)| {
            acc.insert(*k);
            acc
        });
        let pool: PgPool = pool;
        ProgramTransformer {
            storage: SqlxPostgresConnector::from_sqlx_postgres_pool(pool),
            task_sender,
            matchers,
            key_set: hs,
        }
    }

    pub fn break_transaction<'i>(
        &self,
        tx: &'i TransactionInfo<'i>,
    ) -> VecDeque<(IxPair<'i>, Option<Vec<IxPair<'i>>>)> {
        let ref_set: HashSet<&[u8]> = self.key_set.iter().map(|k| k.as_ref()).collect();
        order_instructions(ref_set, tx)
    }

    pub fn match_program(&self, key: &FBPubkey) -> Option<&Box<dyn ProgramParser>> {
        self.matchers.get(&Pubkey::new(key.0.as_slice()))
    }

    pub async fn handle_raw_transaction(&self, data: &[u8]) -> Result<(), IngesterError> {
        // Get root of account info flatbuffers object.
        let transaction_info = root_as_transaction_info(data)
            .map_err(|e| IngesterError::DeserializationError(e.to_string()))?;

        // TODO get signature from tx.
        let signature = [0u8];
        // let signature = account_update.signature().ok_or_else(|| {
        //     IngesterError::DeserializationError(
        //         "Flatbuffers TransactionInfo signature deserialization error".to_string(),
        //     )
        // })?;

        // Setup `ActiveModel`.
        let model = raw_transaction_update::ActiveModel {
            signature: Set(signature.to_vec()),
            slot: Set(transaction_info.slot() as i64),
            // TODO doesn't seem to be any benefit for messenger to output
            // a slice if I just convert it to Vec anyways.
            raw_data: Set(data.to_vec()),
            ..Default::default()
        };

        // Put data into raw table.  The `ON CONFLICT` clause will dedupe the incoming stream.
        let query = raw_transaction_update::Entity::insert(model)
            .on_conflict(
                OnConflict::columns([
                    raw_transaction_update::Column::Signature,
                    raw_transaction_update::Column::Slot,
                ])
                .do_nothing()
                .to_owned(),
            )
            .build(DbBackend::Postgres);

        let txn = self.storage.begin().await?;
        txn.execute(query).await?;

        Ok(())
    }

    pub async fn handle_instruction<'a>(
        &self,
        ix: &'a InstructionBundle<'a>,
    ) -> Result<(), IngesterError> {
        if let Some(program) = self.match_program(&ix.program) {
            let result = program.handle_instruction(ix)?;
            let concrete = result.result_type();

            match concrete {
                ProgramParseResult::Bubblegum(parsing_result) => {
                    handle_bubblegum_instruction(
                        parsing_result,
                        &ix,
                        &self.storage,
                        &self.task_sender,
                    )
                    .await
                }

                _ => Err(IngesterError::NotImplemented),
            }?;
        }
        Ok(())
    }

    pub async fn handle_raw_account_update(&self, data: &[u8]) -> Result<(), IngesterError> {
        // Get root of account info flatbuffers object.
        let account_update = root_as_account_info(data)
            .map_err(|e| IngesterError::DeserializationError(e.to_string()))?;

        let pubkey = account_update.pubkey().ok_or_else(|| {
            IngesterError::DeserializationError(
                "Flatbuffers AccountInfo pubkey deserialization error".to_string(),
            )
        })?;

        // Setup `ActiveModel`.
        let model = raw_account_update::ActiveModel {
            pubkey: Set(pubkey.0.to_vec()),
            write_version: Set(account_update.write_version() as i64),
            slot: Set(account_update.slot() as i64),
            // TODO doesn't seem to be any benefit for messenger to output
            // a slice if I just convert it to Vec anyways.
            raw_data: Set(data.to_vec()),
            ..Default::default()
        };

        // Put data into raw table.  The `ON CONFLICT` clause will dedupe the incoming stream.
        let query = raw_account_update::Entity::insert(model)
            .on_conflict(
                OnConflict::columns([
                    raw_account_update::Column::Pubkey,
                    raw_account_update::Column::WriteVersion,
                    raw_account_update::Column::Slot,
                ])
                .do_nothing()
                .to_owned(),
            )
            .build(DbBackend::Postgres);

        let txn = self.storage.begin().await?;
        txn.execute(query).await?;

        Ok(())
    }

    pub async fn handle_account_update<'b>(
        &self,
        acct: AccountInfo<'b>,
    ) -> Result<(), IngesterError> {
        let owner = acct.owner().unwrap();
        if let Some(program) = self.match_program(owner) {
            let result = program.handle_account(&acct)?;
            let concrete = result.result_type();
            match concrete {
                ProgramParseResult::TokenMetadata(parsing_result) => {
                    handle_token_metadata_account(
                        &acct,
                        parsing_result,
                        &self.storage,
                        &self.task_sender,
                    )
                    .await
                }
                ProgramParseResult::TokenProgramAccount(parsing_result) => {
                    handle_token_program_account(
                        &acct,
                        parsing_result,
                        &self.storage,
                        &self.task_sender,
                    )
                    .await
                }
                _ => Err(IngesterError::NotImplemented),
            }?;
        }
        Ok(())
    }
}
