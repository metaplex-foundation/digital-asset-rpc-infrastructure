use anyhow::Result;
use clap::Parser;
use das_core::{connect_db, MetricsArgs, PoolArgs, Rpc, SolanaRpcArgs};
use digital_asset_types::dao::cl_audits_v2;
use futures::future;
use sea_orm::{CursorTrait, EntityTrait, SqlxPostgresConnector};
use solana_sdk::signature::Signature;
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
}

pub async fn run(config: Args) -> Result<()> {
    let pool = connect_db(config.database).await?;

    let solana_rpc = Rpc::from_config(config.solana);

    let mut output = stdout();
    let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(pool);
    let mut after = None;

    loop {
        let mut query = cl_audits_v2::Entity::find().cursor_by(cl_audits_v2::Column::Id);
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

        for (signature, transaction) in transactions.into_iter().flatten() {
            if let Some(meta) = transaction.transaction.meta {
                if meta.err.is_some() {
                    output
                        .write_all(format!("{}\n", signature).as_bytes())
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

async fn fetch_transaction(
    entry: cl_audits_v2::Model,
    solana_rpc: Rpc,
) -> Result<(Signature, EncodedConfirmedTransactionWithStatusMeta)> {
    let signature = Signature::try_from(entry.tx.as_ref())?;

    let transaction = solana_rpc.get_transaction(&signature).await?;

    Ok((signature, transaction))
}
