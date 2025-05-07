use {
    crate::{
        bubblegum::handle_bubblegum_instruction,
        error::{ProgramTransformerError, ProgramTransformerResult},
        mpl_core_program::handle_mpl_core_account,
        token::handle_token_program_account,
        token_inscription::handle_token_inscription_program_update,
        token_metadata::handle_token_metadata_account,
    },
    blockbuster::{
        instruction::{order_instructions, InstructionBundle, IxPair},
        program_handler::ProgramParser,
        programs::{
            bubblegum::BubblegumParser,
            mpl_core_program::MplCoreParser,
            system::SystemProgramParser,
            token_account::{TokenProgramEntity, TokenProgramParser},
            token_extensions::{Token2022ProgramParser, TokenExtensionsProgramEntity},
            token_inscriptions::TokenInscriptionParser,
            token_metadata::TokenMetadataParser,
            ProgramParseResult,
        },
    },
    das_core::{DownloadMetadataInfo, DownloadMetadataNotifier},
    digital_asset_types::dao::{asset, slot_metas, token_accounts, tokens},
    sea_orm::{
        entity::EntityTrait, query::Select, sea_query::Expr, ColumnTrait, ConnectionTrait,
        DatabaseConnection, DbErr, QueryFilter, Set, SqlxPostgresConnector, TransactionTrait,
    },
    serde::Deserialize,
    serde_json::{Map, Value},
    solana_sdk::{instruction::CompiledInstruction, pubkey::Pubkey, signature::Signature},
    solana_transaction_status::InnerInstructions,
    sqlx::PgPool,
    std::collections::{HashMap, HashSet, VecDeque},
    system::handle_system_program_account,
    token_extensions::handle_token_extensions_program_account,
    tokio::time::{sleep, Duration},
    tracing::{debug, error},
};

