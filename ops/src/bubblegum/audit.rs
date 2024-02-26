use super::rpc::{Rpc, SolanaRpcArgs};
use anyhow::Result;

use borsh::BorshSerialize;
use clap::Parser;
use das_core::{connect_db, MetricsArgs, PoolArgs};
use futures::future;
use log::debug;
use std::{path::PathBuf, str::FromStr};

use digital_asset_types::dao::{cl_audits_v2, sea_orm_active_enums::Instruction};
use sea_orm::{ColumnTrait, CursorTrait, EntityTrait, QueryFilter, SqlxPostgresConnector};
use solana_sdk::{pubkey::Pubkey, signature::Signature};
use solana_transaction_status::EncodedConfirmedTransactionWithStatusMeta;

use tokio::io::{stdout, AsyncWriteExt};

#[derive(Debug, Parser, Clone)]
pub struct Args {
    /// Database configuration
    #[clap(flatten)]
    pub database: PoolArgs,

    /// Metrics configuration
    #[clap(flatten)]
    pub metrics: MetricsArgs,

    /// Solana configuration
    #[clap(flatten)]
    pub solana: SolanaRpcArgs,

    #[arg(long, env, default_value = "10000")]
    pub batch_size: u64,

    #[arg(long, env)]
    pub only_trees: Option<Vec<String>>,

    #[arg(long, env, default_value = "false")]
    pub fix: bool,

    #[arg(long, env)]
    pub log_path: Option<PathBuf>,
}

pub async fn run(config: Args) -> Result<()> {
    let pool = connect_db(config.database).await?;

    let solana_rpc = Rpc::from_config(config.solana);

    let mut output = stdout();
    let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(pool);
    let mut after: Option<i64> = None;

    if let Some(log_path) = config.log_path {
        after = match std::fs::read_to_string(log_path) {
            Ok(content) => content
                .lines()
                .last()
                .map(|last_entry| last_entry.parse().ok())
                .flatten(),
            Err(_) => None,
        };
    }

    loop {
        let mut query = cl_audits_v2::Entity::find();

        if let Some(only_trees) = &config.only_trees {
            let pubkeys = only_trees
                .into_iter()
                .map(|address| {
                    Pubkey::from_str(&address)
                        .map_err(|e| anyhow::anyhow!(e.to_string()))?
                        .try_to_vec()
                        .map_err(|e| anyhow::anyhow!(e.to_string()))
                })
                .collect::<Result<Vec<Vec<u8>>, anyhow::Error>>()?;

            let pubkeys = pubkeys
                .into_iter()
                .map(|pubkey| pubkey.try_to_vec())
                .collect::<Result<Vec<_>, std::io::Error>>()?;

            query = query.filter(cl_audits_v2::Column::Tree.is_in(pubkeys));
        }
        let mut query = query.cursor_by(cl_audits_v2::Column::Id);

        let mut query = query.first(config.batch_size);

        if let Some(after) = after {
            query = query.after(after);
        }

        let entries = query.all(&conn).await?;

        let mut transactions = vec![];

        for entry in entries.clone() {
            transactions.push(fetch_transaction(entry, solana_rpc.clone()));
        }

        let transactions = future::join_all(transactions).await;

        for response in transactions.into_iter().flatten() {
            if let Some(meta) = response.transaction.transaction.meta {
                if meta.err.is_some() {
                    if config.fix {
                        match response.entry.instruction {
                            Instruction::Transfer => {
                                let model: cl_audits_v2::ActiveModel =
                                    response.entry.clone().into();

                                cl_audits_v2::Entity::delete(model).exec(&conn).await?;
                            }
                            _ => {
                                debug!("Unhandled instruction: {:?}", response.entry.instruction);
                            }
                        }
                    }
                    output
                        .write_all(format!("{}\n", response.entry.id).as_bytes())
                        .await?;

                    output.flush().await?;
                }
            }
        }

        after = entries.last().map(|cl_audit_v2| cl_audit_v2.id);

        if entries.is_empty() {
            break;
        }
    }

    Ok(())
}

struct FetchTransactionResponse {
    pub entry: cl_audits_v2::Model,
    pub transaction: EncodedConfirmedTransactionWithStatusMeta,
}

impl FetchTransactionResponse {
    fn new(
        entry: cl_audits_v2::Model,
        transaction: EncodedConfirmedTransactionWithStatusMeta,
    ) -> Self {
        Self { entry, transaction }
    }
}

async fn fetch_transaction(
    entry: cl_audits_v2::Model,
    solana_rpc: Rpc,
) -> Result<FetchTransactionResponse> {
    let signature = Signature::try_from(entry.tx.as_ref())?;

    let transaction = solana_rpc.get_transaction(&signature).await?;

    Ok(FetchTransactionResponse::new(entry, transaction))
}
