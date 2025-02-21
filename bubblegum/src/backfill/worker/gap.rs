use crate::{
    backfill::gap::{OverfetchArgs, TreeGapFill},
    BubblegumContext,
};
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

#[derive(Parser, Debug, Clone)]
pub struct GapWorkerArgs {
    /// The size of the signature channel.
    #[arg(long, env, default_value = "1000")]
    pub gap_channel_size: usize,

    /// The number of gap workers.
    #[arg(long, env, default_value = "25")]
    pub gap_worker_count: usize,

    #[clap(flatten)]
    pub overfetch_args: OverfetchArgs,
}

impl GapWorkerArgs {
    pub fn start(
        &self,
        context: BubblegumContext,
        forward: Sender<Signature>,
    ) -> Result<(JoinHandle<()>, Sender<TreeGapFill>)> {
        let (gap_sender, mut gap_receiver) = channel::<TreeGapFill>(self.gap_channel_size);
        let gap_worker_count = self.gap_worker_count;
        let overfetch_args = self.overfetch_args.clone();

        let handler = tokio::spawn(async move {
            let mut handlers = FuturesUnordered::new();
            let sender = forward.clone();

            while let Some(gap) = gap_receiver.recv().await {
                if handlers.len() >= gap_worker_count {
                    handlers.next().await;
                }

                let client = context.solana_rpc.clone();
                let sender = sender.clone();

                let handle = spawn_crawl_worker(client, sender, gap, overfetch_args.clone());

                handlers.push(handle);
            }

            futures::future::join_all(handlers).await;
        });

        Ok((handler, gap_sender))
    }
}

fn spawn_crawl_worker(
    client: Rpc,
    sender: Sender<Signature>,
    gap: TreeGapFill,
    overfetch_args: OverfetchArgs,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        if let Err(e) = gap.crawl(client, sender, overfetch_args).await {
            error!("tree transaction: {:?}", e);
        }
    })
}
