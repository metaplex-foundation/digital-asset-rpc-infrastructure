use crate::{error::IngesterError, tasks::TaskData};
use blockbuster::{
    instruction::{order_instructions, InstructionBundle, IxPair},
    program_handler::ProgramParser,
    programs::{
        account_compression::AccountCompressionParser, bubblegum::BubblegumParser,
        noop::NoopParser, token_account::TokenAccountParser, token_metadata::TokenMetadataParser,
        ProgramParseResult,
    },
};
use log::{debug, error, info};
use plerkle_serialization::{AccountInfo, Pubkey as FBPubkey, TransactionInfo};
use sea_orm::{DatabaseConnection, SqlxPostgresConnector};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{pubkey, pubkey::Pubkey};
use sqlx::PgPool;
use std::collections::{HashMap, HashSet, VecDeque};
use tokio::sync::mpsc::UnboundedSender;

use crate::program_transformers::{
    account_compression::handle_account_compression_instruction,
    bubblegum::handle_bubblegum_instruction, hpl_account_handler::etl_account_schema_values,
    noop::handle_noop_instruction, token::handle_token_program_account,
    token_metadata::handle_token_metadata_account,
};

mod account_compression;
mod bubblegum;
mod hpl_account_handler;
mod noop;
mod token;
mod token_metadata;

pub struct ProgramTransformer {
    storage: DatabaseConnection,
    rpc_client: Option<RpcClient>,
    task_sender: UnboundedSender<TaskData>,
    matchers: HashMap<Pubkey, Box<dyn ProgramParser>>,
    key_set: HashSet<Pubkey>,
    cl_audits: bool,
}

impl ProgramTransformer {
    pub fn new(pool: PgPool, task_sender: UnboundedSender<TaskData>, cl_audits: bool) -> Self {
        let mut matchers: HashMap<Pubkey, Box<dyn ProgramParser>> = HashMap::with_capacity(1);
        let bgum: BubblegumParser = BubblegumParser {};
        let account_compression = AccountCompressionParser {};
        let token_metadata = TokenMetadataParser {};
        let token = TokenAccountParser {};
        let noop = NoopParser {};
        matchers.insert(bgum.key(), Box::new(bgum));
        matchers.insert(account_compression.key(), Box::new(account_compression));
        matchers.insert(token_metadata.key(), Box::new(token_metadata));
        matchers.insert(token.key(), Box::new(token));
        matchers.insert(noop.key(), Box::new(noop));
        let mut hs = matchers.iter().fold(HashSet::new(), |mut acc, (k, _)| {
            acc.insert(*k);
            acc
        });
        hs.insert(pubkey!("EtXbhgWbWEWamyoNbSRyN5qFXjFbw8utJDHvBkQKXLSL")); // Test HiveControl
        hs.insert(pubkey!("HivezrprVqHR6APKKQkkLHmUG8waZorXexEBRZWh5LRm")); // HiveControl
        hs.insert(pubkey!("ChRCtrG7X5kb9YncA4wuyD68DXXL8Szt3zBCCGiioBTg")); // CharacterManager
        hs.insert(pubkey!("CrncyaGmZfWvpxRcpHEkSrqeeyQsdn4MAedo9KuARAc4")); // Currency
        hs.insert(pubkey!("Pay9ZxrVRXjt9Da8qpwqq4yBRvvrfx3STWnKK4FstPr")); // Payment
        hs.insert(pubkey!("MiNESdRXUSmWY7NkAKdW9nMkjJZCaucguY3MDvkSmr6")); // Staking
        hs.insert(pubkey!("8fTwUdyGfDAcmdu8X4uWb2vBHzseKGXnxZUpZ2D94iit")); // Test GuildKit
        hs.insert(pubkey!("6ARwjKsMY2P3eLEWhdoU5czNezw3Qg6jEfbmLTVQqrPQ")); // Test ResourceManager
        let pool: PgPool = pool;
        ProgramTransformer {
            storage: SqlxPostgresConnector::from_sqlx_postgres_pool(pool),
            rpc_client: None,
            task_sender,
            matchers,
            key_set: hs,
            cl_audits,
        }
    }

