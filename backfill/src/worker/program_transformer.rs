use anyhow::Result;
use clap::Parser;
use das_core::{create_download_metadata_notifier, DownloadMetadataInfo};
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use log::error;
use program_transformers::{ProgramTransformer, TransactionInfo};
use std::sync::Arc;
use tokio::sync::mpsc::{channel, Sender, UnboundedSender};
use tokio::task::JoinHandle;

use crate::BubblegumBackfillContext;

#[derive(Parser, Debug, Clone)]
pub struct ProgramTransformerWorkerArgs {
    #[arg(long, env, default_value = "100000")]
    pub program_transformer_channel_size: usize,
    #[arg(long, env, default_value = "50")]
    pub program_transformer_worker_count: usize,
}

impl ProgramTransformerWorkerArgs {
    pub fn start(
        &self,
        context: BubblegumBackfillContext,
        forwarder: UnboundedSender<DownloadMetadataInfo>,
    ) -> Result<(JoinHandle<()>, Sender<TransactionInfo>)> {
        let (sender, mut receiver) =
            channel::<TransactionInfo>(self.program_transformer_channel_size);

        let worker_forwarder = forwarder.clone();
        let worker_pool = context.database_pool.clone();
        let worker_count = self.program_transformer_worker_count;
        let handle = tokio::spawn(async move {
            let download_metadata_notifier =
                create_download_metadata_notifier(worker_forwarder.clone()).await;
            let program_transformer = Arc::new(ProgramTransformer::new(
                worker_pool.clone(),
                download_metadata_notifier,
            ));

            let mut handlers = FuturesUnordered::new();

            while let Some(transaction) = receiver.recv().await {
                if handlers.len() >= worker_count {
                    handlers.next().await;
                }

                let program_transformer_clone = Arc::clone(&program_transformer);
                let handle = tokio::spawn(async move {
                    if let Err(err) = program_transformer_clone
                        .handle_transaction(&transaction)
                        .await
                    {
                        error!(
                            "Failed to handle bubblegum instruction for txn {:?}: {:?}",
                            transaction.signature, err
                        );
                    }
                });

                handlers.push(handle);
            }

            futures::future::join_all(handlers).await;
        });

        Ok((handle, sender))
    }
}
