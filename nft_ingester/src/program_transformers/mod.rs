use blockbuster::{
    instruction::InstructionBundle,
    program_handler::ProgramParser,
    programs::{
        bubblegum::BubblegumParser,
               ProgramParseResult,
               token_metadata::TokenMetadataParser,
               token_account::TokenAccountParser}
    ,
};

use crate::{error::IngesterError, BgTask};
use plerkle_serialization::{AccountInfo, Pubkey as FBPubkey};
use sea_orm::{DatabaseConnection, SqlxPostgresConnector};
use solana_sdk::pubkey::Pubkey;
use sqlx::PgPool;
use std::collections::HashMap;
use tokio::sync::mpsc::UnboundedSender;

use crate::program_transformers::{
    bubblegum::handle_bubblegum_instruction,
    token_metadata::handle_token_metadata_account,
};
use crate::program_transformers::token::handle_token_program_account;

mod bubblegum;
mod common;
mod token_metadata;
mod token;

pub struct ProgramTransformer {
    storage: DatabaseConnection,
    task_sender: UnboundedSender<Box<dyn BgTask>>,
    matchers: HashMap<Pubkey, Box<dyn ProgramParser>>,
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
        let pool: PgPool = pool;
        ProgramTransformer {
            storage: SqlxPostgresConnector::from_sqlx_postgres_pool(pool),
            task_sender,
            matchers,
        }
    }

    pub fn match_program(&self, key: &FBPubkey) -> Option<&Box<dyn ProgramParser>> {
        self.matchers.get(&Pubkey::new(key.0.as_slice()))
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
                    ).await
                }
                _ => Err(IngesterError::NotImplemented),
            }?;
        }
        Ok(())
    }
}