    pub fn new_with_rpc_client(
        pool: PgPool,
        rpc_client: RpcClient,
        task_sender: UnboundedSender<TaskData>,
        cl_audits: bool,
    ) -> Self {
        let mut this = Self::new(pool, task_sender, cl_audits);
        this.rpc_client = Some(rpc_client);
        this
    }
    pub fn break_transaction<'i>(
        &self,
        tx: &'i TransactionInfo<'i>,
    ) -> VecDeque<(IxPair<'i>, Option<Vec<IxPair<'i>>>)> {
        let ref_set: HashSet<&[u8]> = self.key_set.iter().map(|k| k.as_ref()).collect();
        order_instructions(ref_set, tx)
    }

    #[allow(clippy::borrowed_box)]
    pub fn match_program(&self, key: &FBPubkey) -> Option<&Box<dyn ProgramParser>> {
        match Pubkey::try_from(key.0.as_slice()) {
            Ok(pubkey) => self.matchers.get(&pubkey),
            Err(_error) => {
                log::warn!("failed to parse key: {key:?}");
                None
            }
        }
    }

    pub async fn handle_transaction<'a>(
        &self,
        tx: &'a TransactionInfo<'a>,
    ) -> Result<(), IngesterError> {
        let sig: Option<&str> = tx.signature();
        info!("Handling Transaction: {:?}", sig);
        let instructions = self.break_transaction(tx);
        let accounts = tx.account_keys().unwrap_or_default();
        let slot = tx.slot();
        let txn_id = tx.signature().unwrap_or("");
        let mut keys: Vec<FBPubkey> = Vec::with_capacity(accounts.len());
        for k in accounts.into_iter() {
            keys.push(*k);
        }
        let payer = keys.get(0).map(|fk| Pubkey::from(fk.0));

        let mut not_impl = 0;
        let ixlen = instructions.len();
        debug!("Instructions: {}", ixlen);

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
                txn_id,
                program,
                instruction: Some(instruction),
                inner_ix,
                keys: ix_accounts.as_slice(),
                slot,
            };

            if let Some(program) = self.match_program(&ix.program) {
                debug!("Found a ix for program: {:?}", program.key());
                let result = program.handle_instruction(&ix)?;
                let concrete = result.result_type();
                match concrete {
                    ProgramParseResult::Bubblegum(parsing_result) => {
                        handle_bubblegum_instruction(
                            parsing_result,
                            &ix,
                            &self.storage,
                            &self.task_sender,
                            self.cl_audits,
                        )
                        .await
                        .map_err(|err| {
                            error!(
                                "Failed to handle bubblegum instruction for txn {:?}: {:?}",
                                sig, err
                            );
                            err
                        })?;
                    }
                    ProgramParseResult::AccountCompression(parsing_result) => {
                        handle_account_compression_instruction(
                            parsing_result,
                            &ix,
                            &self.storage,
                            &self.task_sender,
                            self.cl_audits,
                        )
                        .await
                        .map_err(|err| {
                            error!(
                                "Failed to handle account compression instruction for txn {:?}: {:?}",
                                sig, err
                            );
                            return err;
                        })?;
                    }
                    ProgramParseResult::Noop(parsing_result) => {
                        debug!("Handling NOOP Instruction");
                        match handle_noop_instruction(
                            parsing_result,
                            &ix,
                            &self.storage,
                            &self.task_sender,
                            self.cl_audits,
                        )
                        .await
                        {
                            Ok(_) => {}
                            Err(err) => {
                                error!(
                                    "Failed to handle noop instruction for txn {:?}: {:?}",
                                    sig, err
                                );
                            }
                        }
                    }
                    _ => {
                        not_impl += 1;
                        debug!("Could not handle this ix")
                    }
                };
            }
            if let Some(rpc_client) = &self.rpc_client {
                // let whitelist_programs = vec![pubkey!("EtXbhgWbWEWamyoNbSRyN5qFXjFbw8utJDHvBkQKXLSL")];
                etl_account_schema_values(
                    &ix,
                    keys.as_slice(),
                    &payer,
                    &self.storage,
                    rpc_client,
                    &self.task_sender,
                )
                .await
                .map_err(|err| {
                    error!(
                        "Failed to handle bubblegum instruction for txn {:?}: {:?}",
                        sig, err
                    );
                    err
                })?;
            }
        }

        if not_impl == ixlen {
            debug!("Not imple");
            return Err(IngesterError::NotImplemented);
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
                    )
                    .await
                }
                _ => Err(IngesterError::NotImplemented),
            }?;
        }
        Ok(())
    }
}
