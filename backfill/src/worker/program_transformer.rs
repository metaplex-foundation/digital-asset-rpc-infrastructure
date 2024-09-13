use anyhow::Result;
use clap::Parser;
use das_core::{create_download_metadata_notifier, DownloadMetadataInfo};
use log::error;
use program_transformers::{ProgramTransformer, TransactionInfo};
use tokio::sync::mpsc::{channel, Sender, UnboundedSender};
use tokio::task::JoinHandle;

use crate::BubblegumBackfillContext;

#[derive(Parser, Debug, Clone)]
pub struct ProgramTransformerWorkerArgs {
    #[arg(long, env, default_value = "100000")]
    pub program_transformer_channel_size: usize,
}

impl ProgramTransformerWorkerArgs {
    pub fn start(
        &self,
        context: BubblegumBackfillContext,
        forwarder: UnboundedSender<DownloadMetadataInfo>,
    ) -> Result<(JoinHandle<()>, Sender<(Option<i64>, TransactionInfo)>)> {
        let (sender, mut receiver) =
            channel::<(Option<i64>, TransactionInfo)>(self.program_transformer_channel_size);

        let handle = tokio::spawn(async move {
            let mut transactions = Vec::new();
            let pool = context.database_pool.clone();

            let download_metadata_notifier = create_download_metadata_notifier(forwarder).await;

            let program_transformer = ProgramTransformer::new(pool, download_metadata_notifier);

            while let Some(gap) = receiver.recv().await {
                transactions.push(gap);
            }

            transactions.sort_by(|(a, _), (b, _)| a.cmp(&b));

            for (_, transaction) in transactions {
                if let Err(e) = program_transformer.handle_transaction(&transaction).await {
                    error!("handle transaction: {:?}", e)
                };
            }
        });

        Ok((handle, sender))
    }
}
