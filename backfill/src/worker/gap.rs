use anyhow::Result;
use clap::Parser;
use das_core::Rpc;
use futures::{stream::FuturesUnordered, StreamExt};
use log::error;
use solana_sdk::signature::Signature;
use tokio::{
    sync::mpsc::{channel, Sender},
    task::JoinHandle,
};

use crate::gap::TreeGapFill;
use crate::BubblegumBackfillContext;

#[derive(Parser, Debug, Clone)]
pub struct GapWorkerArgs {
    /// The size of the signature channel.
    #[arg(long, env, default_value = "1000")]
    pub gap_channel_size: usize,

    /// The number of gap workers.
    #[arg(long, env, default_value = "25")]
    pub gap_worker_count: usize,
}

impl GapWorkerArgs {
    pub fn start(
        &self,
        context: BubblegumBackfillContext,
        forward: Sender<Signature>,
    ) -> Result<(JoinHandle<()>, Sender<TreeGapFill>)> {
        let (gap_sender, mut gap_receiver) = channel::<TreeGapFill>(self.gap_channel_size);
        let gap_worker_count = self.gap_worker_count;

        let handler = tokio::spawn(async move {
            let mut handlers = FuturesUnordered::new();
            let sender = forward.clone();

            while let Some(gap) = gap_receiver.recv().await {
                if handlers.len() >= gap_worker_count {
                    handlers.next().await;
                }

                let client = context.solana_rpc.clone();
                let sender = sender.clone();

                let handle = spawn_crawl_worker(client, sender, gap);

                handlers.push(handle);
            }

            futures::future::join_all(handlers).await;
        });

        Ok((handler, gap_sender))
    }
}

fn spawn_crawl_worker(client: Rpc, sender: Sender<Signature>, gap: TreeGapFill) -> JoinHandle<()> {
    tokio::spawn(async move {
        if let Err(e) = gap.crawl(client, sender).await {
            error!("tree transaction: {:?}", e);
        }
    })
}
