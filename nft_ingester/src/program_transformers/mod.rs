use blockbuster::{
    instruction::InstructionBundle,
    program_handler::ProgramParser,
    programs::{bubblegum::BubblegumParser, candy_machine::CandyMachineParser, ProgramParseResult},
};

use crate::{error::IngesterError, BgTask};
use plerkle_serialization::{
    account_info_generated::account_info::AccountInfo,
    transaction_info_generated::transaction_info::{self},
};
use sea_orm::{DatabaseConnection, SqlxPostgresConnector};
use solana_sdk::pubkey::Pubkey;
use sqlx::PgPool;
use std::collections::HashMap;
use tokio::sync::mpsc::UnboundedSender;
use transaction_info::Pubkey as FBPubkey;

use crate::program_transformers::bubblegum::handle_bubblegum_instruction;
use crate::program_transformers::candy_guard::handle_candy_guard_account_update;
use crate::program_transformers::candy_machine::handle_candy_machine_account_update;
use crate::program_transformers::candy_machine_core::handle_candy_machine_core_account_update;

mod bubblegum;
mod candy_guard;
mod candy_machine;
mod candy_machine_core;
mod common;

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
        matchers.insert(bgum.key(), Box::new(bgum));
        matchers.insert(candy_machine.key(), Box::new(candy_machine));
        let pool: PgPool = pool;
        ProgramTransformer {
            storage: SqlxPostgresConnector::from_sqlx_postgres_pool(pool),
            task_sender,
            matchers,
        }
    }

    pub fn match_program(&self, key: FBPubkey) -> Option<&Box<dyn ProgramParser>> {
        self.matchers.get(&Pubkey::new(key.0.as_slice()))
    }

    pub async fn handle_instruction<'a>(
        &self,
        ix: &'a InstructionBundle<'a>,
    ) -> Result<(), IngesterError> {
        if let Some(program) = self.match_program(ix.program) {
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
        if let Some(program) = self.match_program(acct.owner()) {
            let result = program.handle_account(acct)?;
            let concrete = result.result_type();
            match concrete {
                ProgramParseResult::CandyMachine(parsing_result) => {
                    handle_candy_machine_account_update(
                        parsing_result,
                        &acct,
                        &self.storage,
                        &self.task_sender,
                    )
                    .await
                }
                ProgramParseResult::CandyMachineCore(parsing_result) => {
                    handle_candy_machine_core_account_update(
                        parsing_result,
                        &acct,
                        &self.storage,
                        &self.task_sender,
                    )
                    .await
                }
                ProgramParseResult::CandyGuard(parsing_result) => {
                    handle_candy_guard_account_update(
                        parsing_result,
                        &acct,
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
