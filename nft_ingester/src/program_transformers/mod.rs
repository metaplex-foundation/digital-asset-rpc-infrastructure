use crate::{error::IngesterError, tasks::TaskData};
use async_trait::async_trait;
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
use solana_client::{nonblocking::rpc_client::RpcClient, rpc_config::RpcAccountInfoConfig};
use solana_sdk::{pubkey, pubkey::Pubkey};
use sqlx::PgPool;
use std::collections::{HashMap, HashSet, VecDeque};
use tokio::sync::mpsc::UnboundedSender;

use crate::program_transformers::{
    account_compression::handle_account_compression_instruction,
    bubblegum::handle_bubblegum_instruction, hpl_account_handler::IndexablePrograms,
    noop::handle_noop_instruction, token::handle_token_program_account,
    token_metadata::handle_token_metadata_account,
};

mod account_compression;
mod bubblegum;
mod hpl_account_handler;
mod noop;
mod token;
mod token_metadata;

pub struct HoneycombPrograms {
    pub rpc_client: RpcClient,
    pub programs: Vec<Pubkey>,
}
impl HoneycombPrograms {
    pub fn new<'a>(rpc_client: RpcClient) -> Self {
        let mut this = Self {
            rpc_client,
            programs: vec![],
        };
        this
    }
}

#[async_trait]
impl IndexablePrograms for HoneycombPrograms {
    fn keys(&self) -> &Vec<Pubkey> {
        &self.programs
    }

    fn rpc_client(&self) -> &RpcClient {
        &self.rpc_client
    }

    async fn populate_programs(&mut self) {
        self.programs
            .push(pubkey!("EtXbhgWbWEWamyoNbSRyN5qFXjFbw8utJDHvBkQKXLSL")); // Test HiveControl
        self.programs
            .push(pubkey!("HivezrprVqHR6APKKQkkLHmUG8waZorXexEBRZWh5LRm")); // HiveControl
        self.programs
            .push(pubkey!("ChRCtrG7X5kb9YncA4wuyD68DXXL8Szt3zBCCGiioBTg")); // CharacterManager
        self.programs
            .push(pubkey!("Assetw8uxLogzVXic5P8wGYpVdesS1oZHfSnBFHAu42s")); // Test Resource Manager
        self.programs
            .push(pubkey!("ATQfyuSouoFHW393YFYeojfBcsPD6KpM4cVCzSwkguT2")); // Resource Manager
        self.programs
            .push(pubkey!("CrncyaGmZfWvpxRcpHEkSrqeeyQsdn4MAedo9KuARAc4")); // Currency
        self.programs
            .push(pubkey!("Pay9ZxrVRXjt9Da8qpwqq4yBRvvrfx3STWnKK4FstPr")); // Payment
        self.programs
            .push(pubkey!("MiNESdRXUSmWY7NkAKdW9nMkjJZCaucguY3MDvkSmr6")); // Staking
        self.programs
            .push(pubkey!("9NGfVYcDmak9tayJMkxRNr8j5Ji6faThXGHNxSSRn1TK")); // Test GuildKit
    }
}

pub struct ProgramTransformer {
    storage: DatabaseConnection,
    task_sender: UnboundedSender<TaskData>,
    matchers: HashMap<Pubkey, Box<dyn ProgramParser>>,
    indexable_programs: Option<Box<dyn IndexablePrograms + Send + Sync>>,
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

        let pool: PgPool = pool;
        ProgramTransformer {
            storage: SqlxPostgresConnector::from_sqlx_postgres_pool(pool),
            task_sender,
            matchers,
            indexable_programs: None,
            key_set: hs,
            cl_audits,
        }
    }

    pub async fn new_with_rpc_client(
        pool: PgPool,
        rpc_client: RpcClient,
        task_sender: UnboundedSender<TaskData>,
        cl_audits: bool,
    ) -> Self {
        let mut this = Self::new(pool, task_sender, cl_audits);

        let mut honeycomb_programs = HoneycombPrograms::new(rpc_client);
        honeycomb_programs.populate_programs().await;
        honeycomb_programs.keys().iter().for_each(|key| {
            this.key_set.insert(key.clone());
        });

        this.indexable_programs = Some(Box::new(honeycomb_programs));
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
            if let Some(indexable_programs) = &self.indexable_programs {
                indexable_programs
                    .index_tx_accounts(&tx, &self.storage)
                    .await;
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
