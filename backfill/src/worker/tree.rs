use crate::{
    gap::{TreeGapFill, TreeGapModel},
    tree::TreeResponse,
    BubblegumBackfillContext,
};
use anyhow::Result;
use clap::Parser;
use das_core::MetadataJsonDownloadWorkerArgs;
use digital_asset_types::dao::cl_audits_v2;
use log::error;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, SqlxPostgresConnector};
use solana_sdk::signature::Signature;
use tokio::task::JoinHandle;

use super::{GapWorkerArgs, ProgramTransformerWorkerArgs, SignatureWorkerArgs};

#[derive(Debug, Clone, Parser)]
pub struct TreeWorkerArgs {
    #[clap(flatten)]
    pub metadata_json_download_worker: MetadataJsonDownloadWorkerArgs,

    #[clap(flatten)]
    pub signature_worker: SignatureWorkerArgs,

    #[clap(flatten)]
    pub gap_worker: GapWorkerArgs,

    #[clap(flatten)]
    pub program_transformer_worker: ProgramTransformerWorkerArgs,

    #[clap(long, env, default_value = "false")]
    pub force: bool,
}
impl TreeWorkerArgs {
    pub fn start(
        &self,
        context: BubblegumBackfillContext,
        tree: TreeResponse,
    ) -> JoinHandle<Result<()>> {
        let db_pool = context.database_pool.clone();
        let metadata_json_download_db_pool = context.database_pool.clone();

        let program_transformer_context = context.clone();
        let signature_context = context.clone();

        let metadata_json_download_worker_args = self.metadata_json_download_worker.clone();
        let program_transformer_worker_args = self.program_transformer_worker.clone();
        let signature_worker_args = self.signature_worker.clone();
        let gap_worker_args = self.gap_worker.clone();
        let force = self.force;

        tokio::spawn(async move {
            let (metadata_json_download_worker, metadata_json_download_sender) =
                metadata_json_download_worker_args.start(metadata_json_download_db_pool)?;

            let (program_transformer_worker, transaction_info_sender) =
                program_transformer_worker_args
                    .start(program_transformer_context, metadata_json_download_sender)?;

            let (signature_worker, signature_sender) =
                signature_worker_args.start(signature_context, transaction_info_sender)?;

            let (gap_worker, tree_gap_sender) = gap_worker_args.start(context, signature_sender)?;

            {
                let conn = SqlxPostgresConnector::from_sqlx_postgres_pool(db_pool);

                let mut gaps = TreeGapModel::find(&conn, tree.pubkey)
                    .await?
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<Result<Vec<_>, _>>()?;

                let upper_known_seq = if force {
                    None
                } else {
                    cl_audits_v2::Entity::find()
                        .filter(cl_audits_v2::Column::Tree.eq(tree.pubkey.as_ref().to_vec()))
                        .order_by_desc(cl_audits_v2::Column::Seq)
                        .one(&conn)
                        .await?
                };

                let lower_known_seq = if force {
                    None
                } else {
                    cl_audits_v2::Entity::find()
                        .filter(cl_audits_v2::Column::Tree.eq(tree.pubkey.as_ref().to_vec()))
                        .order_by_asc(cl_audits_v2::Column::Seq)
                        .one(&conn)
                        .await?
                };

                if let Some(upper_seq) = upper_known_seq {
                    let signature = Signature::try_from(upper_seq.tx.as_ref())?;
                    gaps.push(TreeGapFill::new(tree.pubkey, None, Some(signature)));
                // Reprocess the entire tree if force is true or if the tree has a seq of 0 to keep the current behavior
                } else if force || tree.seq > 0 {
                    gaps.push(TreeGapFill::new(tree.pubkey, None, None));
                }

                if let Some(lower_seq) = lower_known_seq.filter(|seq| seq.seq > 1) {
                    let signature = Signature::try_from(lower_seq.tx.as_ref())?;

                    gaps.push(TreeGapFill::new(tree.pubkey, Some(signature), None));
                }

                for gap in gaps {
                    if let Err(e) = tree_gap_sender.send(gap).await {
                        error!("send gap: {:?}", e);
                    }
                }
            }

            drop(tree_gap_sender);

            futures::future::try_join4(
                gap_worker,
                signature_worker,
                program_transformer_worker,
                metadata_json_download_worker,
            )
            .await?;

            Ok(())
        })
    }
}