mod asset_upserts;
pub mod bubblegum;
pub mod error;
mod mpl_core_program;
mod system;
mod token;
mod token_extensions;
mod token_inscription;
mod token_metadata;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AccountInfo {
    pub slot: u64,
    pub pubkey: Pubkey,
    pub owner: Pubkey,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TransactionInfo {
    pub slot: u64,
    pub signature: Signature,
    pub account_keys: Vec<Pubkey>,
    pub message_instructions: Vec<CompiledInstruction>,
    pub meta_inner_instructions: Vec<InnerInstructions>,
}
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SlotInfo {
    pub slot: i64,
}

pub struct ProgramTransformer {
    storage: PgPool,
    download_metadata_notifier: DownloadMetadataNotifier,
    parsers: HashMap<Pubkey, Box<dyn ProgramParser>>,
    key_set: HashSet<Pubkey>,
}

impl ProgramTransformer {
    pub fn new(pool: PgPool, download_metadata_notifier: DownloadMetadataNotifier) -> Self {
        let mut parsers: HashMap<Pubkey, Box<dyn ProgramParser>> = HashMap::with_capacity(6);
        let bgum = BubblegumParser {};
        let token_metadata = TokenMetadataParser {};
        let token = TokenProgramParser {};
        let mpl_core = MplCoreParser {};
        let token_extensions = Token2022ProgramParser {};
        let token_inscription = TokenInscriptionParser {};
        let system = SystemProgramParser {};
        parsers.insert(bgum.key(), Box::new(bgum));
        parsers.insert(token_metadata.key(), Box::new(token_metadata));
        parsers.insert(token.key(), Box::new(token));
        parsers.insert(mpl_core.key(), Box::new(mpl_core));
        parsers.insert(token_extensions.key(), Box::new(token_extensions));
        parsers.insert(token_inscription.key(), Box::new(token_inscription));
        parsers.insert(system.key(), Box::new(system));
        let hs = parsers.iter().fold(HashSet::new(), |mut acc, (k, _)| {
            acc.insert(*k);
            acc
        });
        ProgramTransformer {
            storage: pool,
            download_metadata_notifier,
            parsers,
            key_set: hs,
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
        let instructions = self.break_transaction(tx_info);
        let mut not_impl = 0;
        let ixlen = instructions.len();
        debug!("Instructions: {}", ixlen);
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

                let db = SqlxPostgresConnector::from_sqlx_postgres_pool(self.storage.clone());

                match concrete {
                    ProgramParseResult::Bubblegum(parsing_result) => {
                        handle_bubblegum_instruction(
                            parsing_result,
                            &ix,
                            &db,
                            &self.download_metadata_notifier,
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
                    ProgramParseResult::TokenProgramEntity(parsing_result) => {
                        if let TokenProgramEntity::CloseIx(acc_to_close) = parsing_result {
                            handle_token_program_close_ix(acc_to_close, &db).await;
                        }
                    }
                    ProgramParseResult::TokenExtensionsProgramEntity(parsing_result) => {
                        if let TokenExtensionsProgramEntity::CloseIx(acc_to_close) = parsing_result
                        {
                            handle_token_program_close_ix(acc_to_close, &db).await;
                        }
                    }
                    _ => {
                        not_impl += 1;
                    }
                };
            }
        }

        if not_impl == ixlen {
            debug!(
                "Not implemented for transaction signature: {:?}",
                tx_info.signature
            );
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
            let db = SqlxPostgresConnector::from_sqlx_postgres_pool(self.storage.clone());
            match result.result_type() {
                ProgramParseResult::TokenMetadata(parsing_result) => {
                    handle_token_metadata_account(
                        account_info,
                        parsing_result,
                        &db,
                        &self.download_metadata_notifier,
                    )
                    .await
                }
                ProgramParseResult::TokenProgramEntity(parsing_result) => {
                    handle_token_program_account(account_info, parsing_result, &db).await
                }
                ProgramParseResult::TokenExtensionsProgramEntity(parsing_result) => {
                    handle_token_extensions_program_account(
                        account_info,
                        parsing_result,
                        &db,
                        &self.download_metadata_notifier,
                    )
                    .await
                }
                ProgramParseResult::MplCore(parsing_result) => {
                    handle_mpl_core_account(
                        account_info,
                        parsing_result,
                        &db,
                        &self.download_metadata_notifier,
                    )
                    .await
                }
                ProgramParseResult::TokenInscriptionAccount(parsing_result) => {
                    handle_token_inscription_program_update(account_info, parsing_result, &db).await
                }
                // handle close Accounts through the system program
                ProgramParseResult::SystemProgramAccount(_) => {
                    handle_system_program_account(account_info, &db).await
                }
                _ => Err(ProgramTransformerError::NotImplemented),
            }?;
        }
        Ok(())
    }

    pub async fn handle_slot_update(&self, slot: i64) -> ProgramTransformerResult<()> {
        let db = SqlxPostgresConnector::from_sqlx_postgres_pool(self.storage.clone());

        let model = slot_metas::ActiveModel { slot: Set(slot) };
        slot_metas::Entity::insert(model).exec(&db).await?;

        slot_metas::Entity::delete_many()
            .filter(slot_metas::Column::Slot.lt(slot))
            .exec(&db)
            .await?;

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

pub fn filter_non_null_fields(value: Value) -> Option<Value> {
    match value {
        Value::Null => None,
        Value::Object(map) => {
            if map.values().all(|v| matches!(v, Value::Null)) {
                None
            } else {
                let filtered_map: Map<String, Value> = map
                    .into_iter()
                    .filter(|(_k, v)| !matches!(v, Value::Null))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();

                if filtered_map.is_empty() {
                    None
                } else {
                    Some(Value::Object(filtered_map))
                }
            }
        }
        _ => Some(value),
    }
}

pub async fn handle_token_program_close_ix(acc_to_close: &Pubkey, db: &DatabaseConnection) {
    let acc_to_close_bytes = acc_to_close.to_bytes().to_vec();
    let mint = tokens::Entity::find_by_id(acc_to_close_bytes.clone())
        .one(db)
        .await
        .ok()
        .flatten();

    if mint.is_some() {
        let token_res = tokens::Entity::delete_by_id(acc_to_close_bytes.clone())
            .exec(db)
            .await;
        let asset_res = asset::Entity::update_many()
            .filter(asset::Column::Id.is_in([acc_to_close_bytes.clone()]))
            .col_expr(asset::Column::Burnt, Expr::value(true))
            .exec(db)
            .await;

        token_res
            .map_err(|err| {
                error!("Failed to delete mint: {:?}", err);
            })
            .map(|r| {
                if r.rows_affected == 1 {
                    debug!("Deleted mint: {:?}", acc_to_close);
                } else {
                    error!("Failed to delete mint no rows affected: {:?}", acc_to_close);
                }
            })
            .ok();

        asset_res
            .map_err(|err| {
                error!("Failed to update asset: {:?}", err);
            })
            .map(|r| {
                if r.rows_affected == 1 {
                    debug!("Updated asset: {:?}", acc_to_close);
                } else {
                    error!(
                        "Failed to update asset no rows affected: {:?}",
                        acc_to_close
                    );
                }
            })
            .ok();
    } else {
        let res = token_accounts::Entity::delete_by_id(acc_to_close_bytes)
            .exec(db)
            .await;

        res.map(|res| {
            if res.rows_affected == 1 {
                debug!("Deleted token account: {:?}", acc_to_close);
            } else {
                error!(
                    "Failed to delete token account no rows affected: {:?}",
                    acc_to_close
                );
            }
        })
        .map_err(|err| {
            error!("Failed to delete token account: {:?}", err);
        })
        .ok();
    }
}
