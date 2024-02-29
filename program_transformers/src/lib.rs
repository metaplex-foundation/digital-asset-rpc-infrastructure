use {
    crate::{
        bubblegum::handle_bubblegum_instruction,
        error::{ProgramTransformerError, ProgramTransformerResult},
        token::handle_token_program_account,
        token_metadata::handle_token_metadata_account,
    },
    blockbuster::{
        instruction::{order_instructions, InstructionBundle, IxPair},
        program_handler::ProgramParser,
        programs::{
            bubblegum::BubblegumParser, token_account::TokenAccountParser,
            token_metadata::TokenMetadataParser, ProgramParseResult,
        },
    },
    futures::future::BoxFuture,
    sea_orm::{DatabaseConnection, SqlxPostgresConnector},
    solana_sdk::{instruction::CompiledInstruction, pubkey::Pubkey, signature::Signature},
    solana_transaction_status::InnerInstructions,
    sqlx::PgPool,
    std::collections::{HashMap, HashSet, VecDeque},
    tracing::{debug, error, info},
};

mod asset_upserts;
mod bubblegum;
pub mod error;
mod token;
mod token_metadata;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccountInfo<'a> {
    pub slot: u64,
    pub pubkey: &'a Pubkey,
    pub owner: &'a Pubkey,
    pub data: &'a [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransactionInfo<'a> {
    pub slot: u64,
    pub signature: &'a Signature,
    pub account_keys: &'a [Pubkey],
    pub message_instructions: &'a [CompiledInstruction],
    pub meta_inner_instructions: &'a [InnerInstructions],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadMetadataInfo {
    asset_data_id: Vec<u8>,
    uri: String,
}

impl DownloadMetadataInfo {
    pub fn new(asset_data_id: Vec<u8>, uri: String) -> Self {
        Self {
            asset_data_id,
            uri: uri.trim().replace('\0', ""),
        }
    }

    pub fn into_inner(self) -> (Vec<u8>, String) {
        (self.asset_data_id, self.uri)
    }
}

pub type DownloadMetadataNotifier = Box<
    dyn Fn(
            DownloadMetadataInfo,
        ) -> BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>>
        + Sync
        + Send,
>;

pub struct ProgramTransformer {
    storage: DatabaseConnection,
    download_metadata_notifier: DownloadMetadataNotifier,
    parsers: HashMap<Pubkey, Box<dyn ProgramParser>>,
    key_set: HashSet<Pubkey>,
    cl_audits: bool,
}

impl ProgramTransformer {
    pub fn new(
        pool: PgPool,
        download_metadata_notifier: DownloadMetadataNotifier,
        cl_audits: bool,
    ) -> Self {
        let mut parsers: HashMap<Pubkey, Box<dyn ProgramParser>> = HashMap::with_capacity(3);
        let bgum = BubblegumParser {};
        let token_metadata = TokenMetadataParser {};
        let token = TokenAccountParser {};
        parsers.insert(bgum.key(), Box::new(bgum));
        parsers.insert(token_metadata.key(), Box::new(token_metadata));
        parsers.insert(token.key(), Box::new(token));
        let hs = parsers.iter().fold(HashSet::new(), |mut acc, (k, _)| {
            acc.insert(*k);
            acc
        });
        let pool: PgPool = pool;
        ProgramTransformer {
            storage: SqlxPostgresConnector::from_sqlx_postgres_pool(pool),
            download_metadata_notifier,
            parsers,
            key_set: hs,
            cl_audits,
        }
    }

    pub fn break_transaction<'a>(
        &self,
        tx_info: &'a TransactionInfo<'_>,
    ) -> VecDeque<(IxPair<'a>, Option<Vec<IxPair<'a>>>)> {
        order_instructions(
            &self.key_set,
            tx_info.account_keys,
            tx_info.message_instructions,
            tx_info.meta_inner_instructions,
        )
    }

    #[allow(clippy::borrowed_box)]
    pub fn match_program(&self, key: &Pubkey) -> Option<&Box<dyn ProgramParser>> {
        self.parsers.get(key)
    }

    pub async fn handle_transaction(
        &self,
        tx_info: &TransactionInfo<'_>,
    ) -> ProgramTransformerResult<()> {
        info!("Handling Transaction: {:?}", tx_info.signature);
        let instructions = self.break_transaction(tx_info);
        let mut not_impl = 0;
        let ixlen = instructions.len();
        debug!("Instructions: {}", ixlen);
        let contains = instructions
            .iter()
            .filter(|(ib, _inner)| ib.0 == mpl_bubblegum::ID);
        debug!("Instructions bgum: {}", contains.count());
        for (outer_ix, inner_ix) in instructions {
            let (program, instruction) = outer_ix;
            let ix_accounts = &instruction.accounts;
            let ix_account_len = ix_accounts.len();
            let max = ix_accounts.iter().max().copied().unwrap_or(0) as usize;
            if tx_info.account_keys.len() < max {
                return Err(ProgramTransformerError::DeserializationError(
                    "Missing Accounts in Serialized Ixn/Txn".to_string(),
                ));
            }
            let ix_accounts =
                ix_accounts
                    .iter()
                    .fold(Vec::with_capacity(ix_account_len), |mut acc, a| {
                        if let Some(key) = tx_info.account_keys.get(*a as usize) {
                            acc.push(*key);
                        }
                        acc
                    });
            let ix = InstructionBundle {
                txn_id: &tx_info.signature.to_string(),
                program,
                instruction: Some(instruction),
                inner_ix: inner_ix.as_deref(),
                keys: ix_accounts.as_slice(),
                slot: tx_info.slot,
            };

            let program_key = ix.program;
            if let Some(program) = self.match_program(&program_key) {
                debug!("Found a ix for program: {:?}", program.key());
                let result = program.handle_instruction(&ix)?;
                let concrete = result.result_type();
                match concrete {
                    ProgramParseResult::Bubblegum(parsing_result) => {
                        handle_bubblegum_instruction(
                            parsing_result,
                            &ix,
                            &self.storage,
                            &self.download_metadata_notifier,
                            self.cl_audits,
                        )
                        .await
                        .map_err(|err| {
                            error!(
                                "Failed to handle bubblegum instruction for txn {:?}: {:?}",
                                tx_info.signature, err
                            );
                            err
                        })?;
                    }
                    _ => {
                        not_impl += 1;
                    }
                };
            }
        }

        if not_impl == ixlen {
            debug!("Not imple");
            return Err(ProgramTransformerError::NotImplemented);
        }
        Ok(())
    }

    pub async fn handle_account_update(
        &self,
        account_info: &AccountInfo<'_>,
    ) -> ProgramTransformerResult<()> {
        if let Some(program) = self.match_program(account_info.owner) {
            let result = program.handle_account(account_info.data)?;
            match result.result_type() {
                ProgramParseResult::TokenMetadata(parsing_result) => {
                    handle_token_metadata_account(
                        account_info,
                        parsing_result,
                        &self.storage,
                        &self.download_metadata_notifier,
                    )
                    .await
                }
                ProgramParseResult::TokenProgramAccount(parsing_result) => {
                    handle_token_program_account(
                        account_info,
                        parsing_result,
                        &self.storage,
                        &self.download_metadata_notifier,
                    )
                    .await
                }
                _ => Err(ProgramTransformerError::NotImplemented),
            }?;
        }
        Ok(())
    }
}
