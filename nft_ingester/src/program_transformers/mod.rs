use blockbuster::{
    instruction::InstructionBundle,
    program_handler::ProgramParser,
    programs::{
        bubblegum::BubblegumParser, candy_guard::CandyGuardParser,
        candy_machine::CandyMachineParser,
        candy_machine_core::CandyMachineParser as CandyMachineCoreParser,
        token_account::TokenAccountParser, token_metadata::TokenMetadataParser, ProgramParseResult,
    },
};

use crate::{error::IngesterError, BgTask};
use plerkle_serialization::{AccountInfo, Pubkey as FBPubkey};
use sea_orm::{DatabaseConnection, SqlxPostgresConnector};
use solana_sdk::pubkey::Pubkey;
use sqlx::PgPool;
use std::collections::HashMap;
use tokio::sync::mpsc::UnboundedSender;

use crate::program_transformers::{
    bubblegum::handle_bubblegum_instruction, candy_guard::handle_candy_guard_account_update,
    candy_machine::handle_candy_machine_account_update,
    candy_machine_core::handle_candy_machine_core_account_update,
    token::handle_token_program_account, token_metadata::handle_token_metadata_account,
};

mod bubblegum;
mod candy_guard;
mod candy_machine;
mod candy_machine_core;
mod common;
mod token;
mod token_metadata;

pub struct ProgramTransformer {
    storage: DatabaseConnection,
    task_sender: UnboundedSender<Box<dyn BgTask>>,
    matchers: HashMap<Pubkey, Box<dyn ProgramParser>>,
}

impl ProgramTransformer {
    pub fn new(pool: PgPool, task_sender: UnboundedSender<Box<dyn BgTask>>) -> Self {
        let mut matchers: HashMap<Pubkey, Box<dyn ProgramParser>> = HashMap::with_capacity(1);
        let bgum = BubblegumParser {};
        let candy_machine = CandyMachineParser {};
        let candy_machine_core = CandyMachineCoreParser {};
        let candy_guard = CandyGuardParser {};
        let token_metadata = TokenMetadataParser {};
        let token = TokenAccountParser {};
        matchers.insert(bgum.key(), Box::new(bgum));
        matchers.insert(token_metadata.key(), Box::new(token_metadata));
        matchers.insert(token.key(), Box::new(token));
        matchers.insert(candy_machine.key(), Box::new(candy_machine));
        matchers.insert(candy_machine_core.key(), Box::new(candy_machine_core));
        matchers.insert(candy_guard.key(), Box::new(candy_guard));
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
        acct: &'b AccountInfo<'b>,
    ) -> Result<(), IngesterError> {
        if let Some(owner) = acct.owner() {
            println!("here in handle account update");
            if let Some(program) = self.match_program(owner) {
                let result = program.handle_account(acct)?;
                let concrete = result.result_type();
                match concrete {
                    ProgramParseResult::CandyMachine(parsing_result) => {
                        handle_candy_machine_account_update(
                            &acct,
                            parsing_result,
                            &self.storage,
                            &self.task_sender,
                        )
                        .await
                    }
                    ProgramParseResult::CandyMachineCore(parsing_result) => {
                        handle_candy_machine_core_account_update(
                            &acct,
                            parsing_result,
                            &self.storage,
                            &self.task_sender,
                        )
                        .await
                    }
                    ProgramParseResult::CandyGuard(parsing_result) => {
                        handle_candy_guard_account_update(
                            &acct,
                            parsing_result,
                            &self.storage,
                            &self.task_sender,
                        )
                        .await
                    }
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
        }
        Ok(())
    }
}
