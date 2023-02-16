use blockbuster::{
    instruction::InstructionBundle,
    program_handler::ProgramParser,
    programs::{
        bubblegum::BubblegumParser, token_account::TokenAccountParser,
        token_metadata::TokenMetadataParser, ProgramParseResult,
    },
};
use crate::{error::IngesterError, TaskData};
use blockbuster::instruction::IxPair;
use plerkle_serialization::{AccountInfo, Pubkey as FBPubkey, TransactionInfo};
use sea_orm::{DatabaseConnection, SqlxPostgresConnector,TransactionTrait};
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

mod bubblegum;
mod token;
mod token_metadata;

pub struct ProgramTransformer {
    storage: DatabaseConnection,
    task_sender: UnboundedSender<TaskData>,
    matchers: HashMap<Pubkey, Box<dyn ProgramParser>>,
    key_set: HashSet<Pubkey>,
}

impl ProgramTransformer {
    pub fn new(pool: PgPool, task_sender: UnboundedSender<TaskData>) -> Self {
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

    pub async fn handle_transaction<'a>(
        &self,
        tx: &'a TransactionInfo<'a>,
    ) -> Result<(), IngesterError> {
        println!("Handling Transaction: {:?}", tx.signature());
        let instructions = self.break_transaction(&tx);
        let accounts = tx.account_keys().unwrap_or_default();
        let slot = tx.slot();
        let mut keys: Vec<FBPubkey> = Vec::with_capacity(accounts.len());
        for k in accounts.into_iter() {
            keys.push(*k);
        }
        
        let txn = self.storage.begin().await?;
        for (outer_ix, inner_ix) in instructions {
            let (program, instruction) = outer_ix;
            let ix_accounts = instruction.accounts().unwrap().iter().collect::<Vec<_>>();
            let ix_account_len = ix_accounts.len();
            let max = ix_accounts.iter().max().copied().unwrap_or(0) as usize;
            if keys.len() < max {
                return Err(IngesterError::DeserializationError(
                    "Missing Accounts in Serialized Ixn/Txn".to_string(),
                ));
            }
            let ix_accounts =
                ix_accounts
                    .iter()
                    .fold(Vec::with_capacity(ix_account_len), |mut acc, a| {
                        if let Some(key) = keys.get(*a as usize) {
                            acc.push(*key);
                        }
                        acc
                    });
            let ix = InstructionBundle {
                txn_id: "",
                program,
                instruction: Some(instruction),
                inner_ix,
                keys: ix_accounts.as_slice(),
                slot,
            };
            if let Some(program) = self.match_program(&ix.program) {
                let result = program.handle_instruction(&ix)?;
                let concrete = result.result_type();
                match concrete {
                    ProgramParseResult::Bubblegum(parsing_result) => {
                        handle_bubblegum_instruction(
                            parsing_result,
                            &ix,
                            &txn,
                            &self.task_sender,
                        )
                        .await
                    }
    
                    _ => Err(IngesterError::NotImplemented),
                }?;
            }
        }
        txn.commit().await?;
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
