use crate::postgres::create_pool;
use crate::util::create_shutdown;
use crate::{config::ConfigMonitor, prom::update_tree_proof_report};
use das_bubblegum::{verify_bubblegum, BubblegumContext, VerifyArgs};
use das_core::{Rpc, SolanaRpcArgs};
use futures::stream::StreamExt;
use tracing::{error, info};

pub async fn run(config: ConfigMonitor) -> anyhow::Result<()> {
    let mut shutdown = create_shutdown()?;
    let database_pool = create_pool(config.postgres).await?;
    let rpc = Rpc::from_config(&SolanaRpcArgs {
        solana_rpc_url: config.rpc,
    });

    let bubblegum_verify = tokio::spawn(async move {
        loop {
            let bubblegum_context = BubblegumContext::new(database_pool.clone(), rpc.clone());
            let verify_args = VerifyArgs {
                only_trees: config.bubblegum.only_trees.clone(),
                max_concurrency: config.bubblegum.max_concurrency,
            };

            match verify_bubblegum(bubblegum_context, verify_args).await {
                Ok(mut reports_receiver) => {
                    while let Some(report) = reports_receiver.recv().await {
                        info!(
                            report = ?report,
                        );
                        update_tree_proof_report(&report);
                    }

                    tokio::time::sleep(tokio::time::Duration::from_secs(600)).await;
                }
                Err(e) => {
                    error!(
                        message = "Error proof report recv",
                        error = ?e
                    );
                }
            }
        }
    });

    if let Some(_signal) = shutdown.next().await {}

    bubblegum_verify.abort();

    Ok(())
}
