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
    plerkle_serialization::{AccountInfo, Pubkey as FBPubkey, TransactionInfo},
    sea_orm::{DatabaseConnection, SqlxPostgresConnector},
    solana_sdk::pubkey::Pubkey,
    sqlx::PgPool,
    std::collections::{HashMap, HashSet, VecDeque},
    tracing::{debug, error, info},
};

mod bubblegum;
mod error;
mod token;
mod token_metadata;

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
    ) -> BoxFuture<'static, Result<(), Box<dyn std::error::Error + Send + Sync>>>,
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
            Ok(pubkey) => self.parsers.get(&pubkey),
            Err(_error) => {
                error!("failed to parse key: {key:?}");
                None
            }
        }
    }

    pub async fn handle_transaction<'a>(
        &self,
        tx: &'a TransactionInfo<'a>,
    ) -> ProgramTransformerResult<()> {
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
        let contains = instructions
            .iter()
            .filter(|(ib, _inner)| ib.0 .0.as_ref() == mpl_bubblegum::ID.as_ref());
        debug!("Instructions bgum: {}", contains.count());
        for (outer_ix, inner_ix) in instructions {
            let (program, instruction) = outer_ix;
            let ix_accounts = instruction.accounts().unwrap().iter().collect::<Vec<_>>();
            let ix_account_len = ix_accounts.len();
            let max = ix_accounts.iter().max().copied().unwrap_or(0) as usize;
            if keys.len() < max {
                return Err(ProgramTransformerError::DeserializationError(
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
                            &self.download_metadata_notifier,
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

    pub async fn handle_account_update<'b>(
        &self,
        acct: AccountInfo<'b>,
    ) -> ProgramTransformerResult<()> {
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
                        &self.download_metadata_notifier,
                    )
                    .await
                }
                ProgramParseResult::TokenProgramAccount(parsing_result) => {
                    handle_token_program_account(
                        &acct,
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
