use cadence_macros::statsd_time;
use chrono::{DateTime, Utc};
use sea_orm::{DatabaseConnection, SqlxPostgresConnector};
use sqlx::{PgPool, Pool, Postgres};
use tokio::sync::mpsc::UnboundedSender;
use {
    crate::{error::IngesterError, utils::IxPair},
    crate::BgTask,
    async_trait::async_trait,
    flatbuffers::{ForwardsUOffset, Vector},
    plerkle_serialization::{
        account_info_generated::account_info,
        transaction_info_generated::transaction_info::{self, CompiledInstruction},
    },
    solana_sdk::pubkey::Pubkey,
    transaction_info::Pubkey as FBPubkey,
    std::collections::HashMap,
    plerkle_serialization::transaction_info_generated::transaction_info::TransactionInfo,
    plerkle_serialization::account_info_generated::account_info::AccountInfo,
};
use blockbuster::instruction::InstructionBundle;
use blockbuster::program_handler::{ProgramMatcher, ProgramParser};
use blockbuster::programs::bubblegum::{BubblegumParser};
use blockbuster::programs::ParsedProgram;
use crate::{order_instructions, parse_logs};
use crate::program_transformers::bubblegum::handle_bubblegum_instruction;

mod bubblegum;
mod common;

pub struct ProgramTransformer {
    storage: DatabaseConnection,
    task_sender: UnboundedSender<Box<dyn BgTask>>,
    matchers: HashMap<Pubkey, ParsedProgram>,
}

impl ProgramTransformer {
    pub fn new(pool: PgPool,
               task_sender: UnboundedSender<Box<dyn BgTask>>) -> Self {
        let mut matchers = HashMap::with_capacity(1);
        matchers.insert(BubblegumParser::key(), ParsedProgram::Bubblegum);
        let pool: PgPool = pool;
        ProgramTransformer {
            storage: SqlxPostgresConnector::from_sqlx_postgres_pool(pool),
            task_sender,
            matchers,
        }
    }

    pub fn match_program(&self, key: FBPubkey) -> Option<&ParsedProgram> {
        self.matchers.get(&Pubkey::new(key.0.as_slice()))
    }

    pub async fn handle_instruction<'a>(&self, ix: &'a InstructionBundle<'a>) -> Result<(), IngesterError> {
        if let Some(program) = self.match_program(ix.program) {
            match program {
                ParsedProgram::Bubblegum => {
                    let parsing_result = BubblegumParser::handle_instruction(&ix)?;
                    handle_bubblegum_instruction(&parsing_result, &ix, &self.storage, &self.task_sender).await
                }
                _ => Err(IngesterError::NotImplemented)
            }?;
        }
        Ok(())
    }

    pub async fn handle_account_update<'b>(&self, acct: AccountInfo<'b>) -> Result<(), IngesterError> {
        Ok(())
    }
}

