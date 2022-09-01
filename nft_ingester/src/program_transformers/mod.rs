use sea_orm::DatabaseConnection;
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
use blockbuster::programs::bubblegum::{Bubblegum, BubblegumParser};
use blockbuster::programs::ParsedProgram;
use crate::{order_instructions, parse_logs};
use crate::program_transformers::bubblegum::handle_bubblegum_instruction;

mod bubblegum;
mod common;

pub struct ProgramTransformer<'a> {
    storage: DatabaseConnection,
    task_sender: UnboundedSender<Box<dyn BgTask>>,
    matchers: HashMap<Pubkey, ParsedProgram>,
}

impl<'a> ProgramTransformer<'a> {
    pub fn new(storage: DatabaseConnection,
               task_sender: UnboundedSender<Box<dyn BgTask>>) -> Self {
        let mut matchers = HashMap::with_capacity(1);
        matchers.insert(BubblegumParser::key(), ParsedProgram::Bubblegum);

        ProgramTransformer {
            storage,
            task_sender,
            matchers,
        }
    }

    pub fn match_program(&self, key: FBPubkey) -> Option<&ParsedProgram> {
        self.matchers.get(&(key.0 as Pubkey))
    }

    pub fn handle_transaction(&self, tx: TransactionInfo) -> Result<(), IngesterError> {
        // Update metadata associated with the programs that store data in leaves
        let instructions = order_instructions(&tx);
        let parsed_logs = parse_logs(tx.log_messages()).unwrap();
        let keys = match tx.account_keys() {
            None => {
                println!("Flatbuffers account_keys missing");
                return Err(IngesterError::DeserializationError("Missing Accounts".to_string()));
            }
            Some(keys) => keys,
        };
        for ((outer_ix, inner_ix), parsed_log) in std::iter::zip(instructions, parsed_logs) {
            let (program, instruction) = outer_ix;
            let program_id = program.key().unwrap();
            if let Some(program) = self.match_program(program_id) {
                let bundle = &InstructionBundle {
                    txn_id: "".to_string(),
                    instruction,
                    inner_ix,
                    keys,
                    instruction_logs: parsed_log.1,
                    slot: tx.slot(),
                };
                match program {
                    ParsedProgram::Bubblegum => {
                        let parsing_result = BubblegumParser::handle_instruction(bundle)?;
                        handle_bubblegum_instruction(parsing_result, bundle, &self.storage, &self.task_sender)
                    }
                    _ => Err(IngesterError::NotImplemented)
                }?;
            }
        }
        Ok(())
    }

    pub fn handle_account_update<'b>(&self, acct: AccountInfo<'b>) -> Result<(), IngesterError> {
        Ok(())
    }
}

