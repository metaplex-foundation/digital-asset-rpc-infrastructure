use {
    crate::{
        bubblegum::handle_bubblegum_instruction,
        error::{ProgramTransformerError, ProgramTransformerResult},
        mpl_core_program::handle_mpl_core_account,
        token::handle_token_program_account,
        token_metadata::handle_token_metadata_account,
    },
    blockbuster::{
        instruction::{order_instructions, InstructionBundle, IxPair},
        program_handler::ProgramParser,
        programs::{
            bubblegum::BubblegumParser, mpl_core_program::MplCoreParser,
            token_account::TokenAccountParser, token_metadata::TokenMetadataParser,
            ProgramParseResult,
        },
    },
    digital_asset_types::dao::batch_mint_to_verify,
    futures::future::BoxFuture,
    sea_orm::{
        entity::EntityTrait, query::Select, ConnectionTrait, DatabaseConnection, DbErr,
        QuerySelect, SqlxPostgresConnector, TransactionTrait,
    },
    solana_sdk::{instruction::CompiledInstruction, pubkey::Pubkey, signature::Signature},
    solana_transaction_status::InnerInstructions,
    sqlx::PgPool,
    std::collections::{HashMap, HashSet, VecDeque},
    tokio::{
        sync::RwLock,
        time::{sleep, Duration},
    },
    tracing::{debug, error, info},
};

mod asset_upserts;
pub mod batch_minting;
mod bubblegum;
pub mod error;
mod mpl_core_program;
mod token;
mod token_metadata;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountInfo {
    pub slot: u64,
    pub pubkey: Pubkey,
    pub owner: Pubkey,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionInfo {
    pub slot: u64,
    pub signature: Signature,
    pub account_keys: Vec<Pubkey>,
    pub message_instructions: Vec<CompiledInstruction>,
    pub meta_inner_instructions: Vec<InnerInstructions>,
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
    batched_trees: Option<RwLock<HashSet<Pubkey>>>,
}

impl ProgramTransformer {
    pub async fn new(
        pool: PgPool,
        download_metadata_notifier: DownloadMetadataNotifier,
        cl_audits: bool,
        skip_batch_minted_trees: bool,
    ) -> Self {
        let mut parsers: HashMap<Pubkey, Box<dyn ProgramParser>> = HashMap::with_capacity(3);
        let bgum = BubblegumParser {};
        let token_metadata = TokenMetadataParser {};
        let token = TokenAccountParser {};
        let mpl_core = MplCoreParser {};
        parsers.insert(bgum.key(), Box::new(bgum));
        parsers.insert(token_metadata.key(), Box::new(token_metadata));
        parsers.insert(token.key(), Box::new(token));
        parsers.insert(mpl_core.key(), Box::new(mpl_core));
        let hs = parsers.iter().fold(HashSet::new(), |mut acc, (k, _)| {
            acc.insert(*k);
            acc
        });
        let pool: PgPool = pool;

        let storage = SqlxPostgresConnector::from_sqlx_postgres_pool(pool);

        let batched_trees = {
            if !skip_batch_minted_trees {
                None
            } else {
                match batch_mint_to_verify::Entity::find()
                    .column(batch_mint_to_verify::Column::MerkleTree)
                    .all(&storage)
                    .await
                {
                    Ok(models) => {
                        let trees_result: Result<HashSet<_>, _> = models
                            .iter()
                            .map(|m| Pubkey::try_from(m.merkle_tree.clone()))
                            .collect();

                        match trees_result {
                            Ok(trees) => Some(RwLock::new(trees)),
                            Err(e) => {
                                error!("Failed to convert merkle_tree to Pubkey: {:?}", e);
                                None
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to fetch batch_mint_to_verify models: {:?}", e);
                        None
                    }
                }
            }
        };

        ProgramTransformer {
            storage,
            download_metadata_notifier,
            parsers,
            key_set: hs,
            cl_audits,
            batched_trees,
        }
    }

    pub fn break_transaction<'a>(
        &self,
        tx_info: &'a TransactionInfo,
    ) -> VecDeque<(IxPair<'a>, Option<Vec<IxPair<'a>>>)> {
        order_instructions(
            &self.key_set,
            tx_info.account_keys.as_slice(),
            tx_info.message_instructions.as_slice(),
            tx_info.meta_inner_instructions.as_slice(),
        )
    }

    #[allow(clippy::borrowed_box)]
    pub fn match_program(&self, key: &Pubkey) -> Option<&Box<dyn ProgramParser>> {
        self.parsers.get(key)
    }

    pub async fn handle_transaction(
        &self,
        tx_info: &TransactionInfo,
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
                        if let Some(batched_trees) = &self.batched_trees {
                            if let Some(change_log) = &parsing_result.tree_update {
                                let batched_trees = batched_trees.read().await;

                                if let Some(_tree) = batched_trees.get(&change_log.id) {
                                    continue;
                                }
                            }
                        }

                        handle_bubblegum_instruction(
                            parsing_result,
                            &ix,
                            &self.storage,
                            &self.download_metadata_notifier,
                            self.cl_audits,
                            &self.batched_trees,
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
        account_info: &AccountInfo,
    ) -> ProgramTransformerResult<()> {
        if let Some(program) = self.match_program(&account_info.owner) {
            let result = program.handle_account(&account_info.data)?;
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
                ProgramParseResult::MplCore(parsing_result) => {
                    handle_mpl_core_account(
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

pub async fn find_model_with_retry<T: ConnectionTrait + TransactionTrait, K: EntityTrait>(
    conn: &T,
    model_name: &str,
    select: &Select<K>,
    retry_intervals: &[u64],
) -> Result<Option<K::Model>, DbErr> {
    let mut retries = 0;
    let metric_name = format!("{}_found", model_name);

    for interval in retry_intervals {
        let interval_duration = Duration::from_millis(*interval);
        sleep(interval_duration).await;

        let model = select.clone().one(conn).await?;
        if let Some(m) = model {
            record_metric(&metric_name, true, retries);
            return Ok(Some(m));
        }
        retries += 1;
    }

    record_metric(&metric_name, false, retries - 1);
    Ok(None)
}

fn record_metric(metric_name: &str, success: bool, retries: u32) {
    let retry_count = &retries.to_string();
    let success = if success { "true" } else { "false" };
    if cadence_macros::is_global_default_set() {
        cadence_macros::statsd_count!(metric_name, 1, "success" => success, "retry_count" => retry_count);
    }
}
