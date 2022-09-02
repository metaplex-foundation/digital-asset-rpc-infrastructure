use cadence_macros::statsd_time;
use chrono::{DateTime, Utc};
use sea_orm::{DatabaseConnection, SqlxPostgresConnector};
use sqlx::{Pool, Postgres};
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
    pub fn new(pool: Pool<Postgres>,
               task_sender: UnboundedSender<Box<dyn BgTask>>) -> Self {
        let mut matchers = HashMap::with_capacity(1);
        matchers.insert(BubblegumParser::key(), ParsedProgram::Bubblegum);


        ProgramTransformer {
            storage: SqlxPostgresConnector::from_sqlx_postgres_pool(pool),
            task_sender,
            matchers,
        }
    }

    pub fn match_program(&self, key: FBPubkey) -> Option<&ParsedProgram> {
        self.matchers.get(&(key.0 as Pubkey))
    }

    pub async fn handle_transaction(&self, tx: TransactionInfo) -> Result<(), IngesterError> {
        // Update metadata associated with the programs that store data in leaves
        let instructions = order_instructions(&tx);
        let keys = tx.account_keys().ok_or(IngesterError::DeserializationError("Missing Accounts".to_string()))?;
        for (outer_ix, inner_ix) in instructions {
            let (program, instruction) = outer_ix;
            let program_id = program.key().unwrap();
            if let Some(program) = self.match_program(program_id) {
                let bundle = &InstructionBundle {
                    txn_id: "".to_string(),
                    instruction,
                    inner_ix,
                    keys,
                    slot: tx.slot(),
                };
                match program {
                    ParsedProgram::Bubblegum => {
                        let parsing_result = BubblegumParser::handle_instruction(bundle)?;
                        handle_bubblegum_instruction(parsing_result, bundle, &self.storage, &self.task_sender).await
                    }
                    _ => Err(IngesterError::NotImplemented)
                }?;
            }
            let finished_at = Utc::now();
            let str_program_id = bs58::encode(program_id).into_string();
            statsd_time!("ingester.ix_process_time", (finished_at.timestamp_millis() - tx.seen_at()) as u64, "program_id" => &str_program_id);
        }
        Ok(())
    }

    pub async fn handle_account_update<'b>(&self, acct: AccountInfo<'b>) -> Result<(), IngesterError> {
        Ok(())
    }
}

